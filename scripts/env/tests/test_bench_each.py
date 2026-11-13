"""`bench-each.sh`: the H100 benchmark one process per query, over a fake run-gpu.sh and nebius.sh.

The fake run-gpu.sh plays the VM's `run bench`, `status`, `follow` and `pull [--clear]`: a case writes
its section into the VM's tree and its rows into the VM's record, as the harness does, and the run's
end keeps the record as `records-<stamp>.tsv`, as the runner does. `pull` keeps a `records.tsv` no run
holds under a stamp, copies the results and the records into the tree on dev, never over a local
record that differs (exit 1, named), and with `--clear` removes each VM record whose copy home is the
same; `--clear` is refused, exit 2, while a run goes. Per query a behaviour: `ok`, `timeout` (a failed
section, no rows), `kill` (rows and no section, every time), `kill-once` (that, then `ok`) or `drop`
(`ok`, but run-gpu.sh exits 1 with the run going, as a dropped ssh leaves it, until `follow`) or
`conflict` (`ok`, and the next pull, its record home, exits 1 naming another file changed on both
sides). Like
run-tests.sh, `run bench` refuses a VM record that holds rows, and like the harness under resume it
keeps a section on the VM whose trailer is this run's, running nothing. The fake nebius.sh runs `ssh`
commands in the VM's home, where `hostname` is the trailer's host.
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
state = pathlib.Path(os.environ["FAKE_STATE"])
seen = json.loads(state.read_text()) if state.exists() else {}

def saved():
    state.write_text(json.dumps(seen))

def kept():
    """records.tsv as records-<stamp>.tsv, the stamps in time order."""
    if record.exists():
        seen["#stamp"] = seen.get("#stamp", 0) + 1
        record.rename(record.with_name(f"records-20260101T{seen['#stamp']:06d}.000Z.tsv"))
        saved()

if verb == "status":
    sys.exit(5 if seen.get("#going") else 0)
if verb == "follow":
    seen.pop("#going", None)
    saved()
    sys.exit(0)
if verb == "pull":
    if seen.get("#going") and a[i + 1:] == ["--clear"]:
        print("run-gpu.sh: the bench run goes on: run-gpu.sh … follow follows it", file=sys.stderr)
        sys.exit(2)
    if not seen.get("#going"):
        kept()
    tree = pathlib.Path(os.environ["FAKE_TREE"]) / "testdata"
    if (vm / "benchmark-results").is_dir():
        shutil.copytree(vm / "benchmark-results", tree / "benchmark-results", dirs_exist_ok=True)
    (tree / "calibration").mkdir(parents=True, exist_ok=True)
    rc = 0
    for f in sorted((vm / "calibration").glob("records*.tsv")):
        if f.name == "records.tsv":
            continue
        home = tree / "calibration" / f.name
        if home.exists() and home.read_bytes() != f.read_bytes():
            print(f"run-gpu.sh: pull: kept testdata/calibration/{f.name}: it changed here and on the VM",
                  file=sys.stderr)
            rc = 1
            continue
        shutil.copy2(f, home)
        if a[i + 1:] == ["--clear"] and re.fullmatch(r"records-\d.*\.tsv", f.name):
            f.unlink()
    if seen.pop("#conflict", None):
        saved()
        print("run-gpu.sh: pull: kept testdata/goldens/tpch.sf40/gpu-result.txt: it changed here and on the VM",
              file=sys.stderr)
        rc = 1
    sys.exit(rc)
assert a[i:] == ["run", "bench"], a
if record.exists() and record.stat().st_size:
    print("run-tests.sh: the record holds an earlier run's rows", file=sys.stderr)
    sys.exit(2)
ds, sf, query, mode = re.fullmatch(r"bench_(tpch|tpcds)_sf(\d+)_(.+)_(tp1_single|tp4_sized)",
                                   os.environ["PCK_TEST_FILTER"]).groups()
seen[query] = seen.get(query, 0) + 1
saved()
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
if behaviour in ("ok", "kill", "drop", "conflict"):
    record.parent.mkdir(parents=True, exist_ok=True)
    fresh = not record.exists() or not record.stat().st_size
    with open(record, "a") as f:
        if fresh:
            f.write(os.environ["FAKE_HEADING"] + os.environ["FAKE_COLUMNS"])
        for seq in range(2):
            f.write(f"{ds}\t{sf}\t{query.replace('_', '-')}\t{mode.replace('_', '-')}\t{seq}\t{seen[query]}\n")
if behaviour in ("ok", "drop", "conflict"):
    with open(tree, "a") as f:
        f.write(f"== {query.replace('_', '-')}\nGpuScan\n  time_us=[[1]] total_us=1\n--- run ---\nrun_us=1\n{trailer}")
if behaviour == "timeout":
    with open(tree, "a") as f:
        f.write(f"== {query.replace('_', '-')}\nstatus=failed reason=timeout: the case ran past "
                f"{os.environ['PEACOCK_BENCH_CASE_TIMEOUT_S']} s\n{trailer}")
kept()
if behaviour == "conflict":
    seen["#conflict"] = 1  # the next pull finds another file changed here and on the VM
    saved()
    sys.exit(0)
if behaviour == "drop":
    seen["#going"] = 1
    saved()
    print("run-gpu.sh: no longer following; the run goes on", file=sys.stderr)
    sys.exit(1)
sys.exit(0 if behaviour == "ok" else 1)
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

    def nebius_calls(self):
        if not self.nebius_log.exists():
            return []
        return [json.loads(line) for line in self.nebius_log.read_text().splitlines()]

    def records_left(self):
        """Every calibration record still on the VM or in the tree."""
        return [p for root in (self.vm, self.tree / "testdata") for p in (root / "calibration").glob("records*.tsv")]


def ok(r):
    assert r.returncode == 0, (r.returncode, r.stdout, r.stderr)
    return r


def test_bench_each_runs_one_process_per_query():
    with Run() as h:
        ok(h.each())
        assert h.filters() == ALL_TP1, h.filters()
        verbs = [c["argv"][len(H100):] for c in h.calls()]
        assert verbs == [["pull", "--clear"]] + [["run", "bench"], ["pull", "--clear"]] * 4, verbs
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
        for call in h.nebius_calls():
            assert call[:4] == H100 and "--dir" not in call, call


def test_bench_each_passes_dir_to_run_gpu_alone():
    with Run() as h:
        (h.home / "peacock" / "other" / "testdata").mkdir(parents=True)
        ok(h.each("--dir", "other"))
        for call in h.calls():
            assert call["argv"][4:6] == ["--dir", "other"], call
        for call in h.nebius_calls():
            assert "--dir" not in call, call
        assert any((h.home / "peacock/other/testdata/benchmark-results" / SLUG).iterdir())


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
        assert h.records_left() == [], h.records_left()
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
        # A killed runner's record, and the record of a run whose pull never came.
        h.host_record().parent.mkdir(parents=True)
        h.host_record().write_text(HEADING + COLUMNS + "tpch\t40\tq6\ttp1-single\t0\t0\n")
        (h.vm / "calibration/records-20250101T000000.000Z.tsv").write_text(
            HEADING + COLUMNS + "tpch\t40\tq1\ttp1-single\t0\t0\n")
        ok(h.each())
        for q in ("q1", "q6"):
            left = h.records / f"tpch-{q}-tp1_single.1.tsv"
            assert left.read_text().endswith(f"tpch\t40\t{q}\ttp1-single\t0\t0\n"), left.read_text()
            assert f"tpch\t40\t{q}\ttp1-single\t0\t1\n" in (h.records / f"tpch-{q}-tp1_single.2.tsv").read_text()
        assert h.filters() == ALL_TP1, h.filters()
        assert h.records_left() == [], h.records_left()


def test_bench_each_removes_no_vm_record_by_hand():
    with Run() as h:
        r = h.each(behaviour={"q6": "kill"})
        assert r.returncode == 1 and (h.records / "tpch-q6-tp1_single.1.tsv").exists(), (r.returncode, r.stderr)
        assert not any("records" in " ".join(call) for call in h.nebius_calls()), h.nebius_calls()
        pulls = [c["argv"][len(H100):] for c in h.calls() if "pull" in c["argv"]]
        assert pulls and all(p == ["pull", "--clear"] for p in pulls), pulls


def test_bench_each_sets_aside_no_record_but_a_run_s():
    with Run() as h:
        # create_nsys_profile.sh's metrics pass writes its record beside a run's; it is no case's.
        metrics = h.vm / "calibration/records-metrics.tsv"
        metrics.parent.mkdir(parents=True)
        metrics.write_text(HEADING + COLUMNS + "tpch\t40\tq6\ttp1-single\t0\t9\n")
        ok(h.each())
        assert sorted(p.name for p in h.records.glob("tpch-q6-*.tsv")) == ["tpch-q6-tp1_single.1.tsv"]
        assert "\t9\n" not in (h.records / "tpch-q6-tp1_single.1.tsv").read_text()
        assert metrics.is_file()


def test_bench_each_sets_aside_the_record_of_a_pull_that_kept_a_file():
    with Run() as h:
        r = h.each(behaviour={"q6": "conflict"})
        assert r.returncode == 1 and "gpu-result.txt" in r.stderr, (r.returncode, r.stderr)
        assert h.filters() == ALL_TP1[:2], h.filters()
        assert "tpch\t40\tq6\ttp1-single\t0\t1\n" in (h.records / "tpch-q6-tp1_single.1.tsv").read_text()
        assert h.records_left() == [], h.records_left()


def test_bench_each_follows_a_run_whose_ssh_dropped():
    with Run() as h:
        r = ok(h.each(behaviour={"q6": "drop"}))
        verbs = [c["argv"][len(H100):] for c in h.calls()]
        at = verbs.index(["run", "bench"], verbs.index(["run", "bench"]) + 1)
        assert verbs[at:at + 4] == [["run", "bench"], ["status"], ["follow"], ["pull", "--clear"]], verbs
        assert h.filters() == ALL_TP1, h.filters()
        assert (h.records / "tpch-q6-tp1_single.1.tsv").exists(), r.stdout


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
