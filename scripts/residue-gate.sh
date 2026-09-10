#!/usr/bin/env bash
# Task 1's prose-and-labels gate, run the way module-layout has to run it: without the
# ':!peacockdb-core/src' exclusion (this task deletes the directory that forced it), with
# --untracked (git grep is blind to files a move has not staged), and with the
# strip-and-rematch over the excluded lines, since -vE drops a whole line and a survivor
# spelling anywhere on it hides real residue.
#
# The four survivor spellings are now gone from the tree, so the exclusion matches nothing
# and the strip-and-rematch has nothing to read. Both are kept anyway: a `SURVIVORS` that
# matches nothing is the proof, and deleting it would make the next reader wonder whether
# the gate was ever excluding anything.
set -uo pipefail
root="$(git rev-parse --show-toplevel)"
# This file states the spellings, so it matches all four of its own greps. It sat under
# `llm-wiki` and was excluded with it; from `scripts/` it has to exclude itself by name.
# Resolved against the repo root rather than taken from `$0`, which is relative to wherever
# the caller stood — git then rejects the pathspec and every section comes back empty, which
# reads exactly like a clean tree. Checked rather than assumed, for the same reason.
self="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/$(basename "${BASH_SOURCE[0]}")"
SELF=":!${self#"$root"/}"
[ -f "$root/${self#"$root"/}" ] || { echo "residue-gate.sh: cannot place itself in the repo" >&2; exit 1; }
cd "$root"
export LC_ALL=C.UTF-8
SURVIVORS='mod batch_partitioned|batch_partitioned/|batch_partitioned::|::batch_partitioned|plan_batch_partitioned|batch_partitioned_driver'

echo "== residue (survivor spellings excluded whole-line)"
git grep -inE --untracked 'batch.?partition' -- ':!llm-wiki' "$SELF" | grep -vE "$SURVIVORS"

echo "== strip-and-rematch over the excluded lines"
git grep -inE --untracked 'batch.?partition' -- ':!llm-wiki' "$SELF" | grep -E "$SURVIVORS" \
  | sed -E "s@$SURVIVORS@@g" | grep -inE 'batch.?partition'

echo "== bp gates (both must be empty)"
git grep -nE --untracked '\bbp[-_]' -- ':!llm-wiki' "$SELF"
git grep -n --untracked 'bp-tickets' -- ':!llm-wiki' "$SELF"
echo "== end"
