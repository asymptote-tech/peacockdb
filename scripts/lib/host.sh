# shellcheck shell=bash
#
# A GPU host, as its file ~/.config/peacock/hosts/<name>.env describes it (llm-wiki/build-test.md,
# "GPU hosts"): PROVIDER, HOST, USER, and what its provider needs. scripts/env/host/<provider>.sh is
# the only place a provider is named; every ssh to a host carries the person's key, GPU_SSH_KEY_FILE.
# Sourced after peacock-config.sh. A function that refuses prints one line and returns 2, and is
# called as a statement, so set -e stops the caller.

HOST_ADAPTERS=$(cd "$(dirname "${BASH_SOURCE[0]}")/../env/host" && pwd)
HOST_FILE_KEYS=(PROVIDER HOST USER INSTANCE_ID)

host_name_is_valid() { [[ "$1" =~ ^[A-Za-z0-9][A-Za-z0-9._-]*$ ]]; }

# host_read <name> — sets HOST_NAME, HOST_PROVIDER, HOST_ADDRESS, HOST_USER, HOST_ADAPTER and, for
# every key of the file, HOST_FILE[<key>]. A key it does not know is refused by its name.
host_read() {
  local name=$1 file line key n=0
  host_name_is_valid "$name" || { peacock_refuse "not a host name: $name"; return; }
  file=$PEACOCK_HOSTS/$name.env
  [ -f "$file" ] || {
    peacock_refuse "no host $name: no $file, which provision-vm.sh --gpu-host $name <user>@<address> writes on the laptop"
    return
  }
  declare -gA HOST_FILE=()
  while IFS= read -r line || [ -n "$line" ]; do
    n=$((n + 1))
    if [[ "$line" =~ ^[[:space:]]*(#|$) ]]; then continue; fi
    [[ "$line" =~ ^([A-Z_]+)=([^[:space:]]+)$ ]] || { peacock_refuse "$file: line $n is not KEY=value"; return; }
    key=${BASH_REMATCH[1]}
    [[ " ${HOST_FILE_KEYS[*]} " == *" $key "* ]] \
      || { peacock_refuse "$file holds $key; a host file holds only ${HOST_FILE_KEYS[*]}"; return; }
    [ -z "${HOST_FILE[$key]:-}" ] || { peacock_refuse "$file names $key more than once"; return; }
    HOST_FILE[$key]=${BASH_REMATCH[2]}
  done < "$file"
  for key in PROVIDER HOST USER; do
    [ -n "${HOST_FILE[$key]:-}" ] || { peacock_refuse "$file has no $key"; return; }
  done
  HOST_NAME=$name HOST_PROVIDER=${HOST_FILE[PROVIDER]} HOST_ADDRESS=${HOST_FILE[HOST]} HOST_USER=${HOST_FILE[USER]}
  HOST_ADAPTER=$HOST_ADAPTERS/$HOST_PROVIDER.sh
  [[ "$HOST_PROVIDER" =~ ^[a-z0-9-]+$ ]] && [ -x "$HOST_ADAPTER" ] \
    || { peacock_refuse "$file names provider $HOST_PROVIDER, which has no scripts/env/host/$HOST_PROVIDER.sh"; return; }
  [[ "$HOST_USER" =~ ^[a-z_][a-z0-9_-]*$ ]] || { peacock_refuse "$file: not a user name: $HOST_USER"; return; }
}

# host_ssh_options — sets HOST_SSH to ssh with the person's key, for the host host_read read. The
# host key is kept under the host's name, so an address that moves on every start still finds it.
host_ssh_options() {
  local key=${GPU_SSH_KEY_FILE:-}
  [ -n "$key" ] || { peacock_refuse "GPU_SSH_KEY_FILE is not set: it names the key to GPU hosts, as peacock.env does on a VM"; return; }
  [[ "$key" =~ ^/[^[:space:]]+$ ]] || { peacock_refuse "GPU_SSH_KEY_FILE must be an absolute path without spaces: $key"; return; }
  [ -f "$key" ] || { peacock_refuse "no GPU_SSH_KEY_FILE $key"; return; }
  HOST_SSH=(ssh -i "$key" -o IdentitiesOnly=yes -o BatchMode=yes -o StrictHostKeyChecking=accept-new
            -o "HostKeyAlias=peacock-$HOST_NAME" -o ConnectTimeout=20 -o ServerAliveInterval=30
            -o ServerAliveCountMax=4)
}

# host_reachable — whether the key logs in to the host now. Needs host_ssh_options first.
host_reachable() {
  local ip
  ip=$(host_ip 2> /dev/null) || return 1
  "${HOST_SSH[@]}" -- "$HOST_USER@$ip" true < /dev/null > /dev/null 2>&1
}

# host_main <adapter> <arg>... — an adapter's command line, `<name> start|stop|status|ip|powers|ssh
# <cmd>`; --needs, the keys beyond PROVIDER, HOST and USER its host files hold; --config-name, the
# file in hosts/<name>/ its CLI is pointed at, none for a provider without one. The adapter defines
# HOST_NEEDS, HOST_CONFIG_NAME and host_start, host_stop, host_status, host_ip and host_powers; powers
# exits 0 when the adapter can start and stop the host, else 1 saying why.
host_main() {
  local script=$1 provider name verb key ip
  shift
  provider=${script##*/}
  provider=${provider%.sh}
  case "${1:-}" in
    -h|--help) peacock_usage "$script" ;;
    --needs) if [ ${#HOST_NEEDS[@]} -gt 0 ]; then printf '%s\n' "${HOST_NEEDS[@]}"; fi; return ;;
    --config-name) if [ -n "$HOST_CONFIG_NAME" ]; then echo "$HOST_CONFIG_NAME"; fi; return ;;
  esac
  [ $# -ge 2 ] || { peacock_refuse "name the host and what to do: $provider.sh <name> start|stop|status|ip|powers|ssh <cmd>"; return; }
  name=$1 verb=$2
  shift 2
  host_name_is_valid "$name" || { peacock_refuse "not a host name: $name"; return; }
  case "$verb" in
    start|stop|status|ip|powers) [ $# = 0 ] || { peacock_refuse "$verb takes no argument: $*"; return; } ;;
    ssh) [ $# -gt 0 ] || { peacock_refuse "ssh needs a command to run on $name"; return; } ;;
    *) peacock_refuse "unknown command $verb: start, stop, status, ip, powers or ssh"; return ;;
  esac
  host_read "$name"
  [ "$HOST_PROVIDER" = "$provider" ] \
    || { peacock_refuse "$name is a $HOST_PROVIDER host: scripts/env/host/$HOST_PROVIDER.sh $name $verb"; return; }
  for key in "${HOST_NEEDS[@]}"; do
    [ -n "${HOST_FILE[$key]:-}" ] || { peacock_refuse "$PEACOCK_HOSTS/$name.env has no $key, which a $provider host needs"; return; }
  done
  HOST_CONFIG=$PEACOCK_HOSTS/$name/$HOST_CONFIG_NAME
  case "$verb" in
    start) host_start ;;
    stop) host_stop ;;
    status) host_status ;;
    ip) host_ip ;;
    powers) host_powers ;;
    ssh)
      host_ssh_options
      ip=$(host_ip)
      exec "${HOST_SSH[@]}" -- "$HOST_USER@$ip" "$@" ;;
  esac
}
