# shellcheck shell=bash
#
# ~/.config/peacock/, on a person's own machine (the laptop) and on a VM (llm-wiki/build-test.md,
# "Containers"): the storage keys, the person's PAT and GPU key, and the VMs this laptop
# provisioned. A function that refuses prints one line to stderr and returns 2; it is called as a
# statement, so set -e stops the caller.

PEACOCK_CONFIG=$HOME/.config/peacock
PEACOCK_ENV=$PEACOCK_CONFIG/peacock.env
PEACOCK_PAT=$PEACOCK_CONFIG/github-pat
PEACOCK_GPU_KEY=$PEACOCK_CONFIG/gpu_ed25519
PEACOCK_VMS=$PEACOCK_CONFIG/vm
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
