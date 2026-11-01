# shellcheck shell=bash
#
# The `embeddings` bucket, which holds the embeddings cache testdata/fetch_embeddings.sh fetches
# from the origins. Keys are by content, `<sha256>/<file>`, the sha256 the pin
# (testdata/embeddings.sha256) names for the file; the upload skips an object already at its
# file's size. Sourced by fetch_embeddings.sh, whose --from-s3 reads it, and by
# scripts/env/upload-embeddings.sh.

EMBEDDINGS_BUCKET=embeddings
# Every aws call names the endpoint, or it goes to real AWS: a caller sets EMBEDDINGS_S3 with
# `s3_args EMBEDDINGS_S3` before its first call, so the origins need no storage named.
. "$(dirname "${BASH_SOURCE[0]}")/s3.sh"

# embeddings_pinned_sha <pin file> <file> — every sha256 the pin names for <file>, one a line.
embeddings_pinned_sha() { awk -v name="$2" '$2 == name { print $1 }' "$1"; }

# embeddings_key <sha256> <file> — the object holding that file.
embeddings_key() { printf '%s/%s' "$1" "$2"; }
