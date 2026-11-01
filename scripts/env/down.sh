#!/usr/bin/env bash
# Stop an instance up.sh started and remove its container (llm-wiki/build-test.md, "Containers").
#
#   down.sh <instance>
#
# Its worktree and home stay, so up.sh <instance> starts it again where it stopped. The machine's
# data, registry and logins are never touched; a worktree goes by hand, with git worktree remove.
set -euo pipefail

usage() { sed -n '2,${/^#/!q;p;}' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'; exit 0; }
die() { echo "down.sh: $*" >&2; exit 2; }

case "${1:-}" in -h|--help) usage ;; esac
[ $# -eq 1 ] || die "want one instance, got: ${*:-nothing}"
[[ "$1" =~ ^[a-z0-9][a-z0-9-]*$ ]] || die "an instance is named in lower-case letters, digits and -: $1"
container=peacock-$1
docker container inspect "$container" > /dev/null 2>&1 || die "no instance $1: no container $container"
docker stop "$container" > /dev/null
docker rm "$container" > /dev/null
echo "instance $1 is down; its worktree stays"
