#!/usr/bin/env bash
# Task 1's prose-and-labels gate, run the way module-layout has to run it:
# without the ':!peacockdb-core/src' exclusion (task 2 deletes the directory that
# forced it), with --untracked (git grep is blind to files a move has not staged),
# and with the strip-and-rematch over the excluded lines, since -vE drops a whole
# line and a survivor spelling anywhere on it hides real residue.
set -uo pipefail
cd "$(git rev-parse --show-toplevel)"
export LC_ALL=C.UTF-8
SURVIVORS='mod batch_partitioned|batch_partitioned/|batch_partitioned::|::batch_partitioned|plan_batch_partitioned|batch_partitioned_driver'

echo "== residue (survivor spellings excluded whole-line)"
git grep -inE --untracked 'batch.?partition' -- ':!llm-wiki' | grep -vE "$SURVIVORS"

echo "== strip-and-rematch over the excluded lines"
git grep -inE --untracked 'batch.?partition' -- ':!llm-wiki' | grep -E "$SURVIVORS" \
  | sed -E "s@$SURVIVORS@@g" | grep -inE 'batch.?partition'

echo "== bp gates (both must be empty)"
git grep -nE --untracked '\bbp[-_]' -- ':!llm-wiki'
git grep -n --untracked 'bp-tickets' -- ':!llm-wiki'
echo "== end"
