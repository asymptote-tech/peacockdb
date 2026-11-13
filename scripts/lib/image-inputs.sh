# shellcheck shell=bash
#
# What an image is made from, as `<input>=<value>` lines: each docker/ file its Dockerfile reads,
# with its sha256; the docker/pins.env lines it declares as ARG, hashed together, less those
# declared only in another cuDF version's stage (GCC_26_02 in `AS cudf-26.02`); and for the build
# image the two sources its prefetch fixes. image.sh writes them into the image's
# `peacockdb.inputs` label, one line joined by spaces; up.sh compares that label with a worktree's
# own, so an instance never runs an image built from other inputs than its tree. The sirius image
# has no conda spec: its extension links libcudf in, at the one cuDF image_sirius_cudf names.

# image_sirius_cudf <tree> — the sirius image's cuDF: major.minor of pins.env's SIRIUS_LIBCUDF.
image_sirius_cudf() {
  local line
  line=$(grep -E '^SIRIUS_LIBCUDF=[0-9]+\.[0-9]+\.[0-9]+$' "$1/docker/pins.env") \
    || { echo "image_sirius_cudf: $1/docker/pins.env has no SIRIUS_LIBCUDF=<major>.<minor>.<patch>" >&2; return 2; }
  line=${line#*=}
  echo "${line%.*}"
}

# image_refusal <tree> <build|run|sirius> <cudf> — one line saying why there is no such image, or
# nothing. A build image needs a conda spec, a run image also a DRIVER_MINIMUM_<cudf> pin; the
# sirius image is its own cuDF's alone, and no other kind is built at it.
image_refusal() {
  local tree=$1 kind=$2 cudf=$3 sirius known="" spec
  sirius=$(image_sirius_cudf "$tree") || return 2
  if [ "$kind" = sirius ]; then
    [ "$cudf" = "$sirius" ] || echo "the sirius image is cuDF $sirius's alone, not $cudf"
    return 0
  fi
  if [ "$cudf" = "$sirius" ]; then
    echo "cuDF $sirius is the sirius image's alone: no $kind image is built at it"
    return 0
  fi
  for spec in "$tree"/docker/conda/cudf-*.spec; do spec=${spec##*/cudf-}; known+="${spec%.spec} "; done
  if [ ! -f "$tree/docker/conda/cudf-$cudf.spec" ]; then
    echo "no cuDF $cudf: docker/conda holds specs for ${known% }"
  elif [ "$kind" = run ] && ! grep -q "^DRIVER_MINIMUM_${cudf//./_}=" "$tree/docker/pins.env"; then
    echo "cuDF $cudf has a build image only: docker/pins.env has no DRIVER_MINIMUM_${cudf//./_}"
  fi
}

# image_inputs <tree> <build|run|sirius> <cudf>
image_inputs() {
  local tree=$1 kind=$2 cudf=$3 f sum mode commit tags names spec versions=""
  local files=("docker/conda/cudf-$cudf.spec" "docker/$kind.Dockerfile" "docker/$kind.Dockerfile.dockerignore")
  case "$kind" in
    # The build image's nvcc comes from the 26.02 spec whatever its cuDF (build.Dockerfile).
    build) files+=(docker/venv.lock docker/cmake-wrapper.sh)
           [ "$cudf" = 26.02 ] || files+=(docker/conda/cudf-26.02.spec) ;;
    run) files+=(docker/runtime-spec.sh docker/run-tests.sh) ;;
    sirius) files=("docker/$kind.Dockerfile" "docker/$kind.Dockerfile.dockerignore" docker/sirius-venv.lock
                   docker/sirius-fetch.sh docker/sirius-config.yaml docker/sirius-bench.py docker/sirius-LICENSE) ;;
    *) echo "image_inputs: no image kind $kind" >&2; return 2 ;;
  esac
  for f in "${files[@]}"; do
    sum=$(sha256sum < "$tree/$f") || { echo "image_inputs: cannot read $tree/$f" >&2; return 2; }
    echo "$f=${sum%% *}"
  done
  for spec in "$tree"/docker/conda/cudf-*.spec; do spec=${spec##*/cudf-}; versions+=" ${spec%.spec}"; done
  names=$(awk -v own="$cudf" -v versions="$versions" '
            function ends(s, tail) {
              return length(s) >= length(tail) && substr(s, length(s) - length(tail) + 1) == tail
            }
            BEGIN { n = split(versions, v, " ") }
            toupper($1) == "FROM" { stage = toupper($3) == "AS" ? $4 : "" }
            $1 == "ARG" { name = $2; sub(/=.*/, "", name)
              for (i = 1; i <= n; i++) {
                suffix = v[i]; gsub(/\./, "_", suffix)
                if (v[i] != own && ends(name, "_" suffix) && ends(stage, "-" v[i])) next
              }
              print name }' "$tree/docker/$kind.Dockerfile" | sort -u | paste -sd'|')
  sum=$(grep -E "^($names)=" "$tree/docker/pins.env" | LC_ALL=C sort | sha256sum) || return 2
  echo "docker/pins.env=${sum%% *}"
  [ "$kind" = build ] || return 0
  read -r mode commit _ <<< "$(git -c core.hooksPath=/dev/null -c core.fsmonitor=false -C "$tree" \
                                 ls-files -s third_party/cudf)"
  [ "$mode" = 160000 ] || { echo "image_inputs: third_party/cudf is not a submodule in $tree's index" >&2; return 2; }
  echo "third_party/cudf=$commit"
  tags=$(awk '/^FetchContent_Declare\(/ { block = 1; name = ""; next }
              block && name == "" && NF { name = $1 }
              block && name == "flatbuffers" && $1 == "GIT_TAG" { print $2 }
              block && /\)/ { block = 0 }' "$tree/cpp/CMakeLists.txt") || return 2
  [[ "$tags" =~ ^[^[:space:]]+$ ]] \
    || { echo "image_inputs: cpp/CMakeLists.txt names no single flatbuffers GIT_TAG: ${tags//$'\n'/ }" >&2; return 2; }
  echo "cpp/CMakeLists.txt:flatbuffers=$tags"
}

# image_inputs_tag <tree> <build|run|sirius> <cudf> — the tag naming an image by its inputs,
# cudf-<cudf>-in-<h>: <h> is the first 12 hex digits of the sha256 of image_inputs' lines. A tree
# finds the image built from its own inputs by it, whatever :cudf-<cudf> points at.
image_inputs_tag() {
  local inputs sum
  inputs=$(image_inputs "$@") || return 2
  sum=$(printf '%s\n' "$inputs" | sha256sum)
  echo "cudf-$3-in-${sum:0:12}"
}

# image_inputs_differ <label> <tree> <kind> <cudf> — the inputs whose values differ between an
# image's label and the tree, one a line, sorted; an input on one side only differs too.
image_inputs_differ() {
  local want
  want=$(image_inputs "$2" "$3" "$4") || return 2
  awk -F= 'NR == FNR { tree[$1] = $2; next }
           { image[$1] = $2 }
           END {
             for (k in tree) if (!(k in image) || image[k] != tree[k]) print k
             for (k in image) if (!(k in tree)) print k
           }' <(echo "$want") <(tr ' ' '\n' <<< "$1" | grep -v '^$') | sort
}
