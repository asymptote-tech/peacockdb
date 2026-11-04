#!/usr/bin/env bash
# vm.sh: comet-research's one door to its two VMs (llm-wiki/tasks/comet-research-impl.md, Task 2).
# It logs every use, counts hours against the caps, arms a dead-man stop, and force-stops what
# was used.
#
#   vm.sh use <vm> [--deadman-min N]   start or find the VM, record its boot id, arm the dead-man
#                                      stop at the minutes left in the cap (or N, if fewer)
#   vm.sh ssh <vm> <cmd…>              run a command; logs a heartbeat (time and boot id)
#   vm.sh status <vm> | ip <vm>        state (and IP when RUNNING) | the IP alone
#   vm.sh push-aws <vm>                copy dev's AWS profiles to the VM over ssh stdin
#   vm.sh stop <vm> [reason]           force-stop one VM (the local dead-man's command)
#   vm.sh stop-used                    force-stop every VM with an open use
#   vm.sh hours <vm>                   hours used, two decimals
#
# <vm> is cpu128 (the Nebius CLI, profile eu-west1) or l40s (scripts/gpu-host.sh). The H100 has
# no entry, by the human's decision. Exit codes as gpu-host.sh: 0 done; 75 no capacity, retry
# later; 69 instance gone; 1 anything else. A `use` that started the VM and then fails stops it.
#
# The log is `local_time<TAB>vm<TAB>event<TAB>detail`, events: use-requested, use (started|found),
# boot, heartbeat, deadman, stop, lost. A use is open from `use` to `stop` or `lost`; `lost`
# closes it at its last heartbeat with the same boot id. A started use counts from its request.
#
# NEVER print, cat or grep a Nebius credential file. This script reads ~/.aws only to copy the
# named profile sections, and never prints a value.
set -euo pipefail

HERE=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
SELF=$HERE/vm.sh
ROOT=$(git -C "$HERE" rev-parse --show-toplevel 2>/dev/null || true)
LOG=${COMET_VM_LOG:-$ROOT/.claude/ensemble/M.vm-log.tsv}
GPU_HOST=${COMET_GPU_HOST:-$ROOT/scripts/gpu-host.sh}
NEBIUS=${COMET_NEBIUS:-nebius}
POLL=${COMET_VM_POLL:-10}
START_TIMEOUT=${COMET_VM_START_TIMEOUT:-900}
CLI_TIMEOUT=${COMET_VM_CLI_TIMEOUT:-120}
# guest: a poweroff timer on the VM. local: a systemd --user timer on dev running `vm.sh stop`.
# Task 2 Step 4 checks the guest kind on the L40S; see comet-research-detail.md for the result.
DEADMAN=${COMET_DEADMAN:-guest}
KNOWN_HOSTS=$HOME/.ssh/known_hosts_comet
[ -x "$HOME/.nebius/bin/nebius" ] && PATH=$HOME/.nebius/bin:$PATH

CPU_ID=computeinstance-e01t2mm4265y8x88ey
CPU_PROFILE=eu-west1

cap_min() { case $1 in cpu128) echo 120;; l40s) echo 60;; esac; }
known_vm() {
  case ${1:-} in cpu128|l40s) return 0;; esac
  echo "vm.sh: unknown VM '${1:-}' (cpu128 or l40s)" >&2; return 1
}

logrow() {
  mkdir -p "$(dirname "$LOG")"
  printf '%s\t%s\t%s\t%s\n' "$(date -Is)" "$1" "$2" "${3:-}" >> "$LOG"
  echo "vm.sh: $1 $2 ${3:-}" >&2
}

# logq <query> [vm] [cap minutes]: read the log. Queries: open (exit status), boot (the open
# use's boot id), hours, left (whole minutes left in the cap), open-vms.
logq() {
  python3 - "$LOG" "$@" <<'PY'
import datetime as dt, math, sys
path, query = sys.argv[1], sys.argv[2]
vm = sys.argv[3] if len(sys.argv) > 3 else None
rows = []
try:
    with open(path) as f:
        for line in f:
            p = line.rstrip("\n").split("\t")
            if len(p) >= 3:
                rows.append((dt.datetime.fromisoformat(p[0]), p[1], p[2], p[3] if len(p) > 3 else ""))
except FileNotFoundError:
    pass
now = dt.datetime.now().astimezone()

def walk(v):
    total, cur, req = dt.timedelta(), None, None
    for t, w, ev, d in rows:
        if w != v:
            continue
        if ev == "use-requested":
            req = t
        elif ev == "use" and cur is None:
            start = req if d == "started" and req is not None else t
            cur = {"start": start, "boot": None, "hb": t}
        elif cur is None:
            continue
        elif ev == "boot" and cur["boot"] is None:
            cur["boot"], cur["hb"] = d, t
        elif ev == "heartbeat" and d == cur["boot"]:
            cur["hb"] = t
        elif ev == "stop":
            total += t - cur["start"]; cur = None
        elif ev == "lost":
            total += cur["hb"] - cur["start"]; cur = None
    if cur is not None:
        total += now - cur["start"]
    return total, cur

if query == "open-vms":
    print(" ".join(sorted({w for _, w, _, _ in rows if walk(w)[1] is not None})))
    sys.exit(0)
total, cur = walk(vm)
if query == "open":
    sys.exit(0 if cur is not None else 1)
elif query == "boot":
    print((cur["boot"] or "") if cur else "")
elif query == "hours":
    print(f"{total.total_seconds() / 3600:.2f}")
elif query == "left":
    print(math.floor(float(sys.argv[4]) - total.total_seconds() / 60))
PY
}

cli() { timeout "$CLI_TIMEOUT" "$NEBIUS" "$@" </dev/null; }

# classify <stderr text> <rc> [code for a timeout]: as gpu-host.sh's classify_error.
classify() {
  case ${2:-1} in 124) echo "${3:-1}"; return;; 126|127) echo 1; return;; esac
  if grep -qE 'code = NotFound|ResourceNotFound' <<<"$1"; then echo 69
  elif grep -qiE 'RESOURCE_EXHAUSTED|ResourceExhausted|NotEnoughResources|not enough|quota|capacity' <<<"$1"; then echo 75
  else echo 1; fi
}

cpu_json() {
  local err out rc=0 text; err=$(mktemp)
  out=$(cli compute instance get --id "$CPU_ID" --profile "$CPU_PROFILE" --format json 2>"$err") || rc=$?
  text=$(cat "$err"); rm -f "$err"
  if [ "$rc" -ne 0 ]; then echo "vm.sh: get failed (rc $rc): $text" >&2; exit "$(classify "$text" "$rc")"; fi
  printf '%s' "$out"
}

state() {  # <vm>
  if [ "$1" = l40s ]; then
    local s; s=$("$GPU_HOST" status) || exit $?; echo "${s%% *}"
  else
    local j; j=$(cpu_json) || exit $?
    python3 -c 'import json,sys; print(json.load(sys.stdin)["status"]["state"])' <<<"$j"
  fi
}

vm_ip() {  # <vm>: the IP while RUNNING, else empty
  if [ "$1" = l40s ]; then "$GPU_HOST" ip 2>/dev/null || true; return; fi
  local j; j=$(cpu_json) || exit $?
  python3 -c '
import json, sys
s = json.load(sys.stdin)["status"]
if s.get("state") == "RUNNING":
    for ni in s.get("network_interfaces") or []:
        a = (ni.get("public_ip_address") or {}).get("address", "")
        if a: print(a.split("/")[0]); break' <<<"$j"
}

remote() {  # <vm> <cmd…>: one ssh, no heartbeat
  local vm=$1; shift
  if [ "$vm" = l40s ]; then "$GPU_HOST" ssh "$@"; return; fi
  local ip; ip=$(vm_ip cpu128)
  [ -n "$ip" ] || { echo "vm.sh: cpu128 is not RUNNING; refusing to ssh" >&2; return 1; }
  mkdir -p "$(dirname "$KNOWN_HOSTS")"
  ssh -o HostKeyAlias=comet-cpu128 -o "UserKnownHostsFile=$KNOWN_HOSTS" \
    -o StrictHostKeyChecking=accept-new -o BatchMode=yes -o ConnectTimeout=15 \
    -o ServerAliveInterval=30 -o ServerAliveCountMax=4 "dmitry@$ip" "$@"
}

boot_id() { local b; b=$(remote "$1" cat /proc/sys/kernel/random/boot_id </dev/null) || return 1; echo "${b//[[:space:]]/}"; }

wait_state() {  # <vm> <wanted> <timeout s>
  local waited=0 s
  while :; do
    s=$(state "$1") || exit $?
    [ "$s" = "$2" ] && return 0
    [ "$waited" -ge "$3" ] && { echo "vm.sh: $1 still $s after ${3}s, wanted $2" >&2; return 1; }
    sleep "$POLL"; waited=$((waited + POLL + 1))
  done
}

wait_ssh() {  # <vm>
  local waited=0
  until remote "$1" true </dev/null 2>/dev/null; do
    [ "$waited" -ge "$START_TIMEOUT" ] && { echo "vm.sh: $1 RUNNING but no ssh after ${START_TIMEOUT}s" >&2; return 1; }
    sleep "$POLL"; waited=$((waited + POLL + 1))
  done
}

start_vm() {  # <vm>; exits with the classified code on failure
  if [ "$1" = l40s ]; then "$GPU_HOST" start || exit $?; return; fi
  local err rc=0 text; err=$(mktemp)
  cli compute instance start --async --id "$CPU_ID" --profile "$CPU_PROFILE" >/dev/null 2>"$err" || rc=$?
  text=$(cat "$err"); rm -f "$err"
  if [ "$rc" -ne 0 ]; then echo "vm.sh: start failed (rc $rc): $text" >&2; exit "$(classify "$text" "$rc" 75)"; fi
  wait_state cpu128 RUNNING "$START_TIMEOUT" || exit 75
  wait_ssh cpu128 || { stop_vm cpu128 || true; logrow cpu128 stop "no ssh after start"; exit 1; }
}

stop_vm() {  # <vm>: force stop, wait for STOPPED
  if [ "$1" = l40s ]; then "$GPU_HOST" stop --force; return; fi
  local err rc=0 text; err=$(mktemp)
  cli compute instance stop --async --id "$CPU_ID" --profile "$CPU_PROFILE" >/dev/null 2>"$err" || rc=$?
  text=$(cat "$err"); rm -f "$err"
  if [ "$rc" -ne 0 ]; then echo "vm.sh: stop failed (rc $rc): $text" >&2; return "$(classify "$text" "$rc")"; fi
  wait_state cpu128 STOPPED "$START_TIMEOUT"
}

arm_deadman() {  # <vm> <minutes>
  local vm=$1 m=$2
  if [ "$DEADMAN" = local ]; then
    systemctl --user stop "comet-deadman-$vm.timer" 2>/dev/null || true
    systemctl --user reset-failed "comet-deadman-$vm.timer" "comet-deadman-$vm.service" 2>/dev/null || true
    systemd-run --user --unit="comet-deadman-$vm" --on-active="${m}m" \
      --setenv=COMET_VM_LOG="$LOG" --setenv=COMET_GPU_HOST="$GPU_HOST" "$SELF" stop "$vm" deadman-local >/dev/null
  else
    remote "$vm" "sudo systemctl stop comet-deadman.timer comet-deadman.service 2>/dev/null; \
sudo systemctl reset-failed comet-deadman.timer comet-deadman.service 2>/dev/null; \
sudo systemd-run --unit=comet-deadman --on-active=${m}m systemctl poweroff" </dev/null >/dev/null
  fi
  logrow "$vm" deadman "$DEADMAN ${m}m"
}

cmd_use() {
  local vm=${1:-}; known_vm "$vm" || exit 1; shift
  local want=""
  [ "${1:-}" = --deadman-min ] && want=$2
  local left; left=$(logq left "$vm" "$(cap_min "$vm")")
  [ "$left" -gt 0 ] || { echo "vm.sh: $vm cap of $(cap_min "$vm") min reached" >&2; exit 1; }
  logrow "$vm" use-requested
  local s how waited=0
  s=$(state "$vm") || exit $?
  while [ "$s" = STARTING ] || [ "$s" = STOPPING ]; do
    [ "$waited" -ge "$START_TIMEOUT" ] && { echo "vm.sh: $vm stuck $s" >&2; exit 75; }
    sleep "$POLL"; waited=$((waited + POLL + 1)); s=$(state "$vm") || exit $?
  done
  case $s in
    RUNNING) how=found
      if [ "$vm" = l40s ]; then "$GPU_HOST" start || exit $?; else wait_ssh "$vm" || exit 1; fi;;
    STOPPED) how=started; start_vm "$vm";;
    *) echo "vm.sh: $vm is $s" >&2; exit 1;;
  esac
  local b; b=$(boot_id "$vm") || { [ $how = started ] && { stop_vm "$vm" || true; logrow "$vm" stop "no boot id"; }; exit 1; }
  if logq open "$vm"; then
    local rec; rec=$(logq boot "$vm")
    [ -n "$rec" ] && [ "$rec" != "$b" ] && logrow "$vm" lost "boot id changed"
  fi
  logrow "$vm" use "$how"
  if [ -z "$(logq boot "$vm")" ]; then logrow "$vm" boot "$b"; else logrow "$vm" heartbeat "$b"; fi
  local m; m=$(logq left "$vm" "$(cap_min "$vm")")
  [ -n "$want" ] && [ "$want" -lt "$m" ] && m=$want
  [ "$m" -lt 1 ] && m=1
  if ! arm_deadman "$vm" "$m"; then
    echo "vm.sh: arming the dead-man stop on $vm failed" >&2
    if [ $how = started ]; then stop_vm "$vm" || true; logrow "$vm" stop "dead-man arm failed"; fi
    exit 1
  fi
  echo "$vm $how; boot $b; dead-man $DEADMAN ${m}m"
}

cmd_ssh() {
  local vm=${1:-}; known_vm "$vm" || exit 1; shift
  local b; b=$(boot_id "$vm") || exit 1
  logrow "$vm" heartbeat "$b" 2>/dev/null
  remote "$vm" "$@"
}

cmd_stop() {
  local vm=${1:-}; known_vm "$vm" || exit 1
  stop_vm "$vm"
  logrow "$vm" stop "${2:-force}"
}

cmd_stop_used() {
  local vms vm s b rec rc=0
  vms=$(logq open-vms)
  [ -n "$vms" ] || { echo "vm.sh: no VM in use"; return 0; }
  for vm in $vms; do
    s=$( (state "$vm") ) || { echo "vm.sh: cannot read $vm's state" >&2; rc=1; continue; }
    case $s in
      STOPPED|STOPPING) logrow "$vm" lost "found $s"; continue;;
    esac
    b=$(boot_id "$vm" 2>/dev/null || true); rec=$(logq boot "$vm")
    [ -n "$b" ] && [ -n "$rec" ] && [ "$b" != "$rec" ] && logrow "$vm" lost "boot id changed"
    if ( stop_vm "$vm" ); then logrow "$vm" stop force
    else echo "vm.sh: force stop of $vm failed" >&2; rc=1; fi
    [ "$DEADMAN" = local ] && { systemctl --user stop "comet-deadman-$vm.timer" 2>/dev/null || true; }
  done
  return "$rc"
}

# push-aws: the profile sections go as JSON on ssh's stdin and are merged into the VM's ~/.aws
# (dir 700, files 600). Only profile names are printed.
cmd_push_aws() {
  local vm=${1:-}; known_vm "$vm" || exit 1
  local profiles=default; [ "$vm" = cpu128 ] && profiles="default eu-west1"
  # shellcheck disable=SC2016
  local merge='import configparser, json, os, sys
d = json.load(sys.stdin); home = os.path.expanduser("~/.aws")
os.makedirs(home, mode=0o700, exist_ok=True); os.chmod(home, 0o700)
for name, secs in d.items():
    p = os.path.join(home, name); c = configparser.RawConfigParser(); c.read(p)
    for s, kv in secs.items():
        if c.has_section(s): c.remove_section(s)
        c.add_section(s)
        for k, v in kv.items(): c.set(s, k, v)
    fd = os.open(p + ".tmp", os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
    with os.fdopen(fd, "w") as f: c.write(f)
    os.replace(p + ".tmp", p); os.chmod(p, 0o600)'
  # shellcheck disable=SC2086
  python3 -c '
import configparser, json, os, sys
home = os.path.expanduser("~/.aws")
files = {"credentials": os.environ.get("AWS_SHARED_CREDENTIALS_FILE", os.path.join(home, "credentials")),
         "config": os.environ.get("AWS_CONFIG_FILE", os.path.join(home, "config"))}
out = {}
for name, path in files.items():
    c = configparser.RawConfigParser(); c.read(path)
    for prof in sys.argv[1:]:
        sec = prof if name == "credentials" or prof == "default" else "profile " + prof
        if c.has_section(sec):
            out.setdefault(name, {})[sec] = dict(c.items(sec))
        elif name == "credentials":
            sys.exit(f"vm.sh: profile {prof} has no section in {path}")
json.dump(out, sys.stdout)' $profiles | remote "$vm" "python3 -c '$merge'" >/dev/null
  echo "pushed AWS profiles to $vm: $profiles"
}

verb=${1:-}; shift || true
case $verb in
  use) cmd_use "$@";;
  ssh) cmd_ssh "$@";;
  status) known_vm "${1:-}" || exit 1
    if [ "$1" = l40s ]; then "$GPU_HOST" status
    else s=$(state cpu128); ip=$(vm_ip cpu128); echo "$s${ip:+ $ip}"; fi;;
  ip) known_vm "${1:-}" || exit 1; ip=$(vm_ip "$1"); [ -n "$ip" ] || exit 1; echo "$ip";;
  push-aws) cmd_push_aws "$@";;
  stop) cmd_stop "$@";;
  stop-used) cmd_stop_used;;
  hours) known_vm "${1:-}" || exit 1; logq hours "$1";;
  *) sed -n '2,25p' "$0" >&2; exit 64;;
esac
