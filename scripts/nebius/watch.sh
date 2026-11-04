#!/usr/bin/env bash
# The hourly checker's body for one VM (nebius-watch.yml; llm-wiki/build-test.md, "Nebius host").
# If the VM is RUNNING, it reads the watcher's last log line over ssh and hard-stops the VM when
# check.py says stop, or when ssh does not answer within the window.
#
#   watch.sh <vm name> [nebius.sh options…]
#     <vm name>  how the workflow's notices and warnings name the VM
#     options    nebius.sh's --instance-id and --profile, passed through
#
# Env: CLOCK_OFFSET, seconds added to the checker's clock (a test dispatch); GPU_WATCH_WINDOW, the
# ssh window in seconds (900); GPU_WATCH_PAUSE, the pause between ssh tries (60).
# Exit 0 when the VM is not running, is gone, is kept, or was hard-stopped. Exit 1 when status
# fails otherwise or the stop fails; 64 for usage.
set -euo pipefail

[ $# -ge 1 ] || { echo "usage: watch.sh <vm name> [nebius.sh options…]" >&2; exit 64; }
vm=$1; shift
here=$(cd "$(dirname "$0")" && pwd)
nebius() { "$here/../nebius.sh" "$@"; }
CLOCK_OFFSET=${CLOCK_OFFSET:-0}
window=${GPU_WATCH_WINDOW:-900}
pause=${GPU_WATCH_PAUSE:-60}

if ! [[ $CLOCK_OFFSET =~ ^(0|[1-9][0-9]{0,5})$ ]]; then
  echo "::error::clock_offset must be a non-negative integer of at most 6 digits"
  exit 1
fi
rc=0; s=$(nebius "$@" status) || rc=$?
if [ "$rc" -eq 69 ]; then
  # The human may delete a VM; the watch stays green without it.
  echo "::notice::$vm: instance no longer exists; skipped"
  exit 0
fi
[ "$rc" -eq 0 ] || { echo "::error::$vm: status failed (rc=$rc)"; exit 1; }
echo "$vm: $s"
case $s in RUNNING*) ;; *) echo "not running; nothing to do"; exit 0;; esac

# A 15-minute window: a VM still booting answers well inside it. A VM that stops or is
# preempted meanwhile fails every try, and the stop below finds it already STOPPED.
up=""; line=""; ok=0; deadline=$((SECONDS + window))
while :; do
  if up=$(nebius "$@" ssh 'cut -d. -f1 /proc/uptime') \
     && line=$(nebius "$@" ssh 'tail -1 /var/log/peacock-gpu-idle.log 2>/dev/null; true'); then
    ok=1; break
  fi
  [ "$SECONDS" -lt "$deadline" ] || break
  sleep "$pause"
done

if [ "$ok" -eq 0 ]; then
  reason="ssh unanswered for ${window}s"
else
  # Up under 10 minutes: the watcher's first tick may not have run, and the log's last line is
  # then the previous session's, which reads as stale. Next hour decides.
  if [[ $up =~ ^[0-9]+$ ]] && [ "$up" -lt 600 ]; then
    echo "up ${up}s, under 10 minutes; keeping it until the next check"
    exit 0
  fi
  # A test dispatch shifts the checker's clock forward, so an idle VM reads as stale.
  if [ "$CLOCK_OFFSET" -ne 0 ]; then
    echo "::notice::the checker's clock is shifted by ${CLOCK_OFFSET} s for a test dispatch"
  fi
  # Any non-zero exit of the checker is a stop, a crash included: it is the guard that runs when
  # everything else is broken, so it fails toward stopping.
  if reason=$(printf '%s\n' "$line" | python3 "$here/check.py" "$(( $(date +%s) + CLOCK_OFFSET ))"); then
    echo "in use; keeping it"
    exit 0
  fi
  reason=${reason:-the checker failed}
fi
# Part of the reason is the VM's own log text: one line, so it cannot start a workflow command
# of its own.
reason=${reason//[$'\r\n']/ }
echo "::warning::hard-stopping $vm: $reason"
rc=0; nebius "$@" stop --force || rc=$?
[ "$rc" -eq 0 ] || { echo "::error::$vm: hard stop failed (rc=$rc)"; exit 1; }
