# shellcheck shell=bash
#
# Shared build/deploy environment for the shad-gpu workflow: toolchain pinning, cargo
# target dir, the remote, and the helpers every phase uses. Sourced by the driver script,
# by create_nsys_profile.sh, and by anything else reaching the host with these settings.
#
# Sourced, never executed: no `set -e` here, and the only side effect below is a refusal
# when this host's glibc version cannot be read, which every shipped binary needs.
#
# scripts/docker-build.sh greps `^CUDF_ROOT=` out of this file for the conda prefix its
# container shims into place; moving or reformatting that assignment breaks it.

CUDF_ROOT=/home/dmitry/data/miniforge3/envs/rapids-cuda-12.2
export CUDF_ROOT

# nvcc 12.2 (the conda env's CUDA toolkit) hard-rejects gcc>12 in host_config.h,
# and Ubuntu's default cc/c++ is gcc-14. CC/CXX are what the `cmake` crate honors;
# the C++ build takes --gcc-version at the call site.
#   sudo apt install gcc-12 g++-12
GCC_VERSION=12
export CC=/usr/bin/gcc-${GCC_VERSION}
export CXX=/usr/bin/g++-${GCC_VERSION}

# One target dir per cuDF root, so each version stays permanently warm. Two things
# bust the fingerprints of the whole DataFusion subgraph: the `rust-only` feature
# (it re-enables arrow's `ffi`), and a different cudf_ROOT (it changes the FFI
# build's resolved Arrow/cudf). Sharing ./target across either recompiles that
# stack on every flip. Override with CARGO_TARGET_DIR.
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$PWD/target-cudf-$(basename "$CUDF_ROOT")}"

# The first build in a fresh target-cudf recompiles the DataFusion stack at opt-3
# (#85), and at full parallelism that exhausts RAM+swap on a small host (seen on a
# 15GiB box: two OOM kills before throttling). Override with CARGO_BUILD_JOBS.
if [ -z "${CARGO_BUILD_JOBS:-}" ]; then
  _mem_gib=$(awk '/MemTotal/{printf "%d", $2/1024/1024}' /proc/meminfo 2>/dev/null || echo 32)
  if [ "${_mem_gib:-32}" -lt 20 ]; then
    export CARGO_BUILD_JOBS=3
    echo "==> low-memory host (${_mem_gib}GiB RAM): throttling CARGO_BUILD_JOBS=3 to avoid OOM"
  fi
fi

REMOTE=shad-gpu
REMOTE_REPO=/home/info/peacockdb

# Validation belongs before a phase's first side effect: half a deploy followed by "you
# cannot do that" is worse than either outcome alone.
die() { echo "$*" >&2; exit 1; }

# The glibc a shipped binary is patched to is the BUILD host's: 2.35 from a 22.04 box or
# CI's container, 2.39 from a 24.04 one. Read here, where the binaries are built, and used
# by the patch phase and by every remote command that loads one.
BUILD_GLIBC=$(getconf GNU_LIBC_VERSION | cut -d' ' -f2)
[ -n "$BUILD_GLIBC" ] || die "cannot read this host's glibc version from getconf"

# The library path a shipped binary needs on the host, as REMOTE shell text: `$HOME` and
# `$LD_LIBRARY_PATH` are left for the remote to expand. Applied per command and never
# exported — exported, the host's own coreutils load the patched glibc and segfault, so a
# run reports a bogus code having actually succeeded.
PATCHED_LD="$REMOTE_REPO/cpp/install/lib:/usr/local/cuda-12.5/compat"
PATCHED_LD="$PATCHED_LD:/home/info/glibc-$BUILD_GLIBC/lib"
PATCHED_LD="$PATCHED_LD:\$HOME/miniforge3/envs/rapids-cuda-12.2/lib:\${LD_LIBRARY_PATH:-}"

# sf40 lives outside the repo on the host and is read in place; 40 GB is not copied per run.
SF40_DIR=/home/info/peacock-datasets/testdata/tpch.sf40

# The "N passed" libtest printed in a log, summed over its passes. That count is the only
# honest answer to "did the filter match anything" — counting output files answers a
# different question. Shipped into a remote script with `declare -f`, since the run it
# counts happens there.
passed_count() {
  sed -n 's/^test result:.* \([0-9][0-9]*\) passed.*/\1/p' "$1" \
    | awk '{n += $1} END {print n + 0}'
}

# rsync over the flaky, bursty shad-gpu link, made self-healing rather than
# all-or-nothing: --partial --inplace so a retry resumes the same file instead of
# restarting it, --timeout=90 so a stalled connection aborts and can reconnect.
# The attempt cap is what stops a genuinely-down host from looping forever. Caller
# passes the mode flags and src/dst.
resilient_rsync() {
  local attempt=1 max_attempts=100 rc=0
  while :; do
    rsync -P --partial --inplace --timeout=90 "$@" && return 0
    rc=$?
    if [ "$attempt" -ge "$max_attempts" ]; then
      echo "rsync: giving up after $attempt attempts (last rc=$rc)" >&2
      return "$rc"
    fi
    echo "rsync: attempt $attempt stalled/failed (rc=$rc); resuming in 5s..." >&2
    attempt=$((attempt + 1))
    sleep 5
  done
}

# stage_cargo_test_binary <target> <staging-dir> [extra cargo args...]
#
# Build one integration test and copy its binary into the staging dir under its
# target name.
stage_cargo_test_binary() {
  local target=$1 staging=$2
  shift 2
  stage_cargo_binary "$target" test "$target" "$staging" --test "$target" "$@"
}

# stage_cargo_lib_binary <staged-name> <staging-dir> [extra cargo args...]
#
# The crate's own unit-test binary, whose artifact line has kind ["lib"] and the lib's
# name, `peacockdb_core`. It is staged under an explicit name because the run loops glob the
# staging directory, and that bare name beside the target binaries reads as one of them.
stage_cargo_lib_binary() {
  local staged=$1 staging=$2
  shift 2
  stage_cargo_binary peacockdb_core lib "$staged" "$staging" --lib "$@"
}

# stage_cargo_binary <cargo target name> <kind> <staged-name> <staging-dir> <cargo args...>
#
# The built path carries a metadata hash, so it is read out of cargo's json artifact
# lines rather than guessed: globbing `deps/<target>-*` picks up every stale hash from
# previous builds. The kind is matched too, since the lib and its test binary share a
# name and only the latter is what `test: true` builds.
stage_cargo_binary() {
  local name=$1 kind=$2 staged=$3 staging=$4
  shift 4
  local exec_path
  # `set -o pipefail` in the caller is what makes a compile failure land here as a
  # build failure rather than as an empty result reported as a missing binary. The
  # reader drains stdin rather than stopping at the match: cargo writes a final
  # `build-finished` line after the artifact, and a closed pipe there is EPIPE.
  if ! exec_path=$(cargo test --no-run -p peacockdb-core "$@" \
      --message-format=json \
    | python3 -c '
import json, sys
name, kind = sys.argv[1], sys.argv[2]
found = None
for line in sys.stdin:
    try: m = json.loads(line)
    except ValueError: continue
    target = m.get("target") or {}
    if found is None and m.get("executable") and target.get("name") == name and kind in target.get("kind", []):
        found = m["executable"]
if found: print(found)
' "$name" "$kind"); then
    echo "ERROR: building $staged failed (cargo output above)" >&2
    return 1
  fi
  if [ -z "$exec_path" ] || [ ! -f "$exec_path" ]; then
    echo "ERROR: $staged built, but no artifact line named its executable" >&2
    return 1
  fi
  mkdir -p "$staging"
  cp -f "$exec_path" "$staging/$staged"
  echo "--- Staged: $staging/$staged"
}

# pull_one <path under testdata/> <what it is>
#
# Fetch one file from the host into the same place here, and return 1 when there is none.
# Tested over ssh rather than by letting the transfer fail: resilient_rsync retries a
# missing source a hundred times, and eight minutes of backoff reads as a hang.
pull_one() {
  local rel=$1 what=$2
  if ! ssh "$REMOTE" test -f "$REMOTE_REPO/testdata/$rel"; then
    echo "==> $what: nothing on the host"
    return 1
  fi
  mkdir -p "testdata/$(dirname "$rel")"
  resilient_rsync "$REMOTE:$REMOTE_REPO/testdata/$rel" "testdata/$rel"
  case "$rel" in
    *.tsv) echo "==> $what: $(grep -vc '^#' "testdata/$rel") rows" ;;
    *)     echo "==> $what: $(du -h "testdata/$rel" | cut -f1)" ;;
  esac
}
