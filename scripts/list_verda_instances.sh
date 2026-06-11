#!/usr/bin/env bash
#
# List Verda Cloud (formerly DataCrunch) instances as "id ip" pairs.
#
# Authentication: Verda uses OAuth2 client-credentials (a client ID + secret
# pair, NOT a single API key). Pass them via environment variables so they
# never appear in `ps`, shell history, or CI logs:
#
#     export VERDA_CLIENT_ID=...
#     export VERDA_CLIENT_SECRET=...
#     ./scripts/list_verda_instances.sh
#
set -euo pipefail

API="${VERDA_API:-https://api.verda.com/v1}"

die() { echo "error: $*" >&2; exit 1; }

[[ -n "${VERDA_CLIENT_ID:-}"     ]] || die "VERDA_CLIENT_ID is not set in the environment"
[[ -n "${VERDA_CLIENT_SECRET:-}" ]] || die "VERDA_CLIENT_SECRET is not set in the environment"
command -v jq   >/dev/null || die "jq is required"
command -v curl >/dev/null || die "curl is required"

# ---- get OAuth2 token --------------------------------------------------------
# Credentials are read from the environment and sent in the request body; they
# are never placed on the command line.
TOKEN="$(
    jq -n --arg id "$VERDA_CLIENT_ID" --arg secret "$VERDA_CLIENT_SECRET" \
        '{grant_type:"client_credentials", client_id:$id, client_secret:$secret}' \
    | curl -fsS -X POST "$API/oauth2/token" \
        -H 'Content-Type: application/json' \
        --data @- \
    | jq -r '.access_token'
)"
[[ -n "$TOKEN" && "$TOKEN" != "null" ]] || die "failed to obtain access token"

# ---- list instances ----------------------------------------------------------
curl -fsS "$API/instances" \
    -H "Authorization: Bearer $TOKEN" \
| jq -r '.[] | "\(.id)\t\(.ip // "-")"'
