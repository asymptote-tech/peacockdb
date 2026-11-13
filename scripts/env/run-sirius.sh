#!/usr/bin/env bash
# Sirius on a GPU VM: docker/sirius-bench.py in the Sirius image, each run held by run-gpu.sh's
# runner on the VM (llm-wiki/build-test.md, "Sirius on the H100").
#
#   run-sirius.sh --instance-id <id> [--profile <name>] [--dir <i>] <verb>
#
#   prepare                    after run-gpu.sh … prepare, with sf40: the Sirius image of this tree's
#                                inputs (PEACOCK_SIRIUS_IMAGE when set), refused unless its
#                                peacockdb.inputs label is this tree's, and a VM without it short of
#                                its size and 10 GB under Docker's root refused before the pull; the
#                                seccomp profile, docker/sirius-seccomp.json, to ~/peacock/<i>/docker/;
#                                then every card held to the image's driver minimum (3)
#   run [--detach] -- <docker/sirius-bench.py arguments>
#                              the Sirius runner as container peacock-<i>, held by a runner on
#                                the VM that waits for the card's lock, its log
#                                logs/sirius-<stamp>.log followed, or, with --detach, not. It needs
#                                no install, and runs the image prepare checked, by its ID. It sends
#                                testdata/<ds>-queries/, the H100's results tree and the dataset's
#                                Sirius log directory (a newer file on the VM kept, for --resume),
#                                and for --list or --resume the dataset's Sirius answers database
#                                too, alike; each file sent is stamped as run-gpu.sh's ship stamps,
#                                so pull brings home what the run changed. A whole run removes the
#                                VM's answers database and stamps this tree's unsent, so pull takes
#                                the run's new one. The VM's sf40 data read-only at its own path as
#                                --data and the shipped testdata as --out; Docker's seccomp profile
#                                less its io_uring and NUMA refusals, memlock unlimited, the VM's
#                                hostname
#
# status, follow, cancel and pull are run-gpu.sh's, for a Sirius run as for any run of the dir. The
# options are run-gpu.sh's, and so are the dir's claim by one tree and the card lock.
#
# Exit 0; 1 the run failed or ended with no status, or ssh dropped while following; 2 bad input
# (nebius.sh's 64 too), a run in progress, a step missing (run-gpu.sh's prepare, this prepare), a
# stale image, one not published (the images.yml dispatch named), an image pull that filled the VM's
# disk, a driver query that fails or lists no GPU, another tree's dir, a copied tree's .run-gpu-tree,
# or the runner's own 2; 3 a card below compute capability 8.0, or one whose driver is older than
# the Sirius image needs; 130 or 143 a followed run interrupted, the run going on; 254 a step whose
# command exited 255 on the VM; 255 an ssh to the VM dropped, the VM perhaps stopped (nebius.sh
# status, start, then the step again); else nebius.sh's (75, 69), or a pull that failed otherwise,
# docker's.
set -euo pipefail
shopt -s inherit_errexit

usage() { sed -n '2,${/^#/!q;p;}' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'; exit 0; }
die() { echo "run-sirius.sh: $*" >&2; exit 2; }

here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(git -C "$here" rev-parse --show-toplevel)
NEBIUS=("${NEBIUS_SH:-$root/scripts/nebius.sh}")
dir=$(basename "$root") instance="" detach=0 ds="" sf="" keeps=0 runner=()
while [ $# -gt 0 ]; do
  case $1 in
    -h|--help) usage ;;
    --instance-id|--profile) [ -n "${2:-}" ] || die "$1 needs a value"; NEBIUS+=("$1" "$2")
                             [ "$1" = --profile ] || instance=$2; shift 2 ;;
    --dir) [ -n "${2:-}" ] || die "--dir needs a name"; dir=$2; shift 2 ;;
    -*) die "unknown option $1" ;;
    *) break ;;
  esac
done
verb=${1:-}
shift || true
case $verb in
  prepare) [ $# -eq 0 ] || die "prepare takes no argument: $*" ;;
  run)
    if [ "${1:-}" = --detach ]; then detach=1; shift; fi
    [ "${1:-}" = -- ] || die "run takes [--detach] -- <docker/sirius-bench.py arguments>"
    shift
    runner=("$@")
    for ((i = 0; i < ${#runner[@]}; i++)); do
      case ${runner[i]} in
        --dataset) ds=${runner[i + 1]:-} ;;
        --sf) sf=${runner[i + 1]:-} ;;
        --list|--resume) keeps=1 ;;
        --data|--out) die "run: run-sirius.sh gives ${runner[i]} (the VM's sf40 data, the shipped testdata): drop it" ;;
      esac
    done
    [[ "$ds" =~ ^(tpch|tpcds)$ ]] || die "run: the runner's --dataset, tpch or tpcds, picks the VM's data: got ${ds:-none}"
    [[ "$sf" =~ ^[0-9]+$ ]] || die "run: the runner's --sf picks the VM's data: got ${sf:-none}" ;;
  *) die "unknown verb ${verb:-(none)}: prepare or run; status, follow, cancel and pull are run-gpu.sh's" ;;
esac
# The tree's id, as run-gpu.sh's: the dir is claimed alike.
TREE_RECORD=$root/.run-gpu-tree
tree_path=$(cd "$root" && pwd -P)
. "$root/scripts/lib/gpu-vm-client.sh"
. "$root/scripts/env/run-sirius-vm.sh"
vm_options_checked
tree_id_read

SIRIUS_CUDF=$(image_sirius_cudf "$root")
SIRIUS_REF=${PEACOCK_SIRIUS_IMAGE:-ghcr.io/asymptote-tech/peacockdb-sirius:$(image_inputs_tag "$root" sirius "$SIRIUS_CUDF")}
# The Sirius image's disk use, measured 3.08 GB on 2026-11-10 (git show
# d290698f:llm-wiki/tasks/sirius-bench-detail.md), and what prepare leaves free beside it.
SIRIUS_IMAGE_GB=4 SIRIUS_SPARE_GB=10
SF40_SETS=(tpch.sf40 tpcds.sf40)

cmd_prepare() {
  local free need=$((SIRIUS_IMAGE_GB + SIRIUS_SPARE_GB))
  tree_claimed
  on_vm vm_sirius_ready "${SF40_SETS[@]}"
  if ! docker image inspect --format '{{.Id}}' "$SIRIUS_REF" > /dev/null 2>&1; then
    read -r -a free <<< "$(on_vm vm_docker_free)"
    [ "$((free[1] / 1000000000))" -ge "$need" ] \
      || die "$((free[1] / 1000000000)) GB free under ${free[0]} on the VM; the Sirius image needs $need" \
             "($SIRIUS_IMAGE_GB GB, and $SIRIUS_SPARE_GB to spare)"
  fi
  machine_image_checked "$SIRIUS_REF" "$root" "$SIRIUS_CUDF" sirius
  (cd "$root" && to_vm "peacock/$dir/" -R docker/sirius-seccomp.json)
  image_read "$SIRIUS_REF" sirius "$SIRIUS_CUDF"
  cards_checked "$SIRIUS_REF"
  echo "==> prepare: $SIRIUS_REF"
}

cmd_run() {
  local answers=testdata/calibration/sirius/$ds.sf$sf sent=() sub digest=""
  tree_claimed
  # The VM's Sirius image as prepare left it: no pull, and the run given the ID read with its labels.
  image_read "$SIRIUS_REF" sirius "$SIRIUS_CUDF"
  cards_checked "$SIRIUS_REF"
  on_vm vm_sirius_idle "$dir"
  # Sirius names its log by day and appends to it: the VM's day log starts from this tree's.
  for sub in testdata/tpch-queries testdata/tpcds-queries testdata/benchmark-results/h100-80gb-hbm3 \
             "$answers/log"; do
    if [ -d "$root/$sub" ]; then sent+=("$sub"); fi
  done
  # A narrowed or resumed run keeps the answers of queries it does not run, and pull brings the
  # VM's database home over this one: so the VM starts from this one. A whole run removes it, so
  # the VM's goes now, and this one is stamped as the run's starting point, never sent.
  if [ "$keeps" -eq 1 ]; then
    if [ -f "$root/$answers/answers.duckdb" ]; then sent+=("$answers/answers.duckdb"); fi
  else
    if [ -f "$root/$answers/answers.duckdb" ]; then digest=$(sha256_of "$root/$answers/answers.duckdb"); fi
    on_vm vm_sirius_answers_cleared "$dir" "$answers/answers.duckdb" "$digest"
  fi
  vm "mkdir -p ~/peacock/$dir/testdata/benchmark-results ~/peacock/$dir/$answers"
  if [ ${#sent[@]} -gt 0 ]; then
    # --update: a file newer on the VM, a run's rows not yet pulled, stays for --resume.
    (cd "$root" && to_vm "peacock/$dir/" -R --update "${sent[@]}")
    STAMPS=$(mktemp)
    trap 'rm -f "$STAMPS"' EXIT
    (cd "$root" && find "${sent[@]}" -type f -print0 | xargs -0 -r sha256sum) > "$STAMPS"
    on_vm_with "$STAMPS" vm_sent_stamped "$dir"
  fi
  if [ "$detach" -eq 1 ]; then
    on_vm vm_sirius "$dir" "$IMAGE_ID" 1 "$PEACOCK_CARD_LOCK" "$ds" "$sf" "${runner[@]}"
  else
    following vm_sirius "$dir" "$IMAGE_ID" 0 "$PEACOCK_CARD_LOCK" "$ds" "$sf" "${runner[@]}"
  fi
}

"cmd_$verb"
