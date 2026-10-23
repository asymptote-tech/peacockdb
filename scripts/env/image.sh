#!/usr/bin/env bash
# Build or publish a peacockdb image for one cuDF version (llm-wiki/tasks/containers.md).
#
#   image.sh build <build|run> <cudf> [--dry-run]
#   image.sh push  <build|run> <cudf> [--dry-run]
#
# <cudf> is a version with a committed spec, docker/conda/cudf-<cudf>.spec. build passes every
# docker/pins.env line as a build arg and tags ghcr.io/asymptote-tech/peacockdb-<kind>:cudf-<cudf>;
# from a clean tree it also tags :cudf-<cudf>-<commit>. push publishes both tags, and only when
# HEAD's commit tag exists locally and the version tag names the same image. --dry-run prints
# the docker commands instead of running them.
set -euo pipefail

usage() { sed -n '2,${/^#/!q;p;}' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'; exit 0; }
die() { echo "image.sh: $*" >&2; exit 2; }
show() { local line; line=$(printf '%q ' "$@"); echo "${line% }"; }

dry_run=0
positional=()
for arg in "$@"; do
  case "$arg" in
    -h|--help) usage ;;
    --dry-run) dry_run=1 ;;
    -*) die "unknown flag $arg" ;;
    *) positional+=("$arg") ;;
  esac
done
[ ${#positional[@]} -eq 3 ] || die "want <build|push> <build|run> <cudf>, got: ${positional[*]:-nothing}"
verb=${positional[0]} kind=${positional[1]} cudf=${positional[2]}
case "$verb" in build|push) ;; *) die "unknown command $verb: build or push" ;; esac
case "$kind" in
  build) ;;
  run) die "the run image has no Dockerfile yet (docker/run.Dockerfile, Task 3)" ;;
  *) die "unknown image kind $kind: build or run" ;;
esac

root=$(git -C "$(dirname "${BASH_SOURCE[0]}")" rev-parse --show-toplevel)
known=$(cd "$root/docker/conda" && ls cudf-*.spec | sed 's/^cudf-//; s/\.spec$//' | tr '\n' ' ')
[ -f "$root/docker/conda/cudf-$cudf.spec" ] || die "no cuDF $cudf: docker/conda holds specs for ${known% }"

read -r mode submodule _ <<< "$(git -C "$root" ls-files -s third_party/cudf)"
[ "$mode" = 160000 ] || die "third_party/cudf is not a submodule in the index"
pin_args=()
while IFS= read -r line; do
  case "$line" in ''|'#'*) continue ;; esac
  [[ "$line" =~ ^[A-Z0-9_]+=[^[:space:]]+$ ]] || die "docker/pins.env: not NAME=value: $line"
  pin_args+=(--build-arg "$line")
done < "$root/docker/pins.env"

commit=$(git -C "$root" rev-parse --short=12 HEAD)
name=ghcr.io/asymptote-tech/peacockdb-$kind
tag=$name:cudf-$cudf
commit_tag=$tag-$commit

if [ "$verb" = build ]; then
  revision=$commit tags=(-t "$tag")
  if [ -z "$(git -C "$root" status --porcelain --untracked-files=normal)" ]; then
    tags+=(-t "$commit_tag")
  else
    revision=$commit-dirty
  fi
  cmd=(docker build -f "$root/docker/$kind.Dockerfile"
       --build-arg "CUDF=$cudf" --build-arg "CUDF_SUBMODULE=$submodule" "${pin_args[@]}"
       --label "org.opencontainers.image.revision=$revision" --label "org.opencontainers.image.version=cudf-$cudf"
       --label org.opencontainers.image.source=https://github.com/asymptote-tech/peacockdb
       "${tags[@]}" "$root")
  if [ "$dry_run" = 1 ]; then show "${cmd[@]}"; else "${cmd[@]}"; fi
  exit 0
fi

image_id() { docker image inspect --format '{{.Id}}' "$1" 2>/dev/null; }
commit_id=$(image_id "$commit_tag") || die "no local image $commit_tag: build it from a clean tree at HEAD"
version_id=$(image_id "$tag") || die "no local image $tag"
[ "$commit_id" = "$version_id" ] || die "$tag is not the image $commit_tag is: rebuild from a clean tree at HEAD"
for t in "$commit_tag" "$tag"; do
  if [ "$dry_run" = 1 ]; then show docker push "$t"; else docker push "$t"; fi
done
