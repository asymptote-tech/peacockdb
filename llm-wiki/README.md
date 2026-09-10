# Running the ensemble

- **Define a task**: run `claude` here on master and ask the `peacockdb-helper` agent for it.
  It brainstorms with you, writes `tasks/<task>.md` and a board entry at `new`, and commits.
  Move the entry to `approved to build` when you are happy with the spec.
- **Run a chain**: one coordinator per chain, each in its own worktree.

      git worktree add ../peacockdb-<chain> <chain-branch>
      cd ../peacockdb-<chain> && ../peacockdb/scripts/ensemble-watchdog.sh <chain-branch>

  It runs unattended until every task reaches `done` — PR open, CI green — or until it stalls.
- **Watch it, or steer it, from inside that worktree**, where its own `.claude/ensemble/`
  lives — status, control and log are per-worktree, never here. `<chain>.status` says why it
  stopped, `<chain>.log` is everything it said, and `echo rebase > .claude/ensemble/<chain>.control`
  reaches it (`pause` and `stop` too). A coordinator reads only its own branch, so `rebase` is
  also how tasks and board edits you made on master get to it. `ENSEMBLE_INTERACTIVE=1` lets
  you type at it instead.
- **Merging is yours**: ask the helper. It merges oldest-first and archives the specs.
- **Clean up after a merge**: `git worktree remove ../peacockdb-<chain>` takes that chain's
  status, control and log with it, and its build tree — an ignored `target/` neither blocks
  the removal nor survives it. It does refuse if a run left an untracked file behind, so look
  at what that is before reaching for `--force`; otherwise the cleanup no-ops and the disk
  stays spent. `git worktree prune` drops registrations whose directory is already gone.
- The board is `tasks/tasks.md`; the instruction set every agent reads is `prompts.md`.
