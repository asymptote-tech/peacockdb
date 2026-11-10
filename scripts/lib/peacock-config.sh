# shellcheck shell=bash
#
# ~/.config/peacock/, on a person's own machine (the laptop) and on a VM (llm-wiki/build-test.md,
# "CPU VMs"): the storage keys, the person's PAT and GPU key, their two Nebius profiles, and the
# VMs this laptop provisioned. A function that refuses prints one line to stderr and returns 2; it
# is called as a statement, so set -e stops the caller.

PEACOCK_CONFIG=$HOME/.config/peacock
PEACOCK_ENV=$PEACOCK_CONFIG/peacock.env
PEACOCK_PAT=$PEACOCK_CONFIG/github-pat
PEACOCK_GPU_KEY=$PEACOCK_CONFIG/gpu_ed25519
PEACOCK_VMS=$PEACOCK_CONFIG/vm
PEACOCK_NEBIUS_CONFIG=$PEACOCK_CONFIG/nebius.yaml
# The profiles nebius.yaml holds, and nothing else; the first is the active one.
PEACOCK_NEBIUS_PROFILES=(editor-eu eu-west1)
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

# A Nebius config is read by the CLI alone: only its answers to `profile list`, `profile active`,
# `config get` and `version` are used, and nothing here opens the file (build-test.md, "Credential
# files").

# peacock_nebius_cli — sets PEACOCK_NEBIUS to the nebius CLI, PATH's before ~/.nebius/bin's.
peacock_nebius_cli() {
  PEACOCK_NEBIUS=$(command -v nebius) && return
  PEACOCK_NEBIUS=$HOME/.nebius/bin/nebius
  [ -x "$PEACOCK_NEBIUS" ] && return
  peacock_refuse "no nebius CLI on PATH or in ~/.nebius/bin: on a laptop, curl -sSL https://storage.eu-north1.nebius.cloud/cli/install.sh | bash; on a VM, scripts/env/bootstrap-vm.sh"
}

# peacock_nebius_profiles — `profile list` on stdin (CLI 0.12: a name a line, the active one with
# " [default]") -> the names, in order. Any other shape returns 1 and prints nothing: the line may
# be a key's, so it is not traced under set -x either.
peacock_nebius_profiles() {
  local -
  set +x
  local line names=() defaults=0
  while IFS= read -r line || [ -n "$line" ]; do
    [[ "$line" =~ ^([A-Za-z0-9_.-]+)( \[default\])?$ ]] || return 1
    names+=("${BASH_REMATCH[1]}")
    [ -z "${BASH_REMATCH[2]}" ] || defaults=$((defaults + 1))
  done
  [ "$defaults" -le 1 ] || return 1
  [ "${#names[@]}" -eq 0 ] || printf '%s\n' "${names[@]}"
}

# peacock_nebius_on <cli> <config> <args…> — one CLI call on <config>, never the CLI's default one.
# Its stdin is /dev/null; the CLI may prompt.
peacock_nebius_on() {
  "$1" --config "$2" --no-check-update "${@:3}" </dev/null
}

# peacock_nebius_version <cli> <config> — the CLI's version, or `unknown` for an answer of another
# shape: it goes into a message, so it is not traced under set -x either.
peacock_nebius_version() {
  local -
  set +x
  local v
  v=$(peacock_nebius_on "$1" "$2" version 2>/dev/null) || v=
  [[ "$v" =~ ^[0-9][0-9A-Za-z.+-]*$ ]] || v=unknown
  printf '%s\n' "$v"
}

# peacock_nebius_listed <cli> <config> — sets PEACOCK_NEBIUS_LISTED to <config>'s profiles, or,
# when the CLI fails or prints another shape, refuses naming the CLI's version, never the list.
peacock_nebius_listed() {
  local list
  if list=$(peacock_nebius_on "$1" "$2" profile list 2>/dev/null | peacock_nebius_profiles); then
    mapfile -t PEACOCK_NEBIUS_LISTED <<<"$list"
    [ -n "$list" ] || PEACOCK_NEBIUS_LISTED=()
    return 0
  fi
  peacock_refuse "the nebius CLI, version $(peacock_nebius_version "$1" "$2"), gave no profile list of the shape this reader knows, CLI 0.12's; it is not shown"
}

# peacock_nebius_cut <cli> <src> <copy> <dest> <active> <profile>[=<pem>]… — <src> copied to <copy>,
# cut to the listed profiles, each key embedded, <active> the active one, and moved to <dest>, mode
# 600. A profile that keeps a key path gets <pem>, else the file the path names, embedded and the
# path dropped. Only the CLI reads <src> and <copy>, and its calls' output is dropped: it can hold a
# key. <copy> holds every profile until the end, so the caller puts it in a private directory on
# <dest>'s filesystem and removes that on any exit. Sets PEACOCK_NEBIUS_LEFT_OUT to how many were
# left out.
peacock_nebius_cut() {
  local cli=$1 src=$2 copy=$3 dest=$4 active=$5 spec name pem path endpoint sa pk keep=() n
  shift 5
  # Made empty at 600 first: cp onto a file keeps its mode, so the copy is never readable by others.
  if ! (umask 077 && : >"$copy") || ! cp -- "$src" "$copy"; then
    peacock_refuse "cannot copy $src"; return
  fi
  peacock_nebius_listed "$cli" "$copy" || return 2
  for spec in "$@"; do
    name=${spec%%=*} pem=
    [ "$name" = "$spec" ] || pem=${spec#*=}
    keep+=("$name")
    [[ " ${PEACOCK_NEBIUS_LISTED[*]} " == *" $name "* ]] \
      || { peacock_refuse "$src has no profile $name"; return; }
    path=$(peacock_nebius_on "$cli" "$copy" --profile "$name" config get private-key-file-path 2>/dev/null) \
      || { peacock_refuse "cannot read profile $name's key path"; return; }
    if [ -z "$path" ]; then
      # Another PEM would replace a working key.
      [ -z "$pem" ] || { peacock_refuse "profile $name embeds its key; no other key file may replace it"; return; }
      continue
    fi
    [ -n "$pem" ] || pem=$path
    [ -f "$pem" ] || { peacock_refuse "profile $name's key path $pem names no file"; return; }
    # update prompts for these three unless given them, and keeps the path beside the embedded key.
    if ! endpoint=$(peacock_nebius_on "$cli" "$copy" --profile "$name" config get endpoint 2>/dev/null) \
      || ! sa=$(peacock_nebius_on "$cli" "$copy" --profile "$name" config get service-account-id 2>/dev/null) \
      || ! pk=$(peacock_nebius_on "$cli" "$copy" --profile "$name" config get public-key-id 2>/dev/null); then
      peacock_refuse "cannot read profile $name's endpoint and ids"; return
    fi
    if ! peacock_nebius_on "$cli" "$copy" profile update "$name" --skip-auth --endpoint "$endpoint" \
        --service-account-id "$sa" --public-key-id "$pk" --private-key-file "$pem" >/dev/null 2>&1 \
      || ! peacock_nebius_on "$cli" "$copy" --profile "$name" config unset private-key-file-path >/dev/null 2>&1; then
      peacock_refuse "cannot embed $pem into profile $name"; return
    fi
  done
  # The CLI refuses to delete the active profile, so activate first.
  peacock_nebius_on "$cli" "$copy" profile activate "$active" >/dev/null 2>&1 \
    || { peacock_refuse "cannot activate profile $active"; return; }
  PEACOCK_NEBIUS_LEFT_OUT=0
  for n in "${PEACOCK_NEBIUS_LISTED[@]}"; do
    [[ " ${keep[*]} " == *" $n "* ]] && continue
    peacock_nebius_on "$cli" "$copy" profile delete "$n" >/dev/null 2>&1 \
      || { peacock_refuse "cannot delete profile $n from the copy"; return; }
    PEACOCK_NEBIUS_LEFT_OUT=$((PEACOCK_NEBIUS_LEFT_OUT + 1))
  done
  if ! chmod 600 "$copy" || ! mv -f -- "$copy" "$dest"; then
    peacock_refuse "cannot move the cut into $dest"; return
  fi
}

# peacock_nebius_config_checked <cli> <file> — refuses unless <file> holds exactly
# PEACOCK_NEBIUS_PROFILES, each key embedded: a key path names a file the machine may not have.
peacock_nebius_config_checked() {
  local cli=$1 file=$2 p path extra=() missing=()
  [ -f "$file" ] || { peacock_refuse "no $file: make it with scripts/env/nebius-profiles.sh"; return; }
  peacock_nebius_listed "$cli" "$file" || return 2
  for p in "${PEACOCK_NEBIUS_LISTED[@]}"; do
    [[ " ${PEACOCK_NEBIUS_PROFILES[*]} " == *" $p "* ]] || extra+=("$p")
  done
  for p in "${PEACOCK_NEBIUS_PROFILES[@]}"; do
    [[ " ${PEACOCK_NEBIUS_LISTED[*]} " == *" $p "* ]] || missing+=("$p")
  done
  if [ "${#extra[@]}" -gt 0 ] || [ "${#missing[@]}" -gt 0 ]; then
    peacock_refuse "$file must hold the profiles ${PEACOCK_NEBIUS_PROFILES[*]} alone${extra[*]:+; it holds ${extra[*]}}${missing[*]:+; it lacks ${missing[*]}}: make it again with scripts/env/nebius-profiles.sh"
    return
  fi
  for p in "${PEACOCK_NEBIUS_PROFILES[@]}"; do
    path=$(peacock_nebius_on "$cli" "$file" --profile "$p" config get private-key-file-path 2>/dev/null) \
      || { peacock_refuse "cannot read profile $p's key path in $file"; return; }
    [ -z "$path" ] || { peacock_refuse "profile $p in $file reads its key from a file: make it again with scripts/env/nebius-profiles.sh"; return; }
  done
}
