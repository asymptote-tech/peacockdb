#!/usr/bin/env bash
# Prepare a fresh CPU VM once for scripts/env/up.sh (llm-wiki/build-test.md, "Containers").
#
#   bootstrap-vm.sh [--ref <branch>] [--cudf <v>]
#
# Run as the machine user, with sudo; provision-vm.sh runs it so. Installs what is missing: Docker,
# and gh and Claude Code with the superpowers plugin at docker/pins.env's versions; makes the data
# directory the machine's users share; clones the repository into ~/peacockdb at <ref>, master by
# default, and the third_party/cudf it pins. In the build image of cuDF <v> (25.02 by default) it
# fills cargo's registry and generates sf1. A step already done is skipped. Its last step is the
# marker ~/.config/peacock/machine: this machine runs instances.
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
[ "$(id -u)" != 0 ] || die "run as the machine user, not root: the instances run as its uid; sudo is used where needed"
# scripts/lib/machine.sh's machine_git, for the calls made before the clone holds it.
safe_git() { git -c core.hooksPath=/dev/null -c core.fsmonitor=false "$@"; }
if [ -n "$ref" ] && [ -e "$clone" ] && [ "$(safe_git -C "$clone" branch --show-current)" != "$ref" ]; then
  die "$clone is there already, not on $ref: --ref names the branch the clone is or will be on"
fi
command -v sudo > /dev/null || die "no sudo: Docker, gh and the base tools are installed with it"

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
if [ ! -e "$clone" ]; then safe_git clone --branch "${ref:-master}" "$REPO_URL" "$clone"; fi
[ -f "$clone/docker/conda/cudf-$cudf.spec" ] || die "no cuDF $cudf: $clone/docker/conda has no cudf-$cudf.spec"
. "$clone/scripts/lib/image-inputs.sh"
. "$clone/scripts/lib/data-inputs.sh"
. "$clone/scripts/lib/machine.sh"
. "$clone/scripts/lib/peacock-config.sh"

# The data directory every developer on the machine shares. Its owner writes it at once; another
# developer through the group, which provision-vm.sh adds them to before their first login.
if ! getent group peacock > /dev/null; then sudo groupadd peacock; fi
if [[ " $(id -nG "$(id -un)") " != *" peacock "* ]]; then sudo usermod -aG peacock "$(id -un)"; fi
if [ ! -d "$MACHINE_DATA" ]; then sudo install -d -o "$(id -un)" -g peacock -m 2775 "$MACHINE_DATA"; fi

# The cuDF cmake files the FFI build reads, at the commit the clone pins; up.sh fetches others.
machine_cudf_source "$clone"

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
  curl -fsSL -o "$tarball" \
    "https://github.com/cli/cli/releases/download/v$gh_version/gh_${gh_version}_linux_amd64.tar.gz"
  echo "$gh_sha  $tarball" | sha256sum -c --quiet -
  sudo tar -xzf "$tarball" -C /usr/local --strip-components=1 --no-same-owner "gh_${gh_version}_linux_amd64/bin/gh"
  rm -f "$tarball"
fi

# Claude Code keeps its login and plugins in one directory, which up.sh mounts into each instance.
export DISABLE_AUTOUPDATER=1 CLAUDE_CONFIG_DIR=$HOME/.claude
marker="# The machine's Claude Code, as scripts/env/bootstrap-vm.sh set it up for up.sh"
if ! grep -qxF "$marker" "$HOME/.bashrc" 2> /dev/null; then
  printf '\n%s\nexport DISABLE_AUTOUPDATER=1 CLAUDE_CONFIG_DIR="$HOME/.claude"\n' "$marker" >> "$HOME/.bashrc"
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
record=$CLAUDE_CONFIG_DIR/plugins/installed_plugins.json
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
  if ! grep -qs claude-plugins-official "$CLAUDE_CONFIG_DIR/plugins/known_marketplaces.json"; then
    "$claude" plugin marketplace add "$MARKETPLACE"
  fi
  "$claude" plugin install superpowers@claude-plugins-official
fi
plugin_is_pinned || die "superpowers is not $plugin_version at $plugin_sha in $record: the marketplace has \
moved on, so docker/pins.env's SUPERPOWERS_VERSION and SUPERPOWERS_SHA must be bumped to what it installed"

# The docker group this run joined takes effect at the next login.
if ! docker info > /dev/null 2>&1; then docker() { sudo docker "$@"; }; fi
ref=$MACHINE_IMAGE:cudf-$cudf
machine_image_checked "$ref" "$clone" "$cudf"
machine_cargo_fetch "$ref" "$clone"
machine_data_set "$ref" "$clone"

install -d -m 700 "$PEACOCK_CONFIG"
echo "bootstrap-vm.sh made this machine one that runs instances: up.sh and down.sh run here" > "$PEACOCK_MARKER"

cat << EOF
The machine is ready. scripts/env/provision-vm.sh, run on your own machine, does the rest; by
hand instead, log out and in again, for the docker group and CLAUDE_CONFIG_DIR, then:
  1. claude: /login, then /exit
  2. gh: a fine-grained PAT for the peacockdb repository alone (llm-wiki/build-test.md,
     "A fresh VM"); gh auth login --with-token --insecure-storage, paste it, Enter, Ctrl-D;
     then gh auth setup-git
  3. git's identity: git config --global user.name <name>; git config --global user.email <email>
  4. the storage keys in ~/.config/peacock/peacock.env (llm-wiki/build-test.md, "The env file")
Then: $clone/scripts/env/up.sh <chain>
EOF
