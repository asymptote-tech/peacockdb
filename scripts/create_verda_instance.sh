#!/usr/bin/env bash
#
# Create a Verda Cloud (formerly DataCrunch) instance, booting from an
# existing OS/boot disk.
#
# Authentication: Verda uses OAuth2 client-credentials, i.e. a client ID +
# secret pair (NOT a single API key). Pass them via environment variables so
# they never appear in `ps`, shell history, or CI logs:
#
#     export VERDA_CLIENT_ID=...
#     export VERDA_CLIENT_SECRET=...
#     ./scripts/create_verda_instance.sh --instance-type 1H100.80S.22V \
#         --boot-disk <existing-os-volume-uuid>
#
# (Sourcing these from a git-ignored .env, your shell keyring, or CI secrets
# all work — the point is to keep them out of argv.)
#
set -euo pipefail

API="${VERDA_API:-https://api.verda.com/v1}"

# ---- defaults ----------------------------------------------------------------
INSTANCE_TYPE=""
BOOT_DISK=""                 # id of existing boot/OS disk -> sent as `image`
IS_SPOT=true                 # spot by default
LOCATION="${VERDA_LOCATION:-}"
HOSTNAME="peacockdb-$(date +%Y%m%d-%H%M%S)"
DESCRIPTION="created by create_verda_instance.sh"
SSH_KEY_IDS="${VERDA_SSH_KEY_IDS:-}"   # comma-separated UUID(s), optional

usage() {
    cat <<EOF
Usage: $(basename "$0") --instance-type TYPE --boot-disk DISK_ID [options]

Required:
  --instance-type TYPE   Verda instance type (e.g. 1H100.80S.22V, 1A100.8V)
  --boot-disk DISK_ID    UUID of an existing boot/OS disk to boot from
  --location CODE        Location code (e.g. FIN-01); env VERDA_LOCATION

Options:
  --spot                 Provision a spot instance (default)
  --on-demand            Provision an on-demand (pay-as-you-go) instance
  --hostname NAME        Instance hostname (default: auto-generated)
  --description TEXT     Free-text description
  --ssh-key-ids IDS      Comma-separated SSH key UUID(s) (env VERDA_SSH_KEY_IDS)
  -h, --help             Show this help

Credentials (required, via environment — never via flags):
  VERDA_CLIENT_ID, VERDA_CLIENT_SECRET
EOF
}

# ---- parse flags -------------------------------------------------------------
while [[ $# -gt 0 ]]; do
    case "$1" in
        --instance-type) INSTANCE_TYPE="$2"; shift 2 ;;
        --boot-disk)     BOOT_DISK="$2";     shift 2 ;;
        --spot)          IS_SPOT=true;       shift ;;
        --on-demand)     IS_SPOT=false;      shift ;;
        --location)      LOCATION="$2";      shift 2 ;;
        --hostname)      HOSTNAME="$2";      shift 2 ;;
        --description)   DESCRIPTION="$2";   shift 2 ;;
        --ssh-key-ids)   SSH_KEY_IDS="$2";   shift 2 ;;
        -h|--help)       usage; exit 0 ;;
        *) echo "Unknown argument: $1" >&2; usage >&2; exit 2 ;;
    esac
done

# ---- validate ----------------------------------------------------------------
die() { echo "error: $*" >&2; exit 1; }

[[ -n "$INSTANCE_TYPE" ]] || { usage >&2; die "--instance-type is required"; }
[[ -n "$BOOT_DISK"     ]] || { usage >&2; die "--boot-disk is required"; }
[[ -n "$LOCATION"      ]] || { usage >&2; die "--location is required (or set VERDA_LOCATION)"; }
[[ -n "${VERDA_CLIENT_ID:-}"     ]] || die "VERDA_CLIENT_ID is not set in the environment"
[[ -n "${VERDA_CLIENT_SECRET:-}" ]] || die "VERDA_CLIENT_SECRET is not set in the environment"
command -v jq   >/dev/null || die "jq is required"
command -v curl >/dev/null || die "curl is required"

if $IS_SPOT; then CONTRACT="SPOT"; else CONTRACT="PAY_AS_YOU_GO"; fi

# ---- get OAuth2 token --------------------------------------------------------
# Credentials are read from the environment and sent in the request body; they
# are never placed on the command line.
echo "Authenticating to $API ..." >&2
TOKEN="$(
    jq -n --arg id "$VERDA_CLIENT_ID" --arg secret "$VERDA_CLIENT_SECRET" \
        '{grant_type:"client_credentials", client_id:$id, client_secret:$secret}' \
    | curl -fsS -X POST "$API/oauth2/token" \
        -H 'Content-Type: application/json' \
        --data @- \
    | jq -r '.access_token'
)"
[[ -n "$TOKEN" && "$TOKEN" != "null" ]] || die "failed to obtain access token"

# ---- build instance creation payload ----------------------------------------
# An existing boot/OS disk id is supplied via the `image` field; no os_volume
# block is needed when booting from an existing volume.
PAYLOAD="$(
    jq -n \
        --arg instance_type "$INSTANCE_TYPE" \
        --arg image         "$BOOT_DISK" \
        --arg hostname      "$HOSTNAME" \
        --arg description   "$DESCRIPTION" \
        --arg location_code "$LOCATION" \
        --arg contract      "$CONTRACT" \
        --argjson is_spot   "$IS_SPOT" \
        --arg ssh_key_ids   "$SSH_KEY_IDS" \
        '{
            instance_type: $instance_type,
            image:         $image,
            hostname:      $hostname,
            description:   $description,
            location_code: $location_code,
            contract:      $contract,
            is_spot:       $is_spot
        }
        + (if $ssh_key_ids == "" then {}
           else {ssh_key_ids: ($ssh_key_ids | split(","))} end)'
)"

echo "Creating $( $IS_SPOT && echo spot || echo on-demand ) instance '$INSTANCE_TYPE' from disk '$BOOT_DISK' in $LOCATION ..." >&2

HTTP_CODE="$(
    printf '%s' "$PAYLOAD" \
    | curl -sS -o /tmp/verda_resp.$$ -w '%{http_code}' -X POST "$API/instances" \
        -H "Authorization: Bearer $TOKEN" \
        -H 'Content-Type: application/json' \
        --data @-
)"
RESPONSE="$(cat /tmp/verda_resp.$$)"; rm -f /tmp/verda_resp.$$

if [[ "$HTTP_CODE" -ge 400 ]]; then
    echo "instance creation failed (HTTP $HTTP_CODE):" >&2
    echo "$RESPONSE" | jq . 2>/dev/null >&2 || echo "$RESPONSE" >&2
    exit 1
fi

echo "$RESPONSE" | jq . 2>/dev/null || echo "$RESPONSE"
