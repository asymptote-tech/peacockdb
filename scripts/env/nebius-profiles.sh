#!/usr/bin/env bash
# Cut ~/.config/peacock/nebius.yaml from this machine's ~/.nebius/config.yaml
# (llm-wiki/build-test.md, "Credential files"): the profiles editor-eu and eu-west1 alone, each key
# embedded, editor-eu active.
#
#   nebius-profiles.sh
#
# A profile whose key sits in a file gets that file embedded. A rerun replaces nebius.yaml. Refused,
# exit 2: a profile missing, or a key path naming no file. It prints the two names and how many
# profiles it left out. Only the nebius CLI reads either config, the source on a temp copy.
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
# removes whole. bash runs EXIT on HUP, INT and TERM too (measured), and a CLI a signal to bash
# alone leaves running then finds no directory to write it back into, unless the CLI makes one.
work=
trap 'rm -rf "$work"' EXIT
work=$(mktemp -d "$PEACOCK_CONFIG/nebius-cut.XXXXXX")
peacock_nebius_cut "$PEACOCK_NEBIUS" "$src" "$work/nebius.yaml" "$PEACOCK_NEBIUS_CONFIG" \
  "${PEACOCK_NEBIUS_PROFILES[0]}" "${PEACOCK_NEBIUS_PROFILES[@]}"
echo "$PEACOCK_NEBIUS_CONFIG: ${PEACOCK_NEBIUS_PROFILES[0]} (active), ${PEACOCK_NEBIUS_PROFILES[1]}; $PEACOCK_NEBIUS_LEFT_OUT other profiles left out"
