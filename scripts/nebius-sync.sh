#!/usr/bin/env bash
# The working tree, uncommitted, to a Nebius VM, nebius-gpu by default (llm-wiki/build-test.md,
# "Nebius host"). Chain J's rsync: ignored build dirs, target dirs and generated data survive on
# both sides. Then the sf1 data is generated on the VM if it is missing, unless --no-data.
#
#   nebius-sync.sh [--instance-id <id>] [--profile <name>] [--dest <dir under ~ on the VM>] [--no-data]
#                                                       prints the dest
# --instance-id and --profile go to every nebius.sh call, which checks them.
set -euo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
RSYNC=${GPU_HOST_RSYNC:-rsync}
root=$(git -C "$here" rev-parse --show-toplevel)
dest=peacockdb-$(basename "$root")
opts=() data=1
while [ $# -gt 0 ]; do
  case $1 in
    --instance-id|--profile) opts+=("$1" "${2:?$1 needs a value}"); shift 2;;
    --dest) dest=${2:?dest}; shift 2;;
    --no-data) data=0; shift;;
    *) echo "nebius-sync: unknown option $1" >&2; sed -n '6,7p' "$0" >&2; exit 64;;
  esac
done
gh=("$here/nebius.sh" "${opts[@]}")
cd "$root"
attempt=1
# --delete-after, not --delete: the first sync must not delete before the .gitignore files
# arrive, or it deletes the ignored data and build dirs it should keep. The P rule protects the
# test run state (.gpu-test/) on the VM whatever .gitignore says.
until "$RSYNC" -a --timeout=90 --delete-after --exclude=.git --filter='P /.gpu-test/' \
        --filter=':- .gitignore' \
        -e "${gh[*]} rsh" ./ "nebius-gpu:$dest/"; do
  [ "$attempt" -ge 5 ] && { echo "nebius-sync: rsync failed $attempt times" >&2; exit 1; }
  echo "nebius-sync: rsync attempt $attempt failed; retrying in $((attempt * 15))s" >&2
  sleep $((attempt * 15)); attempt=$((attempt + 1))
done
# generate_testdata.sh exits 0 early ("already exists, skipping generation") when
# testdata/<bench>.sf1 is present, so no wrapper is needed. But it looks up duckdb before that
# skip, and a non-interactive ssh does not source ~/peacock-env.sh: source it here.
# No PEACOCK_NO_FETCH: generate_testdata.sh neither reads it nor calls fetch_embeddings.sh.
if [ "$data" -eq 1 ]; then
  "${gh[@]}" ssh "[ -f ~/peacock-env.sh ] && . ~/peacock-env.sh; cd ~/$dest \
    && testdata/generate_testdata.sh --bench tpch \
    && testdata/generate_testdata.sh --bench tpcds" >&2
fi
echo "$dest"
