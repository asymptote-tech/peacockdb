# shellcheck shell=bash
# shellcheck disable=SC2164  # each function runs under on_vm's set -euo pipefail, cd's failure fatal
# A GPU VM dir's VM side: functions scripts/lib/gpu-vm-client.sh sources and on_vm sends to the VM
# as text.

# --- the VM's side: sent by on_vm, never run here --------------------------------------------------

# vm_tree_claimed <i> <tree id>: ~/peacock/<i> made, with .tree holding <tree id>; one with no .tree,
# made before a dir kept its tree, claimed; one whose .tree is another id refused, exit 2, as is an
# empty id. .tree is made whole by a link, so a call at the same moment never reads it half-written.
vm_tree_claimed() {
  local base=$HOME/peacock/$1 old=0
  [ -n "$2" ] || { echo "run-gpu.sh: no tree id to claim ~/peacock/$1 with" >&2; exit 2; }
  [ ! -d "$base" ] || [ -f "$base/.tree" ] || old=1
  mkdir -p "$base"
  if [ ! -f "$base/.tree" ]; then
    printf '%s\n' "$2" > "$base/.tree.$$"
    if ln "$base/.tree.$$" "$base/.tree" 2> /dev/null && [ "$old" -eq 1 ]; then
      echo "==> ~/peacock/$1: claimed for this tree; it was made before a dir kept its tree's id"
    fi
    rm -f "$base/.tree.$$"
  fi
  [ "$(cat "$base/.tree")" = "$2" ] || {
    echo "run-gpu.sh: ~/peacock/$1 on the VM is another tree's: name a dir of your own with --dir <name>" >&2
    exit 2
  }
}

# vm_docker: docker through sudo while the docker group the user joined is not yet in effect.
# shellcheck disable=SC2032,SC2033  # `sudo docker` is the binary, as meant
vm_docker() {
  if ! docker info > /dev/null 2>&1; then docker() { sudo docker "$@"; }; fi
}

vm_docker_run() { vm_docker; docker "$@"; }

# A dir's run, in ~/peacock/<i>/: run/state, `<runner pid> <boot id> <what> <stamp> <start time>`,
# <what> the mode or the staged binary's name, written by the runner; its log,
# logs/<what>-<stamp>.log; its record, testdata/calibration/records-<stamp>.tsv once it ended;
# run/status, `<stamp> <code>`, once it ended; run/lock, held around a run's start, a cancel, and a
# write of .shipped or a clear.

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
  local params=${PEACOCK_NVIDIA_PARAMS:-/proc/driver/nvidia/params} keyring repo found nsys conf rc=0
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
  # The module option takes effect when the nvidia module loads, from the initramfs: at boot. From a
  # file: uutils' install (Ubuntu 26.04) refuses /dev/stdin onto a file that exists.
  conf=$(mktemp)
  { echo 'options nvidia NVreg_RestrictProfilingToAdminUsers=0' > "$conf" \
      && sudo install -m 644 "$conf" /etc/modprobe.d/peacock-profiling.conf; } || rc=$?
  rm -f "$conf"
  [ "$rc" -eq 0 ] || exit "$rc"
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

# vm_started <i> <what> <card lock> <judged> <docker run argument>…: under run/lock, once the dir is
# idle, a runner of <what> with the run's mounts, user and hostname and these arguments, detached from
# this session, and its log, empty. <judged> 1: the run's code is judged as a staged binary's
# (vm_staged_judged); 0, it is the container's. run/lock is let go once the runner wrote its state,
# its TERM trap set, so a cancel never finds a runner it cannot stop. The binds' sources are made
# here, as the user: Docker would make a missing one as root, which a later ship could not write.
vm_started() {
  local dir=$1 what=$2 lock=$3 judged=$4 base=$HOME/peacock/$1 data=${PEACOCK_DATA_DIR:-/var/lib/peacock}
  local stamp runner
  shift 4
  vm_docker
  mkdir -p "$base/run" "$base/logs" "$base/cpp/install" "$base/testdata"
  vm_locked "$dir"
  vm_idle "$dir"
  vm_name_free "$dir"
  vm_records_kept "$dir"
  stamp=$(date -u +%Y%m%dT%H%M%S.%3NZ)
  : > "$base/logs/$what-$stamp.log"
  runner="set -euo pipefail; $(declare -f vm_docker vm_container vm_started_at peacock_card_take vm_runner \
    vm_runner_ended vm_staged_judged vm_name_taken); vm_runner \"\$@\""
  # Every fd redirected: the session's pipes close when this call ends, and run/lock is let go.
  setsid nohup bash -c "$runner" run-gpu.sh "$dir" "$what" "$stamp" "$lock" "$judged" \
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
    echo "run-gpu.sh: waiting for ~/peacock/$1/run/lock: another call on the dir holds it" >&2
    flock "$VM_LOCK_FD"
  fi
}

# vm_runner <i> <what> <stamp> <card lock> <judged> <docker run argument>…: the run, detached: its
# output to its log; the card's lock, waited for and held while the container lives; the container
# removed; judged (1) as a staged binary is; its record kept under its stamp; then its status.
# --init: run-tests.sh as PID 1 would ignore docker stop.
# cancel sends TERM until this process ends: each one cuts short the wait in hand, and the flag
# says what to do next, so a TERM at any point lands. The status is the client's, so a TERM that
# comes as the run ends keeps the run's own.
vm_runner() {
  local dir=$1 what=$2 stamp=$3 lock=$4 judged=$5 base=$HOME/peacock/$1 name=peacock-$1 cancelled=0 stopped=0
  local child="" code="" rc=0 state owner
  shift 5
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
    [ "$judged" -eq 0 ] || vm_staged_judged
  fi
  # run-tests.sh refuses a record holding rows, so the next bench needs this one out of its way.
  if [ -f "$base/testdata/calibration/records.tsv" ]; then
    mv "$base/testdata/calibration/records.tsv" "$base/testdata/calibration/records-$stamp.tsv"
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
  if [ "$going" -eq 1 ]; then tail -n +1 -s 0.2 --pid="$pid" -f "$log"; else vm_log_shown "$1" "$log" +1; fi
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
  vm_started "$dir" "$mode" "$lock" 0 "${envs[@]}" "$image" "$mode"
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
  vm_started "$dir" "$name" "$lock" 1 -e LD_LIBRARY_PATH=/peacock/install/lib:/opt/cudf/lib \
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
    vm_log_shown "$1" "$log" 40
    return 1
  fi
  echo "exited $code"
  vm_log_shown "$1" "$log" 40
  [ "$code" -eq 0 ] || return 1
}

# vm_log_shown <i> <log> <+1 or 40>: an ended run's log, whole or its last 40 lines; one pull --clear
# removed is said, with where pull keeps it.
vm_log_shown() {
  if [ -f "$2" ]; then
    if [ "$3" = +1 ]; then cat "$2"; else tail -n "$3" "$2"; fi
  else
    echo "run-gpu.sh: ${2##*/} was cleared from the VM by pull --clear; pull keeps it in" \
         "\${XDG_CACHE_HOME:-~/.cache}/peacock/run-gpu/$1/ on the machine that pulled it"
  fi
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

# vm_records_kept <i>: a records.tsv no run holds kept as records-<stamp>.tsv: the stamp of the
# dir's last run, whose runner died before it could keep it, or, with no run/state (a run from before
# the runner), the file's time as one. The caller holds run/lock and has seen no run going.
vm_records_kept() {
  local record=$HOME/peacock/$1/testdata/calibration/records.tsv pid boot what stamp start going
  [ -f "$record" ] || return 0
  vm_state "$1" || stamp=$(date -u -r "$record" +%Y%m%dT%H%M%S.%3NZ)
  mv "$record" "${record%.tsv}-$stamp.tsv"
}

# vm_shipped <i>: .shipped given the sha256 of the VM's copy of each path on stdin, under
# ~/peacock/<i>/.
vm_shipped() {
  local paths=()
  mapfile -t paths
  cd "$HOME/peacock/$1"
  if [ ${#paths[@]} -gt 0 ]; then sha256sum -- "${paths[@]}"; fi | vm_stamped "$1"
}

# vm_stamped <i>: each `<sha256>  <path>` on stdin made its path's line of .shipped, the other lines
# kept; under run/lock, and whole, through a temp file, so a pull never reads it half-written.
vm_stamped() {
  local base=$HOME/peacock/$1 digest path
  local -A line=()
  mkdir -p "$base/run"
  vm_locked "$1"
  if [ -f "$base/.shipped" ]; then
    while read -r digest path; do line[$path]=$digest; done < "$base/.shipped"
  fi
  while read -r digest path; do line[$path]=$digest; done
  for path in "${!line[@]}"; do printf '%s  %s\n' "${line[$path]}" "$path"; done | sort -k 2 > "$base/.shipped.new"
  mv "$base/.shipped.new" "$base/.shipped"
}

# vm_pull_ready <i> <clear>: refuses, exit 2, before anything was shipped, and a clear (1) while the
# dir's run goes; keeps a record no run holds under its stamp; makes what pull reads. With clear, the
# records and logs the dir holds. Under run/lock, so a run being started is seen as going.
vm_pull_ready() {
  local base=$HOME/peacock/$1 pid boot what stamp start going=0
  [ -d "$base/testdata" ] || { echo "run-gpu.sh: nothing shipped to $base: ship first" >&2; exit 2; }
  mkdir -p "$base/run"
  vm_locked "$1"
  if vm_state "$1" && [ "$going" -eq 1 ] && [ "$2" -eq 1 ]; then
    echo "run-gpu.sh: pull --clear: the $what run started $stamp goes on in ~/peacock/$1 on the VM:" \
         "run-gpu.sh … follow follows it to its end" >&2
    exit 2
  fi
  mkdir -p "$base/testdata/benchmark-results" "$base/testdata/calibration" "$base/logs"
  if [ "$going" -eq 0 ]; then vm_records_kept "$1"; fi
  if [ "$2" -eq 1 ]; then vm_clearable "$1"; fi
}

# vm_pull_list <i>: `<sha256> <stamped sha256, or -> <path>` of each file of the pulled set whose
# sha256 is not its .shipped line's, paths under ~/peacock/<i>/.
vm_pull_list() {
  local digest path roots=() files=()
  local -A stamp=()
  cd "$HOME/peacock/$1"
  if [ -f .shipped ]; then
    while read -r digest path; do stamp[$path]=$digest; done < .shipped
  fi
  for path in testdata/benchmark-results testdata/calibration testdata/goldens logs; do
    if [ -d "$path" ]; then roots+=("$path"); fi
  done
  while IFS= read -r -d '' path; do
    if in_pulled_set "$path"; then files+=("$path"); fi
  done < <(find "${roots[@]}" -type f -print0)
  [ ${#files[@]} -gt 0 ] || return 0
  sha256sum -- "${files[@]}" | while read -r digest path; do
    if [ "${stamp[$path]:-}" != "$digest" ]; then echo "$digest ${stamp[$path]:--} $path"; fi
  done
}

# vm_clearable <i>: what pull --clear may remove: the runs' records, records-<stamp>.tsv (not
# create_nsys_profile.sh's records-metrics.tsv, which stays), and the logs.
vm_clearable() {
  local path
  cd "$HOME/peacock/$1"
  for path in testdata/calibration/records-[0-9]*.tsv logs/*; do
    if [ -f "$path" ]; then echo "$path"; fi
  done
}

# vm_cleared <i>: under run/lock, each record and log of the dir removed whose sha256 stdin gives as
# `<sha256> <path>` of its copy home, with its .shipped line; each other one named and kept.
vm_cleared() {
  local base=$HOME/peacock/$1 digest path have removed=0
  local -A home=() gone=()
  while read -r digest path; do home[$path]=$digest; done
  mkdir -p "$base/run"
  vm_locked "$1"
  while IFS= read -r path; do
    have=$(sha256sum < "$base/$path")
    have=${have%% *}
    if [ "${home[$path]:-}" = "$have" ]; then
      rm "$base/$path"
      gone[$path]=1
      removed=$((removed + 1))
    elif [ -z "${home[$path]:-}" ]; then
      echo "kept $path: not pulled home"
    else
      echo "kept $path: its copy home differs"
    fi
  done < <(vm_clearable "$1")
  if [ "$removed" -gt 0 ] && [ -f "$base/.shipped" ]; then
    while read -r digest path; do
      if [ -z "${gone[$path]:-}" ]; then printf '%s  %s\n' "$digest" "$path"; fi
    done < "$base/.shipped" > "$base/.shipped.new"
    mv "$base/.shipped.new" "$base/.shipped"
  fi
  echo "==> pull --clear: removed $removed records and logs from ~/peacock/$1/"
}
