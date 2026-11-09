# shellcheck shell=bash
#
# ssh_retry <ssh> <ssh argument>… — runs ssh again while it exits 255 before authenticating: the
# remote command cannot have started, and ssh has read none of stdin. After `Authenticated to`, a
# 255 is the command's own or a dropped session, and a host-key refusal is the caller's to repair;
# both are returned, as is a 255 with nothing logged. Each attempt has a log of its own (-E;
# VERBOSE writes that line) and a connection of its own; ssh takes an -o option's first value, so
# ours come first, and the flags that would override them are refused. stderr gets what plain ssh
# would print. sync_retry <command>… runs a sync, safe to repeat whole, again on 255 or rsync's
# I/O timeout, 30. Four attempts, PEACOCK_SSH_PAUSES s apart (5 10 20).

SSH_RETRY_ATTEMPTS=4

# ssh_retry_pause <n> <name> <failure> — names failed attempt <n> on stderr and sleeps its pause;
# false after the last attempt.
ssh_retry_pause() {
  local pauses
  read -ra pauses <<<"${PEACOCK_SSH_PAUSES:-5 10 20}"
  [ "$1" -lt "$SSH_RETRY_ATTEMPTS" ] || return 1
  echo "$2: attempt $1 of $SSH_RETRY_ATTEMPTS $3; again in ${pauses[$1 - 1]} s" >&2
  sleep "${pauses[$1 - 1]}"
}

# ssh_retry_clash <ssh argument>… — prints the first -E, -q, -S or -y among the options, read as ssh
# reads them: before the destination and again after it, up to the command or a `--`.
ssh_retry_clash() {
  local a i c host=
  while [ $# -gt 0 ]; do
    a=$1
    shift
    if [ "$a" = -- ]; then
      return 0
    elif [ "${a#-}" = "$a" ] || [ "$a" = - ]; then
      [ -z "$host" ] || return 0
      host=$a
      continue
    fi
    for ((i = 1; i < ${#a}; i++)); do
      c=${a:i:1}
      case $c in
        [EqSy]) printf -- '-%s\n' "$c"; return 0 ;;
        [bceilmopBDFIJLOPQRwW])
          [ $((i + 1)) -lt ${#a} ] || shift
          break ;;
      esac
    done
  done
}

# ssh_retry_shown <loglevel> <log> — the log's lines plain ssh prints at <loglevel>: below VERBOSE
# none of the three VERBOSE adds; below INFO not the known-hosts warning, though other INFO lines
# (a pty refused, a closed banner, a ServerAlive timeout) still show, as do FATAL's: the log does
# not tell those levels apart.
ssh_retry_shown() {
  local verbose='Authenticated to |Transferred: |Bytes per second: '
  case $1 in
    QUIET | SILENT) ;;
    FATAL | ERROR) grep -vE "^(Warning: Permanently added |$verbose)" "$2" || true ;;
    INFO) grep -vE "^($verbose)" "$2" || true ;;
    *) cat "$2" ;;
  esac
}

ssh_retry() {
  local ssh=$1 clash level log rc n=1
  shift
  clash=$(ssh_retry_clash "$@")
  if [ -n "$clash" ]; then
    echo "ssh_retry: $clash would take ssh's log or connection from the retry; drop it" >&2
    return 2
  fi
  # -G resolves the level without connecting (a config's Match exec still runs); stdin is kept for
  # the attempt, whatever $ssh is. ssh reports a bad argument itself.
  level=$("$ssh" -G "$@" </dev/null 2>/dev/null | sed -n 's/^loglevel //p') || true
  log=$(mktemp)
  while :; do
    : >"$log"
    rc=0
    "$ssh" -E "$log" -o LogLevel=VERBOSE -o ControlPath=none "$@" || rc=$?
    if [ "$rc" != 255 ] || [ ! -s "$log" ] \
      || grep -qE '^Authenticated to |Host key verification failed|REMOTE HOST IDENTIFICATION HAS CHANGED' "$log" \
      || ! ssh_retry_pause "$n" ssh "failed before authenticating ($(tail -n 1 "$log" | tr -d '\r'))"; then
      break
    fi
    n=$((n + 1))
  done
  ssh_retry_shown "${level:-INFO}" "$log" >&2
  rm -f "$log"
  return "$rc"
}

sync_retry() {
  local rc n=1
  while :; do
    rc=0
    "$@" || rc=$?
    case $rc in
      255 | 30) ssh_retry_pause "$n" "${1##*/}" "exited $rc" || return "$rc" ;;
      *) return "$rc" ;;
    esac
    n=$((n + 1))
  done
}
