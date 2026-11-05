# shellcheck shell=bash
#
# What gpu.sh and the host adapters call on the machine they run on, a macOS laptop included, where
# coreutils' sha256sum and timeout are missing. GNU's are taken where they are on PATH.

if ! command -v sha256sum > /dev/null; then sha256sum() { shasum -a 256 "$@"; }; fi

# timeout <seconds> <command>... — the command, killed after <seconds>; 124 then, as GNU's says.
if ! command -v timeout > /dev/null; then
  if command -v gtimeout > /dev/null; then
    timeout() { gtimeout "$@"; }
  else
    timeout() {
      local seconds=$1 pid watcher rc=0 mark
      shift
      mark=$(mktemp -u "${TMPDIR:-/tmp}/peacock-timeout.XXXXXX")
      "$@" &
      pid=$!
      # The watcher writes nowhere: a $(…) around the call must not wait for its sleep.
      # The mark comes first: once the command dies, the wait below returns and the watcher is killed,
      # its sleep with it.
      (
        s=""
        trap 'kill "$s" 2> /dev/null; exit 0' TERM
        sleep "$seconds" & s=$!
        wait "$s"
        touch "$mark" && kill "$pid" 2> /dev/null
      ) > /dev/null 2>&1 &
      watcher=$!
      wait "$pid" || rc=$?
      kill "$watcher" 2> /dev/null || true
      if [ -e "$mark" ]; then rm -f "$mark"; rc=124; fi
      return "$rc"
    }
  fi
fi

# version_at_least <version> <minimum> — dotted versions compared field by field as numbers: the
# sort on a macOS laptop may have no -V.
version_at_least() {
  local IFS=. i a b
  read -r -a a <<< "$1"
  read -r -a b <<< "$2"
  for ((i = 0; i < ${#a[@]} || i < ${#b[@]}; i++)); do
    if ((10#${a[i]:-0} != 10#${b[i]:-0})); then
      ((10#${a[i]:-0} > 10#${b[i]:-0}))
      return
    fi
  done
}
