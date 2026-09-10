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

## The completeness pass

Run on the rebased branch: a reviewer asking what is wrong and an analyst asking what is
missing, neither seeing the other's list. No blocking findings on either side. Both
independently re-derived the spec's Validation section and found it holds — the 33 golden
renames are byte-exact apart from 94 `mode=` lines and the 75 quarantined refusal lines, the
payload digests never moved, the case inventory maps 1:1 under prefix removal, and the two
registry CSVs agree.

Seven important findings between them, four of which are the same underlying fact.

**The gate could not see `peacockdb-core/src`.** Gate 1 excludes that tree by pathspec, to
spare `src/batch_partitioned/`, so nothing read the mode's own module. Four sites inside it
still named the mode: `error.rs`'s two `RunError` display arms, `mod.rs`'s module doc, and a
temp-dir name in `parquet_meta.rs`. `PlanError`'s arms had been reworded and `RunError`'s were
simply missed. The spec's claim that the line-scoped hole was "latent rather than live" was
true only of what the gate reads; the spec now says so and names the scoped check that finds
these. `module-layout.md` gains the consequence for task 2: once the directory is gone the
exclusion has nothing left to spare and would blind the gate to the whole crate.

**Bare `bp` survived in four llm-wiki lines** that neither exemption covers — `tickets.md:343`
and three lines in the ENS-casts specs `wire-schema.md` and `empty-answers.md`. Gate 2 excludes
`llm-wiki` whole and `\bbp[-_]` would not match a bare word anyway, so nothing mechanical was
ever going to catch them.

**The free-ticket counter was not advanced.** This branch filed #197 while `tickets.md:9` still
advertised 197 as free, across an ID space shared with `tasks/active-tickets.md`. Now 198.

The board entry lagged its own rebase, which is the fifth. Fixed with the signoff.

Both agents noted the spec is not frozen — six of the twelve commits rewrote it, and the gate's
expected finish moved from empty to five to six over them. Each of the six survivors was checked
independently and all are deliberate, but "the gate lands where the spec says" is partly
self-fulfilling here. The signoff says so.

## Second rebase, onto `d55ff42c`

The control file still said `rebase` at the 18:17 restart, and master had moved one commit
past the `e7e22600` the first rebase used. Replayed with no conflict at all — master's one
commit carries `tasks.md` (tasks 3 and 4 to `approved to build`) and one line of
`scripts/ensemble-watchdog.sh`, and nothing else. Again no code, no test, no golden, no
fixture, so the first rebase's re-verification argument still stands unchanged.

## The four residues, unproven at the restart

The 17:52 run dispatched a developer to fix the four sites the blind gate hid, and the run was
terminated before it reported. The edits were in the working tree, unverified. They are now
`a7d690e6`, deliberately labelled WIP: two `RunError` display arms in `error.rs`, the module
doc in `mod.rs`, and a temp-dir name in `parquet_meta.rs`. A developer proves them before the
commit is reworded and the signoff written.

## Run of 2026-09-09 18:2x — proving the residues

No control file at start; the board still reads `rebase needed(done)`, and the rebase itself is
finished (both of them). What stands between here and `done` is one thing: `a7d690e6` is
unproven, so the rebase rule's "developer re-runs the proving commands and reports green" has
not been satisfied for the tree as it now stands.

Dispatched a developer against `a7d690e6` with the spec's Validation section as its command
list. The one hazard named in the dispatch: `RunError`'s two display arms are user-visible
strings, and the corpus goldens carry quarantined refusal lines, so this edit can move a golden
even though task 1 is a names-only task. If a golden moves, that is a finding, not a
regeneration.

On green: reword `a7d690e6` off WIP, write the signoff into the spec, set the board to `done`.

## Restart of 2026-09-09 — the 18:2x dispatch died with its run

Nothing new reached the branch or this file after `8da66b0e`, so the developer dispatched to
prove `a7d690e6` was killed before it reported. Control file empty at this restart. Re-dispatched
against the same commit with the same hazard called out: `RunError`'s two display arms are
user-visible, the corpus goldens carry quarantined refusal lines, and a golden that moves is a
finding rather than a regeneration. Same finish condition: on green, reword `a7d690e6` off WIP,
write the signoff into the spec, set the board to `done`.
