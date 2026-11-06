#!/usr/bin/env python3
"""A Nebius VM's idle watcher, one tick per minute from a systemd timer (llm-wiki/build-test.md, "Nebius host").

Stops the instance through the API when no test has run for an hour, two if a build ran in
that time, or when one test process has run for four hours. Clocks never start before boot,
so a VM restarted after days stopped is not stopped on its first tick.

The install file names the instance and its kind. On a `gpu` VM a test is a process on the
card, or one named like a test binary (a hung CPU-side test is never on the card). A `cpu` VM
has no card: there a test is a DuckDB run or a data fetch, and input on an ssh terminal is use
too, without a cap.
"""
from __future__ import annotations

import json, os, re, subprocess, sys, time

#: Written by `nebius.sh install-watch`: {"instance_id": ..., "kind": "gpu"|"cpu"}.
INSTALL = "/etc/peacock-gpu-idle.json"
#: Who a watcher installed before the install file existed is: nebius-gpu.
DEFAULT_INSTANCE_ID = "computeinstance-e00tnrse7ayntnzcyt"
INSTANCE_RE = re.compile(r"^computeinstance-[a-z0-9]+$")
IDLE, IDLE_BUILD, CAP = 3600, 7200, 14400
STATE = "/var/lib/peacock-gpu-idle/state.json"
LOG = "/var/log/peacock-gpu-idle.log"
#: An ssh login's terminals. Their atime is the last input; `nebius.sh ssh <cmd>` gets none.
PTS = "/dev/pts"
TEST_NAMES = ("peacock_", "test_gpu_corpus", "test_node_timing", "peacockdb_core_gpu_lib",
              "prototype_fused_benchmarks")
BUILD_NAMES = ("rsync", "cargo", "cmake", "ninja", "nvcc", "cc1plus")
CPU_TEST_NAMES = ("duckdb", "aws")
#: The fresh-VM bootstrap's work. `docker` is the client: it is matched by its whole name,
#: because dockerd and docker-proxy run all day and would read as a build forever.
CPU_BUILD_NAMES = BUILD_NAMES + ("apt-get", "dpkg", "docker", "git", "curl")
EXACT_NAMES = ("docker",)
#: the kernel's task comm is 15 characters, and `ps -eo comm=` shows exactly that
COMM_LEN = 15
#: a hung nvidia-smi or ps, or API call, must end the tick (the unit allows 150 s in all)
PROBE_TIMEOUT, STOP_TIMEOUT = 30, 90


def decide(state, now, boot, card, tests, build, sessions=()):
    """-> (new state, decision, reason). card and tests are [{pid, name, start}]: the processes
    on the card, and the processes named like a test. Either is test activity, and either one
    running for CAP is a stop. sessions are the last-input times of interactive terminals: each
    since boot is test activity at that time, with no cap."""
    prev_test = max(state.get("last_test", boot), boot)
    prev_build = max(state.get("last_build", 0.0), boot)
    last_test, last_build = prev_test, prev_build
    if tests or card:
        last_test = now
    for s in sessions:
        if boot <= s <= now:
            last_test = max(last_test, s)
    if build:
        last_build = now
    new = {"last_test": last_test, "last_build": last_build, "boot": boot}
    for where, procs in (("on the card", card), ("a test process", tests)):
        for p in procs:
            if now - max(p["start"], boot) >= CAP:
                return new, "cap-stop", f"pid {p['pid']} ({p['name']}), {where}, running for {int(now - p['start'])}s"
    idle = now - last_test
    # The two-hour limit, measured from the last test, applies when a build was seen since
    # that test (judged on the state as it was before this tick, or a build in this tick).
    build_since_test = build or prev_build > prev_test
    limit = IDLE_BUILD if build_since_test else IDLE
    if idle >= limit:
        return new, "idle-stop", f"no test for {int(idle)}s (limit {limit}s)"
    return new, "keep", f"idle {int(idle)}s of {limit}s"


def log_line(now, boot, card, tests, build, state, decision, reason, error=None, sessions=(), instance=None,
             kind="gpu"):
    """tests is the list of name-matched test processes; "tests" keeps its old meaning (any
    seen) and "test_procs" carries them, so the checker can recompute the cap on them, as it
    does the idle limit with "sessions"."""
    return {"t": now, "boot": boot, "card": card, "tests": bool(tests), "test_procs": tests, "build": build,
            "last_test": state["last_test"], "last_build": state["last_build"],
            "decision": decision, "reason": reason, "error": error,
            "sessions": list(sessions), "instance": instance, "kind": kind}


def load_install(path):
    """-> (instance id, kind, problem). No file is nebius-gpu as installed before the file
    existed. A file that cannot be read or trusted gives no id, so no guessed VM is stopped."""
    try:
        with open(path) as f:
            d = json.load(f)
    except FileNotFoundError:
        return DEFAULT_INSTANCE_ID, "gpu", None
    except (OSError, ValueError) as e:
        return None, "gpu", f"install {path}: {type(e).__name__}: {e}"
    iid = d.get("instance_id") if isinstance(d, dict) else None
    kind = d.get("kind") if isinstance(d, dict) else None
    if not (isinstance(iid, str) and INSTANCE_RE.match(iid)) or kind not in ("gpu", "cpu"):
        return None, "gpu", f"install {path}: not an instance id and a kind gpu|cpu: {str(d)[:200]}"
    return iid, kind, None


def parse_card(out):
    """`nvidia-smi --query-compute-apps=pid,process_name --format=csv,noheader` -> (apps, problems).

    apps is [(pid, process_name)]. A line that is not `<pid>, <name>` (NVML failures print
    prose such as "Failed to initialize NVML") is reported in problems, never raised.
    """
    apps, problems = [], []
    for line in filter(None, (l.strip() for l in out.splitlines())):
        pid, sep, name = line.partition(",")
        if sep and pid.strip().isdigit():
            apps.append((int(pid), name.strip()))
        else:
            problems.append(f"nvidia-smi: {line[:200]}")
    return apps, problems


def parse_ps(out):
    """`ps -eo pid=,etimes=,comm=` -> ([(pid, etimes, comm)], problems). Never raises."""
    procs, problems = [], []
    for line in filter(None, (l.strip() for l in out.splitlines())):
        parts = line.split(None, 2)
        try:
            procs.append((int(parts[0]), float(parts[1]), parts[2]))
        except (IndexError, ValueError):
            problems.append(f"ps: {line[:200]}")
    return procs, problems


def matches(comm_names, names):
    """Whether any process name starts with any listed name, as `ps` can show it.

    `ps -eo comm=` truncates to 15 characters, so a listed name longer than that is
    compared by its first 15 (`peacockdb_core_gpu_lib` appears as `peacockdb_core_`). A name
    in EXACT_NAMES matches only itself.
    """
    prefixes = tuple(n[:COMM_LEN] for n in names if n not in EXACT_NAMES)
    exact = [n for n in names if n in EXACT_NAMES]
    return any(c.startswith(prefixes) or c in exact for c in comm_names)


def _boot_time():
    with open("/proc/stat") as f:
        return float(next(l for l in f if l.startswith("btime")).split()[1])


def _run(argv, timeout=PROBE_TIMEOUT):
    return subprocess.run(argv, capture_output=True, text=True, timeout=timeout)


def _card(now):
    """-> (card, problems). A failed probe leaves the card empty and says why."""
    try:
        r = _run(["nvidia-smi", "--query-compute-apps=pid,process_name", "--format=csv,noheader"])
    except (OSError, subprocess.SubprocessError) as e:
        return [], [f"nvidia-smi: {type(e).__name__}: {e}"]
    apps, problems = parse_card(r.stdout)
    if r.returncode != 0 and not problems:
        problems.append(f"nvidia-smi: exit {r.returncode}")
    procs = []
    for pid, name in apps:
        try:
            et = float(_run(["ps", "-o", "etimes=", "-p", str(pid)]).stdout.strip() or 0)
        except (OSError, subprocess.SubprocessError, ValueError) as e:
            et = 0.0
            problems.append(f"ps etimes {pid}: {type(e).__name__}: {e}")
        procs.append({"pid": pid, "name": os.path.basename(name), "start": now - et})
    return procs, problems


def _procs(now, test_names=TEST_NAMES):
    """-> (comm names, name-matched test processes as [{pid, name, start}], problems)."""
    procs, problems = parse_ps(_run(["ps", "-eo", "pid=,etimes=,comm="]).stdout)
    tests = [{"pid": pid, "name": comm, "start": now - et}
             for pid, et, comm in procs if matches([comm], test_names)]
    return [comm for _, _, comm in procs], tests, problems


def _sessions():
    """-> (last-input times of the numbered terminals, problems)."""
    times, problems = [], []
    try:
        names = [n for n in os.listdir(PTS) if n.isdigit()]
    except OSError as e:
        return [], [f"sessions: {e}"]
    for n in names:
        try:
            times.append(os.stat(os.path.join(PTS, n)).st_atime)
        except FileNotFoundError:
            pass  # the login ended between the listing and the stat
        except OSError as e:
            problems.append(f"sessions: {e}")
    return sorted(times), problems


def _stop(instance_id):
    """Ask the API to stop the instance. -> an error string, or None."""
    if instance_id is None:
        return "stop: no instance id installed"
    try:
        r = subprocess.run(["/usr/local/bin/nebius", "compute", "instance", "stop", "--id", instance_id, "--async"],
                           env={**os.environ, "HOME": "/root"}, capture_output=True, text=True, timeout=STOP_TIMEOUT)
    except (OSError, subprocess.SubprocessError) as e:
        return f"stop: {type(e).__name__}: {e}"
    return f"stop: exit {r.returncode}: {r.stderr.strip()[:200]}" if r.returncode != 0 else None


def main():
    now, boot = time.time(), _boot_time()
    instance, kind, problem = load_install(INSTALL)
    problems = [problem] if problem else []
    try:
        with open(STATE) as f:
            state = json.load(f)
        if not isinstance(state, dict):
            raise ValueError("not an object")
    except (OSError, ValueError):
        state = {}
    card, sessions = [], []
    if kind == "gpu":
        card, p = _card(now)
    else:
        sessions, p = _sessions()
    problems += p
    try:
        comms, tests, p = _procs(now, TEST_NAMES if kind == "gpu" else CPU_TEST_NAMES)
        problems += p
    except (OSError, subprocess.SubprocessError) as e:
        comms, tests = [], []
        problems.append(f"ps: {type(e).__name__}: {e}")
    build = matches(comms, BUILD_NAMES if kind == "gpu" else CPU_BUILD_NAMES)
    new, decision, reason = decide(state, now, boot, card, tests, build, sessions)
    # The stop comes before any write: a full disk or a missing directory must not keep the VM up.
    stop_error = _stop(instance) if decision != "keep" else None
    if stop_error:
        problems.append(stop_error)
    try:
        with open(STATE, "w") as f:
            json.dump(new, f)
    except OSError as e:
        problems.append(f"state: {e}")
    line = log_line(now, boot, card, tests, build, new, decision, reason, "; ".join(problems) or None,
                    sessions, instance, kind)
    try:
        with open(LOG, "a") as f:
            f.write(json.dumps(line) + "\n")
    except OSError as e:
        print(f"log: {e}: {json.dumps(line)}", file=sys.stderr)
    return 1 if stop_error else 0


if __name__ == "__main__":
    sys.exit(main())
