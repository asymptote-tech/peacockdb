#!/usr/bin/env bash
# Ready a VM, or record a GPU host, for one developer, from their own machine (llm-wiki/build-test.md).
#
#   provision-vm.sh <login>@<address> [--env <file>] [--user <name>] [--name <vm>] [--ref <branch>] [--new-pat]
#   provision-vm.sh --new-pat
#   provision-vm.sh --gpu-host <name> <user>@<address> [--provider <p>] [--instance-id <id>] [--provider-config <file>]
#
# A VM: <login> has sudo; bootstrap-vm.sh runs as --user, this machine's, at --ref; --env is needed until
# ~/.config/peacock holds the storage keys; the PAT and GPU key go on ssh's stdin; --name, the address by
# default, is up.sh's default VM. --new-pat replaces the PAT on every recorded VM. A GPU host: hosts/<name>.env
# and the provider CLI's config written, the GPU key put there once (its password typed), hosts/ copied to every VM.
[ "${BASH_VERSINFO[0]}" -gt 4 ] || { [ "${BASH_VERSINFO[0]}" -eq 4 ] && [ "${BASH_VERSINFO[1]}" -ge 4 ]; } || {
  echo "${0##*/}: this machine's bash is $BASH_VERSION, older than 4.4: install a current one (on macOS, brew install bash)" >&2; exit 2; }
# Above, in bash 3.2's syntax and before any source: macOS ships 3.2, and what follows needs 4.4.
set -euo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/../lib/peacock-config.sh"
. "$here/../lib/host.sh"
shopt -s inherit_errexit

die() { echo "provision-vm.sh: $*" >&2; exit 2; }

target="" env_in="" user="" name="" ref="" new_pat="" gpu_host="" provider="" instance_id="" provider_config=""
take() { [ -z "${!1}" ] || die "one $2 per call"; printf -v "$1" %s "$3"; }
while [ $# -gt 0 ]; do
  case "$1" in
    -h|--help) peacock_usage "${BASH_SOURCE[0]}" ;;
    --env|--user|--name|--ref|--gpu-host|--provider|--instance-id|--provider-config)
      [ -n "${2:-}" ] || die "$1 needs a value"
      case "$1" in
        --env) var=env_in ;; --user) var=user ;; --name) var=name ;; --ref) var=ref ;;
        --gpu-host) var=gpu_host ;; --provider) var=provider ;; --instance-id) var=instance_id ;;
        --provider-config) var=provider_config ;;
      esac
      take "$var" "$1" "$2"
      shift 2 ;;
    --new-pat) take new_pat "$1" 1; shift ;;
    -*) die "unknown flag $1" ;;
    *) take target "<login>@<address>" "$1"; shift ;;
  esac
done
if [ -n "$gpu_host" ]; then
  [ -n "$target" ] || die "name the GPU host's login: provision-vm.sh --gpu-host <name> <user>@<address>"
  [ -z "$env_in$user$name$ref$new_pat" ] || die "--env, --user, --name, --ref and --new-pat ready a VM, not a GPU host"
  host_name_is_valid "$gpu_host" || die "a GPU host's name is letters, digits, ., _ and -: $gpu_host"
  [[ "$target" =~ ^([a-z_][a-z0-9_-]*)@([^@[:space:]]+)$ ]] || die "not <user>@<address>: $target"
  gpu_target=$target target=""
  provider=${provider:-ssh}
  [[ "$provider" =~ ^[a-z0-9-]+$ ]] && [ -x "$HOST_ADAPTERS/$provider.sh" ] \
    || die "no provider $provider: scripts/env/host has no $provider.sh"
  needs=$("$HOST_ADAPTERS/$provider.sh" --needs)
  for key in $needs; do
    [ "$key" = INSTANCE_ID ] || die "a $provider host needs $key, which provision-vm.sh does not take"
  done
  if [[ " $needs " == *" INSTANCE_ID "* ]]; then
    [[ "$instance_id" =~ ^[A-Za-z0-9][A-Za-z0-9._-]*$ ]] || die "a $provider host needs --instance-id <its id>"
  else
    [ -z "$instance_id" ] || die "--instance-id is for a provider whose hosts need INSTANCE_ID; $provider's do not"
  fi
  config_name=$("$HOST_ADAPTERS/$provider.sh" --config-name)
  if [ -n "$provider_config" ]; then
    [ -n "$config_name" ] || die "--provider-config is for a provider whose CLI reads a config; $provider's does not"
    [ -f "$provider_config" ] || die "no --provider-config $provider_config"
  fi
  command -v ssh-keygen > /dev/null || [ -f "$PEACOCK_GPU_KEY" ] || die "no ssh-keygen to make the GPU key"
elif [ -n "$provider$instance_id$provider_config" ]; then
  die "--provider, --instance-id and --provider-config record a GPU host: provision-vm.sh --gpu-host <name> <user>@<address>"
elif [ -z "$target" ]; then
  [ -n "$new_pat" ] || die "name the VM: provision-vm.sh <login>@<address>, see --help"
  [ -z "$env_in$user$name$ref" ] || die "--env, --user, --name and --ref ready a VM: name it, <login>@<address>"
else
  [[ "$target" =~ ^([a-z_][a-z0-9_-]*)@([^@[:space:]]+)$ ]] || die "not <login>@<address>: $target"
  login=${BASH_REMATCH[1]} address=${BASH_REMATCH[2]}
  user=${user:-$(id -un)}
  [[ "$user" =~ ^[a-z_][a-z0-9_-]{0,31}$ ]] || die "not a Linux user name: $user"
  name=${name:-$address}
  [[ "$name" =~ ^[A-Za-z0-9][A-Za-z0-9._-]*$ ]] || die "a VM's name is letters, digits, ., _ and -: $name"
  [ -z "$ref" ] || [[ "$ref" =~ ^[A-Za-z0-9][A-Za-z0-9._/-]*$ ]] || die "not a branch name: $ref"
  dev=$user@$address
fi

# What this machine must hold, checked before anything is written here or there.
if [ -n "$target" ]; then
  for key in user.name user.email; do
    [ -n "$(git -C "$here" config --get "$key" || true)" ] \
      || die "this machine's git has no $key, which the VM's commits carry: git config --global $key <…>"
  done
  git_name=$(git -C "$here" config --get user.name)
  git_email=$(git -C "$here" config --get user.email)
  if [ -z "$env_in" ] && [ ! -f "$PEACOCK_ENV" ]; then
    die "no $PEACOCK_ENV yet: --env <file> names the storage keys you received"
  fi
  env_src=${env_in:-$PEACOCK_ENV}
  [ -f "$env_src" ] || die "no env file $env_src"
  peacock_env_is_storage_only "$env_src"
  peacock_env_has_storage "$env_src"
  if [ ! -f "$PEACOCK_GPU_KEY" ]; then command -v ssh-keygen > /dev/null || die "no ssh-keygen to make the GPU key"; fi
fi

# The token is read and checked before anything is kept, so a wrong one leaves nothing behind.
if [ -z "$gpu_host" ] && { [ -n "$new_pat" ] || [ ! -s "$PEACOCK_PAT" ]; }; then
  command -v gh > /dev/null || die "no gh on this machine to check the token with"
  owner=${PEACOCK_REPO%/*} repo=${PEACOCK_REPO#*/}
  cat << EOF
A GitHub token for the VMs, made once and kept in $PEACOCK_PAT:
  https://github.com/settings/personal-access-tokens/new
  Resource owner: $owner. Repository access: only select repositories, $repo. An expiration.
  Repository permissions: Contents read and write, Pull requests read and write, Workflows read
  and write, Actions read and write, Commit statuses read-only, Metadata read-only.
Paste the token and press Enter; it is not shown:
EOF
  token=""
  IFS= read -rs token || true
  echo
  [ -n "$token" ] || die "no token given"
  [[ "$token" == github_pat_* ]] || die "not a fine-grained token (github_pat_…): make one as above"
  GH_TOKEN=$token gh api "repos/$owner/$repo" --silent > /dev/null 2>&1 \
    || die "the token cannot read $owner/$repo: check its resource owner, repository and Metadata permission"
fi

umask 077
install -d -m 700 "$PEACOCK_CONFIG"
chmod 700 "$PEACOCK_CONFIG"
if [ -n "${token:-}" ]; then
  printf '%s\n' "$token" > "$PEACOCK_PAT.new"
  mv "$PEACOCK_PAT.new" "$PEACOCK_PAT"
  unset token
fi

ssh_opts=(-o StrictHostKeyChecking=accept-new -o ConnectTimeout=20)
on() { local who=$1; shift; ssh "${ssh_opts[@]}" -- "$who" "$@"; }
# remote_test <command> — <command>'s status as the developer on the VM; an ssh failure stops the run.
remote_test() {
  local rc=0
  on "$dev" "$1" < /dev/null || rc=$?
  [ "$rc" != 255 ] || die "cannot reach $dev over ssh"
  return "$rc"
}

# copy_hosts <user>@<address> — this machine's hosts/ into the VM's ~/.config/peacock, where an
# instance mounts it; a VM that cannot be reached keeps its own and is named.
copy_hosts() {
  local rc=0
  tar -C "$PEACOCK_CONFIG" -cf - hosts \
    | on "$1" 'umask 077 && mkdir -p .config/peacock && tar -C .config/peacock -xf -' || rc=$?
  case "$rc" in
    0) echo "hosts/ is on $1" ;;
    255) echo "cannot reach $1: it keeps its hosts/ until provision-vm.sh --gpu-host reaches it" >&2 ;;
    *) echo "provision-vm.sh: copying hosts/ to $1 failed" >&2; failed=1 ;;
  esac
}

# The GPU key on the host, then the host file, then the file on every VM. ssh asks for the password
# itself, on the terminal; the key goes on its stdin.
if [ -n "$gpu_host" ]; then
  if [ ! -f "$PEACOCK_GPU_KEY" ]; then
    ssh-keygen -q -t ed25519 -N '' -C "peacock-gpu $(id -un)" -f "$PEACOCK_GPU_KEY"
  fi
  chmod 600 "$PEACOCK_GPU_KEY" "$PEACOCK_GPU_KEY.pub"
  key_login() {
    ssh "${ssh_opts[@]}" -i "$PEACOCK_GPU_KEY" -o IdentitiesOnly=yes -o BatchMode=yes -- "$gpu_target" true \
      < /dev/null > /dev/null 2>&1
  }
  if key_login; then
    echo "$gpu_target takes the GPU key already"
  else
    echo "$gpu_target does not take the GPU key yet: type ${gpu_target%@*}'s password there once, and its .pub goes into authorized_keys"
    on "$gpu_target" 'umask 077 && mkdir -p .ssh && touch .ssh/authorized_keys && IFS= read -r key &&
                      { grep -qxF "$key" .ssh/authorized_keys || printf "%s\n" "$key" >> .ssh/authorized_keys; }' \
      < "$PEACOCK_GPU_KEY.pub" || die "could not put the GPU key on $gpu_target"
    key_login || die "the GPU key does not log in to $gpu_target even now: does its sshd take public keys?"
  fi
  install -d -m 700 "$PEACOCK_HOSTS"
  chmod 700 "$PEACOCK_HOSTS"
  hostfile=$(mktemp "$PEACOCK_HOSTS/.$gpu_host.XXXXXX")
  printf 'PROVIDER=%s\nHOST=%s\nUSER=%s\n' "$provider" "${gpu_target#*@}" "${gpu_target%@*}" > "$hostfile"
  if [ -n "$instance_id" ]; then echo "INSTANCE_ID=$instance_id" >> "$hostfile"; fi
  mv "$hostfile" "$PEACOCK_HOSTS/$gpu_host.env"
  # Copied as bytes, never printed: the profile holds a service account's private key.
  if [ -n "$provider_config" ]; then
    install -d -m 700 "$PEACOCK_HOSTS/$gpu_host"
    install -m 600 "$provider_config" "$PEACOCK_HOSTS/$gpu_host/$config_name"
    echo "$PEACOCK_HOSTS/$gpu_host/$config_name: the $provider CLI's config, which starts and stops $gpu_host"
  fi
  echo "$PEACOCK_HOSTS/$gpu_host.env names $gpu_target: gpu.sh $gpu_host prepare readies it"
  failed=""
  if [ -s "$PEACOCK_VMS" ]; then
    while read -r -u 3 _ vm_target; do copy_hosts "$vm_target"; done 3< "$PEACOCK_VMS"
  fi
  [ -z "$failed" ] || exit 1
  exit 0
fi

if [ -n "$target" ]; then
  if [ -n "$env_in" ]; then peacock_env_normalised "$env_in" "$PEACOCK_ENV"; fi
  if [ ! -f "$PEACOCK_GPU_KEY" ]; then
    ssh-keygen -q -t ed25519 -N '' -C "peacock-gpu $(id -un)" -f "$PEACOCK_GPU_KEY"
  fi
  chmod 600 "$PEACOCK_GPU_KEY" "$PEACOCK_GPU_KEY.pub"
  echo "The key to GPU hosts, $PEACOCK_GPU_KEY.pub: put it on a GPU host when you create one."
  cat "$PEACOCK_GPU_KEY.pub"

  # The developer's own Linux user, reached with <login>'s keys. Its sudo widens nothing: the docker
  # group is root already. Joining the groups before its first login makes them its session's.
  on "$target" sudo bash -s -- "$user" "$login" << 'ROOT'
set -euo pipefail
user=$1 login=$2
getent group docker > /dev/null || groupadd --system docker
getent group peacock > /dev/null || groupadd peacock
getent passwd "$user" > /dev/null || useradd --create-home --shell /bin/bash "$user"
usermod -aG docker,peacock "$user"
if [ "$user" != "$login" ]; then
  home=$(getent passwd "$user" | cut -d: -f6)
  gid=$(getent passwd "$user" | cut -d: -f4)
  if [ ! -s "$home/.ssh/authorized_keys" ]; then
    install -d -m 700 -o "$user" -g "$gid" "$home/.ssh"
    install -m 600 -o "$user" -g "$gid" "$(getent passwd "$login" | cut -d: -f6)/.ssh/authorized_keys" \
      "$home/.ssh/authorized_keys"
  fi
  echo "$user ALL=(ALL) NOPASSWD:ALL" | install -m 440 /dev/stdin "/etc/sudoers.d/peacock-$user"
fi
ROOT

  if remote_test 'test -e .config/peacock/machine'; then
    if [ -n "$ref" ]; then
      on_branch=$(on "$dev" 'git -c core.hooksPath=/dev/null -c core.fsmonitor=false -C peacockdb branch --show-current' \
                    < /dev/null) \
        || die "cannot read the branch of ~/peacockdb on $dev"
      [ "$on_branch" = "$ref" ] \
        || die "~/peacockdb on $dev is on ${on_branch:-a detached HEAD}, not $ref: --ref names the branch it is on"
    fi
    echo "$dev is bootstrapped already"
  else
    run='f=$(mktemp) && cat > "$f" && bash "$f"'
    if [ -n "$ref" ]; then run+=" --ref $(printf %q "$ref")"; fi
    on "$dev" "$run"'; s=$?; rm -f "$f"; exit $s' < "$here/bootstrap-vm.sh"
  fi

  # Secrets on ssh's stdin only. The VM's peacock.env names its own copy of the GPU key, on a line
  # of its own whether or not the file ended in a newline.
  copied=(peacock.env gpu_ed25519 gpu_ed25519.pub)
  if [ -d "$PEACOCK_HOSTS" ]; then copied+=(hosts); fi
  tar -C "$PEACOCK_CONFIG" -cf - "${copied[@]}" \
    | on "$dev" 'umask 077 && mkdir -p .config/peacock && chmod 700 .config/peacock &&
                 tar -C .config/peacock -xf - && cd .config/peacock &&
                 { [ -z "$(tail -c 1 peacock.env)" ] || echo; echo "GPU_SSH_KEY_FILE=$HOME/.config/peacock/gpu_ed25519"; } >> peacock.env'
  if [ -n "$new_pat" ] || ! remote_test 'env -u GH_TOKEN -u GITHUB_TOKEN gh auth status > /dev/null 2>&1'; then
    on "$dev" 'gh auth login --with-token --insecure-storage' < "$PEACOCK_PAT"
  fi
  on "$dev" 'gh auth setup-git' < /dev/null
  printf '%s\n%s\n' "$git_name" "$git_email" \
    | on "$dev" 'IFS= read -r n && IFS= read -r e && git config --global user.name "$n" && git config --global user.email "$e"'

  vms=$(mktemp "$PEACOCK_VMS.XXXXXX")
  if [ -f "$PEACOCK_VMS" ]; then awk -v n="$name" '$1 != n' "$PEACOCK_VMS" > "$vms"; fi
  echo "$name $dev" >> "$vms"
  mv "$vms" "$PEACOCK_VMS"
  echo "$name ($dev) is the default VM for up.sh and down.sh"
fi

# Every other recorded VM: one that is down keeps the old token and is named, and the rest go on.
failed=""
if [ -n "$new_pat" ] && [ -s "$PEACOCK_VMS" ]; then
  while read -r -u 3 vm_name vm_target; do
    [ "$vm_name" != "$name" ] || continue
    rc=0
    on "$vm_target" 'gh auth login --with-token --insecure-storage' < "$PEACOCK_PAT" || rc=$?
    case "$rc" in
      0) echo "the new PAT is on $vm_name ($vm_target)" ;;
      255) echo "cannot reach $vm_name ($vm_target): it keeps the old PAT until provision-vm.sh --new-pat reaches it" >&2 ;;
      *) echo "provision-vm.sh: gh on $vm_name ($vm_target) refused the new PAT" >&2; failed=1 ;;
    esac
  done 3< "$PEACOCK_VMS"
fi

if [ -n "$target" ]; then
  if remote_test 'test -s "${CLAUDE_CONFIG_DIR:-$HOME/.claude}/.credentials.json"'; then
    echo "Claude Code is logged in on $dev"
  else
    ssh "${ssh_opts[@]}" -t -- "$dev" 'CLAUDE_CONFIG_DIR=$HOME/.claude DISABLE_AUTOUPDATER=1 ~/.local/bin/claude auth login'
  fi
fi
[ -z "$failed" ] || exit 1
