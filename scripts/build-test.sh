#!/usr/bin/env bash
# The one build command: every build shape, in the build image, over this checkout
# (llm-wiki/build-test.md, "Local build workflows and caches").
#
#   build-test.sh [--cudf <v>] [--rust-only|--gpu] [--build] [--run] [--no-release-bench]
#                 [--pass-env NAME]... [-- <command...>]
#
# <v> is $PEACOCK_CUDF, else 25.02. Outside the image the script runs itself again in
# $PEACOCK_BUILD_IMAGE, else in the image `scripts/env/image-ref.sh build <v>` finds: docker run as
# the caller's uid, the tree at its own path, cargo's target in <tree>/target-img-<v>, HOME in
# <tree>/.home-img, the cargo registry and ccache under ~/.cache/peacock. Inside it:
#
#   -- <command...>  runs the command; its exit is the script's
#   (cpu, default)   --build: the C++ library and its tests into cpp/build, then every Rust target
#                    that builds with cmake, and the CLI; --run: those targets and ctest -L cpu
#   --rust-only      every Rust target that builds without cmake, and the CLI; no C++
#   --gpu            --build: scripts/env/stage-install.sh, the install a device run ships;
#                    --no-release-bench skips its release benchmarks. The device run is
#                    scripts/env/run-gpu.sh's, so --gpu --run is refused
#
# The cpu and gpu builds own cpp/build and stamp it with <v> in cpp/build/.peacock-cudf. A
# cpp/build of another version, or one with no stamp (a host configure), is refused, not
# reconfigured. --run runs every target even after one fails.
# These reach the container by name when set, never by value: RUSTFLAGS CMAKE_GENERATOR LDFLAGS
# LD_LIBRARY_PATH PCK_TEST_FILTER PCK_SHARD UPDATE_CANONICAL PEACOCK_NO_FETCH RUST_TEST_THREADS
# CARGO_BUILD_JOBS, and each --pass-env NAME. RUNNER_TEMP is mounted at its own path when set.
#
# Exit 0; 1 a build or a test failed; 2 bad input, or a refused image, target dir or cpp/build.
set -euo pipefail

usage() { sed -n '2,${/^#/!q;p;}' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'; exit 0; }
die() { echo "build-test.sh: $*" >&2; exit 2; }
removed() { die "$1 is gone: $2"; }

FORWARDED=(RUSTFLAGS CMAKE_GENERATOR LDFLAGS LD_LIBRARY_PATH PCK_TEST_FILTER PCK_SHARD UPDATE_CANONICAL
           PEACOCK_NO_FETCH RUST_TEST_THREADS CARGO_BUILD_JOBS)
DEVICE_RUN="scripts/env/run-gpu.sh --instance-id computeinstance-e00tnrse7ayntnzcyt"

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)
original=("$@")
cudf=${PEACOCK_CUDF:-25.02}
mode=cpu mode_flag="" build=0 run=0 no_release_bench=0
pass_env=() command=()

# --gpu and --rust-only select different builds, so the two together are refused, not ordered.
set_mode() {
  [ -z "$mode_flag" ] || [ "$mode_flag" = "$1" ] \
    || die "$mode_flag and $1 select different builds (rust-only builds without cmake, gpu for a device): pass one"
  mode_flag=$1 mode=${1#--}
}

while [ $# -gt 0 ]; do
  case "$1" in
    -h|--help) usage ;;
    --cudf) [ -n "${2:-}" ] || die "--cudf needs a version such as 25.02"; cudf=$2; shift ;;
    --rust-only|--gpu) set_mode "$1" ;;
    --build) build=1 ;;
    --run) run=1 ;;
    --no-release-bench) no_release_bench=1 ;;
    --pass-env)
      [[ "${2:-}" =~ ^[A-Za-z_][A-Za-z0-9_]*$ ]] || die "--pass-env takes a variable's name, never its value: ${2:-nothing}"
      pass_env+=("$2"); shift ;;
    --) shift; command=("$@"); [ ${#command[@]} -gt 0 ] || die "-- needs a command"; break ;;
    --host|--remote-dir|--remote-cudf-root|--push-binaries)
      removed "$1" "builds run here, in the build image. A device run ships the install with $DEVICE_RUN ship, then run" ;;
    --push-*) removed "$1" "$DEVICE_RUN ship sends the committed testdata with the install" ;;
    --pull-benchmarks|--pull-*) removed "$1" "$DEVICE_RUN pull brings a run's results home" ;;
    --local-cudf-root) removed "$1" "the image's cuDF is /opt/cudf: pick its version with --cudf <v>" ;;
    --gcc-version) removed "$1" "the image's gcc is its cuDF's, GCC_<v> in docker/pins.env: pick it with --cudf <v>" ;;
    --all) removed "$1" "pass --build --run" ;;
    --build-benchmarks) removed "$1" "--gpu --build stages cpp/install/rust-benchmarks/ (scripts/env/stage-install.sh)" ;;
    --run-benchmarks) removed "$1" "$DEVICE_RUN run bench" ;;
    --update-canonical) removed "$1" "UPDATE_CANONICAL=1 scripts/build-test.sh --rust-only --run" ;;
    *) die "unknown flag $1" ;;
  esac
  shift
done

# ---- validation: all of it before the first side effect -------------------------------------
known=""
for spec in "$root"/docker/conda/cudf-*.spec; do spec=${spec##*/cudf-}; known+="${spec%.spec} "; done
[ -f "$root/docker/conda/cudf-$cudf.spec" ] || die "no cuDF $cudf: docker/conda holds specs for ${known% }"
if [ ${#command[@]} -gt 0 ]; then
  [ "$build$run$no_release_bench" = 000 ] && [ -z "$mode_flag" ] \
    || die "a command runs alone: drop --build, --run, --gpu, --rust-only and --no-release-bench beside --"
elif [ "$build" = 0 ] && [ "$run" = 0 ]; then
  die "nothing to do: pass --build, --run, or -- <command>"
fi
[ "$no_release_bench" = 0 ] || { [ "$mode" = gpu ] && [ "$build" = 1 ]; } \
  || die "--no-release-bench is a --gpu --build flag"
[ "$mode" != gpu ] || [ "$run" = 0 ] \
  || die "a device run is not this script's: build with --gpu --build, then $DEVICE_RUN ship and $DEVICE_RUN run list"

# ---- outside the image: run this script again inside it --------------------------------------
if [ "${PEACOCK_IN_IMAGE:-}" != 1 ]; then
  # Per worktree, because a build script bakes CARGO_MANIFEST_DIR into what it builds.
  target=${CARGO_TARGET_DIR:-$root/target-img-$cudf}
  case "$target/" in
    "$root"/*) ;;
    *) die "CARGO_TARGET_DIR=$target is outside this tree, $root: each worktree keeps its own target dir. Unset it or name a dir inside the tree" ;;
  esac
  if [ -n "${PEACOCK_BUILD_IMAGE:-}" ]; then
    image=$PEACOCK_BUILD_IMAGE
  else
    image=$("$root/scripts/env/image-ref.sh" build "$cudf") || exit 2
  fi
  cache=${XDG_CACHE_HOME:-$HOME/.cache}/peacock
  registry=${PEACOCK_CARGO_REGISTRY:-$cache/cargo-registry}
  ccache=${PEACOCK_CCACHE_DIR:-$cache/ccache-$cudf}
  mkdir -p "$registry" "$ccache" "$root/.home-img"
  args=(run --rm --user "$(id -u):$(id -g)" -v "$root:$root")
  # A worktree's git dir lies in its main clone's.
  common=$(git -C "$root" rev-parse --path-format=absolute --git-common-dir)
  case "$common/" in "$root"/*) ;; *) args+=(-v "$common:$common") ;; esac
  args+=(-w "$root" -e PEACOCK_IN_IMAGE=1 -e "PEACOCK_CUDF=$cudf" -e "HOME=$root/.home-img"
         -e "CARGO_TARGET_DIR=$target" -v "$registry:/opt/rust/cargo/registry" -v "$ccache:/tmp/ccache")
  # A CI step's report files; GITHUB_OUTPUT and the like stay the runner's, written by redirecting.
  if [ -n "${RUNNER_TEMP:-}" ]; then args+=(-v "$RUNNER_TEMP:$RUNNER_TEMP" -e RUNNER_TEMP); fi
  for name in "${FORWARDED[@]}"; do
    if [ -n "${!name+set}" ]; then args+=(-e "$name"); fi
  done
  for name in "${pass_env[@]}"; do args+=(-e "$name"); done
  exec docker "${args[@]}" "$image" scripts/build-test.sh "${original[@]}"
fi

# ---- inside the image -------------------------------------------------------------------------
if [ ${#command[@]} -gt 0 ]; then exec "${command[@]}"; fi
cd "$root"
export CARGO_TARGET_DIR=${CARGO_TARGET_DIR:-$root/target-img-$cudf}
STAMP=cpp/build/.peacock-cudf

# Suite selection, DERIVED from the sources, each entry <package>:<test-name>. Targets are
# classified once, by what they REQUIRE, and each mode takes everything it can support: three
# hand-written lists drifted apart, always toward running less than the mode could.
#   needs_cmake   file-gated on not(rust-only): cannot compile without libpeacock_gpu.
#   rust_only     everything else that compiles without the gpu feature: no cmake, no CUDA.
#   cpu           rust_only + needs_cmake, minus the GPU-runtime set.
# Every mode also builds and runs the crate's own unit tests, `--lib`, which no `--test`
# derivation sees. A mode that BUILDS more must not RUN less: rust-only ⊂ cpu ⊂ gpu-capable.
#
# The rust-only membership test is the feature's own definition (build-test.md, "What rust-only
# means"): a file-level `#![cfg(not(feature = "rust-only"))]` is exactly the marker that says
# "this cannot exist without the FFI". Derived from the sources, so a new test file classifies itself.
rust_only_targets() {
  local pkg dir f base
  for pkg in peacockdb-core peacockdb-ffi; do
    dir="$pkg/tests"
    [ -d "$dir" ] || continue
    for f in "$dir"/*.rs; do
      [ -e "$f" ] || continue
      base=$(basename "$f" .rs)
      # File-level gate => needs cmake; excluded from the rust-only set.
      grep -qF '#![cfg(not(feature = "rust-only"))]' "$f" && continue
      # Gated on the gpu feature, which its Cargo.toml entry requires: cargo refuses to build it
      # without, and stage-install.sh builds it.
      grep -qF '#![cfg(feature = "gpu")]' "$f" && continue
      # Excluded for measured reasons: test_ffi compiles under rust-only to ZERO tests (one
      # cfg covers the file), so running it is a run of nothing that reads as coverage;
      # test_crate_privacy compiles probes against the crate's rlib, and the cost-report job
      # runs it on its own.
      case "$base" in
        test_ffi | test_crate_privacy) continue ;;
      esac
      # Needs a GPU at RUNTIME even where it compiles fine — subtracted as a SET, not
      # by name. See gpu_runtime_targets.
      if gpu_runtime_targets | grep -qx "$pkg:$base"; then
        continue
      fi
      printf '%s:%s\n' "$pkg" "$base"
    done
  done
}

# The GPU-RUNTIME set: targets that need a GPU when they RUN, whatever they need to
# compile. Declared once here and subtracted wherever a mode cannot satisfy it, rather
# than filtered by name. Kept in step with build-test-shadgpu.sh's RUST_TESTS and pipeline.yml's
# gpu-tests staging array (cost-report's test_source_formatting, runners.rs, reads all three).
gpu_runtime_targets() {
  cat <<'GPUSET'
peacockdb-core:test_gpu_corpus
peacockdb-core:test_node_timing
peacockdb-core:peacock_gpu_benchmarks
GPUSET
}

# Targets that need cmake to compile at all, in dependency-name order.
needs_cmake_targets() {
  local pkg dir f base
  for pkg in peacockdb-core peacockdb-ffi; do
    dir="$pkg/tests"
    [ -d "$dir" ] || continue
    for f in "$dir"/*.rs; do
      [ -e "$f" ] || continue
      base=$(basename "$f" .rs)
      grep -qF '#![cfg(not(feature = "rust-only"))]' "$f" && printf '%s:%s\n' "$pkg" "$base"
    done
  done
  # test_ffi is not file-gated but is empty without the FFI, so it belongs to the
  # cmake side rather than the rust-only one.
  printf '%s\n' "peacockdb-ffi:test_ffi"
}

DERIVED=() features=()
case "$mode" in
  rust-only)
    mapfile -t DERIVED < <(rust_only_targets)
    features=(--features rust-only) ;;
  cpu)
    # test_gpu_* compile here but need a GPU at run time, which a device run is for.
    mapfile -t DERIVED < <(
      rust_only_targets
      needs_cmake_targets | grep -vxF -f <(gpu_runtime_targets)
    ) ;;
esac
# A derived suite that comes back EMPTY is a bug in the derivation, not a configuration: every
# loop over it would vanish and the script exit 0 having verified nothing.
if [ "$mode" != gpu ] && [ ${#DERIVED[@]} -eq 0 ]; then
  echo "build-test.sh: the derived Rust suite is EMPTY for mode $mode: see rust_only_targets and needs_cmake_targets" >&2
  exit 1
fi

# cpp/build is this version's, or empty, or refused: never reconfigured over another's.
cpp_build_is_ours() {
  local had
  if [ -f "$STAMP" ]; then
    had=$(cat "$STAMP")
    [ "$had" = "$cudf" ] || die "cpp/build is configured for cuDF $had, and this build is cuDF $cudf. Build with --cudf $had, or remove cpp/build and cpp/install first"
  elif [ -n "$(ls -A cpp/build 2>/dev/null)" ]; then
    die "cpp/build holds a configure no build here made: it has no $STAMP stamp (a host build, before the build image?). It is left alone; remove it and cpp/install to build here: rm -rf cpp/build cpp/install"
  fi
}

stamp_cpp_build() {
  mkdir -p cpp/build
  printf '%s\n' "$cudf" > "$STAMP"
}

# A cold cuDF build OOMs a 15 GiB host at full parallelism; CARGO_BUILD_JOBS overrides.
throttle_cargo() {
  local mem_gib
  [ -z "${CARGO_BUILD_JOBS:-}" ] || return 0
  mem_gib=$(awk '/MemTotal/ { printf "%d", $2 / 1024 / 1024 }' /proc/meminfo 2>/dev/null || echo 32)
  if [ "${mem_gib:-32}" -lt 20 ]; then
    export CARGO_BUILD_JOBS=3
    echo "==> low-memory host (${mem_gib} GiB): CARGO_BUILD_JOBS=3"
  fi
}

# One target per cargo invocation, so at most one test binary links at a time.
build_rust() {
  local spec
  for spec in "${DERIVED[@]}"; do
    echo "==> build ${spec%%:*} --test ${spec##*:}"
    cargo test --no-run "${features[@]}" -p "${spec%%:*}" --test "${spec##*:}" || exit 1
  done
  echo "==> build peacockdb-core --lib"
  cargo test --no-run "${features[@]}" -p peacockdb-core --lib || exit 1
  # The CLI, which has no test target: the one caller of the planner from outside the crate.
  echo "==> build the CLI"
  cargo build --features rust-only -p peacockdb || exit 1
}

if [ "$build" = 1 ]; then
  case "$mode" in
    gpu)
      cpp_build_is_ours
      stamp_cpp_build
      throttle_cargo
      stage=(scripts/env/stage-install.sh --cudf "$cudf")
      if [ "$no_release_bench" = 1 ]; then stage+=(--no-release-bench); fi
      "${stage[@]}" || exit 1 ;;
    cpu)
      : "${GCC_VERSION:?no GCC_VERSION: run inside the build image}"
      cpp_build_is_ours
      stamp_cpp_build
      throttle_cargo
      echo "==> build C++ into cpp/build against /opt/cudf (gcc-$GCC_VERSION)"
      scripts/build.sh --configure --build --install --cudf_ROOT /opt/cudf --gcc-version "$GCC_VERSION" \
        --compiler-launcher ccache || exit 1
      build_rust ;;
    rust-only)
      build_rust ;;
  esac
fi

[ "$run" = 1 ] || exit 0

filter=${PCK_TEST_FILTER:-}
rc=0
# Sets rc=1 when the target fails, or ran no test with no filter to excuse it.
run_target() {  # <package> <name> <cargo selector...>
  local pkg=$1 name=$2 log status
  shift 2
  local args=(test "${features[@]}" -p "$pkg" "$@")
  if [ -n "$filter" ]; then args+=(-- "$filter"); fi
  log=$(mktemp)
  echo "==> $pkg $name"
  set +e
  cargo "${args[@]}" 2>&1 | tee "$log"
  status=${PIPESTATUS[0]}
  set -e
  if [ "$status" -ne 0 ]; then
    echo "!!! $name failed (exit $status)"
    rc=1
  elif [ -z "$filter" ] && grep -q '^running 0 tests' "$log"; then
    echo "!!! $name ran 0 tests: nothing was verified"
    rc=1
  fi
  rm -f "$log"
}

if [ "$mode" = cpu ]; then
  [ "$(cat "$STAMP" 2>/dev/null)" = "$cudf" ] || die "cpp/build is not a cuDF $cudf build: run --build first"
  # The FFI binaries load libcudf from the image's prefix.
  export LD_LIBRARY_PATH=/opt/cudf/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}
  echo "==> ctest -L cpu"
  if ! ctest --test-dir cpp/build -L cpu --output-on-failure; then
    echo "!!! ctest -L cpu failed"
    rc=1
  fi
fi
for spec in "${DERIVED[@]}"; do
  run_target "${spec%%:*}" "${spec##*:}" --test "${spec##*:}"
done
run_target peacockdb-core --lib --lib
if [ "$rc" -eq 0 ]; then echo "==> build-test.sh $mode --run: green"; else echo "==> build-test.sh $mode --run: FAILED" >&2; fi
exit "$rc"
