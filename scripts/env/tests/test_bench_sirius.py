"""`bench-sirius.sh`: Sirius on the H100 by flags, over a fake run-sirius.sh, run-gpu.sh and
build-test.sh.

The fakes record their argv. run-sirius.sh and run-gpu.sh exit as FAKE_RC maps `<script> <verb>`
(0 otherwise), so a VM lost mid-run, or the runner's own refusal, reaches bench-sirius.sh as the real
one's would.
build-test.sh stands for the build image the render runs in, with DuckDB 1.5.4.
"""
import csv
import json
import os
import pathlib
import shlex
import subprocess
import sys
import tempfile

sys.path.append(str(pathlib.Path(__file__).resolve().parents[2] / "exec_model" / "tests"))
from harness import main  # noqa: E402

REPO = pathlib.Path(__file__).resolve().parents[3]
SCRIPT = REPO / "scripts" / "env" / "bench-sirius.sh"
H100 = ["--instance-id", "computeinstance-e00e6yena4nzjgaym0", "--profile", "editor-eu"]
# A device list: every status, a list ident with `_`, a note holding a comma, and another scale.
CASES = """\
dataset,sf,query,modes,status,note
tpch,40,q1,tp1_single|tp4_sized,ok,
tpch,40,q6,tp1_single,ok,
tpch,40,aggregate_groupby,tp1_single,ok,
tpch,40,anti_join,,skipped,tpch/anti-join: GpuUnload
tpcds,40,q18,tp1_single,wrong,#306
tpcds,40,q17,,skipped,"the accumulators hold 2593119555 bytes, the whole budget"
tpch,1,q3,tp1_single,ok,
"""
TPCH = "q1,q6,aggregate-groupby,anti-join"
TPCDS = "q18,q17"

FAKE = r'''#!/usr/bin/env python3
import json, os, sys
with open(os.environ["FAKE_LOG"], "a") as f:
    f.write(json.dumps({"tool": os.path.basename(sys.argv[0]), "argv": sys.argv[1:], "cwd": os.getcwd()}) + "\n")
args = sys.argv[1:]
while args and args[0].startswith("--") and args[0] != "--":
    args = args[2:]
verb = args[0] if args else ""
sys.exit(json.loads(os.environ.get("FAKE_RC", "{}")).get(f"{os.path.basename(sys.argv[0])} {verb}", 0))
'''


def bench(*args, rc=None, cases=CASES):
    """bench-sirius.sh <args> with --cases a temp list of `cases`, none with None: (proc, calls)."""
    with tempfile.TemporaryDirectory() as tmp:
        tmp = pathlib.Path(tmp)
        for name in ("run-sirius.sh", "run-gpu.sh", "build-test.sh"):
            (tmp / name).write_text(FAKE)
            (tmp / name).chmod(0o755)
        if cases is not None:
            (tmp / "cases.csv").write_text(cases)
            args = (*args, "--cases", str(tmp / "cases.csv"))
        env = {**os.environ, "RUN_SIRIUS_SH": str(tmp / "run-sirius.sh"), "RUN_GPU_SH": str(tmp / "run-gpu.sh"),
               "BUILD_TEST_SH": str(tmp / "build-test.sh"), "FAKE_LOG": str(tmp / "log"), "FAKE_RC": json.dumps(rc or {})}
        proc = subprocess.run([str(SCRIPT), *args], env=env, capture_output=True, text=True, timeout=60,
                              stdin=subprocess.DEVNULL)
        log = tmp / "log"
        calls = [json.loads(line) for line in log.read_text().splitlines()] if log.exists() else []
        return proc, calls


def sirius(dataset, queries, runs="10", timeout="900", resume=(), within=None):
    """The runner's call; `within`, the dataset's whole list, for a run --query narrows."""
    narrowed = ["--list", within] if within is not None else []
    return [*H100, "run", "--", "--dataset", dataset, "--sf", "40", "--queries", queries, "--runs", runs,
            "--timeout", timeout, *narrowed, *resume]


def render(dataset):
    return ["--", "python3", "testdata/duckdb_result.py", "--dataset", dataset, "--sf", "40", "--answers",
            f"testdata/calibration/sirius/{dataset}.sf40/answers.duckdb", "--out",
            f"testdata/goldens/{dataset}.sf40/sirius-result.txt"]


def steps(calls):
    return [(c["tool"], c["argv"]) for c in calls]


def test_a_dataset_runs_then_pulls_then_renders_at_home():
    proc, calls = bench(*H100, "--dataset", "tpch")
    assert proc.returncode == 0, proc.stderr
    assert steps(calls) == [("run-sirius.sh", sirius("tpch", TPCH)), ("run-gpu.sh", [*H100, "pull"]),
                            ("build-test.sh", render("tpch"))], steps(calls)
    assert calls[-1]["cwd"] == str(REPO), calls[-1]


def test_each_flag_reaches_the_runner():
    proc, calls = bench(*H100, "--dataset", "tpcds", "--runs", "7", "--timeout", "300", "--resume")
    assert proc.returncode == 0, proc.stderr
    assert calls[0]["argv"] == sirius("tpcds", TPCDS, runs="7", timeout="300", resume=["--resume"]), calls[0]


def test_query_narrows_the_list_by_ident_or_stem():
    for given in ("q6", "aggregate_groupby", "aggregate-groupby"):
        proc, calls = bench(*H100, "--dataset", "tpch", "--query", given)
        assert proc.returncode == 0, (given, proc.stderr)
        assert calls[0]["argv"] == sirius("tpch", given.replace("_", "-"), within=TPCH), (given, calls[0])
    proc, calls = bench(*H100, "--dataset", "tpch", "--query", "q6", "--query", "q1", "--resume")
    assert calls[0]["argv"] == sirius("tpch", "q1,q6", within=TPCH, resume=["--resume"]), "the list's order"


def test_a_narrowed_run_names_the_whole_list_so_the_rest_of_the_file_stays():
    """The runner keeps the rows and answers of the list's other queries, and the render at home
    then writes every query's answer: pulling a one-query run leaves the dataset whole."""
    proc, calls = bench(*H100, "--dataset", "tpch", "--query", "q6")
    assert proc.returncode == 0, proc.stderr
    assert steps(calls) == [("run-sirius.sh", sirius("tpch", "q6", within=TPCH)), ("run-gpu.sh", [*H100, "pull"]),
                            ("build-test.sh", render("tpch"))], steps(calls)
    proc, calls = bench(*H100, "--dataset", "tpch")
    assert "--list" not in calls[0]["argv"], "an unnarrowed run is the whole list already"


def test_all_runs_tpch_then_tpcds_each_to_its_render():
    proc, calls = bench(*H100, "--dataset", "all")
    assert proc.returncode == 0, proc.stderr
    assert steps(calls) == [("run-sirius.sh", sirius("tpch", TPCH)), ("run-gpu.sh", [*H100, "pull"]),
                            ("build-test.sh", render("tpch")), ("run-sirius.sh", sirius("tpcds", TPCDS)),
                            ("run-gpu.sh", [*H100, "pull"]), ("build-test.sh", render("tpcds"))], steps(calls)


def test_a_query_narrowed_to_one_dataset_leaves_the_other_out():
    proc, calls = bench(*H100, "--dataset", "all", "--query", "q17")
    assert proc.returncode == 0, proc.stderr
    assert [c["argv"] for c in calls if c["tool"] == "run-sirius.sh"] == [sirius("tpcds", "q17", within=TPCDS)], calls


def test_a_skipped_list_row_runs_like_any_other():
    proc, calls = bench(*H100, "--dataset", "tpch", "--query", "anti_join")
    assert proc.returncode == 0, proc.stderr
    assert calls[0]["argv"] == sirius("tpch", "anti-join", within=TPCH), calls[0]


def test_bad_input_is_refused_before_any_call():
    cases = [((), "--instance-id"), ((*H100, "--dataset", "pbench"), "pbench"),
             ((*H100, "--dataset", "tpch", "--query", "q99"), "q99"), ((*H100, "--dataset", "tpch", "--runs", "0"), "--runs"),
             ((*H100, "--dataset", "tpch", "--runs", "1"), "--runs"), ((*H100, "--dataset", "tpch", "--timeout", "0"), "--timeout"),
             ((*H100, "--dataset", "tpch", "--sf", "x"), "--sf"), ((*H100, "--dataset", "tpch", "--sf", "100"), "sf100"),
             ((*H100,), "--dataset"), ((*H100, "--dataset", "tpch", "--cases", "/no/such.csv"), "/no/such.csv"),
             ((*H100, "--dataset", "tpch", "--colour"), "--colour"),
             ((*H100, "--dataset", "tpch", "--dataset", "tpcds"), "--dataset")]
    for args, word in cases:
        proc, calls = bench(*args, cases=None if "--cases" in args else CASES)
        assert proc.returncode == 2, (args, proc.returncode, proc.stderr)
        assert word in proc.stderr and calls == [], (args, proc.stderr, calls)


def test_a_dry_run_prints_every_command_and_calls_nothing():
    proc, calls = bench(*H100, "--dataset", "all", "--dry-run")
    assert proc.returncode == 0 and calls == [], (proc.stderr, calls)
    lines = [shlex.split(line) for line in proc.stdout.splitlines()]
    assert len(lines) == 6, lines
    assert lines[0][1:] == sirius("tpch", TPCH) and lines[3][1:] == sirius("tpcds", TPCDS), lines
    assert lines[1][1:] == [*H100, "pull"] and lines[2][1:] == render("tpch"), lines


def test_the_vm_scripts_exits_pass_through_and_stop_the_run():
    for step, code in [("run-sirius.sh run", 3), ("run-sirius.sh run", 4), ("run-sirius.sh run", 75),
                       ("run-sirius.sh run", 69), ("run-sirius.sh run", 1), ("run-gpu.sh pull", 75)]:
        proc, calls = bench(*H100, "--dataset", "all", rc={step: code})
        assert proc.returncode == code, (step, code, proc.returncode, proc.stderr)
        assert not [c for c in calls if c["tool"] == "build-test.sh"], (step, code, calls)
        assert len([c for c in calls if c["tool"] == "run-sirius.sh"]) == 1, (step, code, calls)


def test_the_default_list_is_the_h100_s():
    """Without --cases, the committed H100 list: tpcds q17 is a `skipped` row there."""
    proc, calls = bench(*H100, "--dataset", "tpcds", "--query", "q17", "--dry-run", cases=None)
    assert proc.returncode == 0 and calls == [], proc.stderr
    with open(REPO / "testdata" / "benchmark-results" / "h100-80gb-hbm3" / "cases.csv", newline="") as f:
        listed = [row["query"].replace("_", "-") for row in csv.DictReader(f) if (row["dataset"], row["sf"]) == ("tpcds", "40")]
    assert shlex.split(proc.stdout.splitlines()[0])[1:] == sirius("tpcds", "q17", within=",".join(listed)), proc.stdout


def test_help():
    proc, _ = bench("--help")
    assert proc.returncode == 0 and "bench-sirius.sh" in proc.stdout and "--resume" in proc.stdout, proc


if __name__ == "__main__":
    raise SystemExit(main(globals()))
