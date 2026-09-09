#!/bin/bash
#
# Capture the benchmark corpus under Nsight and bring the readings home.
#
# Two passes, because they cannot be one. The TRACE pass records nvtx+cuda and nothing
# else, so its times are the run's times; `nsys_calls.py` reads it down to what each ABI
# call splits into inside libcudf. The HBM pass adds GPU memory counters, which cost the
# query ~7% — its times are unusable and only its TRAFFIC is read, joined onto the clean
# record by the tuple.
#
# Separate from build-test-shadgpu.sh because a profile is a different measurement, not a
# mode of the benchmark run: it takes minutes and gigabytes, it is watched rather than
# gated on, and the tree it would write under counters is a tree of wrong numbers.
#
# The binaries must already be on the host:
#   scripts/docker-build.sh --no-image -- ./scripts/build-test-shadgpu.sh --build-benchmarks
#   ./scripts/build-test-shadgpu.sh --push-binaries --patch
#
# USAGE
#   ./scripts/create_nsys_profile.sh                 # both passes
#   ./scripts/create_nsys_profile.sh --trace         # the clean capture only
#   ./scripts/create_nsys_profile.sh --hbm           # the counters pass only
#   ./scripts/create_nsys_profile.sh --filter bench_tpch_sf40_q6_bp_tp1_single
#
# WRITES (under testdata/calibration/)
#   capture.sqlite      the trace capture's export
#   calls.tsv           what one ABI call splits into, derived from it
#   capture-hbm.sqlite  the counters capture's export
#   records-hbm.tsv     that pass's own record — traffic only, never its microseconds
#   hbm.tsv             the two joined onto records.tsv's coordinates
set -euo pipefail

. "$(dirname "${BASH_SOURCE[0]}")/lib/shadgpu-env.sh"

BENCH_TARGET=peacock_gpu_benchmarks
BENCH_STAGING=cpp/install/rust-benchmarks
# Relative to testdata/ on both sides, so one name drives the remote write and the pull.
CAPTURE_REL=calibration/capture
CALLS_REL=calibration/calls.tsv
HBM_CAPTURE_REL=calibration/capture-hbm
HBM_RECORD_REL=calibration/records-hbm.tsv
HBM_JOINED_REL=calibration/hbm.tsv
# The clean run's record. Read, never written: the join needs the times that pass took,
# and this script's own passes cannot supply them.
RECORD_REL=calibration/records.tsv
# Goldens `nsys_calls.py` reads. sf1 because that is where plan goldens live — a plan's
# recipes are its shape and do not depend on how much data it reads.
PLANS_DIR=testdata/goldens/tpch.sf1

# Part of the measurement, not display: too high a frequency and the device cannot sustain
# the sampling, which nsys_hbm.py refuses by its >100%-of-peak check rather than reporting
# quietly wrong bytes. The SET names the architecture; nsys numbers metrics per set.
HBM_DEVICE=${PCK_BENCH_HBM_DEVICE:-0}
HBM_SET=${PCK_BENCH_HBM_SET:-gh100}
HBM_FREQ=${PCK_BENCH_HBM_FREQ:-20000}

usage() { sed -n '2,${/^#/!q;p;}' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'; exit "${1:-0}"; }

TRACE=0
HBM=0
FILTER=""
while [ $# -gt 0 ]; do
  case "$1" in
    --trace) TRACE=1 ;;
    --hbm) HBM=1 ;;
    --filter) [ $# -ge 2 ] || die "--filter needs a value"; FILTER=$2; shift ;;
    -h|--help) usage 0 ;;
    *) echo "unknown argument: $1" >&2; usage 1 ;;
  esac
  shift
done
# Neither named means both, which is what a full collection wants. No --both flag: a flag
# whose only effect is the default is one more thing to get wrong.
if [ "$TRACE" -eq 0 ] && [ "$HBM" -eq 0 ]; then TRACE=1; HBM=1; fi

# Before the first side effect, and over ssh rather than by letting the run fail on the
# host: a missing binary should say so in a second, not after a push and a launch.
ssh "$REMOTE" test -x "$REMOTE_REPO/$BENCH_STAGING/$BENCH_TARGET" \
  || die "$BENCH_TARGET is not on $REMOTE — build it and --push-binaries --patch first."
[ -d "$PLANS_DIR" ] || die "$PLANS_DIR is missing; nsys_calls.py reads its goldens."

# One pass on the host: capture, export, and say what came of it.
#
# `env` between nsys and the binary, not LD_LIBRARY_PATH in front of nsys: the path
# carries glibc-2.35, and nsys is a host binary that would load it under the host's own
# loader and die. env inherits an untouched environment and sets it for the child alone.
#
# --test-threads=1 is not optional: cuDF/RMM share one process-wide pool and one default
# stream, so concurrent cases would measure each other's contention.
#
# Exported on the machine that captured: `nsys export` needs the same nsys, and only the
# export is small enough to want on the wire. Exported even after a failure — a capture of
# the executions that did happen is the only copy of them.
remote_pass() {                   # remote_pass <label> <capture rel> <record rel|""> <nsys flags…>
  local label=$1 capture_rel=$2 record_rel=$3; shift 3
  local flags="$*"
  ssh "$REMOTE" bash <<EOF
set -uo pipefail
ld=$REMOTE_REPO/cpp/install/lib:/usr/local/cuda-12.5/compat:/home/info/glibc-2.35/lib:\$HOME/miniforge3/envs/rapids-cuda-12.2/lib
export PEACOCK_TESTDATA_DIR=$REMOTE_REPO/testdata
export PEACOCK_TPCH_SF40_DIR=/home/info/peacock-datasets/testdata/tpch.sf40
# The ranges the capture is joined on. Without them the trace has libcudf's calls and no
# way to say which node they belong to.
export PEACOCK_NVTX=1
# The .benchmark.txt tree belongs to the clean run. Either pass would overwrite each
# section with times taken under a profiler — the one number in it that is knowingly wrong.
export PEACOCK_BENCHMARK_RESULTS_RO=1
capture=\$PEACOCK_TESTDATA_DIR/$capture_rel
mkdir -p "\$(dirname "\$capture")"
rm -f "\$capture.nsys-rep" "\$capture.sqlite"
# A record only where something joins one. The trace pass writes none: nsys_calls.py reads
# the capture against plan goldens, and a record from a traced run would be one more file
# claiming to be the measurement while carrying the profiler's overhead.
if [ -n "$record_rel" ]; then
  export PEACOCK_RECORD_PATH=\$PEACOCK_TESTDATA_DIR/$record_rel
  mkdir -p "\$(dirname "\$PEACOCK_RECORD_PATH")"
  # Removed rather than appended to: record.rs writes the header only into a fresh file,
  # so a leftover would swallow this run's rows under an earlier run's heading.
  rm -f "\$PEACOCK_RECORD_PATH"
fi

echo "==> $label pass to \$capture.nsys-rep"
log=/tmp/$BENCH_TARGET.$label.log
nsys profile $flags --force-overwrite=true -o "\$capture" \\
  env LD_LIBRARY_PATH="\$ld:\${LD_LIBRARY_PATH:-}" \\
  $REMOTE_REPO/$BENCH_STAGING/$BENCH_TARGET --nocapture --test-threads=1 $FILTER 2>&1 | tee "\$log"
status=\${PIPESTATUS[0]}

nsys export --type=sqlite --force-overwrite=true -o "\$capture.sqlite" "\$capture.nsys-rep" || true
[ -n "$record_rel" ] && echo "==> rows: \$(grep -vc '^#' "\$PEACOCK_RECORD_PATH" 2>/dev/null || echo 0)"
[ "\$status" -eq 0 ] || { echo "!!! the $label pass FAILED (exit \$status)"; exit "\$status"; }

# libtest's own count is the only honest answer to "did the filter match anything";
# counting files that appeared is a different question.
ran=\$(sed -n 's/^test result:.* \([0-9][0-9]*\) passed.*/\1/p' "\$log" | awk '{n += \$1} END {print n + 0}')
[ "\$ran" -gt 0 ] || { echo "!!! the $label pass ran no tests (filter matched nothing?)"; exit 1; }
echo "==> the $label pass ran \$ran tests"
EOF
}

pull_one() {                      # pull_one <relative path> <what it is>
  local rel=$1 what=$2
  # Tested over ssh rather than by letting the transfer fail: resilient_rsync retries a
  # missing source a hundred times, and eight minutes of backoff is not how "there is
  # nothing there" should read.
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

# --sample=none: the CPU profiler's SIGPROF interrupts the very host spans the ranges
# measure. --cpuctxsw=none for the same reason.
if [ "$TRACE" -eq 1 ]; then
  remote_pass trace "$CAPTURE_REL" "" \
    --trace=nvtx,cuda --sample=none --cpuctxsw=none
  pull_one "$CAPTURE_REL.sqlite" "the trace capture"
  # Derived here rather than on the host: the reader is a local script over local goldens,
  # it costs seconds, and the capture comes home anyway.
  python3 scripts/calibration/nsys_calls.py \
    --capture "testdata/$CAPTURE_REL.sqlite" --plans-dir "$PLANS_DIR" \
    --out "testdata/$CALLS_REL"
  echo "==> the call breakdown: $(grep -vc '^#' "testdata/$CALLS_REL") rows"
fi

if [ "$HBM" -eq 1 ]; then
  remote_pass hbm "$HBM_CAPTURE_REL" "$HBM_RECORD_REL" \
    --trace=nvtx,cuda --sample=none --cpuctxsw=none \
    --gpu-metrics-device="$HBM_DEVICE" --gpu-metrics-set="$HBM_SET" \
    --gpu-metrics-frequency="$HBM_FREQ"
  pull_one "$HBM_CAPTURE_REL.sqlite" "the HBM capture"
  pull_one "$HBM_RECORD_REL" "the HBM pass's record"
  # The join, and the step that had no caller before this script: hbm.tsv was produced by
  # hand once and then rode along, three days older than the capture beside it, while
  # plot.py drew a panel from it. A derived file with no producer goes stale in silence.
  python3 scripts/calibration/nsys_hbm.py \
    --capture "testdata/$HBM_CAPTURE_REL.sqlite" \
    --record "testdata/$HBM_RECORD_REL" \
    --out "testdata/$HBM_JOINED_REL"
  echo "==> HBM traffic: $(grep -vc '^#' "testdata/$HBM_JOINED_REL") rows"
fi

echo "==> redraw with:"
echo "    /usr/bin/python3 scripts/calibration/plot.py \\"
echo "        --record testdata/$RECORD_REL --hbm testdata/$HBM_JOINED_REL \\"
echo "        --out-dir testdata/calibration/plots"
