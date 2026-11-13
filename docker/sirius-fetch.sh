#!/usr/bin/env bash
# sirius-fetch.sh <secrets> <out> — the Sirius extension docker/pins.env pins, from the private
# calibration bucket, unpacked into <out>. docker/sirius.Dockerfile runs it with the pins as build
# args in the environment (SIRIUS_EXTENSION_S3, _SHA256, _BYTES, _UNPACKED_SHA256) and the storage
# keys, endpoint and region as BuildKit secret files under <secrets>. Those reach aws's environment
# alone: never an argv, an output or a file. The download's size and sha256 are checked before it
# is unpacked, and the unpacked sha256 before it is moved into place, so a failed check writes no
# <out>. Exit 0; 1 the fetch or a check failed; 2 bad input.
set -euo pipefail

die() { echo "sirius-fetch.sh: $*" >&2; exit 1; }
refuse() { echo "sirius-fetch.sh: $*" >&2; exit 2; }

[ $# -eq 2 ] || refuse "want <secrets dir> <out file>, got $# arguments"
secrets=$1 out=$2
for pin in SIRIUS_EXTENSION_S3 SIRIUS_EXTENSION_SHA256 SIRIUS_EXTENSION_BYTES SIRIUS_EXTENSION_UNPACKED_SHA256; do
  [ -n "${!pin:-}" ] || refuse "no $pin: docker/pins.env's lines are the build args"
done
for secret in aws_access_key_id aws_secret_access_key aws_endpoint_url aws_region; do
  [ -s "$secrets/$secret" ] || refuse "no secret $secret under $secrets: scripts/env/image.sh passes it"
done

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
AWS_ACCESS_KEY_ID=$(< "$secrets/aws_access_key_id") \
AWS_SECRET_ACCESS_KEY=$(< "$secrets/aws_secret_access_key") \
AWS_ENDPOINT_URL=$(< "$secrets/aws_endpoint_url") \
AWS_REGION=$(< "$secrets/aws_region") \
  aws s3 cp --only-show-errors "$SIRIUS_EXTENSION_S3" "$work/extension.gz" \
  || die "cannot fetch $SIRIUS_EXTENSION_S3"

size=$(stat -c %s "$work/extension.gz")
[ "$size" = "$SIRIUS_EXTENSION_BYTES" ] \
  || die "the download is $size bytes and SIRIUS_EXTENSION_BYTES is $SIRIUS_EXTENSION_BYTES"
sum=$(sha256sum < "$work/extension.gz")
[ "${sum%% *}" = "$SIRIUS_EXTENSION_SHA256" ] \
  || die "the download's sha256 is ${sum%% *} and SIRIUS_EXTENSION_SHA256 is $SIRIUS_EXTENSION_SHA256"
gunzip -c "$work/extension.gz" > "$work/extension"
rm "$work/extension.gz"
sum=$(sha256sum < "$work/extension")
[ "${sum%% *}" = "$SIRIUS_EXTENSION_UNPACKED_SHA256" ] \
  || die "the unpacked sha256 is ${sum%% *} and SIRIUS_EXTENSION_UNPACKED_SHA256 is $SIRIUS_EXTENSION_UNPACKED_SHA256"
mkdir -p "$(dirname "$out")"
mv "$work/extension" "$out"
