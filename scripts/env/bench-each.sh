#!/usr/bin/env bash
# The benchmark on a VM one process per query, over scripts/env/run-gpu.sh (llm-wiki/build-test.md,
# "The H100's device run"), so no process nears the watcher's 4-hour cap and a crashed case leaves
# the next one a fresh CUDA context.
#
#   bench-each.sh --instance-id <id> [--profile <name>] [--dir <i>] --cases <list> --mode <mode>
#                 [--within <cases.csv>] --timeout <s> --records <dir>
#
# Per corpus_query_benchmark! line of <list> (the main list) whose modes carry <mode>, in file order,
# and with --within only a query holding an `ok` or `wrong` row there (scripts/calibration/
# device_cases.py reads it): a section already in this tree's device tree is kept, timed or failed.
# Else `run-gpu.sh … run bench` with PCK_TEST_FILTER=bench_<ds>_sf<sf>_<q>_<mode>,
# PEACOCK_BENCH_RESUME=1, PEACOCK_BENCH_CASE_TIMEOUT_S=<s> and PCK_WRITE_GPU_RESULT=1; then `pull`,
# and the pulled testdata/calibration/records.tsv moved to <dir>/<ds>-<q>-<mode>.<attempt>.tsv and
# removed on the VM, <attempt> one past the highest already there. Before each run, a record a killed
# run left on the VM is pulled and set aside under its last row's case. The device tree is the one
# the first measured case's pull wrote into or, when the run kept the case's section (measured on
# this VM before), the one whose section names the VM's host; its slug is kept in <dir>/device.
# --instance-id and --profile go to every run-gpu.sh and scripts/nebius.sh call, --dir to
# run-gpu.sh alone. --instance-id is required, as run-gpu.sh requires it: without it nebius.sh means
# nebius-gpu, which runs no sf40. The VM is the caller's to start and stop.
#
# Exit 0 every case has its section; 1 one has none (a run died before writing it: rerun); 2 bad
# input; else the run-gpu.sh exit that stopped it (75, 69: the VM; 3, 4: run-gpu.sh's).
set -euo pipefail
shopt -s inherit_errexit

usage() { sed -n '2,${/^#/!q;p;}' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'; exit 0; }
die() { echo "bench-each.sh: $*" >&2; exit 2; }

here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(git -C "$here" rev-parse --show-toplevel)
RUN_GPU=("${RUN_GPU_SH:-$here/run-gpu.sh}")
NEBIUS=("${NEBIUS_SH:-$root/scripts/nebius.sh}")
MODES=" tp1_single tp1_rowgroup tp4_single tp4_rowgroup tp4_sized "
dir=$(basename "$root") instance="" cases="" mode="" within="" timeout="" records=""
while [ $# -gt 0 ]; do
  case $1 in
    -h|--help) usage ;;
    --instance-id|--profile)
      [ -n "${2:-}" ] || die "$1 needs a value"; RUN_GPU+=("$1" "$2"); NEBIUS+=("$1" "$2")
      [ "$1" != --instance-id ] || instance=$2; shift 2 ;;
    --dir) [ -n "${2:-}" ] || die "--dir needs a name"; dir=$2; RUN_GPU+=(--dir "$2"); shift 2 ;;
    --cases) [ -n "${2:-}" ] || die "--cases needs a file"; cases=$2; shift 2 ;;
    --mode) [ -n "${2:-}" ] || die "--mode needs a mode"; mode=$2; shift 2 ;;
    --within) [ -n "${2:-}" ] || die "--within needs a file"; within=$2; shift 2 ;;
    --timeout) [ -n "${2:-}" ] || die "--timeout needs seconds"; timeout=$2; shift 2 ;;
    --records) [ -n "${2:-}" ] || die "--records needs a directory"; records=$2; shift 2 ;;
    *) die "unknown argument $1" ;;
  esac
done
[ -n "$instance" ] || die "--instance-id is required: without it nebius.sh means nebius-gpu, which runs no sf40"
[ -n "$cases" ] && [ -n "$mode" ] && [ -n "$timeout" ] && [ -n "$records" ] \
  || die "--cases, --mode, --timeout and --records are required"
[ -f "$cases" ] || die "--cases $cases: no such file"
case $MODES in *" $mode "*) ;; *) die "--mode $mode: one of$MODES" ;; esac
[[ "$timeout" =~ ^[1-9][0-9]*$ ]] || die "--timeout $timeout: whole seconds above 0"
[ -z "$within" ] || [ -f "$within" ] || die "--within $within: no such file"
[[ "$dir" =~ ^[A-Za-z0-9][A-Za-z0-9._-]*$ ]] || die "--dir $dir: a directory name under ~/peacock/"

BENCH_CASES=$cases
. "$root/scripts/lib/bench-cases.sh"
lines=$(bench_lines) || die "cannot read $cases (above)"
kept=""
if [ -n "$within" ]; then
  kept=$(python3 "$root/scripts/calibration/device_cases.py" "$within" | awk -F'\t' '{ print $1, $2, $3 }') \
    || die "cannot read --within $within (above)"
fi
selected=()
while read -r ds sf query modes; do
  case "|$modes|" in *"|$mode|"*) ;; *) continue ;; esac
  if [ -n "$within" ] && ! grep -qxF "$ds $sf $query" <<< "$kept"; then continue; fi
  selected+=("$ds $sf $query")
done <<< "$lines"
[ ${#selected[@]} -gt 0 ] || die "no case of $cases at $mode${within:+ within $within}: nothing to run"

mkdir -p "$records"
results=$root/testdata/benchmark-results
record=$root/testdata/calibration/records.tsv
host_record="~/peacock/$dir/testdata/calibration/records.tsv"
slug=$(cat "$records/device" 2>/dev/null || true)

# set_aside: the pulled record to <records>/<ds>-<q>-<mode>.<attempt>.tsv by its last row's case,
# and the VM's copy removed, since run-tests.sh refuses a record holding rows. One with no row is
# no case's, and is removed on both sides.
set_aside() {
  local name attempt n
  if [ -s "$record" ]; then
    name=$(awk -F'\t' '/^#/ { next } !names { for (i = 1; i <= NF; i++) at[$i] = i; names = 1; next }
                       { q = $at["query"]; m = $at["mode"]; gsub("-", "_", q); gsub("-", "_", m)
                         last = $at["dataset"] "-" q "-" m } END { print last }' "$record")
    if [ -n "$name" ]; then
      attempt=0
      for n in "$records/$name".*.tsv; do
        [ -e "$n" ] || continue
        n=${n%.tsv}; n=${n##*.}
        [ "$n" -le "$attempt" ] || attempt=$n
      done
      mv "$record" "$records/$name.$((attempt + 1)).tsv"
      echo "==> record set aside: $records/$name.$((attempt + 1)).tsv"
    fi
  fi
  rm -f "$record"
  vm "rm -f $host_record"
}

vm() {
  local rc=0
  "${NEBIUS[@]}" ssh "$1" || rc=$?
  [ "$rc" -eq 0 ] || { echo "bench-each.sh: ${NEBIUS[*]} ssh $1 exited $rc" >&2; exit "$rc"; }
}

pull() {
  local rc=0
  "${RUN_GPU[@]}" pull || rc=$?
  [ "$rc" -eq 0 ] || { echo "bench-each.sh: run-gpu.sh pull exited $rc" >&2; exit "$rc"; }
}

# has_section <slug> <ds> <sf> <query>: the device tree holds the case's section.
has_section() {
  local file=$results/$1/$2.sf$3/$mode.benchmark.txt
  [ -f "$file" ] && grep -qxF "== ${4//_/-}" "$file"
}

# The tree files that hold the case's section, each with its content's sha.
holders() {
  local file
  for file in "$results"/*/"$1.sf$2/$mode.benchmark.txt"; do
    [ -f "$file" ] && grep -qxF "== ${3//_/-}" "$file" && echo "$(sha256sum < "$file" | cut -d' ' -f1) $file"
  done
  return 0
}

# kept_by <host> <ds> <sf> <query>: the slug of each device tree whose section of the case has
# <host>'s trailer, as one a resumed run kept does.
kept_by() {
  local file
  for file in "$results"/*/"$2.sf$3/$mode.benchmark.txt"; do
    [ -f "$file" ] || continue
    if awk -v wanted="== ${4//_/-}" -v host="host=$1" '/^== / { here = $0 == wanted }
           here && $0 == host { found = 1 } END { exit !found }' "$file"; then
      basename "$(dirname "$(dirname "$file")")"
    fi
  done
  return 0
}

# learn <slug>: the device tree, for this run and a rerun.
learn() {
  slug=$1
  echo "$slug" > "$records/device"
  echo "==> the device tree: $results/$slug"
}

vm_host=""
[ ! -s "$record" ] || set_aside
for entry in "${selected[@]}"; do
  read -r ds sf query <<< "$entry"
  if [ -n "$slug" ] && has_section "$slug" "$ds" "$sf" "$query"; then
    echo "==> $ds $query $mode: kept, $slug's section is in the tree"
    continue
  fi
  rc=0
  left=$("${NEBIUS[@]}" ssh "if test -s $host_record; then echo left; else echo none; fi") || rc=$?
  [ "$rc" -eq 0 ] || { echo "bench-each.sh: asking the VM for a record a killed run left exited $rc" >&2; exit "$rc"; }
  if [ "$left" = left ]; then
    pull
    set_aside
  fi
  before=$(holders "$ds" "$sf" "$query")
  start=$(date +%s) rc=0
  PCK_TEST_FILTER=bench_${ds}_sf${sf}_${query}_${mode} PEACOCK_BENCH_RESUME=1 \
    PEACOCK_BENCH_CASE_TIMEOUT_S=$timeout PCK_WRITE_GPU_RESULT=1 "${RUN_GPU[@]}" run bench || rc=$?
  echo "==> $ds $query $mode: run-gpu.sh exited $rc after $(( $(date +%s) - start )) s"
  case $rc in 0|1) ;; *) echo "bench-each.sh: stopped at $ds $query $mode: run-gpu.sh exited $rc" >&2; exit "$rc" ;; esac
  pull
  set_aside
  if [ -z "$slug" ]; then
    new=$(comm -13 <(sort <<< "$before") <(sort <<< "$(holders "$ds" "$sf" "$query")") | awk '{ print $2 }')
    if [ "$(grep -c . <<< "$new")" -eq 1 ]; then
      learn "$(basename "$(dirname "$(dirname "$new")")")"
    else
      # No file changed: the run kept a section this VM measured before, so no pull names the tree.
      if [ -z "$vm_host" ]; then
        rc=0
        vm_host=$("${NEBIUS[@]}" ssh hostname) || rc=$?
        [ "$rc" -eq 0 ] || { echo "bench-each.sh: asking the VM its hostname exited $rc" >&2; exit "$rc"; }
      fi
      named=$(kept_by "$vm_host" "$ds" "$sf" "$query")
      if [ "$(grep -c . <<< "$named")" -eq 1 ]; then learn "$named"; fi
    fi
  fi
done

missing=()
for entry in "${selected[@]}"; do
  read -r ds sf query <<< "$entry"
  if [ -z "$slug" ] || ! has_section "$slug" "$ds" "$sf" "$query"; then missing+=("$ds $query"); fi
done
if [ ${#missing[@]} -gt 0 ]; then
  echo "bench-each.sh: ${#missing[@]} of ${#selected[@]} cases at $mode have no section${slug:+ in $slug}: ${missing[*]};" \
    "rerun to measure them" >&2
  exit 1
fi
echo "==> bench-each.sh: ${#selected[@]} cases at $mode, every one with its section in $results/$slug"
