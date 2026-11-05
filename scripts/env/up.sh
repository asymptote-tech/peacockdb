#!/usr/bin/env bash
# Start chain <chain>'s ensemble instance in the build image (llm-wiki/build-test.md, "Containers").
#
#   up.sh <chain> [--branch <b> | --board <ref>] [--cudf <v> | --image cudf-<v>-<commit>] [--env <file>] [--gpu-host <name>] [--vm <name>]
#
# The container peacock-<user>-<chain> runs scripts/ensemble-watchdog.sh <chain> in its worktree,
# kept by down.sh. A new one is on --branch, a chain that ran before, or else a new chain's first
# branch, from its section on --board (origin/master). A chain runs once a machine. --cudf is 25.02
# by default. --env, ~/.config/peacock/peacock.env by default, holds the four AWS_* keys and maybe
# GPU_SSH_KEY_FILE. --gpu-host, a host in hosts/, is the chain's, as PEACOCK_GPU_HOST. With no
# instances here, the arguments go to the VM provision-vm.sh recorded last, or --vm's. Refused, exit 2.
[ "${BASH_VERSINFO[0]}" -gt 4 ] || { [ "${BASH_VERSINFO[0]}" -eq 4 ] && [ "${BASH_VERSINFO[1]}" -ge 4 ]; } || {
  echo "${0##*/}: this machine's bash is $BASH_VERSION, older than 4.4: install a current one (on macOS, brew install bash)" >&2; exit 2; }
# Above, in bash 3.2's syntax and before any source: macOS ships 3.2, and what follows needs 4.4.
set -euo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/../lib/peacock-config.sh"
shopt -s inherit_errexit

die() { echo "up.sh: $*" >&2; exit 2; }
. "$here/../lib/image-inputs.sh"
. "$here/../lib/data-inputs.sh"
. "$here/../lib/machine.sh"
. "$here/../lib/board.sh"
. "$here/../lib/host.sh"
clone=$(dirname "$(machine_git -C "$here" rev-parse --path-format=absolute --git-common-dir)")

chain="" cudf="" image_tag="" env_file="" vm="" branch="" board_ref="" gpu_host="" forwarded=()
take() { [ -z "${!1}" ] || die "one $2 per call"; printf -v "$1" %s "$3"; }
while [ $# -gt 0 ]; do
  case "$1" in
    -h|--help) peacock_usage "${BASH_SOURCE[0]}" ;;
    --cudf|--image|--env|--vm|--branch|--board|--gpu-host)
      [ -n "${2:-}" ] || die "$1 needs a value"
      case "$1" in
        --cudf) var=cudf ;; --image) var=image_tag ;; --env) var=env_file ;; --vm) var=vm ;;
        --branch) var=branch ;; --board) var=board_ref ;; --gpu-host) var=gpu_host ;;
      esac
      take "$var" "$1" "$2"
      if [ "$1" != --vm ]; then forwarded+=("$1" "$2"); fi
      shift 2 ;;
    -*) die "unknown flag $1" ;;
    *) take chain "chain" "$1"; forwarded+=("$1"); shift ;;
  esac
done
[ -n "$chain" ] || die "name the chain: up.sh <chain>, see --help"
[[ "$chain" =~ ^[A-Za-z0-9][A-Za-z0-9_-]*$ ]] || die "not a chain name: $chain"
[ -z "$branch" ] || [ -z "$board_ref" ] || die "--branch resumes a chain and --board reads a new one: drop one"
for name in "$branch" "$board_ref"; do
  [ -z "$name" ] || [[ "$name" =~ ^[A-Za-z0-9][A-Za-z0-9._/-]*$ ]] || die "not a branch or ref name: $name"
done
[ -z "$gpu_host" ] || host_name_is_valid "$gpu_host" || die "not a GPU host's name: $gpu_host"
if [ -n "$image_tag" ]; then
  [ -z "$cudf" ] || die "--image names its cuDF version: drop --cudf"
  [[ "$image_tag" =~ ^cudf-([0-9]+\.[0-9]+)-[0-9a-f]{12}$ ]] || die "--image takes a tag cudf-<v>-<commit>: $image_tag"
  cudf=${BASH_REMATCH[1]}
fi
peacock_forward up.sh "$vm" "${forwarded[@]}"

cudf=${cudf:-25.02}
[ -f "$clone/docker/conda/cudf-$cudf.spec" ] || die "no cuDF $cudf: docker/conda has no cudf-$cudf.spec"
env_file=${env_file:-$PEACOCK_ENV}
[ -f "$env_file" ] || die "no env file $env_file: provision-vm.sh puts the storage keys there, or --env names one"
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
peacock_env_has_storage "$env_file"
for key in GH_TOKEN GITHUB_TOKEN; do
  if grep -q "^$key=" "$env_file"; then die "$env_file holds $key: no token goes in the env file"; fi
done
ssh_key=$(env_path GPU_SSH_KEY_FILE)
[ -z "$ssh_key" ] || [[ "$ssh_key" == /* ]] || die "GPU_SSH_KEY_FILE must be an absolute path: $ssh_key"
[ -z "$ssh_key" ] || [ -f "$ssh_key" ] || die "no GPU_SSH_KEY_FILE $ssh_key"
[ -z "$gpu_host" ] || [ -f "$PEACOCK_HOSTS/$gpu_host.env" ] \
  || die "no GPU host $gpu_host in $PEACOCK_HOSTS: provision-vm.sh --gpu-host $gpu_host <user>@<address> records it"
# Without --gpu-host the coordinator takes the only host recorded, so several make it guess.
recorded=""
if [ -d "$PEACOCK_HOSTS" ]; then
  recorded=$(find "$PEACOCK_HOSTS" -maxdepth 1 -name '*.env' -printf '%f\n' | sed 's/\.env$//' | sort | paste -sd' ' -)
fi
[ -n "$gpu_host" ] || [ "$(wc -w <<< "$recorded")" -le 1 ] \
  || die "hosts/ records $recorded: name the chain's with --gpu-host <name>"
[ -d "$MACHINE_DATA" ] && [ -w "$MACHINE_DATA" ] \
  || die "the machine's data directory $MACHINE_DATA is missing or not writable for $(id -un):" \
         "bootstrap-vm.sh makes it, provision-vm.sh puts a developer in its group"

# An instance writes the clone's .git. What would run there on the machine's next git call is
# refused; machine_git runs none of it whatever the config says.
hooks=$(find "$clone/.git/hooks" -mindepth 1 ! -name '*.sample' 2> /dev/null | sort | paste -sd' ' || true)
[ -z "$hooks" ] || die "the clone has git hooks, which run as the machine user: remove $hooks"
for key in core.hooksPath core.fsmonitor; do
  [ -z "$(machine_git config --file "$clone/.git/config" --get "$key" || true)" ] \
    || die "the clone's .git/config sets $key, which runs as the machine user: git config --unset $key"
done

# The board is one per repository, so a chain runs once a machine, whichever user started it; the
# lock is machine-wide for the same reason, and held until the container is up.
user=$(id -un)
container=peacock-$user-$chain
mask=$(umask)
umask 002
exec {lock}> "$MACHINE_DATA/.chain-$chain.lock"
umask "$mask"
flock -n "$lock" || die "another up.sh is starting chain $chain"
present=$(docker ps -a --filter "label=peacockdb.chain=$chain" --format '{{.Label "peacockdb.user"}} {{.Names}} {{.State}}')
if [ -n "$present" ]; then
  die "chain $chain is on this machine already ($(awk '{ printf "%s%s of %s, %s", (NR > 1 ? "; " : ""), $2, $1, $3 }' \
    <<< "$present")): down.sh $chain as its user first"
fi

mkdir -p "$MACHINE_INSTANCES"
worktree=$MACHINE_INSTANCES/$chain
home=$MACHINE_INSTANCES/.home/$chain
has() { machine_git -C "$clone" rev-parse --verify --quiet "$1" > /dev/null; }

# worktree_resumed — the worktree on --branch, a chain that ran before: origin's, a local branch
# behind it brought up to it, one that diverged from it refused.
worktree_resumed() {
  local board mine theirs elsewhere
  has "refs/remotes/origin/$branch" || die "origin has no branch $branch"
  board=$(machine_git -C "$clone" show "origin/$branch:llm-wiki/tasks/tasks.md" 2> /dev/null) \
    || die "origin/$branch holds no llm-wiki/tasks/tasks.md"
  board_chain_section "$chain" "origin/$branch's board" <<< "$board"
  if ! has "refs/heads/$branch"; then
    machine_git -C "$clone" worktree add --quiet --track -b "$branch" "$worktree" "origin/$branch"
    return
  fi
  # Checked out elsewhere, the branch must not move under that worktree.
  elsewhere=$(machine_git -C "$clone" worktree list --porcelain \
                | awk -v b="branch refs/heads/$branch" '/^worktree / { w = substr($0, 10) } $0 == b { print w }')
  [ -z "$elsewhere" ] || die "$branch is checked out in $elsewhere already; a chain has a worktree of its own"
  mine=$(machine_git -C "$clone" rev-parse "refs/heads/$branch")
  theirs=$(machine_git -C "$clone" rev-parse "refs/remotes/origin/$branch")
  if machine_git -C "$clone" merge-base --is-ancestor "$mine" "$theirs"; then
    machine_git -C "$clone" update-ref "refs/heads/$branch" "$theirs" "$mine"
  elif ! machine_git -C "$clone" merge-base --is-ancestor "$theirs" "$mine"; then
    die "$branch ($mine) and origin/$branch ($theirs) have diverged: reconcile them, then up.sh again"
  fi
  machine_git -C "$clone" branch --quiet --set-upstream-to="origin/$branch" "$branch"
  machine_git -C "$clone" worktree add --quiet "$worktree" "$branch"
}

# worktree_new — the worktree on a new chain's first branch, ENS-<its first task>, at its base.
# Which branch a chain that ran before is on is never guessed: every origin/ENS* board where the
# chain's first task is past approved to build is named, and up.sh stops.
worktree_new() {
  local ref=${board_ref:-origin/master} board ran=() r first
  has "$ref^{commit}" || die "no $ref to read the board from"
  board=$(machine_git -C "$clone" show "$ref:llm-wiki/tasks/tasks.md" 2> /dev/null) \
    || die "$ref holds no llm-wiki/tasks/tasks.md"
  board_chain "$chain" "$ref" <<< "$board"
  while read -r r; do
    [[ "$r" == origin/ENS* ]] || continue
    board=$(machine_git -C "$clone" show "$r:llm-wiki/tasks/tasks.md" 2> /dev/null) || continue
    if board_ran "$chain" "$BOARD_TASK" <<< "$board"; then ran+=("${r#origin/}"); fi
  done < <(machine_git -C "$clone" for-each-ref --format='%(refname:short)' refs/remotes/origin/)
  [ ${#ran[@]} = 0 ] || die "chain $chain already ran on ${ran[*]}; resume with --branch <b>"
  first=ENS-$BOARD_TASK
  if has "refs/remotes/origin/$first"; then die "origin has $first already: resume with --branch $first"; fi
  if has "refs/heads/$first"; then
    die "the clone has a branch $first, which origin lacks: push it and resume with --branch $first, or delete it"
  fi
  has "refs/remotes/origin/$BOARD_BASE" || die "chain $chain's base $BOARD_BASE is not on origin"
  # The coordinator reads its own branch's board, which the new branch takes from the base.
  board=$(machine_git -C "$clone" show "origin/$BOARD_BASE:llm-wiki/tasks/tasks.md" 2> /dev/null) \
    || die "origin/$BOARD_BASE, chain $chain's base, holds no llm-wiki/tasks/tasks.md"
  board_chain_section "$chain" "origin/$BOARD_BASE's board, which the chain's first branch carries" \
    "give the chain a base whose board holds it" <<< "$board"
  machine_git -C "$clone" worktree add --quiet --no-track -b "$first" "$worktree" "origin/$BOARD_BASE"
}

if [ -e "$worktree" ]; then
  [ "$(machine_git -C "$worktree" rev-parse --path-format=absolute --git-common-dir)" = "$clone/.git" ] \
    || die "$worktree is not a worktree of $clone"
  [ -z "$board_ref" ] || die "chain $chain's worktree is there already, $worktree; --board reads a new chain"
  current=$(machine_git -C "$worktree" branch --show-current)
  [ -z "$branch" ] || [ "$branch" = "$current" ] \
    || die "chain $chain's worktree is on ${current:-a detached HEAD}, not $branch; a rerun keeps its branch"
else
  machine_git -C "$clone" fetch --quiet origin
  if [ -n "$branch" ]; then worktree_resumed; else worktree_new; fi
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
etc=$MACHINE_INSTANCES/.etc/$chain
mkdir -p "$etc"
uid=$(id -u) gid=$(id -g) group=$(id -gn)
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
# Made when missing, so a GPU host recorded later reaches an instance already running.
(umask 077 && mkdir -p "$PEACOCK_HOSTS")
bind "$PEACOCK_HOSTS" "$PEACOCK_HOSTS" ro
if [ -n "$ssh_key" ]; then bind "$ssh_key" "$ssh_key" ro; fi

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

# The cuDF checkout may be another developer's, and git refuses a repository another uid owns.
run=(docker run -d --init --name "$container" --label "peacockdb.chain=$chain" --label "peacockdb.user=$user"
     --user "$MACHINE_USER" -e "HOME=$HOME" -e "PATH=$HOME/.local/bin:$image_path" -e DISABLE_AUTOUPDATER=1
     -e "CLAUDE_CONFIG_DIR=$claude_dir" -e "CCACHE_DIR=$MACHINE_CCACHE"
     -e GIT_CONFIG_COUNT=1 -e GIT_CONFIG_KEY_0=safe.directory -e "GIT_CONFIG_VALUE_0=$worktree/third_party/cudf"
     -e "PEACOCK_INSTANCE=$user-$chain" -e "PEACOCK_IMAGE_TAG=${image_tag:-cudf-$cudf}")
if [ -n "$gpu_host" ]; then run+=(-e "PEACOCK_GPU_HOST=$gpu_host"); fi
run+=(--env-file "$env_file")
for i in "${!dsts[@]}"; do
  spec="type=bind,src=${srcs[$i]},dst=${dsts[$i]}"
  if [ "${modes[$i]}" = ro ]; then spec+=,readonly; fi
  run+=(--mount "$spec")
done
"${run[@]}" -w "$worktree" "$ref" scripts/ensemble-watchdog.sh "$chain" > /dev/null
echo "chain $chain is up: docker exec -it $container bash; scripts/env/down.sh $chain stops it"
if [ -z "$recorded" ]; then
  echo "up.sh: no GPU host is recorded, so chain $chain's GPU steps stall until provision-vm.sh --gpu-host records one" >&2
fi
