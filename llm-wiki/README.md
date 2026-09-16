# Running the ensemble

- **Define a task**: run `claude` here on master and ask the `peacockdb-helper` agent for it.
  It brainstorms with you, writes `tasks/<task>.md` and a board entry at `new`, and commits.
  Move the entry to `approved to build` when you are happy with the spec.
- **Run a chain**: chains are lettered `A`, `B`, `C`, …; each runs in a workspace — a linked
  worktree named `alpha`, `beta`, `gamma`, … — and workspaces are reused from chain to chain.
  Check out the chain's first branch in a free workspace, then name the chain to the watchdog:

      git worktree add ../peacockdb-alpha <first-task-branch>      # once per workspace
      cd ../peacockdb-alpha && git checkout <first-task-branch>    # on reuse
      ../peacockdb/scripts/ensemble-watchdog.sh --non-interactive <chain>

  It runs unattended until every task reaches `done` — PR open, CI green — or until it stalls.
- **Watch it, or steer it, from inside that workspace**, where its own `.claude/ensemble/`
  lives — status, control and log are per-workspace, never here. `<chain>.status` says why it
  stopped, `<chain>.log` is everything it said, and `echo rebase > .claude/ensemble/<chain>.control`
  reaches it (`pause` and `stop` too). A coordinator reads only its own branch, so `rebase` is
  also how tasks and board edits you made on master get to it. Drop `--non-interactive` to type
  at it instead — that is the default, so the flag is what asks for the unattended form.
  A run that hits a usage limit is not a stall: the watchdog waits it out and resumes by
  itself, backing off 5, 15, 30 then 60 minutes.
- **Merging is yours**: ask the helper. It merges oldest-first and archives the specs.
- **Free the workspace after a merge**: `rm .claude/ensemble/<chain>.*` inside it, and it is
  ready for the next chain — the build tree stays, which is the point of reusing it. To
  retire a workspace outright, `git worktree remove ../peacockdb-<name>`; an ignored
  `target/` does not block the removal and does not survive it. Removal is refused if a run left an
  untracked file behind. `git worktree prune` drops registrations whose
  directories are gone.
- The board is `tasks/tasks.md`; the instruction set every agent reads is `prompts.md`.
