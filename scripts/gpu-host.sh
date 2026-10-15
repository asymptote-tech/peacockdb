#!/usr/bin/env bash
# The one door to nebius-gpu, the L40S VM the device tier runs on (llm-wiki/build-test.md,
# "GPU host"). Every caller — a developer, the coordinator, CI, the hourly checker — goes
# through this script, so the instance's IP and host key are handled in one place.
#
#   gpu-host.sh status            state, and the IP when RUNNING
#   gpu-host.sh ip                the IP, or exit 1 when not RUNNING
#   gpu-host.sh start             start, wait for RUNNING and for ssh (idempotent); RUNNING
#                                 with no ssh is exit 1, and a VM this call started is stopped
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
RSYNC=${GPU_HOST_RSYNC:-rsync}
USER_ON_VM=${GPU_HOST_USER:-dmitry}
STATE_DIR=${GPU_HOST_STATE_DIR:-$HOME/.config/peacock/gpu-host}
POLL=${GPU_HOST_POLL:-10}
START_TIMEOUT=${GPU_HOST_START_TIMEOUT:-900}
STOP_TIMEOUT=${GPU_HOST_STOP_TIMEOUT:-600}
CLI_TIMEOUT=${GPU_HOST_CLI_TIMEOUT:-120}
KNOWN_HOSTS=$STATE_DIR/known_hosts
LOG=$STATE_DIR/gpu-host.log
[ -x "$HOME/.nebius/bin/nebius" ] && PATH=$HOME/.nebius/bin:$PATH
mkdir -p "$STATE_DIR"; touch "$KNOWN_HOSTS"

log() { printf '%s %s\n' "$(date -Is)" "$*" | tee -a "$LOG" >&2; }

# cli <args…>: the Nebius CLI, bounded. A hung API call must not hold the caller (and a paid
# VM) past GPU_HOST_CLI_TIMEOUT; timeout(1) exits 124 then. Its stdin is /dev/null: stdin
# belongs to the remote command (`gpu-host.sh ssh … < patch`, rsync's -e transport).
cli() { timeout "$CLI_TIMEOUT" "$NEBIUS" "$@" </dev/null; }

# classify_error <stderr text> <cli rc> [code for a timeout, default 1] -> exit code.
# Only the CLI's own error codes decide 69 and 75; a bare "not found" (a missing binary, a
# missing profile) is an ordinary failure, 1. rc 124 is the timeout, 126/127 a CLI that cannot run.
# NotFound wording checked against the real CLI on 2026-10-14 (an unknown id in the
# instance's own region, rc=13):
#   Error: rpc error: code = NotFound desc = instance not found by id "computeinstance-e00doesnotexist00000"
# An id whose region prefix is wrong gives InvalidArgument instead (rc=3), which is exit 1 here.
classify_error() {
  case ${2:-1} in
    124) echo "${3:-1}"; return;;
    126|127) echo 1; return;;
  esac
  if grep -qE 'code = NotFound|ResourceNotFound' <<<"$1"; then echo 69
  elif grep -qiE 'RESOURCE_EXHAUSTED|ResourceExhausted|NotEnoughResources|not enough|quota|capacity' <<<"$1"; then echo 75
  else echo 1; fi
}

# get_json: the instance as JSON on stdout, or exit with the classified code.
get_json() {
  local out err rc
  err=$(mktemp); rc=0
  out=$(cli compute instance get --id "$INSTANCE_ID" --format json 2>"$err") || rc=$?
  if [ "$rc" -ne 0 ]; then
    local text; text=$(cat "$err"); rm -f "$err"
    log "get failed (rc $rc): $text"; exit "$(classify_error "$text" "$rc")"
  fi
  rm -f "$err"; printf '%s' "$out"
}

# Both read get_json into a variable first. Piped, pipefail would report python's rc on
# empty input instead of get_json's 69 or 75. The explicit "|| exit $?" is needed: bash does
# not carry set -e into a command substitution, so get_json's exit would otherwise be lost.
state() {
  local j; j=$(get_json) || exit $?
  python3 -c 'import json,sys; print(json.load(sys.stdin)["status"]["state"])' <<<"$j"
}

# current_ip: the public IP while RUNNING, else empty. Resolved on every call — the VM is
# preemptible and its IP changes on every start, so nothing may cache it.
current_ip() {
  local j; j=$(get_json) || exit $?
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
  new=$("$KEYSCAN" -t ed25519 "$ip" </dev/null 2>/dev/null | awk 'NF>=3 {print $2" "$3; exit}')
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
    s=$(state) || exit $?  # the caller's "|| exit 75" would swallow set -e here
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
  local s err rc started=0
  s=$(state)
  if [ "$s" != RUNNING ] && [ "$s" != STARTING ]; then
    if [ "$s" = STOPPING ]; then
      # The API rejects start on a stopping instance, and the coordinator retries only 75.
      log "$INSTANCE_ID is STOPPING; waiting for STOPPED before start"
      wait_for_state STOPPED "$STOP_TIMEOUT" || exit 75
    fi
    log "starting $INSTANCE_ID (was $s)"
    err=$(mktemp); rc=0
    # --async: return at once and let wait_for_state do the waiting, bounded by START_TIMEOUT.
    cli compute instance start --async --id "$INSTANCE_ID" >/dev/null 2>"$err" || rc=$?
    if [ "$rc" -ne 0 ]; then
      local text; text=$(cat "$err"); rm -f "$err"; log "start failed (rc $rc): $text"
      exit "$(classify_error "$text" "$rc" 75)"
    fi
    rm -f "$err"; started=1
  fi
  wait_for_state RUNNING "$START_TIMEOUT" || exit 75
  local waited=0
  until vm_ssh true 2>/dev/null; do
    if [ "$waited" -ge "$START_TIMEOUT" ]; then
      # Up but unreachable is a broken host, not a capacity problem: exit 1, never 75, or the
      # coordinator retries for hours against a paid VM. A VM this call started is stopped
      # again; one that was already running is someone else's.
      log "RUNNING but no ssh after ${START_TIMEOUT}s: host up but unreachable"
      if [ "$started" -eq 1 ]; then
        ( cmd_stop --force ) || log "the stop after a failed start failed too; the guards will stop it"
      fi
      exit 1
    fi
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
  local err rc=0 text; err=$(mktemp)
  cli compute instance stop --async --id "$INSTANCE_ID" >/dev/null 2>"$err" || rc=$?
  if [ "$rc" -ne 0 ]; then
    text=$(cat "$err"); rm -f "$err"; log "stop failed (rc $rc): $text"
    exit "$(classify_error "$text" "$rc")"
  fi
  rm -f "$err"
  wait_for_state STOPPED "$STOP_TIMEOUT" || exit 1
  log "stopped"
}

cmd_install_watch() {
  local here; here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
  # The watcher stops the VM through the API, so it needs a profile. The file is copied, never
  # read or printed here.
  vm_ssh 'mkdir -p ~/peacock-gpu-idle'
  local ip opts; ip=$(current_ip); mapfile -t opts < <(ssh_opts)
  "$RSYNC" -a -e "$SSH ${opts[*]}" "$here/gpu-host/idle_watch.py" "$here/gpu-host/peacock-gpu-idle.service" \
    "$here/gpu-host/peacock-gpu-idle.timer" "$USER_ON_VM@$ip:peacock-gpu-idle/"
  "$RSYNC" -a -e "$SSH ${opts[*]}" "$HOME/.nebius/config.yaml" "$USER_ON_VM@$ip:peacock-gpu-idle/nebius-config.yaml"
  # shellcheck disable=SC2016  # the $(...) is meant to expand on the VM, not here
  vm_ssh 'set -e
    command -v nebius >/dev/null || [ -x ~/.nebius/bin/nebius ] || curl -sSL https://storage.eu-north1.nebius.cloud/cli/install.sh | bash
    sudo install -d -m 700 /root/.nebius /var/lib/peacock-gpu-idle
    sudo install -m 600 ~/peacock-gpu-idle/nebius-config.yaml /root/.nebius/config.yaml
    rm -f ~/peacock-gpu-idle/nebius-config.yaml
    src=$(command -v nebius || echo ~/.nebius/bin/nebius)
    # A re-install finds its own earlier copy first on PATH; install(1) refuses src == dst.
    [ "$(readlink -f "$src")" = /usr/local/bin/nebius ] || sudo install -m 755 "$src" /usr/local/bin/nebius
    sudo install -m 755 ~/peacock-gpu-idle/idle_watch.py /usr/local/lib/peacock-gpu-idle.py
    sudo install -m 644 ~/peacock-gpu-idle/peacock-gpu-idle.service ~/peacock-gpu-idle/peacock-gpu-idle.timer /etc/systemd/system/
    sudo systemctl daemon-reload
    sudo systemctl enable --now peacock-gpu-idle.timer
    systemctl is-enabled peacock-gpu-idle.timer'
  # The watcher's stop is only as good as root's profile. Check it the way the watcher calls
  # it. Only the exit status is used; the config is never printed.
  if ! vm_ssh "sudo env HOME=/root timeout 60 /usr/local/bin/nebius compute instance get --id $INSTANCE_ID >/dev/null"; then
    log "install-watch: root's nebius profile on the VM cannot reach the API; the watcher cannot stop the VM"
    echo "gpu-host: install-watch FAILED: root's nebius profile on the VM does not work" >&2
    exit 1
  fi
  log "install-watch: watcher installed, root's profile reaches the API"
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
