#!/usr/bin/env bash
# A GPU host reached over ssh alone, with no provider API to power it (llm-wiki/build-test.md,
# "GPU hosts").
#
#   ssh.sh <name> start|stop|status|ip|powers|ssh <cmd>
#   ssh.sh --needs | --config-name
#
# The host file is ~/.config/peacock/hosts/<name>.env. The host is powered by hand: start and stop
# do nothing, powers says so and exits 1. status says whether GPU_SSH_KEY_FILE logs in as its USER,
# exit 1 when it does not. ip is its HOST. ssh runs <cmd> there. --needs: no key beyond PROVIDER,
# HOST and USER.
[ "${BASH_VERSINFO[0]}" -gt 4 ] || { [ "${BASH_VERSINFO[0]}" -eq 4 ] && [ "${BASH_VERSINFO[1]}" -ge 4 ]; } || {
  echo "${0##*/}: this machine's bash is $BASH_VERSION, older than 4.4: install a current one (on macOS, brew install bash)" >&2; exit 2; }
# Above, in bash 3.2's syntax and before any source: macOS ships 3.2, and what follows needs 4.4.
set -euo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/../../lib/peacock-config.sh"
. "$here/../../lib/host.sh"
shopt -s inherit_errexit

HOST_NEEDS=()
HOST_CONFIG_NAME=""
host_powers() { echo "$HOST_NAME is powered by hand: an ssh host has no API to start or stop it"; return 1; }
host_start() { :; }
host_stop() { :; }
host_ip() { echo "$HOST_ADDRESS"; }
host_status() {
  host_ssh_options
  if host_reachable; then echo "$HOST_NAME: reachable"; else echo "$HOST_NAME: unreachable"; return 1; fi
}

host_main "${BASH_SOURCE[0]}" "$@"
