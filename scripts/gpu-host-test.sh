#!/usr/bin/env bash
# The L40S test list, run on nebius-gpu against a tree holding cpp/install/ (llm-wiki/build-test.md,
# "GPU host"). One list for the developer and for CI.
#
#   gpu-host-test.sh run [--foreground] <tree>   detached by default; logs and rc files in <tree>/.gpu-test/
#   gpu-host-test.sh status <tree>               which binaries are done, running or failed
#
# Out of the list: the sf40 pair (peacock_tpch_tests, peacock_tpchv_tests) and every bench_ case.
set -uo pipefail

SF40=" peacock_tpch_tests peacock_tpchv_tests "
CUDF_LIB=${GPU_HOST_CUDF_LIB:-$HOME/data/miniforge3/envs/rapids-cuda-12.2/lib}

run_all() {  # <tree>; every binary runs even after a failure; rc is the OR of all of them
  local tree=$1 st=$1/.gpu-test rc=0 ran_c=0 ran_r=0 t name trc rung skip filter ld
  mkdir -p "$st"; rm -f "$st"/*.rc
  # Not exported: the conda lib dir can shadow system libs and crash grep/cat/tail. Only the
  # test binaries get it, through env.
  ld=$tree/cpp/install/lib:$CUDF_LIB
  for t in "$tree"/cpp/install/bin/peacock_*_tests; do
    [ -x "$t" ] || continue; name=${t##*/}
    case $SF40 in *" $name "*) echo "--- $name skipped (sf40)"; continue;; esac
    echo "==> $name"; env LD_LIBRARY_PATH="$ld" PEACOCK_TESTDATA_DIR="$tree/testdata" "$t" > "$st/$name.log" 2>&1; trc=$?
    cat "$st/$name.log"
    if grep -q '\[  PASSED  \] 0 tests' "$st/$name.log"; then echo "::error::$name ran 0 tests"; trc=1; fi
    echo "$trc" > "$st/$name.rc"; [ "$trc" -eq 0 ] || rc=1; ran_c=$((ran_c + 1))
  done
  [ "$ran_c" -gt 0 ] || { echo "::error::no peacock_*_tests binaries in $tree/cpp/install/bin"; rc=1; }
  for t in "$tree"/cpp/install/rust-tests/*; do
    [ -x "$t" ] || continue; name=${t##*/}
    # Written out on one command line, not gathered in an array: the CI coverage guard
    # (cost-report's test_source_formatting, runners.rs) reads the rung, the bench_ skip and --test-threads=1 off these lines.
    # The lib holds every rung; the device one is selected by path.
    rung=""; skip=""; filter=()
    [ "$name" = peacockdb_core_gpu_lib ] && rung=gpu_tests::
    # The measurement binary's bench_ cases are the measurement, tens of minutes on a device.
    case $name in peacock_gpu_benchmarks) skip="--skip bench_" ;; esac
    [ -n "${PCK_TEST_FILTER:-}" ] && [ "$name" != peacockdb_core_gpu_lib ] && filter=("$PCK_TEST_FILTER")
    echo "==> $name $rung$skip"
    # --test-threads=1: cuDF/RMM share one process-wide pool; concurrent GPU tests OOM the
    # device and poison the CUDA context for every later test. Sequential is mandatory.
    # shellcheck disable=SC2086  # $rung and $skip are empty or whole words, split on purpose
    env LD_LIBRARY_PATH="$ld" PEACOCK_TESTDATA_DIR="$tree/testdata" "$t" --nocapture --test-threads=1 $rung $skip "${filter[@]}" > "$st/$name.log" 2>&1; trc=$?
    cat "$st/$name.log"
    if grep -q '^running 0 tests' "$st/$name.log"; then echo "::error::$name ran 0 tests"; trc=1; fi
    echo "$trc" > "$st/$name.rc"; [ "$trc" -eq 0 ] || rc=1; ran_r=$((ran_r + 1))
  done
  [ "$ran_r" -gt 0 ] || { echo "::error::no rust test binaries in $tree/cpp/install/rust-tests"; rc=1; }
  if grep -qsE 'Segmentation fault|core dumped' "$st"/*.log; then echo "::error::a segfault on the GPU host"; rc=1; fi
  echo "$rc" > "$st/ALL.rc"; return "$rc"
}

status() {  # <tree>
  local st=$1/.gpu-test f
  [ -d "$st" ] || { echo "no run in $1"; return 1; }
  for f in "$st"/*.log; do
    [ -f "$f" ] || continue
    local n; n=$(basename "$f" .log)
    if [ -f "$st/$n.rc" ]; then echo "$n rc=$(cat "$st/$n.rc")"; else echo "$n running: $(tail -1 "$f")"; fi
  done
  if [ -f "$st/ALL.rc" ]; then echo "ALL rc=$(cat "$st/ALL.rc")"; return 0; fi
  local pid boot
  read -r pid boot < "$st/run.pid" 2>/dev/null || true
  if [ -n "${pid:-}" ] && [ "$boot" = "$(cat /proc/sys/kernel/random/boot_id)" ] && kill -0 "$pid" 2>/dev/null; then
    echo "ALL running"
  elif [ -n "${pid:-}" ]; then
    echo "ALL died (pid $pid is gone or the VM rebooted; no ALL.rc)"; return 1
  else
    echo "ALL running"
  fi
}

case ${1:-} in
  run)
    shift; fg=0; [ "${1:-}" = --foreground ] && { fg=1; shift; }
    tree=$(cd "${1:?tree}" && pwd)
    if [ "$fg" -eq 1 ]; then run_all "$tree"; exit $?; fi
    mkdir -p "$tree/.gpu-test"; rm -f "$tree/.gpu-test/ALL.rc"
    nohup "$0" run --foreground "$tree" > "$tree/.gpu-test/run.out" 2>&1 < /dev/null &
    # pid and boot id: a preemption or reboot kills the run without an ALL.rc, and status must say so.
    echo "$! $(cat /proc/sys/kernel/random/boot_id)" > "$tree/.gpu-test/run.pid"
    echo "detached: pid $! — $0 status $tree";;
  status) status "$(cd "${2:?tree}" && pwd)";;
  *) sed -n '2,8p' "$0" >&2; exit 64;;
esac
