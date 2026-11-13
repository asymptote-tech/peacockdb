#!/usr/bin/env bash
# Build or publish a peacockdb image for one cuDF version (llm-wiki/tasks/containers.md).
#
#   image.sh build <build|run|sirius> <cudf> [--dry-run]
#   image.sh push  <build|run|sirius> <cudf> [--moving] [--dry-run]
#
# A build or run <cudf> has a spec, docker/conda/cudf-<cudf>.spec. A version with no
# DRIVER_MINIMUM_<cudf> pin has a build image only. The sirius image is built at its extension's
# cuDF alone (SIRIUS_LIBCUDF), and no other kind is. build passes every docker/pins.env line as a
# build arg and labels the image peacockdb.inputs (scripts/lib/image-inputs.sh). A sirius build
# fetches the extension from the calibration bucket with AWS_ACCESS_KEY_ID and
# AWS_SECRET_ACCESS_KEY, both from the environment (CI) or else ~/.config/peacock/peacock.env,
# and the storage the bucket script names; docker gets them as BuildKit secrets, by name. It tags ghcr.io/asymptote-tech/
# peacockdb-<kind> three ways: :cudf-<cudf>; the inputs tag :cudf-<cudf>-in-<hash>; and
# :cudf-<cudf>-<commit>, from a clean tree only. push publishes the inputs and commit tags, and
# :cudf-<cudf> only with --moving. It pushes only when all three local tags name one image.
# --dry-run prints the docker commands instead of running them.
set -euo pipefail

usage() { sed -n '2,${/^#/!q;p;}' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'; exit 0; }
die() { echo "image.sh: $*" >&2; exit 2; }
show() { local line; line=$(printf '%q ' "$@"); echo "${line% }"; }

dry_run=0 moving=0
positional=()
for arg in "$@"; do
  case "$arg" in
    -h|--help) usage ;;
    --dry-run) dry_run=1 ;;
    --moving) moving=1 ;;
    -*) die "unknown flag $arg" ;;
    *) positional+=("$arg") ;;
  esac
done
[ ${#positional[@]} -eq 3 ] || die "want <build|push> <build|run> <cudf>, got: ${positional[*]:-nothing}"
verb=${positional[0]} kind=${positional[1]} cudf=${positional[2]}
case "$verb" in build|push) ;; *) die "unknown command $verb: build or push" ;; esac
[ "$verb" = push ] || [ "$moving" = 0 ] || die "--moving is a push flag"
case "$kind" in build|run|sirius) ;; *) die "unknown image kind $kind: build, run or sirius" ;; esac

root=$(git -C "$(dirname "${BASH_SOURCE[0]}")" rev-parse --show-toplevel)
. "$root/scripts/lib/image-inputs.sh"
refusal=$(image_refusal "$root" "$kind" "$cudf") || exit 2
[ -z "$refusal" ] || die "$refusal"

read -r mode submodule _ <<< "$(git -C "$root" ls-files -s third_party/cudf)"
[ "$mode" = 160000 ] || die "third_party/cudf is not a submodule in the index"
pin_args=()
while IFS= read -r line; do
  case "$line" in ''|'#'*) continue ;; esac
  [[ "$line" =~ ^[A-Z0-9_]+=[^[:space:]]+$ ]] || die "docker/pins.env: not NAME=value: $line"
  pin_args+=(--build-arg "$line")
done < "$root/docker/pins.env"

# The sirius build's storage keys: values stay in the environment docker reads them from.
secret_args=()
if [ "$verb" = build ] && [ "$kind" = sirius ]; then
  . "$root/scripts/lib/peacock-config.sh"
  if [ -z "${AWS_ACCESS_KEY_ID:-}" ] && [ -z "${AWS_SECRET_ACCESS_KEY:-}" ] && [ -f "$PEACOCK_ENV" ]; then
    while IFS= read -r line || [ -n "$line" ]; do
      line=${line%$'\r'}
      if [[ "$line" =~ ^([A-Z_]+)=(.*)$ ]] && [[ " ${PEACOCK_STORAGE_KEYS[*]} " == *" ${BASH_REMATCH[1]} "* ]]; then
        printf -v "${BASH_REMATCH[1]}" '%s' "${BASH_REMATCH[2]}"
      fi
    done < "$PEACOCK_ENV"
  fi
  for key in AWS_ACCESS_KEY_ID AWS_SECRET_ACCESS_KEY; do
    [ -n "${!key:-}" ] || die "the sirius build fetches with AWS_ACCESS_KEY_ID and AWS_SECRET_ACCESS_KEY, \
from the environment or $PEACOCK_ENV: no $key"
  done
  . "$root/scripts/lib/calibration-bucket.sh"
  AWS_ENDPOINT_URL=${CALIBRATION_S3[1]} AWS_REGION=${CALIBRATION_S3[3]}
  export AWS_ACCESS_KEY_ID AWS_SECRET_ACCESS_KEY AWS_ENDPOINT_URL AWS_REGION
  for key in AWS_ACCESS_KEY_ID AWS_SECRET_ACCESS_KEY AWS_ENDPOINT_URL AWS_REGION; do
    secret_args+=(--secret "id=${key,,},env=$key")
  done
fi

commit=$(git -C "$root" rev-parse --short=12 HEAD)
name=ghcr.io/asymptote-tech/peacockdb-$kind
tag=$name:cudf-$cudf
commit_tag=$tag-$commit
inputs_tag=$name:$(image_inputs_tag "$root" "$kind" "$cudf") || exit 2

if [ "$verb" = build ]; then
  inputs=$(image_inputs "$root" "$kind" "$cudf") || exit 2
  revision=$commit tags=(-t "$tag" -t "$inputs_tag")
  if [ -z "$(git -C "$root" status --porcelain --untracked-files=normal)" ]; then
    tags+=(-t "$commit_tag")
  else
    revision=$commit-dirty
  fi
  cmd=(docker build -f "$root/docker/$kind.Dockerfile"
       --build-arg "CUDF=$cudf" --build-arg "CUDF_SUBMODULE=$submodule" "${pin_args[@]}" "${secret_args[@]}"
       --label "org.opencontainers.image.revision=$revision" --label "org.opencontainers.image.version=cudf-$cudf"
       --label org.opencontainers.image.source=https://github.com/asymptote-tech/peacockdb
       --label "peacockdb.inputs=$(paste -sd' ' <<< "$inputs")"
       "${tags[@]}" "$root")
  if [ "$dry_run" = 1 ]; then show "${cmd[@]}"; else "${cmd[@]}"; fi
  exit 0
fi

image_id() { docker image inspect --format '{{.Id}}' "$1" 2>/dev/null; }
commit_id=$(image_id "$commit_tag") || die "no local image $commit_tag: build it from a clean tree at HEAD"
version_id=$(image_id "$tag") || die "no local image $tag"
[ "$commit_id" = "$version_id" ] || die "$tag is not the image $commit_tag is: rebuild from a clean tree at HEAD"
inputs_id=$(image_id "$inputs_tag") || die "no local image $inputs_tag: build it from a clean tree at HEAD"
[ "$commit_id" = "$inputs_id" ] || die "$inputs_tag is not the image $commit_tag is: rebuild from a clean tree at HEAD"
pushed=("$inputs_tag" "$commit_tag")
[ "$moving" = 0 ] || pushed+=("$tag")
for t in "${pushed[@]}"; do
  if [ "$dry_run" = 1 ]; then show docker push "$t"; else docker push "$t"; fi
done
