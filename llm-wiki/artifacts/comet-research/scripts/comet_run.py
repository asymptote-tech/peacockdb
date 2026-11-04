#!/usr/bin/env python3
"""Run TPC-H or TPC-DS sf40 on Spark with Comet, on the CPU VM (comet-research-impl.md, Task 2).

    comet_run.py --dataset tpch|tpcds --source s3|local --mode sequential|concurrent --batch K|all
                 --runs R --warmup W --out DIR [--dump-plans DIR] [--warmup-query Q] [--cache]
    comet_run.py --matrix [--calibration calibration.tsv]

One Spark application with one executor on all 128 cores (standalone master, started by
`comet_setup.sh cluster`). The conf is Comet's TPC benchmark conf plus the deviations in
`spark_conf` (comet-internals.md `## Recipes` 3 and 4). Concurrent mode runs one thread per query
in one FAIR pool. Event logs go, zstd-compressed, to ~/comet/eventlogs on the VM.

Writes into DIR: `runs.tsv` (query, run, ms; a `batch` row per run; warm-up runs are `w1`, …),
`conf.txt` (redacted settings, versions, host, local start time), `status`, `rc` (0, 1, or 143
when cut by a signal) and `error.txt` on failure. `--dump-plans` writes per query its final plan
text, its native plan blocks (`.pb`, `.txt`) and rows of `fallbacks.tsv`.

Layout on the VM: this folder with the queries copied in as `queries/tpch/*.sql` and
`queries/tpcds/*.sql` (from the repo's testdata/{tpch,tpcds}-queries); in a checkout those are
used directly. Local data is `~/comet/data/<dataset>-sf40/<table>.parquet`; S3 is
`s3a://<dataset>-sf40/<table>.parquet` in eu-west1 (`--s3-region eu-north1` for the labelled
cross-region reference).
"""
import argparse
import dataclasses
import datetime
import hashlib
import os
import platform
import re
import signal
import socket
import sys
import threading
import time
import traceback
from dataclasses import dataclass
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[3]
COMET_JAR = os.environ.get("COMET_JAR", "/opt/comet/comet-spark-spark4.1_2.13-1.2.0.jar")
COMET_VERSION = "1.2.0"
STARTUP_MIN = 1.0      # Spark start, table registration (estimate)
S3_FACTOR = 2.0        # S3 time over local time, until an S3 run calibrates it (estimate)
REDACT = re.compile(r"(?i)secret|password|token|access[._]?key")

TABLES = {
    "tpch": ["customer", "lineitem", "nation", "orders", "part", "partsupp", "region", "supplier"],
    "tpcds": ["call_center", "catalog_page", "catalog_returns", "catalog_sales", "customer",
              "customer_address", "customer_demographics", "date_dim", "household_demographics",
              "income_band", "inventory", "item", "promotion", "reason", "ship_mode", "store",
              "store_returns", "store_sales", "time_dim", "warehouse", "web_page", "web_returns",
              "web_sales", "web_site"],
}
S3 = {  # region: (AWS profile, endpoint)
    "eu-west1": ("eu-west1", "https://storage.eu-west1.nebius.cloud:443"),
    "eu-north1": ("default", "https://storage.eu-north1.nebius.cloud:443"),
}


# ---- queries and batches -------------------------------------------------------------------

def default_queries_dir(dataset: str) -> Path:
    local = HERE / "queries" / dataset
    return local if local.is_dir() else ROOT / "testdata" / f"{dataset}-queries"


def queries(dataset: str, batch: str) -> list[str]:
    """The first `batch` queries of the dataset's list, or all of it."""
    if dataset == "tpch":
        names = [f"q{i}" for i in range(1, 23)]
    elif dataset == "tpcds":
        names = (HERE / "tpcds-82.txt").read_text().split()
    else:
        raise ValueError(f"unknown dataset {dataset}")
    if batch == "all":
        return names
    k = int(batch)
    if not 0 < k <= len(names):
        raise ValueError(f"batch {k} out of 1..{len(names)}")
    return names[:k]


def statements(text: str) -> list[str]:
    """Split SQL on top-level `;` and turn "double-quoted" names into `backticked` ones: Spark
    reads a double-quoted token as a string. Comment-only pieces are dropped."""
    out, cur, i, n = [], [], 0, len(text)
    while i < n:
        c = text[i]
        if c == "-" and text.startswith("--", i):
            j = text.find("\n", i)
            j = n if j < 0 else j
            cur.append(text[i:j]); i = j
        elif c == "'":
            j = i + 1
            while j < n:
                if text[j] == "'" and text.startswith("''", j):
                    j += 2
                elif text[j] == "'":
                    break
                else:
                    j += 1
            cur.append(text[i:j + 1]); i = j + 1
        elif c == '"':
            j = text.index('"', i + 1)
            cur.append("`" + text[i + 1:j] + "`"); i = j + 1
        elif c == ";":
            out.append("".join(cur)); cur = []; i += 1
        else:
            cur.append(c); i += 1
    out.append("".join(cur))
    code = lambda s: "\n".join(l for l in s.splitlines() if not l.strip().startswith("--")).strip()
    return [s.strip() for s in out if code(s)]


# ---- the matrix ----------------------------------------------------------------------------

@dataclass
class Config:
    priority: int
    dataset: str
    source: str
    mode: str
    batch: str
    runs: int
    warmup_query: str | None = None
    cache: bool = False
    estimate_min: float | None = None

    @property
    def name(self) -> str:
        return f"{self.dataset}-{self.source}-{self.mode}-b{self.batch}" + ("-cache" if self.cache else "")

    def command(self) -> str:
        c = (f"comet_run.py --dataset {self.dataset} --source {self.source} --mode {self.mode} "
             f"--batch {self.batch} --runs {self.runs} --warmup 0 --out measurements/comet/{self.name}")
        if self.warmup_query:
            c += f" --warmup-query {self.warmup_query}"
        return c + (" --cache" if self.cache else "")


# The spec's priority order (Task 4 Step 3), then the coordinator's in-memory-cache pair.
MATRIX = [
    Config(1, "tpch", "local", "sequential", "all", 2),
    Config(1, "tpch", "local", "concurrent", "all", 2),
    Config(2, "tpcds", "local", "sequential", "all", 1),
    Config(2, "tpcds", "local", "concurrent", "all", 1),
    Config(3, "tpch", "s3", "sequential", "all", 1, warmup_query="q1"),
    Config(3, "tpch", "s3", "concurrent", "all", 1),
    Config(4, "tpcds", "s3", "sequential", "all", 1),
    Config(5, "tpch", "local", "sequential", "all", 1, cache=True),
    Config(5, "tpch", "local", "concurrent", "all", 1, cache=True),
]


def read_calibration(path) -> dict[str, dict[str, float]]:
    """calibration.tsv (dataset, query, ms) -> {dataset: {query: ms}}; empty when absent."""
    cal: dict[str, dict[str, float]] = {}
    if path is None or not Path(path).is_file():
        return cal
    for line in Path(path).read_text().splitlines()[1:]:
        ds, q, ms = line.split("\t")[:3]
        cal.setdefault(ds, {})[q] = float(ms)
    return cal


def matrix(calibration=None) -> list[Config]:
    """The nine configurations in priority order, with minute estimates from calibration.tsv.
    Concurrent runs are costed as sequential (no overlap gain assumed: an upper bound); S3 runs
    as local × S3_FACTOR; a cached run adds one pass to fill the cache."""
    cal = read_calibration(calibration)
    out = []
    for c in MATRIX:
        c = dataclasses.replace(c)
        times = cal.get(c.dataset)
        if times:
            total = sum(times.get(q, 0.0) for q in queries(c.dataset, c.batch)) / 60000
            est = c.runs * total
            if c.warmup_query:
                est += times.get(c.warmup_query, 0.0) / 60000
            if c.source == "s3":
                est *= S3_FACTOR
            if c.cache:
                est += total
            c.estimate_min = round(est + STARTUP_MIN, 1)
        out.append(c)
    return out


def print_matrix(calibration) -> None:
    src = calibration if read_calibration(calibration) else "none (no calibration.tsv yet)"
    print(f"# estimates from {src}; concurrent costed as sequential, S3 as local x {S3_FACTOR}")
    print("priority\tname\truns\test_min\tcum_min\tcommand")
    cum = 0.0
    for c in matrix(calibration):
        est = "?" if c.estimate_min is None else f"{c.estimate_min}"
        cum = cum + c.estimate_min if c.estimate_min is not None else cum
        print(f"{c.priority}\t{c.name}\t{c.runs}\t{est}\t{cum:.1f}\t{c.command()}")


# ---- results -------------------------------------------------------------------------------

def prefix_sums(runs_tsv, sizes: list[int], run: int | None = None) -> dict[int, float]:
    """Sequential batch times: the summed ms of the first K queries of a run, in run order.
    Averaged over the measured runs (warm-ups `w…` excluded), or for one `run`."""
    per_run: dict[int, list[float]] = {}
    for line in Path(runs_tsv).read_text().splitlines()[1:]:
        q, r, ms = line.split("\t")[:3]
        if q == "batch" or not r.isdigit():
            continue
        per_run.setdefault(int(r), []).append(float(ms))
    runs = [run] if run is not None else sorted(per_run)
    return {k: sum(sum(per_run[r][:k]) for r in runs) / len(runs) for k in sizes}


def write_conf(path, conf: dict, extra: dict) -> None:
    """Settings as key=value, sorted, with secret-looking keys' values redacted; then `extra`."""
    lines = [f"{k}={'<redacted>' if REDACT.search(k) else v}" for k, v in sorted(conf.items())]
    lines.append("# run")
    lines += [f"{k}={v}" for k, v in extra.items()]
    Path(path).write_text("\n".join(lines) + "\n")


def set_status(out: Path, text: str) -> None:
    tmp = out / "status.tmp"
    tmp.write_text(f"{datetime.datetime.now().astimezone().isoformat(timespec='seconds')} {text}\n")
    tmp.replace(out / "status")


# ---- Spark ---------------------------------------------------------------------------------

def spark_conf(args, out: Path) -> dict[str, str]:
    conf = {
        # Comet's benchmark conf (benchmarks/tpc/run.py:107-116, engines/comet.toml:28-33 @ab46419f)
        "spark.driver.extraClassPath": COMET_JAR,
        "spark.executor.extraClassPath": COMET_JAR,
        "spark.plugins": "org.apache.spark.CometPlugin",
        "spark.shuffle.manager": "org.apache.spark.sql.comet.execution.shuffle.CometShuffleManager",
        "spark.comet.expression.Cast.allowIncompatible": "true",
        "spark.memory.offHeap.enabled": "true",
        "spark.eventLog.enabled": "true",
        # Deviations for one 128-vCPU, 512 GB executor (Recipes 3, estimates)
        "spark.driver.memory": "16g",
        "spark.executor.memory": "64g",
        "spark.memory.offHeap.size": "320g",
        "spark.executor.memoryOverhead": "48g",
        "spark.executor.cores": "128",
        "spark.cores.max": "128",
        "spark.sql.shuffle.partitions": "256",
        "spark.eventLog.dir": "file://" + str(Path.home() / "comet" / "eventlogs"),
        "spark.eventLog.compress": "true",
        "spark.eventLog.compression.codec": "zstd",
        # Fallback reporting (Recipes 2)
        "spark.comet.explain.fallback.enabled": "true",
        "spark.sql.extendedExplainProviders": "org.apache.comet.ExtendedExplainInfo",
    }
    if args.mode == "concurrent":
        xml = out / "fairscheduler.xml"
        xml.write_text('<?xml version="1.0"?>\n<allocations><pool name="batch">'
                       "<schedulingMode>FAIR</schedulingMode><weight>1</weight><minShare>0</minShare>"
                       "</pool></allocations>\n")
        conf["spark.scheduler.mode"] = "FAIR"
        conf["spark.scheduler.allocation.file"] = str(xml)
    if args.source == "s3":
        # Recipes 4. The profile provider reads AWS_PROFILE; no key ever enters this conf,
        # whose fs.s3a.* values are copied into every native plan.
        profile, endpoint = S3[args.s3_region]
        conf.update({
            "spark.hadoop.fs.s3a.endpoint": endpoint,
            "spark.hadoop.fs.s3a.endpoint.region": args.s3_region,
            "spark.hadoop.fs.s3a.path.style.access": "true",
            "spark.hadoop.fs.s3a.aws.credentials.provider":
                "software.amazon.awssdk.auth.credentials.ProfileCredentialsProvider",
            "spark.executorEnv.AWS_PROFILE": profile,
        })
    if args.cache:
        conf["spark.comet.exec.inMemoryCache.enabled"] = "true"
    return conf


def table_path(args, table: str) -> str:
    if args.source == "s3":
        return f"s3a://{args.dataset}-sf40/{table}.parquet"
    return str(Path(args.data_dir).expanduser() / f"{args.dataset}-sf40" / f"{table}.parquet")


def import_pyspark():
    try:
        import pyspark  # noqa: F401
    except ImportError:
        home = Path(os.environ.get("SPARK_HOME", "/opt/spark"))
        sys.path[:0] = [str(home / "python")] + [str(p) for p in (home / "python" / "lib").glob("py4j-*.zip")]
    import pyspark
    return pyspark


# Plan dump: comet-internals.md `## Recipes` 1, with the query already collected and the rows
# written as (query, comet_ops, spark_ops, reason).
P = "org.apache.spark.sql.execution."
AQE, QS = P + "adaptive.AdaptiveSparkPlanExec", P + "adaptive.QueryStageExec"
NATIVE = "org.apache.spark.sql.comet.CometNativeExec"
REUSED = {P + "exchange.ReusedExchangeExec", P + "ReusedSubqueryExec"}
WRAPPERS = {"AdaptiveSparkPlanExec", "InputAdapter", "WholeStageCodegenExec",
            "ReusedExchangeExec", "ReusedSubqueryExec"}


def _isa(o, fqcn):
    c = o.getClass()
    while c is not None:
        if c.getName() == fqcn:
            return True
        c = c.getSuperclass()
    return False


def _seq(s):
    return [s.apply(i) for i in range(s.size())]


def dump_plan(spark, name, df, outdir: Path):
    jvm = spark.sparkContext._jvm
    outdir.mkdir(parents=True, exist_ok=True)
    qe = df._jdf.queryExecution()
    root = qe.executedPlan()
    assert not _isa(root, AQE) or root.isFinalPlan(), name
    nodes, seen, stack = [], set(), [root]
    while stack:
        n = stack.pop()
        k = (n.getClass().getName(), jvm.java.lang.System.identityHashCode(n))
        if k in seen:
            continue
        seen.add(k); nodes.append(n)
        if _isa(n, AQE):
            kids = [n.executedPlan()]
        elif _isa(n, QS):
            kids = [n.plan()]
        elif k[0] in REUSED:
            kids = [n.child()]
        else:
            kids = _seq(n.children())
        stack += kids + _seq(n.subqueries())
    expl = jvm.org.apache.spark.sql.api.python.PythonSQLUtils.explainString(qe, "formatted")
    cov = jvm.org.apache.comet.ExtendedExplainInfo().generateVerboseInfo(root)
    (outdir / f"{name}.plan.txt").write_text(expl + "\n" + cov)
    tag, hashes, reasons = jvm.org.apache.comet.CometExplainInfo.FALLBACK_REASONS(), set(), []
    comet_ops = spark_ops = 0
    for n in nodes:
        op = n.getClass().getSimpleName()
        if not (_isa(n, AQE) or _isa(n, QS) or op in WRAPPERS):
            if op.startswith("Comet"):
                comet_ops += 1
            else:
                spark_ops += 1
        if _isa(n, NATIVE) and n.serializedPlanOpt().isDefined():
            b = bytes(n.serializedPlanOpt().plan().get())
            h = hashlib.sha1(b).hexdigest()
            if h not in hashes:
                base = outdir / f"{name}-{len(hashes):02d}-{op}"
                hashes.add(h)
                base.with_suffix(".pb").write_bytes(b)
                base.with_suffix(".txt").write_text(n.nativeOp().toString())
        v = n.getTagValue(tag)
        if v.isDefined():
            reasons += [f"{op}: {r}" for r in v.get().mkString("\n").split("\n") if r]
    fb = outdir / "fallbacks.tsv"
    new = not fb.exists()
    with open(fb, "a") as f:
        if new:
            f.write("query\tcomet_ops\tspark_ops\treason\n")
        for r in reasons or [""]:
            f.write(f"{name}\t{comet_ops}\t{spark_ops}\t{r.replace(chr(9), ' ')}\n")


class Runner:
    def __init__(self, args, spark, out: Path):
        self.args, self.spark, self.out = args, spark, out
        self.lock = threading.Lock()
        self.sql = {}
        qdir = Path(args.queries_dir) if args.queries_dir else default_queries_dir(args.dataset)
        for q in set(queries(args.dataset, args.batch)) | ({args.warmup_query} - {None}):
            self.sql[q] = statements((qdir / f"{q}.sql").read_text())
        self.tsv = open(out / "runs.tsv", "w")
        self.tsv.write("query\trun\tms\n"); self.tsv.flush()

    def record(self, q, run, ms):
        with self.lock:
            self.tsv.write(f"{q}\t{run}\t{ms:.1f}\n"); self.tsv.flush()

    def query(self, q, run, pool=None) -> float:
        sc = self.spark.sparkContext
        if pool:
            sc.setLocalProperty("spark.scheduler.pool", pool)
        sc.setJobGroup(f"{self.args.dataset}:{q}:{run}", q)
        stmts, ms = self.sql[q], 0.0
        for i, s in enumerate(stmts):
            t = time.perf_counter()
            df = self.spark.sql(s)
            df.collect()
            ms += (time.perf_counter() - t) * 1000
            if self.args.dump_plans and run == "1":  # the dump is not timed
                dump_plan(self.spark, q if len(stmts) == 1 else f"{q}-{i + 1}", df, Path(self.args.dump_plans))
        self.record(q, run, ms)
        return ms

    def sequential(self, qs, run):
        t = time.perf_counter()
        for i, q in enumerate(qs, 1):
            set_status(self.out, f"running run {run} query {i}/{len(qs)} {q}")
            self.query(q, run)
        self.record("batch", run, (time.perf_counter() - t) * 1000)

    def concurrent(self, qs, run):
        from pyspark import InheritableThread
        errors = []

        def one(q):
            try:
                self.query(q, run, pool="batch")
            except Exception as e:  # recorded below; the batch fails as a whole
                errors.append(f"{q}: {e!r}")

        set_status(self.out, f"running run {run} concurrent x{len(qs)}")
        t = time.perf_counter()
        threads = [InheritableThread(target=one, args=(q,)) for q in qs]
        for th in threads:
            th.start()
        for th in threads:
            th.join()
        if errors:
            raise RuntimeError("concurrent batch failed:\n" + "\n".join(errors))
        self.record("batch", run, (time.perf_counter() - t) * 1000)


def run(args) -> int:
    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    for f in ("rc", "error.txt"):
        (out / f).unlink(missing_ok=True)

    def cut(signum, _frame):
        set_status(out, f"cut by signal {signum}")
        (out / "rc").write_text("143\n")
        os._exit(143)
    signal.signal(signal.SIGTERM, cut)

    started = datetime.datetime.now().astimezone().isoformat(timespec="seconds")
    set_status(out, "starting")
    rc = 1
    try:
        (Path.home() / "comet" / "eventlogs").mkdir(parents=True, exist_ok=True)
        conf = spark_conf(args, out)
        if args.source == "s3":
            os.environ["AWS_PROFILE"] = S3[args.s3_region][0]  # the driver's S3A listing
        pyspark = import_pyspark()
        from pyspark.sql import SparkSession
        b = SparkSession.builder.appName(f"comet-{Path(args.out).name}").master(args.master)
        for k, v in conf.items():
            b = b.config(k, v)
        spark = b.getOrCreate()
        jvm = spark.sparkContext._jvm
        write_conf(out / "conf.txt", dict(spark.sparkContext.getConf().getAll()), {
            "host": socket.gethostname(),
            "local_start": started,
            "spark": spark.version,
            "pyspark": pyspark.__version__,
            "comet": f"{COMET_VERSION} ({COMET_JAR})",
            "java": jvm.java.lang.System.getProperty("java.version"),
            "python": platform.python_version(),
            "cpus": os.cpu_count(),
            "argv": " ".join(sys.argv),
        })
        set_status(out, "registering tables")
        for t in TABLES[args.dataset]:
            spark.read.parquet(table_path(args, t)).createOrReplaceTempView(t)
        if args.cache:
            set_status(out, "caching tables")
            for t in TABLES[args.dataset]:
                spark.catalog.cacheTable(t)
                spark.table(t).count()
        r = Runner(args, spark, out)
        qs = queries(args.dataset, args.batch)
        if args.warmup_query:
            set_status(out, f"warm-up query {args.warmup_query}")
            r.query(args.warmup_query, "w0")
        for w in range(1, args.warmup + 1):
            getattr(r, args.mode)(qs, f"w{w}")
        for i in range(1, args.runs + 1):
            getattr(r, args.mode)(qs, str(i))
        rc = 0
    except BaseException:
        (out / "error.txt").write_text(traceback.format_exc())
        traceback.print_exc()
    set_status(out, "done" if rc == 0 else "failed; see error.txt")
    (out / "rc").write_text(f"{rc}\n")
    return rc


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--dataset", choices=["tpch", "tpcds"])
    ap.add_argument("--source", choices=["s3", "local"], default="local")
    ap.add_argument("--mode", choices=["sequential", "concurrent"], default="sequential")
    ap.add_argument("--batch", default="all")
    ap.add_argument("--runs", type=int, default=1)
    ap.add_argument("--warmup", type=int, default=0)
    ap.add_argument("--warmup-query")
    ap.add_argument("--out")
    ap.add_argument("--dump-plans")
    ap.add_argument("--cache", action="store_true", help="in-memory cache: Comet's Arrow cache, tables cached first")
    ap.add_argument("--matrix", action="store_true", help="print the configurations and estimates; run nothing")
    ap.add_argument("--calibration", default=str(HERE.parent / "measurements" / "comet" / "calibration.tsv"))
    ap.add_argument("--s3-region", choices=sorted(S3), default="eu-west1")
    ap.add_argument("--data-dir", default="~/comet/data")
    ap.add_argument("--queries-dir")
    ap.add_argument("--master", default=os.environ.get("COMET_SPARK_MASTER", f"spark://{socket.gethostname()}:7077"))
    args = ap.parse_args(argv)
    if args.matrix:
        print_matrix(args.calibration)
        return 0
    if not (args.dataset and args.out):
        ap.error("--dataset and --out are required unless --matrix")
    queries(args.dataset, args.batch)  # validate before starting Spark
    return run(args)


if __name__ == "__main__":
    sys.exit(main())
