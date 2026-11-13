#!/usr/bin/env bash
# The device run on a Nebius VM, in the run image, over scripts/nebius.sh (llm-wiki/build-test.md,
# "Nebius host").
#
#   run-gpu.sh --instance-id <id> [--profile <name>] [--dir <i>] [--cudf <v>] <verb>
#
#   prepare [--no-sf40 | --sirius]
#                              ready the VM: Docker, the NVIDIA Container Toolkit and nsys if
#                                missing; the GPU's counters open to non-root; the build and run
#                                images, each refused unless its peacockdb.inputs label is this
#                                tree's; sf1, generated in the build image; sf40, the keys of
#                                ~/.config/peacock/peacock.env on stdin; the run image's /opt/cudf
#                                as ~/peacock/cudf-<v>/, for Nsight on the VM itself. --no-sf40,
#                                for the L40S list alone: no sf40, no env file, no nsys, counters
#                                or /opt/cudf copy. --sirius, then the Sirius image of this tree's
#                                inputs (PEACOCK_SIRIUS_IMAGE when set), checked alike, and the
#                                driver it needs (3); a VM without it short of its size and 10 GB
#                                under Docker's root is refused before the pull
#   ship [--install <dir>]     the staged install (this tree's cpp/install by default, which
#                                scripts/env/stage-install.sh makes) and the tree's committed
#                                testdata to ~/peacock/<i>/ on the VM, the datasets linked in; four
#                                rsync streams at once, each retried
#   run [--detach] <list|filter|bench>
#                              the run image's run-tests.sh <mode> as container peacock-<i>, its
#                                log followed, or, with --detach, left running; PCK_TEST_FILTER,
#                                PEACOCK_BENCH_RESUME, PEACOCK_BENCH_CASE_TIMEOUT_S and
#                                PCK_WRITE_GPU_RESULT pass on when set
#   run <name> [--exact <test>]
#                              a binary staged in the install's rust-benchmarks/ (stage-install.sh's
#                                prototype_fused_benchmarks), run in the unchanged run image as its
#                                entrypoint with run-tests.sh's environment, --nocapture
#                                --test-threads=1, and <test> or PCK_TEST_FILTER as the filter;
#                                PEACOCK_BENCH_RESUME passes on when set. Judged as run-tests.sh
#                                judges: a failed exit, a segfault or `running 0 tests` is exit 1
#   sirius [--detach] -- <docker/sirius-bench.py arguments>
#                              the Sirius runner in the Sirius image as container peacock-<i>, its
#                                log followed, or left running: no install, its prepare --sirius
#                                image as it is (no pull), testdata/<ds>-queries/ and the H100's
#                                results tree shipped (a newer file on the VM kept, for --resume),
#                                and for --list or --resume the dataset's Sirius answers database
#                                too, alike,
#                                the VM's sf40 data read-only at its own path as --data and the
#                                shipped testdata as --out; Docker's seccomp profile less its
#                                io_uring and NUMA refusals (docker/sirius-seccomp.json), memlock
#                                unlimited, the VM's hostname. Its exit: 0 or 2 as the runner's,
#                                else 1
#   status                     peacock-<i>: `running`, `exited <code>` or `none`, then the last 40
#                                lines of its log; an ended run's log is kept for pull
#   pull                       testdata/benchmark-results/, testdata/calibration/ and the device's
#                                answers (testdata/goldens/*/gpu-result*.txt) into this tree, and
#                                the runs' logs into ~/.cache/peacock/run-gpu/<i>/
#
# --instance-id and --profile go, as given, to every scripts/nebius.sh call ($NEBIUS_SH), which
# picks the VM. --instance-id is required, nebius-gpu's too: without it nebius.sh picks nebius-gpu
# silently. run-gpu.sh never starts or stops the VM: the caller does, with nebius.sh. <i> is this
# tree's directory name by default, <v> 25.02. prepare and run use the images of this tree's inputs,
# by their inputs tag (scripts/lib/image-inputs.sh), or PEACOCK_RUN_IMAGE and PEACOCK_BUILD_IMAGE
# when set (CI's scripts/env/image-ref.sh refs), checked alike.
#
# Exit 0; 1 the run failed, or a ship stream gave up; 2 bad input (nebius.sh's 64 too), a stale
# image, one not published (the images.yml dispatch named), a run in progress, or a step missing
# (prepare, ship); a pull that failed otherwise, docker's; 3 the VM's driver is older than the
# run image needs; 4 restart the VM (nebius.sh stop --force, start), then prepare again; 5 status:
# the run goes on; else nebius.sh's (75, 69). status: 0 exited 0, 1 exited non-zero, 2 none.
set -euo pipefail
shopt -s inherit_errexit

usage() { sed -n '2,${/^#/!q;p;}' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'; exit 0; }
die() { echo "run-gpu.sh: $*" >&2; exit 2; }

here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(git -C "$here" rev-parse --show-toplevel)
NEBIUS=("${NEBIUS_SH:-$root/scripts/nebius.sh}")
dir=$(basename "$root") cudf=25.02 install="" mode="" instance="" staged="" exact="" no_sf40=0 detach=0
sirius=0 sirius_ds="" sirius_sf="" sirius_keeps=0 runner=()
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
  pull|status) [ $# -eq 0 ] || die "$verb takes no argument: $*" ;;
  prepare)
    while [ $# -gt 0 ]; do
      case $1 in
        --no-sf40) no_sf40=1; shift ;;
        --sirius) sirius=1; shift ;;
        *) die "unknown argument to prepare: $1" ;;
      esac
    done
    [ "$no_sf40" -eq 0 ] || [ "$sirius" -eq 0 ] \
      || die "prepare --sirius --no-sf40: Sirius runs over sf40, which --no-sf40 leaves out" ;;
  sirius)
    if [ "${1:-}" = --detach ]; then detach=1; shift; fi
    [ "${1:-}" = -- ] || die "sirius takes [--detach] -- <docker/sirius-bench.py arguments>"
    shift
    runner=("$@")
    for ((i = 0; i < ${#runner[@]}; i++)); do
      case ${runner[i]} in
        --dataset) sirius_ds=${runner[i + 1]:-} ;;
        --sf) sirius_sf=${runner[i + 1]:-} ;;
        --list|--resume) sirius_keeps=1 ;;
        --data|--out) die "sirius: run-gpu.sh gives ${runner[i]} (the VM's sf40 data, the shipped testdata): drop it" ;;
      esac
    done
    [[ "$sirius_ds" =~ ^(tpch|tpcds)$ ]] || die "sirius: the runner's --dataset, tpch or tpcds, picks the VM's data: got ${sirius_ds:-none}"
    [[ "$sirius_sf" =~ ^[0-9]+$ ]] || die "sirius: the runner's --sf picks the VM's data: got ${sirius_sf:-none}" ;;
  ship)
    while [ $# -gt 0 ]; do
      case $1 in
        --install) [ -n "${2:-}" ] || die "--install needs a directory"; install=$2; shift 2 ;;
        *) die "unknown argument to ship: $1" ;;
      esac
    done ;;
  run)
    if [ "${1:-}" = --detach ]; then detach=1; shift; fi
    [ $# -ge 1 ] || die "run takes a mode, list, filter or bench, or a staged binary's name"
    case $1 in
      list|filter|bench) [ $# -eq 1 ] || die "run $1 takes no argument: ${*:2}"; mode=$1 ;;
      *)
        [ "$detach" -eq 0 ] || die "run --detach $1: --detach takes a mode, list, filter or bench; a staged binary runs attached"
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
  *) die "unknown verb ${verb:-(none)}: prepare, ship, run, sirius, status or pull" ;;
esac
# Not cudf-*: ~/peacock/cudf-<v>/ is the run image's prefix.
[[ "$dir" =~ ^[A-Za-z0-9][A-Za-z0-9._-]*$ && "$dir" != cudf-* ]] \
  || die "--dir $dir: a directory name under ~/peacock/, of letters, digits, '.', '_' and '-', not cudf-*"
[ -f "$root/docker/conda/cudf-$cudf.spec" ] || die "no cuDF $cudf: docker/conda has no cudf-$cudf.spec"
[ -n "$instance" ] \
  || die "name the VM with --instance-id, nebius-gpu's too: without it nebius.sh picks nebius-gpu silently"

. "$root/scripts/lib/peacock-config.sh"
. "$root/scripts/lib/image-inputs.sh"
. "$root/scripts/lib/data-inputs.sh"
. "$root/scripts/lib/machine.sh"
BUILD_REF=${PEACOCK_BUILD_IMAGE:-$MACHINE_IMAGE:$(image_inputs_tag "$root" build "$cudf")}
RUN_REF=${PEACOCK_RUN_IMAGE:-ghcr.io/asymptote-tech/peacockdb-run:$(image_inputs_tag "$root" run "$cudf")}
SHIP_PAUSE=${RUN_GPU_SHIP_PAUSE:-15}  # seconds, times the attempt; the tests set 0
SF40_SETS=(tpch.sf40 tpcds.sf40)
# What prepare runs on the VM, at the same paths under ~/peacock/<i>/: machine_data_set and
# fetch-sf40.sh, with what each reads.
# The Sirius container's seccomp profile goes with them, where `sirius` names it.
KIT=(scripts/env/fetch-sf40.sh scripts/env/sf40.sha256 scripts/lib/peacock-config.sh scripts/lib/s3.sh
     scripts/lib/pinned.sh scripts/lib/data-inputs.sh scripts/lib/machine.sh docker/pins.env
     testdata/generate_testdata.sh docker/sirius-seccomp.json)
# The Sirius image's disk use, measured 3.08 GB on 2026-11-10 (llm-wiki/tasks/sirius-bench-detail.md),
# and what prepare --sirius leaves free beside it.
SIRIUS_IMAGE_GB=4 SIRIUS_SPARE_GB=10

# sirius_image: SIRIUS_CUDF and SIRIUS_REF, the Sirius image of this tree's inputs or the given one.
sirius_image() {
  SIRIUS_CUDF=$(image_sirius_cudf "$root")
  SIRIUS_REF=${PEACOCK_SIRIUS_IMAGE:-ghcr.io/asymptote-tech/peacockdb-sirius:$(image_inputs_tag "$root" sirius "$SIRIUS_CUDF")}
}

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

# driver_checked [<image> <its prepare>]: refuses, exit 3, a VM whose driver is older than the
# image's peacockdb.driver-minimum, naming both; the run image and plain prepare by default.
driver_checked() {
  local ref=${1:-$RUN_REF} prepare=${2:-prepare} need have rc=0
  need=$(docker image inspect --format '{{index .Config.Labels "peacockdb.driver-minimum"}}' "$ref") || rc=$?
  [ "$rc" -ne 1 ] || die "no $ref on the VM: run $prepare first"
  [ "$rc" -eq 0 ] || exit "$rc"
  if [ -z "$need" ] || [ "$need" = "<no value>" ]; then die "$ref has no peacockdb.driver-minimum label"; fi
  have=$(vm "nvidia-smi --query-gpu=driver_version --format=csv,noheader | head -1")
  if [ -z "$have" ] || [ "$(printf '%s\n' "$need" "$have" | sort -V | head -1)" != "$need" ]; then
    echo "run-gpu.sh: the VM's driver ${have:-(none: nvidia-smi answered nothing)} is older than $need, the oldest $ref runs on" >&2
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
# and the data directory.
vm_system() {
  local data=${PEACOCK_DATA_DIR:-/var/lib/peacock} packages=() tool keyring
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
  if [ ! -d "$data" ]; then sudo install -d -o "$(id -un)" -g "$(id -gn)" -m 2775 "$data"; fi
}

# vm_nsight: what Nsight on the VM needs: nsys, and the GPU's counters open to non-root (exit 4
# until a restart).
vm_nsight() {
  local params=${PEACOCK_NVIDIA_PARAMS:-/proc/driver/nvidia/params} keyring repo found nsys
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

# vm_ship_ready <i> [?]<dataset>:<path under the data dir>…: refuses while a run is in progress, or
# while a dataset is missing, unless it is marked ? (sf40, which prepare --no-sf40 leaves out);
# links each one present into ~/peacock/<i>/testdata/ at its absolute path, which run mounts at its
# own path, and drops the link of one absent.
vm_ship_ready() {
  local base=$HOME/peacock/$1 data=${PEACOCK_DATA_DIR:-/var/lib/peacock} pair target name
  vm_docker
  vm_not_running "$1"
  shift
  for pair in "$@"; do
    target=$data/${pair#*:}
    [ -d "$target" ] || [[ $pair == '?'* ]] || { echo "run-gpu.sh: no $target on the VM: run prepare first" >&2; exit 2; }
  done
  mkdir -p "$base/cpp/install" "$base/testdata"
  for pair in "$@"; do
    target=$data/${pair#*:} name=${pair%%:*}
    name=${name#\?}
    if [ -d "$target" ]; then ln -sfn "$target" "$base/testdata/$name"; else rm -f "$base/testdata/$name"; fi
  done
}

# vm_run <i> <run image> <mode> <detach> [<NAME=value>…]: run-tests.sh <mode> as peacock-<i>;
# returns 0, 2 as the run did, else 1. Detached (1), returns once it is up, and vm_status judges it.
vm_run() {
  local dir=$1 ref=$2 mode=$3 detach=$4 base=$HOME/peacock/$1 data=${PEACOCK_DATA_DIR:-/var/lib/peacock}
  local name=peacock-$1 envs=() pair code
  shift 4
  for pair in "$@"; do envs+=(-e "$pair"); done
  vm_docker
  vm_not_running "$dir"
  [ -f "$base/cpp/install/lib/libpeacock_gpu.so" ] || { echo "run-gpu.sh: no install in $base: ship first" >&2; exit 2; }
  mkdir -p "$base/logs"
  docker run -d --name "$name" --label "peacock.log=$mode" --gpus all --user "$(id -u):$(id -g)" \
    --hostname "$(hostname)" -v "$base/cpp/install:/peacock/install:ro" -v "$base/testdata:/peacock/testdata" \
    -v "$data:$data:ro" "${envs[@]}" "$ref" "$mode" > /dev/null
  if [ "$detach" -eq 1 ]; then
    echo "==> $name: run-tests.sh $mode, detached; run-gpu.sh … status says how it goes"
    return 0
  fi
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
  docker run -d --name "$container" --label "peacock.log=$name" --gpus all --user "$(id -u):$(id -g)" \
    --hostname "$(hostname)" -v "$base/cpp/install:/peacock/install:ro" -v "$base/testdata:/peacock/testdata" -v "$data:$data:ro" \
    -e LD_LIBRARY_PATH=/peacock/install/lib:/opt/cudf/lib -e PEACOCK_TESTDATA_DIR=/peacock/testdata \
    "${envs[@]}" --entrypoint "$entry" "$ref" "$@" > /dev/null
  docker logs -f "$container" 2>&1 | tee "$log"
  code=$(docker wait "$container")
  docker rm "$container" > /dev/null
  if [ "$code" -ne 0 ]; then echo "run-gpu.sh: $name exited $code" >&2; return 1; fi
  if grep -qE 'Segmentation fault|core dumped' "$log"; then echo "run-gpu.sh: $name: a segfault in its log" >&2; return 1; fi
  if grep -qE '^running 0 tests' "$log"; then echo "run-gpu.sh: $name ran 0 tests" >&2; return 1; fi
}

# vm_docker_free: Docker's root directory and the bytes free under it.
vm_docker_free() {
  local root
  vm_docker
  root=$(docker info --format '{{.DockerRootDir}}')
  echo "$root $(df --output=avail -B1 "$root" | tail -1)"
}

# vm_sirius <i> <Sirius image> <detach> <ds> <sf> <runner argument>…: docker/sirius-bench.py as
# peacock-<i>, over <data>/<ds>.sf<sf>, writing the shipped testdata; returns 0 or 2 as it did,
# else 1. Detached (1), returns once it is up, and vm_status judges it.
vm_sirius() {
  local dir=$1 ref=$2 detach=$3 sets=${4}.sf${5} base=$HOME/peacock/$1 data=${PEACOCK_DATA_DIR:-/var/lib/peacock}
  local name=peacock-$1 profile=$HOME/peacock/$1/docker/sirius-seccomp.json code
  shift 5
  vm_docker
  vm_not_running "$dir"
  [ -f "$profile" ] || { echo "run-gpu.sh: no $profile on the VM: run prepare --sirius first" >&2; exit 2; }
  [ -d "$data/$sets" ] || { echo "run-gpu.sh: no $data/$sets on the VM: run prepare --sirius first" >&2; exit 2; }
  mkdir -p "$base/logs" "$base/testdata"
  docker run -d --name "$name" --label "peacock.log=sirius" --gpus all --user "$(id -u):$(id -g)" \
    --hostname "$(hostname)" --security-opt "seccomp=$profile" --ulimit memlock=-1:-1 \
    -v "$data:$data:ro" -v "$base/testdata:/peacock/testdata" \
    "$ref" python3 /opt/sirius/sirius-bench.py "$@" --data "$data/$sets" --out /peacock/testdata > /dev/null
  if [ "$detach" -eq 1 ]; then
    echo "==> $name: sirius-bench.py, detached; run-gpu.sh … status says how it goes"
    return 0
  fi
  docker logs -f "$name" 2>&1 | tee "$base/logs/sirius.log"
  code=$(docker wait "$name")
  docker rm "$name" > /dev/null
  case $code in 0|2) return "$code" ;; *) return 1 ;; esac
}

# vm_status <i>: peacock-<i>'s state and the last 40 lines of its log; returns 5 while it runs, 2
# when there is none, else 0 or 1 as it exited. An ended run's whole log goes to
# ~/peacock/<i>/logs/<its peacock.log label>.log, where pull finds it; the next run clears it.
vm_status() {
  local name=peacock-$1 log=$HOME/peacock/$1/logs state code
  vm_docker
  state=$(docker ps -a --filter "name=^$name\$" --format '{{.State}}')
  case $state in
    "") echo none; return 2 ;;
    running|restarting|paused) echo running; docker logs --tail 40 "$name" 2>&1; return 5 ;;
  esac
  code=$(docker inspect --format '{{.State.ExitCode}}' "$name")
  mkdir -p "$log"
  log=$log/$(docker inspect --format '{{index .Config.Labels "peacock.log"}}' "$name").log
  docker logs "$name" > "$log" 2>&1
  echo "exited $code"
  tail -n 40 "$log"
  [ "$code" -eq 0 ] || return 1
}

# vm_pull_ready <i>: refuses, exit 2, before anything was shipped; makes what pull reads.
vm_pull_ready() {
  local base=$HOME/peacock/$1
  [ -d "$base/testdata" ] || { echo "run-gpu.sh: nothing shipped to $base: ship first" >&2; exit 2; }
  mkdir -p "$base/testdata/benchmark-results" "$base/testdata/calibration" "$base/testdata/goldens" "$base/logs"
}

# --- the verbs -----------------------------------------------------------------------------------

cmd_prepare() {
  if [ "$no_sf40" -eq 0 ]; then
    [ -f "$PEACOCK_ENV" ] || die "no $PEACOCK_ENV: the storage keys (llm-wiki/build-test.md, \"The env file\")"
    peacock_env_has_storage "$PEACOCK_ENV"
  fi
  vm "mkdir -p ~/peacock/$dir"
  (cd "$root" && to_vm "peacock/$dir/" -R "${KIT[@]}")
  on_vm vm_system
  [ "$no_sf40" -eq 1 ] || on_vm vm_nsight
  machine_image_checked "$BUILD_REF" "$root" "$cudf"
  machine_image_checked "$RUN_REF" "$root" "$cudf" run
  driver_checked
  on_vm vm_data_set "$dir" "$BUILD_REF"
  if [ "$no_sf40" -eq 0 ]; then
    vm "cd ~/peacock/$dir && scripts/env/fetch-sf40.sh --env - ${SF40_SETS[*]}" "$PEACOCK_ENV"
    on_vm vm_cudf "$RUN_REF" "$cudf"
  fi
  [ "$sirius" -eq 0 ] || prepare_sirius
  echo "==> prepare: the VM is ready for $RUN_REF"
}

# prepare_sirius: the Sirius image, measured against the disk first when the VM lacks it.
prepare_sirius() {
  local free need=$((SIRIUS_IMAGE_GB + SIRIUS_SPARE_GB))
  sirius_image
  if ! docker image inspect --format '{{.Id}}' "$SIRIUS_REF" > /dev/null 2>&1; then
    read -r -a free <<< "$(on_vm vm_docker_free)"
    [ "$((free[1] / 1000000000))" -ge "$need" ] \
      || die "$((free[1] / 1000000000)) GB free under ${free[0]} on the VM; the Sirius image needs $need" \
             "($SIRIUS_IMAGE_GB GB, and $SIRIUS_SPARE_GB to spare)"
  fi
  machine_image_checked "$SIRIUS_REF" "$root" "$SIRIUS_CUDF" sirius
  driver_checked "$SIRIUS_REF" "prepare --sirius"
  echo "==> prepare: $SIRIUS_REF"
}

# ship_stream <name> <rsync arguments…>: one of ship's streams, tried 5 times, attempt × SHIP_PAUSE
# seconds apart; its output in $SHIP_LOGS/<name>. CI's runner reaches a VM over a long, lossy path:
# one rsync stream measured 0.24 MB/s on 2026-10-15, so a fresh 600 MB install took 40 minutes.
# So the install mirrors into the VM's persistent ~/peacock/<i>/ and only a change crosses, as
# rsync's delta; zstd on the wire; four streams at once, because each TCP stream backs off alone;
# --partial-dir, so a retry resumes a big file. --checksum, not size and mtime: a rebuilt file of
# the same size and second would be skipped, an unchanged one with a new mtime sent.
ship_stream() {
  local name=$1 attempt=1 t0=$SECONDS
  shift
  until rsync -a --checksum --timeout=120 --partial-dir=.rsync-partial --stats --compress \
          --compress-choice=zstd --compress-level=3 -e "${NEBIUS[*]} rsh" "$@" >> "$SHIP_LOGS/$name" 2>&1; do
    if [ "$attempt" -ge 5 ]; then
      echo "run-gpu.sh: ship: the $name stream failed $attempt times; its last lines:" >&2
      tail -5 "$SHIP_LOGS/$name" >&2
      return 1
    fi
    echo "run-gpu.sh: ship: $name stream: attempt $attempt failed; again in $((attempt * SHIP_PAUSE)) s" >&2
    sleep "$((attempt * SHIP_PAUSE))"
    attempt=$((attempt + 1))
  done
  echo "==> ship: $name stream: done in $((SECONDS - t0)) s; $(grep -h '^Total bytes sent' "$SHIP_LOGS/$name" | tail -1)"
}

cmd_ship() {
  local links=() entry ds sub dest pids=() pid failed=0
  install=${install:-$root/cpp/install}
  [ -d "$install" ] || die "no staged install at $install: make one with scripts/env/stage-install.sh, or name it with --install"
  for sub in lib rust-tests; do
    [ -d "$install/$sub" ] || die "no $install/$sub: not an install scripts/env/stage-install.sh staged"
  done
  for entry in "${DATA_SETS[@]}"; do
    ds=${entry%% *}
    links+=("$ds:$(data_inputs "$root")/$ds")
  done
  for ds in "${SF40_SETS[@]}"; do links+=("?$ds:$ds"); done
  on_vm vm_ship_ready "$dir" "${links[@]}"
  SHIP_LOGS=$(mktemp -d)
  trap 'rm -rf "$SHIP_LOGS"' EXIT
  # A file, not a pipe: a retry reads the list again.
  git -C "$root" ls-files -z -- testdata > "$SHIP_LOGS/testdata.files"
  dest=vm:peacock/$dir
  ship_stream lib --delete "$install/lib/" "$dest/cpp/install/lib/" & pids+=($!)
  ship_stream rust-tests --delete "$install/rust-tests/" "$dest/cpp/install/rust-tests/" & pids+=($!)
  # The rest of the install, bin/ and rust-benchmarks/; the excluded two are also kept from --delete.
  ship_stream install --delete --exclude=/lib/ --exclude=/rust-tests/ "$install/" "$dest/cpp/install/" & pids+=($!)
  ship_stream testdata --from0 --files-from="$SHIP_LOGS/testdata.files" "$root/" "$dest/" & pids+=($!)
  # Every stream finishes, so one failure does not kill another's transfer midway.
  for pid in "${pids[@]}"; do wait "$pid" || failed=1; done
  [ "$failed" -eq 0 ] || exit 1
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
  on_vm vm_run "$dir" "$RUN_REF" "$mode" "$detach" "${envs[@]}"
}

cmd_sirius() {
  local sub answers=testdata/calibration/sirius/$sirius_ds.sf$sirius_sf
  sirius_image
  # driver_checked first: it refuses a VM without the image as "run prepare --sirius first".
  driver_checked "$SIRIUS_REF" "prepare --sirius"
  machine_image_label_checked "$SIRIUS_REF" "$root" "$SIRIUS_CUDF" sirius
  vm "mkdir -p ~/peacock/$dir/testdata/benchmark-results ~/peacock/$dir/$answers"
  # --update: a file newer on the VM, a run's rows not yet pulled, stays for --resume.
  for sub in tpch-queries tpcds-queries; do
    if [ -d "$root/testdata/$sub" ]; then to_vm "peacock/$dir/testdata/" --update "$root/testdata/$sub"; fi
  done
  if [ -d "$root/testdata/benchmark-results/h100-80gb-hbm3" ]; then
    to_vm "peacock/$dir/testdata/benchmark-results/" --update "$root/testdata/benchmark-results/h100-80gb-hbm3"
  fi
  # A narrowed or resumed run keeps the answers of queries it does not run, and pull brings the
  # VM's database home over this one: so the VM starts from this one. A whole run removes it.
  if [ "$sirius_keeps" -eq 1 ] && [ -f "$root/$answers/answers.duckdb" ]; then
    to_vm "peacock/$dir/$answers/" --update "$root/$answers/answers.duckdb"
  fi
  on_vm vm_sirius "$dir" "$SIRIUS_REF" "$detach" "$sirius_ds" "$sirius_sf" "${runner[@]}"
}

cmd_status() { on_vm vm_status "$dir"; }

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
