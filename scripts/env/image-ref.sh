#!/usr/bin/env bash
# The image this tree builds or runs in, found by its inputs (llm-wiki/build-test.md, "Images").
#
#   image-ref.sh <build|run> <cudf>
#
# Prints one ref. The local ghcr.io/asymptote-tech/peacockdb-<kind>:<inputs tag> when docker has
# it, as that tag; else the published image of the tag, as peacockdb-<kind>@sha256:<digest>. The
# inputs tag is image_inputs_tag over this tree (scripts/lib/image-inputs.sh), so a branch that
# publishes images never moves what another tree resolves. Either way the image's peacockdb.inputs
# label must be this tree's.
#
# Exit 0; 2 bad input, a run image of a build-only version, no image of this tree's inputs, or one
# whose label differs (each differing input named).
set -euo pipefail

usage() { sed -n '2,${/^#/!q;p;}' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'; exit 0; }
die() { echo "image-ref.sh: $*" >&2; exit 2; }

positional=()
for arg in "$@"; do
  case "$arg" in
    -h|--help) usage ;;
    -*) die "unknown flag $arg" ;;
    *) positional+=("$arg") ;;
  esac
done
[ ${#positional[@]} -eq 2 ] || die "want <build|run> <cudf>, got: ${positional[*]:-nothing}"
kind=${positional[0]} cudf=${positional[1]}
case "$kind" in build|run) ;; *) die "unknown image kind $kind: build or run" ;; esac

root=$(git -C "$(dirname "${BASH_SOURCE[0]}")" rev-parse --show-toplevel)
known=$(cd "$root/docker/conda" && ls cudf-*.spec | sed 's/^cudf-//; s/\.spec$//' | tr '\n' ' ')
[ -f "$root/docker/conda/cudf-$cudf.spec" ] || die "no cuDF $cudf: docker/conda holds specs for ${known% }"
[ "$kind" = build ] || grep -q "^DRIVER_MINIMUM_${cudf//./_}=" "$root/docker/pins.env" \
  || die "cuDF $cudf has a build image only: docker/pins.env has no DRIVER_MINIMUM_${cudf//./_}"

. "$root/scripts/lib/image-inputs.sh"
name=ghcr.io/asymptote-tech/peacockdb-$kind
tag=$(image_inputs_tag "$root" "$kind" "$cudf") || exit 2

if label=$(docker image inspect --format '{{index .Config.Labels "peacockdb.inputs"}}' "$name:$tag" 2>/dev/null); then
  ref=$name:$tag
elif remote=$(docker buildx imagetools inspect "$name:$tag" --format '{{json .}}' 2>&1); then
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
    || die "cannot read the manifest docker buildx imagetools printed for $name:$tag"
  [[ "$digest" =~ ^sha256:[0-9a-f]{64}$ ]] || die "$name:$tag: no sha256 digest in its manifest: $digest"
  ref=$name@$digest
else
  branch=$(git -C "$root" rev-parse --abbrev-ref HEAD)
  die "no image of this tree's $kind inputs, $name:$tag, here or on ghcr.io (${remote##*$'\n'}).
  Build it here:  scripts/env/image.sh build $kind $cudf
  or publish it:  gh workflow run images.yml --ref $branch -f kind=$kind -f cudf=$cudf"
fi

if [ -z "$label" ] || [ "$label" = "<no value>" ]; then
  die "$ref has no peacockdb.inputs label: publish it again with scripts/env/image.sh"
fi
differ=$(image_inputs_differ "$label" "$root" "$kind" "$cudf") || exit 2
[ -z "$differ" ] || die "$ref was built from other inputs than this tree: $(paste -sd' ' <<< "$differ")"
echo "$ref"
