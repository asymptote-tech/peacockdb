#!/usr/bin/env python3
"""The Sirius image's runner (llm-wiki/tasks/sirius-bench.md), at /opt/sirius beside the extension,
its PIN and its config.

  sirius-bench.py --dataset tpch|tpcds --sf <n> --data <dir> --queries <stem>,… --runs <n>
                  --timeout <s> --out <testdata> [--list <stem>,…] [--resume]
  sirius-bench.py --self-test

A run times each query, by stem, from <out>/<ds>-queries/<stem>.sql over <data>/<table>.parquet,
in one worker process: Sirius loaded with DuckDB fallback off and the answers pass's settings, a
view per table, and every table pinned once (`tier = 'parquet'`), untimed and outside any
timeout. Sirius logs at info into <out>/calibration/sirius/<ds>.sf<sf>/log/, and each query's
warm-up must show there that it ran on the GPU, with no fallback, or its row is `failed: not run
on the GPU (<what the log showed>)` and it is not timed. Otherwise --runs runs are timed around the
statement and its Arrow fetch, and the last run's Arrow result is stored as table "<stem>" of
<out>/calibration/sirius/<ds>.sf<sf>/answers.duckdb through a plain DuckDB connection. Each row
goes into <out>/benchmark-results/h100-80gb-hbm3/<ds>.sf<sf>/sirius.tsv as it comes:
`ok` with the second-smallest run, else `failed: <first line>`. A query past --timeout (from its
warm-up) or a worker that dies is `failed: timeout after <s> s` or `failed: crashed (<signal>)`,
and a fresh worker pins again for the next query. --resume keeps the file's rows where its
`sirius` and `host` are this run's. --list names the whole list --queries narrows: the file's
other rows and the answers' other tables stay as they are, and a query's own answer is dropped
before it runs; a file of another `sirius` or `host` is refused. A timed row either keeps is
refused where the answers lack its table. Without either, or with no row kept, the file and the
answers are this run's alone. Rows go in the list's order (--list, else --queries).
Exit 0; 1 a worker could not pin; 2 bad input, refused before any query.

--self-test checks the image where no GPU is: the extension's sha256 against the PIN, Sirius's
LICENSE beside it against the PIN, its DuckDB footer, the libraries it needs (the driver's two, which only `docker run --gpus all` mounts, and
the C and C++ runtime), the Python DuckDB and pyarrow, and the config's three settings. It never
loads the extension: a GPU-less host has no libcuda to load it with. Exit 0, or 1 naming each
check that failed.
"""
import argparse
import hashlib
import multiprocessing
import os
import pathlib
import re
import signal
import socket
import subprocess
import sys
import time

HERE = pathlib.Path(__file__).resolve().parent
EXTENSION = HERE / "sirius.duckdb_extension"
PIN = HERE / "PIN"
CONFIG = HERE / "sirius-config.yaml"
# Sirius's Apache-2.0 licence at the pinned commit, which the image redistributes the extension under.
LICENSE = HERE / "LICENSE"

# Sirius's defaults but these (llm-wiki/tasks/sirius-bench.md, Done when 3), as the config spells them.
CONFIG_SETTINGS = {
    "sirius.executor.scan_manager.cache.mode": "sirius",
    "sirius.executor.scan_manager.cache.eviction_threshold_fraction": "1.0",
    "sirius.telemetry.enable_quent": "false",
}

# The results tree the runner writes: Sirius runs on the H100 alone (llm-wiki/tasks/sirius-bench.md).
DEVICE_TREE = "h100-80gb-hbm3"
# sirius.tsv's preamble keys and column line (peacockdb-core's test_support/sirius_results.rs).
PREAMBLE_KEYS = ["sirius", "duckdb", "libcudf", "cuda", "driver", "device", "host"]
COLUMNS = "query\tms_2nd_min\truns_ms\tstatus\tnote"
# DuckDB's sf40 answers were taken under these: benchmarks/duckdb_minimal.sh's ORACLE, the answers
# pass's session. integer_division is a parser setting, so each runs before the query is parsed.
ORACLE_SETTINGS = ["SET default_null_order='nulls_last_on_asc_first_on_desc'", "SET integer_division=true"]
# The name the last run's Arrow result is registered under on the answers connection.
ANSWER = "sirius_answer"
# Sirius's log, read from the environment at LOAD (Sirius 8d17afe, src/sirius_context.cpp). info is
# already its default level, so the timed runs log as Sirius would anyway.
LOG_SETTINGS = {"SIRIUS_LOG_BACKEND": "spdlog", "SIRIUS_LOG_LEVEL": "info"}
# A warm-up ran on the GPU when its log has the first line, then the operator in the plan Sirius
# logs, and no fallback line. Each string's source: llm-wiki/tasks/sirius-bench-detail.md, "Task 7
# (GPU evidence)".
GPU_EXECUTION = "Transparent GPU execution: executing query"
GPU_OPERATOR = "GPU_SCAN"
FALLBACK = re.compile(r"Transparent execution fallback|falling back to DuckDB CPU|fallback to DuckDB")
# spdlog's "[time] [level] [file:line] " before each message.
LOG_PREFIX = re.compile(r"^(\[[^\]]*\] ){3}")
# Sirius flushes its log every 3 s (sirius_log_flush_seconds); a warm-up's lines are awaited this long.
LOG_WAIT_S = 10
STEM = re.compile(r"[a-z0-9][a-z0-9-]*")

# The driver's libraries, not found where no GPU is mounted, and the runtime every image has.
DRIVER_LIBRARIES = {"libcuda.so.1", "libnvidia-ml.so.1"}
RUNTIME_LIBRARIES = {"linux-vdso.so.1", "ld-linux-x86-64.so.2", "libc.so.6", "libm.so.6", "libdl.so.2",
                     "libpthread.so.0", "librt.so.1", "libutil.so.1", "libgcc_s.so.1", "libstdc++.so.6"}


def read_pin(path):
    pairs = (line.split("=", 1) for line in path.read_text().splitlines() if line)
    return {key: value for key, value in pairs}


def footer(path):
    """The eight 32-byte metadata fields before a DuckDB extension's 256-byte signature, as stored:
    field 7 is the magic, 6 the platform, 5 the DuckDB version."""
    with path.open("rb") as f:
        f.seek(-512, 2)
        tail = f.read(256)
    return [tail[i * 32:(i + 1) * 32].rstrip(b"\0").decode("ascii", "replace") for i in range(8)]


def sha256_of(path):
    digest = hashlib.sha256()
    with path.open("rb") as f:
        for block in iter(lambda: f.read(1 << 24), b""):
            digest.update(block)
    return digest.hexdigest()


def library_problems(ldd_output):
    """Each library `ldd` names beyond the driver's and the runtime, each one missing that is not
    the driver's, and each symbol version the image's runtime lacks."""
    problems = []
    for line in ldd_output.splitlines():
        line = line.strip()
        if not line:
            continue
        if "version `" in line and "not found" in line:
            problems.append(f"ldd: {line}")
            continue
        name, _, found = (part.strip() for part in line.partition("=>"))
        name = name.split()[0].rsplit("/", 1)[-1]
        if name not in DRIVER_LIBRARIES | RUNTIME_LIBRARIES:
            problems.append(f"the extension needs {name}, which is neither the driver's nor the C/C++ runtime")
        elif found.startswith("not found") and name not in DRIVER_LIBRARIES:
            problems.append(f"the extension needs {name}, not found in the image")
    return problems


def read_config(text):
    """The config as dotted keys: nested mappings of scalars, two spaces a level, and nothing else."""
    settings, path = {}, []
    for number, raw in enumerate(text.splitlines(), 1):
        line = raw.split(" #", 1)[0].rstrip()
        if not line.strip() or line.lstrip().startswith("#"):
            continue
        indent = len(line) - len(line.lstrip(" "))
        key, colon, value = line.strip().partition(":")
        if indent % 2 or indent // 2 > len(path) or not colon or not key or "\t" in raw:
            raise ValueError(f"line {number} is not a nested mapping of scalars: {raw!r}")
        path = path[:indent // 2]
        if value.strip():
            settings[".".join(path + [key])] = value.strip()
        else:
            path.append(key)
    return settings


def self_test():
    problems, notes = [], []
    pin = read_pin(PIN)
    got = sha256_of(EXTENSION)
    if got != pin["extension_sha256"]:
        problems.append(f"the extension's sha256 is {got}, the PIN's {pin['extension_sha256']}")
    if not LICENSE.is_file():
        problems.append(f"no {LICENSE}: the extension ships under Sirius's Apache-2.0 LICENSE")
    elif (got := sha256_of(LICENSE)) != pin["license_sha256"]:
        problems.append(f"{LICENSE}'s sha256 is {got}, the PIN's {pin['license_sha256']}")
    fields = footer(EXTENSION)
    want = {5: f"v{pin['duckdb']}", 6: "linux_amd64", 7: "4"}
    for at, value in want.items():
        if fields[at] != value:
            problems.append(f"footer field {at} reads {fields[at]!r}, not {value!r}")
    notes.append(f"footer: {fields[5]} {fields[6]}")
    ldd = subprocess.run(["ldd", str(EXTENSION)], capture_output=True, text=True)
    if not (ldd.stdout + ldd.stderr).strip():
        problems.append(f"ldd printed nothing for {EXTENSION} (exit {ldd.returncode})")
    problems += library_problems(ldd.stdout + ldd.stderr)
    import duckdb
    import pyarrow
    if duckdb.__version__ != pin["duckdb"]:
        problems.append(f"Python DuckDB is {duckdb.__version__}, the PIN's {pin['duckdb']}")
    notes.append(f"duckdb {duckdb.__version__}, pyarrow {pyarrow.__version__}")
    try:
        config = read_config(CONFIG.read_text())
    except ValueError as e:
        problems.append(f"{CONFIG}: {e}")
        config = {}
    if config and config != CONFIG_SETTINGS:
        extra = sorted(set(config) - set(CONFIG_SETTINGS))
        wrong = sorted(k for k in CONFIG_SETTINGS if config.get(k) != CONFIG_SETTINGS[k])
        problems.append(f"{CONFIG} is not the three settings: unexpected {extra}, wrong or missing {wrong}")
    notes += [f"config: {key}={value}" for key, value in config.items()]
    for note in notes:
        print(note)
    for problem in problems:
        print(f"sirius-bench.py: self-test: {problem}", file=sys.stderr)
    if problems:
        return 1
    print("self-test ok")
    return 0


class Refused(Exception):
    """Bad input, found before any query runs."""


def first_line(error):
    lines = str(error).strip().splitlines()
    return (lines[0] if lines else type(error).__name__).replace("\t", " ").strip()


def provenance():
    """sirius.tsv's `# ` values: the image's pins, the device nvidia-smi reports, and this host."""
    pin = read_pin(PIN)
    try:
        gpu = subprocess.run(["nvidia-smi", "--query-gpu=name,driver_version", "--format=csv,noheader"],
                             capture_output=True, text=True, check=True).stdout
        header = subprocess.run(["nvidia-smi"], capture_output=True, text=True, check=True).stdout
    except (OSError, subprocess.CalledProcessError) as e:
        raise Refused(f"nvidia-smi: {first_line(e)}: run the image under `docker run --gpus all`")
    name, _, driver = gpu.splitlines()[0].rpartition(",")
    cuda = re.search(r"CUDA Version:\s*([\d.]+)", header)
    if not name.strip() or not cuda:
        raise Refused(f"nvidia-smi names no device or CUDA version: {gpu.strip()!r}")
    values = {"sirius": pin["sirius"], "duckdb": pin["duckdb"], "libcudf": pin["libcudf"], "cuda": cuda.group(1),
              "driver": driver.strip(), "device": name.strip(), "host": socket.gethostname()}
    return {key: re.sub(r"\s+", "_", values[key]) for key in PREAMBLE_KEYS}


def read_results(path):
    """An existing sirius.tsv: its `# ` values and its rows by query, each row's line as written."""
    lines = path.read_text().splitlines()
    preamble = dict(word.split("=", 1) for word in lines[0][2:].split(" ") if "=" in word)
    return preamble, {line.split("\t", 1)[0]: line for line in lines[2:] if line}


def write_results(path, preamble, rows):
    """The file whole, staged and renamed, so a killed run leaves the last complete one."""
    text = "# " + " ".join(f"{key}={value}" for key, value in preamble.items()) + "\n" + COLUMNS + "\n"
    text += "".join(line + "\n" for line in rows.values())
    staged = path.with_name(path.name + ".new")
    staged.write_text(text)
    os.replace(staged, path)


def ok_row(stem, runs):
    measured = ",".join(f"{ms:.1f}" for ms in runs)
    return f"{stem}\t{sorted(runs)[1]:.1f}\t{measured}\tok\t-"


def failed_row(stem, why):
    return f"{stem}\t-\t-\tfailed\t{why}"


def log_sizes(log_dir):
    """Each of Sirius's log files and its size: spdlog names the file by day, sirius_<date>.log."""
    return {path.name: path.stat().st_size for path in log_dir.glob("sirius*.log")}


def log_since(log_dir, sizes):
    """What Sirius has written to its log since `sizes`, oldest day first."""
    text = []
    for path in sorted(log_dir.glob("sirius*.log")):
        with path.open("rb") as f:
            f.seek(sizes.get(path.name, 0))
            text.append(f.read().decode("utf-8", "replace"))
    return "".join(text)


def label_line(label):
    """The QueryBegin line Sirius logs for the statement that labels the next query."""
    return f"SQL: CALL sirius_set_query_label('{label}')"


def warm_up_log(log_dir, sizes, stem):
    """The warm-up's lines: from its label's statement to the timed runs' label's, which bound it
    in a log that other queries' late-flushed lines share. None if the log shows no end in time."""
    start, end = label_line(stem), label_line(f"{stem} timed")
    deadline = time.monotonic() + LOG_WAIT_S
    while True:
        text = log_since(log_dir, sizes)
        begin = text.find(start)
        finish = text.find(end, begin) if begin >= 0 else -1
        if finish >= 0:
            return text[begin:finish]
        if time.monotonic() > deadline:
            return None
        time.sleep(0.1)


def off_the_gpu(log):
    """None when a warm-up's log shows it ran on the GPU, else what the log showed instead."""
    if log is None:
        return f"no log of its warm-up in {LOG_WAIT_S} s"
    for line in log.splitlines():
        if FALLBACK.search(line):
            return LOG_PREFIX.sub("", line).replace("\t", " ").strip()[:100]
    executing = log.find(GPU_EXECUTION)
    if executing < 0:
        return "no Transparent GPU execution line"
    if GPU_OPERATOR not in log[executing:]:
        return "Transparent GPU execution, no GPU_SCAN in its plan"
    return None


def answer(con, answers, log_dir, runs, stem, sql):
    """One query's message to the parent: a labelled warm-up, its GPU evidence, the timed runs and
    the stored answer, or the step that failed. An earlier run's answer goes first, so a query that
    fails now leaves none."""
    try:
        answers.execute(f'DROP TABLE IF EXISTS "{stem}"')
    except Exception as e:
        return "failed", f"the earlier answer was not dropped: {first_line(e)}"
    try:
        sizes = log_sizes(log_dir)
        con.execute(f"CALL sirius_set_query_label('{stem}')")
        con.execute(sql).to_arrow_table()
        con.execute(f"CALL sirius_set_query_label('{stem} timed')")
        shown = off_the_gpu(warm_up_log(log_dir, sizes, stem))
        if shown:
            return "failed", f"not run on the GPU ({shown})"
        timed, table = [], None
        for _ in range(runs):
            table = None
            start = time.perf_counter_ns()
            table = con.execute(sql).to_arrow_table()
            timed.append((time.perf_counter_ns() - start) / 1e6)
    except Exception as e:
        return "failed", first_line(e)
    try:
        answers.register(ANSWER, table)
        try:
            answers.execute(f'CREATE OR REPLACE TABLE "{stem}" AS SELECT * FROM {ANSWER}')
        finally:
            answers.unregister(ANSWER)
    except Exception as e:
        return "failed", f"the answer was not stored: {first_line(e)}"
    return "ok", timed


def worker(conn, config, log_dir, tables, answers_path, runs):
    """One Sirius process: pins every table, then runs what the parent sends until it sends None.
    It answers each query before it reads the next, so a row is the parent's before this exits."""
    os.environ.update(SIRIUS_CONFIG_FILE=str(config), SIRIUS_LOG_DIR=str(log_dir), **LOG_SETTINGS)
    import duckdb
    try:
        con = duckdb.connect(":memory:", config={"allow_unsigned_extensions": "true"})
        con.execute(f"LOAD '{EXTENSION}'")
        con.execute("SET enable_duckdb_fallback = false")
        for setting in ORACLE_SETTINGS:
            con.execute(setting)
        for table, path in tables:
            con.execute(f"CREATE VIEW \"{table}\" AS SELECT * FROM read_parquet('{path}')")
        start = time.perf_counter()
        for table, path in tables:
            con.execute(f"CALL pin_table('{path}', name = '{table}', tier = 'parquet')")
        pinned = time.perf_counter() - start
        answers = duckdb.connect(str(answers_path))
    except Exception as e:
        conn.send(("unready", first_line(e)))
        return
    conn.send(("pinned", pinned))
    while (task := conn.recv()) is not None:
        conn.send(answer(con, answers, log_dir, runs, *task))


class Worker:
    """The parent's side of one worker process."""

    def __init__(self, context, *args):
        self.conn, child = context.Pipe()
        self.proc = context.Process(target=worker, args=(child, *args), daemon=True)
        self.proc.start()
        child.close()

    def ended(self):
        """How the process ended, once it has: its signal's name, or its exit code."""
        self.proc.join(30)
        code = self.proc.exitcode
        if code is None:
            self.kill()
            return "still running"
        return signal.Signals(-code).name if code < 0 else f"exit {code}"

    def kill(self):
        self.proc.kill()
        self.proc.join()

    def stop(self):
        try:
            self.conn.send(None)
        except OSError:
            pass
        self.proc.join(60)
        if self.proc.is_alive():
            self.kill()


def run_query(work, stem, sql, timeout):
    """(row, whether the worker lives on). The timeout counts from the warm-up's start."""
    work.conn.send((stem, sql))
    if not work.conn.poll(timeout):
        work.kill()
        return failed_row(stem, f"timeout after {timeout} s"), False
    try:
        kind, value = work.conn.recv()
    except EOFError:
        return failed_row(stem, f"crashed ({work.ended()})"), False
    if kind == "ok":
        return ok_row(stem, value), True
    return failed_row(stem, value), True


def checked(args):
    """The run's inputs, or Refused naming the first that is wrong: (tables, [(stem, sql)])."""
    if args.dataset not in ("tpch", "tpcds"):
        raise Refused(f"--dataset {args.dataset}: tpch or tpcds")
    if not re.fullmatch(r"\d+", args.sf):
        raise Refused(f"--sf {args.sf}: a scale factor of digits")
    if args.runs < 2:
        raise Refused(f"--runs {args.runs}: the second-smallest of at least two runs is reported")
    if args.timeout < 1:
        raise Refused(f"--timeout {args.timeout}: seconds, at least 1")
    data, out = pathlib.Path(args.data), pathlib.Path(args.out)
    parquet = sorted(data.glob("*.parquet")) if data.is_dir() else []
    if not parquet:
        raise Refused(f"--data {data}: no <table>.parquet there")
    if any("'" in str(path) for path in parquet):
        raise Refused(f"--data {data}: a path with a quote")
    stems = args.queries.split(",")
    queries = []
    for stem in stems:
        if not STEM.fullmatch(stem):
            raise Refused(f"--queries: {stem!r} is not a query's stem (aggregate-groupby, not aggregate_groupby)")
        sql = out / f"{args.dataset}-queries" / f"{stem}.sql"
        if not sql.is_file():
            raise Refused(f"--queries: {stem} has no {sql}")
        queries.append((stem, sql.read_text()))
    if len(set(stems)) != len(stems):
        raise Refused(f"--queries names a query twice: {args.queries}")
    if args.list is not None:
        listed = args.list.split(",")
        bad = [stem for stem in listed if not STEM.fullmatch(stem)]
        if bad:
            raise Refused(f"--list: {bad[0]!r} is not a query's stem (aggregate-groupby, not aggregate_groupby)")
        if len(set(listed)) != len(listed):
            raise Refused(f"--list names a query twice: {args.list}")
        outside = [stem for stem in stems if stem not in listed]
        if outside:
            raise Refused(f"--queries names {', '.join(outside)}, which --list does not")
    return [(path.stem, str(path.resolve())) for path in parquet], queries


def in_order(rows, order):
    """`rows` in `order`'s order, a row it does not name after them in the order it had."""
    rank = {stem: at for at, stem in enumerate(order)}
    return dict(sorted(rows.items(), key=lambda item: rank.get(item[0], len(order))))


def without_answers(answers, rows):
    """The timed rows of `rows` whose table the answers database lacks: all of them where it is missing."""
    timed = [stem for stem, row in rows.items() if row.split("\t")[1] != "-"]
    if not timed or not answers.is_file():
        return timed
    import duckdb
    try:
        con = duckdb.connect(str(answers), read_only=True)
        try:
            stored = {name for (name,) in con.execute("SELECT table_name FROM duckdb_tables()").fetchall()}
        finally:
            con.close()
    except Exception as e:
        raise Refused(f"{answers}: {first_line(e)}")
    return [stem for stem in timed if stem not in stored]


def run(args):
    tables, queries = checked(args)
    preamble = provenance()
    tsv = pathlib.Path(args.out) / "benchmark-results" / DEVICE_TREE / f"{args.dataset}.sf{args.sf}" / "sirius.tsv"
    answers = pathlib.Path(args.out) / "calibration" / "sirius" / f"{args.dataset}.sf{args.sf}" / "answers.duckdb"
    # Kept across runs: each warm-up is found by its own label lines, past the sizes it started at.
    log_dir = answers.with_name("log")
    stems = [stem for stem, _ in queries]
    order = args.list.split(",") if args.list is not None else stems
    rows = {}
    if (args.resume or args.list is not None) and tsv.is_file():
        theirs, kept = read_results(tsv)
        ours = all(theirs.get(key) == preamble[key] for key in ("sirius", "host"))
        if args.list is not None and not ours:
            mine = " ".join(f"{key}={preamble[key]}" for key in ("sirius", "host"))
            raise Refused(f"--list: {tsv} is of sirius={theirs.get('sirius')} host={theirs.get('host')}, "
                          f"not this run's {mine}: run the whole list, or remove the file")
        if ours:
            rows = kept if args.resume else {stem: row for stem, row in kept.items() if stem not in stems}
    if rows:
        unanswered = without_answers(answers, rows)
        if unanswered:
            raise Refused(f"{answers} lacks {', '.join(unanswered)}, the answers of timed rows this run keeps: "
                          f"get the database home (run-gpu.sh … pull from the run that wrote it), "
                          f"or run the whole dataset")
    else:
        # A table left from another run is an answer to a query this run may not answer.
        for stale in (answers, answers.with_name(answers.name + ".wal")):
            stale.unlink(missing_ok=True)
    tsv.parent.mkdir(parents=True, exist_ok=True)
    log_dir.mkdir(parents=True, exist_ok=True)
    write_results(tsv, preamble, in_order(rows, order))
    context = multiprocessing.get_context("spawn")
    work = None
    for stem, sql in queries:
        if stem in rows:
            print(f"{stem}: kept", flush=True)
            continue
        if work is None:
            work = Worker(context, CONFIG, log_dir, tables, answers, args.runs)
            try:
                kind, value = work.conn.recv()
            except EOFError:
                kind, value = "unready", f"crashed ({work.ended()})"
            if kind != "pinned":
                print(f"sirius-bench.py: the worker could not pin {len(tables)} tables: {value}", file=sys.stderr)
                return 1
            print(f"pinned {len(tables)} tables in {value:.1f} s", flush=True)
        row, alive = run_query(work, stem, sql, args.timeout)
        rows = in_order({**rows, stem: row}, order)
        write_results(tsv, preamble, rows)
        print(row.replace("\t", " "), flush=True)
        if not alive:
            work = None
    if work is not None:
        work.stop()
    return 0


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--self-test", action="store_true", help="check the image without a GPU, and exit")
    parser.add_argument("--dataset")
    parser.add_argument("--sf")
    parser.add_argument("--data")
    parser.add_argument("--queries")
    parser.add_argument("--runs", type=int)
    parser.add_argument("--timeout", type=int)
    parser.add_argument("--out")
    parser.add_argument("--list")
    parser.add_argument("--resume", action="store_true")
    args = parser.parse_args()
    run_flags = ["dataset", "sf", "data", "queries", "runs", "timeout", "out"]
    if args.self_test:
        if any(getattr(args, flag) is not None for flag in run_flags + ["list"]) or args.resume:
            parser.error("--self-test runs alone")
        return self_test()
    missing = [f"--{flag}" for flag in run_flags if getattr(args, flag) is None]
    if missing:
        parser.error(f"a run needs {', '.join(missing)}")
    try:
        return run(args)
    except Refused as e:
        print(f"sirius-bench.py: {e}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
