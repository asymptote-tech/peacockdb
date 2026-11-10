# shellcheck shell=bash
#
# A CPU VM's layout, and the steps bootstrap-vm.sh takes in the build image (llm-wiki/build-test.md,
# "CPU VMs"). Every path but the data directory comes from $HOME, and every container runs as
# the caller's uid:gid. Sourced after image-inputs.sh and data-inputs.sh.
# A step function (machine_image_checked, machine_image_label_checked, machine_image_inputs_checked,
# machine_cargo_fetch, machine_data_set, machine_cudf_source) is called as a statement of its own,
# never in `$(…)` or before `||`: set -e is what stops it at a failed step, and both turn it off
# inside. The others print a value for `$(…)`.

MACHINE_IMAGE=ghcr.io/asymptote-tech/peacockdb-build
MACHINE_USER=$(id -u):$(id -g)
# The machine's developers share it: group peacock, setgid, made by bootstrap-vm.sh. What is
# written there is written under umask 002, so another developer can take a lock or clear a
# crashed .partial left by the first.
MACHINE_DATA=${PEACOCK_DATA_DIR:-/var/lib/peacock}
MACHINE_REGISTRY=$HOME/.cache/peacock/cargo-registry
# Where the build image's cargo keeps downloaded crates (CARGO_HOME=/opt/rust/cargo).
IMAGE_REGISTRY=/opt/rust/cargo/registry
# Where a step's container sees the tree and the stage: a host path can cross the image's
# /home/peacock, mode 750, which only uid 1000 enters.
MACHINE_TREE=/peacock/tree
MACHINE_STAGE=/peacock/stage

# machine_cudf_gitlink <tree> — the third_party/cudf commit <tree>'s index pins.
machine_cudf_gitlink() { git -C "$1" ls-files -s third_party/cudf | awk '$1 == 160000 { print $2 }'; }

# machine_cudf_source <tree> — sets CUDF_SOURCE to the machine's checkout of the third_party/cudf
# commit <tree> pins, with the paths the build image's prefetch takes, which the FFI build's
# configure reads. One a commit, so a build's never moves under it; fetched under a lock when the
# machine has none, and appearing only whole. A clone's own third_party/cudf is never read.
machine_cudf_source() {
  local tree=$1 commit url stage lock mask
  commit=$(machine_cudf_gitlink "$tree")
  url=$(git config --file "$tree/.gitmodules" --get submodule.third_party/cudf.url || true)
  if [[ ! "$commit" =~ ^[0-9a-f]{40}$ ]] || [ -z "$url" ]; then
    echo "${0##*/}: $tree pins no third_party/cudf commit, or its .gitmodules names no URL" >&2
    return 2
  fi
  CUDF_SOURCE=$MACHINE_DATA/cudf/$commit
  mask=$(umask)
  umask 002
  mkdir -p "$MACHINE_DATA/cudf"
  exec {lock}> "$MACHINE_DATA/cudf/.$commit.lock"
  flock "$lock"
  if [ ! -d "$CUDF_SOURCE" ]; then
    stage=$CUDF_SOURCE.partial
    rm -rf "$stage"
    git init -q "$stage"
    if ! git -C "$stage" fetch -q --depth 1 --filter=blob:none "$url" "$commit"; then
      rm -rf "$stage"
      echo "${0##*/}: cannot fetch third_party/cudf $commit from $url" >&2
      return 2
    fi
    git -C "$stage" sparse-checkout set --no-cone /cmake/ /cpp/cmake/ /VERSION /RAPIDS_BRANCH
    git -C "$stage" checkout -q FETCH_HEAD
    mv "$stage" "$CUDF_SOURCE"
  fi
  exec {lock}>&-
  umask "$mask"
}

# machine_image_checked <ref> <tree> <cudf> [<kind>] — pulls the image <ref>, of <kind> build (the
# default) or run, then machine_image_label_checked. A failed pull names the images.yml dispatch
# that publishes <tree>'s image: return 2 when the registry has no such tag, else docker's status (a
# network error, or the caller's ssh). docker's stderr goes to a file, not `$(…)`, so a docker
# wrapper's own exit still ends the script; an EXIT trap, ahead of the caller's own, then prints
# the file and removes it. The trap runs inside the pull's `2>`, so it writes to a copy of stderr.
machine_image_checked() {
  local ref=$1 tree=$2 cudf=$3 kind=${4:-build} err rc=0 unpublished=0 branch publish caller_trap=() stderr
  err=$(mktemp)
  exec {stderr}>&2
  eval "caller_trap=($(trap -p EXIT))"  # trap -- '<command>' EXIT, or nothing
  # shellcheck disable=SC2064  # expanded now: the file, the fd and the caller's command are this call's
  trap "{ cat ${err@Q} >&2; rm -f ${err@Q}; ${caller_trap[2]:-}
} 2>&$stderr" EXIT
  docker pull "$ref" 2> "$err" || rc=$?
  cat "$err" >&2
  grep -qE ': not found$|manifest unknown' "$err" && unpublished=1
  rm -f "$err"
  if [ "${#caller_trap[@]}" -gt 0 ]; then trap -- "${caller_trap[2]}" EXIT; else trap - EXIT; fi
  exec {stderr}>&-
  if [ "$rc" -ne 0 ]; then
    branch=$(git -C "$tree" rev-parse --abbrev-ref HEAD 2> /dev/null || true)
    [ -n "$branch" ] && [ "$branch" != HEAD ] || branch="<this tree's branch>"
    publish="images.yml publishes this tree's $kind image for cuDF $cudf as \
${MACHINE_IMAGE%-build}-$kind:$(image_inputs_tag "$tree" "$kind" "$cudf"):
  gh workflow run images.yml --ref $branch -f kind=$kind -f cudf=$cudf"
    if [ "$unpublished" = 1 ]; then
      echo "${0##*/}: $ref is not published. $publish" >&2
      return 2
    fi
    echo "${0##*/}: cannot pull $ref: a registry or network error (above), so try again." \
      "If the image was never published, $publish" >&2
    return "$rc"
  fi
  machine_image_label_checked "$@"
}

# machine_image_label_checked <ref> <tree> <cudf> [<kind>] — the image <ref> as it is, no pull:
# machine_image_inputs_checked over its peacockdb.inputs label.
machine_image_label_checked() {
  local label
  label=$(docker image inspect --format '{{index .Config.Labels "peacockdb.inputs"}}' "$1")
  machine_image_inputs_checked "$label" "$@"
}

# machine_image_inputs_checked <label> <ref> <tree> <cudf> [<kind>] — refuses the image <ref>,
# naming each input that differs, unless <label>, its peacockdb.inputs label as read, is
# image_inputs over <tree>.
machine_image_inputs_checked() {
  local label=$1 ref=$2 tree=$3 cudf=$4 kind=${5:-build} differ
  if [ -z "$label" ] || [ "$label" = "<no value>" ]; then
    echo "${0##*/}: $ref has no peacockdb.inputs label: publish it again with scripts/env/image.sh" >&2
    return 2
  fi
  differ=$(image_inputs_differ "$label" "$tree" "$kind" "$cudf")
  if [ -n "$differ" ]; then
    echo "${0##*/}: $ref was built from other inputs than $tree: $(paste -sd' ' <<< "$differ")" >&2
    return 2
  fi
}

# machine_cargo_fetch <ref> <tree> — fills the machine's registry with what <tree>'s Cargo.lock
# names. cargo's own package lock lies in each container, so the machine's lock is what keeps two
# fetches from writing the registry at once.
machine_cargo_fetch() {
  local lock
  mkdir -p "$MACHINE_REGISTRY"
  exec {lock}> "$MACHINE_REGISTRY.lock"
  flock "$lock"
  docker run --rm --user "$MACHINE_USER" \
    --mount "type=bind,src=$2,dst=$MACHINE_TREE,readonly" --mount "type=bind,src=$MACHINE_REGISTRY,dst=$IMAGE_REGISTRY" \
    -w "$MACHINE_TREE" "$1" cargo fetch --locked
  exec {lock}>&-
}

# machine_data_set <ref> <tree> — sets DATA_SET to the machine's datasets for <tree>'s generator
# inputs, generating them first, offline, when the machine has none. Under a lock, so two callers
# generate a set once; a set appears only whole, renamed from its .partial when done.
machine_data_set() {
  local ref=$1 tree=$2 hash stage entry words lock mask
  hash=$(data_inputs "$tree")
  DATA_SET=$MACHINE_DATA/$hash
  mask=$(umask)
  umask 002
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
        --mount "type=bind,src=$stage,dst=$MACHINE_STAGE" -w "$MACHINE_STAGE" "$ref" \
        bash -c 'umask 002 && exec "$@"' bash "$MACHINE_STAGE/generate_testdata.sh" "${words[@]:1}"
      if [ ! -d "$stage/${words[0]}" ]; then
        echo "${0##*/}: the generator made no ${words[0]} in $stage" >&2
        return 2
      fi
    done
    rm -rf "$stage/generate_testdata.sh" "$stage/.tmp"
    mv "$stage" "$DATA_SET"
  fi
  exec {lock}>&-
  umask "$mask"
}
