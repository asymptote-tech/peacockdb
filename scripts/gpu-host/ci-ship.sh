#!/usr/bin/env bash
# CI's transfer to nebius-gpu: pipeline.yml's gpu-tests ships the runner's cpp/install/, the L40S
# list and the testdata subset with this (llm-wiki/build-test.md, "GPU host"). Run from anywhere
# in the checkout, after cpp/install/ is unpacked.
#
#   ci-ship.sh <run dir, relative to the VM login's home>
#
# The runner reaches the VM over a long, lossy path: one rsync stream measured 0.24 MB/s on
# 2026-10-15, so a fresh 600 MB tree took 40 minutes. So:
# - Every run rsyncs into one persistent cache on the VM, ~/peacockdb-ci/cache. Only what
#   changed since the last run crosses, and a changed binary crosses as rsync's delta.
# - zstd on the wire.
# - Four rsync streams at once, because on a lossy path each TCP stream backs off alone.
# - Each stream retries, and --partial-dir lets a retry resume a big file instead of resending it.
# Then the run directory is a local copy of the cache, made on the VM. The run owns that copy, and
# the cleanup step's rm -rf of it never reaches the cache.
#
# Overrides, for the tests: GPU_SHIP_ROOT (the checkout), GPU_HOST_RSYNC (the rsync binary),
# GPU_SHIP_RSH (rsync's -e transport) and GPU_SHIP_SSH (runs one remote command), both split on
# spaces; GPU_SHIP_ATTEMPTS and GPU_SHIP_PAUSE (seconds, times the attempt number).
set -euo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=${GPU_SHIP_ROOT:-$(cd "$here/../.." && pwd)}
run_dir=${1:?usage: ci-ship.sh <run dir under ~ on the VM>}
RSYNC=${GPU_HOST_RSYNC:-rsync}
RSH=${GPU_SHIP_RSH:-$here/../gpu-host.sh rsh}
read -r -a vm_ssh <<< "${GPU_SHIP_SSH:-$here/../gpu-host.sh ssh}"
ATTEMPTS=${GPU_SHIP_ATTEMPTS:-5}
PAUSE=${GPU_SHIP_PAUSE:-15}
CACHE=peacockdb-ci/cache

# The run dir is rm -rf'd below and by the cleanup step, so it must be a plain path under ~ that
# is not the cache.
case "$run_dir" in
  /* | *..* | "$CACHE" | "$CACHE"/*) echo "ci-ship: refusing run dir '$run_dir'" >&2; exit 2;;
esac
cd "$root"
for d in cpp/install/lib cpp/install/rust-tests; do
  [ -d "$d" ] || { echo "ci-ship: $d is missing; unpack the binaries first" >&2; exit 2; }
done
logs=$(mktemp -d)
trap 'rm -rf "$logs"' EXIT

# Query SQL, the minimal dataset, the generator, the committed goldens and the registry fixture.
# testdata/goldens carries the per-mode `.cpu.txt` files the device tier reads read-only. The
# registry test reads cost-registry.csv. Named by hand, so a new committed fixture has to be added
# here, as in every provisioning path.
TESTDATA=(testdata/tpch-queries testdata/tpcds-queries testdata/tpch.minimal testdata/goldens
          testdata/tpch-vec-queries testdata/pbench-queries testdata/pbench.sf1
          testdata/generate_testdata.sh testdata/cost-registry.csv)

send() {  # <label> <rsync source and dest args…>; retried, and resumed from --partial-dir
  local label=$1 attempt=1; shift
  # --delete: the cache mirrors this run, so a golden or binary the run lost is gone there too.
  # -a keeps lib/'s soname symlinks as symlinks and the binaries' exec bits. --checksum, not
  # size and mtime: a rebuilt file of the same size and second would otherwise be skipped, and
  # an unchanged one with a new mtime would be sent. Hashing the tree costs seconds at each end.
  until "$RSYNC" -a --checksum --delete --timeout=120 --partial-dir=.rsync-partial --stats \
          --compress --compress-choice=zstd --compress-level=3 -e "$RSH" "$@" >> "$logs/$label" 2>&1; do
    if [ "$attempt" -ge "$ATTEMPTS" ]; then
      echo "ci-ship: [$label] failed $attempt times" >&2; tail -5 "$logs/$label" >&2; return 1
    fi
    echo "ci-ship: [$label] attempt $attempt failed; retrying in $((attempt * PAUSE))s" >&2
    sleep "$((attempt * PAUSE))"; attempt=$((attempt + 1))
  done
}

stream() {  # <label> <send calls, one string of rsync args each…>; one background stream, timed
  local label=$1 t0=$SECONDS; shift
  local call
  for call in "$@"; do
    # shellcheck disable=SC2086  # each call is a word list built below, with no spaces in it
    send "$label" $call || return 1
  done
  echo "ci-ship: [$label] done in $((SECONDS - t0))s; $(grep -h -E '^Total bytes sent' "$logs/$label" | tr '\n' ' ')"
}

"${vm_ssh[@]}" "mkdir -p ~/$CACHE/cpp/install/lib ~/$CACHE/cpp/install/rust-tests ~/$CACHE/testdata ~/$CACHE/scripts"

dest=nebius-gpu:$CACHE  # the bare host is a placeholder; gpu-host.sh rsh resolves the IP
pids=() labels=()
stream lib "cpp/install/lib/ $dest/cpp/install/lib/" & pids+=($!); labels+=(lib)
stream rust-tests "cpp/install/rust-tests/ $dest/cpp/install/rust-tests/" & pids+=($!); labels+=(rust-tests)
# The rest of the install. Excluded, lib/ and rust-tests/ are also kept from --delete here.
stream install "--exclude=/lib/ --exclude=/rust-tests/ cpp/install/ $dest/cpp/install/" & pids+=($!); labels+=(install)
stream testdata "${TESTDATA[*]} $dest/testdata/" "scripts/gpu-host-test.sh $dest/scripts/" & pids+=($!); labels+=(testdata)
status=0
for i in "${!pids[@]}"; do
  wait "${pids[$i]}" || { echo "::error::ci-ship: the ${labels[$i]} stream failed" >&2; status=1; }
done
[ "$status" -eq 0 ] || exit 1

# cp -a, not hard links: a test that rewrote a shipped file in place would otherwise rewrite the
# cache. A local copy of the tree takes seconds.
"${vm_ssh[@]}" "rm -rf ~/${run_dir:?} && mkdir -p ~/$run_dir && cp -a ~/$CACHE/cpp ~/$CACHE/testdata ~/$CACHE/scripts ~/$run_dir/"
echo "ci-ship: ~/$run_dir ready"
