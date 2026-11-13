#!/usr/bin/env bash
# The image this tree builds or runs in, found by its inputs (llm-wiki/build-test.md, "Images").
#
#   image-ref.sh [--ref <ref>] <build|run|sirius> <cudf>
#
# Prints one ref. The local ghcr.io/asymptote-tech/peacockdb-<kind>:<inputs tag> when docker has
# it, as that tag; else the published image of the tag, as peacockdb-<kind>@sha256:<digest>. The
# inputs tag is image_inputs_tag over this tree (scripts/lib/image-inputs.sh), so a branch that
# publishes images never moves what another tree resolves. --ref looks up <ref> instead, such as
# $PEACOCK_BUILD_IMAGE, the same way. Either way the image's peacockdb.inputs label must be this
# tree's for <kind> and <cudf>.
#
# Exit 0; 2 bad input, a kind with no image at <cudf> (a build-only version's run image, the sirius
# image at any cuDF but its own), no image of this tree's inputs or no <ref>, or one whose label
# differs (each differing input named).
set -euo pipefail

usage() { sed -n '2,${/^#/!q;p;}' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'; exit 0; }
die() { echo "image-ref.sh: $*" >&2; exit 2; }

positional=() given=""
while [ $# -gt 0 ]; do
  case "$1" in
    -h|--help) usage ;;
    --ref) [ -n "${2:-}" ] || die "--ref needs an image ref"; given=$2; shift ;;
    -*) die "unknown flag $1" ;;
    *) positional+=("$1") ;;
  esac
  shift
done
[ ${#positional[@]} -eq 2 ] || die "want <build|run|sirius> <cudf>, got: ${positional[*]:-nothing}"
kind=${positional[0]} cudf=${positional[1]}
case "$kind" in build|run|sirius) ;; *) die "unknown image kind $kind: build, run or sirius" ;; esac

root=$(git -C "$(dirname "${BASH_SOURCE[0]}")" rev-parse --show-toplevel)
. "$root/scripts/lib/image-inputs.sh"
refusal=$(image_refusal "$root" "$kind" "$cudf") || exit 2
[ -z "$refusal" ] || die "$refusal"
if [ -n "$given" ]; then
  wanted=$given
  # The repository: the ref less its @digest, or less a :tag after its last slash.
  name=${given%%@*}
  [[ "${name##*/}" != *:* ]] || name=${name%:*}
else
  name=ghcr.io/asymptote-tech/peacockdb-$kind
  wanted=$name:$(image_inputs_tag "$root" "$kind" "$cudf") || exit 2
fi

if label=$(docker image inspect --format '{{index .Config.Labels "peacockdb.inputs"}}' "$wanted" 2>/dev/null); then
  ref=$wanted
elif remote=$(docker buildx imagetools inspect "$wanted" --format '{{json .}}' 2>&1); then
  # One line each: the manifest's digest, then the label (empty when the image has none).
  read_remote='
import json, sys
d = json.load(sys.stdin)
image = d.get("image") or {}
if "config" not in image:
    image = image.get("linux/amd64") or {}
print(d["manifest"]["digest"])
print(((image.get("config") or {}).get("Labels") or {}).get("peacockdb.inputs", ""))'
  { read -r digest; read -r label || label=""; } < <(python3 -c "$read_remote" <<< "$remote") \
    || die "cannot read the manifest docker buildx imagetools printed for $wanted"
  [[ "$digest" =~ ^sha256:[0-9a-f]{64}$ ]] || die "$wanted: no sha256 digest in its manifest: $digest"
  ref=$name@$digest
elif [ -n "$given" ]; then
  die "no image $given here or on its registry (${remote##*$'\n'})"
else
  branch=$(git -C "$root" rev-parse --abbrev-ref HEAD)
  die "no image of this tree's $kind inputs, $wanted, here or on ghcr.io (${remote##*$'\n'}).
  Build it here:  scripts/env/image.sh build $kind $cudf
  or publish it:  gh workflow run images.yml --ref $branch -f kind=$kind -f cudf=$cudf"
fi

if [ -z "$label" ] || [ "$label" = "<no value>" ]; then
  die "$ref has no peacockdb.inputs label: publish it again with scripts/env/image.sh"
fi
differ=$(image_inputs_differ "$label" "$root" "$kind" "$cudf") || exit 2
[ -z "$differ" ] || die "$ref was built from other inputs than this tree: $(paste -sd' ' <<< "$differ")"
echo "$ref"
