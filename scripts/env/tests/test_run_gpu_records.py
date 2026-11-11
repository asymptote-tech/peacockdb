"""`run-gpu.sh`: what a tree keeps of its runs, and whose a VM dir is.

A tree's id in `.run-gpu-tree`, beside the path it was made for, or `PEACOCK_TREE_ID`; each VM dir
records the id of the tree that made or claimed it in `.tree`, and refuses every other tree's call
before it touches the dir. Each run's log and calibration record under its stamp; ship's stamp of
what it sent, `.shipped`, and `pull`, which takes what the VM changed since and keeps a file changed
on both sides; `pull --clear`. gpu_host.py has the fakes; test_run_gpu_runs.py has the runner.
"""
import fcntl
import hashlib
import os
import pathlib
import re
import shutil
import signal
import subprocess
import sys
import time

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
sys.path.append(str(pathlib.Path(__file__).resolve().parents[2] / "exec_model" / "tests"))
from harness import main  # noqa: E402
from gpu_host import Host, alive, started_at, until  # noqa: E402
from machine import git  # noqa: E402
from test_run_gpu import rsync_env  # noqa: E402
from test_run_gpu_runs import BOOT, STAGED, held, ok, on_card, refused, shipped, staged, state  # noqa: E402

RECORD = ".run-gpu-tree"
CI = {"PEACOCK_TREE_ID": "ci"}
RESULT = "testdata/benchmark-results/h100/tpch.sf40/tp1_single.benchmark.txt"
CALLS = "testdata/calibration/calls.tsv"
ANSWER = "testdata/goldens/tpch.sf1/gpu-result.txt"
CAL = "testdata/calibration"
PAST = (1_000_000_000, 1_000_000_000)  # 2001-09-09T01:46:40Z


def recorded(tree):
    """The tree's record: its id and the path it was made for."""
    tree_id, path = (tree / RECORD).read_text().splitlines()
    assert re.fullmatch(r"[0-9a-f]{32}", tree_id), tree_id
    return tree_id, path


def owner(h, d="chain-l"):
    return (h.vm_dir(d) / ".tree").read_text().strip()


def tree_copy(h, where):
    """A copy of the Host's tree, under the same name: `where`/chain-l beside dev/."""
    copy = h.tree.parent.parent / where / h.tree.name
    shutil.copytree(h.tree, copy, symlinks=True)
    return copy


def gpu_in(h, tree, *args, env=None):
    """<tree>'s run-gpu.sh, with the Host's VM and fakes."""
    own, h.tree = h.tree, tree
    try:
        return h.gpu(*args, env=env)
    finally:
        h.tree = own


def snapshot(d):
    """Every path under <d>, a file with its bytes: what a refused call must leave as it was."""
    return {str(p.relative_to(d)): p.read_bytes() if p.is_file() and not p.is_symlink() else None
            for p in d.rglob("*")}


def vm_function(h, names, call, stdin=None):
    """<call> under bash on the VM, with run-gpu.sh's VM functions <names> defined, as on_vm sends them."""
    defs = "; ".join(f'eval "$(sed -n \'/^{n}() {{/,/^}}/p\' "$0"/run-gpu*.sh)"' for n in names)
    return subprocess.run(["bash", "-c", f"set -euo pipefail; {defs}; {call}", str(h.tree / "scripts/env")],
                          env={"HOME": str(h.home), "PATH": os.environ["PATH"]}, input=stdin,
                          capture_output=True, text=True, timeout=30)


def test_a_tree_records_its_id_and_its_dir_on_the_vm():
    with Host() as h:
        refused(h.gpu("--dir", "a/b", "ship"), 2, "a/b")
        assert not (h.tree / RECORD).exists(), "a refused call made the record"
        shipped(h)
        tree_id, path = recorded(h.tree)
        assert path == str(h.tree.resolve()), path
        assert owner(h) == tree_id, owner(h)
        for args in (("ship",), ("run", "list"), ("pull",), ("cancel",), ("--dir", "second", "ship")):
            ok(h.gpu(*args))
        assert recorded(h.tree) == (tree_id, path)
        assert owner(h) == tree_id and owner(h, "second") == tree_id, (owner(h), owner(h, "second"))


def test_a_copied_tree_is_refused_its_record_before_any_vm_call():
    with Host() as h:
        shipped(h)
        copy = tree_copy(h, "copy")
        kept = (copy / RECORD).read_text()
        calls = len(h.nebius_calls())
        for args in (("prepare", "--no-sf40"), ("ship",), ("run", "list"), ("run", STAGED), ("pull",),
                     ("pull", "--clear"), ("cancel",), ("status",), ("follow",), ("--dir", "copy", "ship")):
            r = gpu_in(h, copy, *args)
            refused(r, 2, RECORD, str(h.tree.resolve()), "remove", "--dir")
        assert len(h.nebius_calls()) == calls, h.nebius_calls()[calls:]
        assert (copy / RECORD).read_text() == kept
        (copy / RECORD).unlink()
        ok(gpu_in(h, copy, "--dir", "copy", "ship"))
        copy_id, path = recorded(copy)
        assert path == str(copy.resolve()) and copy_id != recorded(h.tree)[0], (copy_id, path)
        assert owner(h, "copy") == copy_id
        ok(h.gpu("ship"))


def test_another_tree_s_call_is_refused_before_it_ships_runs_pulls_or_clears():
    with Host() as h:
        other = tree_copy(h, "other")
        (other / "cpp/install/lib/libpeacock_gpu.so").write_text("the other tree's build\n")
        staged(h)
        d = h.vm_dir()
        result = d / "testdata/benchmark-results/h100/tpch.sf40/tp1_single.benchmark.txt"
        result.parent.mkdir(parents=True)
        result.write_text("the first tree's result\n")
        before, rsh = snapshot(d), h.verbs().count("rsh")
        for args in (("prepare", "--no-sf40"), ("ship",), ("run", "list"), ("run", STAGED), ("pull",),
                     ("pull", "--clear"), ("cancel",)):
            refused(gpu_in(h, other, *args), 2, "~/peacock/chain-l on the VM is another tree's", "--dir")
        assert h.verbs().count("rsh") == rsh, "a refused call synced"
        assert h.docker_runs() == []
        assert snapshot(d) == before, "a refused call touched the dir"
        assert not (other / "testdata/benchmark-results/h100").exists(), "a refused pull brought a file home"
        held(h, "list")
        for args in (("ship",), ("cancel",), ("run", "list")):
            refused(gpu_in(h, other, *args), 2, "another tree's", "--dir")
        assert on_card(h) and len(h.docker_runs()) == 1, h.docker_runs()
        h.hold.unlink()
        until(lambda: h.gpu("status").stdout.startswith("exited 0"), "the first tree's run ended green")
        ok(gpu_in(h, other, "--dir", "other", "ship"))
        assert (h.vm_dir("other") / "cpp/install/lib/libpeacock_gpu.so").read_text() == "the other tree's build\n"
        assert (d / "cpp/install/lib/libpeacock_gpu.so").read_text() == "staged"


def test_a_dir_made_before_tree_is_claimed_by_the_first_tree():
    with Host() as h:
        other = tree_copy(h, "other")
        shipped(h)
        (h.vm_dir() / ".tree").unlink()
        old_log = h.vm_dir() / "logs" / "list.log"
        old_log.parent.mkdir()
        old_log.write_text("a run before the runner\n")
        r = ok(gpu_in(h, other, "cancel"))
        assert "claimed" in r.stdout, r.stdout
        assert owner(h) == recorded(other)[0], owner(h)
        assert old_log.read_text() == "a run before the runner\n"
        refused(h.gpu("ship"), 2, "another tree's", "--dir")
        r = ok(gpu_in(h, other, "ship"))
        assert "claimed" not in r.stdout, r.stdout


def test_a_fixed_tree_id_owns_its_dir_and_writes_no_record():
    with Host() as h:
        other = tree_copy(h, "other")
        (other / RECORD).write_text(f"{'0' * 32}\n/elsewhere\n")
        ok(h.gpu("--dir", "ci", "prepare", "--no-sf40", env=CI))
        ok(h.gpu("--dir", "ci", "ship", env=CI))
        ok(gpu_in(h, other, "--dir", "ci", "ship", env=CI))
        ok(gpu_in(h, other, "--dir", "ci", "cancel", env=CI))
        assert owner(h, "ci") == "ci", owner(h, "ci")
        assert not (h.tree / RECORD).exists(), "a fixed id wrote a record"
        assert (other / RECORD).read_text() == f"{'0' * 32}\n/elsewhere\n"
        calls = len(h.nebius_calls())
        for bad in ("", "a/b", "c i"):
            refused(h.gpu("--dir", "ci", "ship", env={"PEACOCK_TREE_ID": bad}), 2, "PEACOCK_TREE_ID")
        assert len(h.nebius_calls()) == calls, h.nebius_calls()[calls:]
        refused(h.gpu("--dir", "ci", "ship"), 2, "another tree's", "--dir")


def test_a_record_that_cannot_be_made_is_refused_before_any_vm_call():
    for make in ("dangling", "directory"):
        with Host() as h:
            if make == "dangling":
                (h.tree / RECORD).symlink_to(h.tree / "nowhere")
            else:
                (h.tree / RECORD).mkdir()
            refused(h.gpu("ship"), 2, RECORD)
            assert h.nebius_calls() == [], (make, h.nebius_calls())
            assert not (h.vm_dir() / ".tree").exists(), make


def test_the_vm_refuses_an_empty_tree_id():
    with Host() as h:
        refused(vm_function(h, ["vm_tree_claimed"], 'vm_tree_claimed chain-l ""'), 2, "no tree id")
        assert not (h.vm_dir() / ".tree").exists()


def test_status_and_follow_claim_nothing():
    with Host() as h:
        other = tree_copy(h, "other")
        r = h.gpu("status")
        assert r.returncode == 2 and r.stdout.startswith("none"), (r.returncode, r.stdout, r.stderr)
        refused(h.gpu("follow"), 2, "status")
        assert not (h.tree / RECORD).exists(), "status or follow made the record"
        assert not h.vm_dir().exists(), "status or follow made the dir"
        shipped(h)
        ok(h.gpu("run", "list"))
        r = ok(gpu_in(h, other, "status"))
        assert r.stdout.startswith("exited 0"), r.stdout
        r = ok(gpu_in(h, other, "follow"))
        assert "green" in r.stdout, r.stdout
        assert not (other / RECORD).exists(), "status or follow made the record"
        assert owner(h) == recorded(h.tree)[0], owner(h)


# --- each run's log and record under its stamp -----------------------------------------------------

def sha(path):
    return hashlib.sha256(pathlib.Path(path).read_bytes()).hexdigest()


def committed(h, files):
    """<files>, {path: text}, written and committed in the Host's tree: what ship sends."""
    for rel, text in files.items():
        (h.tree / rel).parent.mkdir(parents=True, exist_ok=True)
        (h.tree / rel).write_text(text)
    git(h.tree, "add", *files)
    git(h.tree, "-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "-m", "results")


def stamps(h):
    """The dir's .shipped, path to sha256."""
    lines = (h.vm_dir() / ".shipped").read_text().splitlines()
    return {path: digest for digest, path in (ln.split("  ", 1) for ln in lines)}


def records(d):
    return sorted(p.name for p in (d / CAL).glob("records*.tsv"))


def home_logs(h):
    return h.laptop / ".cache/peacock/run-gpu/chain-l"


def bench(h, rows):
    """`run bench` adding <rows> to its record; its stamp."""
    h.world(run_record=f"# heading\n{rows}\n")
    ok(h.gpu("run", "bench"))
    return state(h)[3]


def test_each_run_logs_under_its_stamp():
    with Host() as h:
        shipped(h)
        logs = []
        for mode in ("list", "list", "filter"):
            ok(h.gpu("run", mode))
            logs.append(f"{mode}-{state(h)[3]}.log")
        assert sorted(p.name for p in (h.vm_dir() / "logs").iterdir()) == sorted(logs)
        ok(h.gpu("pull"))
        assert sorted(p.name for p in home_logs(h).iterdir()) == sorted(logs)


def test_each_bench_run_keeps_its_own_record():
    with Host() as h:
        shipped(h)
        runs = [bench(h, "row of run 1"), bench(h, "row of run 2")]
        d = h.vm_dir()
        assert records(d) == [f"records-{s}.tsv" for s in runs], records(d)
        assert (d / CAL / f"records-{runs[1]}.tsv").read_text() == "# heading\nrow of run 2\n"
        ok(h.gpu("pull"))
        assert records(h.tree) == records(d), records(h.tree)
        for s in runs:
            assert (h.tree / CAL / f"records-{s}.tsv").read_bytes() == (d / CAL / f"records-{s}.tsv").read_bytes()
        # Edited here once home: kept without a word while the VM's copy stands.
        edited = h.tree / CAL / f"records-{runs[0]}.tsv"
        edited.write_text("rows from somewhere else\n")
        os.utime(edited, PAST)
        r = ok(h.gpu("pull"))
        assert r.stderr == "" and edited.read_text() == "rows from somewhere else\n", (r.stderr, edited.read_text())


def test_a_cancelled_or_killed_bench_keeps_its_record():
    with Host() as h:
        shipped(h)
        d = h.vm_dir()
        h.world(run_record="# heading\nrow of the cancelled run\n")
        held(h, "bench")
        cancelled = state(h)[3]
        ok(h.gpu("cancel"))
        assert records(d) == [f"records-{cancelled}.tsv"], records(d)
        h.world(run_record="# heading\nrow of the killed run\n", run_code=137)
        held(h, "bench")
        killed = state(h)[3]
        h.hold.unlink()
        status = d / "run" / "status"
        until(lambda: status.is_file() and status.read_text().split()[0] == killed, "the killed run ended")
        assert status.read_text().split()[1] == "137", status.read_text()
        assert records(d) == [f"records-{cancelled}.tsv", f"records-{killed}.tsv"], records(d)
        assert (d / CAL / f"records-{killed}.tsv").read_text() == "# heading\nrow of the killed run\n"


def test_a_killed_bench_s_record_comes_home():
    with Host() as h:
        shipped(h)
        d = h.vm_dir()
        h.world(run_record="# heading\nrow of the killed run\n")
        held(h, "bench")
        pid, stamp = int(state(h)[0]), state(h)[3]
        os.killpg(pid, signal.SIGKILL)  # the runner and its client: nothing moves the record
        until(lambda: not alive(pid), "the runner died")
        assert records(d) == ["records.tsv"], records(d)
        ok(h.gpu("cancel"))  # the container it left
        h.hold.unlink()
        ok(h.gpu("pull"))
        assert records(d) == [f"records-{stamp}.tsv"], records(d)
        home = h.tree / CAL / f"records-{stamp}.tsv"
        assert home.read_text() == "# heading\nrow of the killed run\n"
        nxt = bench(h, "row of the next run")
        ok(h.gpu("pull"))
        assert records(h.tree) == sorted([f"records-{stamp}.tsv", f"records-{nxt}.tsv"]), records(h.tree)


def test_a_record_left_by_a_runner_that_died_is_kept_under_its_run_s_stamp():
    with Host() as h:
        shipped(h)
        d = h.vm_dir()
        stamp = "20000101T000000.000Z"
        (d / "run").mkdir(exist_ok=True)
        (d / "logs").mkdir(exist_ok=True)
        (d / CAL).mkdir(parents=True, exist_ok=True)
        # A runner of the last boot: the VM restarted under its run, which left its record.
        (d / "run" / "state").write_text(f"{os.getpid()} 00000000-0000-0000-0000-000000000000 bench {stamp} 1\n")
        (d / "logs" / f"bench-{stamp}.log").write_text("a line before the restart\n")
        (d / CAL / "records.tsv").write_text("# heading\nrow before the restart\n")
        h.world(containers={"peacock-chain-l": {"state": "exited", "code": 137}})
        nxt = bench(h, "row after the restart")
        assert records(d) == [f"records-{stamp}.tsv", f"records-{nxt}.tsv"], records(d)
        ok(h.gpu("pull"))
        assert (h.tree / CAL / f"records-{stamp}.tsv").read_text() == "# heading\nrow before the restart\n"
        assert (h.tree / CAL / f"records-{nxt}.tsv").read_text() == "# heading\nrow after the restart\n"


def test_a_record_from_before_the_runner_is_kept_under_its_time():
    with Host() as h:
        shipped(h)
        d = h.vm_dir()
        (d / CAL).mkdir(parents=True, exist_ok=True)
        (d / CAL / "records.tsv").write_text("# heading\nrow of an old run\n")
        os.utime(d / CAL / "records.tsv", PAST)
        (d / "logs").mkdir()
        (d / "logs" / "bench.log").write_text("a log without a stamp\n")
        ok(h.gpu("pull"))
        old = "records-20010909T014640.000Z.tsv"
        assert records(d) == [old] and records(h.tree) == [old], (records(d), records(h.tree))
        assert (h.tree / CAL / old).read_text() == "# heading\nrow of an old run\n"
        assert (home_logs(h) / "bench.log").read_text() == "a log without a stamp\n"
        bench(h, "row of the next run")


def test_pull_never_replaces_a_local_record_that_differs():
    with Host() as h:
        shipped(h)
        name = f"records-{bench(h, 'the VM rows')}.tsv"
        home = h.tree / CAL / name
        home.parent.mkdir(parents=True, exist_ok=True)
        home.write_text("rows from somewhere else\n")
        os.utime(home, PAST)
        refused(h.gpu("pull"), 1, f"kept {CAL}/{name}: it changed here and on the VM")
        assert home.read_text() == "rows from somewhere else\n"
        home.unlink()
        ok(h.gpu("pull"))
        assert home.read_text() == "# heading\nthe VM rows\n"
        # Home unchanged since the pull, the VM's rewritten: a record is still never replaced.
        (h.vm_dir() / CAL / name).write_text("# heading\nrewritten on the VM\n")
        refused(h.gpu("pull"), 1, name)
        assert home.read_text() == "# heading\nthe VM rows\n"


# --- ship's stamp, and pull -----------------------------------------------------------------------

def test_ship_stamps_what_it_shipped():
    with Host() as h:
        committed(h, {RESULT: "== q1\ncommitted\n", CALLS: "calls\n", ANSWER: "cudf=25.02\n"})
        ok(h.gpu("prepare", "--no-sf40"))
        d = h.vm_dir()
        # A run's files on the VM before the ship: never shipped, so never stamped.
        for rel in (f"{CAL}/records-20000101T000000.000Z.tsv", "testdata/benchmark-results/h100/new.txt"):
            (d / rel).parent.mkdir(parents=True, exist_ok=True)
            (d / rel).write_text("a run's\n")
        ok(h.gpu("ship"))
        assert stamps(h) == {rel: sha(d / rel) for rel in (RESULT, CALLS, ANSWER)}, stamps(h)
        before = (d / ".shipped").read_bytes()
        committed(h, {RESULT: "== q1\nmeasured again\n"})
        r = h.gpu("ship", env=rsync_env(h, FAKE_RSYNC_FAIL="testdata.files:99"))
        assert r.returncode == 1 and "testdata stream failed" in r.stderr, (r.returncode, r.stderr)
        assert (d / ".shipped").read_bytes() == before


def test_pull_skips_a_shipped_file_a_checkout_touched():
    with Host() as h:
        committed(h, {RESULT: "== q1\ncommitted\n"})
        shipped(h)
        home = h.tree / RESULT
        for when in (time.time() + 3600, PAST[0]):
            home.write_text("== q1\nanother branch's\n")
            os.utime(home, (when, when))
            r = ok(h.gpu("pull"))
            assert r.stderr == "", (when, r.stderr)
            assert home.read_text() == "== q1\nanother branch's\n", (when, home.read_text())


def test_pull_names_a_file_the_run_changed_that_differs_here():
    with Host() as h:
        committed(h, {RESULT: "== q1\ncommitted\n"})
        shipped(h)
        on_vm, home = h.vm_dir() / RESULT, h.tree / RESULT
        on_vm.write_text("== q1\ncommitted\n== q6\nmeasured on the VM\n")
        home.write_text("== q1\nedited here\n")
        for when in (None, PAST):
            if when:
                os.utime(home, when)
            refused(h.gpu("pull"), 1, f"kept {RESULT}: it changed here and on the VM")
            assert home.read_text() == "== q1\nedited here\n"
            assert on_vm.read_text() == "== q1\ncommitted\n== q6\nmeasured on the VM\n"


def test_pull_says_nothing_of_an_unchanged_identical_file():
    with Host() as h:
        committed(h, {RESULT: "== q1\ncommitted\n"})
        shipped(h)
        home = h.tree / RESULT
        os.utime(home, PAST)
        r = ok(h.gpu("pull"))
        assert r.stderr == "", r.stderr
        assert home.stat().st_mtime == PAST[1], home.stat().st_mtime


def test_pull_takes_each_section_a_run_adds_to_a_shipped_file():
    with Host() as h:
        committed(h, {RESULT: "== q1\ncommitted\n"})
        shipped(h)
        home, text = h.tree / RESULT, "== q1\ncommitted\n"
        later = time.time() + 3600
        os.utime(home, (later, later))  # a checkout after the ship
        for q in ("q6", "q14"):
            text += f"== {q}\nmeasured on the VM\n"
            (h.vm_dir() / RESULT).write_text(text)
            r = ok(h.gpu("pull"))
            assert r.stderr == "" and home.read_text() == text, (q, r.stderr, home.read_text())


def test_a_file_a_run_wrote_before_a_ship_still_comes_home():
    with Host() as h:
        committed(h, {RESULT: "== q1\ncommitted\n"})
        shipped(h)
        name = f"records-{bench(h, 'row')}.tsv"
        new = "testdata/benchmark-results/h100/tpch.sf40/tp4_sized.benchmark.txt"
        (h.vm_dir() / new).write_text("== q1\nmeasured on the VM\n")
        ok(h.gpu("ship"))
        ok(h.gpu("pull"))
        assert (h.tree / new).read_text() == "== q1\nmeasured on the VM\n"
        assert (h.tree / CAL / name).read_text() == "# heading\nrow\n"


def test_a_dir_with_no_ship_stamp_names_each_local_file_that_differs():
    with Host() as h:
        committed(h, {RESULT: "== q1\ncommitted\n", CALLS: "calls\n"})
        shipped(h)
        d = h.vm_dir()
        (d / ".shipped").unlink()  # shipped before the stamp
        (d / RESULT).write_text("== q1\nmeasured on the VM\n")
        new = "testdata/benchmark-results/h100/tpch.sf40/tp4_sized.benchmark.txt"
        (d / new).write_text("== q1\nnew on the VM\n")
        r = h.gpu("pull")
        refused(r, 1, f"kept {RESULT}")
        assert CALLS not in r.stderr and new not in r.stderr, r.stderr
        assert (h.tree / RESULT).read_text() == "== q1\ncommitted\n"
        assert (h.tree / new).read_text() == "== q1\nnew on the VM\n"


def test_pull_brings_the_results_home_and_keeps_an_edit_made_here():
    with Host() as h:
        shipped(h)
        ok(h.gpu("pull"))
        assert not (h.tree / "testdata/benchmark-results/h100").exists(), "a VM with no run brings nothing"
        h.world(run_output="==> run-tests.sh bench: green\n")
        ok(h.gpu("run", "bench"))
        on_vm, home = h.vm_dir() / RESULT, h.tree / RESULT
        on_vm.parent.mkdir(parents=True)
        on_vm.write_text("== q1\nmeasured on the VM\n")
        ok(h.gpu("pull"))
        assert home.read_text() == "== q1\nmeasured on the VM\n"
        assert ["bench: green" in f.read_text() for f in home_logs(h).glob("bench-*.log")] == [True]
        home.write_text("== q1\nedited here, after the pull\n")
        os.utime(home, PAST)
        r = ok(h.gpu("pull"))
        assert r.stderr == "" and home.read_text() == "== q1\nedited here, after the pull\n", r.stderr
        on_vm.write_text("== q1\nmeasured on the VM\n== q6\nmeasured on the VM\n")  # a run changes the VM's
        for when in (PAST[0], time.time() + 3600):
            os.utime(home, (when, when))
            refused(h.gpu("pull"), 1, RESULT)
            assert home.read_text() == "== q1\nedited here, after the pull\n"


# --- pull --clear ---------------------------------------------------------------------------------

def test_pull_clear_removes_what_came_home_and_keeps_the_rest():
    with Host() as h:
        shipped(h)
        d = h.vm_dir()
        first = bench(h, "row 1")
        # create_nsys_profile.sh's metrics pass keeps its record on the VM: no run's.
        (d / CAL / "records-metrics.tsv").write_text("rows of the metrics pass\n")
        ok(h.gpu("pull", "--clear"))
        assert (d / CAL / "records-metrics.tsv").is_file(), "a clear took the metrics pass's record"
        for root in (d, h.tree):
            (root / CAL / "records-metrics.tsv").unlink()
        assert records(d) == [] and list((d / "logs").iterdir()) == [], (records(d), list((d / "logs").iterdir()))
        assert records(h.tree) == [f"records-{first}.tsv"], records(h.tree)
        assert [p.name for p in home_logs(h).iterdir()] == [f"bench-{first}.log"]
        gone = {f"{CAL}/records-{first}.tsv", f"logs/bench-{first}.log"}
        assert not gone & set(stamps(h)), stamps(h)
        differs, same = bench(h, "row 2"), bench(h, "row 3")
        (h.tree / CAL / f"records-{differs}.tsv").write_text("rows from somewhere else\n")
        r = h.gpu("pull", "--clear")
        refused(r, 1, f"records-{differs}.tsv")
        assert records(d) == [f"records-{differs}.tsv"], records(d)
        assert list((d / "logs").iterdir()) == [], list((d / "logs").iterdir())
        assert f"kept {CAL}/records-{differs}.tsv" in r.stdout, r.stdout
        assert (h.tree / CAL / f"records-{same}.tsv").read_text() == "# heading\nrow 3\n"
        assert (h.tree / CAL / f"records-{differs}.tsv").read_text() == "rows from somewhere else\n"


def test_pull_clear_is_refused_while_a_run_goes():
    with Host() as h:
        shipped(h)
        d = h.vm_dir()
        bench(h, "row 1")
        h.world(run_record="# heading\nrow 2\n")
        held(h, "bench")
        before, rsh = snapshot(d), h.verbs().count("rsh")
        refused(h.gpu("pull", "--clear"), 2, "follow")
        assert h.verbs().count("rsh") == rsh, "a refused pull synced"
        assert snapshot(d) == before, "a refused pull touched the dir"
        ok(h.gpu("pull"))  # a pull alone takes what is home-able, the going run's record not
        assert (d / CAL / "records.tsv").is_file() and not (h.tree / CAL / "records.tsv").exists()
        h.hold.unlink()
        until(lambda: h.gpu("status").stdout.startswith("exited 0"), "the run ended")
        ok(h.gpu("pull", "--clear"))
        assert records(d) == [] and len(records(h.tree)) == 2, (records(d), records(h.tree))


def test_pull_clear_is_refused_by_a_run_that_starts_while_it_waits():
    with Host() as h:
        shipped(h)
        d = h.vm_dir()
        stamp = "20000101T000000.000Z"
        log = d / "logs" / f"bench-{stamp}.log"
        log.parent.mkdir(exist_ok=True)
        runner = subprocess.Popen(["sleep", "60"], start_new_session=True)
        try:
            with open(d / "run" / "lock", "a") as lock:  # a run being started here
                fcntl.flock(lock, fcntl.LOCK_EX)
                p = h.gpu_going("pull", "--clear")
                time.sleep(1)
                assert p.poll() is None, p.communicate()
                # The run's state and its log, still empty while it waits for the card.
                log.write_text("")
                (d / "run" / "state").write_text(f"{runner.pid} {BOOT} bench {stamp} {started_at(runner.pid)}\n")
            out, err = p.communicate(timeout=60)
            assert p.returncode == 2 and "follow" in err, (p.returncode, out, err)
            assert log.is_file(), "a clear took the log of a run that goes"
        finally:
            runner.kill()
            runner.wait()


def test_status_and_follow_of_a_run_whose_log_was_cleared():
    with Host() as h:
        shipped(h)
        ok(h.gpu("run", "filter"))
        ok(h.gpu("pull", "--clear"))
        assert list((h.vm_dir() / "logs").iterdir()) == []
        r = ok(h.gpu("status"))
        assert r.stdout.splitlines()[0] == "exited 0" and "cleared" in r.stdout, r.stdout
        assert "${XDG_CACHE_HOME:-~/.cache}/peacock/run-gpu/chain-l/ on the machine that pulled" in r.stdout, r.stdout
        r = ok(h.gpu("follow"))
        assert "cleared" in r.stdout + r.stderr, (r.stdout, r.stderr)


def test_the_vm_s_clear_keeps_records_tsv_and_what_did_not_come_home():
    with Host() as h:
        d = h.vm_dir()
        (d / CAL).mkdir(parents=True)
        (d / "logs").mkdir()
        home_a, left_b = f"{CAL}/records-20000101T000000.000Z.tsv", f"{CAL}/records-20000102T000000.000Z.tsv"
        metrics = f"{CAL}/records-metrics.tsv"
        for rel in (f"{CAL}/records.tsv", metrics, home_a, left_b, "logs/bench-a.log", "logs/bench-b.log"):
            (d / rel).write_text(f"rows of {rel}\n")
        home = {rel: sha(d / rel) for rel in (f"{CAL}/records.tsv", metrics, home_a, "logs/bench-a.log")}
        home["logs/bench-b.log"] = "0" * 64
        r = vm_function(h, ["vm_locked", "vm_clearable", "vm_cleared"], "vm_cleared chain-l",
                        "".join(f"{v} {k}\n" for k, v in home.items()))
        assert r.returncode == 0, (r.returncode, r.stdout, r.stderr)
        assert records(d) == sorted([left_b.split("/")[-1], "records-metrics.tsv", "records.tsv"]), records(d)
        assert [p.name for p in (d / "logs").iterdir()] == ["bench-b.log"]
        assert f"kept {left_b}: not pulled home" in r.stdout, r.stdout
        assert "kept logs/bench-b.log: its copy home differs" in r.stdout, r.stdout
        assert "records.tsv:" not in r.stdout and "metrics" not in r.stdout, r.stdout

if __name__ == "__main__":
    raise SystemExit(main(globals()))
