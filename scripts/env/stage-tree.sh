#!/usr/bin/env bash
# Build and stage the tree gpu.sh ships, in an instance (llm-wiki/build-test.md, "GPU hosts").
#
#   stage-tree.sh [--bench]
#
# cpp/install made again from nothing: libpeacock_gpu.so and the C++ suites, then the device rung's
# rust binaries in rust-tests/, stripped of debug info, and with --bench the measurement harness's
# release build in rust-benchmarks/. Named and stripped as build-test-shadgpu.sh stages and ships
# them; test_stage_tree.py holds the lists to its. Stops at the first failure, with nothing to ship.
set -euo pipefail
shopt -s inherit_errexit

usage() { sed -n '2,${/^#/!q;p;}' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'; exit 0; }
die() { echo "stage-tree.sh: $*" >&2; exit 2; }

RUST_TESTS=(test_gpu_corpus test_node_timing peacock_gpu_benchmarks)
RUST_LIB_STAGED=peacockdb_core_gpu_lib
RUST_TESTS_STAGING=cpp/install/rust-tests
BENCH_TARGET=peacock_gpu_benchmarks
BENCH_STAGING=cpp/install/rust-benchmarks
# Generous: a cold build of the DataFusion stack takes tens of minutes, a hung one forever.
LIMIT=${PEACOCK_STAGE_TIMEOUT:-2h}

bench=""
for arg in "$@"; do
  case "$arg" in
    -h|--help) usage ;;
    --bench) [ -z "$bench" ] || die "one --bench per call"; bench=1 ;;
    -*) die "unknown flag $arg" ;;
    *) die "no argument is positional: $arg" ;;
  esac
done
for key in CUDF_ROOT GCC_VERSION; do
  [ -n "${!key:-}" ] || die "no $key: run in an instance, whose build image sets it"
done
cd "$(dirname "${BASH_SOURCE[0]}")/../.."

# staged <dir> <cargo target> <staged name> <cargo argument>... — the executable cargo builds for
# <cargo target>, read off its json lines rather than guessed, copied to <dir>/<staged name>.
staged() {
  local dir=$1 name=$2 as=$3 exe
  shift 3
  exe=$(timeout "$LIMIT" cargo test --no-run -p peacockdb-core "$@" --message-format=json \
          | jq -r --arg n "$name" 'select(.executable != null and .target.name == $n) | .executable')
  [ -n "$exe" ] || { echo "stage-tree.sh: cargo named no executable for $name" >&2; exit 1; }
  mkdir -p "$dir"
  cp "$exe" "$dir/$as"
  echo "staged $dir/$as"
}

rm -rf cpp/install
timeout "$LIMIT" scripts/build.sh --cudf_ROOT "$CUDF_ROOT" --gcc-version "$GCC_VERSION" --all
for t in "${RUST_TESTS[@]}"; do staged "$RUST_TESTS_STAGING" "$t" "$t" --features gpu --test "$t"; done
staged "$RUST_TESTS_STAGING" peacockdb_core "$RUST_LIB_STAGED" --features gpu --lib
for t in "${RUST_TESTS[@]}" "$RUST_LIB_STAGED"; do strip --strip-debug "$RUST_TESTS_STAGING/$t"; done
if [ -n "$bench" ]; then
  staged "$BENCH_STAGING" "$BENCH_TARGET" "$BENCH_TARGET" --release --test "$BENCH_TARGET"
fi
