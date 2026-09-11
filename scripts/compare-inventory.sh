#!/usr/bin/env bash
# Compare a fresh case inventory against a baseline one.
#   scripts/compare-inventory.sh rust-only \
#     llm-wiki/tasks/module-layout-baselines/inv-rust-only-final.txt /tmp/inv.txt
# The three shapes are the build ladder, and `gpu` reads like the other two: the shape names
# the pair of files being compared, and nothing in the normalisation depends on it.
# Integration targets compare verbatim. Lib cases are named by module path, which a layout
# move changes by construction, so they compare on the suffix from the last segment ending
# in `tests` — `tests`, `ffi_tests`, `gpu_tests`, `schema_tests` — which is unique per case.
#
# The baseline is named rather than computed from this script's own directory: the baselines
# live with the task that took them and are deleted when it is archived, and a default
# pointing into a directory that stops existing is a tool that breaks silently later.
set -uo pipefail
usage="usage: compare-inventory.sh rust-only|cudf|gpu <baseline-file> <fresh-file>"
shape="${1:?$usage}"
base="${2:?$usage}"
fresh="${3:?$usage}"
case "$shape" in rust-only|cudf|gpu) ;; *) echo "$usage" >&2; exit 1 ;; esac
for f in "$base" "$fresh"; do
  [ -f "$f" ] || { echo "compare-inventory.sh: no such inventory: $f" >&2; exit 1; }
done
# Prefix every case with its target, or a case moving between targets would cancel out
# in a globally sorted comparison.
# The module path a lib case is named by is exactly what a layout move changes, so it is
# dropped down to the test module itself. `schema_tests` counts: any segment ending in `tests`
# is the boundary, and the suffix from there on is unique across all 435.
norm() { awk '/^== /{t=$2; next} {print t"\t"$0}' "$1" \
  | sed -E 's/\t[a-zA-Z_][a-zA-Z0-9_:]*::([a-z_]*tests)::/\t\1::/' | sort; }
if diff -u <(norm "$base") <(norm "$fresh"); then
  echo "case inventory ($shape): identical"
else
  echo "case inventory ($shape): DRIFTED" >&2
  exit 1
fi
