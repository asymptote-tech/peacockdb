# drop-mode-name — run detail

Recovery notes for a restarted coordinator. The spec is `drop-mode-name.md`; the board entry
is in `tasks.md`.

## Where the task stands (2026-09-09)

Implementation, review and CI are finished. PR [#141](https://github.com/asymptote-tech/peacockdb/pull/141)
is open against `master` with eleven commits, and its head is the branch tip.

The task ran to `done` under the flat board that preceded the board protocol (#142), so it
never got a completeness signoff. That is the one artifact still missing, and closing it is
what the current run is for.

## The rebase across #142 is done

Ordered by the board's `rebase needed(done)`, carried across from master. Replayed onto
`e7e22600` at 15:43; pre-rebase tip was `abc7ca89`. `git diff abc7ca89 HEAD` is exactly
master's own content and nothing else:

    .claude/agents/*, .claude/commands/, .gitignore, llm-wiki/**, scripts/ensemble-watchdog.sh

No code, no test, no golden, no fixture. The `tasks.md` conflict was resolved by ownership:
master's side for which tasks exist and their prose, this branch's side for the states, and
this branch's `active-tickets.md` link paths, which are task 1's own rename.

## Why no developer re-run after the rebase

The rebase rule exists so a rebase that quietly broke something cannot reach the completeness
pass looking approved. Two things stand in for the re-run here, and both are on the exact
post-rebase SHA `7ed0bcf9`:

- The full pipeline is green — both cuDF versions, the 25.02 GPU build, the remote GPU tier,
  the cost report, the changed-paths and S3 metadata jobs. That is a machine re-run of the
  proving commands on the rebased tree, and a wider one than a developer would run locally.
- The three grep gates, which CI does not run, were re-run by hand. Gate 1 lands on the
  documented six; gates 2 and 3 are empty. Run under `LC_ALL=C.UTF-8`, as the spec requires.

The `.gitignore` change in the rebase only un-ignores `.claude/agents/` and
`.claude/commands/`, so it widens what `--untracked` can see rather than narrowing it. No
gate went blind.
