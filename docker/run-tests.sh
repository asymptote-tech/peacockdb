#!/usr/bin/env bash
# The device run in the run image (llm-wiki/tasks/containers.md): a cpp/install with rust-tests/
# (rust-benchmarks/ for bench) read-only at /peacock/install, the data read-write at
# /peacock/testdata. Only the binaries get the library path: exported, the prefix's libtinfo
# would load into bash itself.
#
#   run-tests.sh list      the L40S list, by scripts/gpu-host-test.sh's rules
#   run-tests.sh filter    the list, PCK_TEST_FILTER narrowing every rust binary but the lib
#   run-tests.sh bench     rust-benchmarks/peacock_gpu_benchmarks; PCK_TEST_FILTER narrows it
#
# Exit 0 green; 1 a binary failed, ran no test or segfaulted; 2 a bad argument or input, found first.
set -euo pipefail

INSTALL=/peacock/install
TESTDATA=/peacock/testdata
LIBS=$INSTALL/lib:/opt/cudf/lib
SF40=" peacock_tpch_tests peacock_tpchv_tests "
# The lib binary holds every rung of a gpu build; the path filter selects the device rung.
LIB_STAGED=peacockdb_core_gpu_lib
LIB_RUNG=gpu_tests::
BENCH_TARGET=peacock_gpu_benchmarks

usage() { sed -n '2,${/^#/!q;p;}' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'; exit 0; }
die() { echo "run-tests.sh: $*" >&2; exit 2; }
rc=0
fail() { echo "run-tests.sh: $*" >&2; rc=1; }

modes=()
for arg in "$@"; do
  case "$arg" in
    -h|--help) usage ;;
    list|filter|bench) modes+=("$arg") ;;
    *) die "unknown argument $arg: list, filter or bench" ;;
  esac
done
[ ${#modes[@]} -eq 1 ] || die "want one of list, filter or bench, got: ${modes[*]:-nothing}"
mode=${modes[0]}
filter=${PCK_TEST_FILTER:-}
case "$mode" in
  list) [ -z "$filter" ] || die "list runs the whole list, but PCK_TEST_FILTER=$filter is set: use filter" ;;
  filter) [ -n "$filter" ] || die "filter needs PCK_TEST_FILTER" ;;
esac
[ -f "$INSTALL/lib/libpeacock_gpu.so" ] || die "no $INSTALL/lib/libpeacock_gpu.so: mount a cpp/install at $INSTALL"
[ -d "$TESTDATA" ] && [ -w "$TESTDATA" ] || die "$TESTDATA is not a writable directory: mount testdata read-write there"

logs=$(mktemp -d)

# Sets `status` to the binary's exit code; its output goes to the terminal and to <log>.
run_binary() {  # <log> [VAR=value...] <binary> <args...>
  local log=$1
  shift
  # Not -e for the pipeline: a failing binary is a result to judge, and every binary runs.
  set +e
  env LD_LIBRARY_PATH="$LIBS" PEACOCK_TESTDATA_DIR="$TESTDATA" "$@" 2>&1 | tee "$log"
  status=${PIPESTATUS[0]}
  set -e
}

# A non-zero exit, a signal or a segfault in the log fails the run.
judge() {  # <name> <log> <exit status>
  local name=$1 log=$2 code=$3 signal
  if [ "$code" -gt 128 ] && signal=$(kill -l $((code - 128)) 2>/dev/null); then
    fail "$name was killed by signal $((code - 128)) (SIG$signal)"
  elif [ "$code" -ne 0 ]; then
    fail "$name exited $code"
  fi
  if grep -qE 'Segmentation fault|core dumped' "$log"; then fail "$name: a segfault in its log"; fi
}

# Zero tests fail a binary that was not narrowed by a filter: one whose tests all skip exits 0.
require_tests() {  # <name> <log> <the zero-tests line, as a regex>
  if grep -qE "$3" "$2"; then fail "$1 ran 0 tests"; fi
}

passed() {  # <log>: the "N passed" libtest printed, summed
  sed -n 's/^test result:.* \([0-9][0-9]*\) passed.*/\1/p' "$1" | awk '{ n += $1 } END { print n + 0 }'
}

run_list() {
  local cpp=() rust=() t name args narrowed=() matched=0
  for t in "$INSTALL"/bin/peacock_*_tests; do if [ -x "$t" ]; then cpp+=("$t"); fi; done
  for t in "$INSTALL"/rust-tests/*; do if [ -x "$t" ]; then rust+=("$t"); fi; done
  [ ${#cpp[@]} -gt 0 ] || die "no $INSTALL/bin/peacock_*_tests: the C++ suites are not in the install"
  [ ${#rust[@]} -gt 0 ] || die "no binaries in $INSTALL/rust-tests: stage the gpu test binaries there"
  if [ -n "$filter" ] && [ ${#rust[@]} -eq 1 ] && [ "${rust[0]##*/}" = "$LIB_STAGED" ]; then
    die "filter narrows every rust binary but $LIB_STAGED, and $INSTALL/rust-tests holds only that one"
  fi

  for t in "${cpp[@]}"; do
    name=${t##*/}
    case $SF40 in *" $name "*) echo "--- $name skipped (sf40)"; continue ;; esac
    echo "==> $name"
    run_binary "$logs/$name.log" "$t"
    judge "$name" "$logs/$name.log" "$status"
    require_tests "$name" "$logs/$name.log" '\[  PASSED  \] 0 tests'
  done
  for t in "${rust[@]}"; do
    name=${t##*/}
    # --test-threads=1: cuDF/RMM share one process-wide pool; concurrent GPU tests OOM the device
    # and poison the CUDA context for every later test.
    args=(--nocapture --test-threads=1)
    if [ "$name" = "$LIB_STAGED" ]; then args+=("$LIB_RUNG"); fi
    # The benchmark binary's bench_ cases are the measurement, which bench runs.
    if [ "$name" = "$BENCH_TARGET" ]; then args+=(--skip bench_); fi
    if [ -n "$filter" ] && [ "$name" != "$LIB_STAGED" ]; then args+=("$filter"); narrowed+=("$name"); fi
    echo "==> $name ${args[*]:2}"
    run_binary "$logs/$name.log" "$t" "${args[@]}"
    judge "$name" "$logs/$name.log" "$status"
    if [ -n "$filter" ] && [ "$name" != "$LIB_STAGED" ]; then
      matched=$((matched + $(passed "$logs/$name.log")))
    else
      require_tests "$name" "$logs/$name.log" '^running 0 tests'
    fi
  done
  if [ ${#narrowed[@]} -gt 0 ] && [ "$matched" -eq 0 ]; then
    fail "PCK_TEST_FILTER=$filter matched no test in ${narrowed[*]}"
  fi
}

run_bench() {
  local bin=$INSTALL/rust-benchmarks/$BENCH_TARGET results=$TESTDATA/benchmark-results
  local record=$TESTDATA/calibration/records.tsv stamp args=(--nocapture --test-threads=1)
  [ -x "$bin" ] || die "no $bin: stage the release build of $BENCH_TARGET at rust-benchmarks/"
  if [ -n "$filter" ]; then args+=("$filter"); fi
  # record.rs writes its header only into a fresh file, so the record starts empty; one with rows
  # is an earlier run's, a segment's or a rerun's, and is not this run's to drop.
  [ ! -s "$record" ] || die "$record holds an earlier run's rows: pull it home, then remove it"
  mkdir -p "$results" "${record%/*}"
  rm -f "$record"
  stamp=$(mktemp)
  echo "==> $BENCH_TARGET ${args[*]:2}"
  run_binary "$logs/$BENCH_TARGET.log" PEACOCK_RECORD_PATH="$record" \
    PEACOCK_TPCH_VEC_PARAMS="$TESTDATA/tpch-vec-queries/query_params.jsonl" "$bin" "${args[@]}"
  judge "$BENCH_TARGET" "$logs/$BENCH_TARGET.log" "$status"
  require_tests "$BENCH_TARGET" "$logs/$BENCH_TARGET.log" '^running 0 tests'
  if [ "$(passed "$logs/$BENCH_TARGET.log")" -eq 0 ]; then fail "$BENCH_TARGET passed no test"; fi
  echo "==> benchmark files written by this run: $(find "$results" -name '*.benchmark.txt' -newer "$stamp" | wc -l)"
  echo "==> calibration rows: $(grep -vc '^#' "$record" 2>/dev/null || true)"
}

case "$mode" in
  list|filter) run_list ;;
  bench) run_bench ;;
esac
if [ "$rc" -eq 0 ]; then echo "==> run-tests.sh $mode: green"; else echo "==> run-tests.sh $mode: FAILED" >&2; fi
exit "$rc"
