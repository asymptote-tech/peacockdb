#!/bin/bash
#
# Fetch the calibration record this checkout pins for one dataset, from the `calibration`
# bucket's key `<dataset>.sf<sf>/<sha256>.tsv` (testdata/calibration/records.sha256 names the
# sha256), into testdata/calibration/<dataset>.sf<sf>/records.tsv, checked against that sha256.
#   AWS=/home/info/bin/aws scripts/calibration/fetch_record.sh --ssh shad-gpu tpch.sf40
#   scripts/calibration/fetch_record.sh tpch.sf40     # an aws CLI and credentials on this host
# AWS is the CLI on whichever host makes the call, `aws` by default: a CI runner's, with
# AWS_ACCESS_KEY_ID/AWS_SECRET_ACCESS_KEY. On shad-gpu it is /home/info/bin/aws, credentials in
# ~/.aws, and not on a non-interactive ssh's PATH — hence the first form, from a dev box.
set -euo pipefail

SELF=$(realpath "${BASH_SOURCE[0]}")
cd "$(dirname "$SELF")/../.."
. scripts/lib/calibration-bucket.sh

die() { echo "$*" >&2; exit 1; }
usage() { sed -n '2,${/^#/!q;p;}' "$SELF" | sed 's/^# \{0,1\}//'; exit "${1:-0}"; }

AWS=${AWS:-aws}
SSH_HOST=""
DATASET=""
while [ $# -gt 0 ]; do
  case "$1" in
    --ssh)
      [ -n "${2:-}" ] || die "--ssh requires a host"
      [ -z "$SSH_HOST" ] || die "one --ssh per call: $SSH_HOST and $2"
      SSH_HOST=$2; shift ;;
    -h|--help) usage 0 ;;
    -*) echo "unknown flag: $1" >&2; usage 1 ;;
    *) [ -z "$DATASET" ] || die "one dataset per call: $DATASET and $1"; DATASET=$1 ;;
  esac
  shift
done

[ -n "$DATASET" ] || { echo "name the dataset, e.g. tpch.sf40" >&2; usage 1; }
[[ "$DATASET" =~ ^[a-z]+\.sf[0-9]+$ ]] || die "$DATASET is not <dataset>.sf<sf>, e.g. tpch.sf40"
want=$(pinned_sha "$DATASET")
[ -n "$want" ] || die "$CALIBRATION_PIN pins no record for $DATASET, so this checkout has none
     to fetch. build-test-shadgpu.sh --pull-benchmarks publishes and pins one."
key=$(record_key "$DATASET" "$want")

dest=testdata/calibration/$DATASET/records.tsv
partial=$dest.partial
mkdir -p "$(dirname "$dest")"
get=("$AWS" "${CALIBRATION_S3[@]}" s3 cp "s3://$CALIBRATION_BUCKET/$key" -)
if [ -n "$SSH_HOST" ]; then
  ssh "$SSH_HOST" "$(printf '%q ' "${get[@]}")" > "$partial" \
    || { rm -f "$partial"; die "fetching s3://$CALIBRATION_BUCKET/$key on $SSH_HOST failed (above)."; }
else
  "${get[@]}" > "$partial" \
    || { rm -f "$partial"; die "fetching s3://$CALIBRATION_BUCKET/$key failed (above)."; }
fi

# The key is the content's hash, so a mismatch is a damaged transfer or object, not a newer run.
got=$(sha256sum "$partial" | cut -d' ' -f1)
if [ "$got" != "$want" ]; then
  rm -f "$partial"
  die "s3://$CALIBRATION_BUCKET/$key arrived with sha256 $got, not the $want it is named by."
fi
mv "$partial" "$dest"
echo "==> $dest: $(($(grep -vc '^#' "$dest") - 1)) rows, sha256 $got as pinned"
