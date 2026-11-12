#!/usr/bin/env bash
# The one build command: every build shape, in the build image, over this checkout
# (llm-wiki/build-test.md, "Local build workflows and caches").
#
#   build-test.sh [--cudf <v>] [--rust-only|--gpu] [--build] [--run] [--no-release-bench]
#                 [--pass-env NAME]... [-- <command...>]
#
# <v> is $PEACOCK_CUDF, else 25.02. Outside the image the script runs itself again in
# $PEACOCK_BUILD_IMAGE, else in the image `scripts/env/image-ref.sh build <v>` finds, either refused
# unless its peacockdb.inputs label is this tree's for <v>: docker run as
# the caller's uid, the tree at its own path, cargo's target in <tree>/target-img-<v>, HOME in
# <tree>/.home-img, the cargo registry and ccache under ~/.cache/peacock. A tree or a worktree's git
# dir under the image's home, /home/peacock, is refused for a uid other than 1000, which cannot
# enter it. Inside it:
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
# reconfigured. Both need third_party/cudf checked out. A shell in the image is <v>'s alone: a
# --cudf naming another is refused there. --run runs every target even after one fails.
# These reach the container by name when set, never by value: RUSTFLAGS CMAKE_GENERATOR LDFLAGS
# LD_LIBRARY_PATH PCK_TEST_FILTER PCK_SHARD UPDATE_CANONICAL PEACOCK_NO_FETCH RUST_TEST_THREADS
# CARGO_BUILD_JOBS, the regeneration knobs PCK_UPDATE_SECTIONS PEACOCK_REWRITE_RECIPE_BYTES
# COST_FILTER PCK_GPU_RESULT_VERSION, and each --pass-env NAME. RUNNER_TEMP is mounted at its own
# path when set.
# The container, peacock-build-<tree>-<v>-<pid>, runs with --init, and -it when stdin and stdout
# are terminals and this is no background job of its terminal. HUP, INT or TERM kills it, one that
# comes before docker has created it too. One left by a build-test.sh that is gone is refused.
#
# Exit 0; 1 a build or a test failed; 2 bad input, or a refused image, tree, target dir, cpp/build
# or leftover container; 129, 130 or 143 interrupted. Outside the image, any other is the container's.
set -euo pipefail

usage() { sed -n '2,${/^#/!q;p;}' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'; exit 0; }
die() { echo "build-test.sh: $*" >&2; exit 2; }
removed() { die "$1 is gone: $2"; }

FORWARDED=(RUSTFLAGS CMAKE_GENERATOR LDFLAGS LD_LIBRARY_PATH PCK_TEST_FILTER PCK_SHARD UPDATE_CANONICAL
           PEACOCK_NO_FETCH RUST_TEST_THREADS CARGO_BUILD_JOBS
           PCK_UPDATE_SECTIONS PEACOCK_REWRITE_RECIPE_BYTES COST_FILTER PCK_GPU_RESULT_VERSION)
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
# Inside, PEACOCK_CUDF names the image's version when the docker run set it, as the outer half does:
# /opt/cudf, gcc and the target dir are that version's, so another --cudf is refused.
[ "${PEACOCK_IN_IMAGE:-}" != 1 ] || [ -z "${PEACOCK_CUDF:-}" ] || [ "$cudf" = "$PEACOCK_CUDF" ] \
  || die "this shell is the build image of cuDF $PEACOCK_CUDF, not $cudf: run --cudf $cudf from the host"
if [ ${#command[@]} -gt 0 ]; then
  if [ "$build$run$no_release_bench" != 000 ] || [ -n "$mode_flag" ]; then
    die "a command runs alone: drop --build, --run, --gpu, --rust-only and --no-release-bench beside --"
  fi
elif [ "$build" = 0 ] && [ "$run" = 0 ]; then
  die "nothing to do: pass --build, --run, or -- <command>"
fi
[ "$no_release_bench" = 0 ] || { [ "$mode" = gpu ] && [ "$build" = 1 ]; } \
  || die "--no-release-bench is a --gpu --build flag"
[ "$mode" != gpu ] || [ "$run" = 0 ] \
  || die "a device run is not this script's: build with --gpu --build, then $DEVICE_RUN ship and $DEVICE_RUN run list"
# cpp/CMakeLists.txt includes cuDF's cmake files from the submodule, which a worktree lacks until checked out.
[ ${#command[@]} -gt 0 ] || [ "$mode" = rust-only ] || [ -f "$root/third_party/cudf/cmake/rapids_config.cmake" ] \
  || die "third_party/cudf is not checked out, and the C++ configure reads its cmake files: git submodule update --init --depth 1 third_party/cudf"

# ---- outside the image: run this script again inside it --------------------------------------
if [ "${PEACOCK_IN_IMAGE:-}" != 1 ]; then
  # The tree, and a worktree's git dir, which lies in its main clone's, keep their own paths in the
  # container: cpp/build's CMake cache and compile_commands.json, which host tools read, name the
  # tree. PEACOCK_IMAGE_HOME is for the tests.
  common=$(git -C "$root" rev-parse --path-format=absolute --git-common-dir)
  image_home=${PEACOCK_IMAGE_HOME:-/home/peacock}
  if [ "$(id -u)" != 1000 ]; then
    for path in "$root" "$common"; do
      case "$path/" in
        "$image_home"/*)
          die "$path lies under $image_home, the build image's home: mode 750, uid 1000's, so uid $(id -u) cannot reach it in the container. Use a clone outside $image_home" ;;
      esac
    done
  fi
  # Per worktree, because a build script bakes CARGO_MANIFEST_DIR into what it builds.
  target=${CARGO_TARGET_DIR:-$root/target-img-$cudf}
  case "$target/" in
    "$root"/*) ;;
    *) die "CARGO_TARGET_DIR=$target is outside this tree, $root: each worktree keeps its own target dir. Unset it or name a dir inside the tree" ;;
  esac
  # A given image is held to this tree's inputs for <v> as a found one is.
  given=()
  if [ -n "${PEACOCK_BUILD_IMAGE:-}" ]; then given=(--ref "$PEACOCK_BUILD_IMAGE"); fi
  image=$("$root/scripts/env/image-ref.sh" "${given[@]}" build "$cudf") || exit 2
  cache=${XDG_CACHE_HOME:-$HOME/.cache}/peacock
  registry=${PEACOCK_CARGO_REGISTRY:-$cache/cargo-registry}
  ccache=${PEACOCK_CCACHE_DIR:-$cache/ccache-$cudf}
  mkdir -p "$registry" "$ccache" "$root/.home-img"
  # One container a run, named by worktree and version, so a signal here can kill it. Its labels
  # let a later run find one that outlived its build-test.sh (a kill -9) and still holds cargo's lock.
  tree=${root##*/}
  name=peacock-build-${tree//[^A-Za-z0-9_.-]/-}-$cudf-$$
  while read -r left pid; do
    [ -z "$left" ] || [ -e "/proc/$pid" ] \
      || die "$left, left by a build-test.sh that is gone, still runs in this tree for cuDF $cudf and holds cargo's lock: docker kill $left"
  done < <(docker ps --filter "label=peacock.tree=$root" --filter "label=peacock.cudf=$cudf" \
             --format '{{.Names}} {{.Label "peacock.pid"}}')
  args=(run --rm --init --name "$name" --label "peacock.tree=$root" --label "peacock.cudf=$cudf"
        --label "peacock.pid=$$" --user "$(id -u):$(id -g)" -v "$root:$root")
  # docker run -it sets the terminal's modes, and SIGTTOU stops a background job that does. With no
  # controlling terminal, tpgid is -1 and nothing can stop it.
  stat=$(< "/proc/$$/stat")
  read -r _ _ pgrp _ _ tpgid _ <<< "${stat##*) }"
  if [ -t 0 ] && [ -t 1 ] && { [ "$tpgid" = -1 ] || [ "$tpgid" = "$pgrp" ]; }; then args+=(-it); fi
  case "$common/" in "$root"/*) ;; *) args+=(-v "$common:$common") ;; esac
  args+=(-w "$root" -e PEACOCK_IN_IMAGE=1 -e "PEACOCK_CUDF=$cudf" -e "HOME=$root/.home-img"
         -e "CARGO_TARGET_DIR=$target" -v "$registry:/opt/rust/cargo/registry" -v "$ccache:/tmp/ccache")
  # A CI step's report files; GITHUB_OUTPUT and the like stay the runner's, written by redirecting.
  if [ -n "${RUNNER_TEMP:-}" ]; then args+=(-v "$RUNNER_TEMP:$RUNNER_TEMP" -e RUNNER_TEMP); fi
  for var in "${FORWARDED[@]}"; do
    if [ -n "${!var+set}" ]; then args+=(-e "$var"); fi
  done
  for var in "${pass_env[@]}"; do args+=(-e "$var"); done
  # docker run in the background: bash runs a trap only once a foreground child ends, and the
  # container outlives a docker run that is killed. Its exit is the script's.
  signalled=""
  trap 'signalled=129' HUP
  trap 'signalled=130' INT
  trap 'signalled=143' TERM
  docker "${args[@]}" "$image" scripts/build-test.sh "${original[@]}" <&0 &
  run=$!
  # A trap ends the wait early; docker run ends once its container does. A signal can come before
  # docker has created the container, so it is killed by name for as long as docker run lives.
  while :; do
    rc=0
    wait "$run" || rc=$?
    kill -0 "$run" 2> /dev/null || break
    while [ -n "$signalled" ] && kill -0 "$run" 2> /dev/null; do
      docker kill "$name" > /dev/null 2>&1 || true
      sleep 0.5
    done
  done
  exit "${signalled:-$rc}"
fi

# ---- inside the image -------------------------------------------------------------------------
# The FFI binaries load libcudf from the image's prefix, a command's as well as --run's.
export LD_LIBRARY_PATH=/opt/cudf/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}
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
# than filtered by name. Kept in step with build-test-shadgpu.sh's RUST_TESTS and stage-install.sh's
# GPU_TESTS, which CI's gpu-tests job stages (cost-report's test_source_formatting, runners.rs, reads
# all three).
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
