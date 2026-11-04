#!/bin/bash
#
# Capture the benchmark corpus under Nsight and bring the readings home. Two passes,
# because they cannot be one: the trace pass records nvtx+cuda alone, so its times are the
# run's and nsys_calls.py can read it down to what an ABI call splits into inside libcudf;
# the metrics pass adds GPU memory counters, which cost the query ~7%, so only its traffic
# is read, onto the clean run's coordinates. Separate from build-test-shadgpu.sh because a
# profile is a different measurement rather than a mode of the benchmark run, and a
# captured run publishes no tree.
#
# USAGE (the binaries must already be on the host, and the pinned records fetched)
#   ./scripts/build-test-shadgpu.sh --build-benchmarks
#   ./scripts/build-test-shadgpu.sh --push-binaries --patch --run-benchmarks --pull-benchmarks
#   scripts/calibration/fetch_record.sh tpch.sf40     # and every other dataset it timed
#
#   ./scripts/create_nsys_profile.sh                 # both passes
#   ./scripts/create_nsys_profile.sh --trace         # the clean capture only
#   ./scripts/create_nsys_profile.sh --metrics       # the counters pass only
#   PCK_TEST_FILTER=bench_tpch_sf40_q6_tp1_single ./scripts/create_nsys_profile.sh --trace
# The counters pass covers the whole corpus: its output is merged and checked against every
# pinned record, so a filter narrows the trace pass only and is refused with --metrics.
#
# The host defaults to shad-gpu (lib/shadgpu-env.sh: its repo, its patched glibc, its
# 25.02 cuDF). Another host — one with a modern glibc and its own cuDF, provisioned by
# build-test.sh — names all three, since the patched loader path is shad-gpu's alone:
#   ./scripts/create_nsys_profile.sh --host verda-gpu --remote-dir /home/dmitry/peacockdb \
#       --remote-cudf-root /home/dmitry/miniforge3/envs/rapids-26.02
# The HBM defaults below are an H200's; another card overrides PCK_BENCH_HBM_SET and
# PCK_BENCH_HBM_PEAK_BW, and nsys and python3 must be on that host's PATH.
#
# WRITES (under testdata/calibration/)
#   capture.sqlite           the trace capture's export
#   calls.tsv                what one ABI call splits into, derived from it
#   hbm.tsv                  the counters pass's traffic, a row per call of the pinned records
# The counters pass is one capture per case, each joined on the host and deleted before the
# next: sampling the whole set at the default frequency writes ~27 MB of export a second of
# run, so the corpus at once would be ~100 GB, and a case at a time is at most the longest
# case. Only hbm.tsv comes home. The metrics pass's own record stays on the host: it is a record
# with the profiler's microseconds in it, and nothing reads a time from it.
set -euo pipefail

. "$(dirname "${BASH_SOURCE[0]}")/lib/shadgpu-env.sh"

BENCH_TARGET=peacock_gpu_benchmarks
BENCH_STAGING=cpp/install/rust-benchmarks
# Relative to testdata/ on both sides, so one name drives the remote write and the pull.
CAPTURE_REL=calibration/capture
CALLS_REL=calibration/calls.tsv
METRICS_CAPTURE_REL=calibration/capture-metrics
# That pass's own record, written on the host and left there: see the header.
METRICS_RECORD_REL=calibration/records-metrics.tsv
HBM_REL=calibration/hbm.tsv
# The clean run's records, as fetch_record.sh puts them: `<dataset>.sf<sf>/` under this.
# Read, never written: the join's coordinates and every time in its output are that run's,
# and this script's passes cannot supply them.
RECORDS_REL=calibration
# Goldens nsys_calls.py checks the capture against, each case in `<dataset>.sf1/` under this.
# sf1 because that is where plan goldens live — a plan's recipes are its shape and do not
# depend on how much data it reads.
PLANS_DIR=testdata/goldens

# Part of the measurement, not display. Too high a sampling frequency and the device cannot
# sustain it, which nsys_hbm.py refuses by its >100%-of-peak check rather than reporting
# quietly wrong bytes; the peak is what turns a percentage into bytes, so a wrong one is a
# silent scale error on every row. The set names the architecture; nsys numbers metrics per set.
HBM_DEVICE=${PCK_BENCH_HBM_DEVICE:-0}
HBM_SET=${PCK_BENCH_HBM_SET:-gh100}
HBM_FREQ=${PCK_BENCH_HBM_FREQ:-20000}
HBM_PEAK_BW=${PCK_BENCH_HBM_PEAK_BW:-4.8e12}

usage() { sed -n '2,${/^#/!q;p;}' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'; exit "${1:-0}"; }

TRACE=0
METRICS=0
REMOTE_CUDF_ROOT=""
HOST_OVERRIDDEN=0
need_value() { [ -n "${2:-}" ] || die "$1 requires a value"; }
while [ $# -gt 0 ]; do
  case "$1" in
    --trace) TRACE=1 ;;
    --metrics) METRICS=1 ;;
    --host)             need_value "$1" "${2:-}"; REMOTE="$2"; HOST_OVERRIDDEN=1; shift ;;
    --remote-dir)       need_value "$1" "${2:-}"; REMOTE_REPO="$2"; HOST_OVERRIDDEN=1; shift ;;
    --remote-cudf-root) need_value "$1" "${2:-}"; REMOTE_CUDF_ROOT="$2"; shift ;;
    -h|--help) usage 0 ;;
    *) echo "unknown argument: $1" >&2; usage 1 ;;
  esac
  shift
done

# The loader path the binary runs under. shad-gpu's is PATCHED_LD: the patched glibc, the
# CUDA compat libs and the 25.02 env, with the repo path baked in when the lib was sourced.
# Another host has none of that, so naming it without its cuDF root would run the binary
# under shad-gpu's paths and fail three directories deep instead of here.
if [ -n "$REMOTE_CUDF_ROOT" ]; then
  BENCH_LD="$REMOTE_REPO/cpp/install/lib:$REMOTE_CUDF_ROOT/lib:\${LD_LIBRARY_PATH:-}"
elif [ "$HOST_OVERRIDDEN" -eq 1 ]; then
  die "--host/--remote-dir without --remote-cudf-root: the default loader path is shad-gpu's
     patched glibc and 25.02 env. Name the host's cuDF root (e.g. .../envs/rapids-26.02)."
else
  BENCH_LD="$PATCHED_LD"
fi
# Nothing named means both passes, which is what a full collection wants. No --both flag: a
# flag whose only effect is the default is one more thing to get wrong.
if [ "$TRACE" -eq 0 ] && [ "$METRICS" -eq 0 ]; then TRACE=1; METRICS=1; fi

# The same filter name the benchmark run takes, so one spelling narrows both. Human-typed
# and reaching the remote script as a single-quoted literal, so quoted for the shell rather
# than assumed to contain no apostrophe.
: "${PCK_TEST_FILTER:=}"
filter_q=$(printf '%q' "$PCK_TEST_FILTER")

# Before the first side effect: a missing input should say so in a second, not after a
# capture and an export. The local checks first, then the one question for the host.
datasets=$(bench_datasets) \
  || die "cannot read the datasets from $BENCH_CASES (the reason is above)"
for name in $datasets; do
  [ -d "$PLANS_DIR/${name%%.sf*}.sf1" ] \
    || die "$PLANS_DIR/${name%%.sf*}.sf1 is missing; nsys_calls.py checks the $name cases against it."
done
# The counters pass's cases, a test name and its case range each, narrowed by the filter.
cases=$(bench_cases) || die "cannot read the cases from $BENCH_CASES (the reason is above)"
# A list of `none` lines alone parses and times nothing: one empty line, no case.
[ -n "$cases" ] || die "$BENCH_CASES declares no timed case"
metric_tests=() metric_cases=()
while read -r dataset sf query mode; do
  bench=bench_${dataset}_sf${sf}_${query}_${mode}
  case "$bench" in *"$PCK_TEST_FILTER"*) ;; *) continue ;; esac
  metric_tests+=("$bench")
  metric_cases+=("$dataset.sf$sf ${query//_/-} ${mode//_/-}")
done <<< "$cases"
all_cases=$(grep -c . <<< "$cases")
if [ "$METRICS" -eq 1 ] && [ ${#metric_tests[@]} -ne "$all_cases" ]; then
  die "--metrics with PCK_TEST_FILTER=$PCK_TEST_FILTER selects ${#metric_tests[@]} of the
     $all_cases cases, and the counters pass's merge is checked against every one of them.
     Narrow the trace pass only: --trace."
fi
record_args=()
for name in $datasets; do
  record=testdata/$RECORDS_REL/$name/records.tsv
  record_args+=(--record "$record")
  [ "$METRICS" -eq 0 ] || [ -f "$record" ] \
    || die "$record is missing, and the counters pass has no times of its own to
     join onto. Fetch it: scripts/calibration/fetch_record.sh $name"
done
ssh "$REMOTE" test -x "$REMOTE_REPO/$BENCH_STAGING/$BENCH_TARGET" \
  || die "$BENCH_TARGET is not on $REMOTE — build it and --push-binaries --patch first."

# One pass on the host: capture, export, and say what came of it.
#
# `env` between nsys and the binary, never LD_LIBRARY_PATH in front of it: the path carries
# the patched glibc, and nsys is a host binary that would load it under the host's own
# loader and die. --test-threads=1 is not optional — cuDF/RMM share one pool and one
# default stream.
#
# Exported on the machine that captured, since `nsys export` needs the same nsys, and even
# after a failure: a capture of the executions that did happen is the only copy of them.
# <filter> is the test binary's own: a substring, or a name and --exact.
remote_pass() {    # remote_pass <label> <capture rel> <record rel|""> <filter> <nsys flags…>
  local label=$1 capture_rel=$2 record_rel=$3 filter=$4; shift 4
  local flags="$*"
  ssh "$REMOTE" bash <<EOF
set -uo pipefail
# The harness finds each dataset as <dataset>.sf<sf> under this, so the host's symlinks
# (build-test-shadgpu.sh's run makes them) are the only dataset configuration there is.
export PEACOCK_TESTDATA_DIR=$REMOTE_REPO/testdata
# The one variable that turns the harness's NVTX ranges on. It also stamps the record's
# heading and stops the run publishing a .benchmark.txt: the tree belongs to the clean run,
# and either pass would overwrite each section with times taken under a profiler.
export PEACOCK_BENCHMARK_CAPTURE=$label
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
  env LD_LIBRARY_PATH="$BENCH_LD" \\
  $REMOTE_REPO/$BENCH_STAGING/$BENCH_TARGET --nocapture --test-threads=1 $filter 2>&1 | tee "\$log"
status=\${PIPESTATUS[0]}

if ! nsys export --type=sqlite --force-overwrite=true -o "\$capture.sqlite" "\$capture.nsys-rep"; then
  echo "!!! nsys export of \$capture.nsys-rep failed (the $label pass exited \$status)"
  exit 1
fi
[ -n "$record_rel" ] && echo "==> rows: \$(grep -vc '^#' "\$PEACOCK_RECORD_PATH" 2>/dev/null || echo 0)"
[ "\$status" -eq 0 ] || { echo "!!! the $label pass FAILED (exit \$status)"; exit "\$status"; }

$(declare -f passed_count)
ran=\$(passed_count "\$log")
[ "\$ran" -gt 0 ] || { echo "!!! the $label pass ran no tests (filter matched nothing?)"; exit 1; }
echo "==> the $label pass ran \$ran tests"
EOF
}

# --sample=none: the CPU profiler's SIGPROF interrupts the very host spans the ranges
# measure. --cpuctxsw=none for the same reason.
if [ "$TRACE" -eq 1 ]; then
  remote_pass trace "$CAPTURE_REL" "" "$filter_q" \
    --trace=nvtx,cuda --sample=none --cpuctxsw=none
  pull_one "$CAPTURE_REL.sqlite" "the trace capture" \
    || die "the trace pass left no capture on $REMOTE; its log is /tmp/$BENCH_TARGET.trace.log there."
  # Derived here rather than on the host: the reader is a local script over local goldens,
  # it costs seconds, and the capture comes home anyway.
  python3 scripts/calibration/nsys_calls.py \
    --capture "testdata/$CAPTURE_REL.sqlite" --plans-dir "$PLANS_DIR" \
    --out "testdata/$CALLS_REL"
  echo "==> the call breakdown: $(grep -vc '^#' "testdata/$CALLS_REL") rows"
fi

if [ "$METRICS" -eq 1 ]; then
  # The reader and the pinned records go to the host, where each case is joined.
  parts_rel=calibration/hbm-parts
  dirs="$REMOTE_REPO/scripts/calibration $REMOTE_REPO/testdata/$parts_rel"
  for name in $datasets; do dirs="$dirs $REMOTE_REPO/testdata/$RECORDS_REL/$name"; done
  ssh "$REMOTE" "rm -rf $REMOTE_REPO/testdata/$parts_rel && mkdir -p $dirs"
  resilient_rsync -a scripts/calibration/{nsys_hbm,nvtx_names,record}.py \
    "$REMOTE:$REMOTE_REPO/scripts/calibration/"
  for name in $datasets; do
    resilient_rsync -a "testdata/$RECORDS_REL/$name/records.tsv" \
      "$REMOTE:$REMOTE_REPO/testdata/$RECORDS_REL/$name/"
  done
  reader="cd $REMOTE_REPO && python3 scripts/calibration/nsys_hbm.py $(printf '%q ' "${record_args[@]}")"
  part_args=()
  for i in "${!metric_tests[@]}"; do
    bench=${metric_tests[$i]}
    echo "==> counters, case $((i + 1)) of ${#metric_tests[@]}: ${metric_cases[$i]}"
    remote_pass metrics "$METRICS_CAPTURE_REL" "$METRICS_RECORD_REL" "$bench --exact" \
      --trace=nvtx,cuda --sample=none --cpuctxsw=none \
      --gpu-metrics-device="$HBM_DEVICE" --gpu-metrics-set="$HBM_SET" \
      --gpu-metrics-frequency="$HBM_FREQ"
    # Joined, then deleted, before the next case: the disk holds one case's capture at most.
    ssh "$REMOTE" "$reader --capture testdata/$METRICS_CAPTURE_REL.sqlite \
      --case $(printf '%q' "${metric_cases[$i]}") --peak-bw $HBM_PEAK_BW \
      --out testdata/$parts_rel/$bench.tsv \
      && rm testdata/$METRICS_CAPTURE_REL.sqlite testdata/$METRICS_CAPTURE_REL.nsys-rep" \
      || die "joining the counters capture of $bench on $REMOTE failed (above); it is still there."
    part_args+=(--parts "testdata/$parts_rel/$bench.tsv")
  done
  # The merge is where the parts are held to the whole record, so a filtered pass stops here.
  ssh "$REMOTE" "$reader $(printf '%q ' "${part_args[@]}") --out testdata/$HBM_REL" \
    || die "merging the counters parts on $REMOTE failed (above)."
  pull_one "$HBM_REL" "HBM traffic" || die "bringing testdata/$HBM_REL home from $REMOTE failed."
fi

echo "==> redraw with:"
echo "    /usr/bin/python3 scripts/calibration/plot.py \\"
printf '        %s %s \\\n' "${record_args[@]}"
echo "        --hbm testdata/$HBM_REL \\"
echo "        --out-dir testdata/calibration/plots"
