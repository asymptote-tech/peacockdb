# gpu-host Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The ensemble and CI start nebius-gpu for a GPU step and stop it afterwards. Two
independent guards stop it when nobody does: a watcher on the VM, and an hourly GitHub workflow.

**Architecture:** One bash entry point, `scripts/gpu-host.sh`, owns the instance: its state, its
IP, its host key, start and stop. Two thin scripts sit on top of it: one syncs a tree to the VM,
one runs the L40S test list there. The decisions that need tests live in small stdlib-Python
modules with pure functions: when the watcher stops the VM, and when the checker does. Their
shells only gather inputs.

**Tech Stack:** bash, the `nebius` CLI (0.12.x), OpenSSH, rsync, systemd, Python 3.10+ stdlib,
GitHub Actions.

**Spec:** [`gpu-host.md`](gpu-host.md)

## Global Constraints

- Instance `computeinstance-e00tnrse7ayntnzcyt`, a constant in `scripts/gpu-host.sh`. Project
  `project-e00tdypyen10mmyep61c8q`.
- **Never open a Nebius credential file**: `~/.nebius/config.yaml`, `~/.nebius/credentials.yaml`,
  any `*.pem` under `~/.nebius/`, on any host. No `cat`, `grep`, `sed`, `head` or Read on them.
  Scripts may copy them with `scp` or `install` but never print them. To learn the CLI's
  surface, use `nebius … --help`.
- Exit codes of `gpu-host.sh`: 0 done; 2 card busy (soft stop refused); 75 no capacity, retry
  later; 69 instance gone; 1 anything else.
- The L40S list excludes `peacock_tpch_tests`, `peacock_tpchv_tests`, `--run-benchmarks` and
  Nsight. `peacock_gpu_benchmarks` runs with `--skip bench_`.
- Idle stop: 1 h with no test activity, or 2 h if build activity was seen in that time. Cap: the
  same process on the card for 4 h. Clocks never start before boot.
- Python tests use the repo's stdlib harness pattern (`python3 <file>`, no pytest), as
  `scripts/calibration/tests/` does.
- Every commit keeps code, code comments and `llm-wiki/` in agreement.

## Review Focus

- **The VM moves IP between two calls in one step.** Nothing may cache the IP. `gpu-host.sh ssh`
  resolves it per call (Task 1's `test_ip_is_resolved_per_call`).
- **A first ssh with an empty known-hosts file.** It must store the key through the API check, not
  fail or prompt (Task 1's `test_first_contact_stores_key`).
- **Stop on an instance that is already STOPPED, or that is STARTING.** Already stopped is a no-op
  with exit 0. STARTING is waited out, then stopped (Task 1's `test_stop_noop_when_stopped` and
  `test_stop_waits_out_starting`).
- **The watcher on a VM just restarted after a week stopped.** Its state is old; the VM must not
  stop at the first tick (Task 3's `test_fresh_boot_over_old_state_keeps`).
- **A test binary that all-skips, or one never shipped.** It must be red, not green (Task 2's
  `test_zero_tests_is_red` and `test_missing_binary_dir_is_red`).

---

### Task 1: `scripts/gpu-host.sh`, its fakes and its tests

**Files:**
- Create: `scripts/gpu-host.sh`
- Create: `scripts/gpu-host/tests/harness.py`, a verbatim copy of `scripts/calibration/tests/harness.py`
- Create: `scripts/gpu-host/tests/fakes.py`
- Create: `scripts/gpu-host/tests/test_gpu_host.py`

**Interfaces:**
- Produces, for every later task:
  - `scripts/gpu-host.sh status|ip|start|stop [--force]|ssh <cmd…>|rsh <host> <cmd…>|install-watch`
  - the exit codes above.
  - Environment overrides, used by the tests and by CI: `NEBIUS` (CLI path, default `nebius`),
    `GPU_HOST_SSH` (default `ssh`), `GPU_HOST_KEYSCAN` (default `ssh-keyscan`),
    `GPU_HOST_USER` (default `dmitry`), `GPU_HOST_KEY` (an identity file, optional),
    `GPU_HOST_STATE_DIR` (default `~/.config/peacock/gpu-host`, which holds `known_hosts`
    and `gpu-host.log`), `GPU_HOST_POLL` (seconds between polls, default 10),
    `GPU_HOST_START_TIMEOUT` (default 900), `GPU_HOST_STOP_TIMEOUT` (default 600).
  - `rsh` exists for `rsync -e "scripts/gpu-host.sh rsh"`. It drops rsync's host argument and
    runs the rest over the same ssh as `ssh`.

- [ ] **Step 1: Learn the CLI's error text for the two cases that need their own codes**

Run on dev, which has the CLI and a profile. These are `--help` and `get` calls; no
credential file is opened:
```bash
nebius compute instance get --id computeinstance-doesnotexist000000 --format json; echo rc=$?
nebius compute instance start --help | sed -n 1,40p
nebius compute instance get --id computeinstance-e00tnrse7ayntnzcyt --format json \
  | jq '{s: .status.state, ip: .status.network_interfaces[0].public_ip_address.address, p: .spec.preemptible}'
```
Expected: the first prints a NOT_FOUND-style error. Record its exact text in the comment above
`classify_error` (Step 3). The last prints a state, an IP of the form `a.b.c.d/32`, and
`on_preemption: STOP`. The capacity error cannot be provoked on purpose, so match broadly:
`RESOURCE_EXHAUSTED`, `NotEnoughResources`, `not enough`, `quota`, `capacity`, case-insensitive.

- [ ] **Step 2: Write the fakes and the failing tests**

`scripts/gpu-host/tests/fakes.py` builds a temp dir holding executable `nebius`, `ssh` and
`ssh-keyscan` fakes, driven by one JSON file. The tests point `NEBIUS`, `GPU_HOST_SSH` and
`GPU_HOST_KEYSCAN` at them:

```python
"""Fake `nebius`, `ssh` and `ssh-keyscan` for gpu-host.sh's tests, driven by one JSON world.

world = {
  "exists": True, "state": "STOPPED", "ip": "203.0.113.7",
  "start_error": "",          # stderr text and rc 1 on `start`
  "start_to": "RUNNING",      # the state `start` leads to
  "states_after_start": [],   # states `get` reports before start_to, one per call
  "ssh": "ok",                # ok | down | changed
  "host_key": "ssh-ed25519 AAAAKEY1",
  "card": "",                 # what nvidia-smi --query-compute-apps prints
  "calls": []                 # appended by every fake: argv lists
}
"""
import json, os, pathlib, stat, sys, tempfile

NEBIUS = r'''#!/usr/bin/env python3
import json, sys
w_path = __import__("os").environ["FAKE_WORLD"]
w = json.load(open(w_path)); a = sys.argv[1:]; w["calls"].append(["nebius"] + a)
def save(): json.dump(w, open(w_path, "w"))
if a[:3] == ["compute", "instance", "get"]:
    if not w["exists"]:
        save(); print("Error: rpc error: code = NotFound desc = instance not found", file=sys.stderr); sys.exit(1)
    if w["states_after_start"]:
        w["state"] = w["states_after_start"].pop(0)
    ni = [{"public_ip_address": {"address": w["ip"] + "/32"}}] if w["state"] == "RUNNING" else []
    save(); print(json.dumps({"status": {"state": w["state"], "network_interfaces": ni}})); sys.exit(0)
if a[:3] == ["compute", "instance", "start"]:
    if w["start_error"]:
        save(); print(w["start_error"], file=sys.stderr); sys.exit(1)
    w["state"] = "STARTING" if w["states_after_start"] else w["start_to"]; save(); sys.exit(0)
if a[:3] == ["compute", "instance", "stop"]:
    w["state"] = "STOPPED"; save(); sys.exit(0)
save(); print("fake nebius: unhandled " + " ".join(a), file=sys.stderr); sys.exit(3)
'''

SSH = r'''#!/usr/bin/env python3
import json, os, sys
w_path = os.environ["FAKE_WORLD"]; w = json.load(open(w_path)); w["calls"].append(["ssh"] + sys.argv[1:])
json.dump(w, open(w_path, "w"))
args = sys.argv[1:]
kh = next(a.split("=", 1)[1] for i, a in enumerate(args) if a.startswith("UserKnownHostsFile="))
if w["ssh"] == "down":
    print("ssh: connect to host: Connection timed out", file=sys.stderr); sys.exit(255)
stored = open(kh).read() if os.path.exists(kh) else ""
if w["host_key"] not in stored:
    print("@@@ WARNING: REMOTE HOST IDENTIFICATION HAS CHANGED! @@@\nHost key verification failed.", file=sys.stderr)
    sys.exit(255)
at = next(i for i, a in enumerate(args) if "@" in a and not a.startswith("-"))
cmd = " ".join(args[at + 1:])
if "nvidia-smi" in cmd:
    print(w["card"], end="")
sys.exit(0)
'''

KEYSCAN = r'''#!/usr/bin/env python3
import json, os, sys
w_path = os.environ["FAKE_WORLD"]; w = json.load(open(w_path)); w["calls"].append(["ssh-keyscan"] + sys.argv[1:])
json.dump(w, open(w_path, "w"))
print(sys.argv[-1] + " " + w["host_key"])
'''

DEFAULT = {"exists": True, "state": "STOPPED", "ip": "203.0.113.7", "start_error": "",
           "start_to": "RUNNING", "states_after_start": [], "ssh": "ok",
           "host_key": "ssh-ed25519 AAAAKEY1", "card": "", "calls": []}


class World:
    def __init__(self, **over):
        self.dir = pathlib.Path(tempfile.mkdtemp(prefix="gpuhost-"))
        self.path = self.dir / "world.json"
        self.write({**DEFAULT, **over, "calls": []})
        for name, body in (("nebius", NEBIUS), ("ssh", SSH), ("ssh-keyscan", KEYSCAN)):
            f = self.dir / name
            f.write_text(body)
            f.chmod(f.stat().st_mode | stat.S_IEXEC)

    def read(self):
        return json.loads(self.path.read_text())

    def write(self, w):
        self.path.write_text(json.dumps(w))

    def set(self, **kv):
        w = self.read(); w.update(kv); self.write(w)

    def env(self):
        return {**os.environ, "FAKE_WORLD": str(self.path), "NEBIUS": str(self.dir / "nebius"),
                "GPU_HOST_SSH": str(self.dir / "ssh"), "GPU_HOST_KEYSCAN": str(self.dir / "ssh-keyscan"),
                "GPU_HOST_STATE_DIR": str(self.dir / "state"), "GPU_HOST_POLL": "0",
                "GPU_HOST_START_TIMEOUT": "5", "GPU_HOST_STOP_TIMEOUT": "5"}
```

`scripts/gpu-host/tests/test_gpu_host.py`:

```python
"""gpu-host.sh against fake nebius/ssh/ssh-keyscan: every exit code, and the host-key rule."""
import pathlib, subprocess, sys
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
from fakes import World  # noqa: E402
from harness import main  # noqa: E402

SCRIPT = str(pathlib.Path(__file__).resolve().parents[2] / "gpu-host.sh")


def run(world, *args):
    p = subprocess.run([SCRIPT, *args], env=world.env(), capture_output=True, text=True, timeout=60)
    return p.returncode, p.stdout, p.stderr


def test_status_stopped_prints_no_ip():
    w = World(state="STOPPED")
    rc, out, _ = run(w, "status")
    assert rc == 0 and out.strip() == "STOPPED", out


def test_status_running_prints_ip():
    w = World(state="RUNNING")
    rc, out, _ = run(w, "status")
    assert rc == 0 and out.split() == ["RUNNING", "203.0.113.7"], out


def test_start_from_stopped_reaches_ssh():
    w = World(state="STOPPED")
    rc, _, err = run(w, "start")
    assert rc == 0, err
    assert any(c[:3] == ["nebius", "compute", "instance"] and "start" in c for c in w.read()["calls"])
    assert any(c[0] == "ssh" for c in w.read()["calls"])


def test_start_when_running_does_not_call_start():
    w = World(state="RUNNING")
    rc, _, err = run(w, "start")
    assert rc == 0, err
    assert not any(c[:4] == ["nebius", "compute", "instance", "start"] for c in w.read()["calls"])


def test_start_capacity_error_is_75():
    w = World(state="STOPPED", start_error="rpc error: code = ResourceExhausted desc = NotEnoughResources")
    rc, _, _ = run(w, "start")
    assert rc == 75


def test_start_never_running_is_75():
    w = World(state="STOPPED", states_after_start=["STARTING"] * 50)
    rc, _, _ = run(w, "start")
    assert rc == 75


def test_missing_instance_is_69():
    w = World(exists=False)
    for verb in ("status", "start", "stop"):
        rc, _, _ = run(w, verb)
        assert rc == 69, verb


def test_soft_stop_refused_while_busy():
    w = World(state="RUNNING", card="4242, test_gpu_corpus\n")
    rc, _, _ = run(w, "stop")
    assert rc == 2 and w.read()["state"] == "RUNNING"


def test_force_stop_while_busy():
    w = World(state="RUNNING", card="4242, test_gpu_corpus\n")
    rc, _, _ = run(w, "stop", "--force")
    assert rc == 0 and w.read()["state"] == "STOPPED"


def test_soft_stop_unreachable_host_stops():
    w = World(state="RUNNING", ssh="down")
    rc, _, _ = run(w, "stop")
    assert rc == 0 and w.read()["state"] == "STOPPED"


def test_stop_noop_when_stopped():
    w = World(state="STOPPED")
    rc, _, _ = run(w, "stop")
    assert rc == 0
    assert not any(c[:4] == ["nebius", "compute", "instance", "stop"] for c in w.read()["calls"])


def test_stop_waits_out_starting():
    w = World(state="STARTING", states_after_start=["STARTING", "RUNNING"])
    rc, _, _ = run(w, "stop", "--force")
    assert rc == 0 and w.read()["state"] == "STOPPED"


def test_first_contact_stores_key():
    w = World(state="RUNNING")
    rc, _, err = run(w, "ssh", "true")
    assert rc == 0, err
    kh = (w.dir / "state" / "known_hosts").read_text()
    assert "nebius-gpu ssh-ed25519 AAAAKEY1" in kh


def test_changed_key_replaced_and_logged():
    w = World(state="RUNNING")
    run(w, "ssh", "true")
    w.set(host_key="ssh-ed25519 AAAAKEY2")
    rc, _, err = run(w, "ssh", "true")
    assert rc == 0, err
    kh = (w.dir / "state" / "known_hosts").read_text()
    assert "AAAAKEY2" in kh and "AAAAKEY1" not in kh
    assert "host key replaced" in (w.dir / "state" / "gpu-host.log").read_text()


def test_ssh_refused_when_api_does_not_vouch():
    w = World(state="STOPPED")
    rc, _, err = run(w, "ssh", "true")
    assert rc == 1 and "not RUNNING" in err
    assert not any(c[0] == "ssh-keyscan" for c in w.read()["calls"])


def test_ip_is_resolved_per_call():
    w = World(state="RUNNING")
    run(w, "ssh", "true")
    w.set(ip="198.51.100.9")
    run(w, "ssh", "true")
    targets = [c[-2] if len(c) > 1 else "" for c in w.read()["calls"] if c[0] == "ssh"]
    assert any("198.51.100.9" in " ".join(c) for c in w.read()["calls"] if c[0] == "ssh"), targets


if __name__ == "__main__":
    main(globals())
```

- [ ] **Step 3: Run the tests and see them fail**

Run: `python3 scripts/gpu-host/tests/test_gpu_host.py`
Expected: every test fails, because `scripts/gpu-host.sh` does not exist.

- [ ] **Step 4: Write `scripts/gpu-host.sh`**

```bash
#!/usr/bin/env bash
# The one door to nebius-gpu, the L40S VM the device tier runs on (llm-wiki/build-test.md,
# "GPU host"). Every caller — a developer, the coordinator, CI, the hourly checker — goes
# through this script, so the instance's IP and host key are handled in one place.
#
#   gpu-host.sh status            state, and the IP when RUNNING
#   gpu-host.sh ip                the IP, or exit 1 when not RUNNING
#   gpu-host.sh start             start, wait for RUNNING and for ssh (idempotent)
#   gpu-host.sh stop [--force]    soft: refuse while a process is on the card; --force: stop anyway
#   gpu-host.sh ssh <cmd…>        run a command on the VM
#   gpu-host.sh rsh <host> <cmd…> the same, for rsync -e (rsync's host argument is dropped)
#   gpu-host.sh install-watch     install the idle watcher and its credentials on the VM
#
# Exit codes: 0 done; 2 card busy; 75 no capacity, retry later; 69 instance gone; 1 other.
#
# NEVER print, cat or grep a Nebius credential file (~/.nebius/config.yaml and friends).
# One redaction attempt already leaked a private key into a transcript.
set -euo pipefail

INSTANCE_ID=computeinstance-e00tnrse7ayntnzcyt
ALIAS=nebius-gpu

NEBIUS=${NEBIUS:-nebius}
SSH=${GPU_HOST_SSH:-ssh}
KEYSCAN=${GPU_HOST_KEYSCAN:-ssh-keyscan}
USER_ON_VM=${GPU_HOST_USER:-dmitry}
STATE_DIR=${GPU_HOST_STATE_DIR:-$HOME/.config/peacock/gpu-host}
POLL=${GPU_HOST_POLL:-10}
START_TIMEOUT=${GPU_HOST_START_TIMEOUT:-900}
STOP_TIMEOUT=${GPU_HOST_STOP_TIMEOUT:-600}
KNOWN_HOSTS=$STATE_DIR/known_hosts
LOG=$STATE_DIR/gpu-host.log
[ -x "$HOME/.nebius/bin/nebius" ] && PATH=$HOME/.nebius/bin:$PATH
mkdir -p "$STATE_DIR"; touch "$KNOWN_HOSTS"

log() { printf '%s %s\n' "$(date -Is)" "$*" | tee -a "$LOG" >&2; }

# classify_error <stderr text> -> exit code. NotFound wording checked against the real CLI
# on <date of Step 1>: <paste the line Step 1 printed>.
classify_error() {
  if grep -qiE 'NotFound|not found' <<<"$1"; then echo 69
  elif grep -qiE 'RESOURCE_EXHAUSTED|ResourceExhausted|NotEnoughResources|not enough|quota|capacity' <<<"$1"; then echo 75
  else echo 1; fi
}

# get_json: the instance as JSON on stdout, or exit with the classified code.
get_json() {
  local out err rc
  err=$(mktemp); rc=0
  out=$("$NEBIUS" compute instance get --id "$INSTANCE_ID" --format json 2>"$err") || rc=$?
  if [ "$rc" -ne 0 ]; then
    local text; text=$(cat "$err"); rm -f "$err"
    log "get failed: $text"; exit "$(classify_error "$text")"
  fi
  rm -f "$err"; printf '%s' "$out"
}

# Both read get_json into a variable first. Piped, pipefail would report python's rc on
# empty input instead of get_json's 69 or 75.
state() {
  local j; j=$(get_json)
  python3 -c 'import json,sys; print(json.load(sys.stdin)["status"]["state"])' <<<"$j"
}

# current_ip: the public IP while RUNNING, else empty. Resolved on every call — the VM is
# preemptible and its IP changes on every start, so nothing may cache it.
current_ip() {
  local j; j=$(get_json)
  python3 -c '
import json, sys
s = json.load(sys.stdin)["status"]
if s.get("state") != "RUNNING": sys.exit(0)
for ni in s.get("network_interfaces") or []:
    a = (ni.get("public_ip_address") or {}).get("address", "")
    if a: print(a.split("/")[0]); break' <<<"$j"
}

ssh_opts() {
  printf '%s\n' -o "HostKeyAlias=$ALIAS" -o "UserKnownHostsFile=$KNOWN_HOSTS" \
    -o StrictHostKeyChecking=yes -o BatchMode=yes -o ConnectTimeout=15 \
    -o ServerAliveInterval=30 -o ServerAliveCountMax=4
  [ -n "${GPU_HOST_KEY:-}" ] && printf '%s\n' -i "$GPU_HOST_KEY"
  return 0
}

# trust_current_key <ip>: the API says <ip> is this instance's, so its key is the one we
# store under the alias. This is the trust anchor; no agent accepts a key by hand.
trust_current_key() {
  local ip=$1 new old
  new=$("$KEYSCAN" -t ed25519 "$ip" 2>/dev/null | awk 'NF>=3 {print $2" "$3; exit}')
  [ -n "$new" ] || { log "ssh-keyscan $ip returned no ed25519 key"; return 1; }
  old=$(awk -v a="$ALIAS" '$1==a {print $2" "$3}' "$KNOWN_HOSTS")
  grep -v "^$ALIAS " "$KNOWN_HOSTS" > "$KNOWN_HOSTS.tmp" || true
  printf '%s %s\n' "$ALIAS" "$new" >> "$KNOWN_HOSTS.tmp"; mv "$KNOWN_HOSTS.tmp" "$KNOWN_HOSTS"
  if [ -n "$old" ]; then log "host key replaced for $INSTANCE_ID at $ip: old $old, new $new"
  else log "host key stored for $INSTANCE_ID at $ip: $new"; fi
}

# vm_ssh <cmd…>: one ssh to the VM at its current IP, repairing the stored key once if the
# API vouches for the IP.
vm_ssh() {
  local ip err rc opts
  ip=$(current_ip)
  [ -n "$ip" ] || { echo "gpu-host: $INSTANCE_ID is not RUNNING; refusing to ssh" >&2; return 1; }
  mapfile -t opts < <(ssh_opts)
  grep -q "^$ALIAS " "$KNOWN_HOSTS" || trust_current_key "$ip" || return 1
  err=$(mktemp); rc=0
  "$SSH" "${opts[@]}" "$USER_ON_VM@$ip" "$@" 2>"$err" || rc=$?
  if [ "$rc" -eq 255 ] && grep -qE 'HOST IDENTIFICATION HAS CHANGED|Host key verification failed' "$err"; then
    rm -f "$err"
    [ "$(current_ip)" = "$ip" ] || { echo "gpu-host: IP moved during the call; retry" >&2; return 1; }
    trust_current_key "$ip" || return 1
    "$SSH" "${opts[@]}" "$USER_ON_VM@$ip" "$@"; return $?
  fi
  cat "$err" >&2; rm -f "$err"; return "$rc"
}

wait_for_state() {  # <wanted> <timeout seconds>
  local want=$1 limit=$2 waited=0 s
  while :; do
    s=$(state)
    [ "$s" = "$want" ] && return 0
    [ "$waited" -ge "$limit" ] && { log "still $s after ${limit}s, wanted $want"; return 1; }
    sleep "$POLL"; waited=$((waited + POLL)); [ "$POLL" -eq 0 ] && waited=$((waited + 1))
  done
}

cmd_status() {
  local s ip; s=$(state); ip=$(current_ip)
  if [ -n "$ip" ]; then echo "$s $ip"; else echo "$s"; fi
}

cmd_start() {
  local s err rc
  s=$(state)
  if [ "$s" != RUNNING ] && [ "$s" != STARTING ]; then
    log "starting $INSTANCE_ID (was $s)"
    err=$(mktemp); rc=0
    "$NEBIUS" compute instance start --id "$INSTANCE_ID" >/dev/null 2>"$err" || rc=$?
    if [ "$rc" -ne 0 ]; then
      local text; text=$(cat "$err"); rm -f "$err"; log "start failed: $text"
      exit "$(classify_error "$text")"
    fi
    rm -f "$err"
  fi
  wait_for_state RUNNING "$START_TIMEOUT" || exit 75
  local waited=0
  until vm_ssh true 2>/dev/null; do
    [ "$waited" -ge "$START_TIMEOUT" ] && { log "RUNNING but no ssh after ${START_TIMEOUT}s"; exit 75; }
    sleep "$POLL"; waited=$((waited + POLL)); [ "$POLL" -eq 0 ] && waited=$((waited + 1))
  done
  log "up: $(cmd_status)"
}

cmd_stop() {
  local force=0 s busy
  [ "${1:-}" = --force ] && force=1
  s=$(state)
  case $s in STOPPED|STOPPING) log "stop: already $s"; return 0;; esac
  [ "$s" = STARTING ] && { wait_for_state RUNNING "$START_TIMEOUT" || true; }
  if [ "$force" -eq 0 ]; then
    busy=$(vm_ssh 'nvidia-smi --query-compute-apps=pid,process_name --format=csv,noheader' 2>/dev/null) \
      || busy="?unreachable"
    if [ "$busy" = "?unreachable" ]; then log "stop: host unreachable over ssh; stopping on the API's state"
    elif [ -n "$busy" ]; then log "stop: card busy: $(tr '\n' ';' <<<"$busy") — not stopping"; exit 2
    fi
  fi
  log "stopping $INSTANCE_ID$([ "$force" -eq 1 ] && echo ' (--force)')"
  "$NEBIUS" compute instance stop --id "$INSTANCE_ID" >/dev/null 2>&1 || true
  wait_for_state STOPPED "$STOP_TIMEOUT" || exit 1
  log "stopped"
}

cmd_install_watch() {
  local here; here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
  # The watcher stops the VM through the API, so it needs a profile. The file is copied, never
  # read or printed here.
  vm_ssh 'mkdir -p ~/peacock-gpu-idle'
  local ip opts; ip=$(current_ip); mapfile -t opts < <(ssh_opts)
  rsync -a -e "$SSH ${opts[*]}" "$here/gpu-host/idle_watch.py" "$here/gpu-host/peacock-gpu-idle.service" \
    "$here/gpu-host/peacock-gpu-idle.timer" "$USER_ON_VM@$ip:peacock-gpu-idle/"
  rsync -a -e "$SSH ${opts[*]}" "$HOME/.nebius/config.yaml" "$USER_ON_VM@$ip:peacock-gpu-idle/nebius-config.yaml"
  vm_ssh 'set -e
    command -v nebius >/dev/null || [ -x ~/.nebius/bin/nebius ] || curl -sSL https://storage.eu-north1.nebius.cloud/cli/install.sh | bash
    sudo install -d -m 700 /root/.nebius /var/lib/peacock-gpu-idle
    sudo install -m 600 ~/peacock-gpu-idle/nebius-config.yaml /root/.nebius/config.yaml
    rm -f ~/peacock-gpu-idle/nebius-config.yaml
    sudo install -m 755 "$(command -v nebius || echo ~/.nebius/bin/nebius)" /usr/local/bin/nebius
    sudo install -m 755 ~/peacock-gpu-idle/idle_watch.py /usr/local/lib/peacock-gpu-idle.py
    sudo install -m 644 ~/peacock-gpu-idle/peacock-gpu-idle.service ~/peacock-gpu-idle/peacock-gpu-idle.timer /etc/systemd/system/
    sudo systemctl daemon-reload
    sudo systemctl enable --now peacock-gpu-idle.timer
    systemctl is-enabled peacock-gpu-idle.timer'
}

verb=${1:-}; shift || true
case $verb in
  status) cmd_status;;
  ip) ip=$(current_ip); [ -n "$ip" ] || exit 1; echo "$ip";;
  start) cmd_start;;
  stop) cmd_stop "$@";;
  ssh) vm_ssh "$@";;
  rsh) shift; vm_ssh "$@";;
  install-watch) cmd_install_watch;;
  *) sed -n '2,16p' "$0" >&2; exit 64;;
esac
```

Fill the `classify_error` comment with Step 1's date and line. The `install-watch` body is
exercised by Task 3's hand check, not by these tests.

- [ ] **Step 5: Run the tests and see them pass**

Run: `chmod +x scripts/gpu-host.sh && python3 scripts/gpu-host/tests/test_gpu_host.py`
Expected: every test passes.

Then: `shellcheck scripts/gpu-host.sh` (install with `sudo apt-get install shellcheck` if absent).
Expected: no findings, or only `SC2029` on lines that deliberately expand on the client side,
each silenced with a `# shellcheck disable=SC2029` that names why.

- [ ] **Step 6: Commit**

```bash
git add scripts/gpu-host.sh scripts/gpu-host/tests/
git commit -m "gpu-host.sh: the one door to nebius-gpu — state, IP, host key, start, stop"
```

---

### Task 2: `scripts/gpu-host-sync.sh` and `scripts/gpu-host-test.sh`

**Files:**
- Create: `scripts/gpu-host-sync.sh`
- Create: `scripts/gpu-host-test.sh`
- Create: `scripts/gpu-host/tests/test_gpu_host_test.py`

**Interfaces:**
- Consumes: `gpu-host.sh ssh` and `gpu-host.sh rsh` (Task 1).
- Produces:
  - `gpu-host-sync.sh [--dest <dir-on-vm>]`. The default dest is `peacockdb-<basename of the
    repo root>`, for example `peacockdb-peacockdb-alpha`. It prints the dest.
  - `gpu-host-test.sh run [--foreground] <tree>`, `gpu-host-test.sh status <tree>`. Both run
    on the VM. State lives in `<tree>/.gpu-test/`: one `<binary>.log` and `<binary>.rc` per
    binary, plus `ALL.rc`. Overrides: `GPU_HOST_CUDF_LIB` (default
    `$HOME/data/miniforge3/envs/rapids-cuda-12.2/lib`) and `PCK_TEST_FILTER`.

- [ ] **Step 1: Write the failing tests for the test list**

`scripts/gpu-host/tests/test_gpu_host_test.py` builds a fake tree whose "binaries" are shell
stubs. Each stub records its argv and prints a test-runner footer:

```python
"""gpu-host-test.sh's list, over stub binaries: what runs, with what, and what is red."""
import pathlib, stat, subprocess, sys, tempfile
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
from harness import main  # noqa: E402

SCRIPT = str(pathlib.Path(__file__).resolve().parents[2] / "gpu-host-test.sh")
GTEST_OK = 'echo "$0 $*" >> "$TREE/argv"; echo "[  PASSED  ] 3 tests."'
GTEST_ZERO = 'echo "$0 $*" >> "$TREE/argv"; echo "[  PASSED  ] 0 tests."'
RUST_OK = 'echo "$0 $*" >> "$TREE/argv"; echo "running 2 tests"; echo "test result: ok. 2 passed; 0 failed"'
RUST_FAIL = 'echo "$0 $*" >> "$TREE/argv"; echo "running 1 tests"; echo "test result: FAILED. 0 passed; 1 failed"; exit 101'


def tree(bins=None, rust=None):
    t = pathlib.Path(tempfile.mkdtemp(prefix="gputree-"))
    (t / "cpp/install/bin").mkdir(parents=True); (t / "cpp/install/rust-tests").mkdir(parents=True)
    (t / "testdata").mkdir()
    bins = bins if bins is not None else {"peacock_gpu_tests": GTEST_OK, "peacock_plan_tests": GTEST_OK,
                                           "peacock_tpch_tests": GTEST_OK, "peacock_tpchv_tests": GTEST_OK}
    rust = rust if rust is not None else {"test_gpu_corpus": RUST_OK, "peacockdb_core_gpu_lib": RUST_OK,
                                          "peacock_gpu_benchmarks": RUST_OK, "test_node_timing": RUST_OK}
    for d, group in (("bin", bins), ("rust-tests", rust)):
        for name, body in group.items():
            f = t / "cpp/install" / d / name
            f.write_text("#!/bin/bash\n" + body + "\n"); f.chmod(f.stat().st_mode | stat.S_IEXEC)
    return t


def run(t, *extra):
    p = subprocess.run([SCRIPT, "run", "--foreground", *extra, str(t)], capture_output=True, text=True,
                       timeout=60, env={"PATH": "/usr/bin:/bin", "TREE": str(t), "HOME": str(t)})
    return p.returncode, p.stdout + p.stderr, (t / "argv").read_text() if (t / "argv").exists() else ""


def test_sf40_pair_never_runs():
    rc, out, argv = run(tree())
    assert rc == 0, out
    assert "peacock_tpch_tests" not in argv and "peacock_tpchv_tests" not in argv


def test_rust_args():
    rc, out, argv = run(tree())
    lines = argv.splitlines()
    assert any(l.endswith("peacock_gpu_benchmarks --nocapture --test-threads=1 --skip bench_") for l in lines), argv
    assert any("peacockdb_core_gpu_lib --nocapture --test-threads=1 gpu_tests::" in l for l in lines), argv
    assert any(l.endswith("test_gpu_corpus --nocapture --test-threads=1") for l in lines), argv


def test_failure_is_red_and_the_rest_still_run():
    t = tree(rust={"test_gpu_corpus": RUST_FAIL, "test_node_timing": RUST_OK})
    rc, out, argv = run(t)
    assert rc != 0 and "test_node_timing" in argv
    assert (t / ".gpu-test/test_gpu_corpus.rc").read_text().strip() != "0"


def test_zero_tests_is_red():
    rc, out, _ = run(tree(bins={"peacock_gpu_tests": GTEST_ZERO}))
    assert rc != 0 and "ran 0 tests" in out


def test_missing_binary_dir_is_red():
    t = tree(bins={}, rust={})
    rc, out, _ = run(t)
    assert rc != 0 and "no peacock_*_tests" in out


def test_status_after_run():
    t = tree()
    run(t)
    p = subprocess.run([SCRIPT, "status", str(t)], capture_output=True, text=True, timeout=30)
    assert "ALL rc=0" in p.stdout, p.stdout


if __name__ == "__main__":
    main(globals())
```

- [ ] **Step 2: Run them and see them fail**

Run: `python3 scripts/gpu-host/tests/test_gpu_host_test.py`
Expected: every test fails, because `gpu-host-test.sh` does not exist.

- [ ] **Step 3: Write `scripts/gpu-host-test.sh`**

It runs on the VM. The list is the one in `build-test.md`'s GPU-host section; keep the two in
agreement.

```bash
#!/usr/bin/env bash
# The L40S test list, run on nebius-gpu against a tree holding cpp/install/ (llm-wiki/build-test.md,
# "GPU host"). One list for the developer and for CI.
#
#   gpu-host-test.sh run [--foreground] <tree>   detached by default; logs and rc files in <tree>/.gpu-test/
#   gpu-host-test.sh status <tree>               which binaries are done, running or failed
#
# Out of the list: the sf40 pair (peacock_tpch_tests, peacock_tpchv_tests) and every bench_ case.
set -uo pipefail

SF40=" peacock_tpch_tests peacock_tpchv_tests "
CUDF_LIB=${GPU_HOST_CUDF_LIB:-$HOME/data/miniforge3/envs/rapids-cuda-12.2/lib}

run_all() {  # <tree>; every binary runs even after a failure; rc is the OR of all of them
  local tree=$1 st=$1/.gpu-test rc=0 ran_c=0 ran_r=0 t name trc args
  mkdir -p "$st"; rm -f "$st"/*.rc
  export LD_LIBRARY_PATH=$tree/cpp/install/lib:$CUDF_LIB PEACOCK_TESTDATA_DIR=$tree/testdata
  for t in "$tree"/cpp/install/bin/peacock_*_tests; do
    [ -x "$t" ] || continue; name=${t##*/}
    case $SF40 in *" $name "*) echo "--- $name skipped (sf40)"; continue;; esac
    echo "==> $name"; "$t" > "$st/$name.log" 2>&1; trc=$?
    cat "$st/$name.log"
    if grep -q '\[  PASSED  \] 0 tests' "$st/$name.log"; then echo "::error::$name ran 0 tests"; trc=1; fi
    echo "$trc" > "$st/$name.rc"; [ "$trc" -eq 0 ] || rc=1; ran_c=$((ran_c + 1))
  done
  [ "$ran_c" -gt 0 ] || { echo "::error::no peacock_*_tests binaries in $tree/cpp/install/bin"; rc=1; }
  for t in "$tree"/cpp/install/rust-tests/*; do
    [ -x "$t" ] || continue; name=${t##*/}
    args=(--nocapture --test-threads=1)
    case $name in
      peacockdb_core_gpu_lib) args+=(gpu_tests::);;
      peacock_gpu_benchmarks) args+=(--skip bench_);;
    esac
    [ -n "${PCK_TEST_FILTER:-}" ] && [ "$name" != peacockdb_core_gpu_lib ] && args+=("$PCK_TEST_FILTER")
    echo "==> $name ${args[*]}"; "$t" "${args[@]}" > "$st/$name.log" 2>&1; trc=$?
    cat "$st/$name.log"
    if grep -q '^running 0 tests' "$st/$name.log"; then echo "::error::$name ran 0 tests"; trc=1; fi
    echo "$trc" > "$st/$name.rc"; [ "$trc" -eq 0 ] || rc=1; ran_r=$((ran_r + 1))
  done
  [ "$ran_r" -gt 0 ] || { echo "::error::no rust test binaries in $tree/cpp/install/rust-tests"; rc=1; }
  if grep -qsE 'Segmentation fault|core dumped' "$st"/*.log; then echo "::error::a segfault on the GPU host"; rc=1; fi
  echo "$rc" > "$st/ALL.rc"; return "$rc"
}

status() {  # <tree>
  local st=$1/.gpu-test f
  [ -d "$st" ] || { echo "no run in $1"; return 1; }
  for f in "$st"/*.log; do
    local n; n=$(basename "$f" .log)
    if [ -f "$st/$n.rc" ]; then echo "$n rc=$(cat "$st/$n.rc")"; else echo "$n running: $(tail -1 "$f")"; fi
  done
  if [ -f "$st/ALL.rc" ]; then echo "ALL rc=$(cat "$st/ALL.rc")"; else echo "ALL running"; fi
}

case ${1:-} in
  run)
    shift; fg=0; [ "${1:-}" = --foreground ] && { fg=1; shift; }
    tree=$(cd "${1:?tree}" && pwd)
    if [ "$fg" -eq 1 ]; then run_all "$tree"; exit $?; fi
    mkdir -p "$tree/.gpu-test"; rm -f "$tree/.gpu-test/ALL.rc"
    nohup "$0" run --foreground "$tree" > "$tree/.gpu-test/run.out" 2>&1 < /dev/null &
    echo "detached: pid $! — $0 status $tree";;
  status) status "$(cd "${2:?tree}" && pwd)";;
  *) sed -n '2,8p' "$0" >&2; exit 64;;
esac
```

`PCK_TEST_FILTER` on the lib binary is left out on purpose. Filtering inside a rung needs
`scripts/lib/rung-args.sh`'s listing, and that is a developer convenience this task does not need.

- [ ] **Step 4: Run the tests and see them pass**

Run: `chmod +x scripts/gpu-host-test.sh && python3 scripts/gpu-host/tests/test_gpu_host_test.py`
Expected: every test passes. Then `shellcheck scripts/gpu-host-test.sh`: clean.

- [ ] **Step 5: Write `scripts/gpu-host-sync.sh`**

```bash
#!/usr/bin/env bash
# The working tree, uncommitted, to nebius-gpu (llm-wiki/build-test.md, "GPU host"). Chain J's
# rsync: ignored build dirs, target dirs and generated data survive on both sides. Then the sf1
# data is generated on the VM if it is missing.
#
#   gpu-host-sync.sh [--dest <dir under ~ on the VM>]     prints the dest
set -euo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(git -C "$here" rev-parse --show-toplevel)
dest=peacockdb-$(basename "$root")
[ "${1:-}" = --dest ] && dest=${2:?dest}
cd "$root"
attempt=1
# --delete-after, not --delete: the first sync must not delete before the .gitignore files
# arrive, or it deletes the ignored data and build dirs it should keep.
until rsync -a --timeout=90 --delete-after --exclude=.git --filter=':- .gitignore' \
        -e "$here/gpu-host.sh rsh" ./ "nebius-gpu:$dest/"; do
  [ "$attempt" -ge 5 ] && { echo "gpu-host-sync: rsync failed $attempt times" >&2; exit 1; }
  echo "gpu-host-sync: rsync attempt $attempt failed; retrying in $((attempt * 15))s" >&2
  sleep $((attempt * 15)); attempt=$((attempt + 1))
done
"$here/gpu-host.sh" ssh "cd ~/$dest && PEACOCK_NO_FETCH=1 testdata/generate_testdata.sh --bench tpch \
  && PEACOCK_NO_FETCH=1 testdata/generate_testdata.sh --bench tpcds" >&2
echo "$dest"
```

Check that `generate_testdata.sh --bench tpch` skips when its data is present. The CI step
"Generate SF-1 testdata (skips if present)" relies on this. Read the script's top and confirm
before relying on it; if it does not skip, wrap the call in a test for the dataset's
`lineitem.parquet` / `store_sales.parquet`.

`gpu-host-sync.sh` has no unit test: it is one rsync line and one ssh. It is verified for real in
Task 7.

- [ ] **Step 6: Commit**

```bash
chmod +x scripts/gpu-host-sync.sh
git add scripts/gpu-host-sync.sh scripts/gpu-host-test.sh scripts/gpu-host/tests/test_gpu_host_test.py
git commit -m "gpu-host: sync a tree to the VM and run the L40S list there, detached"
```

---

### Task 3: The idle watcher

**Files:**
- Create: `scripts/gpu-host/idle_watch.py`
- Create: `scripts/gpu-host/peacock-gpu-idle.service`, `scripts/gpu-host/peacock-gpu-idle.timer`
- Create: `scripts/gpu-host/tests/test_idle_watch.py`

**Interfaces:**
- Consumes: nothing from Tasks 1–2 at run time. `gpu-host.sh install-watch` (Task 1) installs it.
- Produces:
  - `idle_watch.decide(state: dict, now: float, boot: float, card: list[dict], tests: bool,
    build: bool) -> tuple[dict, str, str]`, returning `(new_state, decision, reason)`. The
    decision is one of `keep`, `idle-stop`, `cap-stop`. `card` items are
    `{"pid": int, "name": str, "start": float}`. `state` keys are `last_test`, `last_build`
    (epoch floats) and `boot`.
  - The log line, JSON, one per tick, in `/var/log/peacock-gpu-idle.log`: `{"t", "boot",
    "card", "tests", "build", "last_test", "last_build", "decision", "reason"}`. Task 4 reads it.
  - Constants `IDLE = 3600`, `IDLE_BUILD = 7200`, `CAP = 14400`.

- [ ] **Step 1: Write the failing tests**

```python
"""The watcher's decision, as a function of a process snapshot and a clock."""
import pathlib, sys
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
import idle_watch as w  # noqa: E402
from harness import main  # noqa: E402

BOOT = 1_000_000.0
H = 3600.0


def tick(state, at, card=(), tests=False, build=False):
    return w.decide(state, BOOT + at, BOOT, list(card), tests, build)


def test_fresh_boot_keeps():
    _, d, _ = tick({}, 60)
    assert d == "keep"


def test_fresh_boot_over_old_state_keeps():
    old = {"last_test": BOOT - 7 * 24 * H, "last_build": BOOT - 7 * 24 * H, "boot": BOOT - 8 * 24 * H}
    _, d, _ = tick(old, 60)
    assert d == "keep"


def test_idle_hour_stops():
    s, _, _ = tick({}, 0)
    _, d, r = tick(s, H + 1)
    assert d == "idle-stop", r


def test_test_activity_resets():
    s, _, _ = tick({}, 0)
    s, _, _ = tick(s, 0.9 * H, tests=True)
    _, d, _ = tick(s, 1.5 * H)
    assert d == "keep"


def test_build_extends_to_two_hours():
    s, _, _ = tick({}, 0)
    s, _, _ = tick(s, 0.5 * H, build=True)
    _, d, _ = tick(s, 1.2 * H)
    assert d == "keep"
    _, d, _ = tick(s, 2 * H + 1)
    assert d == "idle-stop"


def test_old_build_does_not_extend():
    s, _, _ = tick({}, 0)
    s, _, _ = tick(s, 0.1 * H, build=True)
    s, _, _ = tick(s, 1.5 * H, tests=True)
    _, d, _ = tick(s, 2.6 * H)
    assert d == "idle-stop"


def test_card_process_is_test_activity():
    s, _, _ = tick({}, 0)
    card = [{"pid": 7, "name": "test_gpu_corpus", "start": BOOT + 0.5 * H}]
    s, _, _ = tick(s, 1.5 * H, card=card)
    _, d, _ = tick(s, 2.0 * H, card=card)
    assert d == "keep"


def test_four_hour_cap():
    card = [{"pid": 7, "name": "test_gpu_corpus", "start": BOOT + 10}]
    s, _, _ = tick({}, 0, card=card)
    _, d, r = tick(s, 4 * H + 11, card=card)
    assert d == "cap-stop" and "7" in r


def test_log_line_has_every_field():
    s, d, r = tick({}, 60)
    line = w.log_line(BOOT + 60, BOOT, [], False, False, s, d, r)
    for k in ("t", "boot", "card", "tests", "build", "last_test", "last_build", "decision", "reason"):
        assert k in line


if __name__ == "__main__":
    main(globals())
```

- [ ] **Step 2: Run them and see them fail**

Run: `python3 scripts/gpu-host/tests/test_idle_watch.py`
Expected: ImportError, because `idle_watch` does not exist.

- [ ] **Step 3: Write `scripts/gpu-host/idle_watch.py`**

```python
#!/usr/bin/env python3
"""nebius-gpu's idle watcher, one tick per minute from a systemd timer (llm-wiki/build-test.md, "GPU host").

Stops the instance through the API when no test has run for an hour, two if a build ran in
that time, or when one process has held the card for four hours. Clocks never start before
boot, so a VM restarted after days stopped is not stopped on its first tick.
"""
from __future__ import annotations

import json, os, subprocess, sys, time

INSTANCE_ID = "computeinstance-e00tnrse7ayntnzcyt"
IDLE, IDLE_BUILD, CAP = 3600, 7200, 14400
STATE = "/var/lib/peacock-gpu-idle/state.json"
LOG = "/var/log/peacock-gpu-idle.log"
TEST_NAMES = ("peacock_", "test_gpu_corpus", "test_node_timing", "peacockdb_core_gpu_lib")
BUILD_NAMES = ("rsync", "cargo", "cmake", "ninja", "nvcc", "cc1plus")


def decide(state, now, boot, card, tests, build):
    last_test = max(state.get("last_test", boot), boot)
    last_build = max(state.get("last_build", 0.0), boot)
    if tests or card:
        last_test = now
    if build:
        last_build = now
    new = {"last_test": last_test, "last_build": last_build, "boot": boot}
    for p in card:
        if now - max(p["start"], boot) >= CAP:
            return new, "cap-stop", f"pid {p['pid']} ({p['name']}) on the card for {int(now - p['start'])}s"
    idle = now - last_test
    build_in_window = last_build > boot and now - last_build < IDLE
    limit = IDLE_BUILD if build_in_window else IDLE
    if idle >= limit:
        return new, "idle-stop", f"no test for {int(idle)}s (limit {limit}s)"
    return new, "keep", f"idle {int(idle)}s of {limit}s"


def log_line(now, boot, card, tests, build, state, decision, reason):
    return {"t": now, "boot": boot, "card": card, "tests": tests, "build": build,
            "last_test": state["last_test"], "last_build": state["last_build"],
            "decision": decision, "reason": reason}


def _boot_time():
    with open("/proc/stat") as f:
        return float(next(l for l in f if l.startswith("btime")).split()[1])


def _card(now):
    out = subprocess.run(["nvidia-smi", "--query-compute-apps=pid,process_name", "--format=csv,noheader"],
                         capture_output=True, text=True).stdout
    procs = []
    for line in filter(None, (l.strip() for l in out.splitlines())):
        pid, name = (x.strip() for x in line.split(",", 1))
        et = subprocess.run(["ps", "-o", "etimes=", "-p", pid], capture_output=True, text=True).stdout.strip()
        procs.append({"pid": int(pid), "name": os.path.basename(name), "start": now - float(et or 0)})
    return procs


def _running(prefixes):
    names = subprocess.run(["ps", "-eo", "comm="], capture_output=True, text=True).stdout.split()
    return any(n.startswith(prefixes) for n in names)


def main():
    now, boot = time.time(), _boot_time()
    try:
        state = json.load(open(STATE))
    except (OSError, ValueError):
        state = {}
    card = _card(now)
    tests, build = _running(TEST_NAMES), _running(BUILD_NAMES)
    new, decision, reason = decide(state, now, boot, card, tests, build)
    json.dump(new, open(STATE, "w"))
    with open(LOG, "a") as f:
        f.write(json.dumps(log_line(now, boot, card, tests, build, new, decision, reason)) + "\n")
    if decision != "keep":
        subprocess.run(["/usr/local/bin/nebius", "compute", "instance", "stop", "--id", INSTANCE_ID, "--async"],
                       env={**os.environ, "HOME": "/root"})
    return 0


if __name__ == "__main__":
    sys.exit(main())
```

Before relying on `--async`, check that `nebius compute instance stop --help` lists it. If it
does not, drop the flag: the stop then blocks the tick until STOPPED, which is harmless.

- [ ] **Step 4: Run the tests and see them pass**

Run: `python3 scripts/gpu-host/tests/test_idle_watch.py`
Expected: every test passes.

- [ ] **Step 5: Write the systemd units**

`scripts/gpu-host/peacock-gpu-idle.service`:
```ini
[Unit]
Description=nebius-gpu idle watcher, one tick (scripts/gpu-host/idle_watch.py)

[Service]
Type=oneshot
ExecStart=/usr/bin/python3 /usr/local/lib/peacock-gpu-idle.py
```

`scripts/gpu-host/peacock-gpu-idle.timer`:
```ini
[Unit]
Description=nebius-gpu idle watcher, every minute; survives reboots

[Timer]
OnBootSec=1min
OnUnitActiveSec=1min
AccuracySec=10s
Persistent=true

[Install]
WantedBy=timers.target
```

- [ ] **Step 6: Commit**

```bash
git add scripts/gpu-host/idle_watch.py scripts/gpu-host/peacock-gpu-idle.* scripts/gpu-host/tests/test_idle_watch.py
git commit -m "gpu-host: the idle watcher — 1 h idle, 2 h after a build, 4 h cap, from boot"
```

---

### Task 4: The hourly checker

**Files:**
- Create: `scripts/gpu-host/check.py`
- Create: `scripts/gpu-host/tests/test_check.py`
- Create: `.github/workflows/gpu-host-watch.yml`

**Interfaces:**
- Consumes: the watcher's JSON log line (Task 3); `gpu-host.sh status|ssh|stop --force` (Task 1).
- Produces: `check.verdict(last_line: str | None, now: float) -> str | None`. It returns a
  reason to hard-stop, or None. Run as `python3 check.py <now>` with the line on stdin. It prints
  the reason and exits 1 to stop, or exits 0 to keep.

- [ ] **Step 1: Write the failing tests**

```python
"""The checker's verdict over the watcher's last log line."""
import json, pathlib, sys
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
import check  # noqa: E402
from harness import main  # noqa: E402

NOW = 2_000_000.0
H = 3600.0


def line(**kv):
    base = {"t": NOW - 60, "boot": NOW - 5 * H, "card": [], "tests": False, "build": False,
            "last_test": NOW - 600, "last_build": NOW - 5 * H, "decision": "keep", "reason": ""}
    return json.dumps({**base, **kv})


def test_fresh_keep_is_none():
    assert check.verdict(line(), NOW) is None


def test_no_log_is_stop():
    assert "no watcher log" in check.verdict(None, NOW)


def test_stale_log_is_stop():
    assert "stale" in check.verdict(line(t=NOW - H - 1), NOW)


def test_idle_recomputed_is_stop():
    assert check.verdict(line(last_test=NOW - H - 120), NOW) is not None


def test_build_window_keeps():
    assert check.verdict(line(last_test=NOW - 1.5 * H, last_build=NOW - 0.5 * H), NOW) is None


def test_cap_recomputed_is_stop():
    card = [{"pid": 9, "name": "test_gpu_corpus", "start": NOW - 4 * H - 1}]
    assert "pid 9" in check.verdict(line(card=card, last_test=NOW - 60), NOW)


def test_watcher_said_stop_is_stop():
    assert check.verdict(line(decision="idle-stop", reason="x"), NOW) is not None


def test_garbage_line_is_stop():
    assert "unreadable" in check.verdict("not json", NOW)


if __name__ == "__main__":
    main(globals())
```

- [ ] **Step 2: Run them and see them fail**

Run: `python3 scripts/gpu-host/tests/test_check.py`
Expected: ImportError.

- [ ] **Step 3: Write `scripts/gpu-host/check.py`**

It reuses the watcher's `decide`, so the two guards cannot drift apart:

```python
#!/usr/bin/env python3
"""The hourly checker's verdict over the idle watcher's last log line (gpu-host-watch.yml).

A reason to hard-stop, or None. It reuses idle_watch.decide, so the two guards apply one rule;
on top of it, a watcher whose log is over an hour old is itself a reason.
"""
from __future__ import annotations

import json, pathlib, sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import idle_watch  # noqa: E402

STALE = 3600


def verdict(last_line, now):
    if not last_line:
        return "no watcher log on the VM"
    try:
        l = json.loads(last_line)
    except ValueError:
        return "the watcher's last log line is unreadable"
    if now - l["t"] > STALE:
        return f"the watcher's log is stale: last line {int(now - l['t'])}s old"
    if l["decision"] != "keep":
        return f"the watcher decided {l['decision']} ({l['reason']}) and the VM still runs"
    state = {"last_test": l["last_test"], "last_build": l["last_build"]}
    _, decision, reason = idle_watch.decide(state, now, l["boot"], l["card"], False, False)
    return None if decision == "keep" else f"{decision}: {reason}"


if __name__ == "__main__":
    line = sys.stdin.read().strip().splitlines()
    r = verdict(line[-1] if line else None, float(sys.argv[1]))
    if r:
        print(r)
        sys.exit(1)
    print("keep")
```

- [ ] **Step 4: Run the tests and see them pass**

Run: `python3 scripts/gpu-host/tests/test_check.py`
Expected: every test passes.

- [ ] **Step 5: Write `.github/workflows/gpu-host-watch.yml`**

Its setup steps are the same as Task 5's `gpu-tests` setup. Both use the composite action
`.github/actions/nebius-setup/action.yml`, which Task 5 creates. Write this workflow after Task 5's
Step 2, or create the action here and let Task 5 reuse it.

```yaml
name: GPU host watch

# The second of nebius-gpu's two idle guards (llm-wiki/build-test.md, "GPU host"). The first is
# the watcher on the VM; this one runs whether or not the VM, its watcher or its network is
# alive. Hard stop: anything this finds is something no party is going to clean up.
on:
  schedule:
    - cron: '0 * * * *'
  workflow_dispatch:

concurrency:
  group: gpu-host-watch
  cancel-in-progress: false

jobs:
  watch:
    runs-on: ubuntu-latest
    environment: nebius-gpu
    timeout-minutes: 30
    steps:
      - uses: actions/checkout@v4
      - uses: ./.github/actions/nebius-setup
        with:
          sa-id: ${{ secrets.NEBIUS_SA_ID }}
          public-key-id: ${{ secrets.NEBIUS_PUBLIC_KEY_ID }}
          project-id: ${{ secrets.NEBIUS_PROJECT_ID }}
          private-key: ${{ secrets.NEBIUS_PRIVATE_KEY }}
          ssh-key: ${{ secrets.NEBIUS_GPU_SSH_KEY }}
      - name: Check, and hard-stop if nobody is using it
        run: |
          set -uo pipefail
          s=$(scripts/gpu-host.sh status) || { echo "status failed rc=$?"; exit 1; }
          echo "state: $s"
          case $s in RUNNING*) ;; *) echo "not running; nothing to do"; exit 0;; esac
          line=""; ok=0
          for i in $(seq 15); do   # a 15-minute window: a VM still booting answers well inside it
            if line=$(scripts/gpu-host.sh ssh 'tail -1 /var/log/peacock-gpu-idle.log 2>/dev/null; true'); then ok=1; break; fi
            sleep 60
          done
          if [ "$ok" -eq 0 ]; then reason="ssh unanswered for 15 minutes"
          else reason=$(printf '%s\n' "$line" | python3 scripts/gpu-host/check.py "$(date +%s)") && reason=""; fi
          if [ -n "$reason" ]; then
            echo "::warning::hard-stopping nebius-gpu: $reason"
            scripts/gpu-host.sh stop --force
          else
            echo "in use; keeping it"
          fi
```

- [ ] **Step 6: Commit**

```bash
git add scripts/gpu-host/check.py scripts/gpu-host/tests/test_check.py .github/workflows/gpu-host-watch.yml
git commit -m "gpu-host: the hourly checker, a hard stop on a dead watcher, idleness or the cap"
```

---

### Task 5: The GPU job on nebius-gpu, and who may run it

**Files:**
- Create: `.github/actions/nebius-setup/action.yml`
- Create: `scripts/gpu-host/check-actor.sh`
- Create: `scripts/gpu-host/tests/test_check_actor.py`
- Modify: `.github/workflows/pipeline.yml`: `on:`; the `gpu-tests` job (today lines 411–679); a
  script-tests step beside "Calibration script tests (Python)" in the `cost-report` job.

**Interfaces:**
- Consumes: `gpu-host.sh start|ssh|rsh|stop` (Task 1); `gpu-host-test.sh run --foreground`
  (Task 2).
- Produces: `check-actor.sh <actor> <comma-separated allowlist>`, exit 0 if the actor is listed;
  `pipeline.yml` dispatchable as `gh workflow run pipeline.yml --ref <branch>`.

- [ ] **Step 1: The actor check, test first**

`scripts/gpu-host/tests/test_check_actor.py`:
```python
"""Only an actor on GPU_ALLOWED_ACTORS reaches the GPU host."""
import pathlib, subprocess, sys
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
from harness import main  # noqa: E402

SCRIPT = str(pathlib.Path(__file__).resolve().parents[1] / "check-actor.sh")


def rc(actor, allowed):
    return subprocess.run([SCRIPT, actor, allowed], capture_output=True, text=True).returncode


def test_listed_passes():
    assert rc("alephonea", "alephonea,romak00") == 0


def test_unlisted_fails():
    assert rc("stranger", "alephonea,romak00") != 0


def test_empty_list_fails():
    assert rc("alephonea", "") != 0


def test_no_substring_match():
    assert rc("alephone", "alephonea") != 0 and rc("x", "x-y") != 0


def test_spaces_tolerated():
    assert rc("romak00", "alephonea, romak00") == 0


if __name__ == "__main__":
    main(globals())
```

Run `python3 scripts/gpu-host/tests/test_check_actor.py` and expect failures. Then write
`scripts/gpu-host/check-actor.sh`:

```bash
#!/usr/bin/env bash
# check-actor.sh <actor> <allowlist>: exit 0 only if <actor> is a whole entry of the
# comma-separated <allowlist> (the repo variable GPU_ALLOWED_ACTORS). An empty list admits nobody.
set -euo pipefail
actor=${1:?actor}; list=${2:-}
IFS=, read -ra names <<<"$list"
for n in "${names[@]}"; do
  n=${n//[[:space:]]/}
  [ -n "$n" ] && [ "$n" = "$actor" ] && exit 0
done
echo "::error::$actor is not in GPU_ALLOWED_ACTORS; the GPU host is not for this run" >&2
exit 1
```

Run the test again and expect a pass.

- [ ] **Step 2: The composite setup action**

`.github/actions/nebius-setup/action.yml`. Before writing the profile command, run
`nebius profile create --help` on dev and use its flag names for the service account ID, the
public key ID, the private key file and the parent ID. Write the exact flags into the step and a
comment saying when you checked them. Do not read `~/.nebius/config.yaml` to find them.

```yaml
name: nebius setup
description: The nebius CLI with a service-account profile, and the VM's ssh key.
inputs:
  sa-id: {required: true}
  public-key-id: {required: true}
  project-id: {required: true}
  private-key: {required: true}
  ssh-key: {required: true}
runs:
  using: composite
  steps:
    - shell: bash
      env:
        SA_ID: ${{ inputs.sa-id }}
        PK_ID: ${{ inputs.public-key-id }}
        PROJECT_ID: ${{ inputs.project-id }}
        PRIVATE_KEY: ${{ inputs.private-key }}
        SSH_KEY: ${{ inputs.ssh-key }}
      run: |
        set -euo pipefail
        curl -sSL https://storage.eu-north1.nebius.cloud/cli/install.sh | bash
        echo "$HOME/.nebius/bin" >> "$GITHUB_PATH"
        install -d -m 700 "$RUNNER_TEMP/nebius"
        printf '%s\n' "$PRIVATE_KEY" > "$RUNNER_TEMP/nebius/sa.pem"; chmod 600 "$RUNNER_TEMP/nebius/sa.pem"
        # Flags from `nebius profile create --help`, checked <date>.
        "$HOME/.nebius/bin/nebius" profile create ci \
          --endpoint api.nebius.cloud \
          --service-account-id "$SA_ID" --public-key-id "$PK_ID" \
          --private-key-file "$RUNNER_TEMP/nebius/sa.pem" --parent-id "$PROJECT_ID"
        install -d -m 700 ~/.ssh
        printf '%s\n' "$SSH_KEY" > ~/.ssh/nebius_gpu; chmod 600 ~/.ssh/nebius_gpu
        echo "GPU_HOST_KEY=$HOME/.ssh/nebius_gpu" >> "$GITHUB_ENV"
```

- [ ] **Step 3: Rewrite `gpu-tests` in `pipeline.yml`**

Add `workflow_dispatch:` under `on:`, next to `push` and `pull_request`. Replace the whole
`gpu-tests` job, from `gpu-tests:` up to and not including `cost-report:`, with:

```yaml
  # nebius-gpu, an L40S VM started for this job and soft-stopped after it (llm-wiki/build-test.md,
  # "GPU host"). It runs on master and on a dispatch, never on a pull_request: a PR runs it when
  # someone with write access, listed in GPU_ALLOWED_ACTORS, dispatches it on the PR's branch.
  # The secrets live in the nebius-gpu Environment, which only this job and gpu-host-watch use.
  gpu-tests:
    name: GPU Tests (nebius-gpu)
    needs: cpp-build-2502
    if: >-
      (github.event_name == 'push' && github.ref == 'refs/heads/master')
      || github.event_name == 'workflow_dispatch'
    runs-on: ubuntu-latest
    environment: nebius-gpu
    timeout-minutes: 150
    concurrency:
      group: nebius-gpu
      cancel-in-progress: false
    env:
      REMOTE_DIR: peacockdb-ci/run-${{ github.run_id }}-${{ github.run_attempt }}
    steps:
      - name: Only allowed actors reach the GPU host
        # Through env, never ${{ }} inside the script: an expression expanded into `run:` is
        # a script-injection hole.
        env:
          ACTOR: ${{ github.actor }}
          ALLOWED: ${{ vars.GPU_ALLOWED_ACTORS }}
          SRC: https://raw.githubusercontent.com/${{ github.repository }}/${{ github.sha }}/scripts/gpu-host/check-actor.sh
        run: |
          curl -sSfL "$SRC" -o "$RUNNER_TEMP/check-actor.sh"
          bash "$RUNNER_TEMP/check-actor.sh" "$ACTOR" "$ALLOWED"
      - uses: actions/checkout@v4
      - uses: actions/download-artifact@v4
        with:
          name: cpp-install-25.02
          path: cpp/install/
      - uses: ./.github/actions/nebius-setup
        with:
          sa-id: ${{ secrets.NEBIUS_SA_ID }}
          public-key-id: ${{ secrets.NEBIUS_PUBLIC_KEY_ID }}
          project-id: ${{ secrets.NEBIUS_PROJECT_ID }}
          private-key: ${{ secrets.NEBIUS_PRIVATE_KEY }}
          ssh-key: ${{ secrets.NEBIUS_GPU_SSH_KEY }}
      - name: Start nebius-gpu (up to 30 minutes for capacity)
        run: |
          for i in 1 2 3 4; do
            rc=0; scripts/gpu-host.sh start || rc=$?
            [ "$rc" -eq 0 ] && exit 0
            [ "$rc" -eq 75 ] || exit "$rc"
            echo "no capacity (attempt $i); retrying in 7 minutes"; sleep 420
          done
          exit 75
      - name: Ship binaries, scripts and testdata
        timeout-minutes: 20
        run: |
          chmod +x cpp/install/bin/* cpp/install/rust-tests/* 2>/dev/null || true
          scripts/gpu-host.sh ssh "mkdir -p ~/$REMOTE_DIR/testdata ~/$REMOTE_DIR/scripts"
          rs() { rsync -a --timeout=300 -e "scripts/gpu-host.sh rsh" "$@"; }
          rs cpp/install nebius-gpu:$REMOTE_DIR/cpp/
          rs scripts/gpu-host-test.sh nebius-gpu:$REMOTE_DIR/scripts/
          rs testdata/tpch-queries testdata/tpcds-queries testdata/tpch.minimal testdata/goldens \
             testdata/tpch-vec-queries testdata/generate_testdata.sh testdata/cost-registry.csv \
             nebius-gpu:$REMOTE_DIR/testdata/
      - name: Generate SF-1 testdata (skips if present)
        run: |
          scripts/gpu-host.sh ssh "cd ~/$REMOTE_DIR && PEACOCK_NO_FETCH=1 testdata/generate_testdata.sh --bench tpch \
            && PEACOCK_NO_FETCH=1 testdata/generate_testdata.sh --bench tpcds"
      - name: Run the L40S list
        run: scripts/gpu-host.sh ssh "~/$REMOTE_DIR/scripts/gpu-host-test.sh run --foreground ~/$REMOTE_DIR"
      - name: Clean up and soft-stop
        if: always()
        run: |
          scripts/gpu-host.sh ssh "rm -rf ~/$REMOTE_DIR" || true
          scripts/gpu-host.sh stop || echo "soft stop declined (rc=$?); the watcher will take it down"
```

The actor check runs before checkout, from the commit's own copy of the script, so nothing in
the job runs before it. If `raw.githubusercontent.com` is unwelcome, move it after `checkout`:
checkout runs no project code.

Every `gpu-tests` comment about shad-gpu, glibc 2.35 and sf40 goes with the old job. The CI
data generation on shad-gpu set `DUCKDB=/home/info/.local/bin/duckdb`. Check `which duckdb` on
the VM; if it is not on `PATH`, set `DUCKDB` to its path in that step.

- [ ] **Step 4: The script tests and shellcheck run in CI**

In the `cost-report` job, after "Calibration script tests (Python)", add:

```yaml
      # gpu-host's scripts, against fake nebius/ssh and synthetic clocks: no VM, no secret.
      - name: GPU host script tests
        run: |
          set -euo pipefail
          shopt -s nullglob
          files=(scripts/gpu-host/tests/test_*.py)
          [ ${#files[@]} -gt 0 ] || { echo "no gpu-host test files matched"; exit 1; }
          status=0
          for f in "${files[@]}"; do echo "== $f"; python3 "$f" || status=1; done
          shellcheck scripts/gpu-host.sh scripts/gpu-host-sync.sh scripts/gpu-host-test.sh scripts/gpu-host/check-actor.sh || status=1
          exit "$status"
```

- [ ] **Step 5: Lint the workflows**

Run: `npx --yes actionlint@latest` from the repo root, or `docker run --rm -v "$PWD":/repo -w /repo rhysd/actionlint`.
Expected: no findings in `pipeline.yml`, `gpu-host-watch.yml` or the action.

- [ ] **Step 6: Commit**

```bash
chmod +x scripts/gpu-host/check-actor.sh
git add .github/ scripts/gpu-host/check-actor.sh scripts/gpu-host/tests/test_check_actor.py
git commit -m "CI: gpu-tests on nebius-gpu, on master and on an allowed actor's dispatch"
```

---

### Task 6: The wiki, the ensemble and the board

**Files:**
- Modify: `llm-wiki/prompts.md`: Shared rules; Coordinator; Developer.
- Modify: `.claude/commands/ensemble.md`, wherever it names a GPU host.
- Modify: `llm-wiki/build-test.md`: a new `## GPU host` section before `## Remote hosts`;
  `## CI structure` for `gpu-tests` and `gpu-host-watch`; `## Remote hosts` for shad-gpu's
  status.
- Modify: `llm-wiki/tasks/tasks.md`: chains J and K's host-override blocks.

**Interfaces:**
- Consumes: every name above, exactly as Tasks 1–5 define it.

- [ ] **Step 1: `prompts.md`, Shared rules.** Add this bullet:

```markdown
- **Never open a Nebius credential file.** `~/.nebius/config.yaml`, `~/.nebius/credentials.yaml`
  and any `*.pem` under `~/.nebius/`, on any host, the VM's copy included: no `cat`, `grep`,
  `sed`, Read, or redaction attempt. A redaction leaked a service account's private key into a
  transcript on 2026-10-14. Reach the cloud through `scripts/gpu-host.sh`; learn the CLI from
  `nebius … --help`.
```

- [ ] **Step 2: `prompts.md`, Coordinator.** Replace the "Check whether verda is up" bullet's GPU
wording, keeping the verda part for CPU runs. Add:

```markdown
- **The GPU host is nebius-gpu, started and stopped through `scripts/gpu-host.sh`**
  (`build-test.md`, "GPU host"). Before dispatching a round with a GPU step, `gpu-host.sh start`.
  On exit 75 (no capacity), retry every 15 minutes for 10 hours. Record the first failure's time
  in `<task>-detail.md` so a restart keeps the clock, and wait in bounded waits of at most 10
  minutes. After 10 hours, write `stalled: gpu host unavailable` and exit. Exit 69 (the instance
  is gone) is a stall at once: rebuilding it is the human's.
- **Hard stops are yours.** `gpu-host.sh stop --force` on entering `completeness approved`, on
  entering `done`, and before every exit: stalled, a tight window, or `stop`. Developers and CI
  stop softly.
- **The final device run is CI's.** In `completeness approved`, after the hard stop, run
  `gh workflow run pipeline.yml --ref <branch>`. Find the run with `gh run list --workflow
  pipeline.yml --branch <branch> --event workflow_dispatch --limit 1`, and wait on it in bounded
  waits. `done` needs its `GPU Tests (nebius-gpu)` green as well as the PR's CI. Then the hard
  stop, then `done`.
- **Contention is expected.** Other chains and CI share the VM. A run killed by someone else's
  stop is rerun, not diagnosed.
```

- [ ] **Step 3: `prompts.md`, Developer.** Add:

```markdown
- **A GPU step is bracketed.** `scripts/gpu-host.sh start` → `scripts/gpu-host-sync.sh` →
  `scripts/gpu-host.sh ssh 'cd ~/<dest> && . ~/peacock-env.sh && ./scripts/build-test-shadgpu.sh --build'`
  → `scripts/gpu-host.sh ssh '~/<dest>/scripts/gpu-host-test.sh run ~/<dest>'`, polled with
  `… status`, → `scripts/gpu-host.sh stop`. Only the L40S list runs: no sf40, no benchmarks.
  If the host is lost mid-step: `status`, `start` (on 75, report to the coordinator, which owns
  the retry), sync, rerun the step. Never edit `~/.ssh/config` for it, and never accept a host
  key by hand: `gpu-host.sh` does both.
```

Also update the Developer's "Definition of done" line: GPU tests green on nebius-gpu's L40S list,
not shad-gpu.

- [ ] **Step 4: `build-test.md`, a `## GPU host` section.** In short sentences, covering:
  - the instance, its card, its env, preemptible with STOP;
  - the three scripts and their verbs;
  - the exit codes;
  - the L40S list and what is out;
  - the watcher's rules and its log;
  - the hourly checker;
  - the two workflows;
  - the Environment, its five secrets and `GPU_ALLOWED_ACTORS`;
  - who soft-stops and who hard-stops;
  - the credential-file rule.

  In `## Remote hosts`, mark shad-gpu as down until further notice and point here. In
  `## CI structure`, rewrite the `gpu-tests` paragraph and add `gpu-host-watch.yml`.

- [ ] **Step 5: `tasks.md`.** Replace each of chains J and K's host-override blocks with:

```markdown
> **GPU host:** nebius-gpu through `scripts/gpu-host.sh`, `gpu-host-sync.sh` and `gpu-host-test.sh`
> (`build-test.md`, "GPU host"). The L40S list only; sf40 and benchmarks are out and deferred.
> The final device run is the dispatched `GPU Tests (nebius-gpu)` CI job.
```

Keep each chain's other rules (cost regressions, ticket numbers, resequencing) as they are.

- [ ] **Step 6: Check agreement, then commit.**
Run: `git grep -n 'shad-gpu\|89\.169\.' -- llm-wiki/prompts.md .claude/commands/ensemble.md llm-wiki/tasks/tasks.md`
Expected: no hit that tells an agent to use shad-gpu or a hard-coded nebius IP.

```bash
git add llm-wiki/prompts.md .claude/commands/ensemble.md llm-wiki/build-test.md llm-wiki/tasks/tasks.md
git commit -m "Wiki: nebius-gpu is the GPU host — lifecycle, hard stops, the credential-file rule"
```

---

### Task 7: The real VM, once, and the PR

Done by the helper with the human, not a subagent: it starts a paid VM and needs the human's
secrets. Every result goes into the PR description.

- [ ] **Step 1: Prerequisites.** The human has created the `nebius-gpu` Environment, its five
  secrets and `GPU_ALLOWED_ACTORS`, and has added the CI ssh public key to the VM's
  `~/.ssh/authorized_keys`.
- [ ] **Step 2: The door.** From dev: `scripts/gpu-host.sh status`, `start`, `ssh hostname`,
  `ip`, a soft `stop`, `start` again, `stop --force`. Expected: exit 0 each time; the IP printed
  after the second start may differ, and `ssh` still works.
- [ ] **Step 3: The chains wait for a rebase.** Chains J, K and L are not resumed until each is
  rebased onto master after this merge; until then their coordinators follow the old procedure.
  The human decides whether that rebase re-verifies the tasks (it brings `scripts/` and
  `.github/` changes).
- [ ] **Step 4: The watcher.** `start`, `install-watch`; `ssh 'sudo reboot'`; after it answers
  again: `ssh 'systemctl is-active peacock-gpu-idle.timer; tail -2 /var/log/peacock-gpu-idle.log'`.
  Expected: `install-watch` exits 0, which means root's profile reached the API; `active`; and
  fresh lines after the reboot, each with a `test_procs` field.
- [ ] **Step 5: The watcher's stop.** On the VM, run one tick by hand with the limits shortened:
  `ssh 'sudo python3 -c "import sys; sys.argv=[\"x\"]; sys.path.insert(0,\"/usr/local/lib\"); import importlib.util as u; s=u.spec_from_file_location(\"w\",\"/usr/local/lib/peacock-gpu-idle.py\"); w=u.module_from_spec(s); s.loader.exec_module(w); w.IDLE=0; w.main()"'`.
  Expected: `status` reaches STOPPED within minutes.
- [ ] **Step 6: The checker's verdict by hand.** `start`; wait two minutes;
  `scripts/gpu-host.sh ssh 'tail -1 /var/log/peacock-gpu-idle.log' | python3 scripts/gpu-host/check.py $(( $(date +%s) + 7200 ))`.
  Expected: a stop reason, exit 1. Then `stop --force`.
- [ ] **Step 7: Sync and the list.** `scripts/gpu-host.sh start && scripts/gpu-host-sync.sh`,
  `--build` on the VM, `gpu-host-test.sh run`, poll `status` to `ALL rc=0`, soft `stop`.
- [ ] **Step 8: Push, PR, dispatch.** Push the branch and open the PR against master. Run
  `gh workflow run pipeline.yml --ref gpu-host`. If GitHub refuses because master's
  `pipeline.yml` has no `workflow_dispatch`, land the one-line trigger on master first, as the
  spec says, and dispatch again. Expected: `GPU Tests (nebius-gpu)` green, and the VM STOPPED
  afterwards.
- [ ] **Step 9: An unlisted actor.** Temporarily set `GPU_ALLOWED_ACTORS` to a name that is not
  the dispatcher's, dispatch, and see the job fail at "Only allowed actors reach the GPU host".
  Restore the variable.
- [ ] **Step 10: After the merge.** Dispatch `gpu-host-watch.yml` once against a stopped VM
  (expected: "not running; nothing to do"). Then see it hard-stop an idle VM. `start` the VM,
  with the watcher installed. A live watcher on a VM idle under an hour reads as keep, so make
  the watcher look dead: `ssh 'sudo systemctl stop peacock-gpu-idle.timer && sudo mv
  /var/log/peacock-gpu-idle.log /var/log/peacock-gpu-idle.log.bak'`. Once the VM has been up
  more than 10 minutes, dispatch `gpu-host-watch.yml` again. Expected: "hard-stopping
  nebius-gpu: no watcher log on the VM", and `status` shows `STOPPED`. The timer stays enabled,
  so it runs again on the next boot. The next day, confirm hourly runs in the Actions list.
- [ ] **Step 11: Hard-stop the VM.** Whatever state the steps above left it in:
  `scripts/gpu-host.sh stop --force; scripts/gpu-host.sh status`. Expected: `STOPPED`. Record it in
  the PR description. The implementation ends with the VM down, not left to the watcher.
