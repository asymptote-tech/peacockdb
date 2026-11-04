"""watch.sh, the hourly checker's body for one VM, over the real nebius.sh and fake nebius/ssh."""
import json, os, pathlib, subprocess, sys, time
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
from fakes import World  # noqa: E402
from harness import main  # noqa: E402

WATCH = str(pathlib.Path(__file__).resolve().parents[1] / "watch.sh")
H100 = "computeinstance-e00e6yena4nzjgaym0"
CPU = "computeinstance-e01t2mm4265y8x88ey"
CPU_OPTS = ["--instance-id", CPU, "--profile", "eu-west1"]
H = 3600


def watcher_line(t, **kv):
    base = {"t": t, "boot": t - 5 * H, "card": [], "tests": False, "test_procs": [], "build": False,
            "last_test": t - 600, "last_build": t - 5 * H, "decision": "keep", "reason": "",
            "sessions": [], "instance": CPU, "kind": "cpu"}
    return json.dumps({**base, **kv})


def run(world, *args, env=None):
    # A 2-second ssh window, no pause between tries: the 15-minute default is for CI.
    p = subprocess.run([WATCH, *args], capture_output=True, text=True, timeout=60, stdin=subprocess.DEVNULL,
                       env={**world.env(), "GPU_WATCH_WINDOW": "2", "GPU_WATCH_PAUSE": "0", **(env or {})})
    return p.returncode, p.stdout, p.stderr


def nebius_calls(w):
    return [c for c in w.read()["calls"] if c[0] == "nebius"]


def stopped(w):
    return any(c[3:4] == ["stop"] for c in nebius_calls(w))


def test_watch_is_executable():
    # The workflow runs it by path.
    assert os.access(WATCH, os.X_OK), WATCH


def test_watch_stopped_vm_is_a_no_op():
    w = World(state="STOPPED")
    rc, out, err = run(w, "nebius-gpu", "--profile", "ci")
    assert rc == 0, (rc, out, err)
    assert "not running; nothing to do" in out, out
    assert not stopped(w)


def test_watch_gone_instance_is_a_notice():
    w = World(exists=False)
    rc, out, err = run(w, "nebius-h100", "--instance-id", H100, "--profile", "ci")
    assert rc == 0, (rc, out, err)
    assert "::notice::nebius-h100: instance no longer exists; skipped" in out, out
    assert not stopped(w)


def test_watch_status_failure_is_1():
    w = World(get_error="Error: rpc error: code = Unavailable desc = try later")
    rc, out, err = run(w, "nebius-cpu128", *CPU_OPTS)
    assert rc == 1, (rc, out, err)
    assert "::error::nebius-cpu128: status failed" in out, out
    assert not stopped(w)


def test_watch_fresh_keep_line_keeps():
    w = World(state="RUNNING", watch_log=watcher_line(time.time() - 60))
    rc, out, err = run(w, "nebius-cpu128", *CPU_OPTS)
    assert rc == 0, (rc, out, err)
    assert "in use; keeping it" in out, out
    assert not stopped(w) and w.read()["state"] == "RUNNING"


def test_watch_stale_line_hard_stops():
    w = World(state="RUNNING", watch_log=watcher_line(time.time() - 2 * H))
    rc, out, err = run(w, "nebius-cpu128", *CPU_OPTS)
    assert rc == 0, (rc, out, err)
    assert "::warning::hard-stopping nebius-cpu128: the watcher's log is stale" in out, out
    assert w.read()["state"] == "STOPPED"


def test_watch_no_log_hard_stops():
    w = World(state="RUNNING", watch_log="")
    rc, out, err = run(w, "nebius-h100", "--instance-id", H100, "--profile", "ci")
    assert rc == 0, (rc, out, err)
    assert "::warning::hard-stopping nebius-h100: no watcher log on the VM" in out, out
    assert w.read()["state"] == "STOPPED"


def test_watch_ssh_down_hard_stops():
    w = World(state="RUNNING", ssh="down")
    rc, out, err = run(w, "nebius-cpu128", *CPU_OPTS)
    assert rc == 0, (rc, out, err)
    assert "::warning::hard-stopping nebius-cpu128: ssh unanswered for 2s" in out, out
    assert w.read()["state"] == "STOPPED"


def test_watch_young_vm_waits():
    # Its watcher may not have ticked yet: the log's last line would be the last session's.
    w = World(state="RUNNING", uptime="300", watch_log=watcher_line(time.time() - 2 * H))
    rc, out, err = run(w, "nebius-cpu128", *CPU_OPTS)
    assert rc == 0, (rc, out, err)
    assert "up 300s, under 10 minutes; keeping it" in out, out
    assert not stopped(w)


def test_watch_clock_offset_shifts_the_checker():
    w = World(state="RUNNING", watch_log=watcher_line(time.time() - 60))
    rc, out, err = run(w, "nebius-gpu", "--profile", "ci", env={"CLOCK_OFFSET": "7200"})
    assert rc == 0, (rc, out, err)
    assert "::notice::the checker's clock is shifted by 7200 s" in out, out
    assert "::warning::hard-stopping nebius-gpu:" in out, out
    assert w.read()["state"] == "STOPPED"


def test_watch_reason_is_one_line():
    # The reason carries the VM's own log text; a newline in it could start a workflow command.
    w = World(state="RUNNING", watch_log=watcher_line(time.time() - 60, decision="idle-stop",
                                                        reason="x\n::error::forged"))
    rc, out, err = run(w, "nebius-cpu128", *CPU_OPTS)
    assert rc == 0, (rc, out, err)
    assert not any(l.startswith("::error::") for l in out.splitlines()), out
    assert w.read()["state"] == "STOPPED"


def test_watch_failed_stop_is_1():
    w = World(state="RUNNING", ssh="down", stop_error="Error: rpc error: code = Internal desc = boom")
    rc, out, err = run(w, "nebius-cpu128", *CPU_OPTS)
    assert rc == 1, (rc, out, err)
    assert "::warning::hard-stopping nebius-cpu128" in out, out
    assert "::error::nebius-cpu128: hard stop failed (rc=1)" in out, out


def test_watch_passes_instance_and_profile():
    w = World(state="RUNNING", watch_log=watcher_line(time.time() - 2 * H))
    rc, out, err = run(w, "nebius-cpu128", *CPU_OPTS)
    assert rc == 0, (rc, out, err)
    calls = nebius_calls(w)
    assert stopped(w) and len(calls) > 2, calls
    for c in calls:
        assert any(a == "--id" and b == CPU for a, b in zip(c, c[1:])), c
        assert any(a == "--profile" and b == "eu-west1" for a, b in zip(c, c[1:])), c


def test_watch_bad_clock_offset_is_1():
    for bad in ("-5", "1e3", "007", "1234567", "1 2"):
        w = World(state="RUNNING")
        rc, out, err = run(w, "nebius-gpu", "--profile", "ci", env={"CLOCK_OFFSET": bad})
        assert rc == 1, (bad, rc, out, err)
        assert "::error::" in out and "clock_offset" in out, (bad, out)
        assert not w.read()["calls"], (bad, w.read()["calls"])


def test_watch_without_a_vm_name_is_64():
    w = World(state="RUNNING")
    rc, out, err = run(w)
    assert rc == 64, (rc, out, err)
    assert not w.read()["calls"]


if __name__ == "__main__":
    sys.exit(main(globals()))
