#!/usr/bin/env bash
# The device run on a Nebius VM, in the run image, over scripts/nebius.sh (llm-wiki/build-test.md,
# "Nebius host").
#
#   run-gpu.sh --instance-id <id> [--profile <name>] [--dir <i>] [--cudf <v>] <verb>
#
#   prepare [--no-sf40]        ready the VM: Docker, the NVIDIA Container Toolkit and nsys if
#                                missing; the GPU's counters open to non-root; the build and run
#                                images, each refused unless its peacockdb.inputs label is this
#                                tree's; sf1, generated in the build image; sf40, the keys of
#                                ~/.config/peacock/peacock.env on stdin; the run image's /opt/cudf
#                                as ~/peacock/cudf-<v>/, for Nsight on the VM itself. --no-sf40,
#                                for the L40S list alone: no sf40, no env file, no nsys, counters
#                                or /opt/cudf copy
#   ship [--install <dir>]     the staged install (this tree's cpp/install by default, which
#                                scripts/env/stage-install.sh makes) and the tree's committed
#                                testdata to ~/peacock/<i>/ on the VM, the datasets linked in; four
#                                rsync streams at once, each retried
#   run [--detach] <list|filter|bench>
#                              the run image's run-tests.sh <mode> as container peacock-<i>, held by
#                                a runner on the VM that outlives the session, its log followed, or,
#                                with --detach, not; PCK_TEST_FILTER, PEACOCK_BENCH_RESUME,
#                                PEACOCK_BENCH_CASE_TIMEOUT_S and PCK_WRITE_GPU_RESULT pass on when set
#   run <name> [--exact <test>]
#                              a binary staged in the install's rust-benchmarks/ (stage-install.sh's
#                                prototype_fused_benchmarks), run in the unchanged run image as its
#                                entrypoint with run-tests.sh's environment, --nocapture
#                                --test-threads=1, and <test> or PCK_TEST_FILTER as the filter;
#                                PEACOCK_BENCH_RESUME passes on when set. Judged as run-tests.sh
#                                judges: a failed exit, a segfault or `running 0 tests` is exit 1
#   follow                     the dir's run's log from its start, then live; its exit the run's
#   cancel                     ends the dir's run: its wait for the card given up, or its container
#                                stopped and removed; a container no runner holds removed
#   status                     the dir's run: `none`, `waiting` for the card, `running`, `exited
#                                <code>` or `ended without a status`, then the last 40 lines of its
#                                log, which ~/peacock/<i>/logs/<mode or name>-<stamp>.log keeps for pull
#   pull                       testdata/benchmark-results/, testdata/calibration/ and the device's
#                                answers (testdata/goldens/*/gpu-result*.txt) into this tree, and
#                                the runs' logs into ~/.cache/peacock/run-gpu/<i>/
#
# --instance-id and --profile go, as given, to every scripts/nebius.sh call ($NEBIUS_SH), which
# picks the VM. --instance-id is required, nebius-gpu's too: without it nebius.sh picks nebius-gpu
# silently. run-gpu.sh never starts or stops the VM: the caller does, with nebius.sh. <i> is this
# tree's directory name by default, <v> 25.02. prepare and run use the images of this tree's inputs,
# by their inputs tag (scripts/lib/image-inputs.sh), or PEACOCK_RUN_IMAGE and PEACOCK_BUILD_IMAGE
# when set (CI's scripts/env/image-ref.sh refs), checked alike. A run is given the run image by the
# ID read with its labels, so a tag moved since is not run. A run waits for the card's lock, one for
# every user of the VM (PEACOCK_CARD_LOCK), and holds it while its container lives. Ctrl-C, a TERM or
# a dropped ssh ends the following alone; follow takes it up again.
#
# Exit 0; 1 the run failed or ended with no status, a ship stream gave up, or ssh dropped while
# following; 2 bad input (nebius.sh's 64 too), a stale image, one not published (the images.yml
# dispatch named), a run in progress, a step missing (prepare, ship), or a driver query that fails or
# lists no GPU; a pull that failed otherwise, docker's; 3 a card below compute capability 8.0, or one
# whose driver is older than the run image needs; 4 restart the VM (nebius.sh stop --force, start),
# then prepare again; 5 status: the run goes on; 130 or 143 a run or follow interrupted, the run
# going on; else nebius.sh's (75, 69).
# status: 0 exited 0, 1 exited non-zero or ended without a status, 2 none. follow: as run, 2 no run.
# cancel: 0, or 1 when the run ended with no status or its container stays.
set -euo pipefail
shopt -s inherit_errexit

usage() { sed -n '2,${/^#/!q;p;}' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'; exit 0; }
die() { echo "run-gpu.sh: $*" >&2; exit 2; }

here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(git -C "$here" rev-parse --show-toplevel)
NEBIUS=("${NEBIUS_SH:-$root/scripts/nebius.sh}")
dir=$(basename "$root") cudf=25.02 install="" mode="" instance="" staged="" exact="" no_sf40=0 detach=0
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
  pull|status|follow|cancel) [ $# -eq 0 ] || die "$verb takes no argument: $*" ;;
  prepare)
    while [ $# -gt 0 ]; do
      case $1 in
        --no-sf40) no_sf40=1; shift ;;
        *) die "unknown argument to prepare: $1" ;;
      esac
    done ;;
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
  *) die "unknown verb ${verb:-(none)}: prepare, ship, run, follow, cancel, status or pull" ;;
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
. "$root/scripts/lib/ssh-retry.sh"
BUILD_REF=${PEACOCK_BUILD_IMAGE:-$MACHINE_IMAGE:$(image_inputs_tag "$root" build "$cudf")}
RUN_REF=${PEACOCK_RUN_IMAGE:-ghcr.io/asymptote-tech/peacockdb-run:$(image_inputs_tag "$root" run "$cudf")}
SHIP_PAUSE=${RUN_GPU_SHIP_PAUSE:-15}  # seconds, times the attempt; the tests set 0
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

# What every function on_vm sends runs with: the helpers, and what starts a runner.
VM_HELPERS=(vm_docker vm_state vm_started_at vm_status_code vm_container vm_name_free vm_name_taken vm_idle
            vm_locked vm_started vm_follow vm_runner vm_runner_ended vm_staged_judged peacock_card_take)

# on_vm <function> <arg…>: a function below, sent as text and run on the VM under bash, with
# set -euo pipefail and the helpers.
on_vm() {
  local fn=$1 script args=""
  shift
  script="set -euo pipefail; $(declare -f "${VM_HELPERS[@]}" "$fn"); $fn \"\$@\""
  [ $# -eq 0 ] || args=$(printf ' %q' "$@")
  vm "bash -c $(printf '%q' "$script") run-gpu.sh$args"
}

# Every rsync runs through `nebius.sh … rsh`, which drops rsync's host argument, `vm`. rsh is one
# attempt; the sync is retried whole.
to_vm() {  # to_vm <dest under ~> <rsync arguments and sources…>
  local dest=$1
  shift
  sync_retry rsync -a --partial --timeout=90 -e "${NEBIUS[*]} rsh" "$@" "vm:$dest"
}
from_vm() {  # from_vm <source under ~> <local dest> [<rsync arguments…>]
  local src=$1 dest=$2
  shift 2
  sync_retry rsync -a --partial --timeout=90 -e "${NEBIUS[*]} rsh" "$@" "vm:$src" "$dest"
}

# following <function> <arg…>: on_vm of a call that follows a run. Ctrl-C, a TERM or a dropped ssh
# (255) end the following alone, saying how to take it up again; the run goes on. The call runs in
# the background: bash runs a trap only once a foreground child ends, and a TERM may come to this
# script alone. It runs in a process group of its own (set -m), which the trap kills whole, a process
# it forks meanwhile included.
following() {
  local rc=0
  trap 'followed_no_more 130' INT
  trap 'followed_no_more 143' TERM
  set -m
  on_vm "$@" &
  follower=$!
  set +m
  wait "$follower" || rc=$?
  trap - INT TERM
  [ "$rc" -ne 255 ] || followed_no_more 1
  return "$rc"
}
followed_no_more() {
  if [ -n "${follower:-}" ]; then kill -TERM -- "-$follower" 2> /dev/null || true; fi
  echo "run-gpu.sh: no longer following; the run goes on, on the VM:" \
       "scripts/env/run-gpu.sh ${NEBIUS[*]:1} --dir $dir follow follows it" >&2
  exit "$1"
}

# docker, for machine_image_checked and the label reads: the VM's. Nothing runs a container here.
# shellcheck disable=SC2032  # vm_docker's `sudo docker` runs the VM's binary, never this
docker() { on_vm vm_docker_run "$@"; }

# run_image_read: sets RUN_ID to the ID of the image RUN_REF names on the VM, and RUN_MINIMUM and
# RUN_MINIMUM_PTX to its two driver labels, all from one read, so a tag moved since is never run.
# Refuses, exit 2, an image missing a label or built from other inputs than this tree.
run_image_read() {
  local labels=(peacockdb.inputs peacockdb.driver-minimum peacockdb.driver-minimum-ptx) fmt='{{.Id}}' out
  local values=() i rc=0
  for i in "${labels[@]}"; do fmt+="{{\"\\n\"}}{{index .Config.Labels \"$i\"}}"; done
  out=$(docker image inspect --format "$fmt" "$RUN_REF") || rc=$?
  [ "$rc" -ne 1 ] || die "no $RUN_REF on the VM: run prepare first"
  [ "$rc" -eq 0 ] || exit "$rc"
  # $(…) drops the lines of empty labels at the end, so each is read with a default.
  mapfile -t values <<< "$out"
  machine_image_inputs_checked "${values[1]:-}" "$RUN_REF" "$root" "$cudf" run
  for i in 2 3; do
    if [ -z "${values[i]:-}" ] || [ "${values[i]}" = "<no value>" ]; then
      die "$RUN_REF has no ${labels[i - 1]} label: publish it again with scripts/env/image.sh"
    fi
  done
  RUN_ID=${values[0]} RUN_MINIMUM=${values[2]} RUN_MINIMUM_PTX=${values[3]}
}

version_below() { [ "$1" != "$2" ] && [ "$(printf '%s\n' "$1" "$2" | sort -V | head -1)" = "$1" ]; }

# cards_checked: refuses, exit 3, a card below compute capability 8.0, which the run image's
# binaries do not run on, or one whose driver is older than RUN_MINIMUM. A card past 9.x
# JIT-compiles the image's PTX, so its driver is held to RUN_MINIMUM_PTX as well.
cards_checked() {
  local answer rc=0 line driver cap major need why
  # Both streams: a missing nvidia-smi is bash's `command not found`, on stderr.
  answer=$(vm "nvidia-smi --query-gpu=driver_version,compute_cap --format=csv,noheader 2>&1") || rc=$?
  case $rc in
    0) ;;
    69|75) exit "$rc" ;;  # nebius.sh's: the VM, not its card
    *) die "the driver query on the VM failed (exit $rc): ${answer:-no output}" ;;
  esac
  [ -n "$answer" ] || die "nvidia-smi on the VM lists no GPU"
  while IFS= read -r line; do
    [[ "$line" =~ ^([0-9]+(\.[0-9]+)+),\ (([0-9]+)\.[0-9]+)$ ]] \
      || die "nvidia-smi on the VM answered '$line', not a card's driver version and compute capability"
    driver=${BASH_REMATCH[1]} cap=${BASH_REMATCH[3]} major=${BASH_REMATCH[4]}
    if [ "$major" -lt 8 ]; then
      echo "run-gpu.sh: a card on the VM has compute capability $cap; $RUN_REF runs on 8.0 and later" >&2
      exit 3
    fi
    need=$RUN_MINIMUM why="the oldest $RUN_REF runs on"
    if [ "$major" -ne 8 ] && [ "$major" -ne 9 ] && version_below "$need" "$RUN_MINIMUM_PTX"; then
      need=$RUN_MINIMUM_PTX why="the oldest that JIT-compiles $RUN_REF's PTX for a card of compute capability $cap"
    fi
    if version_below "$driver" "$need"; then
      echo "run-gpu.sh: a card's driver on the VM is $driver, older than $need, $why" >&2
      exit 3
    fi
  done <<< "$answer"
}

# --- the VM's side: sent by on_vm, never run here --------------------------------------------------

# vm_docker: docker through sudo while the docker group the user joined is not yet in effect.
# shellcheck disable=SC2033  # `sudo docker` is the binary, as meant
vm_docker() {
  if ! docker info > /dev/null 2>&1; then docker() { sudo docker "$@"; }; fi
}

vm_docker_run() { vm_docker; docker "$@"; }

# A dir's run, in ~/peacock/<i>/: run/state, `<runner pid> <boot id> <what> <stamp> <start time>`,
# <what> the mode or the staged binary's name, written by the runner; its log,
# logs/<what>-<stamp>.log; run/status, `<stamp> <code>`, once it ended; run/lock, held around a
# run's start and a cancel.

# vm_state <i>: reads run/state into the caller's pid, boot, what, stamp and start, and going: 1 while
# the runner lives, else 0. The boot id and the start time tell it from another process the pid went
# to, after a restart or within one boot. 1 with no state.
vm_state() {
  local file=$HOME/peacock/$1/run/state
  [ -f "$file" ] || return 1
  read -r pid boot what stamp start < "$file"
  going=0
  if [ "$boot" = "$(cat /proc/sys/kernel/random/boot_id)" ] && [ -n "$start" ] \
     && [ "$(vm_started_at "$pid")" = "$start" ]; then
    going=1
  fi
}

# vm_started_at <pid>: the process's start time, in clock ticks since boot (/proc/<pid>/stat's field
# 22); nothing when there is no such process. Its name, field 2, may hold spaces, so fields are
# counted from its closing parenthesis.
vm_started_at() {
  local stat fields
  stat=$(cat "/proc/$1/stat" 2> /dev/null) || return 0
  read -ra fields <<< "${stat##*) }"
  echo "${fields[19]}"
}

# vm_status_code <i> <stamp>: the code the run of <stamp> ended with, when it has one.
vm_status_code() {
  local file=$HOME/peacock/$1/run/status st code
  [ -f "$file" ] || return 0
  read -r st code < "$file"
  if [ "$st" = "$2" ]; then echo "$code"; fi
}

# vm_container <i>: `<state> <dir>` of container peacock-<i>, nothing with none. Docker's names are
# the VM's, so another user's dir of the same name runs a container of this name: <dir> is its
# peacock.dir label, this dir with none (a container from before the label).
vm_container() {
  local line
  line=$(docker ps -a --filter "name=^peacock-$1\$" --format '{{.State}} {{.Label "peacock.dir"}}')
  [ -n "$line" ] || return 0
  line=${line% }
  if [ "${line#* }" = "$line" ]; then line+=" $HOME/peacock/$1"; fi
  echo "$line"
}

# vm_idle <i>: refuses, exit 2, while the dir's run goes, or while its container runs held by no
# runner (one killed); clears its container that ended, as a VM restart leaves it. Another dir's
# container of its name is vm_name_free's.
vm_idle() {
  local pid boot what stamp start going state owner
  if vm_state "$1" && [ "$going" -eq 1 ]; then
    echo "run-gpu.sh: the $what run started $stamp goes on in ~/peacock/$1 on the VM:" \
         "run-gpu.sh … follow follows it, … cancel ends it" >&2
    exit 2
  fi
  read -r state owner <<< "$(vm_container "$1")"
  if [ -z "$state" ] || [ "$owner" != "$HOME/peacock/$1" ]; then return 0; fi
  case $state in
    running|restarting|paused)
      echo "run-gpu.sh: peacock-$1 is $state on the VM, held by no run: run-gpu.sh … cancel removes it" >&2
      exit 2 ;;
    *) docker rm "peacock-$1" > /dev/null ;;
  esac
}

# vm_name_free <i>: refuses, exit 2, a run while another dir's container of its name stays after its
# run ended: the name is docker's, the VM's. One that runs is let be: the card lock orders the two
# runs, and that run's runner removes it.
vm_name_free() {
  local state owner
  read -r state owner <<< "$(vm_container "$1")"
  if [ -n "$state" ] && [ "$owner" != "$HOME/peacock/$1" ]; then
    case $state in running|restarting|paused) return 0 ;; esac
    vm_name_taken "$1" "$owner" >&2
    exit 2
  fi
}

vm_name_taken() {
  echo "run-gpu.sh: container peacock-$1 on the VM is $2's, which no run holds: its owner's" \
       "run-gpu.sh … cancel removes it; or name a dir of your own with --dir"
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

# vm_cudf <run image ID> <v>: the image's /opt/cudf as ~/peacock/cudf-<v>/, stamped with the ID;
# kept while the stamp is the ID.
vm_cudf() {
  local id=$1 dst=$HOME/peacock/cudf-$2 cid lock
  vm_docker
  mkdir -p "$HOME/peacock"
  exec {lock}< "$HOME/peacock"
  flock "$lock"
  if [ -f "$dst/.image-id" ] && [ "$(cat "$dst/.image-id")" = "$id" ]; then
    echo "==> $dst: the run image's /opt/cudf ($id), already"
    return 0
  fi
  rm -rf "$dst.partial"
  mkdir "$dst.partial"
  cid=$(docker create "$id")
  docker cp "$cid:/opt/cudf/." "$dst.partial/"
  docker rm "$cid" > /dev/null
  echo "$id" > "$dst.partial/.image-id"
  rm -rf "$dst"
  mv "$dst.partial" "$dst"
  echo "==> $dst: the run image's /opt/cudf ($id)"
}

# vm_ship_ready <i> [?]<dataset>:<path under the data dir>…: refuses while a run is in progress, or
# while a dataset is missing, unless it is marked ? (sf40, which prepare --no-sf40 leaves out);
# links each one present into ~/peacock/<i>/testdata/ at its absolute path, which run mounts at its
# own path, and drops the link of one absent.
vm_ship_ready() {
  local base=$HOME/peacock/$1 data=${PEACOCK_DATA_DIR:-/var/lib/peacock} pair target name
  vm_docker
  vm_idle "$1"
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

# vm_started <i> <what> <card lock> <docker run argument>…: under run/lock, once the dir is idle, a
# runner of <what> with the run's mounts, user and hostname and these arguments, detached from this
# session, and its log, empty. run/lock is let go once the runner wrote its state, its TERM trap set,
# so a cancel never finds a runner it cannot stop.
vm_started() {
  local dir=$1 what=$2 lock=$3 base=$HOME/peacock/$1 data=${PEACOCK_DATA_DIR:-/var/lib/peacock} stamp runner
  shift 3
  vm_docker
  mkdir -p "$base/run" "$base/logs"
  vm_locked "$dir"
  vm_idle "$dir"
  vm_name_free "$dir"
  stamp=$(date -u +%Y%m%dT%H%M%S.%3NZ)
  : > "$base/logs/$what-$stamp.log"
  runner="set -euo pipefail; $(declare -f vm_docker vm_container vm_started_at peacock_card_take vm_runner \
    vm_runner_ended vm_staged_judged vm_name_taken); vm_runner \"\$@\""
  # Every fd redirected: the session's pipes close when this call ends, and run/lock is let go.
  setsid nohup bash -c "$runner" run-gpu.sh "$dir" "$what" "$stamp" "$lock" \
    --user "$(id -u):$(id -g)" --hostname "$(hostname)" --label "peacock.dir=$base" \
    -v "$base/cpp/install:/peacock/install:ro" -v "$base/testdata:/peacock/testdata" -v "$data:$data:ro" \
    "$@" < /dev/null > /dev/null 2>&1 {VM_LOCK_FD}>&- &
  while [ "$(cut -d' ' -f4 "$base/run/state" 2> /dev/null)" != "$stamp" ]; do
    kill -0 "$!" 2> /dev/null || { echo "run-gpu.sh: the runner of $what ended before it started; its log is $base/logs/$what-$stamp.log" >&2; exit 1; }
    sleep 0.05
  done
  exec {VM_LOCK_FD}>&-
}

# vm_locked <i>: run/lock taken on fd VM_LOCK_FD; a wait for it is said.
vm_locked() {
  exec {VM_LOCK_FD}> "$HOME/peacock/$1/run/lock"
  if ! flock -n "$VM_LOCK_FD"; then
    echo "run-gpu.sh: waiting for ~/peacock/$1/run/lock: another call starts or cancels a run there" >&2
    flock "$VM_LOCK_FD"
  fi
}

# vm_runner <i> <what> <stamp> <card lock> <docker run argument>…: the run, detached: its output to
# its log; the card's lock, waited for and held while the container lives; the container removed;
# a staged binary judged; then its status. --init: run-tests.sh as PID 1 would ignore docker stop.
# cancel sends TERM until this process ends: each one cuts short the wait in hand, and the flag
# says what to do next, so a TERM at any point lands. The status is the client's, so a TERM that
# comes as the run ends keeps the run's own.
vm_runner() {
  local dir=$1 what=$2 stamp=$3 lock=$4 base=$HOME/peacock/$1 name=peacock-$1 cancelled=0 stopped=0 child=""
  local code="" rc=0 state owner
  shift 4
  exec >> "$base/logs/$what-$stamp.log" 2>&1
  rm -f "$base/run/exit"
  trap 'cancelled=1' TERM
  echo "$$ $(cat /proc/sys/kernel/random/boot_id) $what $stamp $(vm_started_at $$)" > "$base/run/state.new"
  mv "$base/run/state.new" "$base/run/state"
  vm_docker
  if [ "$cancelled" -eq 0 ]; then peacock_card_take "$lock" || rc=$?; fi
  if [ "$cancelled" -eq 1 ]; then
    kill "${PEACOCK_CARD_WAITER:-}" 2> /dev/null || true
  elif [ "$rc" -ne 0 ]; then
    code=1
  else
    # This dir's ended container was cleared at the start, a running one refused. One here now is
    # another dir's that its runner, dead, left behind: the card lock is ours.
    read -r state owner <<< "$(vm_container "$dir")"
    if [ -n "$state" ]; then
      vm_name_taken "$dir" "$owner"
      code=2
    else
      ( rc=0; docker run --init --name "$name" --gpus all "$@" || rc=$?; echo "$rc" > "$base/run/exit" ) &
      child=$!
      while kill -0 "$child" 2> /dev/null; do
        if [ "$cancelled" -eq 0 ]; then
          wait "$child" || true
        else
          stopped=1  # the client may still be making the container: stopped until the client ends
          docker stop -t 30 "$name" > /dev/null 2>&1 || true
          sleep 0.2
        fi
      done
    fi
  fi
  trap '' TERM
  if [ -z "$code" ]; then code=$(cat "$base/run/exit" 2> /dev/null) || code=143; fi
  read -r state owner <<< "$(vm_container "$dir")"
  if [ -n "$state" ] && [ "$owner" = "$base" ]; then docker rm -f "$name" > /dev/null 2>&1 || true; fi
  if [ "$cancelled" -eq 1 ] && { [ -z "$child" ] || [ "$stopped" -eq 1 ]; }; then
    echo "run-gpu.sh: cancelled"
  elif [ -n "$child" ]; then
    case $what in list|filter|bench) ;; *) vm_staged_judged ;; esac
  fi
  vm_runner_ended
}

vm_runner_ended() {
  echo "$stamp $code" > "$base/run/status.new"
  mv "$base/run/status.new" "$base/run/status"
}

# vm_staged_judged: a staged binary's code, as run-tests.sh judges: a failed exit, a segfault or
# `running 0 tests` in its log is 1.
vm_staged_judged() {
  local log=$base/logs/$what-$stamp.log
  if [ "$code" -ne 0 ]; then
    echo "run-gpu.sh: $what exited $code"
    code=1
  elif grep -qE 'Segmentation fault|core dumped' "$log"; then
    echo "run-gpu.sh: $what: a segfault in its log"
    code=1
  elif grep -qE '^running 0 tests' "$log"; then
    echo "run-gpu.sh: $what ran 0 tests"
    code=1
  fi
}

# vm_follow <i>: the dir's run's log from its first line, live while the run goes; then exits as run
# exits for its status: 0 and 2 as they are, else 1. No run is 2, none with a status 1.
vm_follow() {
  local base=$HOME/peacock/$1 pid boot what stamp start going code log
  vm_state "$1" || { echo "run-gpu.sh: no run in ~/peacock/$1 on the VM to follow: run-gpu.sh … status" >&2; exit 2; }
  log=$base/logs/$what-$stamp.log
  if [ "$going" -eq 1 ]; then tail -n +1 -s 0.2 --pid="$pid" -f "$log"; else cat "$log"; fi
  code=$(vm_status_code "$1" "$stamp")
  if [ -z "$code" ]; then
    echo "run-gpu.sh: the $what run started $stamp ended with no status: its runner was killed, or the VM restarted" >&2
    exit 1
  fi
  case $code in 0|2) exit "$code" ;; *) exit 1 ;; esac
}

# vm_run <i> <run image ID> <mode> <detach> <card lock> [<NAME=value>…]: run-tests.sh <mode> as
# peacock-<i>, followed; detached (1), returns once its runner is started.
vm_run() {
  local dir=$1 image=$2 mode=$3 detach=$4 lock=$5 base=$HOME/peacock/$1 envs=() pair
  shift 5
  for pair in "$@"; do envs+=(-e "$pair"); done
  [ -f "$base/cpp/install/lib/libpeacock_gpu.so" ] || { echo "run-gpu.sh: no install in $base: ship first" >&2; exit 2; }
  vm_started "$dir" "$mode" "$lock" "${envs[@]}" "$image" "$mode"
  if [ "$detach" -eq 1 ]; then
    echo "==> peacock-$dir: run-tests.sh $mode, detached; run-gpu.sh … status says how it goes, … follow follows it"
    return 0
  fi
  vm_follow "$dir"
}

# vm_run_staged <i> <run image ID> <name> <card lock> <NAME=value>… -- <argument>…: the install's
# rust-benchmarks/<name> as peacock-<i>'s entrypoint in the run image, followed; refuses, exit 2, a
# name staged there as no executable.
vm_run_staged() {
  local dir=$1 image=$2 name=$3 lock=$4 base=$HOME/peacock/$1 envs=()
  shift 4
  while [ "$1" != -- ]; do envs+=(-e "$1"); shift; done
  shift
  [ -x "$base/cpp/install/rust-benchmarks/$name" ] \
    || { echo "run-gpu.sh: no executable $base/cpp/install/rust-benchmarks/$name: stage and ship it first" >&2; exit 2; }
  vm_started "$dir" "$name" "$lock" -e LD_LIBRARY_PATH=/peacock/install/lib:/opt/cudf/lib \
    -e PEACOCK_TESTDATA_DIR=/peacock/testdata "${envs[@]}" --entrypoint "/peacock/install/rust-benchmarks/$name" \
    "$image" "$@"
  vm_follow "$dir"
}

# vm_status <i>: the dir's run, a line, then the last 40 lines of its log; returns 2 with no run, 5
# while it waits for the card or runs, 0 or 1 as it exited, 1 with no status.
vm_status() {
  local base=$HOME/peacock/$1 pid boot what stamp start going code log
  vm_docker
  vm_state "$1" || { echo none; return 2; }
  log=$base/logs/$what-$stamp.log
  code=$(vm_status_code "$1" "$stamp")
  if [ -z "$code" ] && [ "$going" -eq 1 ]; then
    read -r state owner <<< "$(vm_container "$1")"
    if [ -n "$state" ] && [ "$owner" = "$base" ]; then echo running; else echo waiting; fi
    tail -n 40 "$log"
    return 5
  fi
  if [ -z "$code" ]; then
    echo "ended without a status"
    tail -n 40 "$log"
    return 1
  fi
  echo "exited $code"
  tail -n 40 "$log"
  [ "$code" -eq 0 ] || return 1
}

# vm_cancel <i>: TERM to the dir's runner, and its end awaited; with none, a container of the dir no
# runner holds removed. Under run/lock, so no run starts meanwhile. 1 when the run ended with no
# status, or the container stays.
vm_cancel() {
  local base=$HOME/peacock/$1 pid boot what stamp start going code state owner
  vm_docker
  if [ -d "$base/run" ]; then vm_locked "$1"; fi
  if vm_state "$1" && [ "$going" -eq 1 ]; then
    while kill -TERM "$pid" 2> /dev/null; do sleep 0.2; done
    code=$(vm_status_code "$1" "$stamp")
    if [ -z "$code" ]; then
      echo "run-gpu.sh: the $what run started $stamp ended with no status; its log is $base/logs/$what-$stamp.log" >&2
      exit 1
    fi
    if grep -q 'run-gpu.sh: cancelled$' "$base/logs/$what-$stamp.log"; then
      echo "cancelled the $what run of $1 started $stamp (exit $code)"
    else
      echo "the $what run of $1 started $stamp ended by itself (exit $code) before the cancel"
    fi
    return 0
  fi
  read -r state owner <<< "$(vm_container "$1")"
  if [ -n "$state" ] && [ "$owner" = "$base" ]; then
    docker rm -f "peacock-$1" > /dev/null 2>&1 || true
    if [ -n "$(vm_container "$1")" ]; then
      echo "run-gpu.sh: peacock-$1 is still on the VM: docker could not remove it" >&2
      exit 1
    fi
    echo "removed peacock-$1, which no run held"
    return 0
  fi
  echo "no run of $1 goes on the VM"
}

# vm_pull_ready <i>: refuses, exit 2, before anything was shipped; makes what pull reads.
vm_pull_ready() {
  local base=$HOME/peacock/$1
  [ -d "$base/testdata" ] || { echo "run-gpu.sh: nothing shipped to $base: ship first" >&2; exit 2; }
  mkdir -p "$base/testdata/benchmark-results" "$base/testdata/calibration" "$base/logs"
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
  run_image_read
  cards_checked
  on_vm vm_data_set "$dir" "$BUILD_REF"
  if [ "$no_sf40" -eq 0 ]; then
    vm "cd ~/peacock/$dir && scripts/env/fetch-sf40.sh --env - ${SF40_SETS[*]}" "$PEACOCK_ENV"
    on_vm vm_cudf "$RUN_ID" "$cudf"
  fi
  echo "==> prepare: the VM is ready for $RUN_REF"
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
  # The VM's run image as prepare left it: no pull, so a run never takes an image prepare did not
  # check, and a tree whose image inputs moved since is refused.
  run_image_read
  cards_checked
  [ -z "${PCK_TEST_FILTER:-}" ] || envs+=("PCK_TEST_FILTER=$PCK_TEST_FILTER")
  [ -z "${PEACOCK_BENCH_RESUME:-}" ] || envs+=("PEACOCK_BENCH_RESUME=$PEACOCK_BENCH_RESUME")
  # The engine's two measurement switches (architecture.md, "cuDF options"): the cache's size and
  # the scan's reader.
  [ -z "${PEACOCK_PINNED_HOST_CACHE_BYTES:-}" ] || envs+=("PEACOCK_PINNED_HOST_CACHE_BYTES=$PEACOCK_PINNED_HOST_CACHE_BYTES")
  [ -z "${PEACOCK_SCAN_READER:-}" ] || envs+=("PEACOCK_SCAN_READER=$PEACOCK_SCAN_READER")
  [ -z "${PEACOCK_BENCH_CASE_TIMEOUT_S:-}" ] || envs+=("PEACOCK_BENCH_CASE_TIMEOUT_S=$PEACOCK_BENCH_CASE_TIMEOUT_S")
  [ -z "${PCK_WRITE_GPU_RESULT:-}" ] || envs+=("PCK_WRITE_GPU_RESULT=$PCK_WRITE_GPU_RESULT")
  if [ "$detach" -eq 1 ]; then
    on_vm vm_run "$dir" "$RUN_ID" "$mode" 1 "$PEACOCK_CARD_LOCK" "${envs[@]}"
  else
    following vm_run "$dir" "$RUN_ID" "$mode" 0 "$PEACOCK_CARD_LOCK" "${envs[@]}"
  fi
}

cmd_status() { on_vm vm_status "$dir"; }
cmd_follow() { following vm_follow "$dir"; }
cmd_cancel() { on_vm vm_cancel "$dir"; }

cmd_run_staged() {
  local envs=() args=(--nocapture --test-threads=1)
  run_image_read
  cards_checked
  [ -z "${PCK_TEST_FILTER:-}" ] || envs+=("PCK_TEST_FILTER=$PCK_TEST_FILTER")
  [ -z "${PEACOCK_BENCH_RESUME:-}" ] || envs+=("PEACOCK_BENCH_RESUME=$PEACOCK_BENCH_RESUME")
  if [ -n "$exact" ]; then args+=(--exact "$exact"); elif [ -n "${PCK_TEST_FILTER:-}" ]; then args+=("$PCK_TEST_FILTER"); fi
  following vm_run_staged "$dir" "$RUN_ID" "$staged" "$PEACOCK_CARD_LOCK" "${envs[@]}" -- "${args[@]}"
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
