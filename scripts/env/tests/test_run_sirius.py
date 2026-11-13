"""`run-sirius.sh`: Sirius's prepare and run on the VM, its runs held by run-gpu.sh's runner. A run
takes the card lock, `run-gpu.sh` follows, cancels and pulls it, its files come home through the
stamps, and its image runs by the ID its labels were read from.

gpu_host.py has the fakes: a device run holds while `h.hold` exists. test_run_gpu*.py have run-gpu.sh.
"""
import hashlib
import json
import os
import pathlib
import sys
import time

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
sys.path.append(str(pathlib.Path(__file__).resolve().parents[2] / "exec_model" / "tests"))
from harness import main  # noqa: E402
from gpu_host import H100, HOSTNAME, SIRIUS_CUDF, Host, label, until  # noqa: E402
from machine import REPO  # noqa: E402

NAME = "peacock-chain-l"
SIRIUS_ID = "sha256:sirius1"
# The H100's driver, which CUDA 13 runs on; the fake VM's default, 550, is older than 580.65.06.
SIRIUS_DRIVER = "580.173"
RUNNER = ["--dataset", "tpch", "--sf", "40", "--queries", "q6,aggregate-groupby", "--runs", "10",
          "--timeout", "900"]
RESULTS = "testdata/benchmark-results/h100-80gb-hbm3/tpch.sf40/sirius.tsv"
ANSWERS = "testdata/calibration/sirius/tpch.sf40/answers.duckdb"


def ok(r):
    assert r.returncode == 0, (r.returncode, r.stdout, r.stderr)
    return r


def refused(r, code, *words):
    assert r.returncode == code, (r.returncode, r.stdout, r.stderr)
    for w in words:
        assert w in r.stderr, (w, r.stderr)


def values(args, flag):
    return [args[i + 1] for i, a in enumerate(args) if a == flag]


def sirius(h, *args, env=None):
    """The tree's run-sirius.sh <args>, on the H100."""
    return h.script("scripts/env/run-sirius.sh", *H100, *args, env=env)


def sirius_prepared(h):
    """run-gpu.sh's prepare, with sf40, then run-sirius.sh's, on cards the Sirius image runs on."""
    h.world(cards=[[SIRIUS_DRIVER, "9.0"]])
    ok(h.gpu("prepare"))
    ok(sirius(h, "prepare"))
    return h


def sirius_runs(h):
    return [a for a in h.docker_runs() if SIRIUS_ID in a]


def state(h, d="chain-l"):
    """The dir's run/state: runner pid, boot id, what, stamp, start time."""
    return (h.vm_dir(d) / "run" / "state").read_text().split()


def log_text(h, d="chain-l"):
    _, _, what, stamp = state(h, d)[:4]
    return (h.vm_dir(d) / "logs" / f"{what}-{stamp}.log").read_text()


def on_card(h, name=NAME):
    return h.world()["containers"].get(name, {}).get("state") == "running"


def ended(h, d="chain-l"):
    """The dir's last run ended: its status names its stamp."""
    status = h.vm_dir(d) / "run" / "status"
    return status.is_file() and status.read_text().split()[0] == state(h, d)[3]


def first_line(r):
    return r.stdout.splitlines()[0] if r.stdout else ""


def sha256(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


# --- prepare --------------------------------------------------------------------------------------

def test_prepare_pulls_the_sirius_image_and_ships_its_profile():
    with Host() as h:
        sirius_prepared(h)
        pulled = [a[-1] for a in h.docker_calls() if a[0] == "pull"]
        assert pulled[-1] == h.sirius_ref and set(pulled) == {h.run_ref, h.build_ref, h.sirius_ref}, pulled
        shipped = h.vm_dir() / "docker" / "sirius-seccomp.json"
        assert shipped.read_bytes() == (REPO / "docker" / "sirius-seccomp.json").read_bytes()


def test_prepare_short_of_disk_stops_before_the_pull_naming_both_numbers():
    with Host() as h:
        h.world(cards=[[SIRIUS_DRIVER, "9.0"]])
        ok(h.gpu("prepare"))
        h.world(free_bytes=8 * 10**9)
        refused(sirius(h, "prepare"), 2, "8 GB free under /var/lib/docker", "needs 14")
        assert h.sirius_ref not in [a[-1] for a in h.docker_calls() if a[0] == "pull"]


def test_prepare_with_the_image_on_the_vm_measures_no_disk():
    with Host() as h:
        h.world(cards=[[SIRIUS_DRIVER, "9.0"]])
        ok(h.gpu("prepare"))
        h.world(free_bytes=8 * 10**9, absent=[])
        ok(sirius(h, "prepare"))


def test_prepare_refuses_a_driver_older_than_the_sirius_image_needs():
    with Host() as h:
        ok(h.gpu("prepare"))
        refused(sirius(h, "prepare"), 3, "550.54.15", "580.65.06")


def test_prepare_refuses_a_sirius_image_of_other_inputs():
    with Host() as h:
        h.world(cards=[[SIRIUS_DRIVER, "9.0"]])
        ok(h.gpu("prepare"))
        images = h.world()["images"]
        images[h.sirius_ref]["labels"]["peacockdb.inputs"] = label(h.tree, "sirius", change="docker/sirius-bench.py",
                                                                   cudf=SIRIUS_CUDF)
        h.world(images=images)
        refused(sirius(h, "prepare"), 2, "docker/sirius-bench.py", h.sirius_ref)


def test_prepare_on_a_vm_without_sf40_is_refused_naming_run_gpu_prepare():
    """Sirius runs over sf40: a VM prepared --no-sf40 is refused before the Sirius image is pulled."""
    with Host() as h:
        h.world(cards=[[SIRIUS_DRIVER, "9.0"]])
        ok(h.gpu("prepare", "--no-sf40"))
        refused(sirius(h, "prepare"), 2, "tpch.sf40", "run-gpu.sh", "prepare")
        assert h.sirius_ref not in [a[-1] for a in h.docker_calls() if a[0] == "pull"]


# --- run ------------------------------------------------------------------------------------------

def test_run_runs_the_runner_in_the_sirius_image_with_no_install():
    with Host() as h:
        sirius_prepared(h)
        r = ok(sirius(h, "run", "--", *RUNNER))
        assert "run-tests.sh list: green" in r.stdout, r.stdout
        [run] = sirius_runs(h)
        at = run.index(SIRIUS_ID)
        options, command = run[:at], run[at + 1:]
        base = h.vm_dir()
        assert values(options, "--security-opt") == [f"seccomp={base}/docker/sirius-seccomp.json"], options
        assert values(options, "--ulimit") == ["memlock=-1:-1"], options
        assert values(options, "--hostname") == [HOSTNAME], options
        assert values(options, "--gpus") == ["all"] and values(options, "--name") == [NAME], options
        assert values(options, "--label") == [f"peacock.dir={base}"], options
        mounts = values(options, "-v")
        assert f"{h.data}:{h.data}:ro" in mounts and f"{base}/testdata:/peacock/testdata" in mounts, mounts
        assert not [o for o in options if o == "--privileged" or o.startswith("--cap-add")], options
        assert command == ["python3", "/opt/sirius/sirius-bench.py", *RUNNER, "--data", f"{h.data}/tpch.sf40",
                           "--out", "/peacock/testdata"], command
        assert (base / "testdata" / "tpch-queries" / "q01.sql").read_text() == "select 1;\n"
        assert list((base / "cpp" / "install").iterdir()) == [], "an install in a dir never shipped"
        assert [a for a in h.docker_calls() if a[0] == "pull"].count(["pull", h.sirius_ref]) == 1, "run pulled"


def test_run_ships_the_sirius_results_a_resume_keeps():
    with Host() as h:
        sirius_prepared(h)
        tsv = h.tree / RESULTS
        tsv.parent.mkdir(parents=True)
        tsv.write_text("# rows of an earlier run\n")
        ok(sirius(h, "run", "--", *RUNNER, "--resume"))
        assert (h.vm_dir() / RESULTS).read_text() == "# rows of an earlier run\n"
        assert sirius_runs(h)[0][-5] == "--resume", sirius_runs(h)


def test_run_ships_the_answers_a_narrowed_or_resumed_run_keeps():
    """A narrowed run (--list) keeps the other queries' answers and --resume the kept rows', and
    `pull` brings the VM's database home: so the VM gets the laptop's first, a newer one there kept.
    A whole run starts its answers afresh and is sent none."""
    with Host() as h:
        sirius_prepared(h)
        answers = h.tree / ANSWERS
        answers.parent.mkdir(parents=True)
        answers.write_bytes(b"every query's answers")
        shipped = h.vm_dir() / ANSWERS
        ok(sirius(h, "run", "--", *RUNNER))
        assert not shipped.exists(), "a whole run was sent the answers it removes"
        for extra in (["--list", "q1,q6,aggregate-groupby"], ["--resume"]):
            shipped.unlink(missing_ok=True)
            ok(sirius(h, "run", "--", *RUNNER, *extra))
            assert shipped.read_bytes() == b"every query's answers", extra
        shipped.write_bytes(b"a newer run's answers")
        later = answers.stat().st_mtime + 60
        os.utime(shipped, (later, later))
        ok(sirius(h, "run", "--", *RUNNER, "--list", "q1,q6,aggregate-groupby"))
        assert shipped.read_bytes() == b"a newer run's answers", "a newer database on the VM was replaced"


def test_run_detached_is_judged_by_status_and_pulled():
    with Host() as h:
        sirius_prepared(h)
        r = ok(sirius(h, "run", "--detach", "--", *RUNNER))
        assert "detached" in r.stdout and "follow" in r.stdout, r.stdout
        until(lambda: ended(h), "the detached run ended")
        assert state(h)[2] == "sirius", state(h)
        assert ok(h.gpu("status")).stdout.startswith("exited 0"), "status"
        ok(h.gpu("pull"))
        assert (h.laptop / f".cache/peacock/run-gpu/chain-l/sirius-{state(h)[3]}.log").is_file()


def test_run_exits_as_the_runner_did():
    with Host() as h:
        sirius_prepared(h)
        for code, want in [(0, 0), (1, 1), (2, 2), (139, 1)]:
            h.world(run_code=code)
            r = sirius(h, "run", "--", *RUNNER)
            assert r.returncode == want, (code, r.returncode, r.stderr)


def test_run_before_prepare_is_refused():
    with Host() as h:
        h.world(cards=[[SIRIUS_DRIVER, "9.0"]])
        ok(h.gpu("prepare"))
        refused(sirius(h, "run", "--", *RUNNER), 2, "run-sirius.sh prepare")
        assert sirius_runs(h) == []


def test_run_arguments_are_refused_before_any_call():
    with Host() as h:
        cases = [(RUNNER, "--"), (["--", "--sf", "40", "--queries", "q6"], "--dataset"),
                 (["--", "--dataset", "tpch", "--queries", "q6"], "--sf"),
                 (["--", *RUNNER, "--out", "/x"], "--out"), (["--", *RUNNER, "--data", "/x"], "--data"),
                 (["--detach"], "--")]
        for args, word in cases:
            refused(sirius(h, "run", *args), 2, word, "run-sirius.sh")
        assert h.nebius_calls() == []


# Moby's default profile as Docker 29.9 vendors it (github.com/moby/profiles seccomp/v0.2.4, commit
# 245180c51918481c0525424b3ee025d2b435d46c), as the sha256 of its canonical JSON: sorted keys, no
# spaces. The Sirius profile is that and one rule more.
MOBY_DEFAULT_SHA256 = "20ef2b6bfd980e7dd56a971d9ec0d71bc0f2d1184415f2d968a19bbcff9ee2b2"
SIRIUS_CALLS = ["io_uring_setup", "io_uring_enter", "io_uring_register", "mbind", "set_mempolicy", "get_mempolicy"]


def test_the_sirius_profile_is_dockers_default_and_one_rule_for_six_calls():
    profile = json.loads((REPO / "docker" / "sirius-seccomp.json").read_text())
    assert "245180c51918481c0525424b3ee025d2b435d46c" in profile["comment"], profile["comment"]
    assert "seccomp/default.json" in profile["comment"] and "moby/profiles" in profile["comment"]
    *default_rules, added = profile["syscalls"]
    assert added["names"] == SIRIUS_CALLS and added["action"] == "SCMP_ACT_ALLOW", added
    assert set(added) == {"names", "action", "comment"}, "the rule holds no condition: no includes or excludes"
    default = {k: v for k, v in profile.items() if k != "comment"}
    default["syscalls"] = default_rules
    canonical = json.dumps(default, sort_keys=True, separators=(",", ":")).encode()
    assert hashlib.sha256(canonical).hexdigest() == MOBY_DEFAULT_SHA256, "not moby's default beside the rule"


# --- the runner: one card lock, follow, cancel -----------------------------------------------------

def test_a_sirius_run_waits_for_a_run_holding_the_card():
    with Host() as h:
        sirius_prepared(h)
        ok(h.gpu("--dir", "b", "ship"))
        h.hold.touch()
        ok(h.gpu("--dir", "b", "run", "--detach", "list"))
        until(lambda: on_card(h, "peacock-b"), "the list run holds the card")
        ok(sirius(h, "run", "--detach", "--", *RUNNER))
        until(lambda: "waiting for the card" in log_text(h), "the Sirius run waits")
        r = h.gpu("status")
        assert r.returncode == 5 and first_line(r) == "waiting", (r.returncode, r.stdout)
        time.sleep(0.5)
        assert sirius_runs(h) == [], "a Sirius container started while another run held the card"
        h.hold.unlink()
        until(lambda: ended(h), "the Sirius run ended")
        assert [SIRIUS_ID in a for a in h.docker_runs()] == [False, True], h.docker_runs()
        assert (h.vm_dir() / "run" / "status").read_text().split()[1] == "0"


def test_a_run_waits_for_a_sirius_run_holding_the_card():
    with Host() as h:
        sirius_prepared(h)
        ok(h.gpu("--dir", "b", "ship"))
        h.hold.touch()
        ok(sirius(h, "run", "--detach", "--", *RUNNER))
        until(lambda: on_card(h), "the Sirius run holds the card")
        ok(h.gpu("--dir", "b", "run", "--detach", "list"))
        until(lambda: "waiting for the card" in log_text(h, "b"), "the list run waits")
        r = h.gpu("--dir", "b", "status")
        assert r.returncode == 5 and first_line(r) == "waiting", (r.returncode, r.stdout)
        time.sleep(0.5)
        assert len(h.docker_runs()) == 1, h.docker_runs()
        h.hold.unlink()
        until(lambda: ended(h, "b"), "the list run ended")
        assert [SIRIUS_ID in a for a in h.docker_runs()] == [True, False], h.docker_runs()
        assert (h.vm_dir("b") / "run" / "status").read_text().split()[1] == "0"


def test_cancel_and_follow_reach_a_sirius_run():
    with Host() as h:
        sirius_prepared(h)
        h.world(run_output="sirius: q6 timed\n")
        h.hold.touch()
        ok(sirius(h, "run", "--detach", "--", *RUNNER))
        until(lambda: on_card(h), "the Sirius run holds the card")
        f = h.gpu_going("follow")
        time.sleep(0.5)
        assert f.poll() is None, "follow ended while the run goes"
        h.hold.unlink()
        out, err = f.communicate(timeout=30)
        assert f.returncode == 0 and "sirius: q6 timed" in out, (f.returncode, out, err)
        h.hold.touch()
        ok(sirius(h, "run", "--detach", "--", *RUNNER))
        until(lambda: on_card(h), "the second Sirius run holds the card")
        r = ok(h.gpu("cancel"))
        assert "cancelled the sirius run" in r.stdout, r.stdout
        assert log_text(h).splitlines()[-1] == "run-gpu.sh: cancelled", log_text(h)
        assert h.world()["containers"] == {}


def test_a_sirius_run_s_exit_2_is_its_own_and_never_judged_as_a_binary():
    """A staged binary's judgment makes `running 0 tests` exit 1; a Sirius run keeps the runner's 2."""
    with Host() as h:
        sirius_prepared(h)
        h.world(run_output="running 0 tests\n", run_code=2)
        refused(sirius(h, "run", "--", *RUNNER), 2)
        assert (h.vm_dir() / "run" / "status").read_text().split()[1] == "2"
        h.world(run_code=0)
        ok(sirius(h, "run", "--", *RUNNER))


# --- the stamps and pull ---------------------------------------------------------------------------

def test_a_resumed_sirius_run_keeps_the_vm_s_newer_rows_and_pull_takes_them():
    with Host() as h:
        sirius_prepared(h)
        tsv, vm_tsv = h.tree / RESULTS, h.vm_dir() / RESULTS
        tsv.parent.mkdir(parents=True)
        tsv.write_text("# rows of an earlier run\n")
        ok(sirius(h, "run", "--", *RUNNER))
        vm_tsv.write_text("# rows of an earlier run\nq6\tok\n")  # the run's rows, not pulled
        later = tsv.stat().st_mtime + 60
        os.utime(vm_tsv, (later, later))
        ok(sirius(h, "run", "--", *RUNNER, "--resume"))
        assert vm_tsv.read_text() == "# rows of an earlier run\nq6\tok\n", "the VM's newer rows were replaced"
        ok(h.gpu("pull"))
        assert tsv.read_text() == "# rows of an earlier run\nq6\tok\n"


def test_a_sirius_result_changed_on_both_sides_is_a_pull_conflict():
    with Host() as h:
        sirius_prepared(h)
        tsv, vm_tsv = h.tree / RESULTS, h.vm_dir() / RESULTS
        tsv.parent.mkdir(parents=True)
        tsv.write_text("# rows of an earlier run\n")
        ok(sirius(h, "run", "--", *RUNNER))
        vm_tsv.write_text("# rows of an earlier run\nq6\tok\n")
        tsv.write_text("# rows of an earlier run\nq1\tok\n")
        refused(h.gpu("pull"), 1, RESULTS, "changed here and on the VM")
        assert tsv.read_text() == "# rows of an earlier run\nq1\tok\n"


def test_a_resumed_run_over_a_result_changed_here_too_is_a_pull_conflict():
    """A file --update kept on the VM, newer there, is not stamped as this tree's copy: one changed
    here too is a conflict at pull, never replaced by the VM's."""
    with Host() as h:
        sirius_prepared(h)
        tsv, vm_tsv = h.tree / RESULTS, h.vm_dir() / RESULTS
        tsv.parent.mkdir(parents=True)
        tsv.write_text("# rows of an earlier run\n")
        ok(sirius(h, "run", "--", *RUNNER))
        vm_tsv.write_text("# rows of an earlier run\nq6\tok\n")
        tsv.write_text("# rows of an earlier run\nq1\tok\n")
        later = tsv.stat().st_mtime + 60
        os.utime(vm_tsv, (later, later))
        ok(sirius(h, "run", "--", *RUNNER, "--resume"))
        refused(h.gpu("pull"), 1, RESULTS, "changed here and on the VM")
        assert tsv.read_text() == "# rows of an earlier run\nq1\tok\n"


def test_a_sirius_run_s_answers_come_home_through_the_stamps():
    """A narrowed run is sent the laptop's answers, stamped; the database the run leaves is pulled
    home over the laptop's with no conflict."""
    with Host() as h:
        sirius_prepared(h)
        answers, vm_answers = h.tree / ANSWERS, h.vm_dir() / ANSWERS
        answers.parent.mkdir(parents=True)
        answers.write_bytes(b"every query's answers")
        ok(sirius(h, "run", "--", *RUNNER, "--list", "q1,q6,aggregate-groupby"))
        assert f"{sha256(answers)}  {ANSWERS}" in (h.vm_dir() / ".shipped").read_text().splitlines()
        vm_answers.write_bytes(b"q6's answer anew")
        ok(h.gpu("pull"))
        assert answers.read_bytes() == b"q6's answer anew"



def test_a_whole_run_s_answers_replace_a_local_database_the_dir_never_pulled():
    """A whole run starts its answers afresh. A local database the dir's .shipped never stamped,
    another dir's or one pulled before stamps, is this tree's as the run starts: pull takes the
    run's new one over it, exit 0."""
    with Host() as h:
        sirius_prepared(h)
        answers, vm_answers = h.tree / ANSWERS, h.vm_dir() / ANSWERS
        answers.parent.mkdir(parents=True)
        answers.write_bytes(b"another dir's answers")
        ok(sirius(h, "run", "--", *RUNNER))
        assert not vm_answers.exists(), "a whole run was sent the answers it removes"
        vm_answers.write_bytes(b"this run's answers")
        ok(h.gpu("pull"))
        assert answers.read_bytes() == b"this run's answers"


def test_a_whole_run_that_writes_no_answers_leaves_the_local_database():
    """The VM's old database and its .wal go before a whole run, as the runner removes them; a run
    that ends before it writes leaves nothing there, and pull leaves the local copy as it is."""
    with Host() as h:
        sirius_prepared(h)
        answers, vm_answers = h.tree / ANSWERS, h.vm_dir() / ANSWERS
        answers.parent.mkdir(parents=True)
        answers.write_bytes(b"this tree's answers")
        vm_answers.parent.mkdir(parents=True)
        vm_answers.write_bytes(b"an older run's answers")
        wal = vm_answers.with_name("answers.duckdb.wal")
        wal.write_bytes(b"an older run's log")
        h.world(run_code=1)
        refused(sirius(h, "run", "--", *RUNNER), 1)
        assert not vm_answers.exists() and not wal.exists(), "the VM's old answers outlived a whole run"
        ok(h.gpu("pull"))
        assert answers.read_bytes() == b"this tree's answers"


def test_a_same_day_sirius_log_from_another_dir_comes_home_with_the_run_s_lines():
    """Sirius names its log by day and appends to it. A local one the dir never pulled is sent with
    the run, so the VM's copy is it and this run's lines, which pull takes, exit 0."""
    with Host() as h:
        sirius_prepared(h)
        rel = pathlib.Path(ANSWERS).with_name("log") / "sirius_2026-11-13.log"
        log, vm_log = h.tree / rel, h.vm_dir() / rel
        log.parent.mkdir(parents=True)
        log.write_text("another dir's lines\n")
        ok(sirius(h, "run", "--", *RUNNER))
        vm_log.parent.mkdir(parents=True, exist_ok=True)
        with open(vm_log, "a") as f:  # Sirius appends to the day's file
            f.write("this run's lines\n")
        ok(h.gpu("pull"))
        assert log.read_text() == "another dir's lines\nthis run's lines\n"

def test_pull_clear_clears_a_sirius_run_s_log():
    with Host() as h:
        sirius_prepared(h)
        ok(sirius(h, "run", "--", *RUNNER))
        log = h.vm_dir() / "logs" / f"sirius-{state(h)[3]}.log"
        assert log.is_file()
        r = ok(h.gpu("pull", "--clear"))
        assert not log.exists(), r.stdout
        assert (h.laptop / f".cache/peacock/run-gpu/chain-l/{log.name}").is_file()


# --- the image and the cards -----------------------------------------------------------------------

def test_a_sirius_run_runs_the_image_by_the_id_it_checked():
    with Host() as h:
        sirius_prepared(h)
        images = h.world()["images"]
        moved = {"id": "sha256:sirius2", "labels": {**images[h.sirius_ref]["labels"]}}
        h.world(moves={h.sirius_ref: [1, moved]})
        before = len(h.docker_calls())
        ok(sirius(h, "run", "--", *RUNNER))
        calls = h.docker_calls()[before:]
        inspects = [a for a in calls if a[:2] == ["image", "inspect"] and a[-1] == h.sirius_ref]
        assert len(inspects) == 1, inspects
        (run,) = [a for a in calls if a[:1] == ["run"] and "--gpus" in a]
        assert SIRIUS_ID in run and h.sirius_ref not in run and "sha256:sirius2" not in run, run


def test_a_card_below_the_sirius_minimum_is_refused_with_exit_3():
    with Host() as h:
        sirius_prepared(h)
        for cards, words in (([[SIRIUS_DRIVER, "9.0"], ["575.0", "9.0"]], ("575.0", "580.65.06")),
                             ([["590.1", "7.5"]], ("7.5", "8.0"))):
            h.world(cards=cards)
            refused(sirius(h, "run", "--", *RUNNER), 3, *words)
        assert sirius_runs(h) == []


def test_a_sirius_run_on_a_dir_never_shipped_leaves_cpp_install_the_user_s():
    with Host() as h:
        sirius_prepared(h)
        ok(sirius(h, "--dir", "fresh", "prepare"))
        ok(sirius(h, "--dir", "fresh", "run", "--", *RUNNER))
        install = h.vm_dir("fresh") / "cpp" / "install"
        assert install.is_dir() and install.stat().st_uid == os.getuid(), install
        assert h.world().get("root_made", []) == [], "docker made a bind's source, as root"


# --- run-gpu.sh -----------------------------------------------------------------------------------

def test_run_gpu_names_run_sirius_for_the_old_verbs():
    with Host() as h:
        for args in (("sirius", "--", *RUNNER), ("prepare", "--sirius")):
            refused(h.gpu(*args), 2, "sirius moved", "scripts/env/run-sirius.sh prepare | run")
        assert h.nebius_calls() == []


def test_run_sirius_h_prints_its_usage():
    with Host() as h:
        r = ok(h.script("scripts/env/run-sirius.sh", "-h"))
        assert r.stdout.startswith("Sirius on a GPU VM"), r.stdout
        assert "prepare" in r.stdout and "run [--detach] --" in r.stdout, r.stdout


if __name__ == "__main__":
    raise SystemExit(main(globals()))
