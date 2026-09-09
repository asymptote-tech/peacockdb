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

chain=${1:?usage: ensemble-watchdog.sh <chain-branch>}
status=".claude/ensemble/${chain}.status"
max_idle=${ENSEMBLE_MAX_IDLE_RESTARTS:-3}
board="llm-wiki/tasks/tasks.md"

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
if [ -n "${ENSEMBLE_INTERACTIVE:-}" ]; then
  printf 'watchdog: interactive; not logging\n'
else
  printf 'watchdog: logging to %s/%s\n' "$(pwd)" "$log"
fi

while :; do
  before=$(board_state)
  if [ -n "${ENSEMBLE_INTERACTIVE:-}" ]; then
    claude "/ensemble ${chain}"      # a human is driving; not logged
  else
    printf '\n===== %s  /ensemble %s =====\n' "$(date -Is)" "$chain" >> "$log"
    # Unattended, so permissions cannot be granted: a headless run refuses any tool that
    # would prompt, and the coordinator then fails to commit or push while looking like it
    # simply made no progress. This is confined to a chain worktree on a chain branch.
    claude -p --dangerously-skip-permissions "/ensemble ${chain}" 2>&1 | tee -a "$log"
  fi

  if grep -q '^stalled:' "$status" 2>/dev/null; then
    printf 'watchdog: %s\n' "$(cat "$status")"
    exit 0
  fi

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
