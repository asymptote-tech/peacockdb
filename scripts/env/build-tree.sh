#!/usr/bin/env bash
# Build and stage this tree for gpu.sh, on a machine with docker and outside an instance: stage-tree.sh
# in the build image, as an instance runs it (llm-wiki/build-test.md, "GPU hosts").
#
#   build-tree.sh [--cudf-source <dir>] [--cudf <v> | --image <tag>] [--registry <dir>] [--ccache <dir>] [--bench]
#
# --cudf-source is a git checkout of the third_party/cudf commit the tree pins, the tree's own by
# default. --registry and --ccache are kept between builds, ~/.cache/peacock/cargo-registry and
# ~/.cache/peacock/ccache by default. The image, peacockdb-build:cudf-<v> (25.02 by default) or
# --image's tag, is pulled and checked against the tree. --bench is stage-tree.sh's. Interrupted, it
# kills its container.
[ "${BASH_VERSINFO[0]}" -gt 4 ] || { [ "${BASH_VERSINFO[0]}" -eq 4 ] && [ "${BASH_VERSINFO[1]}" -ge 4 ]; } || {
  echo "${0##*/}: this machine's bash is $BASH_VERSION, older than 4.4: install a current one (on macOS, brew install bash)" >&2; exit 2; }
set -euo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/../lib/peacock-config.sh"
shopt -s inherit_errexit

die() { echo "build-tree.sh: $*" >&2; exit 2; }
fail() { echo "build-tree.sh: $*" >&2; exit 1; }
. "$here/../lib/image-inputs.sh"
. "$here/../lib/data-inputs.sh"
. "$here/../lib/machine.sh"
. "$here/../lib/portable.sh"
tree=$(cd "$here/../.." && pwd)
# The container's own paths, never the host's (MACHINE_TREE). HOME is a tmpfs: nothing in it outlives
# a step.
HOME_IN=/peacock/home CCACHE_IN=/peacock/ccache

source_dir="" cudf="" image="" registry="" ccache="" bench=""
take() { [ -z "${!1}" ] || die "one $2 per call"; printf -v "$1" %s "$3"; }
while [ $# -gt 0 ]; do
  case "$1" in
    -h|--help) peacock_usage "${BASH_SOURCE[0]}" ;;
    --cudf-source|--cudf|--image|--registry|--ccache)
      [ -n "${2:-}" ] || die "$1 needs a value"
      case "$1" in --cudf-source) var=source_dir ;; *) var=${1#--} ;; esac
      take "$var" "$1" "$2"
      shift 2 ;;
    --bench) take bench "$1" --bench; shift ;;
    -*) die "unknown flag $1" ;;
    *) die "no argument is positional: $1" ;;
  esac
done
[ -z "${PEACOCK_INSTANCE:-}" ] || die "this is instance $PEACOCK_INSTANCE, in the build image already: run scripts/env/stage-tree.sh"
command -v docker > /dev/null || die "no docker on this machine: the tree is built in the build image"
command -v flock > /dev/null || die "no flock on this machine: it keeps two fetches out of one registry"
if [ -n "$image" ]; then
  [[ "$image" =~ ^cudf-([0-9]+\.[0-9]+)(-[0-9a-f]{12})?$ ]] || die "not an image tag cudf-<v>[-<commit>]: $image"
  [ -z "$cudf" ] || die "the image $image names its cuDF version: drop --cudf"
  cudf=${BASH_REMATCH[1]}
else
  cudf=${cudf:-25.02}
  image=cudf-$cudf
fi
[ -f "$tree/docker/conda/cudf-$cudf.spec" ] || die "no cuDF $cudf: docker/conda has no cudf-$cudf.spec"

pin=$(machine_cudf_gitlink "$tree")
[[ "$pin" =~ ^[0-9a-f]{40}$ ]] || die "$tree pins no third_party/cudf commit"
source_dir=${source_dir:-$tree/third_party/cudf}
[ -d "$source_dir" ] || die "no cuDF source $source_dir: --cudf-source names a checkout of $pin"
source_dir=$(cd "$source_dir" && pwd -P)
# Its own toplevel: an uninitialised submodule's directory answers for the tree around it.
top=$(machine_git -c "safe.directory=$source_dir" -C "$source_dir" rev-parse --show-toplevel 2> /dev/null) || top=""
[ "$top" = "$source_dir" ] || die "$source_dir is no git checkout of cuDF: --cudf-source names one at $pin"
head=$(machine_git -c "safe.directory=$source_dir" -C "$source_dir" rev-parse HEAD)
[ "$head" = "$pin" ] || die "$source_dir is at $head, and the tree pins third_party/cudf at $pin"
for kind in registry ccache; do
  if [ -n "${!kind}" ]; then
    [ -d "${!kind}" ] || die "no $kind directory ${!kind}"
    printf -v "$kind" %s "$(cd "${!kind}" && pwd)"
  elif [ "$kind" = registry ]; then
    registry=$MACHINE_REGISTRY
  else
    ccache=$MACHINE_CCACHE
  fi
done
mkdir -p "$registry" "$ccache"
# cmake refuses a build directory configured at another path than the tree's in the container.
made=$(sed -n 's/^CMAKE_CACHEFILE_DIR:INTERNAL=//p' "$tree/cpp/build/CMakeCache.txt" 2> /dev/null) || made=""
[ -z "$made" ] || [ "$made" = "$MACHINE_TREE/cpp/build" ] \
  || die "$tree/cpp/build was configured at $made, and the build image builds the tree at $MACHINE_TREE: remove cpp/build"
# One build a tree: its container is named for it.
container=peacock-build-$(id -un)-$(printf %s "$tree" | sha256sum | cut -c1-12)
! docker container inspect "$container" > /dev/null 2>&1 \
  || die "$tree is being built already, in $container: wait for it, or docker kill $container"

ref=$MACHINE_IMAGE:$image
machine_image_checked "$ref" "$tree" "$cudf"
# The cuDF checkout may be another user's, and git refuses a repository another uid owns.
run=(docker run --rm --init --name "$container" --user "$MACHINE_USER" -e "HOME=$HOME_IN" -e "CCACHE_DIR=$CCACHE_IN"
     -e GIT_CONFIG_COUNT=1 -e GIT_CONFIG_KEY_0=safe.directory -e "GIT_CONFIG_VALUE_0=$MACHINE_TREE/third_party/cudf"
     --mount "type=tmpfs,dst=$HOME_IN"
     --mount "type=bind,src=$tree,dst=$MACHINE_TREE"
     --mount "type=bind,src=$source_dir,dst=$MACHINE_TREE/third_party/cudf,readonly"
     --mount "type=bind,src=$registry,dst=$IMAGE_REGISTRY"
     --mount "type=bind,src=$ccache,dst=$CCACHE_IN"
     -w "$MACHINE_TREE" "$ref")

# in_image <command>... — <command> in the build image. The client goes in the background, which
# ignores SIGINT, so a signal here reaches the trap at once, and the trap kills the container: the
# client passes on no signal a TTY would.
client=""
in_image() {
  local rc=0
  "${run[@]}" "$@" &
  client=$!
  wait "$client" || rc=$?
  client=""
  return "$rc"
}
# interrupted — the container killed, until its client is gone: the client may be making it still.
interrupted() {
  trap '' INT TERM
  while [ -n "$client" ] && kill -0 "$client" 2> /dev/null; do docker kill "$container" > /dev/null 2>&1 || sleep 0.5; done
  docker rm -f "$container" > /dev/null 2>&1 || true
  echo "build-tree.sh: interrupted; $container is gone" >&2
  exit 130
}
trap interrupted INT TERM

# The registry's lock, as machine_cargo_fetch takes it: cargo's own lies in each container.
exec {lock}> "$registry.lock"
if ! flock -n "$lock"; then
  echo "build-tree.sh: waiting: another fetch holds $registry.lock" >&2
  flock "$lock"
fi
in_image cargo fetch --locked || fail "cargo fetch in $ref failed (exit $?)"
exec {lock}>&-
in_image bash scripts/env/stage-tree.sh ${bench:+"$bench"} || fail "stage-tree.sh in $ref failed (exit $?)"
echo "staged $tree/cpp/install: gpu.sh <host> ship sends it"
