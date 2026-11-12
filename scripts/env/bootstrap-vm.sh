#!/usr/bin/env bash
# Prepare a CPU VM for a developer (llm-wiki/build-test.md, "CPU VMs").
#
#   bootstrap-vm.sh [--ref <branch>] [--cudf <v>]
#
# Run as the machine user, with sudo; provision-vm.sh runs it so. Installs what is missing: Docker,
# and gh, nebius and Claude Code with the superpowers plugin at docker/pins.env's versions; makes
# the data directory the machine's users share; clones the repository into ~/peacockdb at <ref>,
# master by default. In the build image of cuDF <v> (25.02 by default) and of the clone's inputs,
# by its inputs tag, it fills cargo's registry and generates sf1. Last, it checks out the clone's
# third_party/cudf, which the C++ configure reads. A step already done is skipped.
set -euo pipefail
shopt -s inherit_errexit

usage() { sed -n '2,${/^#/!q;p;}' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'; exit 0; }
die() { echo "bootstrap-vm.sh: $*" >&2; exit 2; }

REPO_URL=https://github.com/asymptote-tech/peacockdb.git
MARKETPLACE=anthropics/claude-plugins-official
clone=$HOME/peacockdb
ref="" cudf=""
while [ $# -gt 0 ]; do
  case "$1" in
    -h|--help) usage ;;
    --ref|--cudf)
      [ -n "${2:-}" ] || die "$1 needs a value"
      if [ "$1" = --ref ]; then ref=$2; else cudf=$2; fi
      shift 2 ;;
    -*) die "unknown flag $1" ;;
    *) die "no argument is positional: $1" ;;
  esac
done
[ -z "$cudf" ] || [[ "$cudf" =~ ^[0-9]+\.[0-9]+$ ]] || die "--cudf takes a version such as 25.02: $cudf"
cudf=${cudf:-25.02}
[ "$(id -u)" != 0 ] || die "run as the developer, not root: the clone, gh, Claude Code and the image's runs are theirs; sudo is used where needed"
if [ -n "$ref" ] && [ -e "$clone" ]; then
  on_branch=$(git -C "$clone" branch --show-current)
  [ "$on_branch" = "$ref" ] \
    || die "$clone is on ${on_branch:-a detached HEAD}, not $ref: --ref names the branch the clone is or will be on"
fi
command -v sudo > /dev/null || die "no sudo: Docker, gh, nebius and the base tools are installed with it"

packages=()
for tool in git curl python3 flock; do
  command -v "$tool" > /dev/null || packages=(git curl ca-certificates python3 util-linux)
done
if ! command -v docker > /dev/null; then packages+=(docker.io); fi
if [ ${#packages[@]} -gt 0 ]; then
  sudo apt-get update -q
  sudo apt-get install -yq "${packages[@]}"
fi
if [[ " $(id -nG "$(id -un)") " != *" docker "* ]]; then sudo usermod -aG docker "$(id -un)"; fi
if [ ! -e "$clone" ]; then git clone --branch "${ref:-master}" "$REPO_URL" "$clone"; fi
[ -f "$clone/docker/conda/cudf-$cudf.spec" ] || die "no cuDF $cudf: $clone/docker/conda has no cudf-$cudf.spec"
. "$clone/scripts/lib/image-inputs.sh"
. "$clone/scripts/lib/data-inputs.sh"
. "$clone/scripts/lib/machine.sh"
declare -F image_inputs_tag > /dev/null \
  || die "$clone/scripts/lib/image-inputs.sh has no image_inputs_tag: the clone's ref predates finding images by their inputs. Give a newer --ref"

# The data directory every developer on the machine shares. Its owner writes it at once; another
# developer through the group, which provision-vm.sh adds them to before their first login.
if ! getent group peacock > /dev/null; then sudo groupadd peacock; fi
if [[ " $(id -nG "$(id -un)") " != *" peacock "* ]]; then sudo usermod -aG peacock "$(id -un)"; fi
if [ ! -d "$MACHINE_DATA" ]; then sudo install -d -o "$(id -un)" -g peacock -m 2775 "$MACHINE_DATA"; fi

# pin <name> — its value in the clone's docker/pins.env.
pin() {
  local line
  line=$(grep "^$1=" "$clone/docker/pins.env" || true)
  [[ "$line" =~ ^$1=[^[:space:]]+$ ]] || die "$clone/docker/pins.env holds no single $1"
  echo "${line#*=}"
}

# gh where the image has it: `gh auth setup-git` writes this absolute path into ~/.gitconfig.
gh_version=$(pin GH_VERSION)
gh_sha=$(pin GH_SHA256)
if [ "$(/usr/local/bin/gh --version 2> /dev/null | awk 'NR == 1 { print $3 }')" != "$gh_version" ]; then
  tarball=$(mktemp)
  # Every way out removes the download, as the nebius one's below.
  trap 'rm -f "$tarball"' EXIT
  curl -fsSL -o "$tarball" \
    "https://github.com/cli/cli/releases/download/v$gh_version/gh_${gh_version}_linux_amd64.tar.gz"
  echo "$gh_sha  $tarball" | sha256sum -c --quiet -
  sudo tar -xzf "$tarball" -C /usr/local --strip-components=1 --no-same-owner "gh_${gh_version}_linux_amd64/bin/gh"
  rm -f "$tarball"
  trap - EXIT
fi

# nebius from the URL the build image takes; nebius.sh finds it on PATH.
nebius_version=$(pin NEBIUS_VERSION)
nebius_sha=$(pin NEBIUS_SHA256)
if [ "$(nebius version 2> /dev/null)" != "$nebius_version" ]; then
  url=https://artifacts.nebius.cloud/cli/release/$nebius_version/linux/x86_64/nebius
  binary=$(mktemp)
  # Every way out, an interrupt included, removes the download; the script has no EXIT trap of its own.
  trap 'rm -f "$binary"' EXIT
  curl -fsSL --retry 3 -o "$binary" "$url" || die "cannot download $url"
  echo "$nebius_sha  $binary" | sha256sum -c --status - || die "$url is not docker/pins.env's NEBIUS_SHA256"
  sudo install -m 755 "$binary" /usr/local/bin/nebius
  rm -f "$binary"
  trap - EXIT
fi

# Claude Code stays at its pin: it never updates itself.
export DISABLE_AUTOUPDATER=1
pinned="export DISABLE_AUTOUPDATER=1"
if ! grep -qxF "$pinned" "$HOME/.bashrc" 2> /dev/null; then
  printf '\n# Claude Code at its docker/pins.env version, as scripts/env/bootstrap-vm.sh installed it\n%s\n' \
    "$pinned" >> "$HOME/.bashrc"
fi
claude_version=$(pin CLAUDE_CODE_VERSION)
claude=$HOME/.local/bin/claude
claude_at() { "$claude" --version 2> /dev/null | awk '{ print $1 }'; }
if [ "$(claude_at)" != "$claude_version" ]; then
  curl -fsSL https://claude.ai/install.sh | bash -s -- "$claude_version"
fi
[ "$(claude_at)" = "$claude_version" ] || die "Claude Code at $claude is not $claude_version"

# The marketplace pins each plugin to a commit and cannot be added at one, so the pin is checked
# on what the install recorded.
plugin_version=$(pin SUPERPOWERS_VERSION)
plugin_sha=$(pin SUPERPOWERS_SHA)
record=$HOME/.claude/plugins/installed_plugins.json
plugin_is_pinned() {
  python3 - "$record" "$plugin_version" "$plugin_sha" << 'PY'
import json, sys
path, version, sha = sys.argv[1:]
try:
    entries = json.load(open(path))["plugins"]["superpowers@claude-plugins-official"]
except (OSError, KeyError, ValueError):
    sys.exit(1)
sys.exit(0 if any(e.get("version") == version and e.get("gitCommitSha") == sha for e in entries) else 1)
PY
}
if ! plugin_is_pinned; then
  if ! grep -qs claude-plugins-official "$HOME/.claude/plugins/known_marketplaces.json"; then
    "$claude" plugin marketplace add "$MARKETPLACE"
  fi
  "$claude" plugin install superpowers@claude-plugins-official
fi
plugin_is_pinned || die "superpowers is not $plugin_version at $plugin_sha in $record: the marketplace has \
moved on, so docker/pins.env's SUPERPOWERS_VERSION and SUPERPOWERS_SHA must be bumped to what it installed"

# The docker group this run joined takes effect at the next login.
if ! docker info > /dev/null 2>&1; then docker() { sudo docker "$@"; }; fi
# The image of the clone's own inputs, by its inputs tag: :cudf-<v> moves with master alone.
ref=$MACHINE_IMAGE:$(image_inputs_tag "$clone" build "$cudf")
machine_image_checked "$ref" "$clone" "$cudf"
machine_cargo_fetch "$ref" "$clone"
machine_data_set "$ref" "$clone"
# The C++ configure, the FFI's too, reads cuDF's cmake files from the clone's own submodule; no step
# above reads them, so a failed fetch stops nothing else. Two runs at once each check it out, so
# the checkout is serialised under a lock in the clone's git dir, a linked worktree's common one.
git_dir=$(git -C "$clone" rev-parse --path-format=absolute --git-common-dir)
exec {submodule_lock}> "$git_dir/peacock-submodule.lock"
flock "$submodule_lock"
git -C "$clone" submodule update --init --depth 1 third_party/cudf \
  || die "cannot check out $clone/third_party/cudf at $(machine_cudf_gitlink "$clone")"
exec {submodule_lock}>&-

cat << EOF
The machine is ready. scripts/env/provision-vm.sh, run on your own machine, does the rest; by
hand instead, log out and in again, for the docker group, then:
  1. claude: /login, then /exit
  2. gh: a fine-grained PAT for the peacockdb repository alone (llm-wiki/build-test.md,
     "A fresh VM"); gh auth login --with-token --insecure-storage, paste it, Enter, Ctrl-D;
     then gh auth setup-git
  3. git's identity: git config --global user.name <name>; git config --global user.email <email>
  4. the storage keys in ~/.config/peacock/peacock.env (llm-wiki/build-test.md, "The env file")
EOF
