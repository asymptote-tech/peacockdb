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
