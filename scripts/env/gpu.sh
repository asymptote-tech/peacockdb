#!/usr/bin/env bash
# The device run on a GPU host, from a built tree (llm-wiki/build-test.md, "GPU hosts").
#
#   gpu.sh <host> prepare [--cudf <v> | --image <tag>] [--instance <name>] [--env <file>]
#   gpu.sh <host> run <list|filter|bench> [--filter <f>] [--cudf <v> | --image <tag>] [--instance <name>]
#   gpu.sh <host> ship | pull [--clear] | cancel [--instance <name>]
#   gpu.sh <host> start | stop [--force]
# Outside an instance, --instance names the tree's dir on the host, ~/peacock/<name>/, once: the tree records
# it in .gpu-test/.instance. --env is the storage keys' file, peacock.env by default. prepare readies the
# host; ship sends the tree; run runs it there, detached, followed; pull brings the results home, and --clear
# then removes the host's records now home; cancel stops the run. A powered host is started and soft-stopped.
[ "${BASH_VERSINFO[0]}" -gt 4 ] || { [ "${BASH_VERSINFO[0]}" -eq 4 ] && [ "${BASH_VERSINFO[1]}" -ge 4 ]; } || {
  echo "${0##*/}: this machine's bash is $BASH_VERSION, older than 4.4: install a current one (on macOS, brew install bash)" >&2; exit 2; }
# Above, in bash 3.2's syntax and before any source: macOS ships 3.2, and what follows needs 4.4.
set -euo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/../lib/peacock-config.sh"
shopt -s inherit_errexit

die() { echo "gpu.sh: $*" >&2; exit 2; }
# Exit 3 is the driver's alone; a tool that fails, whatever its own status, is 1.
fail() { echo "gpu.sh: $*" >&2; exit 1; }
. "$here/../lib/image-inputs.sh"
. "$here/../lib/data-inputs.sh"
. "$here/../lib/machine.sh"
. "$here/../lib/host.sh"
. "$here/../lib/portable.sh"
tree=$(cd "$here/../.." && pwd)
# Three lines: the instance's name, the tree's id, its path. A host name begins with a letter or digit, so
# no host's .gpu-test/<name>/ is this file.
RECORD=.gpu-test/.instance

# What the host needs of the tree before anything else runs there: on-gpu-host.sh, the libraries it
# sources, and what the sf1 hash and its generation read.
TOOLS=(scripts/env/on-gpu-host.sh scripts/lib/peacock-config.sh scripts/lib/machine.sh scripts/lib/data-inputs.sh
       scripts/lib/s3.sh testdata/generate_testdata.sh docker/pins.env)
# The device tier's committed fixtures: CI's ship list, ci-ship.sh's TESTDATA, less the generator.
SHIPPED=(testdata/tpch-queries testdata/tpcds-queries testdata/tpch.minimal testdata/goldens
         testdata/tpch-vec-queries testdata/pbench-queries testdata/pbench.sf1 testdata/cost-registry.csv
         testdata/fixtures)

name="" verb="" cudf="" image="" instance="" env="" filter="" force="" clear="" words=()
take() { [ -z "${!1}" ] || die "one $2 per call"; printf -v "$1" %s "$3"; }
while [ $# -gt 0 ]; do
  case "$1" in
    -h|--help) peacock_usage "${BASH_SOURCE[0]}" ;;
    --cudf|--image|--instance|--env|--filter)
      [ -n "${2:-}" ] || die "$1 needs a value"
      take "${1#--}" "$1" "$2"
      shift 2 ;;
    --force) take force "$1" 1; shift ;;
    --clear) take clear "$1" 1; shift ;;
    -*) die "unknown flag $1" ;;
    *) if [ -z "$name" ]; then name=$1; elif [ -z "$verb" ]; then verb=$1; else words+=("$1"); fi; shift ;;
  esac
done
[ -n "$name" ] || die "name the host: gpu.sh <host> prepare|ship|run <mode>|pull|cancel|start|stop, see --help"
[ -n "$verb" ] || die "say what to do on $name: prepare, ship, run, pull, cancel, start or stop"
mode=""
case "$verb" in
  run)
    [ ${#words[@]} = 1 ] || die "run takes one of list, filter or bench, got: ${words[*]:-nothing}"
    mode=${words[0]}
    case "$mode" in list|filter|bench) ;; *) die "run takes one of list, filter or bench, not $mode" ;; esac ;;
  prepare|ship|pull|cancel|start|stop) [ ${#words[@]} = 0 ] || die "$verb takes no argument: ${words[*]}" ;;
  *) die "unknown command $verb: prepare, ship, run, pull, cancel, start or stop" ;;
esac
[ -z "$force" ] || [ "$verb" = stop ] || die "--force is for stop, not $verb"
[ -z "$clear" ] || [ "$verb" = pull ] || die "--clear is for pull, not $verb"
case "$verb" in
  prepare|run)
    [ -z "$image" ] || [ -z "${PEACOCK_IMAGE_TAG:-}" ] \
      || die "--image names the image outside an instance; this instance runs $PEACOCK_IMAGE_TAG"
    tag=${image:-${PEACOCK_IMAGE_TAG:-}}
    if [ -n "$tag" ]; then
      [[ "$tag" =~ ^cudf-([0-9]+\.[0-9]+)(-[0-9a-f]{12})?$ ]] || die "not an image tag cudf-<v>[-<commit>]: $tag"
      [ -z "$cudf" ] || die "the image $tag names its cuDF version (up.sh --cudf or --image, or --image here): drop --cudf"
      cudf=${BASH_REMATCH[1]}
    else
      cudf=${cudf:-25.02}
      tag=cudf-$cudf
    fi
    [ -f "$tree/docker/conda/cudf-$cudf.spec" ] || die "no cuDF $cudf: docker/conda has no cudf-$cudf.spec" ;;
  *) [ -z "$cudf$image" ] || die "--cudf and --image are for prepare and run, not $verb" ;;
esac
if [ -n "$env" ]; then
  [ "$verb" = prepare ] || die "--env is for prepare, not $verb"
  [ -z "${PEACOCK_INSTANCE:-}" ] || die "--env names the storage keys outside an instance; this instance has its own"
  [ -f "$env" ] || die "no env file $env"
fi
case "$verb" in
  start|stop) [ -z "$instance" ] || die "--instance is for prepare, ship, run, pull and cancel" ;;
  *)
    recorded="" tree_id="" fresh=""
    if [ -n "${PEACOCK_INSTANCE:-}" ]; then
      [ -z "$instance" ] || die "--instance names a tree outside an instance; this is instance $PEACOCK_INSTANCE"
      instance=$PEACOCK_INSTANCE tree_id=instance:$PEACOCK_INSTANCE
    elif [ -e "$tree/$RECORD" ]; then
      path="" here_path=$(cd "$tree" && pwd -P)
      { IFS= read -r recorded && IFS= read -r tree_id && IFS= read -r path; } < "$tree/$RECORD" || true
      [[ "$recorded" =~ ^[A-Za-z0-9][A-Za-z0-9._-]*$ && "$tree_id" =~ ^[0-9a-f]{32}$ && "$path" == /* ]] \
        || die "$RECORD is not an instance's name, an id and a path, a line each: remove it, and give --instance"
      [ "$path" = "$here_path" ] || die "$RECORD was written for $path, not this tree: a copy needs an instance" \
        "of its own, so remove $RECORD and give --instance <a new name>"
      [ -z "$instance" ] || [ "$instance" = "$recorded" ] \
        || die "this tree is instance $recorded, as $RECORD records, not $instance: to move it, remove $RECORD first"
      if [ -z "$instance" ]; then echo "instance $recorded, as $RECORD records for this tree"; fi
      instance=$recorded
    fi
    [ -n "$instance" ] || die "not in an instance: --instance <name> names this tree's dir on the host, ~/peacock/<name>/"
    [[ "$instance" =~ ^[A-Za-z0-9][A-Za-z0-9._-]*$ ]] || die "not an instance name: $instance"
    if [ -z "$tree_id" ]; then fresh=1 tree_id=$(od -An -N16 -tx1 /dev/urandom | tr -d ' \n'); fi ;;
esac

if [ -n "$filter" ]; then
  [ "$verb" = run ] || die "--filter is for run filter and run bench, not $verb"
  [ "$mode" != list ] || die "run list runs the whole list, but --filter $filter is given: run filter"
  [ -z "${PCK_TEST_FILTER:-}" ] || [ "$PCK_TEST_FILTER" = "$filter" ] \
    || die "--filter $filter and PCK_TEST_FILTER=$PCK_TEST_FILTER differ: give one"
fi
filter=${filter:-${PCK_TEST_FILTER:-}} resume=${PEACOCK_BENCH_RESUME:-}
case "$mode" in
  list) [ -z "$filter" ] || die "run list runs the whole list, but PCK_TEST_FILTER=$filter is set: run filter" ;;
  filter) [ -n "$filter" ] || die "run filter needs --filter <f> or PCK_TEST_FILTER" ;;
esac
case "$resume" in ""|0|1) ;; *) die "PEACOCK_BENCH_RESUME is unset, 0 or 1, not $resume" ;; esac
BEAT=${PEACOCK_HOLD_BEAT:-60} HOLD_TIMEOUT=${PEACOCK_HOLD_TIMEOUT:-300}
[[ "$HOLD_TIMEOUT" =~ ^[1-9][0-9]*$ ]] || die "PEACOCK_HOLD_TIMEOUT is whole seconds, not $HOLD_TIMEOUT"
[[ "$BEAT" =~ ^[1-9][0-9]*$ ]] && [ "$BEAT" -lt "$HOLD_TIMEOUT" ] \
  || die "PEACOCK_HOLD_BEAT is whole seconds below PEACOCK_HOLD_TIMEOUT ($HOLD_TIMEOUT), not $BEAT"
[ -z "$resume" ] || [ "$verb" != run ] || [ "$mode" = bench ] \
  || die "PEACOCK_BENCH_RESUME=$resume is set, but resuming is run bench's: unset it for run $mode"
host_read "$name"
host_ssh_options
if [ "$verb" = ship ]; then
  [ -f "$tree/cpp/install/lib/libpeacock_gpu.so" ] \
    || die "no $tree/cpp/install/lib/libpeacock_gpu.so: build and stage the binaries first"
fi

adapter=scripts/env/host/$HOST_PROVIDER.sh
# Whether the adapter can start and stop this host; why not, when it cannot.
powered=1 rc=0
power=$("$HOST_ADAPTER" "$name" powers 2>&1) || rc=$?
# 1 is the adapter's "powered by hand"; any other failure is its own, said as it is.
case "$rc" in
  0) ;;
  1) powered="" ;;
  2) die "$adapter cannot say whether it powers $name: ${power##*$'\n'}" ;;
  *) fail "$adapter $name powers failed (exit $rc): ${power##*$'\n'}" ;;
esac
by_hand="$power; start it by hand"
# powered_or_refused <verb> — refuses <verb> on a host its adapter cannot power.
powered_or_refused() { [ -n "$powered" ] || die "gpu.sh does not $1 $name: $power ($adapter)"; }
call_adapter() { "$HOST_ADAPTER" "$name" "$@" || fail "$adapter $name $1 failed (exit $?)"; }

dir=peacock/$instance
rsh="${HOST_SSH[*]}"
on() { "${HOST_SSH[@]}" -- "$remote" "$@"; }
# on_host <command> <arg>... — on-gpu-host.sh in this instance's dir on the host, its status as is.
on_host() { on "bash $dir/scripts/env/on-gpu-host.sh$(printf ' %q' "$@")"; }
# step <what> <command> <arg>... — on_host, its output passed on: a refusal there stays exit 2, a lost
# connection and any other failure are 1.
step() {
  local what=$1 rc=0
  shift
  on_host "$@" || rc=$?
  case "$rc" in
    0) ;;
    2) exit 2 ;;
    255) fail "lost the ssh connection to $name while $what" ;;
    127) die "nothing of $instance is on $name: gpu.sh $name prepare, then ship" ;;
    *) fail "$what failed on $name (exit $rc)" ;;
  esac
}
# Compressed: a ship or a pull waits on the laptop's uplink. -z, which every rsync takes; two of 3.2
# or later negotiate zstd for it.
synced() { local rc=0; rsync -z "$@" || rc=$?; [ "$rc" = 0 ] || fail "rsync with $name failed (exit $rc)"; }

# held [<instance dir>] — the host's activity lock, held shared through an ssh session that the
# script and then a heartbeat reach on stdin, until released: a soft stop, this instance's or another's,
# declines while this command works on the host. A session the heartbeat no longer reaches, its
# caller gone, lets go after PEACOCK_HOLD_TIMEOUT seconds. Sets HELD to held, or unheld on a host with
# no data directory yet. With an instance dir, the instance's run lock too.
held() {
  local line=""
  HELD=""
  # ssh alone is the coproc's work: when the session ends, the coproc says how, as a line, and ends.
  # It starts on a line from here, once its fds are copied: bash unsets HOLD as soon as it reaps it.
  coproc HOLD {
    IFS= read -r _
    rc=0
    on "bash -s -- hold $HOLD_TIMEOUT ${1:-}" < <(cat "$here/on-gpu-host.sh"; cat) || rc=$?
    echo "exit $rc"
  }
  exec {hold_out}<&"${HOLD[0]}" {hold_in}>&"${HOLD[1]}"
  eval "exec ${HOLD[0]}<&- ${HOLD[1]}>&-"
  echo go >&"$hold_in"
  read -r -t 120 line <&"$hold_out" || true
  if [ "$line" = held ] || [ "$line" = unheld ]; then
    HELD=$line
    # Only the loop's stdout reaches the session, never its sleep's. It beats while gpu.sh lives: killed
    # alone, with no EXIT trap, gpu.sh's hold then times out. Killed by released, it takes its sleep.
    ( eval "exec ${hold_in}>&- ${hold_out}<&-"
      s=""
      trap 'kill "$s" 2> /dev/null; exit 0' TERM
      while { sleep "$BEAT" > /dev/null & s=$!; wait "$s"; } && kill -0 $$ 2> /dev/null; do echo beat || exit 0; done ) \
      >&"$hold_in" 2> /dev/null &
    BEATER=$!
    return
  fi
  released
  case "$line" in
    "exit 2") exit 2 ;;
    "exit 255") fail "lost the ssh connection to $name while holding its activity lock" ;;
    *) fail "cannot hold $name's activity lock (${line:-no answer})" ;;
  esac
}
# released — the hold let go: the heartbeat stopped, the session's stdin closed, its end read through.
# A session that ended before that, its ssh lost or its read timed out, is said, whatever its status:
# the host was unheld for the rest of the command.
released() {
  local early=""
  if [ -n "${BEATER:-}" ]; then kill "$BEATER" 2> /dev/null || true; wait "$BEATER" 2> /dev/null || true; BEATER=""; fi
  if [ -n "${hold_in:-}" ]; then
    # Only our close ends a session that still holds: a line there before it is an end of its own.
    read -r -t 0.1 early <&"$hold_out" || true
    eval "exec ${hold_in}>&-"
    hold_in=""
  fi
  if [ -n "${hold_out:-}" ]; then
    cat <&"$hold_out" > /dev/null || true
    eval "exec ${hold_out}<&-"
    hold_out=""
  fi
  if [ "${HELD:-}" = held ] && [ -n "$early" ]; then
    echo "gpu.sh: the hold on $name ended early (${early}): $name was not held for all of this command" >&2
  fi
}

# tools_shipped — gpu.sh's own scripts into the instance's dir by tar: rsync may not be there yet. Each
# file is a new one, so a runner bash is still reading keeps the script it started with.
tools_shipped() {
  local rc=0
  tar -C "$tree" -cf - "${TOOLS[@]}" | on "mkdir -p $dir && $claim && tar --unlink-first -C $dir -xf -" || rc=$?
  case "$rc" in
    0) ;;
    255) fail "lost the ssh connection to $name while shipping gpu.sh's scripts" ;;
    *) fail "shipping gpu.sh's scripts to $name:~/$dir failed (exit $rc)" ;;
  esac
}
# idle_card — whether the host's card and the host are idle: no process on the card, no device run,
# no gpu.sh step of any instance. Unreachable over ssh, it is idle only when its adapter reports it
# STOPPED or STOPPING: a running host that does not answer may still be running a detached run.
idle_card() {
  local out rc=0 state
  # Its stderr kept with its answer, the last line: the reason a busy that fails gives.
  out=$(on "bash -s -- busy" < "$here/on-gpu-host.sh" 2>&1) || rc=$?
  out=${out##*$'\n'}
  case "$rc:$out" in
    0:idle) return 0 ;;
    0:busy:*) echo "gpu.sh: not stopping $name: ${out#busy: }" >&2; return 1 ;;
    255:*)
      state=$("$HOST_ADAPTER" "$name" status 2> /dev/null) || state=""
      case "$state" in STOPPED|STOPPING) return 0 ;; esac
      echo "gpu.sh: not stopping $name: its card cannot be seen over ssh and its state is ${state:-unknown};" \
           "gpu.sh $name stop --force stops it unchecked" >&2
      return 1 ;;
    *) echo "gpu.sh: not stopping $name: its card's state is unknown (exit $rc): ${out:-no answer}" >&2; return 1 ;;
  esac
}

# soft_stopped — at the exit of a command that started the host: stops it unless it is in use. Its
# own failure is said and never changes the command's status.
soft_stopped() {
  local ip
  if ! ip=$("$HOST_ADAPTER" "$name" ip 2> /dev/null); then
    echo "gpu.sh: not stopping $name: its address is unknown" >&2
    return
  fi
  remote=$HOST_USER@$ip
  idle_card || return 0
  "$HOST_ADAPTER" "$name" stop > /dev/null || echo "gpu.sh: stopping $name failed (exit $?): it may still be running" >&2
}

started=""
on_exit() {
  local status=$?
  released
  if [ -n "$started" ]; then soft_stopped; fi
  exit "$status"
}
trap on_exit EXIT

case "$verb" in
  start)
    powered_or_refused start
    call_adapter start
    exit 0 ;;
  stop)
    powered_or_refused stop
    if [ -n "$force" ]; then call_adapter stop; exit 0; fi
    if ip=$("$HOST_ADAPTER" "$name" ip 2> /dev/null); then
      remote=$HOST_USER@$ip
      idle_card || exit 2
    else
      # Unchecked only when the host is not running: an address the adapter cannot give is no proof.
      state=$("$HOST_ADAPTER" "$name" status 2> /dev/null) || state=""
      case "$state" in
        STOPPED|STOPPING) ;;
        *) die "cannot see $name's card: its address is unknown and its state is ${state:-unknown}; gpu.sh $name stop --force stops it unchecked" ;;
      esac
    fi
    call_adapter stop
    exit 0 ;;
esac

if [ -n "$powered" ]; then
  # A stopped host runs nothing: cancel does not start one to find that out.
  if [ "$verb" = cancel ]; then
    state=$("$HOST_ADAPTER" "$name" status 2> /dev/null) || state=""
    case "$state" in STOPPED|STOPPING) echo "$name is $state: no run of $instance is going"; exit 0 ;; esac
  fi
  call_adapter start
  started=1
fi
ip=$("$HOST_ADAPTER" "$name" ip) || fail "$adapter $name ip failed (exit $?)"
remote=$HOST_USER@$ip

# A refused key is a refusal; a host that does not answer is a lost connection, as everywhere else.
err=$(on true 2>&1 < /dev/null > /dev/null) || {
  if [[ "$err" == *"Permission denied"* ]]; then
    die "the GPU key does not log in to $name ($remote): ${err##*$'\n'}." \
        "provision-vm.sh --gpu-host $name <user>@<address> on the laptop puts it there"
  fi
  if [ -n "$powered" ]; then hint="check it: $adapter $name status"; else hint=$by_hand; fi
  fail "cannot reach $name ($remote): ${err##*$'\n'}. If the host is down, $hint"
}

# The instance's dir is one tree's: its .tree holds the id of the tree that made it, or of the first to
# reach a dir made before .tree was. Another tree's command, a copy's or a namesake's, is refused before
# it ships, runs or clears anything. A dir not made yet is claimed by the command that makes it.
claim="{ (set -C && echo $tree_id > $dir/.tree) 2> /dev/null || true; }"
rc=0
owner=$(on "if [ -d $dir ]; then $claim && cat $dir/.tree; fi" < /dev/null) || rc=$?
[ "$rc" != 255 ] || fail "lost the ssh connection to $name while reading ~/$dir/.tree"
[ "$rc" = 0 ] || fail "reading ~/$dir/.tree on $name failed (exit $rc)"
if [ -n "$owner" ] && [ "$owner" != "$tree_id" ]; then
  if [ -n "${PEACOCK_INSTANCE:-}" ]; then die "~/$dir on $name is another tree's, not instance $instance's"; fi
  die "~/$dir on $name is another tree's: give this tree an instance name no one else on $HOST_USER@$name uses"
fi
if [ -n "$fresh" ]; then
  mkdir -p "$tree/.gpu-test"
  printf '%s\n' "$instance" "$tree_id" "$(cd "$tree" && pwd -P)" > "$tree/$RECORD"
fi

pin() { sed -n "s/^$1=//p" "$tree/docker/pins.env"; }

# driver_checked — exit 3 unless every card's driver is at least what the run image needs: the
# version's minimum, and for a card of compute capability other than 8.x or 9.x, which JIT-compiles
# the PTX, DRIVER_MINIMUM_PTX as well.
driver_checked() {
  local key=DRIVER_MINIMUM_${cudf//./_} minimum ptx answer rc=0 line v cap need why
  minimum=$(pin "$key") ptx=$(pin DRIVER_MINIMUM_PTX)
  for v in "$minimum" "$ptx"; do
    [[ "$v" =~ ^[0-9]+(\.[0-9]+)+$ ]] || die "docker/pins.env holds no single $key and DRIVER_MINIMUM_PTX"
  done
  answer=$(on 'nvidia-smi --query-gpu=driver_version,compute_cap --format=csv,noheader' < /dev/null) || rc=$?
  [ "$rc" != 255 ] || fail "lost the ssh connection to $name while reading its driver"
  [ "$rc" = 0 ] || die "nvidia-smi failed on $name (exit $rc): no NVIDIA driver answers there"
  while IFS= read -r line; do
    [[ "$line" =~ ^([0-9]+(\.[0-9]+)+),\ (([0-9]+)\.[0-9]+)$ ]] \
      || die "nvidia-smi on $name answered '$line', not a driver version and a compute capability"
    v=${BASH_REMATCH[1]} cap=${BASH_REMATCH[4]}
    need=$minimum why="the oldest the cuDF $cudf run image runs on"
    [ "$cap" -ge 8 ] || die "the card on $name has compute capability ${BASH_REMATCH[3]}: the images run on 8.0 and later"
    if [ "$cap" != 8 ] && [ "$cap" != 9 ] && version_at_least "$ptx" "$minimum"; then
      need=$ptx why="the oldest that JIT-compiles the PTX for a card of compute capability $cap.x"
    fi
    if ! version_at_least "$v" "$need"; then
      echo "gpu.sh: the driver on $name is $v, older than $need, $why" >&2
      exit 3
    fi
  done <<< "$answer"
}

# image_checked <image> <build|run> — sets IMAGE_ID to the image <image> names on the host, refusing
# it, naming each input that differs, unless its peacockdb.inputs label is the tree's.
image_checked() {
  local ref=$1 kind=$2 out label differ rc=0
  out=$(on_host label "$ref" < /dev/null) || rc=$?
  case "$rc" in
    0) ;;
    255) fail "lost the ssh connection to $name while reading $ref" ;;
    *) die "$ref is not on $name: gpu.sh $name prepare pulls it" ;;
  esac
  { IFS= read -r IMAGE_ID; IFS= read -r label; } <<< "$out"
  [[ "$IMAGE_ID" =~ ^sha256:[0-9a-f]{64}$ ]] || die "docker on $name gave no image ID for $ref: $IMAGE_ID"
  if [ -z "$label" ] || [ "$label" = "<no value>" ]; then
    die "$ref on $name has no peacockdb.inputs label: publish it again with scripts/env/image.sh"
  fi
  differ=$(image_inputs_differ "$label" "$tree" "$kind" "$cudf")
  [ -z "$differ" ] || die "$ref on $name was built from other inputs than this tree: $(paste -sd' ' - <<< "$differ")" \
                          "(gpu.sh $name prepare pulls :$tag again)"
}

# merged <staged dir> <dir> <newer|never> — each staged file into <dir>. A local file that differs is
# kept and named, and the pull exits 1: under newer when it is the newer of the two, under never always.
merged() {
  local from=$1 to=$2 rule=$3 rel
  while IFS= read -r -d '' rel; do
    rel=${rel#./}
    if [ -e "$to/$rel" ] && ! cmp -s "$from/$rel" "$to/$rel"; then
      if [ "$rule" = never ]; then
        echo "gpu.sh: kept ${to#"$tree"/}/$rel: it differs from $name's, and a record is never replaced" >&2
        kept=1
        continue
      elif [ "$to/$rel" -nt "$from/$rel" ]; then
        echo "gpu.sh: kept ${to#"$tree"/}/$rel: it is newer here than on $name" >&2
        kept=1
        continue
      fi
    fi
    mkdir -p "$(dirname "$to/$rel")"
    cp -p "$from/$rel" "$to/$rel"
  done < <(cd "$from" && find . -type f -print0)
}

# finished <status> — exits with a run's status: its own 0, 1 or 2, and 1 for anything else. A lost
# session says whether the host is still there to follow the run on.
finished() {
  case "$1" in
    0|1|2) exit "$1" ;;
    255)
      if on true < /dev/null 2> /dev/null; then
        fail "the ssh session to $name dropped; the run goes on there, and gpu.sh $name run $mode follows it"
      fi
      fail "lost $name while following the run: it is down or unreachable" ;;
    *) fail "the run on $name ended with exit $1" ;;
  esac
}

run_ref=$MACHINE_RUN_IMAGE:${tag:-}
build_ref=$MACHINE_IMAGE:${tag:-}
case "$verb" in
  prepare)
    held
    driver_checked
    tools_shipped
    step "installing" install < /dev/null
    # A first prepare made the data directory just now: the lock is held from here on.
    if [ "$HELD" = unheld ]; then released; held; fi
    step "docker pull $run_ref" pull "$run_ref" < /dev/null
    image_checked "$run_ref" run
    step "docker pull $build_ref" pull "$build_ref" < /dev/null
    image_checked "$build_ref" build
    build_id=$IMAGE_ID
    step "generating sf1" sf1 "$build_id" < /dev/null
    missing=$(step "listing sf40" sf40-missing < /dev/null)
    if [ -n "$missing" ]; then
      if [ -n "${PEACOCK_INSTANCE:-}" ]; then
        for key in "${PEACOCK_STORAGE_KEYS[@]}"; do
          [ -n "${!key:-}" ] || die "prepare needs $key in the environment, for sf40: peacock.env holds it in an instance"
        done
      else
        peacock_env_storage_read "${env:-$PEACOCK_ENV}"
      fi
      mapfile -t sets <<< "$missing"
      # The storage keys on ssh's stdin, for that one command: never argv, never a file on the host.
      for key in "${PEACOCK_STORAGE_KEYS[@]}"; do printf '%s=%s\n' "$key" "${!key}"; done \
        | step "fetching sf40" sf40 "$build_id" "${sets[@]}"
    fi
    echo "$name is ready for $instance: gpu.sh $name ship" ;;
  ship)
    # The instance's run lock too, for the whole ship: the binaries are a run's while it goes.
    held "$dir"
    [ "$HELD" = held ] || die "$name has no data directory $HOST_USER can write yet: gpu.sh $name prepare first"
    tools_shipped
    on "mkdir -p $dir/cpp/install $dir/testdata" < /dev/null || fail "making ~/$dir on $name failed over ssh (exit $?)"
    # The host's hold finds these rsyncs' servers by the dir in their argv, which protected args leave out.
    RSYNC_PROTECT_ARGS=0 synced -a --delete --timeout="$HOLD_TIMEOUT" -e "$rsh" "$tree/cpp/install/" "$remote:$dir/cpp/install/"
    (cd "$tree" && RSYNC_PROTECT_ARGS=0 synced -a --delete --relative --timeout="$HOLD_TIMEOUT" -e "$rsh" \
      "${SHIPPED[@]}" "${TOOLS[@]}" "$remote:$dir/")
    echo "shipped to $name:~/$dir" ;;
  run)
    held
    pass=()
    if [ -n "$filter" ]; then pass+=("PCK_TEST_FILTER=$filter"); fi
    if [ -n "$resume" ]; then pass+=("PEACOCK_BENCH_RESUME=$resume"); fi
    state=$(step "reading the run's state" run-state < /dev/null)
    status=0
    if [[ "$state" == "running "* ]]; then
      read -r _ _ stamp <<< "$state"
      on_host follow "$mode" "$stamp" "${pass[@]}" < /dev/null || status=$?
      finished "$status"
    fi
    driver_checked
    image_checked "$run_ref" run
    on_host run "$IMAGE_ID" "$mode" "${pass[@]}" < /dev/null || status=$?
    finished "$status" ;;
  pull)
    held
    if [ -n "$clear" ]; then
      state=$(step "reading the run's state" run-state < /dev/null)
      if [[ "$state" == "running "* ]]; then
        read -r _ running stamp <<< "$state"
        die "the $running run of $instance started $stamp is still going on $name: pull without --clear, or once it ends"
      fi
    fi
    step "keeping a killed run's record" settle < /dev/null
    # Every source made first, as an rsync without --ignore-missing-args (macOS's) needs.
    on "mkdir -p $dir/testdata/benchmark-results $dir/testdata/calibration $dir/logs && $claim" < /dev/null \
      || fail "making ~/$dir on $name failed over ssh (exit $?)"
    stage=$tree/.gpu-test/$name/pull
    rm -rf "$stage"
    mkdir -p "$stage/testdata/benchmark-results" "$stage/testdata/calibration" "$stage/logs"
    synced -a -e "$rsh" "$remote:$dir/testdata/benchmark-results/" "$stage/testdata/benchmark-results/"
    synced -a --include='records-*.tsv' --exclude='*' -e "$rsh" \
      "$remote:$dir/testdata/calibration/" "$stage/testdata/calibration/"
    synced -a -e "$rsh" "$remote:$dir/logs/" "$stage/logs/"
    kept=""
    merged "$stage/testdata/calibration" "$tree/testdata/calibration" never
    # What the host may clear: each record pulled, by the sha256 of the copy now home.
    home=()
    if [ -n "$clear" ]; then
      while IFS= read -r -d '' rel; do
        rel=${rel#./}
        if [ -f "$tree/testdata/calibration/$rel" ]; then
          read -r sum _ < <(sha256sum < "$tree/testdata/calibration/$rel")
          home+=("$sum $rel")
        fi
      done < <(cd "$stage/testdata/calibration" && find . -type f -print0)
    fi
    rm -rf "$stage/testdata/calibration"
    merged "$stage/testdata" "$tree/testdata" newer
    merged "$stage/logs" "$tree/.gpu-test/$name/logs" newer
    rm -rf "$stage"
    if [ -n "$clear" ]; then
      rc=0
      if [ ${#home[@]} -gt 0 ]; then printf '%s\n' "${home[@]}"; fi | on_host clear || rc=$?
      case "$rc" in
        0) ;;
        2) echo "gpu.sh: the pull itself completed; $name cleared nothing" >&2; exit 2 ;;
        255) fail "lost the ssh connection to $name while clearing its records; the pull itself completed" ;;
        *) fail "clearing the records failed on $name (exit $rc); the pull itself completed" ;;
      esac
    fi
    [ -z "$kept" ] || exit 1
    echo "pulled from $name: testdata/benchmark-results/, testdata/calibration/records-*.tsv, .gpu-test/$name/logs/" ;;
  cancel)
    held
    step "cancelling the run" cancel < /dev/null ;;
esac
