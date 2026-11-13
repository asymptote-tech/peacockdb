#!/usr/bin/env bash
# The device run on a Nebius VM, in the run image, over scripts/nebius.sh (llm-wiki/build-test.md,
# "Nebius host").
#
#   run-gpu.sh --instance-id <id> [--profile <name>] [--dir <i>] [--cudf <v>] <verb>
#
#   prepare                    ready the VM: Docker, the NVIDIA Container Toolkit and nsys if
#                                missing; the GPU's counters open to non-root; the build and run
#                                images, each refused unless its peacockdb.inputs label is this
#                                tree's; sf1, generated in the build image; sf40, the keys of
#                                ~/.config/peacock/peacock.env on stdin; the run image's /opt/cudf
#                                as ~/peacock/cudf-<v>/, for Nsight on the VM itself
#   ship [--install <dir>]     the staged install (this tree's cpp/install by default, which
#                                scripts/env/stage-install.sh makes) and the tree's committed
#                                testdata to ~/peacock/<i>/ on the VM, the datasets linked in
#   run <list|filter|bench>    the run image's run-tests.sh <mode> as container peacock-<i>, its
#                                log followed; PCK_TEST_FILTER, PEACOCK_BENCH_RESUME,
#                                PEACOCK_BENCH_CASE_TIMEOUT_S and PCK_WRITE_GPU_RESULT pass on
#                                when set
#   run <name> [--exact <test>]
#                              a binary staged in the install's rust-benchmarks/ (stage-install.sh's
#                                prototype_fused_benchmarks), run in the unchanged run image as its
#                                entrypoint with run-tests.sh's environment, --nocapture
#                                --test-threads=1, and <test> or PCK_TEST_FILTER as the filter;
#                                PEACOCK_BENCH_RESUME passes on when set. Judged as run-tests.sh
#                                judges: a failed exit, a segfault or `running 0 tests` is exit 1
#   pull                       testdata/benchmark-results/, testdata/calibration/ and the device's
#                                answers (testdata/goldens/*/gpu-result*.txt) into this tree, and
#                                the runs' logs into ~/.cache/peacock/run-gpu/<i>/
#
# --instance-id and --profile go, as given, to every scripts/nebius.sh call ($NEBIUS_SH), which
# picks the VM. --instance-id is required: without it nebius.sh means nebius-gpu, the shared L40S,
# which runs no sf40 and no benchmark. run-gpu.sh never starts or stops the VM: the caller does,
# with nebius.sh. <i> is this tree's directory name by default, <v> 25.02.
#
# Exit 0; 1 the run failed; 2 bad input (nebius.sh's 64 too), a stale image, a run in progress,
# or a step missing (prepare, ship); 3 the VM's driver is older than the run image needs; 4 restart
# the VM (nebius.sh stop --force, start), then prepare again; else nebius.sh's (75, 69).
set -euo pipefail
shopt -s inherit_errexit

usage() { sed -n '2,${/^#/!q;p;}' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'; exit 0; }
die() { echo "run-gpu.sh: $*" >&2; exit 2; }

here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(git -C "$here" rev-parse --show-toplevel)
NEBIUS=("${NEBIUS_SH:-$root/scripts/nebius.sh}")
dir=$(basename "$root") cudf=25.02 install="" mode="" instance="" staged="" exact=""
while [ $# -gt 0 ]; do
  case $1 in
    -h|--help) usage ;;
    --instance-id|--profile) [ -n "${2:-}" ] || die "$1 needs a value"; NEBIUS+=("$1" "$2")
                             [ "$1" = --profile ] || instance=$2; shift 2 ;;
    --dir) [ -n "${2:-}" ] || die "--dir needs a name"; dir=$2; shift 2 ;;
    --cudf) [ -n "${2:-}" ] || die "--cudf needs a version such as 25.02"; cudf=$2; shift 2 ;;
    -*) die "unknown option $1" ;;
    *) break ;;
  esac
done
verb=${1:-}
shift || true
case $verb in
  prepare|pull) [ $# -eq 0 ] || die "$verb takes no argument: $*" ;;
  ship)
    while [ $# -gt 0 ]; do
      case $1 in
        --install) [ -n "${2:-}" ] || die "--install needs a directory"; install=$2; shift 2 ;;
        *) die "unknown argument to ship: $1" ;;
      esac
    done ;;
  run)
    [ $# -ge 1 ] || die "run takes a mode, list, filter or bench, or a staged binary's name"
    case $1 in
      list|filter|bench) [ $# -eq 1 ] || die "run $1 takes no argument: ${*:2}"; mode=$1 ;;
      *)
        [[ "$1" =~ ^[A-Za-z0-9_]+$ ]] || die "run $1: neither list, filter, bench nor a binary's name"
        staged=$1
        shift
        while [ $# -gt 0 ]; do
          case $1 in
            --exact) [ -n "${2:-}" ] || die "--exact needs a test name"; exact=$2; shift 2 ;;
            *) die "unknown argument to run $staged: $1" ;;
          esac
        done
        [ -z "$exact" ] || [ -z "${PCK_TEST_FILTER:-}" ] \
          || die "--exact $exact and PCK_TEST_FILTER=$PCK_TEST_FILTER both name the tests: give one" ;;
    esac ;;
  *) die "unknown verb ${verb:-(none)}: prepare, ship, run or pull" ;;
esac
# Not cudf-*: ~/peacock/cudf-<v>/ is the run image's prefix.
[[ "$dir" =~ ^[A-Za-z0-9][A-Za-z0-9._-]*$ && "$dir" != cudf-* ]] \
  || die "--dir $dir: a directory name under ~/peacock/, of letters, digits, '.', '_' and '-', not cudf-*"
[ -f "$root/docker/conda/cudf-$cudf.spec" ] || die "no cuDF $cudf: docker/conda has no cudf-$cudf.spec"
[ -n "$instance" ] \
  || die "name the VM with --instance-id: without it nebius.sh means nebius-gpu, the shared L40S, which runs no sf40"

. "$root/scripts/lib/peacock-config.sh"
. "$root/scripts/lib/image-inputs.sh"
. "$root/scripts/lib/data-inputs.sh"
. "$root/scripts/lib/machine.sh"
BUILD_REF=$MACHINE_IMAGE:cudf-$cudf
RUN_REF=ghcr.io/asymptote-tech/peacockdb-run:cudf-$cudf
SF40_SETS=(tpch.sf40 tpcds.sf40)
# What prepare runs on the VM, at the same paths under ~/peacock/<i>/: machine_data_set and
# fetch-sf40.sh, with what each reads.
KIT=(scripts/env/fetch-sf40.sh scripts/env/sf40.sha256 scripts/lib/peacock-config.sh scripts/lib/s3.sh
     scripts/lib/pinned.sh scripts/lib/data-inputs.sh scripts/lib/machine.sh docker/pins.env
     testdata/generate_testdata.sh)

# --- dev's side: every call reaches the VM through nebius.sh -------------------------------------

# vm <command> [<stdin file>]: <command> on the VM; stdin is /dev/null unless named.
vm() {
  local rc=0
  "${NEBIUS[@]}" ssh "$1" < "${2:-/dev/null}" || rc=$?
  [ "$rc" -ne 64 ] || die "${NEBIUS[0]} refused its options: ${NEBIUS[*]:1}"
  return "$rc"
}

# on_vm <function> <arg…>: a function below, sent as text and run on the VM under bash, with
# set -euo pipefail and the helpers every one of them uses.
on_vm() {
  local fn=$1 script args=""
  shift
  script="set -euo pipefail; $(declare -f vm_docker vm_not_running "$fn"); $fn \"\$@\""
  [ $# -eq 0 ] || args=$(printf ' %q' "$@")
  vm "bash -c $(printf '%q' "$script") run-gpu.sh$args"
}

# Every rsync runs through `nebius.sh … rsh`, which drops rsync's host argument, `vm`.
to_vm() {  # to_vm <dest under ~> <rsync arguments and sources…>
  local dest=$1
  shift
  rsync -a --partial --timeout=90 -e "${NEBIUS[*]} rsh" "$@" "vm:$dest"
}
from_vm() {  # from_vm <source under ~> <local dest> [<rsync arguments…>]
  local src=$1 dest=$2
  shift 2
  rsync -a --partial --timeout=90 -e "${NEBIUS[*]} rsh" "$@" "vm:$src" "$dest"
}

# docker, for machine_image_checked and the label reads: the VM's. Nothing runs a container here.
# shellcheck disable=SC2032  # vm_docker's `sudo docker` runs the VM's binary, never this
docker() { on_vm vm_docker_run "$@"; }

# driver_checked: refuses, exit 3, a VM whose driver is older than the run image's
# peacockdb.driver-minimum, naming both.
driver_checked() {
  local need have rc=0
  need=$(docker image inspect --format '{{index .Config.Labels "peacockdb.driver-minimum"}}' "$RUN_REF") || rc=$?
  [ "$rc" -ne 1 ] || die "no $RUN_REF on the VM: run prepare first"
  [ "$rc" -eq 0 ] || exit "$rc"
  [ -n "$need" ] && [ "$need" != "<no value>" ] || die "$RUN_REF has no peacockdb.driver-minimum label"
  have=$(vm "nvidia-smi --query-gpu=driver_version --format=csv,noheader | head -1")
  if [ -z "$have" ] || [ "$(printf '%s\n' "$need" "$have" | sort -V | head -1)" != "$need" ]; then
    echo "run-gpu.sh: the VM's driver ${have:-(none: nvidia-smi answered nothing)} is older than $need, the oldest $RUN_REF runs on" >&2
    exit 3
  fi
}

# --- the VM's side: sent by on_vm, never run here --------------------------------------------------

# vm_docker: docker through sudo while the docker group the user joined is not yet in effect.
# shellcheck disable=SC2033  # `sudo docker` is the binary, as meant
vm_docker() {
  if ! docker info > /dev/null 2>&1; then docker() { sudo docker "$@"; }; fi
}

vm_docker_run() { vm_docker; docker "$@"; }

# vm_not_running <i>: refuses, exit 2, while container peacock-<i> runs; clears one that ended.
vm_not_running() {
  local name=peacock-$1 state
  state=$(docker ps -a --filter "name=^$name\$" --format '{{.State}}')
  case $state in
    "") ;;
    running|restarting|paused) echo "run-gpu.sh: $name is $state on the VM: a run is in progress" >&2; exit 2 ;;
    *) docker rm "$name" > /dev/null ;;
  esac
}

# vm_system: what the VM needs before any image: tools, Docker and the NVIDIA Container Toolkit,
# nsys, the data directory, and the GPU's counters open to non-root (exit 4 until a restart).
vm_system() {
  local data=${PEACOCK_DATA_DIR:-/var/lib/peacock} params=${PEACOCK_NVIDIA_PARAMS:-/proc/driver/nvidia/params}
  local packages=() tool keyring repo found nsys
  for tool in curl python3 flock gpg; do
    command -v "$tool" > /dev/null || packages=(curl ca-certificates python3 util-linux gnupg)
  done
  command -v docker > /dev/null || packages+=(docker.io)
  if [ ${#packages[@]} -gt 0 ]; then
    sudo apt-get update -q
    sudo apt-get install -yq "${packages[@]}"
  fi
  if ! docker info > /dev/null 2>&1; then sudo usermod -aG docker "$(id -un)"; fi
  vm_docker
  if ! command -v nvidia-ctk > /dev/null; then
    keyring=/usr/share/keyrings/nvidia-container-toolkit-keyring.gpg
    curl -fsSL https://nvidia.github.io/libnvidia-container/gpgkey | sudo gpg --dearmor --yes -o "$keyring"
    curl -fsSL https://nvidia.github.io/libnvidia-container/stable/deb/nvidia-container-toolkit.list \
      | sed "s#deb https://#deb [signed-by=$keyring] https://#" \
      | sudo tee /etc/apt/sources.list.d/nvidia-container-toolkit.list > /dev/null
    sudo apt-get update -q
    sudo apt-get install -yq nvidia-container-toolkit
  fi
  if ! docker info --format '{{json .Runtimes}}' | grep -q '"nvidia"'; then
    sudo nvidia-ctk runtime configure --runtime=docker
    sudo systemctl restart docker
  fi
  # nsys on PATH: a copy already on the VM (the CUDA toolkit's), else the CLI from NVIDIA's
  # devtools repository.
  if ! command -v nsys > /dev/null; then
    found=""
    for nsys in /opt/nvidia/nsight-systems*/*/bin/nsys /usr/local/cuda*/bin/nsys; do
      if [ -x "$nsys" ]; then found=$nsys; fi
    done
    if [ -z "$found" ]; then
      # shellcheck source=/dev/null
      repo=https://developer.download.nvidia.com/devtools/repos/ubuntu$(. /etc/os-release && echo "${VERSION_ID//./}")/$(dpkg --print-architecture)
      keyring=/usr/share/keyrings/nvidia-devtools.gpg
      curl -fsSL "$repo/nvidia.pub" | sudo gpg --dearmor --yes -o "$keyring"
      echo "deb [signed-by=$keyring] $repo/ /" | sudo tee /etc/apt/sources.list.d/nvidia-devtools.list > /dev/null
      sudo apt-get update -q
      sudo apt-get install -yq nsight-systems-cli
      for nsys in "$(command -v nsys || true)" /opt/nvidia/nsight-systems*/*/bin/nsys; do
        if [ -x "$nsys" ]; then found=$nsys; break; fi
      done
    fi
    [ -n "$found" ] || { echo "run-gpu.sh: no nsys on the VM, even after installing nsight-systems-cli" >&2; exit 1; }
    command -v nsys > /dev/null || sudo ln -sfn "$found" /usr/local/bin/nsys
  fi
  if [ ! -d "$data" ]; then sudo install -d -o "$(id -un)" -g "$(id -gn)" -m 2775 "$data"; fi
  # The module option takes effect when the nvidia module loads, from the initramfs: at boot.
  sudo install -m 644 /dev/stdin /etc/modprobe.d/peacock-profiling.conf \
    <<< 'options nvidia NVreg_RestrictProfilingToAdminUsers=0'
  if grep -qs '^RmProfilingAdminOnly: 1' "$params"; then
    sudo update-initramfs -u
    echo "run-gpu.sh: the GPU's performance counters are admin-only until the nvidia module reloads:" \
      "restart the VM (scripts/nebius.sh … stop --force, then start), then run prepare again" >&2
    exit 4
  fi
}

# vm_data_set <i> <build image>: sf1 for the tree's generator, in the build image (machine.sh).
vm_data_set() {
  cd "$HOME/peacock/$1"
  vm_docker
  . scripts/lib/data-inputs.sh
  . scripts/lib/machine.sh
  machine_data_set "$2" "$PWD"
  echo "==> sf1: $DATA_SET"
}

# vm_cudf <run image> <v>: the image's /opt/cudf as ~/peacock/cudf-<v>/, stamped with the image's
# id; kept while the stamp is the image's.
vm_cudf() {
  local ref=$1 dst=$HOME/peacock/cudf-$2 id cid lock
  vm_docker
  id=$(docker image inspect --format '{{.Id}}' "$ref")
  mkdir -p "$HOME/peacock"
  exec {lock}< "$HOME/peacock"
  flock "$lock"
  if [ -f "$dst/.image-id" ] && [ "$(cat "$dst/.image-id")" = "$id" ]; then
    echo "==> $dst: $ref's /opt/cudf, already"
    return 0
  fi
  rm -rf "$dst.partial"
  mkdir "$dst.partial"
  cid=$(docker create "$ref")
  docker cp "$cid:/opt/cudf/." "$dst.partial/"
  docker rm "$cid" > /dev/null
  echo "$id" > "$dst.partial/.image-id"
  rm -rf "$dst"
  mv "$dst.partial" "$dst"
  echo "==> $dst: $ref's /opt/cudf"
}

# vm_ship_ready <i> <dataset>:<path under the data dir>…: refuses while a run is in progress, or
# while a dataset is missing; links each into ~/peacock/<i>/testdata/ at its absolute path, which
# run mounts at its own path.
vm_ship_ready() {
  local base=$HOME/peacock/$1 data=${PEACOCK_DATA_DIR:-/var/lib/peacock} pair target
  vm_docker
  vm_not_running "$1"
  shift
  for pair in "$@"; do
    target=$data/${pair#*:}
    [ -d "$target" ] || { echo "run-gpu.sh: no $target on the VM: run prepare first" >&2; exit 2; }
  done
  mkdir -p "$base/cpp/install" "$base/testdata"
  for pair in "$@"; do ln -sfn "$data/${pair#*:}" "$base/testdata/${pair%%:*}"; done
}

# vm_run <i> <run image> <mode> [<NAME=value>…]: run-tests.sh <mode> as peacock-<i>; returns 0, 2
# as the run did, else 1.
vm_run() {
  local dir=$1 ref=$2 mode=$3 base=$HOME/peacock/$1 data=${PEACOCK_DATA_DIR:-/var/lib/peacock}
  local name=peacock-$1 envs=() pair code
  shift 3
  for pair in "$@"; do envs+=(-e "$pair"); done
  vm_docker
  vm_not_running "$dir"
  [ -f "$base/cpp/install/lib/libpeacock_gpu.so" ] || { echo "run-gpu.sh: no install in $base: ship first" >&2; exit 2; }
  mkdir -p "$base/logs"
  docker run -d --name "$name" --gpus all --user "$(id -u):$(id -g)" --hostname "$(hostname)" \
    -v "$base/cpp/install:/peacock/install:ro" -v "$base/testdata:/peacock/testdata" -v "$data:$data:ro" \
    "${envs[@]}" "$ref" "$mode" > /dev/null
  docker logs -f "$name" 2>&1 | tee "$base/logs/$mode.log"
  code=$(docker wait "$name")
  docker rm "$name" > /dev/null
  case $code in 0|2) return "$code" ;; *) return 1 ;; esac
}

# vm_run_staged <i> <run image> <name> <NAME=value>… -- <argument>…: the install's
# rust-benchmarks/<name> as peacock-<i>'s entrypoint in the run image, with the run's mounts;
# refuses, exit 2, a name staged there as no executable. Returns 0, else 1, as run-tests.sh judges.
vm_run_staged() {
  local dir=$1 ref=$2 name=$3 base=$HOME/peacock/$1 data=${PEACOCK_DATA_DIR:-/var/lib/peacock}
  local container=peacock-$1 entry=/peacock/install/rust-benchmarks/$3 envs=() code log
  shift 3
  while [ "$1" != -- ]; do envs+=(-e "$1"); shift; done
  shift
  vm_docker
  vm_not_running "$dir"
  [ -x "$base/cpp/install/rust-benchmarks/$name" ] \
    || { echo "run-gpu.sh: no executable $base/cpp/install/rust-benchmarks/$name: stage and ship it first" >&2; exit 2; }
  mkdir -p "$base/logs"
  log=$base/logs/$name.log
  docker run -d --name "$container" --gpus all --user "$(id -u):$(id -g)" --hostname "$(hostname)" \
    -v "$base/cpp/install:/peacock/install:ro" -v "$base/testdata:/peacock/testdata" -v "$data:$data:ro" \
    -e LD_LIBRARY_PATH=/peacock/install/lib:/opt/cudf/lib -e PEACOCK_TESTDATA_DIR=/peacock/testdata \
    "${envs[@]}" --entrypoint "$entry" "$ref" "$@" > /dev/null
  docker logs -f "$container" 2>&1 | tee "$log"
  code=$(docker wait "$container")
  docker rm "$container" > /dev/null
  if [ "$code" -ne 0 ]; then echo "run-gpu.sh: $name exited $code" >&2; return 1; fi
  if grep -qE 'Segmentation fault|core dumped' "$log"; then echo "run-gpu.sh: $name: a segfault in its log" >&2; return 1; fi
  if grep -qE '^running 0 tests' "$log"; then echo "run-gpu.sh: $name ran 0 tests" >&2; return 1; fi
}

# vm_pull_ready <i>: refuses, exit 2, before anything was shipped; makes what pull reads.
vm_pull_ready() {
  local base=$HOME/peacock/$1
  [ -d "$base/testdata" ] || { echo "run-gpu.sh: nothing shipped to $base: ship first" >&2; exit 2; }
  mkdir -p "$base/testdata/benchmark-results" "$base/testdata/calibration" "$base/logs"
}

# --- the verbs -----------------------------------------------------------------------------------

cmd_prepare() {
  [ -f "$PEACOCK_ENV" ] || die "no $PEACOCK_ENV: the storage keys (llm-wiki/build-test.md, \"The env file\")"
  peacock_env_has_storage "$PEACOCK_ENV"
  vm "mkdir -p ~/peacock/$dir"
  (cd "$root" && to_vm "peacock/$dir/" -R "${KIT[@]}")
  on_vm vm_system
  machine_image_checked "$BUILD_REF" "$root" "$cudf"
  machine_image_checked "$RUN_REF" "$root" "$cudf" run
  driver_checked
  on_vm vm_data_set "$dir" "$BUILD_REF"
  vm "cd ~/peacock/$dir && scripts/env/fetch-sf40.sh --env - ${SF40_SETS[*]}" "$PEACOCK_ENV"
  on_vm vm_cudf "$RUN_REF" "$cudf"
  echo "==> prepare: the VM is ready for $RUN_REF"
}

cmd_ship() {
  local links=() entry ds
  install=${install:-$root/cpp/install}
  [ -d "$install" ] || die "no staged install at $install: make one with scripts/env/stage-install.sh, or name it with --install"
  for entry in "${DATA_SETS[@]}"; do
    ds=${entry%% *}
    links+=("$ds:$(data_inputs "$root")/$ds")
  done
  for ds in "${SF40_SETS[@]}"; do links+=("$ds:$ds"); done
  on_vm vm_ship_ready "$dir" "${links[@]}"
  to_vm "peacock/$dir/cpp/install/" --delete "$install/"
  git -C "$root" ls-files -z -- testdata | to_vm "peacock/$dir/" --from0 --files-from=- "$root/"
  echo "==> ship: $install and the committed testdata to ~/peacock/$dir/"
}

cmd_run() {
  local envs=()
  # driver_checked first: it refuses a VM without the run image as "run prepare first" (exit 2).
  driver_checked
  # The VM's run image as prepare left it: no pull, so a run never takes an image prepare did not
  # check, and a tree whose image inputs moved since is refused.
  machine_image_label_checked "$RUN_REF" "$root" "$cudf" run
  [ -z "${PCK_TEST_FILTER:-}" ] || envs+=("PCK_TEST_FILTER=$PCK_TEST_FILTER")
  [ -z "${PEACOCK_BENCH_RESUME:-}" ] || envs+=("PEACOCK_BENCH_RESUME=$PEACOCK_BENCH_RESUME")
  # The engine's two measurement switches (architecture.md, "cuDF options"): the cache's size and
  # the scan's reader.
  [ -z "${PEACOCK_PINNED_HOST_CACHE_BYTES:-}" ] || envs+=("PEACOCK_PINNED_HOST_CACHE_BYTES=$PEACOCK_PINNED_HOST_CACHE_BYTES")
  [ -z "${PEACOCK_SCAN_READER:-}" ] || envs+=("PEACOCK_SCAN_READER=$PEACOCK_SCAN_READER")
  [ -z "${PEACOCK_BENCH_CASE_TIMEOUT_S:-}" ] || envs+=("PEACOCK_BENCH_CASE_TIMEOUT_S=$PEACOCK_BENCH_CASE_TIMEOUT_S")
  [ -z "${PCK_WRITE_GPU_RESULT:-}" ] || envs+=("PCK_WRITE_GPU_RESULT=$PCK_WRITE_GPU_RESULT")
  on_vm vm_run "$dir" "$RUN_REF" "$mode" "${envs[@]}"
}

cmd_run_staged() {
  local envs=() args=(--nocapture --test-threads=1)
  driver_checked
  machine_image_label_checked "$RUN_REF" "$root" "$cudf" run
  [ -z "${PCK_TEST_FILTER:-}" ] || envs+=("PCK_TEST_FILTER=$PCK_TEST_FILTER")
  [ -z "${PEACOCK_BENCH_RESUME:-}" ] || envs+=("PEACOCK_BENCH_RESUME=$PEACOCK_BENCH_RESUME")
  if [ -n "$exact" ]; then args+=(--exact "$exact"); elif [ -n "${PCK_TEST_FILTER:-}" ]; then args+=("$PCK_TEST_FILTER"); fi
  on_vm vm_run_staged "$dir" "$RUN_REF" "$staged" "${envs[@]}" -- "${args[@]}"
}

cmd_pull() {
  local logs=${XDG_CACHE_HOME:-$HOME/.cache}/peacock/run-gpu/$dir
  on_vm vm_pull_ready "$dir"
  mkdir -p "$root/testdata/benchmark-results" "$root/testdata/calibration" "$logs"
  # --update: a file edited here since it was shipped stays as it is.
  from_vm "peacock/$dir/testdata/benchmark-results/" "$root/testdata/benchmark-results/" --update
  from_vm "peacock/$dir/testdata/calibration/" "$root/testdata/calibration/" --update
  from_vm "peacock/$dir/testdata/goldens/" "$root/testdata/goldens/" --update --prune-empty-dirs \
    --include='*/' --include='gpu-result*.txt' --exclude='*'
  from_vm "peacock/$dir/logs/" "$logs/"
  echo "==> pull: testdata/benchmark-results/, testdata/calibration/ and testdata/goldens/*/gpu-result*.txt; the runs' logs in $logs"
}

if [ -n "$staged" ]; then cmd_run_staged; else "cmd_$verb"; fi
