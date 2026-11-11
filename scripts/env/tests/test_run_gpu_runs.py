"""`run-gpu.sh`: every run held by a runner on the VM, detached from the session: `follow`, `cancel`,
`--init`, and one card lock for every dir and user of the VM.

gpu_host.py has the fakes: a device run holds while `h.hold` exists. test_run_gpu.py has the verbs.
"""
import fcntl
import os
import pathlib
import signal
import subprocess
import sys
import tempfile
import time

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
sys.path.append(str(pathlib.Path(__file__).resolve().parents[2] / "exec_model" / "tests"))
from harness import main  # noqa: E402
from gpu_host import Host, alive, until  # noqa: E402
from test_run_gpu import nsys_tree  # noqa: E402

NAME = "peacock-chain-l"
STAGED = "prototype_fused_benchmarks"
BOOT = pathlib.Path("/proc/sys/kernel/random/boot_id").read_text().strip()


def ok(r):
    assert r.returncode == 0, (r.returncode, r.stdout, r.stderr)
    return r


def refused(r, code, *words):
    assert r.returncode == code, (r.returncode, r.stdout, r.stderr)
    for w in words:
        assert w in r.stderr, (w, r.stderr)


def shipped(h, *dirs):
    """Prepared, and shipped to each of <dirs> (the tree's own by default)."""
    ok(h.gpu("prepare", "--no-sf40"))
    for d in dirs or (None,):
        ok(h.gpu(*(("--dir", d) if d else ()), "ship"))
    return h


def staged(h):
    exe = h.tree / "cpp/install/rust-benchmarks" / STAGED
    exe.parent.mkdir(parents=True, exist_ok=True)
    exe.write_text("staged")
    exe.chmod(0o755)
    return shipped(h)


def state(h, d="chain-l"):
    """The dir's run/state: runner pid, boot id, what, stamp, start time."""
    return (h.vm_dir(d) / "run" / "state").read_text().split()


def log_text(h, d="chain-l"):
    _, _, what, stamp = state(h, d)[:4]
    return (h.vm_dir(d) / "logs" / f"{what}-{stamp}.log").read_text()


def on_card(h, name=NAME):
    return h.world()["containers"].get(name, {}).get("state") == "running"


def stops(h):
    return [a for a in h.docker_calls() if a[0] == "stop"]


def held(h, *args, d=None, user=None):
    """A detached run that holds the card while h.hold exists; returns once it has its container."""
    h.hold.touch()
    ok((user or h).gpu(*(("--dir", d) if d else ()), "run", "--detach", *args))
    until(lambda: on_card(h, f"peacock-{d or 'chain-l'}"), "the run's container started")


def first_line(r):
    return r.stdout.splitlines()[0] if r.stdout else ""


# --- a run outlives its session -------------------------------------------------------------------

def test_a_run_outlives_its_session_and_is_followed():
    with Host() as h:
        shipped(h)
        h.world(run_output="first line\nsecond line\n")
        h.hold.touch()
        p = h.gpu_going("run", "list")
        until(lambda: on_card(h), "the run's container started")
        os.killpg(p.pid, signal.SIGKILL)
        p.wait()
        assert alive(int(state(h)[0])), "the runner went with the session"
        refused(h.gpu("run", "list"), 2, "follow")
        f = h.gpu_going("follow")
        time.sleep(0.5)
        assert f.poll() is None, "follow ended while the run goes"
        h.hold.unlink()
        out, err = f.communicate(timeout=30)
        assert f.returncode == 0, (f.returncode, out, err)
        assert out.splitlines()[:2] == ["first line", "second line"], out
        assert len(h.docker_runs()) == 1, h.docker_runs()


def test_an_interrupted_attached_run_goes_on_and_names_follow():
    for sig, code in ((signal.SIGINT, 130), (signal.SIGTERM, 143)):
        with Host() as h:
            shipped(h)
            h.hold.touch()
            p = h.gpu_going("run", "list")
            until(lambda: on_card(h), "the run's container started")
            os.killpg(p.pid, sig)
            out, err = p.communicate(timeout=30)
            assert p.returncode == code and "follow" in err, (sig, p.returncode, out, err)
            r = h.gpu("status")
            assert r.returncode == 5 and first_line(r) == "running", (r.returncode, r.stdout, r.stderr)


def test_a_term_to_run_gpu_sh_alone_ends_the_following():
    # As a CI step's timeout or a wrapper sends it: to the script, not its process group.
    with Host() as h:
        shipped(h)
        h.hold.touch()
        for args in (("run", "list"), ("follow",)):
            p = h.gpu_going(*args)
            until(lambda: on_card(h), "the run's container started")
            time.sleep(0.5)  # the follow's ssh is under way
            os.kill(p.pid, signal.SIGTERM)
            out, err = p.communicate(timeout=10)
            assert p.returncode == 143 and "follow" in err, (args, p.returncode, out, err)
            r = h.gpu("status")
            assert r.returncode == 5 and first_line(r) == "running", (r.returncode, r.stdout, r.stderr)


def test_a_dropped_ssh_names_follow():
    with Host() as h:
        shipped(h)
        held(h, "list")
        r = h.gpu("follow", env={"FAKE_NEBIUS_RC": "255", "FAKE_NEBIUS_RC_ON": "vm_follow"})
        refused(r, 1, "follow")


def test_every_run_has_an_init():
    with Host() as h:
        staged(h)
        h.world(run_output="running 1 test\n")
        ok(h.gpu("run", "list"))
        ok(h.gpu("run", STAGED))
        runs = h.docker_runs()
        assert len(runs) == 2 and all("--init" in a for a in runs), runs


def test_run_while_the_dir_s_run_goes_is_refused_naming_follow():
    with Host() as h:
        staged(h)
        held(h, "list")
        for args in (("run", "list"), ("run", "--detach", "bench"), ("run", STAGED), ("ship",)):
            refused(h.gpu(*args), 2, "follow", "list")
        assert len(h.docker_runs()) == 1, h.docker_runs()


def test_two_runs_started_at_once_in_one_dir_start_one():
    with Host() as h:
        shipped(h)
        h.hold.touch()
        both = [h.gpu_going("run", "--detach", "list") for _ in range(2)]
        outs = [(p.wait(timeout=60), *p.communicate()) for p in both]
        assert sorted(o[0] for o in outs) == [0, 2], outs
        assert "follow" in next(o for o in outs if o[0] == 2)[2], outs
        until(lambda: on_card(h), "the one run's container started")
        assert len(h.docker_runs()) == 1, h.docker_runs()


def test_a_run_waits_while_its_dir_s_run_is_being_started():
    with Host() as h:
        shipped(h)
        (h.vm_dir() / "run").mkdir(exist_ok=True)
        with open(h.vm_dir() / "run" / "lock", "a") as lock:  # another call starting a run here
            fcntl.flock(lock, fcntl.LOCK_EX)
            p = h.gpu_going("run", "--detach", "list")
            time.sleep(1)
            assert p.poll() is None and not (h.vm_dir() / "run" / "state").exists() and h.docker_runs() == []
        out, err = p.communicate(timeout=30)
        assert p.returncode == 0 and "waiting for ~/peacock/chain-l/run/lock" in err, (p.returncode, out, err)
        until(lambda: len(h.docker_runs()) == 1, "the run started once its dir was let go")


def test_a_run_from_before_a_restart_is_not_going():
    with Host() as h:
        shipped(h)
        stamp = "20000101T000000.000Z"
        (h.vm_dir() / "run").mkdir(exist_ok=True)
        (h.vm_dir() / "logs").mkdir(exist_ok=True)
        # A live process, its start time too, under the boot id of another: the runner's, before the VM
        # restarted. Only the boot id tells them apart.
        start = open(f"/proc/{os.getpid()}/stat").read().rsplit(") ", 1)[1].split()[19]
        (h.vm_dir() / "run" / "state").write_text(
            f"{os.getpid()} 00000000-0000-0000-0000-000000000000 list {stamp} {start}\n")
        (h.vm_dir() / "logs" / f"list-{stamp}.log").write_text("a line before the restart\n")
        h.world(containers={NAME: {"state": "exited", "code": 137}})  # docker keeps it across the restart
        r = h.gpu("status")
        assert r.returncode == 1 and first_line(r) == "ended without a status", (r.returncode, r.stdout)
        assert "a line before the restart" in r.stdout, r.stdout
        refused(h.gpu("follow"), 1, "no status")
        ok(h.gpu("run", "list"))
        assert len(h.docker_runs()) == 1 and h.world()["containers"] == {}


def test_a_runner_pid_another_process_took_is_not_going():
    with Host() as h:
        shipped(h)
        stamp = "20000101T000000.000Z"
        (h.vm_dir() / "run").mkdir(exist_ok=True)
        (h.vm_dir() / "logs").mkdir(exist_ok=True)
        other = subprocess.Popen(["sleep", "60"], start_new_session=True)
        try:
            # This boot's id and a live pid, but a start time not that process's: the runner died and
            # its pid went to another process.
            (h.vm_dir() / "run" / "state").write_text(f"{other.pid} {BOOT} list {stamp} 1\n")
            (h.vm_dir() / "logs" / f"list-{stamp}.log").write_text("a line of the dead run\n")
            r = h.gpu("status")
            assert r.returncode == 1 and first_line(r) == "ended without a status", (r.returncode, r.stdout)
            refused(h.gpu("follow"), 1, "no status")
            r = ok(h.gpu("cancel"))
            assert "no run" in r.stdout and other.poll() is None, (r.stdout, other.poll())
            ok(h.gpu("run", "list"))
        finally:
            other.kill()
            other.wait()


def test_a_run_that_ends_before_it_is_followed_gives_its_status():
    with Host() as h:
        shipped(h)
        h.world(run_code=1)
        assert h.gpu("run", "list").returncode == 1
        r = h.gpu("follow")
        assert r.returncode == 1 and "run-tests.sh list: green" in r.stdout, (r.returncode, r.stdout, r.stderr)
        h.world(run_code=2)
        assert h.gpu("run", "--detach", "list").returncode == 0
        until(lambda: (h.vm_dir() / "run" / "status").is_file()
              and (h.vm_dir() / "run" / "status").read_text().split()[0] == state(h)[3], "the run ended")
        assert h.gpu("follow").returncode == 2


def test_follow_and_status_with_no_run_name_status():
    with Host() as h:
        shipped(h)
        refused(h.gpu("follow"), 2, "no run", "status")
        r = h.gpu("status")
        assert r.returncode == 2 and first_line(r) == "none", (r.returncode, r.stdout)


# --- cancel -------------------------------------------------------------------------------------

def test_cancel_stops_the_run_and_keeps_what_an_ended_run_keeps():
    with Host() as h:
        shipped(h)
        h.hold.touch()
        run = h.gpu_going("run", "list")
        until(lambda: on_card(h), "the run's container started")
        stamp = state(h)[3]
        r = ok(h.gpu("cancel"))
        assert "cancelled" in r.stdout and stamp in r.stdout, r.stdout
        assert stops(h) and stops(h)[0] == ["stop", "-t", "30", NAME], stops(h)
        assert {a[-1] for a in stops(h)} == {NAME}, stops(h)
        calls = h.docker_calls()
        assert calls.index(stops(h)[0]) < max(i for i, a in enumerate(calls) if a[0] == "rm" and a[-1] == NAME)
        assert h.world()["containers"] == {}
        assert log_text(h).splitlines()[-1] == "run-gpu.sh: cancelled", log_text(h)
        r = h.gpu("status")
        assert r.returncode == 1 and first_line(r) == "exited 143", (r.returncode, r.stdout)
        out, err = run.communicate(timeout=30)
        assert run.returncode == 1, (run.returncode, out, err)
        before = len(stops(h))
        r = ok(h.gpu("cancel"))
        assert "no run" in r.stdout and len(stops(h)) == before, r.stdout


def test_a_cancel_as_the_runner_starts_still_cancels():
    with Host() as h:
        shipped(h)
        h.hold.touch()
        h.world(info_delays=[0, 0, 1.5])  # the image read's docker call, the start's, then the runner's
        ok(h.gpu("run", "--detach", "list"))
        r = ok(h.gpu("cancel"))
        assert "cancelled the list run" in r.stdout, r.stdout
        assert (h.vm_dir() / "run" / "status").read_text().split()[1] == "143"
        assert log_text(h).splitlines()[-1] == "run-gpu.sh: cancelled", log_text(h)
        assert h.docker_runs() == []


def test_cancel_stops_a_container_its_client_is_still_making():
    with Host() as h:
        shipped(h)
        h.world(create_delay=1.5)
        h.hold.touch()
        ok(h.gpu("run", "--detach", "list"))
        until(lambda: h.docker_runs(), "the client started making the container")
        ok(h.gpu("cancel"))
        assert h.world()["containers"] == {}
        assert not alive(int(state(h)[0]))
        assert (h.vm_dir() / "run" / "status").read_text().split()[1] == "143"


def test_cancel_ends_a_run_still_waiting_for_the_card():
    with Host() as h:
        shipped(h, "chain-l", "b")
        held(h, "list", d="b")
        ok(h.gpu("run", "--detach", "list"))
        until(lambda: "waiting for the card" in log_text(h), "this run waits")
        r = h.gpu("status")
        assert r.returncode == 5 and first_line(r) == "waiting", (r.returncode, r.stdout)
        pid = int(state(h)[0])
        r = ok(h.gpu("cancel"))
        assert "cancelled the list run" in r.stdout and stops(h) == [], (r.stdout, stops(h))
        assert not alive(pid), "the runner outlived the cancel"
        assert log_text(h).splitlines()[-2:] == [f"waiting for the card: another run holds {h.card_lock}",
                                                 "run-gpu.sh: cancelled"], log_text(h)
        assert (h.vm_dir() / "run" / "status").read_text().split()[1] == "143"
        assert on_card(h, "peacock-b"), "the run on the card was stopped"
        h.hold.unlink()
        until(lambda: not on_card(h, "peacock-b"), "the run on the card ended")
        assert len(h.docker_runs()) == 1, h.docker_runs()


def test_cancel_removes_a_container_a_killed_runner_left():
    with Host() as h:
        shipped(h)
        held(h, "list")
        os.killpg(int(state(h)[0]), signal.SIGKILL)  # the runner and its client, the container left
        until(lambda: not alive(int(state(h)[0])), "the runner died")
        r = ok(h.gpu("cancel"))
        assert "removed" in r.stdout and h.world()["containers"] == {}, r.stdout
        h.hold.unlink()
        ok(h.gpu("run", "list"))
        h.world(containers={NAME: {"state": "running", "code": None}}, vanish_on_rm=True)
        r = ok(h.gpu("cancel"))
        assert "removed" in r.stdout and h.world()["containers"] == {}, r.stdout


def test_a_container_of_the_dir_still_on_the_vm_refuses_a_new_run():
    with Host() as h:
        shipped(h)
        h.world(containers={NAME: {"state": "running", "code": None}})
        refused(h.gpu("run", "list"), 2, NAME, "cancel")
        refused(h.gpu("ship"), 2, NAME, "cancel")
        assert h.docker_runs() == []
        ok(h.gpu("cancel"))
        ok(h.gpu("run", "list"))
        assert len(h.docker_runs()) == 1


def test_cancel_with_nothing_going_says_so():
    with Host() as h:
        r = ok(h.gpu("cancel"))
        assert "no run of chain-l goes" in r.stdout, r.stdout
        shipped(h)
        ok(h.gpu("run", "list"))
        r = ok(h.gpu("cancel"))
        assert "no run of chain-l goes" in r.stdout and stops(h) == [], r.stdout


def test_cancel_leaves_other_dirs_alone():
    with Host() as h:
        shipped(h, "chain-l", "b")
        other = shipped(h.second_user())
        held(h, "list", user=other)  # another user's dir of this one's name: its container has this name
        ok(h.gpu("--dir", "b", "run", "--detach", "list"))
        until(lambda: "waiting" in log_text(h, "b"), "this user's other dir waits")
        r = ok(h.gpu("cancel"))
        assert "no run" in r.stdout and stops(h) == [], (r.stdout, stops(h))
        assert on_card(h) and alive(int(state(other)[0])) and alive(int(state(h, "b")[0]))
        h.hold.unlink()
        for runner, d in ((other, "chain-l"), (h, "b")):
            status = runner.vm_dir(d) / "run" / "status"
            until(lambda: status.is_file() and status.read_text().split()[0] == state(runner, d)[3], "both ended")
            assert status.read_text().split()[1] == "0", (d, status.read_text())


def test_another_dir_s_container_of_this_name_refuses_a_run_and_is_kept():
    for left in ("exited", "running"):
        with Host() as h:
            shipped(h)
            other = shipped(h.second_user())
            held(h, "list", user=other)
            if left == "exited":
                h.hold.unlink()
                until(lambda: (other.vm_dir() / "run" / "status").is_file(), "the other run ended")
                w = h.world()
                w["containers"][NAME] = {"state": "exited", "code": 0,
                                         "labels": {"peacock.dir": str(other.vm_dir())}}  # its runner killed after
                h.world(containers=w["containers"])
            else:
                os.killpg(int(state(other)[0]), signal.SIGKILL)  # its runner and client; the container runs on
                until(lambda: not alive(int(state(other)[0])), "the other runner died")
            r = h.gpu("run", "list")
            assert r.returncode == 2 and str(other.vm_dir()) in r.stdout + r.stderr, (left, r.returncode, r.stdout,
                                                                                       r.stderr)
            assert h.world()["containers"][NAME]["state"] == left, (left, h.world()["containers"])
            assert len(h.docker_runs()) == 1, h.docker_runs()


def test_ship_beside_another_dir_s_ended_container_of_this_name_ships_and_keeps_it():
    with Host() as h:
        shipped(h)
        other = h.second_user()
        h.world(containers={NAME: {"state": "exited", "code": 0, "labels": {"peacock.dir": str(other.vm_dir())}}})
        ok(h.gpu("ship"))
        assert h.world()["containers"][NAME]["state"] == "exited", h.world()["containers"]


# --- one card lock -----------------------------------------------------------------------------

def test_one_card_lock_for_every_dir_and_user():
    with Host() as h:
        shipped(h, "chain-l", "b")
        other = shipped(h.second_user())
        held(h, "list")
        # The other user's dir has this dir's name, so its container is named as this one's.
        for runner, d in ((h, "b"), (other, None)):
            ok(runner.gpu(*(("--dir", d) if d else ()), "run", "--detach", "list"))
            until(lambda: "waiting for the card" in log_text(runner, d or "chain-l"), "a second run waits")
            r = runner.gpu(*(("--dir", d) if d else ()), "status")
            assert r.returncode == 5 and first_line(r) == "waiting", (r.returncode, r.stdout)
        time.sleep(0.5)
        assert len(h.docker_runs()) == 1, h.docker_runs()
        h.hold.unlink()
        for runner, d in ((h, "chain-l"), (h, "b"), (other, "chain-l")):
            status = runner.vm_dir(d) / "run" / "status"
            until(lambda: status.is_file() and status.read_text().split()[0] == state(runner, d)[3], "each run ended")
            assert status.read_text().split()[1] == "0", (d, status.read_text())
        assert len(h.docker_runs()) == 3, h.docker_runs()


def test_the_card_lock_is_mode_666():
    with Host() as h:
        shipped(h)
        old = os.umask(0o022)
        try:
            ok(h.gpu("run", "list"))
        finally:
            os.umask(old)
        assert h.card_lock.stat().st_mode & 0o777 == 0o666, oct(h.card_lock.stat().st_mode)


def test_a_staged_binary_runs_under_the_runner_and_is_judged():
    with Host() as h:
        staged(h)
        h.world(run_output="running 1 test\nSegmentation fault\n")
        r = h.gpu("run", STAGED)
        assert r.returncode == 1 and "segfault" in r.stdout, (r.returncode, r.stdout, r.stderr)
        assert state(h)[2] == STAGED and "segfault" in log_text(h)
        r = h.gpu("status")
        assert r.returncode == 1 and first_line(r) == "exited 1", (r.returncode, r.stdout)


def test_nsys_capture_waits_for_the_card_lock():
    with tempfile.TemporaryDirectory() as tmp:
        tmp = pathlib.Path(tmp)
        tree, vm, env = nsys_tree(tmp)
        env["PCK_TEST_FILTER"] = "bench_tpch_sf40_q6_tp1_single"
        with open(env["PEACOCK_CARD_LOCK"], "a") as lock:  # a run on the card
            fcntl.flock(lock, fcntl.LOCK_EX)
            capture = subprocess.Popen(
                ["bash", "scripts/create_nsys_profile.sh", "--trace", "--ssh", f"{tmp / 'rsh'} --vm h100",
                 "--host", "nebius-h100", "--remote-dir", str(vm), "--remote-cudf-root", str(tmp / "cudf")],
                cwd=tree, env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, start_new_session=True)
            try:
                time.sleep(1.5)
                assert capture.poll() is None, capture.communicate()
                assert not list((tree / "testdata").rglob("*.nsys-rep")) and not list(vm.rglob("*.nsys-rep"))
            except BaseException:
                if capture.poll() is None:
                    os.killpg(capture.pid, signal.SIGKILL)
                raise
        out, err = capture.communicate(timeout=60)
        assert capture.returncode == 0, (capture.returncode, out, err)
        assert "waiting for the card" in out and "the trace pass ran 1 tests" in out, out


if __name__ == "__main__":
    raise SystemExit(main(globals()))
