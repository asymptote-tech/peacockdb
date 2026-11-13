#!/usr/bin/env bash
# The CPU-VM build's image, pulled on the VM and refused unless it was built from this tree
# (llm-wiki/build-test.md, "The CPU-VM build"). Run on dev, before the build's `docker run`.
#
#   check-build-image.sh --instance-id <id> [--profile <name>] [--cudf <v>]
#
# docker runs on the VM, through scripts/nebius.sh ssh ($NEBIUS_SH); the comparison runs here.
# The VM's copy of the tree comes from nebius-sync.sh without .git, and the build image's inputs
# include the third_party/cudf commit this tree's git index pins. --instance-id is required:
# without it nebius.sh means nebius-gpu, which builds nothing. <v> is 25.02 by default. The image is
# the one of this tree's inputs, by its inputs tag, or $PEACOCK_BUILD_IMAGE when set, such as
# `scripts/env/image-ref.sh build <v>` prints. Prints the ref alone, for the build's docker run.
#
# Exit 0 the VM's image is this tree's; 2 bad input (nebius.sh's 64 too), or an image without a
# label, built from other inputs, not published (the images.yml dispatch named), or whose pull
# filled the VM's disk; 255 an ssh to the VM dropped, the VM perhaps stopped (nebius.sh status,
# start, then this again); else nebius.sh's (75, 69, 1), or a failed pull's.
set -euo pipefail
shopt -s inherit_errexit

usage() { sed -n '2,${/^#/!q;p;}' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'; exit 0; }
die() { echo "check-build-image.sh: $*" >&2; exit 2; }

here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(git -C "$here" rev-parse --show-toplevel)
NEBIUS=("${NEBIUS_SH:-$root/scripts/nebius.sh}")
cudf=25.02 instance=""
while [ $# -gt 0 ]; do
  case $1 in
    -h|--help) usage ;;
    --instance-id|--profile) [ -n "${2:-}" ] || die "$1 needs a value"; NEBIUS+=("$1" "$2")
                             [ "$1" = --profile ] || instance=$2; shift 2 ;;
    --cudf) [ -n "${2:-}" ] || die "--cudf needs a version such as 25.02"; cudf=$2; shift 2 ;;
    *) die "unknown argument $1" ;;
  esac
done
[ -f "$root/docker/conda/cudf-$cudf.spec" ] || die "no cuDF $cudf: docker/conda has no cudf-$cudf.spec"
[ -n "$instance" ] || die "name the VM with --instance-id: without it nebius.sh means nebius-gpu"

. "$root/scripts/lib/image-inputs.sh"
. "$root/scripts/lib/data-inputs.sh"
. "$root/scripts/lib/machine.sh"
MACHINE_NEBIUS="scripts/nebius.sh ${NEBIUS[*]:1}"

# docker, for machine_image_checked: the VM's.
docker() {
  local rc=0
  "${NEBIUS[@]}" ssh "$(printf '%q ' docker "$@")" < /dev/null || rc=$?
  [ "$rc" -ne 64 ] || die "${NEBIUS[0]} refused its options: ${NEBIUS[*]:1}"
  if [ "$rc" -eq 255 ]; then
    echo "check-build-image.sh: the ssh to the VM dropped." >&2
    machine_vm_lost
  fi
  return "$rc"
}

ref=${PEACOCK_BUILD_IMAGE:-$MACHINE_IMAGE:$(image_inputs_tag "$root" build "$cudf")}
machine_image_checked "$ref" "$root" "$cudf" >&2
echo "==> $ref on the VM is this tree's" >&2
echo "$ref"
