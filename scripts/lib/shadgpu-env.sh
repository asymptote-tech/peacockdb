# shellcheck shell=bash
#
# Shared build/deploy environment for the shad-gpu workflow: toolchain pinning, cargo
# target dir, the remote, and the helpers the phases use. Sourced by the driver script
# and by anything else that has to reach the host with the same settings.
#
# Sourced, never executed: no `set -e` here, and nothing below has a side effect
# beyond exporting variables and defining functions.
#
# scripts/docker-build.sh greps `^CUDF_ROOT=` out of this file to derive the conda
# prefix its container shims into place; moving or reformatting that assignment
# breaks the container build.

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

# One TCP connection for the whole invocation, shared by every ssh and rsync below.
#
# Not a speed-up. A phase opens a dozen connections and sshd caps how many may be
# MID-HANDSHAKE at once (MaxStartups); past that it resets them before authentication —
# "kex_exchange_identification: read: Connection reset by peer" — and the phase dies with
# rc=255, losing a measurement to a limit we walked into ourselves. Roughly one connection
# in two here. The socket lives in the run's own directory and dies with it; %C hashes
# host/port/user, so two checkouts do not share one, and ControlPersist=60 keeps it just
# past the gap between phases.
SSH_CONTROL_DIR="${SSH_CONTROL_DIR:-${TMPDIR:-/tmp}/peacock-ssh-$(id -u)}"
mkdir -p "$SSH_CONTROL_DIR"
chmod 700 "$SSH_CONTROL_DIR"
export SSH_OPTS="-o ControlMaster=auto -o ControlPath=$SSH_CONTROL_DIR/%C -o ControlPersist=60"
# `ssh` and `rsync -e ssh` both go through this, so neither can be left behind when the
# options change. Quoted expansion is deliberate: the options contain no spaces inside a
# single argument, and word-splitting them is how they reach ssh as separate flags.
ssh() { command ssh $SSH_OPTS "$@"; }

# rsync over the flaky, bursty shad-gpu link, made self-healing rather than
# all-or-nothing: --partial --inplace so a retry resumes the same file instead of
# restarting it, --timeout=90 so a stalled connection aborts and can reconnect.
# The attempt cap is what stops a genuinely-down host from looping forever. Caller
# passes the mode flags and src/dst.
resilient_rsync() {
  local attempt=1 max_attempts=100 rc=0
  while :; do
    rsync -P --partial --inplace --timeout=90 -e "ssh $SSH_OPTS" "$@" && return 0
    rc=$?
    # 23 is "some files were not transferred" — here almost always a tracked file deleted
    # but not staged, which `git ls-files --cached` still lists. Retrying cannot help: it
    # will not be there on the hundredth attempt either, and the loop spends eight minutes
    # of backoff before saying so. Once, then out, naming the cause.
    if [ "$rc" -eq 23 ]; then
      echo "rsync: rc=23, some sources were not transferred — see the link_stat lines" >&2
      echo "       above. A tracked file deleted but not staged is still in the file" >&2
      echo "       list; 'git add -A <path>' or restore it. Not retrying: it is not a" >&2
      echo "       stall." >&2
      return "$rc"
    fi
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
# target name. The built path carries a metadata hash, so it is read out of cargo's
# json artifact lines rather than guessed: globbing `deps/<target>-*` picks up every
# stale hash from previous builds.
stage_cargo_test_binary() {
  local target=$1 staging=$2
  shift 2
  local exec_path
  # `set -o pipefail` in the caller is what makes a compile failure land here as a build
  # failure rather than an empty result read as a missing binary. The filter forwards
  # compiler messages to stderr as they stream: under --message-format=json cargo puts
  # DIAGNOSTICS on stdout too, so keeping only the artifact line eats every error.
  if ! exec_path=$(cargo test --no-run -p peacockdb-core --test "$target" \
      --message-format=json "$@" \
    | python3 -c '
import json, sys
name = sys.argv[1]
found = None
# Read to the END rather than breaking at the artifact line. Breaking closes stdin while
# cargo is still writing, and cargo then dies with `error: Broken pipe (os error 32)` --
# which `set -o pipefail` reports as a build failure of a target that built fine.
for line in sys.stdin:
    try: m = json.loads(line)
    except ValueError:
        sys.stderr.write(line); sys.stderr.flush(); continue
    if m.get("reason") == "compiler-message":
        text = (m.get("message") or {}).get("rendered")
        if text:
            sys.stderr.write(text); sys.stderr.flush()
        continue
    if m.get("executable") and (m.get("target") or {}).get("name") == name:
        found = m["executable"]
if found:
    print(found)
' "$target"); then
    echo "ERROR: building $target failed (see the compiler messages above)" >&2
    return 1
  fi
  if [ -z "$exec_path" ] || [ ! -f "$exec_path" ]; then
    echo "ERROR: $target built, but no artifact line named its executable" >&2
    return 1
  fi
  mkdir -p "$staging"
  cp -f "$exec_path" "$staging/$target"
  echo "--- Staged: $staging/$target"
}
