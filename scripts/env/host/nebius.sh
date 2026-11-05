#!/usr/bin/env bash
# A GPU host on nebius, started and stopped through its CLI (llm-wiki/build-test.md, "GPU hosts").
#
#   nebius.sh <name> start|stop|status|ip|powers|ssh <cmd>
#   nebius.sh --needs | --config-name
#
# The host file names the instance, INSTANCE_ID. The CLI on PATH is pointed at hosts/<name>/config.yaml,
# a service account's profile, which is never opened, printed or copied here; without it the host is
# powered by hand and ip is its HOST. start waits out a stop, starts with --async, and waits for the
# GPU key to log in, stopping again what it started when it never does; stop waits for STOPPED.
[ "${BASH_VERSINFO[0]}" -gt 4 ] || { [ "${BASH_VERSINFO[0]}" -eq 4 ] && [ "${BASH_VERSINFO[1]}" -ge 4 ]; } || {
  echo "${0##*/}: this machine's bash is $BASH_VERSION, older than 4.4: install a current one (on macOS, brew install bash)" >&2; exit 2; }
# Above, in bash 3.2's syntax and before any source: macOS ships 3.2, and what follows needs 4.4.
set -euo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/../../lib/peacock-config.sh"
. "$here/../../lib/host.sh"
. "$here/../../lib/portable.sh"
shopt -s inherit_errexit

HOST_NEEDS=(INSTANCE_ID)
HOST_CONFIG_NAME=config.yaml
WAIT=${PEACOCK_HOST_WAIT:-900}
POLL=${PEACOCK_HOST_POLL:-10}

# without_power — why the CLI cannot reach the host, empty when it can.
without_power() {
  if [ ! -f "$HOST_CONFIG" ]; then
    echo "no $HOST_CONFIG: provision-vm.sh --gpu-host $HOST_NAME … --provider-config <file> puts the CLI's profile there"
  elif ! command -v nebius > /dev/null; then
    echo "no nebius CLI on PATH"
  fi
}
powered() { local why; why=$(without_power); [ -z "$why" ] || { peacock_refuse "$why"; return; }; }
host_powers() {
  local why
  why=$(without_power)
  if [ -n "$why" ]; then echo "$HOST_NAME is powered by hand: $why"; return 1; fi
  echo "$HOST_NAME is started and stopped through the nebius CLI"
}

# cli <arg>... — the CLI, bounded: a hung API call must not hold the caller. Its stdin is /dev/null,
# since what ssh carries is not the CLI's.
cli() { timeout 120 nebius --config "$HOST_CONFIG" --no-check-update "$@" < /dev/null; }

# field <state|ip> — from the instance's JSON on stdin.
field() {
  python3 -c '
import json, sys
s = json.load(sys.stdin)["status"]
if sys.argv[1] == "state":
    print(s.get("state", ""))
for ni in s.get("network_interfaces") or [] if sys.argv[1] == "ip" else []:
    a = (ni.get("public_ip_address") or {}).get("address", "")
    if a:
        print(a.split("/")[0])
        break' "$1"
}

instance() { cli compute instance get --id "${HOST_FILE[INSTANCE_ID]}" --format json; }
state() { local j; j=$(instance) || return; field state <<< "$j"; }
# wait_for <state> — polls until the instance is in <state>; 1, saying so, once WAIT seconds have passed.
wait_for() {
  local deadline=$((SECONDS + WAIT)) s
  while :; do
    s=$(state) || return
    if [ "$s" = "$1" ]; then return 0; fi
    [ "$SECONDS" -lt "$deadline" ] || { echo "nebius.sh: $HOST_NAME is $s, not $1, after ${WAIT}s" >&2; return 1; }
    sleep "$POLL"
  done
}
stopped() {
  cli compute instance stop --async --id "${HOST_FILE[INSTANCE_ID]}" > /dev/null
  wait_for STOPPED
}

host_status() { powered; state; }
host_ip() {
  local j s ip
  if [ -n "$(without_power)" ]; then echo "$HOST_ADDRESS"; return; fi
  j=$(instance) || return
  s=$(field state <<< "$j")
  [ "$s" = RUNNING ] || { peacock_refuse "$HOST_NAME is $s, not RUNNING: scripts/env/host/nebius.sh $HOST_NAME start"; return; }
  ip=$(field ip <<< "$j")
  [ -n "$ip" ] || { peacock_refuse "$HOST_NAME is RUNNING with no public address"; return; }
  echo "$ip"
}
# A host this call started is stopped again when it never comes up: never one it found running.
host_start() {
  local s started="" deadline
  powered
  host_ssh_options
  s=$(state)
  if [ "$s" != RUNNING ] && [ "$s" != STARTING ]; then
    # The API refuses a start while the instance is stopping.
    if [ "$s" = STOPPING ]; then wait_for STOPPED; fi
    cli compute instance start --async --id "${HOST_FILE[INSTANCE_ID]}" > /dev/null
    started=1
  fi
  if ! wait_for RUNNING; then
    if [ -n "$started" ]; then stopped || true; fi
    return 1
  fi
  deadline=$((SECONDS + WAIT))
  until host_reachable; do
    if [ "$SECONDS" -ge "$deadline" ]; then
      echo "nebius.sh: $HOST_NAME is RUNNING, but the GPU key does not log in over ssh after ${WAIT}s" >&2
      if [ -n "$started" ]; then stopped; fi
      return 1
    fi
    sleep "$POLL"
  done
  echo "$HOST_NAME is up"
}
host_stop() {
  local s
  powered
  s=$(state)
  case "$s" in
    STOPPED) ;;
    STOPPING) wait_for STOPPED ;;
    # A host still STARTING after the wait is stopped anyway, as scripts/nebius.sh stops it.
    *) if [ "$s" = STARTING ]; then wait_for RUNNING || true; fi
       stopped ;;
  esac
  echo "$HOST_NAME is stopped"
}

host_main "${BASH_SOURCE[0]}" "$@"
