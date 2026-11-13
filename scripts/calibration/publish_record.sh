#!/bin/bash
#
# Publish split records from dev: each `<from>/<name>/records.tsv` to the `calibration` bucket as
# `<name>/<sha256>.tsv`, read back and checked, then pinned in testdata/calibration/records.sha256
# and moved to testdata/calibration/<name>/records.tsv, where fetch_record.sh would put it. <name> is
# `<device>/<dataset>.sf<sf>`, as split_record.py prints it. build-test-shadgpu.sh's
# --pull-benchmarks does the same through shad-gpu; this is for a record joined and split on dev
# (join_records.py, split_record.py), as bench-each.sh's segments are.
#   scripts/calibration/publish_record.sh --from <split dir> h100-80gb-hbm3/tpch.sf40 h100-80gb-hbm3/tpcds.sf40
# AWS is the CLI, `aws` by default, with the bucket's credentials; the endpoint and region are
# lib/calibration-bucket.sh's. Every name and file is checked before the first upload.
set -euo pipefail

SELF=$(realpath "${BASH_SOURCE[0]}")
cd "$(dirname "$SELF")/../.."
. scripts/lib/calibration-bucket.sh

die() { echo "$*" >&2; exit 1; }
usage() { sed -n '2,${/^#/!q;p;}' "$SELF" | sed 's/^# \{0,1\}//'; exit "${1:-0}"; }

AWS=${AWS:-aws}
FROM=""
NAMES=()
while [ $# -gt 0 ]; do
  case "$1" in
    --from) [ -n "${2:-}" ] || die "--from requires a directory"; FROM=$2; shift ;;
    -h|--help) usage 0 ;;
    -*) echo "unknown flag: $1" >&2; usage 1 ;;
    *) NAMES+=("$1") ;;
  esac
  shift
done
[ -n "$FROM" ] || die "--from <split dir> is required: where split_record.py wrote the files"
[ ${#NAMES[@]} -gt 0 ] || die "name what to publish, e.g. h100-80gb-hbm3/tpch.sf40"
for name in "${NAMES[@]}"; do
  [[ "$name" =~ ^[a-z0-9]+(-[a-z0-9]+)*/[a-z]+\.sf[0-9]+$ ]] \
    || die "$name is not <device>/<dataset>.sf<sf>, e.g. h100-80gb-hbm3/tpch.sf40"
  [ -s "$FROM/$name/records.tsv" ] || die "no $FROM/$name/records.tsv: nothing was published"
done

for name in "${NAMES[@]}"; do
  file=$FROM/$name/records.tsv
  sha=$(sha256sum "$file" | cut -d' ' -f1)
  key=$(record_key "$name" "$sha")
  "$AWS" "${CALIBRATION_S3[@]}" s3 cp --content-type text/tab-separated-values "$file" "s3://$CALIBRATION_BUCKET/$key" \
    || die "uploading $key failed (above)."
  landed=$("$AWS" "${CALIBRATION_S3[@]}" s3 cp "s3://$CALIBRATION_BUCKET/$key" - | sha256sum | cut -d' ' -f1) \
    || die "reading s3://$CALIBRATION_BUCKET/$key back failed (above); it is not pinned."
  [ "$landed" = "$sha" ] || die "s3://$CALIBRATION_BUCKET/$key reads back as $landed, not the $sha sent;
     it is not pinned. Publish it again."
  { awk -v path="$name/records.tsv" '$2 != path' "$CALIBRATION_PIN"
    printf '%s  %s\n' "$sha" "$name/records.tsv"; } | LC_ALL=C sort -k2 > "$CALIBRATION_PIN.new"
  mv "$CALIBRATION_PIN.new" "$CALIBRATION_PIN"
  mkdir -p "testdata/calibration/$name"
  mv "$file" "testdata/calibration/$name/records.tsv"
  echo "==> s3://$CALIBRATION_BUCKET/$key, pinned for $name"
done
echo "==> commit $CALIBRATION_PIN beside the tree to keep the run"
