#!/usr/bin/env bash
# Cut ~/.config/peacock/nebius.yaml from this machine's ~/.nebius/config.yaml
# (llm-wiki/build-test.md, "Credential files"): the profiles editor-eu and eu-west1 alone, each key
# embedded, editor-eu active.
#
#   nebius-profiles.sh
#
# nebius.sh and provision-vm.sh run it when nebius.yaml is missing, so by hand it is a refresh: a
# profile or key that changed, or the file removed. A profile whose key sits in a file gets that
# file embedded. A rerun replaces nebius.yaml. Refused, exit 2: a profile missing, one that is not
# a service account's (a browser login acts as you), or a key path naming no file. It prints the two
# names and how many profiles it left out. Only the nebius CLI reads either config, the source on a
# temp copy.
[ "${BASH_VERSINFO[0]}" -gt 4 ] || { [ "${BASH_VERSINFO[0]}" -eq 4 ] && [ "${BASH_VERSINFO[1]}" -ge 4 ]; } || {
  echo "${0##*/}: this machine's bash is $BASH_VERSION, older than 4.4: install a current one (on macOS, brew install bash)" >&2; exit 2; }
# Above, in bash 3.2's syntax and before any source: macOS ships 3.2, and what follows needs 4.4.
set -euo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source-path=SCRIPTDIR source=../lib/peacock-config.sh
. "$here/../lib/peacock-config.sh"

case $#:${1:-} in
  0:) ;;
  1:-h|1:--help) peacock_usage "${BASH_SOURCE[0]}" ;;
  *) peacock_refuse "takes no argument but --help: $*" ;;
esac

peacock_nebius_cli
src=$HOME/.nebius/config.yaml
[ -f "$src" ] || peacock_refuse "no $src: the nebius CLI's own profiles are cut from it"
install -d -m 700 "$PEACOCK_CONFIG"
# The copy holds every profile until the cut ends, so it lives in a directory of its own that EXIT
# removes whole. For a HUP, INT or TERM, a trap of its own removes the directory, then dies of that
# signal: without one, a HUP that lands while bash waits for a command substitution skipped EXIT
# (measured). Dying of the signal, not exiting 130, is what stops a bash caller. With the traps, a
# signal to bash alone waits for the CLI call that is running, so no CLI outlives the directory.
work=
trap 'rm -rf "$work"' EXIT
trap 'rm -rf "$work"; trap - HUP EXIT; kill -HUP $$' HUP
trap 'rm -rf "$work"; trap - INT EXIT; kill -INT $$' INT
trap 'rm -rf "$work"; trap - TERM EXIT; kill -TERM $$' TERM
work=$(mktemp -d "$PEACOCK_CONFIG/nebius-cut.XXXXXX")
# Cut inside the directory and checked there, so a refused cut never reaches nebius.yaml.
peacock_nebius_cut "$PEACOCK_NEBIUS" "$src" "$work/nebius.yaml" "$work/cut.yaml" \
  "${PEACOCK_NEBIUS_PROFILES[0]}" "${PEACOCK_NEBIUS_PROFILES[@]}"
peacock_nebius_service_accounts_checked "$PEACOCK_NEBIUS" "$work/cut.yaml" "$src"
mv -f -- "$work/cut.yaml" "$PEACOCK_NEBIUS_CONFIG" || peacock_refuse "cannot move the cut into $PEACOCK_NEBIUS_CONFIG"
echo "$PEACOCK_NEBIUS_CONFIG: ${PEACOCK_NEBIUS_PROFILES[0]} (active), ${PEACOCK_NEBIUS_PROFILES[1]}; $PEACOCK_NEBIUS_LEFT_OUT other profiles left out"
