# shellcheck shell=bash
# shellcheck disable=SC2154  # root, dir, instance, NEBIUS, TREE_RECORD and tree_path: the caller's
#
# Dev's side of a GPU VM's dir, for each script that runs there (run-gpu.sh, run-sirius.sh): the VM
# reached through nebius.sh, the syncs, the dir's claim, the pull set and its stamps, an image read
# with its labels, and the cards held to it. The VM's side is run-gpu-vm.sh, sourced here.
#
# The caller sets root (the tree), dir (<i>, the VM dir ~/peacock/<i>/), NEBIUS (nebius.sh and its
# options, an array), instance (--instance-id's value), TREE_RECORD and tree_path (the tree's id
# file and its real path), and a die that exits 2 naming its script; then sources this, runs
# vm_options_checked, and tree_id_read. Its own messages name the caller, ${0##*/}, as machine.sh's
# do; following names run-gpu.sh's follow, which follows any run of the dir.

. "$root/scripts/lib/peacock-config.sh"
. "$root/scripts/lib/image-inputs.sh"
. "$root/scripts/lib/data-inputs.sh"
. "$root/scripts/lib/machine.sh"
MACHINE_NEBIUS="scripts/nebius.sh ${NEBIUS[*]:1}"
. "$root/scripts/lib/ssh-retry.sh"

# vm_options_checked: refuses, exit 2, a dir name that is no single name under ~/peacock/, and a VM
# not named by --instance-id.
vm_options_checked() {
  # Not cudf-*: ~/peacock/cudf-<v>/ is the run image's prefix.
  [[ "$dir" =~ ^[A-Za-z0-9][A-Za-z0-9._-]*$ && "$dir" != cudf-* ]] \
    || die "--dir $dir: a directory name under ~/peacock/, of letters, digits, '.', '_' and '-', not cudf-*"
  [ -n "$instance" ] \
    || die "name the VM with --instance-id, nebius-gpu's too: without it nebius.sh picks nebius-gpu silently"
}

# tree_id_read: sets TREE_ID, the tree's id, which a VM dir keeps as its owner's (run-gpu.sh's
# usage): PEACOCK_TREE_ID, else .run-gpu-tree's, made by tree_claimed; empty while the tree has no
# record. Refuses, exit 2, a record made for another path.
tree_id_read() {
  local lines=()
  if [ -n "${PEACOCK_TREE_ID+set}" ]; then
    [[ "$PEACOCK_TREE_ID" =~ ^[A-Za-z0-9._-]+$ ]] \
      || die "PEACOCK_TREE_ID=$PEACOCK_TREE_ID: a tree's id is letters, digits, '.', '_' and '-'"
    TREE_ID=$PEACOCK_TREE_ID
    return 0
  fi
  TREE_ID=""
  [ -f "$TREE_RECORD" ] || return 0
  mapfile -t lines < "$TREE_RECORD"
  [ "${lines[1]:-}" = "$tree_path" ] \
    || die ".run-gpu-tree was written for ${lines[1]:-no path}, not this tree: a copy or a moved tree" \
           "needs an id of its own: remove .run-gpu-tree, then name a dir of its own with --dir <name>"
  [[ "${lines[0]:-}" =~ ^[0-9a-f]{32}$ ]] || die "$TREE_RECORD holds no id: remove it"
  TREE_ID=${lines[0]}
}

# --- dev's side: every call reaches the VM through nebius.sh -------------------------------------

# vm <command> [<stdin file>]: <command> on the VM; stdin is /dev/null unless named. 255 is ssh's
# own, a dropped connection, so the command's own 255 (nvidia-smi's "other error") arrives as 254.
# The trailer that does it follows on a new line, so <command> must be complete: a trailing
# backslash, an open quote or an unterminated heredoc would swallow it.
# VM_NEXT names the step after a dropped ssh's VM start, when it is not this one again.
vm() {
  # shellcheck disable=SC2016  # expanded on the VM, after the command
  local rc=0 own='s=$?; [ "$s" -ne 255 ] || s=254; exit "$s"'
  "${NEBIUS[@]}" ssh "$1"$'\n'"$own" < "${2:-/dev/null}" || rc=$?
  [ "$rc" -ne 64 ] || die "${NEBIUS[0]} refused its options: ${NEBIUS[*]:1}"
  if [ "$rc" -eq 255 ]; then
    echo "${0##*/}: the ssh to the VM dropped." >&2
    machine_vm_lost "${VM_NEXT:-}"
  fi
  return "$rc"
}

# What every function on_vm sends runs with: the helpers, and what starts a runner.
VM_HELPERS=(vm_docker vm_state vm_started_at vm_status_code vm_container vm_name_free vm_name_taken vm_idle
            vm_locked vm_started vm_follow vm_runner vm_runner_ended vm_staged_judged peacock_card_take
            vm_records_kept vm_stamped vm_clearable vm_log_shown in_pulled_set)

# on_vm <function> <arg…>: a function of run-gpu-vm.sh, sent as text and run on the VM under bash,
# with set -euo pipefail and the helpers. on_vm_with <stdin file> <function> <arg…>: the same, the
# file on its stdin.
on_vm() { on_vm_with /dev/null "$@"; }
on_vm_with() {
  local input=$1 fn=$2 script args=""
  shift 2
  script="set -euo pipefail; $(declare -f "${VM_HELPERS[@]}" "$fn"); $fn \"\$@\""
  [ $# -eq 0 ] || args=$(printf ' %q' "$@")
  vm "bash -c $(printf '%q' "$script") run-gpu.sh$args" "$input"
}

# in_pulled_set <path under ~/peacock/<i>/>: the path is one pull brings home, here and on the VM:
# the results, the calibration files but a going run's records.tsv, the device's answers, the logs.
in_pulled_set() {
  case $1 in
    testdata/calibration/records.tsv) return 1 ;;
    testdata/benchmark-results/*|testdata/calibration/*|logs/*) return 0 ;;
    testdata/goldens/*) [[ ${1##*/} == gpu-result*.txt ]] ;;
    *) return 1 ;;
  esac
}

sha256_of() {
  local line
  line=$(sha256sum < "$1")
  echo "${line%% *}"
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
  local rc=0 next="follow it again: scripts/env/run-gpu.sh ${NEBIUS[*]:1} --dir $dir follow"
  trap 'followed_no_more 130' INT
  trap 'followed_no_more 143' TERM
  set -m
  # A run in progress refuses this step again: after a dropped ssh, the next step is follow.
  { VM_NEXT=$next; on_vm "$@"; } &
  follower=$!
  set +m
  wait "$follower" || rc=$?
  trap - INT TERM
  [ "$rc" -ne 255 ] || followed_no_more 1
  return "$rc"
}
followed_no_more() {
  if [ -n "${follower:-}" ]; then kill -TERM -- "-$follower" 2> /dev/null || true; fi
  echo "${0##*/}: no longer following; the run goes on, on the VM:" \
       "scripts/env/run-gpu.sh ${NEBIUS[*]:1} --dir $dir follow follows it" >&2
  exit "$1"
}

# docker, for machine_image_checked and the label reads: the VM's. Nothing runs a container here.
# shellcheck disable=SC2032  # vm_docker's `sudo docker` runs the VM's binary, never this
docker() { on_vm vm_docker_run "$@"; }

# image_read <ref> <kind> <cudf>: sets IMAGE_ID to the ID of the image <ref> names on the VM, and
# IMAGE_MINIMUM and IMAGE_MINIMUM_PTX to its two driver labels, all from one read, so a tag moved
# since is never run. The PTX label is a run image's alone; for another kind IMAGE_MINIMUM_PTX is
# empty. Refuses, exit 2, an image missing a label or built from other inputs than this tree.
image_read() {
  local ref=$1 kind=$2 cudf=$3 labels=(peacockdb.inputs peacockdb.driver-minimum) fmt='{{.Id}}' out
  local values=() i rc=0
  [ "$kind" != run ] || labels+=(peacockdb.driver-minimum-ptx)
  for i in "${labels[@]}"; do fmt+="{{\"\\n\"}}{{index .Config.Labels \"$i\"}}"; done
  out=$(docker image inspect --format "$fmt" "$ref") || rc=$?
  [ "$rc" -ne 1 ] || die "no $ref on the VM: ${0##*/} prepare pulls it"
  [ "$rc" -eq 0 ] || exit "$rc"
  # $(…) drops the lines of empty labels at the end, so each is read with a default.
  mapfile -t values <<< "$out"
  machine_image_inputs_checked "${values[1]:-}" "$ref" "$root" "$cudf" "$kind"
  for ((i = 2; i <= ${#labels[@]}; i++)); do
    if [ -z "${values[i]:-}" ] || [ "${values[i]}" = "<no value>" ]; then
      die "$ref has no ${labels[i - 1]} label: publish it again with scripts/env/image.sh"
    fi
  done
  # shellcheck disable=SC2034  # IMAGE_ID is read by the caller, which runs the image by it
  IMAGE_ID=${values[0]} IMAGE_MINIMUM=${values[2]} IMAGE_MINIMUM_PTX=${values[3]:-}
}

version_below() { [ "$1" != "$2" ] && [ "$(printf '%s\n' "$1" "$2" | sort -V | head -1)" = "$1" ]; }

# cards_checked <ref>: refuses, exit 3, a card below compute capability 8.0, which the image's
# binaries do not run on, or one whose driver is older than IMAGE_MINIMUM. A card past 9.x
# JIT-compiles a run image's PTX, so its driver is held to IMAGE_MINIMUM_PTX as well, when set.
cards_checked() {
  local ref=$1 answer rc=0 line driver cap major need why
  # Both streams: a missing nvidia-smi is bash's `command not found`, on stderr.
  answer=$(vm "nvidia-smi --query-gpu=driver_version,compute_cap --format=csv,noheader 2>&1") || rc=$?
  case $rc in
    0) ;;
    69|75) exit "$rc" ;;  # nebius.sh's: the VM, not its card
    254) die "the driver query on the VM failed (exit 254 (255 on the VM)): ${answer:-no output}" ;;
    *) die "the driver query on the VM failed (exit $rc): ${answer:-no output}" ;;
  esac
  [ -n "$answer" ] || die "nvidia-smi on the VM lists no GPU"
  while IFS= read -r line; do
    [[ "$line" =~ ^([0-9]+(\.[0-9]+)+),\ (([0-9]+)\.[0-9]+)$ ]] \
      || die "nvidia-smi on the VM answered '$line', not a card's driver version and compute capability"
    driver=${BASH_REMATCH[1]} cap=${BASH_REMATCH[3]} major=${BASH_REMATCH[4]}
    if [ "$major" -lt 8 ]; then
      echo "${0##*/}: a card on the VM has compute capability $cap; $ref runs on 8.0 and later" >&2
      exit 3
    fi
    need=$IMAGE_MINIMUM why="the oldest $ref runs on"
    if [ -n "$IMAGE_MINIMUM_PTX" ] && [ "$major" -ne 8 ] && [ "$major" -ne 9 ] \
       && version_below "$need" "$IMAGE_MINIMUM_PTX"; then
      need=$IMAGE_MINIMUM_PTX why="the oldest that JIT-compiles $ref's PTX for a card of compute capability $cap"
    fi
    if version_below "$driver" "$need"; then
      echo "${0##*/}: a card's driver on the VM is $driver, older than $need, $why" >&2
      exit 3
    fi
  done <<< "$answer"
}

# tree_claimed: the dir on the VM made or claimed for this tree, or refused, exit 2, as another's;
# the first VM step of every verb that writes there. The record is made first, by a link, so a call
# killed between the two never leaves a dir claimed by an id the tree has lost, and two calls at once
# keep one id.
tree_claimed() {
  local tmp
  if [ -z "$TREE_ID" ]; then
    tmp=$(mktemp "$TREE_RECORD.XXXXXX")
    printf '%s\n%s\n' "$(od -An -N16 -tx1 /dev/urandom | tr -d ' \n')" "$tree_path" > "$tmp"
    ln "$tmp" "$TREE_RECORD" 2> /dev/null || true
    rm -f "$tmp"
    tree_id_read
    # The link fails on a name taken by another call's record, and also on a dangling link or a
    # directory there, or a filesystem without hard links: then there is still no record.
    [ -n "$TREE_ID" ] || die "could not make $TREE_RECORD: remove what is there, or set PEACOCK_TREE_ID"
  fi
  on_vm vm_tree_claimed "$dir" "$TREE_ID"
}

. "$root/scripts/env/run-gpu-vm.sh"
