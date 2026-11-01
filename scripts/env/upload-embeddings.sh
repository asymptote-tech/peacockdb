#!/usr/bin/env bash
# Publish the embeddings cache to S3, once: every file testdata/embeddings.sha256 pins, to the
# bucket scripts/lib/embeddings-bucket.sh names, under <sha256>/<file>.
#
#   scripts/env/upload-embeddings.sh [--cache <dir>]
#
# --cache is the directory holding the files, testdata/embeddings-cache by default. Every file is
# checked against its pin before the first upload: one missing or differing stops the run with
# nothing uploaded. An object already in the bucket at the file's size is left alone, so a re-run
# uploads only what is missing. The aws CLI takes its credentials from AWS_* or ~/.aws, and the
# storage from AWS_ENDPOINT_URL and AWS_REGION.
set -euo pipefail

usage() { sed -n '2,${/^#/!q;p;}' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'; exit 0; }
die() { echo "upload-embeddings.sh: $*" >&2; exit 2; }

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
. "$root/scripts/lib/embeddings-bucket.sh"
pin=$root/testdata/embeddings.sha256
cache=$root/testdata/embeddings-cache
cache_given=0
while [ $# -gt 0 ]; do
  case "$1" in
    -h|--help) usage ;;
    --cache)
      [ -n "${2:-}" ] || die "--cache needs a directory"
      [ "$cache_given" = 0 ] || die "one --cache per call: $cache and $2"
      cache=$2 cache_given=1
      shift ;;
    *) die "unknown argument $1" ;;
  esac
  shift
done
[ -d "$cache" ] || die "no cache directory $cache"
s3_args EMBEDDINGS_S3

shas=() names=()
while read -r sha name; do
  case "$sha" in ''|'#'*) continue ;; esac
  [[ "$sha" =~ ^[0-9a-f]{64}$ ]] && [ -n "$name" ] || die "$pin: not '<sha256>  <file>': $sha $name"
  shas+=("$sha") names+=("$name")
done < "$pin"
[ ${#names[@]} -gt 0 ] || die "$pin pins no file"

for i in "${!names[@]}"; do
  file=$cache/${names[$i]}
  [ -f "$file" ] || die "$file is missing: nothing uploaded"
  got=$(sha256sum "$file" | cut -d' ' -f1)
  [ "$got" = "${shas[$i]}" ] || die "$file has sha256 $got, not the pinned ${shas[$i]}: nothing uploaded"
done

for i in "${!names[@]}"; do
  file=$cache/${names[$i]}
  key=$(embeddings_key "${shas[$i]}" "${names[$i]}")
  size=$(stat -Lc%s "$file")
  # A failed head-object is taken as absent: if it was access, the upload below says so.
  have=$(aws "${EMBEDDINGS_S3[@]}" s3api head-object --bucket "$EMBEDDINGS_BUCKET" --key "$key" \
           --query ContentLength --output text 2>/dev/null) || have=""
  if [ "$have" = "$size" ]; then
    echo "  present: s3://$EMBEDDINGS_BUCKET/$key"
    continue
  fi
  echo "  uploading ${names[$i]} -> s3://$EMBEDDINGS_BUCKET/$key"
  aws "${EMBEDDINGS_S3[@]}" s3 cp --only-show-errors "$file" "s3://$EMBEDDINGS_BUCKET/$key"
done
echo "==> ${#names[@]} files in s3://$EMBEDDINGS_BUCKET, as $pin pins them"
