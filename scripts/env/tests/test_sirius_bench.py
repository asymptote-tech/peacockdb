"""docker/sirius-bench.py, the Sirius image's runner. Its --self-test checks the image without a GPU:
the extension's sha256 against the image's PIN, its footer, the libraries it needs, and the
config, and never loads it. The run pins every table once per worker, times each query, stores
its answer, and survives a query that errors, hangs or crashes its worker.

It runs over a copy of the script beside fake image files, with fake `ldd`, `nvidia-smi`, `duckdb`
and `pyarrow` first on the paths, so nothing here needs the image or a GPU.
"""
import hashlib
import json
import os
import re
import socket
import pathlib
import shutil
import subprocess
import sys
import tempfile

sys.path.append(str(pathlib.Path(__file__).resolve().parents[2] / "exec_model" / "tests"))
from harness import main  # noqa: E402

REPO = pathlib.Path(__file__).resolve().parents[3]
SCRIPT = REPO / "docker" / "sirius-bench.py"
CONFIG = REPO / "docker" / "sirius-config.yaml"
LICENSE = "Apache License\nVersion 2.0, January 2004\n"

# What `ldd` prints for the real extension on a GPU-less host: the driver's two libraries are not
# found, everything else is the C and C++ runtime.
LDD = """\tlinux-vdso.so.1 (0x00007ffc)
\tlibnvidia-ml.so.1 => not found
\tlibgcc_s.so.1 => /lib/x86_64-linux-gnu/libgcc_s.so.1 (0x00007f01)
\tlibutil.so.1 => /lib/x86_64-linux-gnu/libutil.so.1 (0x00007f02)
\tlibrt.so.1 => /lib/x86_64-linux-gnu/librt.so.1 (0x00007f03)
\tlibpthread.so.0 => /lib/x86_64-linux-gnu/libpthread.so.0 (0x00007f04)
\tlibcuda.so.1 => not found
\tlibdl.so.2 => /lib/x86_64-linux-gnu/libdl.so.2 (0x00007f05)
\tlibstdc++.so.6 => /lib/x86_64-linux-gnu/libstdc++.so.6 (0x00007f06)
\tlibm.so.6 => /lib/x86_64-linux-gnu/libm.so.6 (0x00007f07)
\tlibc.so.6 => /lib/x86_64-linux-gnu/libc.so.6 (0x00007f08)
\t/lib64/ld-linux-x86-64.so.2 (0x00007f09)
"""

FAKE_DUCKDB = '''__version__ = "1.5.6"
def connect(*args, **kwargs):
    raise SystemExit("the self-test connected to DuckDB")
'''


def footer(fields):
    """A DuckDB extension footer: eight 32-byte fields, last first, then a 256-byte signature."""
    return b"".join(f.encode().ljust(32, b"\0") for f in fields) + b"\0" * 256


GOOD_FOOTER = ["", "", "", "CPP", "dev", "v1.5.6", "linux_amd64", "4"]


def self_test(fields=GOOD_FOOTER, ldd=LDD, pin_sha=None, config=None, licence=LICENSE):
    """The self-test over fake image files; `licence` None leaves the LICENSE out."""
    with tempfile.TemporaryDirectory() as tmp:
        tmp = pathlib.Path(tmp)
        here = tmp / "opt-sirius"
        here.mkdir()
        shutil.copy2(SCRIPT, here / "sirius-bench.py")
        extension = b"\x7fELF fake extension body" + footer(fields)
        (here / "sirius.duckdb_extension").write_bytes(extension)
        sha = pin_sha or hashlib.sha256(extension).hexdigest()
        licence_sha = hashlib.sha256(LICENSE.encode()).hexdigest()
        (here / "PIN").write_text(f"sirius=8d17afec481c56903b61b74a9787423b470298a1\nduckdb=1.5.6\n"
                                  f"libcudf=26.08.01\nextension_sha256={sha}\nlicense_sha256={licence_sha}\n")
        if licence is not None:
            (here / "LICENSE").write_text(licence)
        (here / "sirius-config.yaml").write_text(config if config is not None else CONFIG.read_text())
        fakes = tmp / "fakes"
        fakes.mkdir()
        (fakes / "ldd").write_text(f"#!/bin/bash\ncat <<'EOF'\n{ldd}EOF\n")
        (fakes / "ldd").chmod(0o755)
        (fakes / "duckdb.py").write_text(FAKE_DUCKDB)
        (fakes / "pyarrow.py").write_text('__version__ = "25.0.1"\n')
        env = {**os.environ, "PATH": f"{fakes}:{os.environ['PATH']}", "PYTHONPATH": str(fakes)}
        return subprocess.run([sys.executable, str(here / "sirius-bench.py"), "--self-test"], env=env,
                              capture_output=True, text=True, timeout=60)


def failed(proc, *words):
    assert proc.returncode == 1, (proc.returncode, proc.stdout, proc.stderr)
    assert all(word in proc.stderr for word in words), (words, proc.stderr)


def test_the_self_test_passes_on_the_image_s_files():
    proc = self_test()
    assert proc.returncode == 0, (proc.stdout, proc.stderr)
    assert "self-test ok" in proc.stdout, proc.stdout


def test_an_extension_that_is_not_the_pinned_one_fails():
    failed(self_test(pin_sha="0" * 64), "sha256", "0" * 64)


def test_an_extension_for_another_duckdb_or_platform_fails():
    for at, value in [(5, "v1.5.4"), (6, "osx_arm64"), (7, "3")]:
        fields = list(GOOD_FOOTER)
        fields[at] = value
        failed(self_test(fields=fields), "footer", value)


def test_sirius_s_licence_must_ship_beside_the_extension():
    """The extension is Apache-2.0, which the image redistributes: its LICENSE goes with it, the one
    the PIN names."""
    failed(self_test(licence=None), "LICENSE")
    failed(self_test(licence=LICENSE + "amended\n"), "LICENSE", "sha256")


def test_a_library_beyond_the_driver_and_the_c_runtime_fails():
    failed(self_test(ldd=LDD + "\tlibcudf.so => /opt/cudf/lib/libcudf.so (0x1)\n"), "libcudf.so")


def test_a_runtime_library_the_image_lacks_fails():
    failed(self_test(ldd=LDD.replace("libstdc++.so.6 => /lib/x86_64-linux-gnu/libstdc++.so.6 (0x00007f06)",
                                     "libstdc++.so.6 => not found")), "libstdc++.so.6", "not found")
    failed(self_test(ldd=LDD + "\t/opt/sirius/sirius.duckdb_extension: /lib/x86_64-linux-gnu/libstdc++.so.6: "
                               "version `GLIBCXX_3.4.32' not found\n"), "GLIBCXX_3.4.32")


def test_the_config_must_hold_exactly_its_three_settings():
    text = CONFIG.read_text()
    failed(self_test(config=text.replace("enable_quent: false", "enable_quent: true")), "enable_quent")
    failed(self_test(config=text + "  memory:\n    gpu:\n      usage_limit_fraction: 0.5\n"),
           "usage_limit_fraction")


def test_the_config_file_is_the_three_settings():
    """docker/sirius-config.yaml itself, read by the self-test's own reader: Sirius's defaults but
    for pinning's two settings and the telemetry it turns off (llm-wiki/tasks/sirius-bench.md)."""
    proc = self_test()
    assert proc.returncode == 0, proc.stderr
    for line in ["sirius.executor.scan_manager.cache.mode=sirius",
                 "sirius.executor.scan_manager.cache.eviction_threshold_fraction=1.0",
                 "sirius.telemetry.enable_quent=false"]:
        assert line in proc.stdout, (line, proc.stdout)


# --- the run -----------------------------------------------------------------------------------

# A DuckDB that logs every connection, statement and registration as a JSON line, and answers a
# query (named by its `/* <stem> */` comment) as FAKE_DUCKDB_SCRIPT scripts it: a warm-up and
# per-run latency in ms, an error, a hang, a crash by signal, or an error writing its answer. The
# latency also moves the runner's `time.perf_counter_ns`, which reads the fake's clock alone.
# After LOAD it also writes Sirius's own log as Sirius 8d17afe does under SIRIUS_LOG_BACKEND=spdlog:
# `<SIRIUS_LOG_DIR>/sirius_<day>.log`, a QueryBegin line per statement, buffered and flushed every
# `log_flush_s`. A query's `log` says what its executions write: `gpu` (the default), `cpu` (a
# silent decline), `no_scan`, `fallback` (a runtime fallback) or `plan_fallback`; `rotate` starts a
# new day's file at its warm-up. A database file's tables are the stems after ": " in its text.
FAKE_RUNNER_DUCKDB = r'''import json, os, re, signal, threading, time
__version__ = "1.5.6"
_SCRIPT = json.load(open(os.environ["FAKE_DUCKDB_SCRIPT"]))
_counts = {}
_sirius = {"dir": None, "day": "2026-11-10", "pending": [], "ordinal": 0}
_lock = threading.Lock()
# The runner's clock: it moves by each query's scripted latency alone, so what the runner measures
# around a statement is that latency exactly, whatever else the host is doing.
_clock = {"ns": 0}
time.perf_counter_ns = lambda: _clock["ns"]
_PLAN = "Query Plan:\nPipeline #0\n  {} (id=0)\n  -> GROUPED_AGGREGATE (id=1)"
_LINES = {
    "gpu": ["Transparent execution: physical plan replaced with GPU operator",
            "Transparent GPU execution: executing query", _PLAN.format("GPU_SCAN"),
            "Transparent GPU execution: query completed"],
    "cpu": [],
    "no_scan": ["Transparent GPU execution: executing query", _PLAN.format("GPU_VALUES"),
                "Transparent GPU execution: query completed"],
    "fallback": ["Transparent GPU execution: executing query", _PLAN.format("GPU_SCAN"),
                 "Transparent GPU execution error: out of memory",
                 "Transparent execution: GPU execution failed at runtime; falling back to DuckDB CPU "
                 "within the same transaction. GPU error: out of memory",
                 "Transparent GPU execution: query completed"],
    "plan_fallback": ["Transparent execution fallback (unsupported): Distinct not supported"],
}


def _log(**entry):
    with open(os.environ["FAKE_DUCKDB_LOG"], "a") as f:
        f.write(json.dumps({"pid": os.getpid(), **entry}) + "\n")


def _sirius_log(message):
    if _sirius["dir"] is None:
        return
    with _lock:
        _sirius["pending"].append((_sirius["day"], f"[2026-11-10 02:21:00.000] [info] [fake.cpp:1] {message}\n"))


def _flush_every(seconds):
    while True:
        time.sleep(seconds)
        with _lock:
            pending, _sirius["pending"] = _sirius["pending"], []
        for day, line in pending:
            with open(os.path.join(_sirius["dir"], f"sirius_{day}.log"), "a") as f:
                f.write(line)


def _load():
    _log(event="load", log={k: os.environ.get(k) for k in ("SIRIUS_LOG_BACKEND", "SIRIUS_LOG_LEVEL", "SIRIUS_LOG_DIR")})
    if os.environ.get("SIRIUS_LOG_BACKEND", "spdlog") == "spdlog":
        _sirius["dir"] = os.environ.get("SIRIUS_LOG_DIR", "log")
        os.makedirs(_sirius["dir"], exist_ok=True)
        threading.Thread(target=_flush_every, args=(_SCRIPT.get("log_flush_s", 0.05),), daemon=True).start()


class _Table:
    def __init__(self, ident):
        self.ident = ident


class _Result:
    def __init__(self, table, rows=()):
        self._table, self._rows = table, list(rows)

    def to_arrow_table(self):
        return self._table

    def fetchall(self):
        return self._rows


class _Connection:
    def __init__(self, database, config):
        self.database, self.registered = database, {}
        _log(event="connect", database=database, config=config)

    def execute(self, sql):
        _log(event="execute", database=self.database, sql=sql)
        if self.database == ":memory:":
            if sql.startswith("LOAD "):
                _load()
            else:
                _sirius["ordinal"] += 1
                _sirius_log(f"QueryBegin: instance=0x1 connection=1 query={_sirius['ordinal']} SQL: "
                            + " ".join(sql.split()))
        if "pin_table" in sql:
            time.sleep(_SCRIPT.get("pin_s", 0))
            return _Result(None)
        if self.database != ":memory:":
            if "duckdb_tables()" in sql:
                stems = open(self.database).read().partition(": ")[2]
                return _Result(None, [(stem,) for stem in stems.split(",") if stem])
            m = re.fullmatch(r'CREATE OR REPLACE TABLE "([^"]+)" AS SELECT \* FROM (\w+)', sql)
            if m:
                error = _SCRIPT.get("queries", {}).get(m.group(1), {}).get("answer_error")
                if error:
                    raise RuntimeError(error)
                _log(event="answer", stem=m.group(1), table=self.registered[m.group(2)].ident)
            return _Result(None)
        m = re.search(r"/\* (\S+) \*/", sql)
        if not m:
            return _Result(None)
        stem = m.group(1)
        q = _SCRIPT.get("queries", {}).get(stem, {})
        n = _counts[stem] = _counts.get(stem, 0) + 1
        if n == 1 and q.get("rotate"):
            _sirius["day"] = q["rotate"]
        if q.get("error"):
            raise RuntimeError(q["error"])
        if q.get("crash"):
            _sirius_log("Transparent GPU execution: executing query")
            os.kill(os.getpid(), getattr(signal, q["crash"]))
        if q.get("hang"):
            time.sleep(3600)
        ms = q.get("warmup_ms", 1) if n == 1 else q.get("runs_ms", [1] * 100)[n - 2]
        time.sleep(ms / 1000)
        _clock["ns"] += round(ms * 1e6)
        for line in _LINES[q.get("log", "gpu")]:
            _sirius_log(line)
        return _Result(_Table(f"{stem}#{n}"))

    def register(self, name, table):
        self.registered[name] = table
        _log(event="register", database=self.database, name=name, table=table.ident)

    def unregister(self, name):
        self.registered.pop(name)

    def close(self):
        pass


def connect(database=":memory:", config=None, **kwargs):
    return _Connection(database, config or {})
'''

FAKE_NVIDIA_SMI = """#!/bin/bash
case "$*" in
  *--query-gpu=name,driver_version*) echo "NVIDIA H100 80GB HBM3, 580.173" ;;
  *) echo "| NVIDIA-SMI 580.173      Driver Version: 580.173      CUDA Version: 13.0     |" ;;
esac
"""

TABLES = ["lineitem", "nation", "region"]


def log_dir(out):
    """Where the runner points Sirius's log: beside the answers, gitignored, pulled home."""
    return out / "calibration" / "sirius" / "tpch.sf40" / "log"
PIN_COMMIT = "8d17afec481c56903b61b74a9787423b470298a1"


class Run:
    def __init__(self, proc, tsv, calls, logs, answers_kept=None):
        self.proc, self.tsv, self.calls, self.logs = proc, tsv, calls, logs
        # Whether the answers database `run_queries(answers=…)` left there is still the same file.
        self.answers_kept = answers_kept

    def rows(self):
        """`sirius.tsv`'s rows as {query: (ms_2nd_min, runs_ms, status, note)}."""
        lines = self.tsv.splitlines()[2:]
        return {f[0]: tuple(f[1:]) for f in (line.split("\t") for line in lines)}

    def executed(self, pattern, database=":memory:"):
        return [c for c in self.calls if c["event"] == "execute" and c["database"] == database
                and re.search(pattern, c["sql"])]


def run_queries(stems, script=None, runs=10, timeout=60, extra=(), existing=None, files=None, old_log=None,
                answers=None):
    """The runner over `stems` (each a `/* <stem> */ SELECT 1` file) in a temp image and testdata.
    `old_log` is a Sirius log an earlier run left in the log directory; `answers`, a list of stems,
    leaves an earlier run's answers database there too, holding those stems' tables."""
    with tempfile.TemporaryDirectory() as tmp:
        tmp = pathlib.Path(tmp)
        here = tmp / "opt-sirius"
        here.mkdir()
        shutil.copy2(SCRIPT, here / "sirius-bench.py")
        (here / "sirius.duckdb_extension").write_bytes(b"extension")
        (here / "PIN").write_text(f"sirius={PIN_COMMIT}\nduckdb=1.5.6\nlibcudf=26.08.01\nextension_sha256={'0' * 64}\n")
        shutil.copy2(CONFIG, here / "sirius-config.yaml")
        fakes = tmp / "fakes"
        fakes.mkdir()
        (fakes / "duckdb.py").write_text(FAKE_RUNNER_DUCKDB)
        (fakes / "pyarrow.py").write_text('__version__ = "25.0.1"\n')
        (fakes / "nvidia-smi").write_text(FAKE_NVIDIA_SMI)
        (fakes / "nvidia-smi").chmod(0o755)
        data = tmp / "data" / "tpch.sf40"
        data.mkdir(parents=True)
        for table in TABLES:
            (data / f"{table}.parquet").write_bytes(b"PAR1")
        out = tmp / "testdata"
        (out / "tpch-queries").mkdir(parents=True)
        for stem in files if files is not None else stems:
            (out / "tpch-queries" / f"{stem}.sql").write_text(f"/* {stem} */ SELECT 1;\n")
        tsv = out / "benchmark-results" / "h100-80gb-hbm3" / "tpch.sf40" / "sirius.tsv"
        if existing is not None:
            tsv.parent.mkdir(parents=True)
            tsv.write_text(existing)
        if old_log is not None:
            log_dir(out).mkdir(parents=True)
            (log_dir(out) / "sirius_2026-11-10.log").write_text(old_log)
        answers_db = log_dir(out).with_name("answers.duckdb")
        earlier = f"an earlier run's answers: {','.join(answers or [])}"
        if answers is not None:
            answers_db.parent.mkdir(parents=True, exist_ok=True)
            answers_db.write_text(earlier)
        (tmp / "script.json").write_text(json.dumps(script or {}))
        log = tmp / "duckdb.log"
        env = {**os.environ, "PATH": f"{fakes}:{os.environ['PATH']}", "PYTHONPATH": str(fakes),
               "FAKE_DUCKDB_SCRIPT": str(tmp / "script.json"), "FAKE_DUCKDB_LOG": str(log)}
        args = ["--dataset", "tpch", "--sf", "40", "--data", str(data), "--queries", ",".join(stems),
                "--runs", str(runs), "--timeout", str(timeout), "--out", str(out), *extra]
        proc = subprocess.run([sys.executable, str(here / "sirius-bench.py"), *args], env=env, cwd=tmp,
                              capture_output=True, text=True, timeout=300)
        calls = [json.loads(line) for line in log.read_text().splitlines()] if log.exists() else []
        logs = {p.name: p.read_text() for p in sorted(log_dir(out).glob("*"))} if log_dir(out).is_dir() else {}
        kept = answers_db.is_file() and answers_db.read_text() == earlier if answers is not None else None
        return Run(proc, tsv.read_text() if tsv.exists() else "", calls, logs, kept)


def preamble(run):
    return dict(word.split("=", 1) for word in run.tsv.splitlines()[0][2:].split(" "))


def test_pins_every_table_once_per_dataset():
    run = run_queries(["q1", "q3", "q6"])
    assert run.proc.returncode == 0, run.proc.stderr
    pins = run.executed(r"pin_table")
    assert len(pins) == len(TABLES), pins
    for table in TABLES:
        [pin] = [p for p in pins if f"name = '{table}'" in p["sql"]]
        assert re.fullmatch(rf"CALL pin_table\('[^']*/{table}\.parquet', name = '{table}', tier = 'parquet'\)",
                            pin["sql"]), pin
    sirius = [c for c in run.calls if c["event"] == "connect" and c["database"] == ":memory:"]
    assert len(sirius) == 1 and sirius[0]["config"] == {"allow_unsigned_extensions": "true"}, sirius
    setup = [c["sql"] for c in run.executed(r"^(LOAD|SET|CREATE VIEW)")]
    assert setup[0].startswith("LOAD '") and setup[0].endswith("/sirius.duckdb_extension'"), setup
    assert setup[1:4] == ["SET enable_duckdb_fallback = false",
                          "SET default_null_order='nulls_last_on_asc_first_on_desc'",
                          "SET integer_division=true"], setup
    first_pin = run.calls.index(pins[0])
    assert all(run.calls.index(c) < first_pin for c in run.executed(r"^CREATE VIEW")), "views after the pin"
    assert set(run.rows()) == {"q1", "q3", "q6"}, run.tsv


def test_times_ten_runs_and_reports_the_second_smallest():
    # The fake's clock moves by each run's scripted latency and by nothing else, so the times are
    # exact whatever the host's load.
    timed = [45, 5, 35, 15, 25, 10, 40, 20, 30, 50]
    run = run_queries(["q6"], script={"queries": {"q6": {"warmup_ms": 5, "runs_ms": timed}}})
    assert run.proc.returncode == 0, run.proc.stderr
    assert len(run.executed(r"/\* q6 \*/")) == 1 + 10
    ms, runs, status, note = run.rows()["q6"]
    assert status == "ok" and note == "-", run.rows()
    assert runs == ",".join(f"{t:.1f}" for t in timed), runs
    assert ms == "10.0", (ms, runs)


def test_the_answer_is_the_sirius_select_s_arrow_result():
    run = run_queries(["q6"], runs=3)
    assert run.proc.returncode == 0, run.proc.stderr
    [answer] = [c for c in run.calls if c["event"] == "answer"]
    assert answer == {**answer, "stem": "q6", "table": "q6#4"}, answer
    assert not run.executed(r"(?i)\bCREATE\s+(OR\s+REPLACE\s+)?TABLE\b"), "a table was created on Sirius"
    [plain] = {c["database"] for c in run.calls if c["event"] == "connect"} - {":memory:"}
    assert plain.endswith("/calibration/sirius/tpch.sf40/answers.duckdb"), plain


def test_an_error_is_a_failed_row_with_its_first_line():
    run = run_queries(["q8", "q6"], script={"queries": {"q8": {"error": "NotImplemented: INTERSECT\nat line 4"}}})
    assert run.proc.returncode == 0, run.proc.stderr
    assert run.rows()["q8"] == ("-", "-", "failed", "NotImplemented: INTERSECT"), run.rows()
    assert run.rows()["q6"][2] == "ok", run.rows()
    assert len(run.executed(r"pin_table")) == len(TABLES), "an error is not a crash: no re-pin"


def test_an_answer_write_error_after_good_runs_is_failed():
    run = run_queries(["q6"], script={"queries": {"q6": {"answer_error": "IO Error: disk full"}}})
    assert run.proc.returncode == 0, run.proc.stderr
    ms, runs, status, note = run.rows()["q6"]
    assert (ms, runs, status) == ("-", "-", "failed") and "IO Error: disk full" in note, run.rows()


def test_a_hang_is_killed_repinned_and_the_next_query_runs():
    run = run_queries(["q9", "q6"], script={"queries": {"q9": {"hang": True}}}, timeout=2)
    assert run.proc.returncode == 0, run.proc.stderr
    assert run.rows()["q9"] == ("-", "-", "failed", "timeout after 2 s"), run.rows()
    assert run.rows()["q6"][2] == "ok", run.rows()
    pins = run.executed(r"pin_table")
    assert len(pins) == 2 * len(TABLES) and len({p["pid"] for p in pins}) == 2, pins


def test_a_crash_is_repinned_and_the_next_query_runs():
    run = run_queries(["q9", "q6"], script={"queries": {"q9": {"crash": "SIGSEGV"}}})
    assert run.proc.returncode == 0, run.proc.stderr
    assert run.rows()["q9"] == ("-", "-", "failed", "crashed (SIGSEGV)"), run.rows()
    assert run.rows()["q6"][2] == "ok", run.rows()
    assert len({p["pid"] for p in run.executed(r"pin_table")}) == 2


def test_the_pin_is_outside_the_timeout():
    run = run_queries(["q1", "q6"], script={"pin_s": 3}, timeout=1)
    assert run.proc.returncode == 0, run.proc.stderr
    assert [row[2] for row in run.rows().values()] == ["ok", "ok"], run.rows()


def existing(sirius=PIN_COMMIT, host=None, rows="q1\t5.0\t5.0,6.0\tok\t-\n"):
    host = host or socket.gethostname()
    return (f"# sirius={sirius} duckdb=1.5.6 libcudf=26.08.01 cuda=13.0 driver=580.173 "
            f"device=NVIDIA_H100_80GB_HBM3 host={host}\nquery\tms_2nd_min\truns_ms\tstatus\tnote\n{rows}")


def test_resume_keeps_rows_of_this_commit_and_host_only():
    run = run_queries(["q1", "q6"], extra=["--resume"], existing=existing(), answers=["q1"])
    assert run.proc.returncode == 0, run.proc.stderr
    assert not run.executed(r"/\* q1 \*/") and run.executed(r"/\* q6 \*/"), run.calls
    assert run.rows()["q1"] == ("5.0", "5.0,6.0", "ok", "-") and run.rows()["q6"][2] == "ok", run.rows()
    for other in [existing(sirius="0" * 40), existing(host="another-host")]:
        run = run_queries(["q1", "q6"], extra=["--resume"], existing=other)
        assert run.proc.returncode == 0, run.proc.stderr
        assert run.executed(r"/\* q1 \*/"), "a row of another commit or host was kept"
        assert run.rows()["q1"] != ("5.0", "5.0,6.0", "ok", "-"), run.rows()
    # Without --resume or --list the file and the answers are this run's alone.
    run = run_queries(["q6"], existing=existing(), answers=["q1"])
    assert set(run.rows()) == {"q6"}, run.rows()
    assert run.answers_kept is False, "an unnarrowed run kept an earlier run's answers"
    # A narrowed run with --resume keeps its own query's row too.
    run = run_queries(["q1"], extra=["--resume", "--list", "q1,q6"], existing=existing(), answers=["q1"])
    assert run.proc.returncode == 0, run.proc.stderr
    assert not run.executed(r"/\* q1 \*/") and run.rows() == {"q1": ("5.0", "5.0,6.0", "ok", "-")}, run.rows()


THREE = "q1\t5.0\t5.0,6.0\tok\t-\nq3\t7.0\t7.0,8.0\twrong\t#306\nq6\t-\t-\tfailed\tboom\n"


def test_a_narrowed_run_replaces_its_own_rows_and_answers_alone():
    """`--list` names the whole list `--queries` narrows: the file's other rows stay as they are, in
    the list's order, and so do the answers database's other tables."""
    run = run_queries(["q3"], extra=["--list", "q1,q3,q6"], existing=existing(rows=THREE), answers=["q1", "q3"])
    assert run.proc.returncode == 0, run.proc.stderr
    assert list(run.rows()) == ["q1", "q3", "q6"], run.tsv
    assert run.rows()["q1"] == ("5.0", "5.0,6.0", "ok", "-") and run.rows()["q6"] == ("-", "-", "failed", "boom")
    assert run.rows()["q3"][2:] == ("ok", "-") and run.rows()["q3"][0] != "7.0", run.rows()
    assert not run.executed(r"/\* q[16] \*/"), "a query outside the narrowed run ran"
    assert run.answers_kept, "a narrowed run removed the answers of the queries it did not run"
    # The worker's, past the parent's check of which answers the database holds.
    answers = [c for c in run.calls if c["event"] == "execute" and c["database"] != ":memory:"
               and "duckdb_tables()" not in c["sql"]]
    assert [c["sql"] for c in answers][0] == 'DROP TABLE IF EXISTS "q3"', answers
    # A query the file has no row for goes where the list puts it.
    run = run_queries(["q3"], extra=["--list", "q1,q3,q6"], existing=existing(rows=THREE.replace(
        "q3\t7.0\t7.0,8.0\twrong\t#306\n", "")), answers=["q1"])
    assert run.proc.returncode == 0, run.proc.stderr
    assert list(run.rows()) == ["q1", "q3", "q6"], run.tsv


def test_a_narrowed_run_drops_its_own_answer_before_it_runs():
    """A narrowed query that fails now leaves no answer of an earlier run behind it."""
    run = run_queries(["q3"], extra=["--list", "q1,q3"], script={"queries": {"q3": {"error": "boom"}}},
                      existing=existing(), answers=["q1", "q3"])
    assert run.proc.returncode == 0, run.proc.stderr
    assert run.rows()["q3"][2] == "failed" and run.rows()["q1"][2] == "ok", run.rows()
    drop = [i for i, c in enumerate(run.calls) if c["event"] == "execute" and c["sql"] == 'DROP TABLE IF EXISTS "q3"']
    [warm_up] = [i for i, c in enumerate(run.calls) if c["event"] == "execute" and "/* q3 */" in c["sql"]]
    assert drop and drop[0] < warm_up, run.calls


def test_a_narrowed_run_over_another_commit_s_or_host_s_file_is_refused():
    for other in [existing(sirius="0" * 40, rows=THREE), existing(host="another-host", rows=THREE)]:
        run = run_queries(["q3"], extra=["--list", "q1,q3,q6"], existing=other, answers=["q1", "q3"])
        assert run.proc.returncode == 2, (run.proc.returncode, run.proc.stderr)
        assert "sirius.tsv" in run.proc.stderr and "--list" in run.proc.stderr, run.proc.stderr
        assert run.calls == [] and run.tsv == other and run.answers_kept, "a refused run changed something"
    # No file yet: nothing to keep, nothing to refuse, and the run starts with no answers database.
    run = run_queries(["q3"], extra=["--list", "q1,q3,q6"], answers=["q1", "q6"])
    assert run.proc.returncode == 0 and list(run.rows()) == ["q3"], (run.proc.stderr, run.tsv)
    assert run.answers_kept is False, "a narrowed run that kept no row kept an earlier run's answers"


def test_a_kept_timed_row_without_its_answer_is_refused():
    """A narrowed or resumed run keeps a timed row only beside its answer, else the render would write
    the answers of the queries it ran alone. An untimed row has no answer to keep."""
    for extra, answers, missing in [(["--list", "q1,q3,q6"], None, "q1"),
                                    (["--list", "q1,q3,q6"], ["q3"], "q1"),
                                    (["--resume"], ["q1"], "q3")]:
        rows = THREE if "--list" in extra else THREE.replace("\twrong\t", "\tok\t")
        run = run_queries(["q3"] if "--list" in extra else ["q1", "q3", "q6"], extra=extra,
                          existing=existing(rows=rows), answers=answers)
        assert run.proc.returncode == 2, (extra, answers, run.proc.returncode, run.proc.stderr)
        assert f"lacks {missing}" in run.proc.stderr and "pull" in run.proc.stderr, run.proc.stderr
        assert "answers.duckdb" in run.proc.stderr, run.proc.stderr
        assert run.tsv == existing(rows=rows), "a refused run changed sirius.tsv"
        assert not [c for c in run.calls if c["event"] == "load"], "a refused run loaded Sirius"
        assert answers is None or run.answers_kept, "a refused run changed the answers"


def test_queries_are_named_by_stem():
    run = run_queries(["aggregate-groupby"], runs=2)
    assert run.proc.returncode == 0, run.proc.stderr
    assert list(run.rows()) == ["aggregate-groupby"], run.tsv
    assert [c["stem"] for c in run.calls if c["event"] == "answer"] == ["aggregate-groupby"]


def test_the_file_parses_as_the_rust_fixture_does():
    """The fixture `parse_sirius_results` reads: its `# ` keys in order and its column line, and
    each row's values in the shapes the reader takes."""
    fixture = (REPO / "testdata" / "fixtures" / "sirius-results.tsv").read_text().splitlines()
    run = run_queries(["q8", "q6"], script={"queries": {"q8": {"error": "boom"}}}, runs=3)
    lines = run.tsv.splitlines()
    assert [w.split("=")[0] for w in lines[0][2:].split(" ")] == [w.split("=")[0] for w in fixture[0][2:].split(" ")]
    assert lines[1] == fixture[1]
    assert preamble(run) == {"sirius": PIN_COMMIT, "duckdb": "1.5.6", "libcudf": "26.08.01", "cuda": "13.0",
                             "driver": "580.173", "device": "NVIDIA_H100_80GB_HBM3", "host": socket.gethostname()}
    for line in lines[2:]:
        query, ms, runs, status, note = line.split("\t")
        if status == "ok":
            assert re.fullmatch(r"\d+\.\d", ms) and re.fullmatch(r"\d+\.\d(,\d+\.\d)*", runs), line
        else:
            assert (ms, runs, status) == ("-", "-", "failed") and note, line


# --- the GPU evidence --------------------------------------------------------------------------

def not_on_gpu(run, stem, *shown):
    """The row is `failed: not run on the GPU (…)` naming what the log showed, and the query ran only
    its warm-up: no timed run, no answer."""
    ms, runs, status, note = run.rows()[stem]
    assert (ms, runs, status) == ("-", "-", "failed"), run.rows()
    assert note.startswith("not run on the GPU (") and note.endswith(")"), note
    assert all(word in note for word in shown), (shown, note)
    assert "\t" not in note and "\n" not in note and len(note) <= 160, note
    assert len(run.executed(rf"/\* {stem} \*/")) == 1, "timed runs after a warm-up without evidence"
    assert stem not in [c["stem"] for c in run.calls if c["event"] == "answer"]


def test_sirius_s_log_is_on_at_info_beside_the_answers():
    run = run_queries(["q6"], runs=2)
    assert run.proc.returncode == 0, run.proc.stderr
    [load] = [c for c in run.calls if c["event"] == "load"]
    assert load["log"]["SIRIUS_LOG_BACKEND"] == "spdlog" and load["log"]["SIRIUS_LOG_LEVEL"] == "info", load
    assert load["log"]["SIRIUS_LOG_DIR"].endswith("/testdata/calibration/sirius/tpch.sf40/log"), load
    assert list(run.logs) == ["sirius_2026-11-10.log"] and "GPU_SCAN" in run.logs["sirius_2026-11-10.log"]
    # info is Sirius's default level, so the timed runs log as the measured run did: nothing switches it.
    assert not run.executed(r"sirius_log_level"), run.executed(r"sirius_log_level")


def test_a_query_with_gpu_evidence_is_ok_and_its_warm_up_is_labelled():
    run = run_queries(["q6"], runs=2)
    assert run.proc.returncode == 0, run.proc.stderr
    assert run.rows()["q6"][2:] == ("ok", "-"), run.rows()
    sirius = [c["sql"] for c in run.calls if c["event"] == "execute" and c["database"] == ":memory:"
              and re.search(r"sirius_set_query_label|/\* q6 \*/", c["sql"])]
    assert sirius[:3] == ["CALL sirius_set_query_label('q6')", "/* q6 */ SELECT 1;\n",
                          "CALL sirius_set_query_label('q6 timed')"], sirius


def test_a_query_without_gpu_evidence_is_not_run_on_the_gpu():
    for log, shown in [("cpu", "no Transparent GPU execution"), ("no_scan", "no GPU_SCAN")]:
        run = run_queries(["q6", "q1"], script={"queries": {"q6": {"log": log}}})
        assert run.proc.returncode == 0, run.proc.stderr
        not_on_gpu(run, "q6", shown)
        assert run.rows()["q1"][2] == "ok", run.rows()


def test_a_fallback_line_is_not_run_on_the_gpu():
    for log, shown in [("fallback", "falling back to DuckDB CPU"),
                       ("plan_fallback", "Transparent execution fallback (unsupported)")]:
        run = run_queries(["q6"], script={"queries": {"q6": {"log": log}}})
        assert run.proc.returncode == 0, run.proc.stderr
        not_on_gpu(run, "q6", shown)


def test_each_query_is_judged_by_its_own_warm_up():
    """Sirius flushes its log every few seconds, so the lines of one query's timed runs can land
    after the next query starts: they must not count for it."""
    run = run_queries(["q1", "q6", "q3"], script={"log_flush_s": 1, "queries": {"q6": {"log": "cpu"}}}, runs=2)
    assert run.proc.returncode == 0, run.proc.stderr
    assert run.rows()["q1"][2] == "ok" and run.rows()["q3"][2] == "ok", run.rows()
    not_on_gpu(run, "q6", "no Transparent GPU execution")


OLD_Q6_LOG = "".join(f"[2026-11-09 23:00:00.000] [info] [fake.cpp:1] {line}\n" for line in [
    "QueryBegin: instance=0x1 connection=1 query=9 SQL: CALL sirius_set_query_label('q6')",
    "Transparent GPU execution: executing query", "Query Plan:\nPipeline #0\n  GPU_SCAN (id=0)",
    "Transparent GPU execution: query completed",
    "QueryBegin: instance=0x1 connection=1 query=10 SQL: CALL sirius_set_query_label('q6 timed')"])


def test_an_earlier_run_s_log_is_kept_and_is_not_evidence():
    run = run_queries(["q6"], script={"queries": {"q6": {"log": "cpu"}}}, old_log=OLD_Q6_LOG)
    assert run.proc.returncode == 0, run.proc.stderr
    not_on_gpu(run, "q6", "no Transparent GPU execution")
    assert run.logs["sirius_2026-11-10.log"].startswith(OLD_Q6_LOG), "the earlier log was not kept"


def test_a_log_rotated_at_midnight_is_read_across_both_files():
    run = run_queries(["q1", "q6"], script={"queries": {"q6": {"rotate": "2026-11-11"}}}, runs=2)
    assert run.proc.returncode == 0, run.proc.stderr
    assert list(run.logs) == ["sirius_2026-11-10.log", "sirius_2026-11-11.log"], list(run.logs)
    assert "GPU_SCAN" in run.logs["sirius_2026-11-11.log"]
    assert [row[2] for row in run.rows().values()] == ["ok", "ok"], run.rows()


def test_after_a_crash_the_new_worker_s_log_judges_the_next_query():
    run = run_queries(["q9", "q6", "q1"], script={"queries": {"q9": {"crash": "SIGSEGV"}, "q6": {"log": "cpu"}}})
    assert run.proc.returncode == 0, run.proc.stderr
    assert run.rows()["q9"] == ("-", "-", "failed", "crashed (SIGSEGV)"), run.rows()
    not_on_gpu(run, "q6", "no Transparent GPU execution")
    assert run.rows()["q1"][2] == "ok", run.rows()


def test_a_warm_up_the_log_never_shows_is_not_run_on_the_gpu():
    run = run_queries(["q6"], script={"log_flush_s": 3600})
    assert run.proc.returncode == 0, run.proc.stderr
    not_on_gpu(run, "q6", "no log of its warm-up")


def test_bad_input_is_refused_before_any_query():
    for extra, stems, files in [
        (["--runs", "1"], ["q6"], None), (["--timeout", "0"], ["q6"], None),
        ([], ["q99"], ["q6"]), ([], ["aggregate_groupby"], ["aggregate-groupby"]), ([], [""], []),
        (["--list", "q1,q3"], ["q6"], None), (["--list", "q1,q6,q1"], ["q6"], None),
        (["--list", "q1,aggregate_groupby"], ["q1"], None)]:
        run = run_queries(stems, extra=extra, files=files)
        assert run.proc.returncode == 2, (extra, stems, run.proc.returncode, run.proc.stderr)
        assert run.calls == [] and run.tsv == "", (extra, stems)
        assert run.proc.stderr.strip(), (extra, stems)


if __name__ == "__main__":
    raise SystemExit(main(globals()))
