"""gpu.sh: the host's locks: the activity hold and its heartbeat, an instance's run lock, a ship's
rsync servers holding it, and a busy instance. The rig is test_gpu.py's (`gpuhost.py`).
"""
import contextlib
import fcntl
import json
import os
import pathlib
import re
import signal
import subprocess
import sys
import time

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
sys.path.append(str(pathlib.Path(__file__).resolve().parents[2] / "exec_model" / "tests"))
from harness import main  # noqa: E402
from gpuhost import (HOSTNAME, INSTANCE, NAME, USER, GpuRig, device_runs, install, ok, prepared,  # noqa: E402
                     refused, ship_dir, shipped, stderr_line, until)
from machine import REPO, flag_values, path_without  # noqa: E402


def test_a_run_whose_instance_is_busy_is_neither_started_nor_followed():
    with GpuRig() as rig:
        shipped(rig)
        ok(rig.gpu(NAME, "run", "list"))
        state = ship_dir(rig) / "run" / "state"
        lock = rig.data_dir() / "runs" / f"{USER}-{INSTANCE}.lock"
        with open(lock, "a") as held:  # another session of this instance, a ship's or a run's being started
            fcntl.flock(held, fcntl.LOCK_EX)
            r = rig.gpu(NAME, "run", "list")
            refused(r, "busy")
            r = rig.gpu(NAME, "ship")
            refused(r, "busy")
        assert len(device_runs(rig)) == 1 and state.is_file()


def test_a_container_of_the_instance_still_on_the_host_refuses_a_new_run():
    with GpuRig() as rig:
        shipped(rig)
        (rig.host / "containers").mkdir()
        (rig.host / "containers" / f"peacock-run-{USER}-{INSTANCE}").write_text("")
        refused(rig.gpu(NAME, "run", "list"), f"peacock-run-{USER}-{INSTANCE}")
        assert device_runs(rig) == []
        (rig.host / "containers" / f"peacock-run-{USER}-{INSTANCE}").unlink()
        ok(rig.gpu(NAME, "run", "list"))
        assert flag_values(device_runs(rig)[0], "--name") == [f"peacock-run-{USER}-{INSTANCE}"]


def test_a_hold_whose_caller_falls_silent_lets_go():
    with GpuRig() as rig:
        prepared(rig)
        script = (ship_dir(rig) / "scripts/env/on-gpu-host.sh").read_bytes()
        # As gpu.sh sends it: the script on stdin, then the heartbeat on the same stream, here never.
        hold = subprocess.Popen(["bash", "-s", "--", "hold", "1"], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                text=False, cwd=rig.host_home(),
                                env={**rig.on_host_env(), "PATH": rig.on_host_env()["PATH"]})
        hold.stdin.write(script)
        hold.stdin.flush()
        assert hold.stdout.readline() == b"held\n"
        assert hold.wait(timeout=10) == 0
        hold.stdin.close()
        lock = rig.data_dir() / ".activity.lock"
        with open(lock) as f:
            fcntl.flock(f, fcntl.LOCK_EX | fcntl.LOCK_NB)
        hold = rig.on_host("hold", "1", stdin=subprocess.PIPE, background=True)
        assert hold.stdout.readline() == "held\n"
        assert hold.wait(timeout=10) == 0, hold.communicate()
        hold.stdin.close()
        lock = rig.data_dir() / ".activity.lock"
        with open(lock) as f:
            fcntl.flock(f, fcntl.LOCK_EX | fcntl.LOCK_NB)


def test_a_command_longer_than_the_hold_s_timeout_keeps_it_by_its_heartbeat():
    with GpuRig() as rig:
        shipped(rig)
        beat = {"PEACOCK_HOLD_BEAT": "1", "PEACOCK_HOLD_TIMEOUT": "2"}
        (rig.host / "hold_smi").write_text("")
        before = len(rig.host_calls(rig.host, "nvidia-smi"))
        run = rig.gpu(NAME, "run", "list", env=beat, background=True)
        until(lambda: len(rig.host_calls(rig.host, "nvidia-smi")) > before, "the run reads the driver")
        time.sleep(3.5)
        busy = rig.on_host("busy")
        assert busy.stdout.strip() == "busy: a gpu.sh step is running on the host", busy
        (rig.host / "hold_smi").unlink()
        assert run.wait(timeout=60) == 0, run.communicate()


def test_a_gpu_sh_killed_alone_lets_go_of_its_hold():
    with GpuRig() as rig:
        shipped(rig)
        beat = {"PEACOCK_HOLD_BEAT": "1", "PEACOCK_HOLD_TIMEOUT": "2"}
        (rig.host / "hold_smi").write_text("")
        before = len(rig.host_calls(rig.host, "nvidia-smi"))
        run = rig.gpu(NAME, "run", "list", env=beat, background=True)
        try:
            until(lambda: len(rig.host_calls(rig.host, "nvidia-smi")) > before, "the run reads the driver")
            assert rig.on_host("busy").stdout.strip() == "busy: a gpu.sh step is running on the host"
            os.kill(run.pid, signal.SIGKILL)  # the pid alone, as an OOM kill or a supervisor's timeout does
            run.wait()
            until(lambda: rig.on_host("busy").stdout.strip() == "idle", "the orphaned hold lets go", seconds=20)
            (rig.host / "hold_smi").unlink()
            ok(rig.gpu(NAME, "run", "list"))
        finally:
            try:
                os.killpg(run.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass


def test_a_second_user_s_first_prepare_joins_the_group_before_it_holds():
    with GpuRig() as rig:
        (rig.host / "groups").write_text(f"{USER}\n")
        rig.data_dir().chmod(0o555)  # another user's directory, writable only through the docker group
        try:
            ok(rig.gpu(NAME, "prepare"))
        finally:
            rig.data_dir().chmod(0o2775)
        assert f"usermod -aG docker {USER}" in rig.host_calls(rig.host, "sudo")
        assert (rig.data_dir() / rig.data_hash() / "tpch.sf1").is_dir()
        holds = [c for c in rig.calls("ssh") if c["argv"][-1].startswith("bash -s -- hold ")]
        assert len(holds) == 2, "unheld before the install, held again in a session that has the group"


def test_a_run_during_a_ship_is_busy():
    with GpuRig() as rig:
        shipped(rig)
        gate = rig.root / "rsync-gate"
        gate.write_text("")
        before = len(rig.calls("rsync"))
        ship = rig.gpu(NAME, "ship", env={"FAKE_RSYNC_GATE": str(gate)}, background=True)
        try:
            until(lambda: len(rig.calls("rsync")) > before, "the ship's rsync starts")
            refused(rig.gpu(NAME, "run", "list"), "busy")
            assert device_runs(rig) == [], "no run on binaries half shipped"
        finally:
            gate.unlink()
        assert ship.wait(timeout=60) == 0, ship.communicate()
        ok(rig.gpu(NAME, "run", "list"))


def test_a_lock_held_without_a_live_unfinished_run_is_busy():
    with GpuRig() as rig:
        shipped(rig)
        ok(rig.gpu(NAME, "run", "list"))
        state = ship_dir(rig) / "run" / "state"
        pid, mode, log, stamp, digest, envhash = state.read_text().split()
        dead = subprocess.Popen(["true"])
        dead.wait()
        unfinished = pathlib.Path(log).with_name("list-20000101T000000.000Z.log")
        unfinished.write_text("a run that never wrote its status\n")
        lock = rig.data_dir() / "runs" / f"{USER}-{INSTANCE}.lock"
        cases = ((str(os.getpid()), log), (str(dead.pid), str(unfinished)))  # alive and ended; dead with no status
        for pid_, log_ in cases:
            state.write_text(f"{pid_} {mode} {log_} {stamp} {digest} {envhash}\n")
            with open(lock, "a") as held:
                fcntl.flock(held, fcntl.LOCK_EX)
                refused(rig.gpu(NAME, "run", "list"), "busy")
        assert len(device_runs(rig)) == 1


def test_the_portable_timeout_says_124_and_passes_the_command_s_own_status():
    with GpuRig() as rig:
        (rig.root / "plain").mkdir()
        system = path_without(rig.root / "plain", {"timeout", "gtimeout"})
        script = f'. "{REPO}/scripts/lib/portable.sh"; type timeout | head -1; '
        for call, want in (("timeout 1 sleep 5", 124), ("timeout 37 sh -c 'exit 7'", 7)):
            start = time.time()
            r = subprocess.run(["bash", "-c", script + f"{call}; echo status $?"], env={"PATH": str(system)},
                               capture_output=True, text=True, timeout=30)
            assert "function" in r.stdout and f"status {want}" in r.stdout, (call, r.stdout, r.stderr)
            assert time.time() - start < 4, call
        assert sleeps_of("37") == [], "the watcher's sleep outlived the command it watched"


def test_a_hold_lost_mid_command_is_named():
    with GpuRig() as rig:
        shipped(rig)
        (rig.host / "hold_smi").write_text("")
        before = len(rig.host_calls(rig.host, "nvidia-smi"))
        run = rig.gpu(NAME, "run", "list", background=True)
        until(lambda: len(rig.host_calls(rig.host, "nvidia-smi")) > before, "the run reads the driver")
        home = str(rig.host_home())
        holds = [int(pid) for pid in os.listdir("/proc") if pid.isdigit() and _holds(pid, home)]
        assert len(holds) == 1, holds
        os.kill(holds[0], signal.SIGKILL)
        (rig.host / "hold_smi").unlink()
        out, err = run.communicate(timeout=60)
        assert run.returncode == 0 and "green" in out, (run.returncode, err)
        assert "the hold on l40s ended early" in err, err


def _holds(pid, home):
    """Whether process <pid> is a host-side hold session of the host whose home is <home>."""
    try:
        return (open(f"/proc/{pid}/cmdline").read().split("\0")[:4] == ["bash", "-s", "--", "hold"]
                and os.readlink(f"/proc/{pid}/cwd") == home)
    except OSError:
        return False


def sleeps_of(seconds):
    """The pids of this user's `sleep <seconds>` processes."""
    found = []
    for pid in os.listdir("/proc"):
        try:
            if pid.isdigit() and open(f"/proc/{pid}/cmdline").read().split("\0")[:2] == ["sleep", seconds]:
                found.append(pid)
        except OSError:
            pass
    return found


def server_argv(rig, index):
    """The command real rsync has its remote shell run for the fake rsync's call <index>, user and host
    dropped: the argv of the rsync server that call starts on a real host. Replayed under the call's own
    RSYNC_PROTECT_ARGS, which takes the paths out of that argv."""
    shim = rig.root / "server-argv"
    shim.write_text('#!/bin/sh\n[ "$1" = -l ] && shift 2\nshift\necho "$@" > "$0.out"\nexit 1\n')
    shim.chmod(0o755)
    args = list(rig.calls("rsync")[index])
    args[args.index("-e") + 1] = str(shim)
    args = [re.sub(r"^[^@/:]+@[^:/]+:", "u@h:", a) for a in args]
    protect = json.loads(pathlib.Path(f"{rig.logs['rsync']}.env").read_text().splitlines()[index])
    env = {k: v for k, v in os.environ.items() if k != "RSYNC_PROTECT_ARGS"}
    if protect is not None:
        env["RSYNC_PROTECT_ARGS"] = protect
    subprocess.run(["rsync", *args], capture_output=True, timeout=30, cwd=rig.tree, env=env)
    return pathlib.Path(f"{shim}.out").read_text().split()


STAND_IN = """#!/bin/sh
# A host's rsync server, held while its gate exists.
while [ -e "$(dirname "$0")/gate" ]; do sleep 0.05; done
"""


def stand_in(rig, argv):
    """A process whose argv is <argv>, as a host's rsync server's is, until its gate goes or it is killed."""
    d = rig.root / f"stand-in-{len(list(rig.root.glob('stand-in-*')))}"
    d.mkdir()
    install(d / argv[0], STAND_IN)
    (d / "gate").write_text("")
    return subprocess.Popen([str(d / argv[0]), *argv[1:]], start_new_session=True), d / "gate"


def laptop_side(rig, pgid):
    """Process group <pgid> but for the host's hold session and its children: what a laptop that
    vanished takes with it, the host's side of the hold left as it is."""
    procs = {}
    for pid in os.listdir("/proc"):
        try:
            if pid.isdigit():
                fields = open(f"/proc/{pid}/stat").read().rsplit(")", 1)[1].split()
                procs[int(pid)] = (int(fields[1]), int(fields[2]))
        except OSError:
            pass
    holds = {pid for pid in procs if _holds(pid, str(rig.host_home()))}

    def held(pid):
        while pid > 1 and pid not in holds:
            pid = procs.get(pid, (0, 0))[0]
        return pid in holds
    return [pid for pid, (_, group) in procs.items() if group == pgid and not held(pid)]


def vanished(rig, *, timeout):
    """A ship whose laptop vanished mid-rsync: its gpu.sh and rsync gone, a stand-in of the rsync server
    it started left on the host. The laptop has RSYNC_PROTECT_ARGS=1, which ship overrides.
    Returns the ship, the stand-in and its gate."""
    gate = rig.root / "rsync-gate"
    gate.write_text("")
    before = len(rig.calls("rsync"))
    env = {"PEACOCK_HOLD_BEAT": "1", "PEACOCK_HOLD_TIMEOUT": timeout, "FAKE_RSYNC_GATE": str(gate),
           "RSYNC_PROTECT_ARGS": "1"}
    ship = rig.gpu(NAME, "ship", env=env, background=True)
    until(lambda: len(rig.calls("rsync")) > before, "the ship's rsync starts")
    argv = server_argv(rig, before)
    assert argv[:2] == ["rsync", "--server"] and argv[-2:] == [".", f"peacock/{INSTANCE}/cpp/install/"], argv
    assert f"--timeout={timeout}" in argv, argv
    server, server_gate = stand_in(rig, argv)
    for pid in laptop_side(rig, ship.pid):
        with contextlib.suppress(ProcessLookupError):  # the heartbeat's sleep may end in between
            os.kill(pid, signal.SIGKILL)
    ship.wait()
    gate.unlink()
    return ship, server, server_gate


def ended(*processes):
    for process in processes:
        for kill in (os.killpg, os.kill):
            try:
                kill(process.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
        process.wait()


def test_a_ship_killed_alone_keeps_its_locks_while_the_host_s_rsync_server_writes():
    with GpuRig() as rig:
        shipped(rig)
        ok(rig.gpu(NAME, "run", "list"))  # a state, so the instance's probe answers at once
        ship, server, gate = vanished(rig, timeout="2")
        try:
            # The hold's stdin ended with the fake ssh: time for a hold that does not wait on the server
            # to let go, so that its not waiting shows.
            time.sleep(4)
            refused(rig.gpu(NAME, "run", "list"), "busy")
            assert rig.on_host("busy").stdout.strip() == "busy: a gpu.sh step is running on the host"
            assert len(device_runs(rig)) == 1, "no run on binaries half shipped"
            gate.unlink()
            server.wait(timeout=10)
            until(lambda: rig.on_host("busy").stdout.strip() == "idle", "the hold lets go once the server ends",
                  seconds=20)
            ok(rig.gpu(NAME, "run", "list"))
        finally:
            ended(ship, server)


def test_a_half_open_ship_server_has_the_hold_s_timeout_and_holds_the_instance_until_it_ends():
    with GpuRig() as rig:
        shipped(rig)
        ok(rig.gpu(NAME, "run", "list"))
        ship, server, _ = vanished(rig, timeout="6")  # its --timeout keeps it sending, so the host's TCP ends it
        try:
            time.sleep(1)  # past the hold's read, ended with the fake ssh
            assert rig.on_host("busy").stdout.strip() == "busy: a gpu.sh step is running on the host"
            ended(server)
            until(lambda: rig.on_host("busy").stdout.strip() == "idle", "the hold lets go once the server ends",
                  seconds=10)
        finally:
            ended(ship, server)


def test_a_pull_s_server_of_the_instance_holds_no_ship():
    with GpuRig() as rig:
        shipped(rig)
        ok(rig.gpu(NAME, "run", "list"))
        before = len(rig.calls("rsync"))
        ok(rig.gpu(NAME, "pull"))
        argv = server_argv(rig, before)
        assert argv[:3] == ["rsync", "--server", "--sender"], argv
        server, _ = stand_in(rig, argv)  # another machine's pull, going on or half-open
        before = len(rig.calls("rsync"))
        ship = rig.gpu(NAME, "ship", background=True)
        try:
            out, err = ship.communicate(timeout=60)
            assert ship.returncode == 0 and "shipped to" in out, (ship.returncode, out, err)
            calls = rig.calls("rsync")[before:]
            assert len(calls) == 2 and all("--timeout=300" in c for c in calls), calls
            assert server.poll() is None
            assert rig.on_host("busy").stdout.strip() == "idle"
        finally:
            ended(ship, server)


def test_an_instance_s_hold_waits_on_no_server_of_another_instance():
    with GpuRig() as rig:
        shipped(rig)
        # dev1.A's pattern, its dot unescaped, matches dev1-A's server.
        server, _ = stand_in(rig, ["rsync", "--server", "-logDtpre.iLsfxCIvu", "--delete", ".",
                                   f"peacock/{INSTANCE}/cpp/install/"])
        hold = rig.on_host("hold", "1", f"peacock/{INSTANCE.replace('-', '.')}", background=True)
        try:
            assert hold.communicate(timeout=20)[0] == "held\n"
        finally:
            ended(hold, server)


def test_each_of_ship_s_two_servers_holds_the_instance_s_hold():
    with GpuRig() as rig:
        shipped(rig)
        ok(rig.gpu(NAME, "ship", env={"RSYNC_PROTECT_ARGS": "1"}))  # a laptop's, which ship overrides
        calls = rig.calls("rsync")[-2:]
        assert all("--delete" in c for c in calls) and "--relative" in calls[1], calls
        for index in (-2, -1):
            server, gate = stand_in(rig, server_argv(rig, index))
            hold = rig.on_host("hold", "1", f"peacock/{INSTANCE}", background=True)
            try:
                with contextlib.suppress(subprocess.TimeoutExpired):
                    hold.communicate(timeout=3)
                    raise AssertionError(f"the hold let go under the server of {calls[index]}")
                gate.unlink()
                assert hold.communicate(timeout=15)[0] == "held\n"
            finally:
                ended(hold, server)


def test_a_hold_that_lapses_while_the_command_works_is_named():
    with GpuRig() as rig:
        shipped(rig)
        (rig.host / "hold_smi").write_text("")
        before = len(rig.host_calls(rig.host, "nvidia-smi"))
        run = rig.gpu(NAME, "run", "list", env={"PEACOCK_HOLD_BEAT": "1", "PEACOCK_HOLD_TIMEOUT": "2"},
                      background=True)
        until(lambda: len(rig.host_calls(rig.host, "nvidia-smi")) > before, "the run reads the driver")
        # The heartbeat stalled, as on a laptop asleep or a gpu.sh stopped: the beater and its sleep.
        beaters = []
        until(lambda: beaters.extend(_beaters(run.pid)) or beaters, "the heartbeat is found")
        for pid in beaters:
            os.kill(pid, signal.SIGSTOP)
        until(lambda: rig.on_host("busy").stdout.strip() == "idle", "the host's hold times out", seconds=20)
        for pid in beaters:
            os.kill(pid, signal.SIGCONT)
        (rig.host / "hold_smi").unlink()
        out, err = run.communicate(timeout=60)
        assert run.returncode == 0 and "green" in out, (run.returncode, err)
        assert "the hold on l40s ended early (exit 0)" in err, err


def _beaters(gpu_pid):
    """gpu.sh's heartbeat subshell and its sleep: the sleep whose parent's parent is gpu.sh."""
    def parent(pid):
        return int(open(f"/proc/{pid}/stat").read().rsplit(")", 1)[1].split()[1])
    for pid in sleeps_of("1"):
        try:
            loop = parent(pid)
            if parent(loop) == gpu_pid:
                return [loop, int(pid)]
        except OSError:
            pass
    return []


def test_a_command_neither_waits_for_nor_leaves_its_heartbeat_s_sleep():
    with GpuRig() as rig:
        shipped(rig)
        start = time.time()
        ok(rig.gpu(NAME, "run", "list", env={"PEACOCK_HOLD_BEAT": "29"}))
        assert time.time() - start < 15, f"{time.time() - start:.1f}s: the release waited out the heartbeat's sleep"
        assert sleeps_of("29") == [], "the heartbeat's sleep outlived the command"


def test_a_second_install_waits_and_says_so():
    machine = pathlib.Path("/run/lock/peacock-install.lock")

    def stamp():
        return machine.stat().st_mtime_ns if machine.exists() else None
    with GpuRig() as rig:
        before = stamp()
        with open(rig.host / "install.lock", "a") as lock:  # another user's prepare, mid-install
            fcntl.flock(lock, fcntl.LOCK_EX)
            prepare = rig.gpu(NAME, "prepare", background=True)
            line = stderr_line(prepare, "waiting on")
            assert HOSTNAME in line and str(rig.host / "install.lock") in line, line
            deadline = time.time() + 2
            while time.time() < deadline:
                assert prepare.poll() is None, "prepare ended while another install held the lock"
                assert not [c for c in rig.host_calls(rig.host, "docker") if c[0] == "pull"], "pulled while waiting"
                time.sleep(0.05)
        assert prepare.wait(timeout=60) == 0, prepare.communicate()
        assert stamp() == before, "the rig's own lock, never the machine's"


if __name__ == "__main__":
    raise SystemExit(main(globals()))
