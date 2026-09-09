# 1 — the mode has no name any more

First of four. The legacy modes are gone, so "batch partitioned" and its `bp` abbreviation
distinguish nothing; every occurrence is a qualifier against an alternative that no longer exists.

This task changes **names only**. It renames identifiers, mode labels, golden filenames, one Python
module, one ticket page and the prose that carries them. It moves no Rust file and changes no
behaviour, which is what makes its validation absolute: every derived artifact must reproduce byte
for byte.

**`peacockdb-core/src/batch_partitioned/` is the one name that survives**, deliberately.
[`module-layout.md`](module-layout.md) places its contents into components, and doing the flattening
here would move every file twice for one outcome. Task 2 is where the last path goes.

The four tasks in order: this one, [`module-layout.md`](module-layout.md),
[`test-layout.md`](test-layout.md), [`test-support.md`](test-support.md). None may run beside the
four in [`tasks.md`](tasks.md) — rebasing across them is a whole-tree conflict.

## What changes

138 files carry the name in a path; 964 content lines outside the goldens and 169 inside.

**Mode labels lose the prefix**: `bp-tp4-sized` becomes `tp4-sized`. One table owns them, `BP_MODES`
in `tests/common/bp_mode.rs`, and `ident()` derives the macro spelling by replacing hyphens — so the
table plus a sed over `corpus_cases.inc` covers the Rust side. Then `cost-registry.csv`'s fifteen
`bp_*` column headers, `testdata/fixtures/two-row-registry.csv`'s identical headers, and the
twenty-one sites in `cost-report/src/main.rs`.

**Golden files move rather than regenerate.**

- Only `bp-mini.result.txt` carries a mode inside it — the `mode=` line, 34 sections in tpch and 60
  in tpcds. `.plans.txt`, `.cpu.txt` and `.cost.txt` carry none, so those are a `git mv` and nothing
  else.
- `bp-recipe-payloads.txt` keeps its digests. They are over the flat-buffer bytes and no mode label
  crosses the wire, so a digest that moves here means something other than a rename happened. Do
  not set `PEACOCK_REWRITE_RECIPE_BYTES`.

**Entry points**: `plan_batch_partitioned` becomes `planner::plan` and `batch_partitioned_driver`
becomes `executor::run` in task 2, when the modules they live in acquire those names. Here they
keep their names; renaming a function whose module is about to move is one edit made twice.

**`llm-wiki/tasks/bp-tickets.md` becomes `active-tickets.md`**, staying in `tasks/`. 34 references
in eleven files — `archived-tasks.md` (11), `cost-report/src/main.rs` (6), `build-test.md` (3),
`test_cpu_batch_partitioned.rs` (2), `tasks.md` (2), `casts.md` (2), and one each in
`peacockdb/src/main.rs`, `tickets.md`, `wire-schema.md`, `refcounted-tables.md`. Its fourteen
`<a id="tNN">` anchors keep their ids, so every `#tNN` link still resolves; only the filename moves.
`cost-report`'s six are load-bearing — the widget resolves ticket links against that path.

**In the archives, update the link paths and leave the prose.** `archived-tasks.md` and
`archived-tickets.md` are a record of what happened; rewriting their sentences to say "the engine"
would falsify the history they exist to hold. Paths must resolve; wording stays.

**The Python prototype renames a module.** `scripts/exec_model/batch_partitioned_driver.py` becomes
`partitioned_driver.py`, and ten files import it by name. Python has no compiler to catch a miss —
the failure is an `ImportError` at collection time, so run the prototype suite before committing.

**Test targets** rename, which reaches CI twice: `.github/workflows/pipeline.yml` and
`test_ci_coverage.rs`, whose exemption table and three GPU target lists name the binaries. That
guard fails on a miss, which is the check. `exec-model-corpus.yml` carries the phrase in a comment;
`pipeline.yml` is not the only workflow to sweep.

**Two traps.**

- **`batch` alone is a domain word.** `BatchSizing`, `Batching`, `batch_rows`, `AggregateBatches`,
  `CudfCoalesceBatches` all stay. Only the two-word phrase goes.
- **In `llm-wiki` the phrase is the mode's name, not a qualifier** — 185 hits, plus roughly ten in
  `cpp/` and `gpu_plan.fbs`. Those sentences want rewriting to say the engine; a mechanical strip
  leaves them ungrammatical and, worse, still wrong.

## Validation

The rename is inert by construction, so the bar is that every derived artifact reproduces exactly.
"It compiles and the tests pass" proves nothing here — the tests would pass over a golden that
quietly changed.

### Baselines, before the first edit

1. `sha256sum` over every file under `testdata/goldens/`, saved outside the tree.
2. `cargo test -p peacockdb-core --lib -- --list` plus one `--list` per integration target, as the
   case-name inventory. 437 lib cases, eighteen targets.
3. The warning count from a clean `cargo build` and `cargo build --features rust-only`.
4. `git rev-parse HEAD`, so a bisect has a floor.

### The checks

- **The golden hashes move in exactly two files.** After the `git mv` and the two seds, the sha256
  list must differ from the baseline only in the two `bp-mini.result.txt` files, and there only on
  `mode=` lines. Any other hash change means the rename touched content it should not have.
- **Then regenerate anyway, and require an empty diff.** `UPDATE_CANONICAL=1` over
  `test_batch_partitioned_plans` and the corpus cpu tier rewrites every golden from a live run;
  `git diff` after it must be empty. The hash check says the files did not move; this says the
  engine still produces them.
- **The payload digests are the sharpest instrument.** Regenerate with
  `PEACOCK_REWRITE_RECIPE_BYTES=1` and the fixed `/tmp` testdata symlink; an empty diff means the
  flat-buffer bytes are unchanged down to statement order. A digest that moves during a rename means
  the run must stop, not that the new digest gets committed.
- **The case inventory maps under one transformation.** Every `--list` name must map to a baseline
  name by removing a `bp_` or `bp-` prefix, and per-target counts must match. A vanished case is a
  `#[test]` lost to a bad sed; a new one is a duplicated module.
- **`cost-report`'s own tests are the registry guard.** They read both `cost-registry.csv` and
  `testdata/fixtures/two-row-registry.csv`, so a header renamed in one file and not the other fails
  there rather than in a later task.
- **The exec-model suite runs before the commit**, not after. Its 216 cases are the only check on
  the Python module rename, and an `ImportError` there is silent until collection.
- **Grep gates, all empty**: `git grep -inE 'batch.partition' -- ':!llm-wiki' ':!peacockdb-core/src'`,
  `git grep -nE '\bbp[-_]' -- ':!peacockdb-core/src'`, and `git grep -n 'bp-tickets'`. The two
  exclusions are the src tree, whose directory name task 2 removes, and `llm-wiki`, whose prose is
  rewritten by hand rather than swept.
- **`test_ci_coverage` passes**, so a missed target rename fails there rather than silently
  un-gating a tier.

### Done when

Every golden regenerates to an empty diff and the payload digests are byte-identical; the case
inventory maps by prefix removal with no count changing; `cost-report` and the exec-model suite are
green; the grep gates are empty; and CI is green with no new warnings against the recorded count.
