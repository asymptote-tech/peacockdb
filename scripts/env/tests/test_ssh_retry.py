"""`scripts/lib/ssh-retry.sh`: ssh retried while it fails before authenticating, never after.

The fake `ssh` answers each call from a JSON list of attempts: the lines it writes to its `-E` log
(appending, ending each in CR LF, as ssh does), its exit, its stdout, and whether it echoes its
stdin. Its `-G` prints the first `-o LogLevel=` it is given, INFO by default, and is not an attempt.
A last test runs the real ssh at a local port nothing listens on, its config and known hosts in a
temp dir.
"""
import json
import os
import pathlib
import socket
import subprocess
import sys
import tempfile

sys.path.append(str(pathlib.Path(__file__).resolve().parents[2] / "exec_model" / "tests"))
from harness import main  # noqa: E402

LIB = pathlib.Path(__file__).resolve().parents[2] / "lib" / "ssh-retry.sh"
REFUSED = "ssh: connect to host vm port 22: Connection refused"
AUTHENTICATED = 'Authenticated to vm ([10.0.0.7]:22) using "publickey".'
ADDED = "Warning: Permanently added '10.0.0.7' (ED25519) to the list of known hosts."
SESSION = [ADDED, AUTHENTICATED, "Transferred: sent 2780, received 2936 bytes, in 0.1 seconds",
           "Bytes per second: sent 27800.0, received 29360.0"]

FAKE_SSH = r'''#!/usr/bin/env python3
import json, os, sys
a = sys.argv[1:]
if a[0] == "-G":
    levels = [a[i + 1].split("=", 1)[1] for i in range(len(a) - 1)
              if a[i] == "-o" and a[i + 1].startswith("LogLevel=")]
    print("loglevel " + (levels[0] if levels else "INFO"))
    sys.exit(0)
with open(os.environ["FAKE_SSH_CALLS"], "a") as f:
    f.write(json.dumps(a) + "\n")
with open(os.environ["FAKE_SSH_CALLS"]) as f:
    n = len(f.readlines())
attempt = json.load(open(os.environ["FAKE_SSH_ATTEMPTS"]))[n - 1]
with open(a[a.index("-E") + 1], "a", newline="") as log:
    log.write("".join(line + "\r\n" for line in attempt.get("log", [])))
if attempt.get("read_stdin"):
    sys.stdout.buffer.write(sys.stdin.buffer.read())
sys.stdout.write(attempt.get("stdout", ""))
sys.exit(attempt.get("rc", 0))
'''

FAKE_SLEEP = '#!/bin/sh\necho "$1" >> "$FAKE_SLEEP_LOG"\n'

FAKE_SYNC = r'''#!/usr/bin/env python3
import json, os, sys
with open(os.environ["FAKE_SYNC_CALLS"], "a") as f:
    f.write(json.dumps(sys.argv[1:]) + "\n")
with open(os.environ["FAKE_SYNC_CALLS"]) as f:
    n = len(f.readlines())
sys.exit(json.loads(os.environ["FAKE_SYNC_RCS"])[n - 1])
'''


class Box:
    def __init__(self, tmp, attempts=(), pauses="0 0 0"):
        self.tmp = pathlib.Path(tmp)
        self.bin = self.tmp / "bin"
        self.bin.mkdir()
        for name, text in (("ssh", FAKE_SSH), ("sleep", FAKE_SLEEP), ("fakesync", FAKE_SYNC)):
            (self.bin / name).write_text(text)
            (self.bin / name).chmod(0o755)
        (self.tmp / "attempts.json").write_text(json.dumps(list(attempts)))
        self.env = dict(os.environ, PATH=f"{self.bin}:{os.environ['PATH']}",
                        FAKE_SSH_CALLS=str(self.tmp / "ssh.calls"),
                        FAKE_SSH_ATTEMPTS=str(self.tmp / "attempts.json"),
                        FAKE_SLEEP_LOG=str(self.tmp / "sleep.log"),
                        FAKE_SYNC_CALLS=str(self.tmp / "sync.calls"))
        if pauses is None:
            self.env.pop("PEACOCK_SSH_PAUSES", None)
        else:
            self.env["PEACOCK_SSH_PAUSES"] = pauses

    def call(self, function, *args, stdin=b"", env=None):
        return subprocess.run(
            ["bash", "-c", f'set -euo pipefail; . "$0"; {function} "$@"', str(LIB), *args],
            input=stdin, capture_output=True, env=dict(self.env, **(env or {})), timeout=60)

    def ssh(self, *args, stdin=b""):
        return self.call("ssh_retry", "ssh", *args, stdin=stdin)

    def sync(self, rcs):
        return self.call("sync_retry", "fakesync", "-a", "src/", "vm:dst/",
                         env={"FAKE_SYNC_RCS": json.dumps(rcs)})

    def calls(self, name="ssh"):
        path = self.tmp / f"{name}.calls"
        return [json.loads(line) for line in path.read_text().splitlines()] if path.exists() else []


def retry_lines(stderr):
    return [line for line in stderr.splitlines() if line.startswith("ssh: attempt ")]


def own_lines(stderr):
    return [line for line in stderr.splitlines() if not line.startswith("ssh: attempt ")]


def retry_line(n, failure, pause=0):
    return f"ssh: attempt {n} of 4 failed before authenticating ({failure}); again in {pause} s"


def refusal(**more):
    return dict({"log": [REFUSED], "rc": 255}, **more)


def test_a_connection_that_never_authenticates_is_tried_four_times():
    with tempfile.TemporaryDirectory() as tmp:
        box = Box(tmp, [refusal()] * 4)
        r = box.ssh("vm", "true")
        err = r.stderr.decode()
        assert r.returncode == 255, (r.returncode, err)
        assert len(box.calls()) == 4, box.calls()
        assert retry_lines(err) == [retry_line(n, REFUSED) for n in (1, 2, 3)], err
        assert own_lines(err) == [REFUSED], err


def test_a_255_after_authenticating_is_returned_at_once():
    with tempfile.TemporaryDirectory() as tmp:
        box = Box(tmp, [{"log": [AUTHENTICATED, "Connection to vm closed by remote host."], "rc": 255}])
        r = box.ssh("vm", "make")
        err = r.stderr.decode()
        assert r.returncode == 255, (r.returncode, err)
        assert len(box.calls()) == 1, box.calls()
        assert "Authenticated to" not in err, err
        assert own_lines(err) == ["Connection to vm closed by remote host."], err


def test_a_255_with_an_empty_log_is_not_retried():
    with tempfile.TemporaryDirectory() as tmp:
        box = Box(tmp, [{"log": [], "rc": 255}] * 4)
        r = box.ssh("vm", "true")
        assert r.returncode == 255, (r.returncode, r.stderr)
        assert len(box.calls()) == 1, box.calls()
        assert r.stderr == b"", r.stderr


def test_a_host_key_refusal_is_not_retried():
    for log in (["Host key verification failed."],
                ["@    WARNING: REMOTE HOST IDENTIFICATION HAS CHANGED!     @",
                 "Permission denied (publickey)."]):
        with tempfile.TemporaryDirectory() as tmp:
            box = Box(tmp, [{"log": log, "rc": 255}] * 4)
            r = box.ssh("vm", "true")
            err = r.stderr.decode()
            assert r.returncode == 255, (r.returncode, err)
            assert len(box.calls()) == 1, box.calls()
            assert own_lines(err) == log, err


def up_to(box, attempts, name, *args):
    """ssh_retry_up_to <attempts> <name> ssh <args>, the caller's <name> local preset to 7: (the run,
    the value it was left with)."""
    r = box.call(f'f() {{ local {name}=7 rc_=0; ssh_retry_up_to {attempts} {name} "$@" || rc_=$?; '
                 f'echo "value=${name}"; return "$rc_"; }}; f', "ssh", *args)
    return r, r.stdout.decode().splitlines()[-1]


# The names the helper's locals had before their prefix: without it, a caller's variable named like
# a local would resolve to that local.
NAMES = ("authenticated", "rc", "log", "n", "ssh", "level", "clash", "max", "attempts")


def test_the_caller_learns_whether_the_returned_attempt_authenticated():
    cases = (([refusal(), {"log": [AUTHENTICATED, "Connection to vm closed by remote host."], "rc": 255}],
              ("vm", "true"), 255, "1"),
             ([{"log": [AUTHENTICATED], "rc": 0}], ("vm", "true"), 0, "1"),
             ([refusal()] * 4, ("vm", "true"), 255, "0"),
             ([{"log": ["Host key verification failed."], "rc": 255}], ("vm", "true"), 255, "0"),
             ([{"log": [], "rc": 255}], ("vm", "true"), 255, "0"),
             ([refusal()], ("-q", "vm", "true"), 2, "0"))
    for name in NAMES:
        for attempts, args, rc, value in cases:
            with tempfile.TemporaryDirectory() as tmp:
                r, line = up_to(Box(tmp, attempts), 4, name, *args)
                assert (r.returncode, line) == (rc, f"value={value}"), (name, attempts, r.returncode, line, r.stderr)


def test_ssh_retry_up_to_bounds_the_attempts_and_names_its_bound():
    with tempfile.TemporaryDirectory() as tmp:
        box = Box(tmp, [refusal()] * 4)
        r, line = up_to(box, 2, "auth", "vm", "true")
        err = r.stderr.decode()
        assert (r.returncode, line) == (255, "value=0"), (r.returncode, line, err)
        assert len(box.calls()) == 2, box.calls()
        assert retry_lines(err) == [retry_line(1, REFUSED).replace("of 4", "of 2")], err
    with tempfile.TemporaryDirectory() as tmp:
        box = Box(tmp, [refusal()] * 4)
        r, line = up_to(box, 1, "auth", "vm", "true")
        err = r.stderr.decode()
        assert (r.returncode, line, len(box.calls())) == (255, "value=0", 1), (r.returncode, line, box.calls())
        assert retry_lines(err) == [] and own_lines(err) == [REFUSED], err


def test_a_flag_that_hides_the_log_or_shares_the_connection_is_refused():
    for args, flag in ((["-q", "vm", "true"], "-q"), (["-E", "mine.log", "vm", "true"], "-E"),
                       (["-S", "ctl", "vm", "true"], "-S"), (["-y", "vm", "true"], "-y"),
                       (["-tq", "vm", "true"], "-q"), (["-p", "22", "vm", "-q", "true"], "-q"),
                       (["-Smine", "vm"], "-S")):
        with tempfile.TemporaryDirectory() as tmp:
            box = Box(tmp, [refusal()] * 4)
            r = box.ssh(*args)
            err = r.stderr.decode()
            assert r.returncode == 2, (args, r.returncode, err)
            assert box.calls() == [], (args, box.calls())
            assert err.startswith(f"ssh_retry: {flag} ") and len(err.splitlines()) == 1, (args, err)


def test_a_command_s_own_flags_and_v_are_not_refused():
    for args in (["-v", "vm", "true"], ["-oLogLevel=ERROR", "-pq", "vm"], ["vm", "grep", "-q", "x"],
                 ["--", "vm", "-q"], ["-i", "-q", "vm"]):
        with tempfile.TemporaryDirectory() as tmp:
            box = Box(tmp, [{"log": [AUTHENTICATED], "rc": 0}])
            r = box.ssh(*args)
            assert r.returncode == 0, (args, r.returncode, r.stderr)
            assert box.calls()[0][6:] == args, box.calls()


def test_the_command_s_own_status_passes_through():
    with tempfile.TemporaryDirectory() as tmp:
        box = Box(tmp, [{"log": [AUTHENTICATED], "rc": 3, "stdout": "out\n"}])
        r = box.ssh("vm", "exit 3")
        assert r.returncode == 3, (r.returncode, r.stderr)
        assert len(box.calls()) == 1, box.calls()
        assert r.stdout == b"out\n", r.stdout


def test_a_retry_s_stdin_reaches_the_remote_whole():
    data = bytes(range(256)) * 256
    with tempfile.TemporaryDirectory() as tmp:
        box = Box(tmp, [refusal(), refusal(), {"log": [AUTHENTICATED], "read_stdin": True}])
        r = box.ssh("vm", "cat", stdin=data)
        assert r.returncode == 0, (r.returncode, r.stderr)
        assert len(box.calls()) == 3, box.calls()
        assert r.stdout == data, len(r.stdout)


def test_each_attempt_has_its_own_log_and_no_shared_connection():
    with tempfile.TemporaryDirectory() as tmp:
        box = Box(tmp, [refusal(log=["first refusal"]), refusal(log=["Permission denied (publickey)."]),
                        {"log": [AUTHENTICATED, "third's own line"], "rc": 0}])
        r = box.ssh("-p", "2222", "vm", "true")
        err = r.stderr.decode()
        assert r.returncode == 0, (r.returncode, err)
        calls = box.calls()
        assert len(calls) == 3, calls
        for argv in calls:
            assert argv[0] == "-E" and argv[2:] == [
                "-o", "LogLevel=VERBOSE", "-o", "ControlPath=none", "-p", "2222", "vm", "true"], argv
        assert retry_lines(err) == [retry_line(1, "first refusal"),
                                    retry_line(2, "Permission denied (publickey).")], err
        assert own_lines(err) == ["third's own line"], err


def test_the_pauses_are_5_10_and_20_seconds():
    with tempfile.TemporaryDirectory() as tmp:
        box = Box(tmp, [refusal()] * 4, pauses=None)
        r = box.ssh("vm", "true")
        assert r.returncode == 255, (r.returncode, r.stderr)
        assert (box.tmp / "sleep.log").read_text().split() == ["5", "10", "20"]
        assert retry_lines(r.stderr.decode()) == [
            retry_line(n, REFUSED, pause) for n, pause in ((1, 5), (2, 10), (3, 20))], r.stderr


def test_verbose_only_lines_stay_off_stderr():
    with tempfile.TemporaryDirectory() as tmp:
        box = Box(tmp, [{"log": SESSION, "rc": 0}])
        r = box.ssh("vm", "true")
        assert r.returncode == 0, (r.returncode, r.stderr)
        assert r.stderr.decode() == ADDED + "\r\n", r.stderr


def test_stderr_holds_what_plain_ssh_prints_at_the_caller_s_log_level():
    for level, attempts, shown in (
            ("ERROR", [{"log": SESSION, "rc": 0}], []),
            ("QUIET", [refusal()] * 4, []),
            ("VERBOSE", [{"log": SESSION, "rc": 0}], SESSION)):
        with tempfile.TemporaryDirectory() as tmp:
            box = Box(tmp, attempts)
            r = box.ssh("-o", f"LogLevel={level}", "vm", "true")
            err = r.stderr.decode()
            assert own_lines(err) == shown, (level, err)
            assert len(retry_lines(err)) == len(attempts) - 1, (level, err)


def test_the_real_ssh_at_a_closed_port_is_tried_four_times_and_leaves_stdin():
    with tempfile.TemporaryDirectory() as tmp:
        tmp = pathlib.Path(tmp)
        (tmp / "config").write_text("")
        with socket.socket() as s:
            s.bind(("127.0.0.1", 0))
            port = s.getsockname()[1]
        script = ('. "$0"; ssh_retry ssh -F "$1" -o UserKnownHostsFile="$2" -o BatchMode=yes '
                  '-p "$3" 127.0.0.1 true; rc=$?; cat; exit "$rc"')
        r = subprocess.run(
            ["bash", "-c", script, str(LIB), str(tmp / "config"), str(tmp / "known_hosts"), str(port)],
            input=b"payload\n", capture_output=True, timeout=60,
            env=dict(os.environ, PEACOCK_SSH_PAUSES="0 0 0"))
        err = r.stderr.decode()
        refused = f"ssh: connect to host 127.0.0.1 port {port}: Connection refused"
        assert r.returncode == 255, (r.returncode, err)
        assert retry_lines(err) == [retry_line(n, refused) for n in (1, 2, 3)], err
        assert own_lines(err) == [refused], err
        assert r.stdout == b"payload\n", r.stdout
        assert not (tmp / "known_hosts").exists()


def test_sync_retry_reruns_a_command_that_exits_255_or_30():
    for rcs in ([255, 255, 0], [30, 255, 0]):
        with tempfile.TemporaryDirectory() as tmp:
            box = Box(tmp)
            r = box.sync(rcs)
            assert r.returncode == 0, (rcs, r.returncode, r.stderr)
            assert box.calls("sync") == [["-a", "src/", "vm:dst/"]] * 3
            assert r.stderr.decode().splitlines() == [
                f"fakesync: attempt {n} of 4 exited {rc}; again in 0 s" for n, rc in ((1, rcs[0]), (2, 255))]


def test_sync_retry_returns_any_other_exit_at_once():
    for rc in (1, 23):
        with tempfile.TemporaryDirectory() as tmp:
            box = Box(tmp)
            r = box.sync([rc, 0])
            assert r.returncode == rc, (rc, r.returncode, r.stderr)
            assert len(box.calls("sync")) == 1


def test_sync_retry_stops_after_four_attempts():
    with tempfile.TemporaryDirectory() as tmp:
        box = Box(tmp)
        r = box.sync([255] * 4 + [0])
        assert r.returncode == 255, (r.returncode, r.stderr)
        assert len(box.calls("sync")) == 4
        assert len(r.stderr.decode().splitlines()) == 3, r.stderr


if __name__ == "__main__":
    raise SystemExit(main(globals()))
