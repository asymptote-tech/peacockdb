# shellcheck shell=bash
#
# ~/.config/peacock/, on a person's own machine (the laptop) and on a VM (llm-wiki/build-test.md,
# "Containers"): the storage keys, the person's PAT and GPU key, the VMs this laptop provisioned,
# the GPU hosts, and the marker bootstrap-vm.sh leaves on a machine that runs instances. A function that refuses
# prints one line to stderr and returns 2; it is called as a statement, so set -e stops the caller.

PEACOCK_CONFIG=$HOME/.config/peacock
PEACOCK_ENV=$PEACOCK_CONFIG/peacock.env
PEACOCK_PAT=$PEACOCK_CONFIG/github-pat
PEACOCK_GPU_KEY=$PEACOCK_CONFIG/gpu_ed25519
PEACOCK_VMS=$PEACOCK_CONFIG/vm
PEACOCK_MARKER=$PEACOCK_CONFIG/machine
# One file a GPU host, <name>.env, written by provision-vm.sh --gpu-host (scripts/lib/host.sh).
PEACOCK_HOSTS=$PEACOCK_CONFIG/hosts
# bootstrap-vm.sh's clone, relative to the home of the user who ran it, and the repository it clones,
# which the VMs' PAT is made for whatever the laptop clone's origin is.
PEACOCK_CLONE_IN_HOME=peacockdb
PEACOCK_REPO=asymptote-tech/peacockdb
# What peacock.env holds on a laptop, and must hold on a VM.
PEACOCK_STORAGE_KEYS=(AWS_ACCESS_KEY_ID AWS_SECRET_ACCESS_KEY AWS_ENDPOINT_URL AWS_REGION)

peacock_refuse() { echo "${0##*/}: $*" >&2; return 2; }

# peacock_usage <script> — prints <script>'s header comment, below its first line, without the #,
# and exits. Bash alone: a laptop's sed may be BSD's.
peacock_usage() {
  local line
  {
    IFS= read -r line
    while IFS= read -r line && [[ "$line" == '#'* ]]; do
      line=${line#\#}
      printf '%s\n' "${line# }"
    done
  } < "$1"
  exit 0
}

# peacock_env_normalised <file> <copy> — <file> with every CR dropped and a final newline, as <copy>
# (mode 600): a line appended later must not run onto the last key. Written beside <copy> and moved
# into place, so <file> may be <copy>.
peacock_env_normalised() {
  local next
  next=$(mktemp "$2.XXXXXX")
  awk '{ gsub(/\r/, ""); print }' "$1" > "$next"
  mv "$next" "$2"
}

# peacock_env_has_storage <file> — refuses unless each storage key is in <file> once and not
# empty. A value is never read, so none can be printed.
peacock_env_has_storage() {
  local key
  for key in "${PEACOCK_STORAGE_KEYS[@]}"; do
    case "$(grep -c "^$key=" "$1" || true)" in
      0) peacock_refuse "$1 has no $key"; return ;;
      1) grep -q "^$key=." "$1" || { peacock_refuse "$key is empty in $1"; return; } ;;
      *) peacock_refuse "$1 names $key more than once"; return ;;
    esac
  done
}

# peacock_env_is_storage_only <file> — refuses a line that is not one of the storage keys, naming
# the key, or the line's number when it is not NAME=value: the file is passed between developers,
# and a token must not ride along. Blank and # lines are skipped, as docker skips them.
peacock_env_is_storage_only() {
  local n=0 line name
  while IFS= read -r line || [ -n "$line" ]; do
    n=$((n + 1))
    if [[ "$line" =~ ^[[:space:]]*(#|$) ]]; then continue; fi
    if [[ ! "$line" =~ ^([A-Za-z_][A-Za-z0-9_]*)= ]]; then
      peacock_refuse "$1: line $n is not NAME=value; it holds only the storage keys ${PEACOCK_STORAGE_KEYS[*]}"
      return
    fi
    name=${BASH_REMATCH[1]}
    if [[ " ${PEACOCK_STORAGE_KEYS[*]} " != *" $name "* ]]; then
      peacock_refuse "$1 holds $name; it may hold only the storage keys ${PEACOCK_STORAGE_KEYS[*]}"
      return
    fi
  done < "$1"
}

# peacock_vm_target <name> — sets PEACOCK_VM_TARGET to <user>@<address> of VM <name> in the vm
# file, of its last line when <name> is empty.
peacock_vm_target() {
  local name=$1 n=0 line words found=""
  [ -s "$PEACOCK_VMS" ] || {
    peacock_refuse "this machine runs no instances and knows no VM: run scripts/env/provision-vm.sh <login>@<address> first"
    return
  }
  while IFS= read -r line || [ -n "$line" ]; do
    n=$((n + 1))
    read -r -a words <<< "$line"
    [ ${#words[@]} = 2 ] && [[ "${words[1]}" =~ ^[^@[:space:]]+@[^@[:space:]]+$ ]] \
      || { peacock_refuse "$PEACOCK_VMS: line $n is not <name> <user>@<address>"; return; }
    if [ -z "$name" ] || [ "${words[0]}" = "$name" ]; then found=${words[1]}; fi
  done < "$PEACOCK_VMS"
  [ -n "$found" ] || { peacock_refuse "no VM $name in $PEACOCK_VMS"; return; }
  PEACOCK_VM_TARGET=$found
}

# peacock_forward <script> <vm> <arg>... — on a machine without the marker, runs the VM's
# scripts/env/<script> with <arg>... over ssh, as the user the vm file records, on VM <vm> or the
# default, and exits with its status. On a machine with the marker it returns, refusing a <vm>.
# A forwarded call that lands on a machine without the marker stops there rather than hop again.
peacock_forward() {
  local script=$1 vm=$2
  shift 2
  if [ -e "$PEACOCK_MARKER" ]; then
    [ -z "$vm" ] || peacock_refuse "--vm is for a machine that forwards to a VM; this one runs instances"
    return
  fi
  if [ -n "${PEACOCK_FORWARDED:-}" ]; then
    peacock_refuse "forwarded here, to a machine without $PEACOCK_MARKER: provision this VM with provision-vm.sh"
    return
  fi
  peacock_vm_target "$vm"
  # One string, as ssh hands it to the remote shell: each argument quoted, $HOME left to the VM.
  exec ssh -- "$PEACOCK_VM_TARGET" \
    "PEACOCK_FORWARDED=1 \$HOME/$PEACOCK_CLONE_IN_HOME/scripts/env/$script$(printf ' %q' "$@")"
}
