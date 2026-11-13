"""`bench-each.sh`: the H100 benchmark one process per query, over a fake run-gpu.sh and nebius.sh.

The fake run-gpu.sh plays the VM's `run bench` and `pull`: a case writes its section into the
VM's tree and its rows into the VM's record, as the harness does, and `pull` copies both into the
tree on dev. Per query a behaviour: `ok`, `timeout` (a failed section, no rows), `kill` (rows and
no section, every time) or `kill-once` (that, then `ok`). Like run-tests.sh, `run bench` refuses a
VM record that holds rows, and like the harness under resume it keeps a section on the VM whose
trailer is this run's, running nothing. The fake nebius.sh runs `ssh` commands in the VM's home,
where `hostname` is the trailer's host.
"""
import json
import os
import pathlib
import shutil
import subprocess
import sys
import tempfile

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
sys.path.append(str(pathlib.Path(__file__).resolve().parents[2] / "exec_model" / "tests"))
from harness import main  # noqa: E402
from machine import REPO, git  # noqa: E402

H100 = ["--instance-id", "computeinstance-e00e6yena4nzjgaym0", "--profile", "editor-eu"]
SLUG = "h100-80gb-hbm3"
CASES = """\
// a comment naming corpus_query_benchmark!(tpch, 40, q99, tp1_single); is no case
corpus_query_benchmark!(tpch, 40, q1, tp1_single | tp4_sized);
corpus_query_benchmark!(tpch, 40, q6, tp1_single);
corpus_query_benchmark!(tpch, 40, q2, none);
corpus_query_benchmark!(tpch, 40, aggregate_groupby, tp1_single);
corpus_query_benchmark!(tpcds, 40, q37, tp1_single);
"""
ALL_TP1 = ["bench_tpch_sf40_q1_tp1_single", "bench_tpch_sf40_q6_tp1_single",
           "bench_tpch_sf40_aggregate_groupby_tp1_single", "bench_tpcds_sf40_q37_tp1_single"]
COLUMNS = "dataset\tsf\tquery\tmode\tnode_seq\tdevice_us\n"
HEADING = "# peacockdb cost-model calibration record.\n# run: capture=none\n# run: device=NVIDIA H100 80GB HBM3\n"
VM_HOST = "h100-node"
TRAILER = f"device=NVIDIA H100 80GB HBM3\ndriver=580.95.05\ncuda=12.9\ncudf=25.02.02\nhost={VM_HOST}\n"

FAKE_RUN_GPU = r'''#!/usr/bin/env python3
import json, os, pathlib, re, shutil, sys
a = sys.argv[1:]
with open(os.environ["FAKE_RUN_GPU_LOG"], "a") as f:
    env = {k: os.environ[k] for k in ("PCK_TEST_FILTER", "PEACOCK_BENCH_RESUME", "PEACOCK_BENCH_CASE_TIMEOUT_S",
                                      "PCK_WRITE_GPU_RESULT") if k in os.environ}
    f.write(json.dumps({"argv": a, "env": env}) + "\n")
i, d = 0, pathlib.Path(os.environ["FAKE_TREE"]).name
while a[i].startswith("--"):
    if a[i] == "--dir":
        d = a[i + 1]
    i += 2
verb = a[i]
vm = pathlib.Path(os.environ["FAKE_HOST_HOME"]) / "peacock" / d / "testdata"
record = vm / "calibration" / "records.tsv"
if verb == "pull":
    tree = pathlib.Path(os.environ["FAKE_TREE"]) / "testdata"
    for sub in ("benchmark-results", "calibration"):
        if (vm / sub).is_dir():
            shutil.copytree(vm / sub, tree / sub, dirs_exist_ok=True)
    sys.exit(0)
assert a[i:] == ["run", "bench"], a
if record.exists() and record.stat().st_size:
    print("run-tests.sh: the record holds an earlier run's rows", file=sys.stderr)
    sys.exit(2)
ds, sf, query, mode = re.fullmatch(r"bench_(tpch|tpcds)_sf(\d+)_(.+)_(tp1_single|tp4_sized)",
                                   os.environ["PCK_TEST_FILTER"]).groups()
state = pathlib.Path(os.environ["FAKE_STATE"])
seen = json.loads(state.read_text()) if state.exists() else {}
seen[query] = seen.get(query, 0) + 1
state.write_text(json.dumps(seen))
behaviour = json.loads(os.environ.get("FAKE_BEHAVIOUR", "{}")).get(query, "ok")
if behaviour == "kill-once":
    behaviour = "kill" if seen[query] == 1 else "ok"
if behaviour == "exit75":
    sys.exit(75)
tree = vm / "benchmark-results" / "h100-80gb-hbm3" / f"{ds}.sf{sf}" / f"{mode}.benchmark.txt"
tree.parent.mkdir(parents=True, exist_ok=True)
trailer = os.environ["FAKE_TRAILER"]
heading = f"== {query.replace('_', '-')}\n"
if os.environ.get("PEACOCK_BENCH_RESUME") == "1" and tree.exists():
    text = tree.read_text()
    at = text.find(heading)
    if at >= 0 and (text[at:].split("\n== ", 1)[0].rstrip("\n") + "\n").endswith(trailer):
        print(f"kept {query}")
        sys.exit(0)
if behaviour in ("ok", "kill"):
    record.parent.mkdir(parents=True, exist_ok=True)
    fresh = not record.exists() or not record.stat().st_size
    with open(record, "a") as f:
        if fresh:
            f.write(os.environ["FAKE_HEADING"] + os.environ["FAKE_COLUMNS"])
        for seq in range(2):
            f.write(f"{ds}\t{sf}\t{query.replace('_', '-')}\t{mode.replace('_', '-')}\t{seq}\t{seen[query]}\n")
if behaviour == "ok":
    with open(tree, "a") as f:
        f.write(f"== {query.replace('_', '-')}\nGpuScan\n  time_us=[[1]] total_us=1\n--- run ---\nrun_us=1\n{trailer}")
    sys.exit(0)
if behaviour == "timeout":
    with open(tree, "a") as f:
        f.write(f"== {query.replace('_', '-')}\nstatus=failed reason=timeout: the case ran past "
                f"{os.environ['PEACOCK_BENCH_CASE_TIMEOUT_S']} s\n{trailer}")
sys.exit(1)
'''

FAKE_NEBIUS = r'''#!/usr/bin/env python3
import json, os, subprocess, sys
a = sys.argv[1:]
with open(os.environ["FAKE_NEBIUS_LOG"], "a") as f:
    f.write(json.dumps(a) + "\n")
i = 0
while i < len(a) and a[i].startswith("--"):
    if a[i] not in ("--instance-id", "--profile"):
        print(f"nebius.sh: unknown option {a[i]}", file=sys.stderr)
        sys.exit(64)
    i += 2
assert a[i] == "ssh", a
env = {**os.environ, "HOME": os.environ["FAKE_HOST_HOME"],
       "PATH": os.environ["FAKE_VM_BIN"] + ":" + os.environ["PATH"]}
sys.exit(subprocess.run(["bash", "-c", " ".join(a[i + 1:])], env=env, cwd=env["HOME"]).returncode)
'''


class Run:
    """A tree on dev holding bench-each.sh, the fakes, and the VM's home."""

    def __enter__(self):
        self.tmp = tempfile.TemporaryDirectory()
        tmp = pathlib.Path(self.tmp.name)
        self.tree = tmp / "chain-l"
        for rel in ("scripts/env/bench-each.sh", "scripts/lib/bench-cases.sh", "scripts/calibration/device_cases.py"):
            (self.tree / rel).parent.mkdir(parents=True, exist_ok=True)
            if (REPO / rel).exists():
                shutil.copy2(REPO / rel, self.tree / rel)
        self.cases = self.tree / "cases.inc"
        self.cases.write_text(CASES)
        # An H200 tree holding q1, as the committed one does: never this run's device.
        h200 = self.tree / "testdata/benchmark-results/h200/tpch.sf40/tp1_single.benchmark.txt"
        h200.parent.mkdir(parents=True)
        h200.write_text("== q1\nGpuScan\n  time_us=[[1]] total_us=1\n--- run ---\ndevice=NVIDIA H200\n")
        git(self.tree, "init", "-q", "-b", "master")
        self.home = tmp / "vm-home"
        self.vm = self.home / "peacock" / "chain-l" / "testdata"
        (self.vm / "benchmark-results/h200/tpch.sf40").mkdir(parents=True)
        shutil.copy2(h200, self.vm / "benchmark-results/h200/tpch.sf40/tp1_single.benchmark.txt")
        self.records = tmp / "records"
        vm_bin = tmp / "vm-bin"
        vm_bin.mkdir()
        (vm_bin / "hostname").write_text(f"#!/bin/sh\necho {VM_HOST}\n")
        (vm_bin / "hostname").chmod(0o755)
        for name, text in (("run-gpu.sh", FAKE_RUN_GPU), ("nebius.sh", FAKE_NEBIUS)):
            (tmp / name).write_text(text)
            (tmp / name).chmod(0o755)
        self.run_log, self.nebius_log = tmp / "run-gpu.log", tmp / "nebius.log"
        self.env = {**os.environ, "RUN_GPU_SH": str(tmp / "run-gpu.sh"), "NEBIUS_SH": str(tmp / "nebius.sh"),
                    "FAKE_RUN_GPU_LOG": str(self.run_log), "FAKE_NEBIUS_LOG": str(self.nebius_log),
                    "FAKE_TREE": str(self.tree), "FAKE_HOST_HOME": str(self.home), "FAKE_STATE": str(tmp / "state"),
                    "FAKE_TRAILER": TRAILER, "FAKE_HEADING": HEADING, "FAKE_COLUMNS": COLUMNS,
                    "FAKE_VM_BIN": str(vm_bin)}
        for name in ("PCK_TEST_FILTER", "PEACOCK_BENCH_RESUME", "PEACOCK_BENCH_CASE_TIMEOUT_S", "PCK_WRITE_GPU_RESULT"):
            self.env.pop(name, None)
        return self

    def __exit__(self, *exc):
        self.tmp.cleanup()

    def each(self, *args, mode="tp1_single", behaviour=None, timeout="1800", host=H100):
        cmd = ["bash", str(self.tree / "scripts/env/bench-each.sh"), *host, "--cases", str(self.cases),
               "--mode", mode, "--timeout", timeout, "--records", str(self.records), *map(str, args)]
        env = {**self.env, "FAKE_BEHAVIOUR": json.dumps(behaviour or {})}
        return subprocess.run(cmd, env=env, cwd=self.tree, capture_output=True, text=True, timeout=120)

    def calls(self):
        if not self.run_log.exists():
            return []
        return [json.loads(line) for line in self.run_log.read_text().splitlines()]

    def runs(self):
        return [c for c in self.calls() if c["argv"][-2:] == ["run", "bench"]]

    def filters(self):
        return [c["env"]["PCK_TEST_FILTER"] for c in self.runs()]

    def section(self, ds, query, mode="tp1_single"):
        path = self.tree / f"testdata/benchmark-results/{SLUG}/{ds}.sf40/{mode}.benchmark.txt"
        text = path.read_text() if path.exists() else ""
        return f"== {query}\n" in text

    def host_record(self):
        return self.vm / "calibration/records.tsv"


def ok(r):
    assert r.returncode == 0, (r.returncode, r.stdout, r.stderr)
    return r


def test_bench_each_runs_one_process_per_query():
    with Run() as h:
        ok(h.each())
        assert h.filters() == ALL_TP1, h.filters()
        verbs = [c["argv"][len(H100):] for c in h.calls()]
        assert verbs == [["run", "bench"], ["pull"]] * 4, verbs
        for ds, q in (("tpch", "q1"), ("tpch", "q6"), ("tpch", "aggregate-groupby"), ("tpcds", "q37")):
            assert h.section(ds, q), (ds, q)


def test_bench_each_resumes_and_sets_the_timeout():
    with Run() as h:
        ok(h.each(timeout="900"))
        for call in h.runs():
            assert call["argv"][:4] == H100, call
            env = call["env"]
            assert env["PEACOCK_BENCH_RESUME"] == "1" and env["PEACOCK_BENCH_CASE_TIMEOUT_S"] == "900", env
            assert env["PCK_WRITE_GPU_RESULT"] == "1", env
        for call in [json.loads(line) for line in h.nebius_log.read_text().splitlines()]:
            assert call[:4] == H100 and "--dir" not in call, call


def test_bench_each_passes_dir_to_run_gpu_alone():
    with Run() as h:
        (h.home / "peacock" / "other" / "testdata").mkdir(parents=True)
        ok(h.each("--dir", "other"))
        for call in h.calls():
            assert call["argv"][4:6] == ["--dir", "other"], call
        for call in [json.loads(line) for line in h.nebius_log.read_text().splitlines()]:
            assert "--dir" not in call and "peacock/other/" in " ".join(call), call


def test_bench_each_skips_a_failed_case_on_rerun():
    with Run() as h:
        ok(h.each(behaviour={"q6": "timeout"}))
        path = h.tree / f"testdata/benchmark-results/{SLUG}/tpch.sf40/tp1_single.benchmark.txt"
        assert "== q6\nstatus=failed reason=timeout: the case ran past 1800 s\n" in path.read_text()
        before = len(h.runs())
        r = ok(h.each(behaviour={"q6": "timeout"}))
        assert len(h.runs()) == before, h.filters()
        assert "q6" in r.stdout and "kept" in r.stdout, r.stdout


def test_bench_each_learns_the_device_from_a_section_the_run_kept():
    """A fresh --records over a device tree this VM measured: `run bench` keeps the case's section, so
    no pull changes the tree, and the device is the tree whose section names the VM's host."""
    with Run() as h:
        ok(h.each())
        path = h.tree / f"testdata/benchmark-results/{SLUG}/tpch.sf40/tp1_single.benchmark.txt"
        text = path.read_text()
        # Another device's tree holding q1 too, from another host: never this run's device.
        other = h.tree / "testdata/benchmark-results/l40s/tpch.sf40/tp1_single.benchmark.txt"
        other.parent.mkdir(parents=True)
        other.write_text(f"== q1\nGpuScan\n--- run ---\n{TRAILER.replace('H100 80GB HBM3', 'L40S')}"
                         .replace(f"host={VM_HOST}", "host=l40s-node"))
        h.records = h.records.with_name("records-2")
        measured = len(h.runs())
        r = ok(h.each())
        assert (h.records / "device").read_text() == f"{SLUG}\n", r.stdout
        assert h.filters()[measured:] == ALL_TP1[:1], "one run learns the device; the tree keeps the rest"
        assert path.read_text() == text
        assert list(h.records.glob("*.tsv")) == [], "a kept case writes no record"


def test_bench_each_moves_each_record_aside():
    with Run() as h:
        ok(h.each())
        first = h.records / "tpch-q1-tp1_single.1.tsv"
        text = first.read_text()
        assert text.startswith(HEADING) and "\ttpch\t40\tq1\t" not in text, text
        assert "tpch\t40\tq1\ttp1-single\t0\t1\n" in text, text
        assert sorted(p.name for p in h.records.glob("*.tsv")) == [
            "tpcds-q37-tp1_single.1.tsv", "tpch-aggregate_groupby-tp1_single.1.tsv",
            "tpch-q1-tp1_single.1.tsv", "tpch-q6-tp1_single.1.tsv"]
        assert not h.host_record().exists()
        assert not (h.tree / "testdata/calibration/records.tsv").exists()
        # q1 measured again: its section dropped on both sides.
        for root in (h.tree / "testdata", h.vm):
            path = root / f"benchmark-results/{SLUG}/tpch.sf40/tp1_single.benchmark.txt"
            path.write_text(path.read_text().split("== q6\n", 1)[1].join(["== q6\n", ""]))
        ok(h.each())
        assert h.filters()[-1] == "bench_tpch_sf40_q1_tp1_single", h.filters()
        assert first.read_text() == text
        assert "tpch\t40\tq1\ttp1-single\t0\t2\n" in (h.records / "tpch-q1-tp1_single.2.tsv").read_text()


def test_bench_each_sets_aside_a_record_a_killed_run_left():
    with Run() as h:
        h.host_record().parent.mkdir(parents=True)
        h.host_record().write_text(HEADING + COLUMNS + "tpch\t40\tq6\ttp1-single\t0\t0\n")
        ok(h.each())
        left = h.records / "tpch-q6-tp1_single.1.tsv"
        assert left.read_text().endswith("tpch\t40\tq6\ttp1-single\t0\t0\n"), left.read_text()
        assert "tpch\t40\tq6\ttp1-single\t0\t1\n" in (h.records / "tpch-q6-tp1_single.2.tsv").read_text()
        assert h.filters() == ALL_TP1, h.filters()


def test_bench_each_measures_a_killed_case_again():
    with Run() as h:
        r = h.each(behaviour={"aggregate_groupby": "kill-once"})
        assert r.returncode == 1 and "aggregate_groupby" in r.stderr, (r.stdout, r.stderr)
        assert (h.records / "tpch-aggregate_groupby-tp1_single.1.tsv").exists()
        ok(h.each(behaviour={"aggregate_groupby": "kill-once"}))
        assert h.filters()[-1] == "bench_tpch_sf40_aggregate_groupby_tp1_single", h.filters()
        assert (h.records / "tpch-aggregate_groupby-tp1_single.2.tsv").exists()
        assert h.section("tpch", "aggregate-groupby")


def test_bench_each_takes_the_mode_s_lines_only():
    with Run() as h:
        ok(h.each(mode="tp4_sized"))
        assert h.filters() == ["bench_tpch_sf40_q1_tp4_sized"], h.filters()
        assert h.section("tpch", "q1", mode="tp4_sized")


def test_bench_each_within_narrows_the_queries():
    with Run() as h:
        within = h.tree / "h100-cases.csv"
        within.write_text("dataset,sf,query,modes,status,note\n"
                          "tpch,40,q1,tp1_single,wrong,#7\n"
                          "tpch,40,q6,,skipped,timeout: the case ran past 1800 s\n"
                          "tpch,40,aggregate_groupby,tp1_single,ok,\n")
        ok(h.each("--within", within))
        assert h.filters() == ["bench_tpch_sf40_q1_tp1_single", "bench_tpch_sf40_aggregate_groupby_tp1_single"]


def test_bench_each_stops_on_a_host_it_cannot_reach():
    with Run() as h:
        r = h.each(behaviour={"q6": "exit75"})
        assert r.returncode == 75, (r.returncode, r.stdout, r.stderr)
        assert h.filters() == ALL_TP1[:2], h.filters()


def test_bench_each_refuses_before_any_call():
    with Run() as h:
        for args, kwargs in ((["--within", h.tree / "missing.csv"], {}), ([], {"mode": "tp9"}),
                             ([], {"timeout": "0"}), ([], {"timeout": "30m"}), (["--color", "red"], {})):
            r = h.each(*args, **kwargs)
            assert r.returncode == 2, (args, kwargs, r.returncode, r.stderr)
        empty = h.tree / "empty.csv"
        empty.write_text("dataset,sf,query,modes,status,note\ntpch,40,q6,,skipped,no\n")
        r = h.each("--within", empty)
        assert r.returncode == 2 and "no case" in r.stderr, r.stderr
        assert h.calls() == [] and not h.nebius_log.exists()


def test_bench_each_refuses_a_run_without_an_instance_id_before_any_call():
    """Without --instance-id, nebius.sh means nebius-gpu, which runs no sf40, and run-gpu.sh
    refuses; bench-each.sh refuses first, with nothing called."""
    with Run() as h:
        r = h.each(host=["--profile", "editor-eu"])
        assert r.returncode == 2 and "--instance-id" in r.stderr, (r.returncode, r.stderr)
        assert h.calls() == [] and not h.nebius_log.exists()


if __name__ == "__main__":
    raise SystemExit(main(globals()))
