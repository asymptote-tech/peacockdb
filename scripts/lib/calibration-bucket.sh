# shellcheck shell=bash
#
# The `calibration` bucket, where the benchmark's per-call record is published. Keys are by
# content, `<dataset>.sf<sf>/<sha256>.tsv`, so an object is never replaced and every record a
# commit ever pinned stays fetchable. Sourced, from the repo root, by
# scripts/calibration/fetch_record.sh, which reads it, and by build-test-shadgpu.sh, whose
# --pull-benchmarks writes it.

CALIBRATION_BUCKET=calibration
# Every aws call names the endpoint, or it goes to real AWS. Nebius, as upload_datasets.sh.
CALIBRATION_S3=(--endpoint-url https://storage.eu-north1.nebius.cloud:443 --region eu-north-1)
# Which record each dataset's tree was measured beside: `<sha256>  <dataset>.sf<sf>/records.tsv`,
# sha256sum's format relative to testdata/calibration/, where the fetch puts it. Committed.
CALIBRATION_PIN=testdata/calibration/records.sha256

# pinned_sha <dataset>.sf<sf> — the sha256 the pin names for it, empty when it names none.
pinned_sha() { awk -v path="$1/records.tsv" '$2 == path { print $1 }' "$CALIBRATION_PIN"; }

# record_key <dataset>.sf<sf> <sha256> — the object holding that record.
record_key() { printf '%s/%s.tsv' "$1" "$2"; }
