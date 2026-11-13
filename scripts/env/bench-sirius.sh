#!/usr/bin/env bash
# Sirius on the H100, by flags (sirius-bench in llm-wiki/archive/archived-tasks.md): per dataset,
# run-sirius.sh's run over a device list's queries, then run-gpu.sh's pull, then the answers
# rendered at home.
#
#   bench-sirius.sh --instance-id <id> [--profile <name>] --dataset tpch|tpcds|all [--sf <n>]
#                   [--cases <cases.csv>] [--query <q>]… [--runs <n>] [--timeout <s>] [--resume]
#                   [--dry-run]
#
# The queries are every row of --cases (testdata/benchmark-results/h100-80gb-hbm3/cases.csv by
# default) at the dataset and --sf (40), `skipped` rows too: their notes are peacockdb's refusals,
# not Sirius's. --query, an ident or a stem, narrows them, in the list's order; each reaches the
# runner as its stem (aggregate-groupby). A narrowed run also gives the runner the dataset's whole
# list as --list, so it replaces its own rows and answers alone and keeps the rest; it refuses a
# sirius.tsv of another Sirius commit or host, and a kept timed row whose answer the database
# lacks. Per dataset, tpch before tpcds:
#   run-sirius.sh … run -- --dataset <ds> --sf <sf> --queries <stems> --runs <n> --timeout <s>
#     [--list <every stem>] [--resume]
#   run-gpu.sh … pull
#   scripts/build-test.sh -- python3 testdata/duckdb_result.py --dataset <ds> --sf <sf>
#     --answers testdata/calibration/sirius/<ds>.sf<sf>/answers.duckdb
#     --out testdata/goldens/<ds>.sf<sf>/sirius-result.txt
# The render runs in the build image, whose DuckDB is the 1.5.4 the oracle is pinned at. --runs is
# 10 by default (test_support's MEASURED_RUNS), --timeout 900 s a query, the pin excluded. --resume
# keeps the rows this Sirius commit and host already wrote. --dry-run prints the commands and runs
# none. --instance-id and --profile go to every run-sirius.sh and run-gpu.sh call; the VM is the
# caller's to start and stop, and `run-gpu.sh … prepare` then `run-sirius.sh … prepare` ready it.
#
# Exit 0; 2 bad input, refused before any call; else the exit of the run-sirius.sh or run-gpu.sh
# call that stopped the run (1 the run failed, 2 the runner refused, 3 and 4 theirs, 75 and 69 the
# VM's).
set -euo pipefail
shopt -s inherit_errexit

usage() { sed -n '2,${/^#/!q;p;}' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'; exit 0; }
die() { echo "bench-sirius.sh: $*" >&2; exit 2; }

here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(git -C "$here" rev-parse --show-toplevel)
RUN_SIRIUS=("${RUN_SIRIUS_SH:-$here/run-sirius.sh}")
RUN_GPU=("${RUN_GPU_SH:-$here/run-gpu.sh}")
BUILD_TEST=${BUILD_TEST_SH:-$root/scripts/build-test.sh}
instance="" profile="" dataset="" sf="" cases="" runs="" timeout="" resume=0 dry_run=0 queries=()

# value <flag> <its value> <the value so far>: refuses a flag without a value, or given twice.
value() {
  [ -n "$2" ] || die "$1 needs a value"
  [ -z "$3" ] || die "$1 is given twice: $3 and $2"
}
while [ $# -gt 0 ]; do
  case $1 in
    -h|--help) usage ;;
    --instance-id) value "$1" "${2:-}" "$instance"; instance=$2; shift 2 ;;
    --profile) value "$1" "${2:-}" "$profile"; profile=$2; shift 2 ;;
    --dataset) value "$1" "${2:-}" "$dataset"; dataset=$2; shift 2 ;;
    --sf) value "$1" "${2:-}" "$sf"; sf=$2; shift 2 ;;
    --cases) value "$1" "${2:-}" "$cases"; cases=$2; shift 2 ;;
    --runs) value "$1" "${2:-}" "$runs"; runs=$2; shift 2 ;;
    --timeout) value "$1" "${2:-}" "$timeout"; timeout=$2; shift 2 ;;
    --query) value "$1" "${2:-}" ""; queries+=("$2"); shift 2 ;;
    --resume) resume=1; shift ;;
    --dry-run) dry_run=1; shift ;;
    *) die "unknown argument $1" ;;
  esac
done
[ -n "$instance" ] || die "--instance-id is required: without it nebius.sh means nebius-gpu, which runs no sf40"
case $dataset in
  tpch|tpcds) sets=("$dataset") ;;
  all) sets=(tpch tpcds) ;;
  "") die "--dataset tpch, tpcds or all is required" ;;
  *) die "--dataset $dataset: tpch, tpcds or all" ;;
esac
sf=${sf:-40} runs=${runs:-10} timeout=${timeout:-900}
cases=${cases:-$root/testdata/benchmark-results/h100-80gb-hbm3/cases.csv}
[[ "$sf" =~ ^[0-9]+$ ]] || die "--sf $sf: a scale factor of digits"
[[ "$runs" =~ ^[0-9]+$ ]] && [ "$runs" -ge 2 ] || die "--runs $runs: at least 2, for the second-smallest"
[[ "$timeout" =~ ^[1-9][0-9]*$ ]] || die "--timeout $timeout: whole seconds above 0"
[ -f "$cases" ] || die "--cases $cases: no such file"

# Every row of the list at --sf as `<dataset> <stem>`, in file order: the device list's own CSV,
# whose notes may hold a comma.
rows=$(python3 -I - "$cases" "$sf" <<'PY'
import csv, sys
with open(sys.argv[1], newline="") as f:
    for row in csv.DictReader(f):
        if row["sf"] == sys.argv[2]:
            print(row["dataset"], row["query"].replace("_", "-"))
PY
) || die "cannot read --cases $cases (above)"

declare -A wanted=()
for query in "${queries[@]}"; do wanted[${query//_/-}]=unmatched; done
# Per dataset to run, `<dataset> <stems> <every stem of its list>`.
plan=()
for ds in "${sets[@]}"; do
  stems=() every=()
  while read -r row_ds stem; do
    [ "$row_ds" = "$ds" ] || continue
    every+=("$stem")
    if [ ${#queries[@]} -gt 0 ]; then
      [ -n "${wanted[$stem]+named}" ] || continue
      wanted[$stem]=matched
    fi
    stems+=("$stem")
  done <<< "$rows"
  if [ ${#stems[@]} -gt 0 ]; then plan+=("$ds $(IFS=,; echo "${stems[*]}") $(IFS=,; echo "${every[*]}")"); fi
done
for query in "${!wanted[@]}"; do
  [ "${wanted[$query]}" = matched ] || die "--query $query: no row of $cases at sf$sf for ${sets[*]}"
done
[ ${#plan[@]} -gt 0 ] || die "no row of $cases at sf$sf for ${sets[*]}: nothing to run"

vm=(--instance-id "$instance")
[ -z "$profile" ] || vm+=(--profile "$profile")
extra=()
[ "$resume" -eq 0 ] || extra+=(--resume)

# step <command…>: printed under --dry-run; else run, a non-zero exit ending this one as its own.
step() {
  local rc=0
  if [ "$dry_run" -eq 1 ]; then
    printf '%q ' "$@" | sed 's/ $//'
    echo
    return 0
  fi
  "$@" || rc=$?
  [ "$rc" -eq 0 ] || { echo "bench-sirius.sh: ${1##*/} ${*:2:3} … exited $rc" >&2; exit "$rc"; }
}

cd "$root"
for entry in "${plan[@]}"; do
  read -r ds list every <<< "$entry"
  narrowed=()
  [ ${#queries[@]} -eq 0 ] || narrowed=(--list "$every")
  step "${RUN_SIRIUS[@]}" "${vm[@]}" run -- --dataset "$ds" --sf "$sf" --queries "$list" --runs "$runs" \
    --timeout "$timeout" "${narrowed[@]}" "${extra[@]}"
  step "${RUN_GPU[@]}" "${vm[@]}" pull
  step "$BUILD_TEST" -- python3 testdata/duckdb_result.py --dataset "$ds" --sf "$sf" \
    --answers "testdata/calibration/sirius/$ds.sf$sf/answers.duckdb" --out "testdata/goldens/$ds.sf$sf/sirius-result.txt"
done
