#!/usr/bin/env bash
# The one door to the Nebius VMs: nebius-gpu, the L40S the device tier runs on, by default, and
# any other instance by --instance-id (llm-wiki/build-test.md, "Nebius host"). Every caller — a
# developer, the coordinator, CI, the hourly checker — goes through this script, so each
# instance's IP and host key are handled in one place.
#
#   nebius.sh [--instance-id <id>] [--profile <name>] <verb> …
#     --instance-id  the VM (default nebius-gpu's); its host-key alias is nebius-<id>
#     --profile      the nebius CLI profile for every call (default: the active one)
#
#   nebius.sh status            state, and the IP when RUNNING
#   nebius.sh ip                the IP, or exit 1 when not RUNNING
#   nebius.sh start             start, wait for RUNNING and for ssh (idempotent); RUNNING
#                                 with no ssh is exit 1, and a VM this call started is stopped
#   nebius.sh stop [--force]    soft: refuse while the card has a process or its lock is held,
#                                 or, on a VM without one, while duckdb runs; --force: stop anyway
#   nebius.sh ssh <cmd…>        run a command on the VM; an ssh that fails before
#                                 authenticating is tried again, four attempts in all
#   nebius.sh rsh <host> <cmd…> one attempt, for rsync -e: the sync around it is what is retried
#                                 (rsync's host argument is dropped)
#   nebius.sh install-watch [--key-file <pem>]
#                               install the idle watcher, root's one profile and the install file;
#                                 a profile's key path gets the file it names embedded, or the PEM
#                                 --key-file names; beside an embedded key --key-file is refused
#
# Every CLI call names its config, GPU_HOST_NEBIUS_CONFIG, else ~/.config/peacock/nebius.yaml; a
# missing nebius.yaml is cut first by scripts/env/nebius-profiles.sh where ~/.nebius/config.yaml
# exists. The CLI is NEBIUS, else nebius on PATH, else ~/.nebius/bin's; no config or no CLI is exit 1.
# The ssh key is GPU_HOST_KEY, else ~/.config/peacock/gpu_ed25519 when it exists.
#
# Exit codes: 0 done; 2 busy, soft stop refused; 75 no capacity, retry later; 69 instance gone;
# 1 other; 64 usage.
#
# NEVER print, cat or grep a Nebius credential file (~/.nebius/config.yaml, nebius.yaml, …).
# One redaction attempt already leaked a private key into a transcript.
set -euo pipefail
# shellcheck source-path=SCRIPTDIR source=lib/ssh-retry.sh
. "$(dirname "${BASH_SOURCE[0]}")/lib/ssh-retry.sh"
# shellcheck source-path=SCRIPTDIR source=lib/peacock-config.sh
. "$(dirname "${BASH_SOURCE[0]}")/lib/peacock-config.sh"

DEFAULT_ID=computeinstance-e00tnrse7ayntnzcyt
INSTANCE_ID=$DEFAULT_ID
PROFILE=

bad_option() { echo "nebius: $1" >&2; exit 64; }

id_set=0 profile_set=0 GIVEN=()
while :; do
  case ${1:-} in
    --instance-id)
      [ "$id_set" -eq 0 ] || bad_option "--instance-id given twice"
      [[ ${2:-} =~ ^computeinstance-[a-z0-9]+$ ]] || bad_option "--instance-id '${2:-}' is not a computeinstance id"
      INSTANCE_ID=$2; id_set=1; GIVEN+=("$1" "$2"); shift 2;;
    --profile)
      [ "$profile_set" -eq 0 ] || bad_option "--profile given twice"
      [[ ${2:-} =~ ^[A-Za-z0-9_.-]+$ ]] || bad_option "--profile '${2:-}' is not a profile name"
      PROFILE=$2; profile_set=1; GIVEN+=("$1" "$2"); shift 2;;
    *) break;;
  esac
done
# One alias per instance in the one known-hosts file, so one VM's new key never touches another's.
if [ "$INSTANCE_ID" = "$DEFAULT_ID" ]; then ALIAS=nebius-gpu; else ALIAS=nebius-$INSTANCE_ID; fi

SSH=${GPU_HOST_SSH:-ssh}
KEYSCAN=${GPU_HOST_KEYSCAN:-ssh-keyscan}
RSYNC=${GPU_HOST_RSYNC:-rsync}
USER_ON_VM=${GPU_HOST_USER:-dmitry}
STATE_DIR=${GPU_HOST_STATE_DIR:-$HOME/.config/peacock/nebius}
NEBIUS_CONFIG=${GPU_HOST_NEBIUS_CONFIG:-$PEACOCK_NEBIUS_CONFIG}
POLL=${GPU_HOST_POLL:-10}
START_TIMEOUT=${GPU_HOST_START_TIMEOUT:-900}
STOP_TIMEOUT=${GPU_HOST_STOP_TIMEOUT:-600}
CLI_TIMEOUT=${GPU_HOST_CLI_TIMEOUT:-120}
KNOWN_HOSTS=$STATE_DIR/known_hosts
LOG=$STATE_DIR/nebius.log

verb=${1:-}; shift || true
case $verb in
  status|ip|start|stop|ssh|rsh|install-watch) ;;
  *) sed -n '2,32p' "$0" >&2; exit 64;;
esac
# Exit 1, not peacock-config.sh's 2: here 2 is "busy".
if [ -z "${GPU_HOST_NEBIUS_CONFIG:-}" ]; then peacock_nebius_config_cut || exit 1; fi
[ -f "$NEBIUS_CONFIG" ] \
  || { echo "nebius: no $NEBIUS_CONFIG: make it with scripts/env/nebius-profiles.sh" >&2; exit 1; }
if [ -z "${NEBIUS:-}" ]; then peacock_nebius_cli || exit 1; NEBIUS=$PEACOCK_NEBIUS; fi
mkdir -p "$STATE_DIR"; touch "$KNOWN_HOSTS"

log() { printf '%s %s\n' "$(date -Is)" "$*" | tee -a "$LOG" >&2; }

# cli <args…>: the Nebius CLI on NEBIUS_CONFIG, bounded, with --profile when one was given. A hung
# API call must not hold the caller (and a paid VM) past GPU_HOST_CLI_TIMEOUT; timeout(1) exits 124
# then. Its stdin is /dev/null: stdin belongs to the remote command (`nebius.sh ssh … < patch`,
# rsync's -e transport).
cli() { timeout "$CLI_TIMEOUT" "$NEBIUS" "$@" --config "$NEBIUS_CONFIG" ${PROFILE:+--profile "$PROFILE"} </dev/null; }

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
  if [ -n "${GPU_HOST_KEY:-}" ]; then printf '%s\n' -i "$GPU_HOST_KEY"
  elif [ -f "$PEACOCK_GPU_KEY" ]; then printf '%s\n' -i "$PEACOCK_GPU_KEY"
  fi
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

# vm_ssh_with <attempts> <cmd…>: one ssh to the VM at its current IP, through the helper, repairing
# the stored key once if the API vouches for the IP. An rsync's transport is one attempt: the rsync
# around it is what is retried, so retries never nest. Only a refusal before authenticating is
# ssh's own: after it, that text on stderr is the remote command's, and the command may have run.
vm_ssh_with() {
  local attempts=$1 ip err rc opts authenticated
  shift
  ip=$(current_ip)
  [ -n "$ip" ] || { echo "nebius: $INSTANCE_ID is not RUNNING; refusing to ssh" >&2; return 1; }
  mapfile -t opts < <(ssh_opts)
  grep -q "^$ALIAS " "$KNOWN_HOSTS" || trust_current_key "$ip" || return 1
  err=$(mktemp); rc=0
  ssh_retry_up_to "$attempts" authenticated "$SSH" "${opts[@]}" "$USER_ON_VM@$ip" "$@" 2>"$err" || rc=$?
  if [ "$rc" -eq 255 ] && [ "$authenticated" = 0 ] \
    && grep -qE 'HOST IDENTIFICATION HAS CHANGED|Host key verification failed' "$err"; then
    rm -f "$err"
    [ "$(current_ip)" = "$ip" ] || { echo "nebius: IP moved during the call; retry" >&2; return 1; }
    trust_current_key "$ip" || return 1
    ssh_retry_up_to "$attempts" authenticated "$SSH" "${opts[@]}" "$USER_ON_VM@$ip" "$@"; return $?
  fi
  cat "$err" >&2; rm -f "$err"; return "$rc"
}
vm_ssh() { vm_ssh_with "$SSH_RETRY_ATTEMPTS" "$@"; }

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
  # Wall clock, not a count of polls: each vm_ssh may spend minutes in its own retries.
  local deadline=$((SECONDS + START_TIMEOUT))
  until vm_ssh true 2>/dev/null; do
    if [ "$SECONDS" -ge "$deadline" ]; then
      # Up but unreachable is a broken host, not a capacity problem: exit 1, never 75, or the
      # coordinator retries for hours against a paid VM. A VM this call started is stopped
      # again; one that was already running is someone else's.
      log "RUNNING but no ssh after ${START_TIMEOUT}s: host up but unreachable"
      if [ "$started" -eq 1 ]; then
        ( cmd_stop --force ) || log "the stop after a failed start failed too; the guards will stop it"
      fi
      exit 1
    fi
    sleep "$POLL"
  done
  log "up: $(cmd_status)"
}

cmd_stop() {
  local force=0 s busy lock
  [ "${1:-}" = --force ] && force=1
  s=$(state)
  case $s in STOPPED|STOPPING) log "stop: already $s"; return 0;; esac
  [ "$s" = STARTING ] && { wait_for_state RUNNING "$START_TIMEOUT" || true; }
  if [ "$force" -eq 0 ]; then
    # The card's apps on a GPU VM; a VM without nvidia-smi (the CPU VM) has no card, and there a
    # DuckDB run is the work a stop would cut short. A held card lock is a run or a capture, maybe
    # with nothing on the card yet; it is opened read-only, as another user's file in sticky
    # /run/lock refuses an open that may create it. A failing probe reads as unreachable.
    lock=$(printf '%q' "$PEACOCK_CARD_LOCK")
    busy=$(vm_ssh 'if command -v nvidia-smi >/dev/null; then nvidia-smi --query-compute-apps=pid,process_name --format=csv,noheader; else pgrep -a -x duckdb || true; fi;'" if [ -e $lock ] && ! flock -n 9 9< $lock; then echo 'card lock $lock held'; fi" 2>/dev/null) \
      || busy="?unreachable"
    if [ "$busy" = "?unreachable" ]; then log "stop: host unreachable over ssh; stopping on the API's state"
    elif [ -n "$busy" ]; then log "stop: busy: $(tr '\n' ';' <<<"$busy") — not stopping"; exit 2
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

install_fail() { log "install-watch FAILED: $1"; exit 1; }

# bounded_nebius <args…>: the CLI, bounded as cli() is, without its --profile: install-watch asks
# it the active profile of NEBIUS_CONFIG, and the cut makes its calls with it on a temp copy. Those
# stay local (--skip-auth on the update), and their output never reaches a log.
bounded_nebius() { timeout "$CLI_TIMEOUT" "$NEBIUS" "$@"; }

# active_profile: NEBIUS_CONFIG's active profile, or a failure when it has none or the answer is
# not a profile name. The name is logged and read as <name>[=<pem>], and an answer of another shape
# may be anything, so it is not traced under set -x either.
active_profile() {
  local -
  set +x
  local a
  a=$(peacock_nebius_on bounded_nebius "$NEBIUS_CONFIG" profile active 2>/dev/null) || return 1
  [[ "$a" =~ ^[A-Za-z0-9_.-]+$ ]] || return 1
  printf '%s\n' "$a"
}

# The VM gets one profile, self-contained, as root's only one. It is cut on dev from a temp copy,
# so no other profile or key reaches the VM even for a moment. The config and the PEM are copied,
# never read here: only the CLI reads them (build-test.md, "Credential files").
cmd_install_watch() {
  local key='' p why
  case "$#:${1:-}" in
    0:) ;;
    2:--key-file) key=$2;;
    *) bad_option "install-watch takes only --key-file <pem>";;
  esac
  [ -z "$key" ] || [ -f "$key" ] || install_fail "--key-file $key: no such file"
  # bash runs EXIT on HUP, INT and TERM too, so the copy, which holds every profile, never stays.
  install_dir=$(mktemp -d); trap 'rm -rf "$install_dir"' EXIT
  p=$PROFILE
  [ -n "$p" ] || p=$(active_profile) || install_fail "the active profile of $NEBIUS_CONFIG is missing or not a profile name; it is not shown"
  # The cut's stderr is its refusal alone: every CLI call in it drops its own.
  if ! peacock_nebius_cut bounded_nebius "$NEBIUS_CONFIG" "$(mktemp "$install_dir/config.yaml.XXXXXX")" \
      "$install_dir/config.yaml" "$p" "$p${key:+=$key}" 2>"$install_dir/refusal"; then
    why=$(<"$install_dir/refusal")
    install_fail "$NEBIUS cannot cut profile $p alone out of $NEBIUS_CONFIG${key:+ with --key-file $key}: ${why#"${0##*/}: "}"
  fi
  log "install-watch: profile $p goes to the VM alone; $PEACOCK_NEBIUS_LEFT_OUT other(s) left out"

  local here; here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
  vm_ssh 'mkdir -p ~/peacock-gpu-idle'
  # This script's rsh, with the options it was given, drops rsync's host argument, `vm`.
  local rsh=("$here/nebius.sh" "${GIVEN[@]}" rsh)
  sync_retry "$RSYNC" -a -e "${rsh[*]}" "$here/nebius/idle_watch.py" "$here/nebius/peacock-gpu-idle.service" \
    "$here/nebius/peacock-gpu-idle.timer" vm:peacock-gpu-idle/
  sync_retry "$RSYNC" -a -e "${rsh[*]}" "$install_dir/config.yaml" vm:peacock-gpu-idle/nebius-config.yaml
  # The install file goes first: a timer that ticks without it stops nebius-gpu.
  # shellcheck disable=SC2016  # the $(...) is meant to expand on the VM, not here
  vm_ssh "id=$INSTANCE_ID"'
    set -e
    trap "rm -f $HOME/peacock-gpu-idle/nebius-config.yaml" EXIT
    kind=cpu; if command -v nvidia-smi >/dev/null; then kind=gpu; fi
    printf "{\"instance_id\": \"%s\", \"kind\": \"%s\"}\n" "$id" "$kind" | sudo tee /etc/peacock-gpu-idle.json >/dev/null
    echo "kind=$kind"
    command -v nebius >/dev/null || [ -x ~/.nebius/bin/nebius ] || curl -sSL https://storage.eu-north1.nebius.cloud/cli/install.sh | bash
    sudo install -d -m 700 /root/.nebius /var/lib/peacock-gpu-idle
    sudo install -m 600 ~/peacock-gpu-idle/nebius-config.yaml /root/.nebius/config.yaml
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
    echo "nebius: install-watch FAILED: root's nebius profile on the VM does not work" >&2
    exit 1
  fi
  log "install-watch: watcher installed, root's profile reaches the API"
}

case $verb in
  status) cmd_status;;
  ip) ip=$(current_ip); [ -n "$ip" ] || exit 1; echo "$ip";;
  start) cmd_start;;
  stop) cmd_stop "$@";;
  ssh) vm_ssh "$@";;
  rsh) shift; vm_ssh_with 1 "$@";;
  install-watch) cmd_install_watch "$@";;
esac
