#!/usr/bin/env bash
# Stop chain <chain>'s instance up.sh started and remove its container (llm-wiki/build-test.md,
# "Containers").
#
#   down.sh <chain> [--vm <name>]
#
# Only the caller's own container, found by its labels peacockdb.user and peacockdb.chain. Its
# worktree and home stay, so up.sh <chain> starts it again where it stopped. The machine's data,
# registry and logins are never touched; a worktree goes by hand, with git worktree remove. On a
# machine that runs no instances, it runs on the VM provision-vm.sh recorded last, or on --vm's.
[ "${BASH_VERSINFO[0]}" -gt 4 ] || { [ "${BASH_VERSINFO[0]}" -eq 4 ] && [ "${BASH_VERSINFO[1]}" -ge 4 ]; } || {
  echo "${0##*/}: this machine's bash is $BASH_VERSION, older than 4.4: install a current one (on macOS, brew install bash)" >&2; exit 2; }
# Above, in bash 3.2's syntax and before any source: macOS ships 3.2, and what follows needs 4.4.
set -euo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/../lib/peacock-config.sh"

die() { echo "down.sh: $*" >&2; exit 2; }

chain="" vm=""
while [ $# -gt 0 ]; do
  case "$1" in
    -h|--help) peacock_usage "${BASH_SOURCE[0]}" ;;
    --vm) [ -n "${2:-}" ] && [ -z "$vm" ] || die "--vm takes one name"; vm=$2; shift 2 ;;
    -*) die "unknown flag $1" ;;
    *) [ -z "$chain" ] || die "want one chain, got: $chain $1"; chain=$1; shift ;;
  esac
done
[ -n "$chain" ] || die "name the chain: down.sh <chain>, see --help"
[[ "$chain" =~ ^[A-Za-z0-9][A-Za-z0-9_-]*$ ]] || die "not a chain name: $chain"
peacock_forward down.sh "$vm" "$chain"

# By label, not by name: with - in user and chain names, peacock-<user>-<chain> can be two pairs.
user=$(id -un)
container=$(docker ps -a --filter "label=peacockdb.user=$user" --filter "label=peacockdb.chain=$chain" \
              --format '{{.Names}}')
[ -n "$container" ] || die "no instance of chain $chain of yours: no container labelled for $user and $chain"
docker stop "$container" > /dev/null
docker rm "$container" > /dev/null
echo "chain $chain is down; its worktree stays"
