#!/usr/bin/env bash
# gpu.sh's side of a GPU host, run from the instance's dir there, ~/peacock/<instance>/; busy is also
# read from stdin, needing nothing shipped (llm-wiki/build-test.md, "GPU hosts").
#
#   on-gpu-host.sh install | pull <image> | label <image> | sf1 <image> | sf40-missing | settle | busy
#   on-gpu-host.sh hold <seconds> [<instance dir>] | sf40 <image> <dataset>... (keys on stdin) | fetch …
#   on-gpu-host.sh run <image id> <mode> [NAME=value]... | run-state | follow <mode> <stamp> [NAME=value]...
#   on-gpu-host.sh cancel | clear (`<sha256> <record>` lines on stdin)
#
# The data directory is /var/lib/peacock or $PEACOCK_DATA_DIR. Every step, and each gpu.sh command whole,
# holds the activity lock shared, which busy tests. A failure that is not a refusal (2) is exit 1.
set -Eeuo pipefail
trap 'exit 1' ERR
here=$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")" && pwd)
instance_dir=$(cd "$here/../.." && pwd)
instance=${instance_dir##*/}
DEFAULT_DATA=/var/lib/peacock
DATA=${PEACOCK_DATA_DIR:-$DEFAULT_DATA}

die() { echo "on-gpu-host.sh: $*" >&2; exit 2; }
usage() { sed -n '2,${/^#/!q;p;}' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'; exit 0; }

SF40=(tpch.sf40 tpcds.sf40)
TOOLKIT=https://nvidia.github.io/libnvidia-container
TOOLKIT_KEYRING=/usr/share/keyrings/nvidia-container-toolkit-keyring.gpg
INSTALL_LOCK=${PEACOCK_INSTALL_LOCK:-/run/lock/peacock-install.lock}

# data_ready — refuses a data directory that is not absolute, missing or not writable for this user,
# then sources the machine's steps, which keep their sets there. Only the default is ever made, by
# install: a PEACOCK_DATA_DIR that is missing is most likely a disk that is not mounted.
data_ready() {
  data_dir_ok
  PEACOCK_DATA_DIR=$DATA
  . "$here/../lib/data-inputs.sh"
  . "$here/../lib/machine.sh"
}

data_dir_ok() {
  [[ "$DATA" == /* ]] || die "PEACOCK_DATA_DIR must be an absolute path: $DATA"
  if [ ! -d "$DATA" ] && [ "$DATA" != "$DEFAULT_DATA" ]; then
    die "PEACOCK_DATA_DIR $DATA does not exist: mount its disk, or make it group docker, setgid, mode 2775"
  fi
  [ -d "$DATA" ] && [ -w "$DATA" ] \
    || die "the host's data directory $DATA is missing or not writable for $(id -un): gpu.sh <host> prepare makes it"
}

# shared_file <path> — <path>, made group-writable when missing, whatever the session's umask: the
# host's other users take the same locks.
shared_file() { if [ ! -e "$1" ]; then (umask 002 && : >> "$1"); fi; }
shared_dir() { (umask 002 && mkdir -p "$1"); }

# active — holds the host's activity lock shared until this process ends: a soft stop declines while
# any step, of any instance, holds it. A runner inherits it from the run that started it. Read-only:
# a shared flock needs no more.
active() { shared_file "$DATA/.activity.lock"; exec {activity}< "$DATA/.activity.lock"; flock -s "$activity"; }

# Docker, rsync (ship's) and the NVIDIA Container Toolkit, each when missing; the user in docker's
# group; the default data directory, shared by the host's users through that group. docker restarts
# only with a toolkit just installed: nothing ran on the card through it before.
install_missing() {
  local packages=() joined="" installing
  [[ "$DATA" == /* ]] || die "PEACOCK_DATA_DIR must be an absolute path: $DATA"
  # One install at a time on the host, whoever runs it: two first prepares would race on apt's lock,
  # the toolkit's keyring and its sources list. Outside the data directory, which may not exist yet.
  if [ ! -e "$INSTALL_LOCK" ]; then (umask 000 && : >> "$INSTALL_LOCK"); fi
  exec {installing}< "$INSTALL_LOCK"
  if ! flock -n "$installing"; then
    echo "waiting on $(hostname): another prepare's install holds $INSTALL_LOCK" >&2
    flock "$installing"
  fi
  if ! command -v docker > /dev/null; then packages+=(docker.io); fi
  if ! command -v rsync > /dev/null; then packages+=(rsync); fi
  if [ ${#packages[@]} -gt 0 ]; then
    sudo apt-get update -q < /dev/null
    sudo apt-get install -y "${packages[@]}" < /dev/null
  fi
  if ! command -v nvidia-ctk > /dev/null; then
    curl -fsSL "$TOOLKIT/gpgkey" | sudo gpg --dearmor --yes -o "$TOOLKIT_KEYRING"
    curl -fsSL "$TOOLKIT/stable/deb/nvidia-container-toolkit.list" \
      | sed "s#deb https://#deb [signed-by=$TOOLKIT_KEYRING] https://#g" \
      | sudo tee /etc/apt/sources.list.d/nvidia-container-toolkit.list > /dev/null
    sudo apt-get update -q < /dev/null
    sudo apt-get install -y nvidia-container-toolkit < /dev/null
    sudo nvidia-ctk runtime configure --runtime=docker < /dev/null
    sudo systemctl restart docker < /dev/null
  fi
  if [[ " $(id -nG) " != *" docker "* ]]; then sudo usermod -aG docker "$(id -un)" < /dev/null; joined=1; fi
  if [ "$DATA" = "$DEFAULT_DATA" ] && [ ! -e "$DATA" ]; then
    sudo install -d -o "$(id -un)" -g docker -m 2775 "$DATA" < /dev/null
  fi
  # The group just joined is this session's only from the next ssh on, which is every later step's:
  # a directory writable through it is checked there, not here.
  if [ -n "$joined" ] && [ -d "$DATA" ]; then return; fi
  data_ready
  active
}

# label <image> — the ID <image> names now, then the label of that ID: the image checked is the image
# gpu.sh runs, even when the tag moves between the two reads.
label() {
  local id
  id=$(docker image inspect --format '{{.Id}}' "$1")
  echo "$id"
  docker image inspect --format '{{index .Config.Labels "peacockdb.inputs"}}' "$id"
}

# sf40_present <dir> — its SHA256SUMS holds, over every file its manifest.json lists at its size.
sf40_present() {
  local file size
  [ -f "$1/SHA256SUMS" ] && [ -f "$1/manifest.json" ] || return 1
  (cd "$1" && sha256sum --quiet --status -c SHA256SUMS) || return 1
  # manifest.json is {"<table>.parquet": <bytes>, …}, as scripts/upload_datasets.sh writes it.
  while IFS=: read -r file size; do
    if [ -z "$file" ]; then continue; fi
    [ -f "$1/$file" ] && [ "$(stat -c %s "$1/$file")" = "$size" ] || return 1
  done < <(tr -d '{}" \n\r' < "$1/manifest.json" | tr ',' '\n')
}

sf40_missing() {
  local ds
  for ds in "${SF40[@]}"; do
    if ! sf40_present "$DATA/sf40/$ds"; then echo "$ds"; fi
  done
}

# storage_keys — reads the storage keys from stdin, NAME=value a line, into KEYS: each once, no other.
storage_keys() {
  local line
  . "$here/../lib/peacock-config.sh"
  KEYS=()
  while IFS= read -r line; do
    [[ " ${PEACOCK_STORAGE_KEYS[*]} " == *" ${line%%=*} "* ]] || die "only the storage keys are read on stdin"
    KEYS+=("$line")
  done
  [ ${#KEYS[@]} = ${#PEACOCK_STORAGE_KEYS[@]} ] || die "the ${#PEACOCK_STORAGE_KEYS[@]} storage keys are read on stdin"
}

# sf40 <build image> <dataset>... — each from its bucket, <dataset> with - for ., in the build image,
# which has the aws CLI. The keys go from this stdin to the container's stdin and are never written.
sf40() {
  local ref=$1 ds final stage lock mask scripts
  shift
  storage_keys
  scripts=$(cd "$here/.." && pwd)
  mask=$(umask)
  umask 002
  mkdir -p "$DATA/sf40"
  for ds in "$@"; do
    [[ " ${SF40[*]} " == *" $ds "* ]] || die "no sf40 dataset $ds: ${SF40[*]}"
    final=$DATA/sf40/$ds stage=$DATA/sf40/$ds.partial
    exec {lock}> "$DATA/sf40/.$ds.lock"
    flock "$lock"
    if ! sf40_present "$final"; then
      rm -rf "$stage"
      mkdir "$stage"
      printf '%s\n' "${KEYS[@]}" | docker run --rm -i --user "$MACHINE_USER" \
        --mount "type=bind,src=$scripts,dst=/peacock/scripts,readonly" --mount "type=bind,src=$stage,dst=$MACHINE_STAGE" \
        "$ref" bash /peacock/scripts/env/on-gpu-host.sh fetch "${ds/./-}" "$MACHINE_STAGE"
      (cd "$stage" && sha256sum -- manifest.json *.parquet > SHA256SUMS)
      sf40_present "$stage" || die "$ds from s3://${ds/./-} does not match its manifest.json"
      rm -rf "$final"
      mv "$stage" "$final"
    fi
    exec {lock}>&-
  done
  umask "$mask"
}

fetch() {
  local bucket=$1 stage=$2 key s3
  storage_keys
  for key in "${KEYS[@]}"; do export "${key?}"; done
  . "$here/../lib/s3.sh"
  s3_args s3
  aws "${s3[@]}" s3 cp --recursive --only-show-errors "s3://$bucket/" "$stage/"
}

# busy — one line: `idle`, or `busy: <why>`. Busy is a process on the card, the device lock held, or
# a step of gpu.sh holding the activity lock, a run waiting for the card among them.
busy() {
  local apps
  apps=$(nvidia-smi --query-compute-apps=pid --format=csv,noheader < /dev/null) \
    || { echo "busy: nvidia-smi cannot say what is on the card"; return; }
  if [ -n "$apps" ]; then echo "busy: a process on the card: $(paste -sd' ' - <<< "$apps")"; return; fi
  if [ -e "$DATA/.device.lock" ] && ! flock -n "$DATA/.device.lock" true; then
    echo "busy: a device run holds $DATA/.device.lock"
    return
  fi
  if [ -e "$DATA/.activity.lock" ] && ! flock -n -x "$DATA/.activity.lock" true; then
    echo "busy: a gpu.sh step is running on the host"
    return
  fi
  echo idle
}

# The instance's run lock is the host user's, as run/state is: two users may name instances alike.
run_lock() {
  shared_dir "$DATA/runs"
  exec {own}> "$DATA/runs/$(id -un)-$instance.lock"
}

# run_probe — RUN is free (the instance's lock taken and let go), going (pid, running, log, stamp,
# digest and envhash its run's, alive and with no status), or busy: the lock is held by no live run,
# a ship's or a run being started. A run being started has no state for a moment.
run_probe() {
  local tries=0
  run_lock
  if flock -n "$own"; then
    exec {own}>&-
    RUN=free
    return
  fi
  while [ ! -f "$instance_dir/run/state" ] && [ "$tries" -lt 100 ]; do
    sleep 0.1
    tries=$((tries + 1))
  done
  RUN=busy
  if [ -f "$instance_dir/run/state" ]; then
    read -r pid running log stamp digest envhash < "$instance_dir/run/state"
    if [ ! -f "${log%.log}.status" ] && kill -0 "$pid" 2> /dev/null; then RUN=going; fi
  fi
}
busy_instance() { die "$instance is busy on $(hostname): a ship or a run being started holds its lock; try again"; }
going_run() {
  die "a $running run of $instance started $stamp is still going on $(hostname): gpu.sh <host> run $running follows it"
}

# hold <seconds> [<instance dir>] — the activity lock, held shared for as long as a line comes on stdin
# every <seconds>: a caller that falls silent lets go too. With an instance dir, as ship gives it, the
# instance's run lock too, so no run starts under its rsync. Read from stdin, with nothing shipped: on a
# host with no data directory this user can write yet, it holds nothing and says so. That is a first
# prepare's, or a new user's whose install puts them in the docker group the directory is shared by.
hold() {
  local seconds=$1 ack=unheld
  if [ -n "${2:-}" ]; then instance_dir=$HOME/$2 instance=${2##*/}; fi
  if [ -d "$DATA" ] && [ -w "$DATA" ]; then
    data_dir_ok
    active
    ack=held
    if [ -n "${2:-}" ]; then
      run_probe
      case "$RUN" in going) going_run ;; busy) busy_instance ;; esac
      run_lock
      flock -n "$own" || busy_instance
    fi
  fi
  echo "$ack"
  while IFS= read -r -t "$seconds" _; do :; done
  # A ship's rsync outlives a gpu.sh killed alone, whose heartbeat stops: the locks go only once no
  # rsync server of this user receives into the instance's dir. A pull's server is a --sender; ship's
  # --timeout ends a server whose client vanished.
  if [ -n "${2:-}" ]; then
    while [ -n "$(pgrep -a -u "$(id -u)" -f "rsync --server .* \. ${2//./\\.}/" | grep -v -- " --sender ")" ]; do
      sleep 1
    done
  fi
  # Read from stdin, bash would go on to read its next command there, and hold the lock with it.
  exit 0
}

binaries_digest() {
  (cd "$instance_dir" && find cpp/install -type f -printf '%P %s %T@\n' | LC_ALL=C sort | sha256sum | cut -c1-16)
}
env_hash() { printf '%s\n' "$@" | sha256sum | cut -c1-16; }
env_of_run() { if [ -s "$instance_dir/run/env" ]; then paste -sd' ' - < "$instance_dir/run/env"; else echo "neither PCK_TEST_FILTER nor PEACOCK_BENCH_RESUME"; fi; }

# follow <pid> <log> — the run's log until the run ends, then its status, kept beside the log.
follow() {
  tail -n +1 --pid="$1" -f "$2"
  if [ ! -f "${2%.log}.status" ]; then
    echo "on-gpu-host.sh: the run ended with no status: it was killed, or the host restarted" >&2
    exit 1
  fi
  exit "$(cat "${2%.log}.status")"
}

# mismatch <mode> [NAME=value]... — why the instance's last run, going or ended, is not this one: of
# another mode, another environment, or binaries changed since it started. Empty when it is.
mismatch() {
  local want=$1
  shift
  if [ "$running" != "$want" ]; then
    echo "this command asks for a $want run"
  elif [ "$(env_hash "$@")" != "$envhash" ]; then
    echo "it ran with $(env_of_run), and this command with ${*:-neither PCK_TEST_FILTER nor PEACOCK_BENCH_RESUME}"
  elif [ "$(binaries_digest)" != "$digest" ]; then
    echo "the binaries in $instance_dir changed since it started, so its status is not theirs"
  fi
}

# following <mode> [NAME=value]... — follows the instance's going run when it is this one.
following() {
  local want=$1 why
  shift
  [ "$running" = "$want" ] || going_run
  why=$(mismatch "$want" "$@")
  [ -z "$why" ] || die "the $running run started $stamp is still going, but $why: wait for it to end; its log is $log"
  echo "on-gpu-host.sh: the $want run started $stamp is still going: following it" >&2
  follow "$pid" "$log"
}

# interrupted_kept — a record a killed run left at run-tests.sh's one path is that run's: kept under
# its start time, where pull brings it. Called with the instance's run lock held, so no run is going.
interrupted_kept() {
  local record=$instance_dir/testdata/calibration/records.tsv old=""
  [ -f "$record" ] || return 0
  if [ -f "$instance_dir/run/state" ]; then old=$(cut -d' ' -f4 "$instance_dir/run/state"); fi
  mv "$record" "${record%.tsv}-${old:-$(date -u +%Y%m%dT%H%M%S.%3NZ)}-interrupted.tsv"
}

container_name() { echo "peacock-run-$(id -un)-$instance"; }

# run <image id> <mode> [NAME=value]... — starts the run detached from this session and follows it.
# The runner inherits this instance's lock and the activity lock, both taken here, so while it lasts a
# second run of the instance follows it or is refused, ship is refused, and the host is not stopped.
run() {
  local id=$1 mode=$2 hash ds pair container
  shift 2
  hash=$(data_inputs "$instance_dir")
  [ -d "$DATA/$hash" ] || die "no sf1 for this tree in $DATA: gpu.sh <host> prepare first"
  [ -f "$instance_dir/cpp/install/lib/libpeacock_gpu.so" ] || die "no binaries in $instance_dir: gpu.sh <host> ship first"
  if [ "$mode" = bench ]; then
    for ds in "${SF40[@]}"; do
      [ -d "$DATA/sf40/$ds" ] || die "run bench reads $ds, which $DATA/sf40 lacks: gpu.sh <host> prepare fetches it"
    done
  fi
  for pair in "$@"; do
    [[ "$pair" =~ ^(PCK_TEST_FILTER|PEACOCK_BENCH_RESUME)= ]] || die "run passes PCK_TEST_FILTER and PEACOCK_BENCH_RESUME, not $pair"
  done
  shared_dir "$instance_dir/run"
  shared_dir "$instance_dir/logs"
  run_probe
  case "$RUN" in going) following "$mode" "$@" ;; busy) busy_instance ;; esac
  run_lock
  flock -n "$own" || busy_instance
  # A runner killed while its container lives would let a second container onto the card beside it.
  container=$(container_name)
  if docker container inspect "$container" > /dev/null 2>&1; then
    die "the container $container of an earlier run of $instance is still on $(hostname): wait for it, or docker rm -f $container"
  fi
  interrupted_kept
  rm -f "$instance_dir/run/state"
  stamp=$(date -u +%Y%m%dT%H%M%S.%3NZ)
  log=$instance_dir/logs/$mode-$stamp.log
  : > "$log"
  printf '%s\n' "$@" | grep . > "$instance_dir/run/env" || true
  setsid nohup bash "$here/on-gpu-host.sh" runner "$id" "$mode" "$stamp" "$@" > /dev/null 2>&1 < /dev/null &
  echo "$! $mode $log $stamp $(binaries_digest) $(env_hash "$@")" > "$instance_dir/run/state.new"
  mv "$instance_dir/run/state.new" "$instance_dir/run/state"
  follow "$!" "$log"
}

# bench_outcome <log> <status> [cancelled] — one line: how many bench_ cases ran, were kept from an
# earlier run, passed and failed, the failed ones named; or that the run was cancelled, or broke before
# the harness summed up. NUL bytes dropped: grep and awk read a log holding one as binary.
bench_outcome() {
  local log=$1 status=$2 how=${3:-} ran kept failed summed names line
  # libtest names the failed cases under the last `failures:` before its summary, one indented name
  # a line; an earlier one holds their output.
  read -r ran kept failed summed names < <(tr -d '\000' < "$log" | awk '
    /^test bench_[^ ]+ \.\.\. / { ran++ }
    / on a device: kept, / { kept++ }
    /^failures:$/ { list = ""; n = 0; on = 1; next }
    on && /^    bench_[^ ]+$/ { list = list " " $1; n++ }
    /^test result: / { summed = 1; if (on) { all = all list; failed += n }; list = ""; on = 0 }
    END { printf "%d %d %d %d%s\n", ran, kept, failed, summed, all }')
  line="==> bench outcome:"
  if [ -n "$how" ]; then
    line+=" cancelled (exit $status) after $(cases "$ran") began"
  elif [ "$summed" = 0 ]; then
    line+=" the run broke (exit $status) after $(cases "$ran") began: the harness summed up none of them"
  elif [ "$ran" = 0 ]; then
    line+=" no case ran (exit $status)"
  else
    line+=" $(cases "$ran") ran: $((ran - kept - failed)) passed"
    if [ "$kept" -gt 0 ]; then line+=", $kept kept from an earlier run"; fi
    line+=", $failed failed"
    if [ "$failed" -gt 0 ]; then line+=": $names"; fi
    if [ "$failed" = 0 ] && [ "$status" != 0 ]; then line+=", and the run failed after them (exit $status)"; fi
  fi
  if [ -n "$how" ] || [ "$summed" = 0 ] || [ "$failed" -gt 0 ]; then
    line+="; what it measured, failures included, is in testdata/benchmark-results/"
  fi
  echo "$line"
}
cases() { if [ "$1" = 1 ]; then echo "1 case"; else echo "$1 cases"; fi; }

# runner <image id> <mode> <stamp> [NAME=value]... — the detached run: it waits for the host's device
# lock, saying so, holds it for the whole docker run, keeps a bench's record as records-<stamp>.tsv,
# and writes its status beside its log. Two chains' tiers on one card fail each other's memory pools.
runner() {
  local id=$1 mode=$2 stamp=$3 entry ds mounts=() envs=() pair dev status=0 host record hash log
  local container exits phase child
  shift 3
  log=$instance_dir/logs/$mode-$stamp.log
  exec >> "$log" 2>&1
  hash=$(data_inputs "$instance_dir")
  mounts=(--mount "type=bind,src=$instance_dir/cpp/install,dst=/peacock/install,readonly"
          --mount "type=bind,src=$instance_dir/testdata,dst=/peacock/testdata")
  # Mount points made as this user: docker would make them as root in the instance's testdata.
  for entry in "${DATA_SETS[@]}"; do
    ds=${entry%% *}
    mkdir -p "$instance_dir/testdata/$ds"
    mounts+=(--mount "type=bind,src=$DATA/$hash/$ds,dst=/peacock/testdata/$ds,readonly")
  done
  for ds in "${SF40[@]}"; do
    if [ ! -d "$DATA/sf40/$ds" ]; then continue; fi
    mkdir -p "$instance_dir/testdata/$ds"
    mounts+=(--mount "type=bind,src=$DATA/sf40/$ds,dst=/peacock/testdata/$ds,readonly")
  done
  for pair in "$@"; do envs+=(-e "$pair"); done
  host=$(hostname) container=$(container_name) exits=$instance_dir/run/client-exit
  shared_file "$DATA/.device.lock"
  exec {dev}< "$DATA/.device.lock"
  rm -f "$exits"
  # cancel signals this process alone. Each wait is in the background, so the trap runs at once.
  phase=waiting child=""
  trap runner_cancelled TERM
  if ! flock -n "$dev"; then
    echo "waiting on $host: another chain's device run holds its GPU"
    flock "$dev" &
    child=$!
    wait "$child"
  fi
  phase=running
  # --init: the run image's PID 1 is run-tests.sh, which as PID 1 would ignore docker stop's TERM.
  # The client's status goes to a file, which a cancel reads too, whenever it comes.
  ( rc=0
    docker run --rm --init --name "$container" --gpus all --user "$MACHINE_USER" --hostname "$host" \
      "${mounts[@]}" "${envs[@]}" "$id" "$mode" || rc=$?
    echo "$rc" > "$exits" ) &
  child=$!
  wait "$child" || true
  trap '' TERM
  status=$(cat "$exits")
  if [ "$mode" = bench ]; then bench_outcome "$log" "$status"; fi
  run_ended "$log" "$stamp" "$status"
}

# runner_cancelled — cancel's TERM, at any point of the runner. A run still going is said to be
# cancelled in its log, its wait for the card given up or its container stopped, until its client is
# gone: the client may be making it still. One made and never started is removed. The status is the
# client's when it ran, so a cancel that comes as the run ends keeps the run's own.
runner_cancelled() {
  local cancelled=1
  trap '' TERM
  if [ "$phase" = running ] && ! kill -0 "$child" 2> /dev/null; then cancelled=""; fi
  if [ -n "$cancelled" ]; then echo "on-gpu-host.sh: cancelled"; fi
  if [ "$phase" = waiting ] && [ -n "$child" ]; then kill "$child" 2> /dev/null || true; fi
  if [ "$phase" = running ]; then
    while kill -0 "$child" 2> /dev/null; do docker stop -t 30 "$container" > /dev/null 2>&1 || sleep 0.5; done
    docker rm -f "$container" > /dev/null 2>&1 || true
  fi
  status=$(cat "$exits" 2> /dev/null) || status=143
  if [ "$mode" = bench ]; then bench_outcome "$log" "$status" ${cancelled:+cancelled}; fi
  run_ended "$log" "$stamp" "$status"
  exit 0
}

# run_ended <log> <stamp> <status> — the run's record kept as its own, then its status beside its log.
# run-tests.sh writes the record at one path and refuses one with rows.
run_ended() {
  local record=$instance_dir/testdata/calibration/records.tsv
  if [ -f "$record" ]; then mv "$record" "${record%.tsv}-$2.tsv"; fi
  echo "$3" > "${1%.log}.status"
}

# cancel — stops the instance's run: its runner gives up its wait for the card or stops its
# container, and keeps what any run that ends keeps. A container a killed runner left is removed.
# Nothing going is no failure.
cancel() {
  local container
  container=$(container_name)
  run_probe
  case "$RUN" in
    busy) busy_instance ;;
    free)
      if docker container inspect "$container" > /dev/null 2>&1; then
        container_removed "$container"
        echo "removed $container, which an earlier run of $instance left on $(hostname)"
      else
        echo "no run of $instance is going on $(hostname)"
      fi
      return ;;
  esac
  kill -TERM "$pid" 2> /dev/null || true
  while kill -0 "$pid" 2> /dev/null; do sleep 0.2; done
  if [ ! -f "${log%.log}.status" ]; then
    echo "on-gpu-host.sh: the $running run of $instance started $stamp ended with no status; its log is $log" >&2
    exit 1
  fi
  if tr -d '\000' < "$log" | grep -qx 'on-gpu-host.sh: cancelled'; then
    echo "cancelled the $running run of $instance started $stamp (exit $(cat "${log%.log}.status")):" \
         "gpu.sh <host> pull brings its log and record home"
  else
    echo "the $running run of $instance started $stamp ended by itself (exit $(cat "${log%.log}.status"))" \
         "before the cancel: gpu.sh <host> pull brings its log and record home"
  fi
}

# records_cleared — removes each records-*.tsv of the instance whose sha256 a line on stdin gives, its
# copy pulled home being the same bytes, and says of each other one why it stays. records.tsv, a run's
# record not yet kept under its own name, is never removed. Under the instance's run lock: no run is going.
records_cleared() {
  local dir=$instance_dir/testdata/calibration sum name record host none=1
  local -A home=()
  while read -r sum name; do home[$name]=$sum; done
  run_probe
  case "$RUN" in going) going_run ;; busy) busy_instance ;; esac
  run_lock
  flock -n "$own" || busy_instance
  host=$(hostname)
  for record in "$dir"/records-*.tsv; do
    [ -f "$record" ] || continue
    name=${record##*/} none=""
    read -r sum _ < <(sha256sum < "$record")
    if [ -z "${home[$name]:-}" ]; then
      echo "kept $name on $host: it was not pulled home"
    elif [ "${home[$name]}" != "$sum" ]; then
      echo "kept $name on $host: the copy pulled home differs"
    else
      rm -f "$record"
      echo "removed $name from $host: the copy pulled home is the same"
    fi
  done
  if [ -f "$dir/records.tsv" ]; then echo "kept records.tsv on $host: a run's record, not yet kept under its own name"; fi
  if [ -n "$none" ]; then echo "no records-*.tsv of $instance on $host to clear"; fi
}

# container_removed <name> — stopped and removed; one already gone is no failure, one still there is.
container_removed() {
  docker stop -t 30 "$1" > /dev/null 2>&1 || true
  docker rm -f "$1" > /dev/null 2>&1 || true
  if docker container inspect "$1" > /dev/null 2>&1; then
    echo "on-gpu-host.sh: $1 is still on $(hostname): docker could not remove it" >&2
    exit 1
  fi
}

# follow_command <mode> <stamp> [NAME=value]... — follows the run run-state named; one that ended in
# between gives its own log and status.
follow_command() {
  local want=$1 named=$2 why
  shift 2
  run_probe
  case "$RUN" in
    going) following "$want" "$@" ;;
    busy) busy_instance ;;
  esac
  [ -f "$instance_dir/run/state" ] || die "no run of $instance is going on $(hostname)"
  read -r pid running log stamp digest envhash < "$instance_dir/run/state"
  [ "$stamp" = "$named" ] && [ -f "${log%.log}.status" ] || die "no run of $instance is going on $(hostname)"
  # Ended, the run is still held to what following it asks: its status must be this run's.
  why=$(mismatch "$want" "$@")
  [ -z "$why" ] || die "the $running run started $stamp ended with status $(cat "${log%.log}.status") before it was" \
    "followed, but $why; its log is $log. Run the command again: it starts the $want run now"
  cat "$log"
  exit "$(cat "${log%.log}.status")"
}

[ $# -gt 0 ] || die "want a command: install, pull, label, sf1, sf40-missing, sf40, settle, hold, busy, run, run-state, follow, cancel or clear; see --help"
command=$1
shift
case "$command" in
  -h|--help) usage ;;
  install) [ $# = 0 ] || die "install takes no argument"; install_missing ;;
  busy) [ $# = 0 ] || die "busy takes no argument"; busy ;;
  hold) [ $# -ge 1 ] && [ $# -le 2 ] || die "hold <seconds> [<instance dir>]"; hold "$@" ;;
  fetch) [ $# = 2 ] || die "fetch <bucket> <dir>"; fetch "$@" ;;
  runner) data_ready; runner "$@" ;;
  pull|label|sf1|sf40-missing|sf40|settle|run|run-state|follow|cancel|clear)
    data_ready
    active
    case "$command" in
      pull) [ $# = 1 ] || die "pull <image>"; docker pull "$1" ;;
      label) [ $# = 1 ] || die "label <image>"; label "$1" ;;
      sf1) [ $# = 1 ] || die "sf1 <build image>"; machine_data_set "$1" "$instance_dir"; echo "sf1: $DATA_SET" ;;
      sf40-missing) [ $# = 0 ] || die "sf40-missing takes no argument"; sf40_missing ;;
      sf40) [ $# -ge 2 ] || die "sf40 <build image> <dataset>..."; sf40 "$@" ;;
      settle)
        [ $# = 0 ] || die "settle takes no argument"
        run_lock
        if flock -n "$own"; then interrupted_kept; fi ;;
      run)
        [ $# -ge 2 ] || die "run <image id> <list|filter|bench> [NAME=value]..."
        case "$2" in list|filter|bench) ;; *) die "run takes list, filter or bench, not $2" ;; esac
        run "$@" ;;
      run-state)
        [ $# = 0 ] || die "run-state takes no argument"
        run_probe
        case "$RUN" in going) echo "running $running $stamp" ;; busy) busy_instance ;; *) echo idle ;; esac ;;
      follow) [ $# -ge 2 ] || die "follow <mode> <stamp> [NAME=value]..."; follow_command "$@" ;;
      cancel) [ $# = 0 ] || die "cancel takes no argument"; cancel ;;
      clear) [ $# = 0 ] || die "clear takes no argument"; records_cleared ;;
    esac ;;
  *) die "unknown command $command" ;;
esac
