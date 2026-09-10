#!/usr/bin/env bash
# Case inventory for one feature shape: every `--list` case of `--lib` and of each
# `peacockdb-core/tests/test_*.rs` target, grouped by target. A refactor that moves no test
# brings every line back byte-identical, which is what makes it a baseline.
#   scripts/case-inventory.sh rust-only  > /tmp/inv.txt
#   CUDF_ROOT=~/data/miniforge3/envs/rapids-cuda-12.2 \
#     scripts/case-inventory.sh cudf     > /tmp/inv-cudf.txt
# A cudf-shape binary run without LD_LIBRARY_PATH lists zero cases instead of failing
# (build-test.md), so the cudf shape prepends the FFI OUT_DIR and the cuDF lib dir.
set -euo pipefail
shape="${1:?usage: case-inventory.sh rust-only|cudf}"
root="$(git rev-parse --show-toplevel)"
cd "$root"

targets=(--lib)
while IFS= read -r t; do targets+=(--test "$t"); done < <(
  find peacockdb-core/tests -maxdepth 1 -name 'test_*.rs' -printf '%f\n' | sed 's/\.rs$//' | sort
)

case "$shape" in
  rust-only)
    cargo="cargo"; feat=(--features rust-only)
    ;;
  cudf)
    : "${CUDF_ROOT:?set CUDF_ROOT for the cudf shape}"
    cargo="$root/scripts/cargo-cudf.sh"; feat=()
    export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$root/target-cudf-$(basename "$CUDF_ROOT")}"
    ffi_out="$(echo "$CARGO_TARGET_DIR"/debug/build/peacockdb-ffi-*/out/lib | tr ' ' ':')"
    export LD_LIBRARY_PATH="$ffi_out:$CUDF_ROOT/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
    ;;
  *) echo "unknown shape $shape" >&2; exit 1 ;;
esac

"$cargo" test "${feat[@]}" -p peacockdb-core "${targets[@]}" --no-run >/dev/null

i=0
while [ $i -lt ${#targets[@]} ]; do
  if [ "${targets[$i]}" = "--lib" ]; then sel=(--lib); name="--lib"; i=$((i+1))
  else sel=(--test "${targets[$((i+1))]}"); name="${targets[$((i+1))]}"; i=$((i+2)); fi
  echo "== $name"
  "$cargo" test "${feat[@]}" -p peacockdb-core "${sel[@]}" -- --list 2>/dev/null | sort
done
