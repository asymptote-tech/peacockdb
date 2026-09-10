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

## Restart of 2026-09-09 (third) — the dispatch is narrowed rather than repeated

Two dispatches to prove `a7d690e6` have now died with their coordinator run, and nothing new
reached the branch or this file after `8a7e0f25`. Both were handed the spec's whole Validation
section as their command list, which is the full pipeline including the device tier — hours
inside one call, with the coordinator holding the dispatch open the whole time. Repeating it a
third time repeats the shape that failed. So this run establishes the hazard by grep first, and
sends the developer a bounded list.

**The golden hazard is not live.** The two reworded arms are `RunError`'s `Protocol` and
`CallFailed` display strings. `git grep --untracked -iE 'protocol violation|call failed'` over
the tree outside `llm-wiki` returns only `peacockdb-core/src` — comments, the two arms
themselves, and test matches on the inner `said` payload, never on the display prefix. No file
under `testdata/` carries either string. The quarantined refusal lines in the ten `.plans.txt`
goldens come from `PlanError` at `error.rs:20` and `translate/mod.rs:221`, which this commit
does not touch. So no golden can move, and the device tier is not the first reader of anything
here.

**The three gates are at the documented finish on `a7d690e6`**, re-run by hand under
`LC_ALL=C.UTF-8`: gate 1 lands on the six deliberate survivors, the `peacockdb-core/src`-scoped
form with survivor spellings stripped per line lands on the one mapping site
(`gpu_rowgroup_prune.rs:151`), and gates 2 and 3 are empty. The four llm-wiki `bp` residues are
gone bar `tasks.md:43`, which is the board describing the abbreviation the task removes — same
exemption as the four specs. The free-ticket counter reads 198.

**What the developer is asked to prove**, and nothing beyond it: a clean build on both feature
sets against the recorded warning count; the lib tests over `batch_partitioned` (the `error`,
`driver` and `parquet_meta` cases, the last because the commit renames a temp dir); a
`UPDATE_CANONICAL=1` regen of the plan goldens and the cpu corpus tier with an empty `git diff`
after it, `PEACOCK_REWRITE_RECIPE_BYTES` never set; and `test_ci_coverage`. The device tier is
deliberately left to CI: the commit reaches no device path and no golden filename, and `done`
already waits on the PR being green, so the device claim is made by the pipeline rather than by
a dispatch long enough to die again.

On green: reword `a7d690e6` off WIP, force-push (remote is still at the pre-residue `7ed0bcf9`),
write the signoff into the spec, set the board to `done`, and wait for CI on #141.

## Proving `a7d690e6` — developer run of 2026-09-09 (third dispatch)

Bounded command list, per the narrowed dispatch. Progress recorded as it lands.

- `cargo build --features rust-only -p peacockdb-core -p peacockdb` after touching
  `peacockdb-core/src/lib.rs` and `peacockdb/src/main.rs` so both workspace crates actually
  recompile (a fully cached build prints no warnings and so proves nothing about the count).
  **Green in 10m33s, zero warnings.** No warning count is recorded anywhere in the spec or this
  file — baseline 3 was taken but never written down — so zero is the count this run establishes.
- Default (cudf) feature set: no warm `target-cudf-*` in this worktree, so it is a cold build
  against `rapids-cuda-12.2` (cuDF 25.02, gcc-12) via `scripts/cargo-cudf.sh`, throttled to
  `CARGO_BUILD_JOBS=3` per build-test.md's 15 GiB rule. Running in the background.
- The strings the commit reworded are asserted nowhere: `git grep 'protocol violation|call
  failed'` over the Rust tree hits only comments, the two arms themselves, and a `.expect()`
  message. Confirms the coordinator's golden-hazard grep from the Rust side too.

Note for the next developer: this harness caps a foreground command at 600s and moves anything
longer to the background, so every build and suite here is a background run polled from the
foreground, not a `timeout <n>` in front of a blocking call.

### Results, on `2f6a3e52` (which carries `a7d690e6`)

| command | result |
|---|---|
| `cargo build --features rust-only -p peacockdb-core -p peacockdb` | green, 10m33s, **0 warnings** |
| `cargo test --features rust-only -p peacockdb-core --lib -- --test-threads=4` | **437 passed, 0 failed**, 0 warnings |
| `UPDATE_CANONICAL=1 cargo test --features rust-only -p peacockdb-core --test test_plan_goldens -- --test-threads=2` | 19 passed, 0 failed |
| `UPDATE_CANONICAL=1 cargo test --features rust-only -p peacockdb-core --test test_cpu_corpus -- --test-threads=2` | **448 passed, 0 failed**, 28m20s |
| `cargo test --features rust-only -p peacockdb-core --test test_ci_coverage` | 7 passed, 0 failed |
| `cargo test --features rust-only -p peacockdb-core --test test_plan_goldens` (verify, no regen) | 19 passed, 0 failed |

`PEACOCK_REWRITE_RECIPE_BYTES` was never set, so
`the_payload_golden_carries_what_each_call_hands_the_executor` compared the committed digests
against freshly built bytes and passed.

**`git diff` is empty outside `llm-wiki/`** after both regens. The regen did write: 33 files
under `testdata/goldens/` and all ten `.plans.txt` plus `recipe-payloads.txt` have fresh mtimes
and unchanged bytes. No golden moved, which is what the reworded `RunError` arms predicted.

The three named cases all ran: `parquet_meta::tests::a_scan_over_several_files_is_refused_
rather_than_measured_from_one` (the renamed temp dir), 133 `driver::` cases, and — worth
recording — **`error.rs` has no `mod tests`**. The two reworded display arms are asserted
nowhere: `driver/tests/failure.rs` matches `RunError::CallFailed(said)` on the payload, never on
the rendered prefix. Pre-existing, and true of the old strings too, so it is not a regression;
it is why the goldens rather than a unit test are what proves this edit inert.

### The default (cudf) build cannot run in this worktree

`CUDF_ROOT=~/data/miniforge3/envs/rapids-cuda-12.2 scripts/cargo-cudf.sh build` fails in
`peacockdb-ffi`'s build script, before compiling anything of ours:

    CMake Error at CMakeLists.txt:10 (include): include could not find requested file:
      cpp/../third_party/cudf/cpp/../cmake/rapids_config.cmake

`third_party/cudf` is an empty directory here — a new worktree does not populate submodules, and
`cpp/CMakeLists.txt` bootstraps rapids-cmake out of the vendored tree even when building against
a host cuDF install. Populating it is a git operation and a developer does not do those. This
host also has no `nvidia-smi`, so `rapids_cuda_init_architectures` would have no native arch to
detect either. Every dependency below the FFI crate did compile, so the failure is the submodule
and nothing else.

What stands in for the claim: the commit's three hunks are feature-independent — `error.rs` has
no `cfg` anywhere, the `mod.rs` line is the module doc above the two
`#[cfg(not(feature = "rust-only"))]` items, and the `parquet_meta.rs` line is inside
`#[cfg(test)]`. So the cudf leg compiles the same three lines the rust-only leg just compiled
warning-free. The remaining risk is carried by CI's dataset-matrix, which builds both cuDF legs
and which `done` already waits on.

### Drift, pre-existing on master

`build-test.md:35` names the recipe-payload case `tpch_and_tpcds_recipe_payloads`. The test is
`the_payload_golden_carries_what_each_call_hands_the_executor` (`test_plan_goldens.rs:229`), and
master's copy of the file already has the new name with the old one in the page, so this branch
did not cause it.

### A second dispatch is running the same commands in this worktree, right now

Two shell trees that are not this run's are executing in `/media/data/peacockdb-refactorings`
while it works: one writing `/tmp/dmn-corpus.log` (an `UPDATE_CANONICAL=1` corpus regen that
finished 448 passed at 19:28, then `test_cpu_end_to_end` + `test_cpu_executors`), and one
writing `/tmp/verify-drop-mode-name/*.log` (lib, plan goldens, cpu corpus, cost-report,
ci-coverage, then a sweep of ten more targets). Their logs corroborate this run — 448 corpus
cases green, `test_corpus_goldens` 20 green, `test_cost_model` 3 green — and none of them was
killed.

So the two earlier dispatches did not die with their coordinator in the sense assumed here:
**the coordinator's call returned, but the shell children kept running detached.** That matters
twice. A restarted coordinator that concludes "nothing reached the branch, so the work did not
happen" is reading a report that never came back, not an absent run. And two agents regenerating
goldens into one working tree at once is a real hazard: an empty `git diff` stays a valid
result, since both write the same bytes from the same code, but a red one could not be
attributed without checking which run wrote last. Nothing was killed by this run except its own
failed cudf build; the other trees were left alone because a live agent may still be waiting on
them.

## Independent verification of `a7d690e6` — 2026-09-09 18:2x–19:3x

A second, concurrent dispatch. Its numbers agree with the section above line for line, so they
are not repeated; what follows is only what it adds.

### One finding: the tree is not rustfmt-clean at `parquet_meta.rs:262`

Shortening the temp-dir name made the `format!` fit in 99 columns, so rustfmt now wants it on
one line and the committed four-line form is hand-formatting. `coding-style.md` says to run
rustfmt over the files you touched, and that did not happen for this hunk.

    rustfmt --edition 2021 --check <copy of parquet_meta.rs>     # rc=1, one diff at :262
    rustfmt --edition 2021 --check <copy of error.rs>            # clean

The fix is rustfmt's own output:

    let dir = std::env::temp_dir().join(format!("peacockdb-multifile-{}", std::process::id()));

Not a CI failure — no workflow runs `cargo fmt`. Left unapplied because this dispatch was
verification-only; the tree is at HEAD.

**Trap while checking it:** rustfmt 1.8.0-stable **writes the file under `--check`** on this
host. It reformatted `parquet_meta.rs` in place, which showed up as an unexpected ` M` in
`git status`. Restored with `git checkout -- <path>` and re-checked on a copy in `/tmp`.
Check formatting on a copy, never in the worktree.

### The two hazards

**Golden or assertion still expecting the old text — none.** `git grep --untracked` for
`batch-partitioned protocol violation` and `batch-partitioned call failed` is empty tree-wide;
`grep -rn` over all of `testdata/` for `batch.?partition`, `protocol violation` and
`call failed` is empty. And the goldens structurally cannot carry them: every refusal line in a
`.plans.txt` is `refused: unsupported: …` from `plan_batch_partitioned` or `not runnable:
unsupported: …` from `attach_recipes`, and both return `PlanError`, whose arms were reworded in
the quarantine commit. `RunError` is run-time and reaches no golden. Outside `src/`, `RunError`
appears only in `test_cpu_end_to_end.rs`, and only its `BudgetExceeded` arm, whose `Display` is
`{message}` and did not change.

**The temp-dir rename — no collision.** Five `temp_dir()` sites in the tree, all distinct
prefixes: `peacockdb-multifile-<pid>` (this one), `peacockdb-join-fixture-<pid>-<name>`,
`peacock-cpu-source-<pid>.parquet`, `peacock-corpus-<pid>-<name>`,
`peacock-gpu-executors-<pid>.parquet`. The case ran green:
`batch_partitioned::parquet_meta::tests::a_scan_over_several_files_is_refused_rather_than_measured_from_one`.

### The gates, by hand, `LC_ALL=C.UTF-8`, `--untracked`

Gate 1 lands on the documented six; gates 2 and 3 empty; the `peacockdb-core/src`-scoped form
with survivor spellings stripped per line lands on the one mapping site,
`gpu_rowgroup_prune.rs:151`. Both of the spec's warnings reproduce exactly: `LC_ALL=C` drops
gate 1 to **three** (the arrow sites), and the line-scoped strip-and-rematch over the excluded
lines returns nothing — though there are now **166** such lines, not the 170 the spec records.

`--untracked` makes no difference at HEAD (6 either way) because nothing is untracked any more.
It mattered while the renames were in flight and stays in the command for the next slice.

### Running the dataset tiers in this worktree

`testdata/{tpch,tpcds}.sf1` do not exist here — they are gitignored and a worktree does not
carry ignored files. `PEACOCK_TESTDATA_DIR` is the wrong lever: it moves the golden root too,
and the goldens must come from this branch. Symlink the two dirs at the main checkout instead:

    ln -sfn /media/data/peacockdb/testdata/tpch.sf1  testdata/tpch.sf1
    ln -sfn /media/data/peacockdb/testdata/tpcds.sf1 testdata/tpcds.sf1

**Delete them when done.** `testdata/.gitignore`'s `/tpch.sf*/` is a directory-only pattern, so
it does not match a symlink: the two show up as `??` and a `git add -A testdata` would commit
them. They also widen what a `--untracked` gate reads.

### Why no GPU run

Nothing a device executes can see this commit. `error.rs` has no `cfg`; the `mod.rs` line is a
doc comment; the `parquet_meta.rs` line is `#[cfg(test)]` inside the lib, and `--lib` runs only
in the rust-only CPU tier (`test_ci_coverage`'s `line_runs_lib_tests` check is over that step).
No GPU target references `RunError`, and `test_gpu_corpus` reads cpu-authored sections
read-only. A device run would re-prove the goldens, which the CPU regen already proved.

## The residues are proven (2026-09-09, third run)

The narrowed dispatch came back green: 0 warnings on the rust-only build of both crates, 437 lib
cases, 19 plan-golden cases, 448 cpu-corpus cases, 7 `test_ci_coverage` cases, and a clean
re-verify of the plan goldens after the regen. `git diff` was empty outside `llm-wiki/` — the 33
golden files were rewritten with unchanged bytes, and `PEACOCK_REWRITE_RECIPE_BYTES` was never
set, so the payload digests were compared rather than overwritten. No golden moved, as the grep
predicted.

**The two earlier dispatches were never killed.** Their shells were still running detached in
this worktree, which is why nothing reached the branch: the coordinator's call returned without a
report while the work carried on. Both have since finished, both green, and they cover eleven
targets the narrowed list left out — `test_corpus_goldens` 20, `test_cost_model` 3,
`test_cpu_end_to_end` 24, `test_cpu_executors` 1, `test_golden_format` 24,
`test_layout_injection` 4, `test_null_analysis` 8, `test_planner_join_capability` 13,
`test_planner_join_refusals` 10, `test_gpu_batch` 0 under rust-only, and `cost-report`'s own 37,
which are the spec's registry guard. No processes are left running now.

The lesson is not the one the previous two restarts drew. A dispatch whose coordinator dies keeps
running and keeps writing into the shared tree, so a restarted coordinator can find a second agent
regenerating goldens beside its own. That is harmless while every diff is empty and an attribution
problem the moment one is not — check for foreign process trees in the worktree before dispatching,
not only for what reached the branch.

**The cudf leg could not be built here, for a reason now fixed.** `third_party/cudf` was an empty
directory: a new worktree does not populate submodules, and `cpp/CMakeLists.txt:10` bootstraps
rapids-cmake from the vendored tree even against a host cuDF install. Populated from the primary
checkout with `git -c protocol.file.allow=always -c submodule."third_party/cudf".url=
/media/data/peacockdb/third_party/cudf submodule update --init third_party/cudf`, so tasks 2-4 can
build both feature sets here. The cudf leg of this commit is left to CI regardless: the three hunks
carry no `cfg` and the rust-only leg compiled them warning-free.

`build-test.md:35` named the recipe-payload case `tpch_and_tpcds_recipe_payloads`; it is
`the_payload_golden_carries_what_each_call_hands_the_executor` at `test_plan_goldens.rs:229`.
Drift inherited from master, corrected here rather than left for a later pass. `cost-report`'s
`sha_links is never used` warning is on master too, at the same function.
