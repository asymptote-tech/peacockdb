# shellcheck shell=bash
#
# A CPU VM's layout for the ensemble's containers, and the steps bootstrap-vm.sh and up.sh both
# take in the build image (llm-wiki/build-test.md, "Containers"). Every path comes from $HOME, and
# every container runs as the caller's uid:gid. Sourced after image-inputs.sh and data-inputs.sh.
# A step function (machine_image_checked, machine_cargo_fetch, machine_data_set,
# machine_cudf_source) is called as a statement of its own, never in `$(…)` or before `||`: set -e
# is what stops it at a failed step, and both turn it off inside. The others print a value for `$(…)`.

MACHINE_IMAGE=ghcr.io/asymptote-tech/peacockdb-build
MACHINE_USER=$(id -u):$(id -g)
MACHINE_DATA=${PEACOCK_DATA_DIR:-$HOME/peacock-data}
MACHINE_REGISTRY=$HOME/.cache/peacock/cargo-registry
MACHINE_CCACHE=$HOME/.cache/peacock/ccache
MACHINE_INSTANCES=$HOME/peacock-instances
# Where the build image's cargo keeps downloaded crates (CARGO_HOME=/opt/rust/cargo).
IMAGE_REGISTRY=/opt/rust/cargo/registry

# machine_git <arg>... — git on the machine. An instance writes the clone's .git, so a hook or an
# fsmonitor it planted there would run as the machine user on the next call: none ever runs.
machine_git() { git -c core.hooksPath=/dev/null -c core.fsmonitor=false "$@"; }

# machine_cudf_gitlink <tree> — the third_party/cudf commit <tree>'s index pins.
machine_cudf_gitlink() { machine_git -C "$1" ls-files -s third_party/cudf | awk '$1 == 160000 { print $2 }'; }

# machine_cudf_source <tree> — sets CUDF_SOURCE to the machine's checkout of the third_party/cudf
# commit <tree> pins, with the paths the build image's prefetch takes, which the FFI build's
# configure reads. One a commit, so an instance's never moves under it; fetched under a lock when
# the machine has none, and appearing only whole. A clone's own third_party/cudf is never read.
machine_cudf_source() {
  local tree=$1 commit url stage lock
  commit=$(machine_cudf_gitlink "$tree")
  url=$(machine_git config --file "$tree/.gitmodules" --get submodule.third_party/cudf.url || true)
  if [[ ! "$commit" =~ ^[0-9a-f]{40}$ ]] || [ -z "$url" ]; then
    echo "${0##*/}: $tree pins no third_party/cudf commit, or its .gitmodules names no URL" >&2
    return 2
  fi
  CUDF_SOURCE=$MACHINE_DATA/cudf/$commit
  mkdir -p "$MACHINE_DATA/cudf"
  exec {lock}> "$MACHINE_DATA/cudf/.$commit.lock"
  flock "$lock"
  if [ ! -d "$CUDF_SOURCE" ]; then
    stage=$CUDF_SOURCE.partial
    rm -rf "$stage"
    machine_git init -q "$stage"
    if ! machine_git -C "$stage" fetch -q --depth 1 --filter=blob:none "$url" "$commit"; then
      rm -rf "$stage"
      echo "${0##*/}: cannot fetch third_party/cudf $commit from $url" >&2
      return 2
    fi
    machine_git -C "$stage" sparse-checkout set --no-cone /cmake/ /cpp/cmake/ /VERSION /RAPIDS_BRANCH
    machine_git -C "$stage" checkout -q FETCH_HEAD
    mv "$stage" "$CUDF_SOURCE"
  fi
  exec {lock}>&-
}

# machine_image_checked <ref> <tree> <cudf> — pulls the build image <ref>; refuses it, naming each
# input that differs, unless its peacockdb.inputs label is image_inputs over <tree>.
machine_image_checked() {
  local ref=$1 tree=$2 cudf=$3 label differ
  docker pull "$ref"
  label=$(docker image inspect --format '{{index .Config.Labels "peacockdb.inputs"}}' "$ref")
  if [ -z "$label" ] || [ "$label" = "<no value>" ]; then
    echo "${0##*/}: $ref has no peacockdb.inputs label: publish it again with scripts/env/image.sh" >&2
    return 2
  fi
  differ=$(image_inputs_differ "$label" "$tree" build "$cudf")
  if [ -n "$differ" ]; then
    echo "${0##*/}: $ref was built from other inputs than $tree: $(paste -sd' ' <<< "$differ")" >&2
    return 2
  fi
}

# machine_cargo_fetch <ref> <tree> — fills the machine's registry with what <tree>'s Cargo.lock
# names. The one step that writes it: an instance mounts it read-only, which is what makes it safe
# to share. cargo's own package lock lies in each container, so the machine's lock is what keeps
# two fetches from writing the registry at once.
machine_cargo_fetch() {
  local lock
  mkdir -p "$MACHINE_REGISTRY"
  exec {lock}> "$MACHINE_REGISTRY.lock"
  flock "$lock"
  docker run --rm --user "$MACHINE_USER" \
    --mount "type=bind,src=$2,dst=$2,readonly" --mount "type=bind,src=$MACHINE_REGISTRY,dst=$IMAGE_REGISTRY" \
    -w "$2" "$1" cargo fetch --locked
  exec {lock}>&-
}

# machine_data_set <ref> <tree> — sets DATA_SET to the machine's datasets for <tree>'s generator
# inputs, generating them first, offline, when the machine has none. Under a lock, so two callers
# generate a set once; a set appears only whole, renamed from its .partial when done.
machine_data_set() {
  local ref=$1 tree=$2 hash stage entry words lock
  hash=$(data_inputs "$tree")
  DATA_SET=$MACHINE_DATA/$hash
  mkdir -p "$MACHINE_DATA"
  exec {lock}> "$MACHINE_DATA/.$hash.lock"
  flock "$lock"
  if [ ! -d "$DATA_SET" ]; then
    stage=$DATA_SET.partial
    rm -rf "$stage"
    mkdir "$stage"
    cp "$tree/testdata/generate_testdata.sh" "$stage/"
    for entry in "${DATA_SETS[@]}"; do
      read -r -a words <<< "$entry"
      docker run --rm --user "$MACHINE_USER" --network none -e PEACOCK_NO_FETCH=1 \
        --mount "type=bind,src=$stage,dst=$stage" -w "$stage" "$ref" "$stage/generate_testdata.sh" "${words[@]:1}"
      if [ ! -d "$stage/${words[0]}" ]; then
        echo "${0##*/}: the generator made no ${words[0]} in $stage" >&2
        return 2
      fi
    done
    rm -rf "$stage/generate_testdata.sh" "$stage/.tmp"
    mv "$stage" "$DATA_SET"
  fi
  exec {lock}>&-
}
