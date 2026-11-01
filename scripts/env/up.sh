#!/usr/bin/env bash
# Start an ensemble instance in the build image (llm-wiki/build-test.md, "Containers").
#
#   up.sh <instance> --env <file> [--chain <c>] [--cudf <v> | --image cudf-<v>-<commit>]
#
# The instance is the container peacock-<instance> over its own worktree of this clone, kept by
# down.sh. --chain starts scripts/ensemble-watchdog.sh <c> in it. --cudf is 25.02 by default.
# --env is a docker env file: AWS_ACCESS_KEY_ID, AWS_SECRET_ACCESS_KEY, AWS_ENDPOINT_URL and
# AWS_REGION, and optionally GPU_SSH_KEY_FILE and PROVIDER_CONFIG_DIR, absolute paths mounted
# read-only. Refused before anything starts without a login or a key; after the pull, for an image
# of other inputs than the worktree's. The trust model: llm-wiki/build-test.md, "Containers".
set -euo pipefail
shopt -s inherit_errexit

usage() { sed -n '2,${/^#/!q;p;}' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'; exit 0; }
die() { echo "up.sh: $*" >&2; exit 2; }

here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/../lib/image-inputs.sh"
. "$here/../lib/data-inputs.sh"
. "$here/../lib/machine.sh"
clone=$(dirname "$(machine_git -C "$here" rev-parse --path-format=absolute --git-common-dir)")

instance="" chain="" cudf="" image_tag="" env_file=""
take() { [ -z "${!1}" ] || die "one $2 per call"; printf -v "$1" %s "$3"; }
while [ $# -gt 0 ]; do
  case "$1" in
    -h|--help) usage ;;
    --chain|--cudf|--image|--env)
      [ -n "${2:-}" ] || die "$1 needs a value"
      case "$1" in --chain) var=chain ;; --cudf) var=cudf ;; --image) var=image_tag ;; --env) var=env_file ;; esac
      take "$var" "$1" "$2"
      shift 2 ;;
    -*) die "unknown flag $1" ;;
    *) take instance "instance" "$1"; shift ;;
  esac
done
[ -n "$instance" ] || die "name the instance: up.sh <instance>, see --help"
[[ "$instance" =~ ^[a-z0-9][a-z0-9-]*$ ]] || die "an instance is named in lower-case letters, digits and -: $instance"
[ -z "$chain" ] || [[ "$chain" =~ ^[A-Za-z0-9_-]+$ ]] || die "not a chain name: $chain"
if [ -n "$image_tag" ]; then
  [ -z "$cudf" ] || die "--image names its cuDF version: drop --cudf"
  [[ "$image_tag" =~ ^cudf-([0-9]+\.[0-9]+)-[0-9a-f]{12}$ ]] || die "--image takes a tag cudf-<v>-<commit>: $image_tag"
  cudf=${BASH_REMATCH[1]}
fi
cudf=${cudf:-25.02}
[ -f "$clone/docker/conda/cudf-$cudf.spec" ] || die "no cuDF $cudf: docker/conda has no cudf-$cudf.spec"
[ -n "$env_file" ] || die "--env <file> is required: the keys file, see --help"
[ -f "$env_file" ] || die "no env file $env_file"
env_file=$(realpath "$env_file")
[ "$(id -u)" != 0 ] || die "run as the machine user, not root: claude refuses root with --dangerously-skip-permissions"

# What the container has of the logins is what is on disk: a token in GH_TOKEN or in a desktop
# keyring stays on the machine, so gh must hold its token in hosts.yml, a fine-grained PAT.
claude_dir=${CLAUDE_CONFIG_DIR:-$HOME/.claude}
[ -e "$HOME/.local/bin/claude" ] && [ -d "$HOME/.local/share/claude" ] \
  || die "no Claude Code in ~/.local: run scripts/env/bootstrap-vm.sh"
[ -s "$claude_dir/.credentials.json" ] \
  || die "Claude Code is not logged in (no $claude_dir/.credentials.json): run claude, then /login"
hosts=$HOME/.config/gh/hosts.yml
[ -f "$hosts" ] \
  || die "gh has no login on disk (no ~/.config/gh/hosts.yml): gh auth login --with-token --insecure-storage"
tokens=$(grep -c 'oauth_token:' "$hosts" || true)
[ "$tokens" -gt 0 ] \
  || die "~/.config/gh/hosts.yml holds no oauth_token, as a keyring login leaves it," \
         "and the container has no keyring: gh auth login --with-token --insecure-storage"
[ "$(grep -c 'oauth_token: github_pat_' "$hosts" || true)" = "$tokens" ] \
  || die "gh's token is not a fine-grained PAT (github_pat_…): see llm-wiki/build-test.md, \"A fresh VM\""
env -u GH_TOKEN -u GITHUB_TOKEN gh auth status > /dev/null 2>&1 \
  || die "gh is not logged in: run gh auth login, then gh auth setup-git"
for key in user.name user.email; do
  [ -n "$(machine_git config --file "$HOME/.gitconfig" --get "$key" || true)" ] \
    || die "~/.gitconfig holds no git identity: set user.name and user.email with git config --global"
done
helpers=$(machine_git config --file "$HOME/.gitconfig" --get-all credential.https://github.com.helper || true)
[[ "$helpers" == *"/usr/local/bin/gh auth git-credential"* ]] \
  || die "~/.gitconfig does not ask /usr/local/bin/gh for GitHub's credentials: run gh auth setup-git"

# env_path <name> — the path the env file gives <name>, empty when it names none. A key's value is
# never read: only that it is there, once, and not empty.
env_path() {
  local lines
  lines=$(grep "^$1=" "$env_file" || true)
  [ "$(grep -c . <<< "$lines" || true)" -le 1 ] || die "$env_file names $1 more than once"
  printf '%s' "${lines#*=}"
}
for key in AWS_ACCESS_KEY_ID AWS_SECRET_ACCESS_KEY AWS_ENDPOINT_URL AWS_REGION; do
  case "$(grep -c "^$key=" "$env_file" || true)" in
    0) die "$env_file has no $key" ;;
    1) grep -q "^$key=." "$env_file" || die "$key is empty in $env_file" ;;
    *) die "$env_file names $key more than once" ;;
  esac
done
ssh_key=$(env_path GPU_SSH_KEY_FILE)
provider=$(env_path PROVIDER_CONFIG_DIR)
[ -z "$ssh_key" ] || [[ "$ssh_key" == /* ]] || die "GPU_SSH_KEY_FILE must be an absolute path: $ssh_key"
[ -z "$ssh_key" ] || [ -f "$ssh_key" ] || die "no GPU_SSH_KEY_FILE $ssh_key"
[ -z "$provider" ] || [[ "$provider" == /* ]] || die "PROVIDER_CONFIG_DIR must be an absolute path: $provider"
[ -z "$provider" ] || [ -d "$provider" ] || die "no PROVIDER_CONFIG_DIR $provider"

# An instance writes the clone's .git. What would run there on the machine's next git call is
# refused; machine_git runs none of it whatever the config says.
hooks=$(find "$clone/.git/hooks" -mindepth 1 ! -name '*.sample' 2> /dev/null | sort | paste -sd' ' || true)
[ -z "$hooks" ] || die "the clone has git hooks, which run as the machine user: remove $hooks"
for key in core.hooksPath core.fsmonitor; do
  [ -z "$(machine_git config --file "$clone/.git/config" --get "$key" || true)" ] \
    || die "the clone's .git/config sets $key, which runs as the machine user: git config --unset $key"
done

container=peacock-$instance
if state=$(docker container inspect --format '{{.State.Status}}' "$container" 2> /dev/null); then
  die "instance $instance is there already ($container, $state): scripts/env/down.sh $instance first"
fi

mkdir -p "$MACHINE_INSTANCES"
exec {lock}> "$MACHINE_INSTANCES/.$instance.lock"
flock -n "$lock" || die "another up.sh is starting instance $instance"
worktree=$MACHINE_INSTANCES/$instance
home=$MACHINE_INSTANCES/.home/$instance
if [ -e "$worktree" ]; then
  [ "$(machine_git -C "$worktree" rev-parse --path-format=absolute --git-common-dir)" = "$clone/.git" ] \
    || die "$worktree is not a worktree of $clone"
else
  machine_git -C "$clone" worktree add --quiet --detach "$worktree"
fi
machine_cudf_source "$worktree"

ref=$MACHINE_IMAGE:${image_tag:-cudf-$cudf}
machine_image_checked "$ref" "$worktree" "$cudf"
image_path=$(docker image inspect --format '{{range .Config.Env}}{{println .}}{{end}}' "$ref" | sed -n 's/^PATH=//p')
[[ "$image_path" =~ ^[^[:space:]]+$ ]] || die "$ref sets no single PATH"
machine_data_set "$ref" "$worktree"
machine_cargo_fetch "$ref" "$worktree"

# The machine user in the container's passwd and group, home and all: ssh and anything else that
# looks the uid up finds it. Outside the instance's home, which the instance can write.
etc=$MACHINE_INSTANCES/.etc/$instance
mkdir -p "$etc"
uid=$(id -u) gid=$(id -g) user=$(id -un) group=$(id -gn)
docker run --rm --user "$MACHINE_USER" --network none "$ref" cat /etc/passwd \
  | awk -F: -v id="$uid" -v name="$user" '$3 != id && $1 != name' > "$etc/passwd"
echo "$user:x:$uid:$gid::$HOME:/bin/bash" >> "$etc/passwd"
docker run --rm --user "$MACHINE_USER" --network none "$ref" cat /etc/group \
  | awk -F: -v id="$gid" -v name="$group" '$3 != id && $1 != name' > "$etc/group"
echo "$group:x:$gid:" >> "$etc/group"

srcs=() dsts=() modes=()
bind() { srcs+=("$1") dsts+=("$2") modes+=("${3:-rw}"); }
bind "$home" "$HOME"
bind "$worktree" "$worktree"
bind "$clone/.git" "$clone/.git"
bind "$CUDF_SOURCE" "$worktree/third_party/cudf" ro
bind "$etc/passwd" /etc/passwd ro
bind "$etc/group" /etc/group ro
bind "$claude_dir" "$claude_dir"
bind "$HOME/.local/share/claude" "$HOME/.local/share/claude" ro
bind "$HOME/.local/bin/claude" "$HOME/.local/bin/claude" ro
bind "$HOME/.config/gh" "$HOME/.config/gh"
bind "$HOME/.gitconfig" "$HOME/.gitconfig" ro
bind "$MACHINE_CCACHE" "$MACHINE_CCACHE"
bind "$MACHINE_REGISTRY" "$IMAGE_REGISTRY" ro
for entry in "${DATA_SETS[@]}"; do
  bind "$DATA_SET/${entry%% *}" "$worktree/testdata/${entry%% *}" ro
done
if [ -d "$HOME/.config/peacock/hosts" ]; then bind "$HOME/.config/peacock/hosts" "$HOME/.config/peacock/hosts" ro; fi
if [ -n "$ssh_key" ]; then bind "$ssh_key" "$ssh_key" ro; fi
if [ -n "$provider" ]; then bind "$provider" "$provider" ro; fi

# Docker makes a missing mount point as root, inside the mount that holds it: in the instance's
# home or worktree on the machine, where it would leave a directory its uid cannot write beside.
mkdir -p "$home" "$MACHINE_CCACHE"
for i in "${!dsts[@]}"; do
  outer=""
  for j in "${!dsts[@]}"; do
    if [[ "${dsts[$i]}" == "${dsts[$j]}"/* ]] && { [ -z "$outer" ] || [ ${#dsts[$j]} -gt ${#dsts[$outer]} ]; }; then
      outer=$j
    fi
  done
  [ -n "$outer" ] || continue
  point=${srcs[$outer]}${dsts[$i]#"${dsts[$outer]}"}
  if [ -d "${srcs[$i]}" ]; then
    mkdir -p "$point"
  else
    mkdir -p "$(dirname "$point")"
    touch "$point"
  fi
done

run=(docker run -d --init --name "$container" --user "$MACHINE_USER" -e "HOME=$HOME"
     -e "PATH=$HOME/.local/bin:$image_path" -e DISABLE_AUTOUPDATER=1 -e "CLAUDE_CONFIG_DIR=$claude_dir"
     -e "CCACHE_DIR=$MACHINE_CCACHE")
run+=(--env-file "$env_file")
for i in "${!dsts[@]}"; do
  spec="type=bind,src=${srcs[$i]},dst=${dsts[$i]}"
  if [ "${modes[$i]}" = ro ]; then spec+=,readonly; fi
  run+=(--mount "$spec")
done
cmd=(sleep infinity)
if [ -n "$chain" ]; then cmd=(scripts/ensemble-watchdog.sh "$chain"); fi
"${run[@]}" -w "$worktree" "$ref" "${cmd[@]}" > /dev/null
echo "instance $instance is up: docker exec -it $container bash; scripts/env/down.sh $instance stops it"
