#!/usr/bin/env bash
# The working tree, uncommitted, to a Nebius VM, nebius-gpu by default (llm-wiki/build-test.md,
# "Nebius host"): what run-gpu.sh, which ships a staged install and the committed testdata, does
# not send. The CPU-VM build builds from it. Chain J's rsync: ignored build dirs, target dirs and
# generated data survive on both sides. No data is generated: sf1 on a VM is run-gpu.sh prepare's
# or bootstrap-vm.sh's. --no-data is accepted and does nothing, for the callers that pass it.
#
#   nebius-sync.sh [--instance-id <id>] [--profile <name>] [--dest <dir under ~ on the VM>] [--no-data]
#                                                       prints the dest
# --instance-id and --profile go to every nebius.sh call, which checks them.
set -euo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
RSYNC=${GPU_HOST_RSYNC:-rsync}
root=$(git -C "$here" rev-parse --show-toplevel)
dest=peacockdb-$(basename "$root")
opts=()
while [ $# -gt 0 ]; do
  case $1 in
    --instance-id|--profile) opts+=("$1" "${2:?$1 needs a value}"); shift 2;;
    --dest) dest=${2:?dest}; shift 2;;
    --no-data) shift;;
    *) echo "nebius-sync: unknown option $1" >&2; sed -n '8,9p' "$0" >&2; exit 64;;
  esac
done
gh=("$here/nebius.sh" "${opts[@]}")
cd "$root"
attempt=1
# --delete-after, not --delete: the first sync must not delete before the .gitignore files
# arrive, or it deletes the ignored data and build dirs it should keep.
until "$RSYNC" -a --timeout=90 --delete-after --exclude=.git --filter=':- .gitignore' \
        -e "${gh[*]} rsh" ./ "nebius-gpu:$dest/"; do
  [ "$attempt" -ge 5 ] && { echo "nebius-sync: rsync failed $attempt times" >&2; exit 1; }
  echo "nebius-sync: rsync attempt $attempt failed; retrying in $((attempt * 15))s" >&2
  sleep $((attempt * 15)); attempt=$((attempt + 1))
done
echo "$dest"
