#!/usr/bin/env bash
# Interactive helper session: the agent definition's model and effort apply only to a
# dispatched subagent, so a session started by hand has to name them itself.
set -euo pipefail

# The helper works in the primary checkout on master. A chain worktree is a coordinator's,
# and a second writer on its board is what the writer split exists to prevent.
branch=$(git rev-parse --abbrev-ref HEAD 2>/dev/null || echo "(not a git repo)")
if [ "$branch" != master ] || [ "$(git rev-parse --git-dir)" != "$(git rev-parse --git-common-dir)" ]; then
  echo "warning: the helper belongs in the primary checkout on master; this is $PWD on $branch" >&2
fi

exec claude --agent peacockdb-helper --effort xhigh --dangerously-skip-permissions "$@"
