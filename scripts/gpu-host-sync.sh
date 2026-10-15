#!/usr/bin/env bash
# The working tree, uncommitted, to nebius-gpu (llm-wiki/build-test.md, "GPU host"). Chain J's
# rsync: ignored build dirs, target dirs and generated data survive on both sides. Then the sf1
# data is generated on the VM if it is missing.
#
#   gpu-host-sync.sh [--dest <dir under ~ on the VM>]     prints the dest
set -euo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(git -C "$here" rev-parse --show-toplevel)
dest=peacockdb-$(basename "$root")
[ "${1:-}" = --dest ] && dest=${2:?dest}
cd "$root"
attempt=1
# --delete-after, not --delete: the first sync must not delete before the .gitignore files
# arrive, or it deletes the ignored data and build dirs it should keep. The P rule protects the
# test run state (.gpu-test/) on the VM whatever .gitignore says.
until rsync -a --timeout=90 --delete-after --exclude=.git --filter='P /.gpu-test/' \
        --filter=':- .gitignore' \
        -e "$here/gpu-host.sh rsh" ./ "nebius-gpu:$dest/"; do
  [ "$attempt" -ge 5 ] && { echo "gpu-host-sync: rsync failed $attempt times" >&2; exit 1; }
  echo "gpu-host-sync: rsync attempt $attempt failed; retrying in $((attempt * 15))s" >&2
  sleep $((attempt * 15)); attempt=$((attempt + 1))
done
# generate_testdata.sh exits 0 early ("already exists, skipping generation") when
# testdata/<bench>.sf1 is present, so no wrapper is needed. But it looks up duckdb before that
# skip, and a non-interactive ssh does not source ~/peacock-env.sh: source it here.
# No PEACOCK_NO_FETCH: generate_testdata.sh neither reads it nor calls fetch_embeddings.sh.
"$here/gpu-host.sh" ssh "[ -f ~/peacock-env.sh ] && . ~/peacock-env.sh; cd ~/$dest \
  && testdata/generate_testdata.sh --bench tpch \
  && testdata/generate_testdata.sh --bench tpcds" >&2
echo "$dest"
