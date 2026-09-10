#!/usr/bin/env bash
# Compare a fresh case inventory against the baseline.
#   compare-inventory.sh rust-only|cudf <fresh-inventory-file>
# Integration targets compare verbatim. Lib cases are named by module path, which this
# task moves by construction, so they compare on the suffix from the last `::tests::`
# onward — unique across all 437, so the comparison is still exact.
set -uo pipefail
shape="${1:?usage: compare-inventory.sh rust-only|cudf <file>}"
fresh="${2:?usage: compare-inventory.sh rust-only|cudf <file>}"
base="$(dirname "$0")/inv-$shape.txt"
# Prefix every case with its target, or a case moving between targets would cancel out
# in a globally sorted comparison.
# The module path a lib case is named by is exactly what this task moves, so it is dropped
# down to the test module itself. `schema_tests` counts: any segment ending in `tests` is the
# boundary, and the suffix from there on is unique across all 437.
norm() { awk '/^== /{t=$2; next} {print t"\t"$0}' "$1" \
  | sed -E 's/\t[a-zA-Z_][a-zA-Z0-9_:]*::([a-z_]*tests)::/\t\1::/' | sort; }
if diff -u <(norm "$base") <(norm "$fresh"); then
  echo "case inventory ($shape): identical"
else
  echo "case inventory ($shape): DRIFTED" >&2
  exit 1
fi
