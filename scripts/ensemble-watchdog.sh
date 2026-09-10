#!/usr/bin/env bash
# Restarts the chain coordinator until it reports that nothing can progress.
#
# The coordinator exits when its window gets tight, which is normal and not a failure — the
# board is the state, so a fresh one resumes. Two things stop the loop: the coordinator saying
# it is stalled, and a run of restarts that advances no task.
#
# A run is never timed out from here. Tasks are long, a build can run for hours, and a
# watchdog that cannot tell a slow task from a stuck one would kill the slow ones. Bounding a
# dispatch is the coordinator's job, because only it knows what the dispatch was supposed to
# do; see the rule about dispatches that stop moving in llm-wiki/prompts.md.
#
# Progress is measured as a change in this chain's board states, not as a commit. A
# coordinator that writes a detail-file note every pass commits each time while advancing
# nothing, and a commit-based counter would call that progress forever.
set -uo pipefail

usage="usage: ensemble-watchdog.sh [--non-interactive] [--max-idle-restarts N] [--limit-seconds N] <chain-branch>"

# Interactive is the default because a human starting one watches it; --non-interactive is the
# unattended form, which is the one that needs the log and the headless permission grant.
non_interactive=""
max_idle=3
limit_seconds=600
chain=""

die() { printf 'watchdog: %s\n%s\n' "$1" "$usage" >&2; exit 2; }

# A count, taken before anything is written: a typo should fail here, not halfway through a run.
whole_number() {
  case "$2" in ''|*[!0-9]*) die "$1 takes a whole number of ${3}, got '$2'" ;; esac
  [ "$2" -gt 0 ] || die "$1 must be greater than zero"
}

while [ $# -gt 0 ]; do
  case "$1" in
    --non-interactive) non_interactive=1; shift ;;
    --max-idle-restarts) [ $# -ge 2 ] || die "$1 needs a value"
                         whole_number "$1" "$2" restarts; max_idle=$2; shift 2 ;;
    --limit-seconds)     [ $# -ge 2 ] || die "$1 needs a value"
                         whole_number "$1" "$2" seconds; limit_seconds=$2; shift 2 ;;
    -h|--help) printf '%s\n' "$usage"; exit 0 ;;
    -*) die "unknown flag $1" ;;
    *) [ -z "$chain" ] || die "two chains named, $chain and $1"; chain=$1; shift ;;
  esac
done
[ -n "$chain" ] || die "no chain named"

status=".claude/ensemble/${chain}.status"
board="llm-wiki/tasks/tasks.md"

# How long to wait after each consecutive fast failure; --limit-seconds says how fast one has
# to be to count. The last figure repeats: a limit window is bounded, so
# waiting an hour at a time resumes the chain by itself without anyone parsing a reset time.
# Ten minutes is generous on purpose — a coordinator that reads the board, dispatches nothing
# and dies has not worked either, and waiting is the cheaper thing to be wrong about.
backoff=(300 900 1800 3600)
backoff_step=0

# The loop must still terminate. A limit window is hours, so a run of fast failures that
# outlasts every plausible one is a broken setup wearing a limit's shape — a missing binary, a
# chain nobody can drive — and waiting another hour will not fix it.
max_limit_waits=8
limit_waits=0
limit_waited=0

# The states of this chain's tasks, digested. Scoped to our own section: a rebase can bring
# another chain's edits across, and those are not our progress.
board_state() {
  awk -v c="$chain" '
    $0 ~ "^## Chain " c "([ (]|$)" { inside = 1; next }
    /^## / { inside = 0 }
    inside && /state:/ { print }
  ' "$board" 2>/dev/null | md5sum | cut -d' ' -f1
}

# Run from inside that chain's worktree. Getting this wrong drives a coordinator across
# another chain's branch, and the first thing it would do is commit a board it should not own.
#
# The check is worktree identity, not branch name: a chain is a stack of branches and the
# coordinator walks up it, so HEAD stops matching the chain name after the first task. A
# linked worktree has its own git dir; the primary checkout's git dir is the common one.
if [ "$(git rev-parse --git-dir)" = "$(git rev-parse --git-common-dir)" ]; then
  printf 'watchdog: this is the primary checkout; run inside the chain worktree\n' >&2
  exit 2
fi

marker=".claude/ensemble/chain"
mkdir -p "$(dirname "$marker")"
if [ -s "$marker" ]; then
  owner=$(cat "$marker")
  if [ "$owner" != "$chain" ]; then
    printf 'watchdog: this worktree belongs to %s, not %s\n' "$owner" "$chain" >&2
    exit 2
  fi
else
  printf '%s\n' "$chain" > "$marker"
fi

rm -f "$status"
idle=0

# Headless runs print one closing message and nothing else, so without this a coordinator that
# dies leaves no trace of what it last said. Appended, not truncated: the interesting history
# is across restarts. Interactive runs are not teed — a pipe costs the session its terminal.
log=".claude/ensemble/${chain}.log"
if [ -z "$non_interactive" ]; then
  printf 'watchdog: interactive; not logging\n'
else
  printf 'watchdog: logging to %s/%s\n' "$(pwd)" "$log"
fi

while :; do
  before=$(board_state)
  started=$SECONDS
  if [ -z "$non_interactive" ]; then
    claude  --dangerously-skip-permissions "/ensemble ${chain}"      # a human is driving; not logged
    rc=$?
  else
    printf '\n===== %s  /ensemble %s =====\n' "$(date -Is)" "$chain" >> "$log"
    # Unattended, so permissions cannot be granted: a headless run refuses any tool that
    # would prompt, and the coordinator then fails to commit or push while looking like it
    # simply made no progress. This is confined to a chain worktree on a chain branch.
    claude -p --dangerously-skip-permissions "/ensemble ${chain}" 2>&1 | tee -a "$log"
    rc=${PIPESTATUS[0]}   # tee's status otherwise, which is 0 however the coordinator died
  fi
  elapsed=$((SECONDS - started))

  if grep -q '^stalled:' "$status" 2>/dev/null; then
    printf 'watchdog: %s\n' "$(cat "$status")"
    exit 0
  fi

  # A usage limit ends a run in seconds having done nothing, and says so nowhere this script
  # can read — so the only evidence is a non-zero exit that came back too fast to have worked.
  # Counting those as idle restarts spends the whole allowance in one second and reports a
  # stall that is really a wait, so back off instead and leave the board and the counter alone.
  if [ "$rc" -ne 0 ] && [ "$elapsed" -lt "$limit_seconds" ] && [ "$(board_state)" = "$before" ]; then
    limit_waits=$((limit_waits + 1))
    if [ "$limit_waits" -gt "$max_limit_waits" ]; then
      printf 'stalled: %d runs failed inside %ds over %dm of waiting; not a usage limit\n' \
        "$limit_waits" "$limit_seconds" "$((limit_waited / 60))" > "$status"
      printf 'watchdog: %s' "$(cat "$status")"
      exit 1
    fi
    wait_for=${backoff[$backoff_step]}
    printf 'watchdog: run exited %d after %ds with nothing advanced; waiting %dm for the limit to reset\n' \
      "$rc" "$elapsed" "$((wait_for / 60))" | tee -a "$log"
    backoff_step=$(( backoff_step + 1 < ${#backoff[@]} ? backoff_step + 1 : ${#backoff[@]} - 1 ))
    limit_waited=$((limit_waited + wait_for))
    sleep "$wait_for"
    continue
  fi
  backoff_step=0
  limit_waits=0

  if [ "$(board_state)" = "$before" ]; then
    idle=$((idle + 1))
    if [ "$idle" -ge "$max_idle" ]; then
      # Say so on the board's own channel, not just on stdout: a human coming back to this
      # chain reads the status file, and a run that ends silently leaves it looking mid-flight.
      printf 'stalled: no task advanced in %d restarts; last run %s\n' "$idle" "$(date -Is)" > "$status"
      printf 'watchdog: %s' "$(cat "$status")"
      exit 1
    fi
  else
    idle=0
  fi
done
