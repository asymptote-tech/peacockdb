#!/usr/bin/env bash
# The install a device run ships (scripts/env/run-gpu.sh), built in the build image.
#
#   stage-install.sh [--cudf <v>]
#
# Run at the tree root inside ghcr.io/asymptote-tech/peacockdb-build:cudf-<v>, 25.02 by default
# (llm-wiki/build-test.md, "The CPU-VM build"). The C++ build and install into cpp/install; then,
# staged from empty, the gpu test binaries into cpp/install/rust-tests/ (test_gpu_corpus,
# test_node_timing, peacock_gpu_benchmarks, and the crate's own as peacockdb_core_gpu_lib) and
# the release build of peacock_gpu_benchmarks into cpp/install/rust-benchmarks/, as
# build-test-shadgpu.sh --build and --build-benchmarks stage them, and beside it the release build of
# prototype_fused_benchmarks (query-fuser), which run-gpu.sh runs by name; each stripped of debug info.
# cargo builds into target-img-<v> in the tree, unless CARGO_TARGET_DIR is set.
#
# Exit 0; 2 bad input, or not the build image of <v>; 1 a build failed.
set -euo pipefail

usage() { sed -n '2,${/^#/!q;p;}' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'; exit 0; }
die() { echo "stage-install.sh: $*" >&2; exit 2; }

cudf=25.02
while [ $# -gt 0 ]; do
  case "$1" in
    -h|--help) usage ;;
    --cudf) [ -n "${2:-}" ] || die "--cudf needs a version such as 25.02"; cudf=$2; shift 2 ;;
    *) die "unknown argument $1" ;;
  esac
done
[ -f "docker/conda/cudf-$cudf.spec" ] || die "no cuDF $cudf: run at the tree root; docker/conda has no cudf-$cudf.spec"
# The image sets GCC_VERSION, its cuDF version's compiler (docker/pins.env, GCC_<v>).
[ -n "${GCC_VERSION:-}" ] || die "no GCC_VERSION: run inside the build image"
gcc_pin=$(sed -n "s/^GCC_${cudf//./_}=//p" docker/pins.env)
[ "$GCC_VERSION" = "$gcc_pin" ] \
  || die "this image's gcc is $GCC_VERSION, and cuDF $cudf's is ${gcc_pin:-unpinned}: not the build image of cuDF $cudf"

. scripts/lib/stage-cargo.sh
export CARGO_TARGET_DIR=${CARGO_TARGET_DIR:-$PWD/target-img-$cudf}
TESTS=cpp/install/rust-tests
BENCH=cpp/install/rust-benchmarks
# build-test-shadgpu.sh's RUST_TESTS and RUST_LIB_STAGED: the device rung, and the names the run
# image's run-tests.sh knows.
GPU_TESTS=(test_gpu_corpus test_node_timing peacock_gpu_benchmarks)
LIB_STAGED=peacockdb_core_gpu_lib

./scripts/build.sh --configure --build --install --cudf_ROOT /opt/cudf --gcc-version "$GCC_VERSION"

# From empty: the run globs the directories, so a binary left by an earlier build would run.
rm -rf "$TESTS" "$BENCH"
staged=()
for t in "${GPU_TESTS[@]}"; do
  stage_cargo_test_binary "$t" "$TESTS" --features gpu
  staged+=("$TESTS/$t")
done
stage_cargo_lib_binary "$LIB_STAGED" "$TESTS" --features gpu
staged+=("$TESTS/$LIB_STAGED")
# --release, and without --features gpu, as --build-benchmarks builds it: the harness refuses to
# measure a debug build.
stage_cargo_test_binary peacock_gpu_benchmarks "$BENCH" --release
staged+=("$BENCH/peacock_gpu_benchmarks")
# Its Cargo.toml entry requires both features; no CI step builds it.
stage_cargo_test_binary prototype_fused_benchmarks "$BENCH" --release --features gpu,test-support
staged+=("$BENCH/prototype_fused_benchmarks")

# --strip-debug keeps the dynamic symbol table; the debug info is most of each binary's size.
for f in "${staged[@]}"; do strip --strip-debug "$f"; done
echo "==> staged into cpp/install: ${staged[*]#cpp/install/}"
