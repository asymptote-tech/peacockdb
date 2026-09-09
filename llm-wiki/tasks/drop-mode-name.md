# 1 — the mode has no name any more

Kind: production

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

138 files carry the name in a path; 964 content lines outside the goldens and 169 inside. (The
gate below also lands on 138 — a coincidence, not the same 138.)

`BpMode`, `BP_MODES` and `bp_mode.rs` become `Mode`, `MODES` and `mode.rs`. The lowercase gate
catches the filename but not the two identifiers, and a `BpMode` in a tree with no `bp` anywhere
else is the qualifier-against-nothing this task exists to remove.

**Mode labels lose the prefix**: `bp-tp4-sized` becomes `tp4-sized`. One table owns them, `MODES`
in `tests/common/mode.rs` (`BP_MODES` in `bp_mode.rs` before the rename above), and `ident()` derives the macro spelling by replacing hyphens — so the
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
`test_cpu_end_to_end.rs` (2), `tasks.md` (2), `casts.md` (2), and one each in
`peacockdb/src/main.rs`, `tickets.md`, `wire-schema.md`, `refcounted-tables.md`. Its fourteen
`<a id="tNN">` anchors keep their ids, so every `#tNN` link still resolves; only the filename moves.
`cost-report`'s six are load-bearing — the widget resolves ticket links against that path.

**In the archives, update the link paths and leave the prose.** `archived-tasks.md` and
`archived-tickets.md` are a record of what happened; rewriting their sentences to say "the engine"
would falsify the history they exist to hold. Paths must resolve; wording stays.

**The Python prototype renames a module.** `scripts/exec_model/batch_partitioned_driver.py` becomes
`partitioned_driver.py`, and ten files import it by name. Python has no compiler to catch a miss —
the failure is an `ImportError` at collection time, so run the prototype suite before committing.

**Test targets** rename, each to what `build-test.md` already calls its tier:

| was | becomes | the tier it is |
|---|---|---|
| `test_batch_partitioned_injection` | `test_layout_injection` | Layout injection mechanism |
| `test_batch_partitioned_plans` | `test_plan_goldens` | Plan goldens — and it sits beside `test_corpus_goldens` |
| `test_cpu_batch_partitioned` | `test_cpu_end_to_end` | end to end: SQL in, rows out; its macro is already `end_to_end!` |
| `test_cpu_bp_corpus` | `test_cpu_corpus` | |
| `test_gpu_bp_corpus` | `test_gpu_corpus` | |

`test_inc2_conformance` is not renamed here — [`test-layout.md`](test-layout.md) makes it
`test_murmur_conformance` when it moves in-crate, and renaming it twice is one edit made twice.

This reaches CI twice: `.github/workflows/pipeline.yml` and
`test_ci_coverage.rs`, whose exemption table and three GPU target lists name the binaries. That
guard fails on a miss, which is the check. `exec-model-corpus.yml` carries the phrase in a comment;
`pipeline.yml` is not the only workflow to sweep.

**Four traps.**

- **`batch` alone is a domain word.** `BatchSizing`, `Batching`, `batch_rows`, `AggregateBatches`,
  `CudfCoalesceBatches` all stay. Only the two-word phrase goes.
- **The two words are not always adjacent.** `batch_single_partition_driver.py` carries the same
  qualifier with `single_` between them, so a `batch.partition` regex misses it; it becomes
  `single_partition_driver.py`, with its function and class. Left alone it would have been the only
  `batch` qualifier surviving in `scripts/`.
- **`batch→partition` is not the mode name.** Four sites — `gpu_plan.fbs:312` and `:346`,
  `node_session.cpp:220`, `gpu_rowgroup_prune.rs:151` — describe the row-group→batch→partition
  *mapping*, which is a real three-level structure and stays. A regex with `.` between the words
  matches the arrow, so a careless sweep mangles them.
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
  `test_plan_goldens` and the corpus cpu tier rewrites every golden from a live run;
  `git diff` after it must be empty. The hash check says the files did not move; this says the
  engine still produces them.
- **The refusal message is the one exception, and it is quarantined.** `error.rs:20` renders
  `"unsupported in batch-partitioned mode: {what}"` and `translate/mod.rs:221` says "do not plan in
  batch-partitioned mode (#143)" — and those strings land in **75 lines across the ten
  `.plans.txt` goldens**. Reword them in their own commit, last: everything before it must
  regenerate to an empty diff, and that commit's regeneration must produce a diff of exactly those
  75 lines and nothing else. Same shape as task 2's `GpuHashJoin` quarantine, and the same reason —
  it separates the strong check from the one known change. The wording is the developer's, under
  two constraints: it must not say "mode", and the two sites must agree.
- **The payload digests are the sharpest instrument, and the plain regen is what reads them.**
  Under `UPDATE_CANONICAL=1` alone, `test_plan_goldens` compares the committed digests against the
  bytes it just built and fails naming the file. Setting `PEACOCK_REWRITE_RECIPE_BYTES=1` makes it
  rewrite instead of compare, which is the instrument switched off — so never set it here. A
  digest that moves during a rename means the run stops, not that the new digest gets committed.
  (`module-layout.md`'s quarantine reads this same paragraph.)
- **The case inventory maps under one transformation.** Every `--list` name must map to a baseline
  name by removing a `bp_` or `bp-` prefix, and per-target counts must match. A vanished case is a
  `#[test]` lost to a bad sed; a new one is a duplicated module.
- **`cost-report`'s own tests are the registry guard.** They read both `cost-registry.csv` and
  `testdata/fixtures/two-row-registry.csv`, so a header renamed in one file and not the other fails
  there rather than in a later task.
- **The exec-model suite runs before the commit**, not after. Its 216 cases are the only check on
  the Python module rename, and an `ImportError` there is silent until collection.
- **The prose-and-labels gate.** The naive form cannot be empty, because this task deliberately
  keeps `src/batch_partitioned/` and deliberately does not rename `plan_batch_partitioned` or
  `batch_partitioned_driver` — 132 lines outside `peacockdb-core/src` name the module path (128 in
  `peacockdb-core/tests/**`, 4 in `peacockdb/src/main.rs`) and 42 more name those two functions. All
  of it is the residue task 2 removes. So the gate excludes the four spellings that survive:

  ```
  git grep -inE --untracked 'batch.?partition' -- ':!llm-wiki' ':!peacockdb-core/src' \
    | grep -vE 'mod batch_partitioned|batch_partitioned/|batch_partitioned::|::batch_partitioned|plan_batch_partitioned|batch_partitioned_driver'
  ```

  **The pattern is `batch.?partition`, case-insensitive, and both halves of that are load-bearing.**
  `.?` rather than `.` catches `BatchPartitionedDriver`, which a separator-requiring pattern cannot
  see at all — that is how a class survived its own module's rename. And the exclusions are the
  four surviving spellings written out rather than a bare `batch_partitioned`, which would swallow
  every underscore residue including `--test test_batch_partitioned_plans` and
  `mod test_batch_partitioned_injection` — the target-rename miss the gate exists to catch.

  **Run it in a UTF-8 locale.** `→` is three bytes, and under `LC_ALL=C` a `.` matches one byte, so
  the pattern cannot span the arrow and the three mapping sites drop out — the gate reports three
  and reads as cleaner rather than as half-blind. The residue half is unaffected either way, since
  CamelCase needs zero characters and every separator form is one byte, so a C-locale run is safe
  but its count is not the documented one. `build-test.md` sends people to `LC_ALL=C` for
  cross-host comparison, so this is a real way to meet it.

  The goldens are deliberately **not** excluded. They carry no hits once the refusal is reworded,
  so excluding them buys nothing today — and they are exactly where that wording lands, so a gate
  blind to them could not catch it coming back.

  **The exclusion is line-scoped, which is the one hole left in it.** `grep -vE` drops the whole
  line, so a deliberate survivor anywhere on a line shields real residue sharing it —
  `mod batch_partitioned; // renamed from test_batch_partitioned_plans` is invisible to the gate.
  Latent rather than live today: stripping the six survivor spellings from every excluded line and
  re-matching returns nothing. But 170 lines carry a survivor spelling, and the later tasks move
  the tree those lines live in, so re-run that strip-and-rematch after each slice rather than
  assuming it still holds.

  At the finish it lands on **six**, all deliberate: the three `batch→partition` mapping sites,
  `README.md:371` and `source.py:3` naming `ParquetBatchPartitioner` (the same structure, not the
  mode), and `test_ci_coverage.rs:431`, which is task 2 residue and goes when the module does.

  Then `git grep -nE --untracked '\bbp[-_]' -- ':!peacockdb-core/src' ':!llm-wiki'` and
  `git grep -n --untracked 'bp-tickets' -- ':!llm-wiki'`, both empty. `llm-wiki` is excluded whole
  because it is rewritten by hand rather than swept, and two parts of it keep `bp` on purpose: the
  archive's 21 mode labels, which record what the modes were called at the time and would be
  falsified by an edit, and these four task specs, which have to quote both spellings to say what
  becomes what.

  **`--untracked` is not optional, and it is the trap that would have shipped residue.** `git grep`
  does not see untracked files, so every gate is blind to exactly the 42 files this task renames —
  they are `??` until staged. Run without it and the gate goes **green over the renamed files
  themselves**: the first pass here left the phrase in `mode.rs`'s module doc and `mode_named`
  panic, and in two renamed targets' module docs, with gate 1 reporting clean. The same shape bit a
  `git grep -l` sed list, and that one at least went red in the Python suite. This one would not
  have.
- **`test_ci_coverage` passes**, so a missed target rename fails there rather than silently
  un-gating a tier.

### What a device run does and does not add

A renamed golden that `mode.rs` computes a different path for fails **rust-only, before any
device**: `test_corpus_goldens` opens `cpu_golden()`, `cost_golden()` and `result_golden()` by
computed path with `.expect(...)`, and `test_cost_model` sweeps the same directory. So the device
tier is not the first reader of those filenames, and a run that claims to be proving them is
claiming the wrong thing.

Nor is "the sections resolve" the whole of it: `test_corpus_goldens` already checks committed
sections it did not write, against their own arithmetic, with no run at all. What the device run
uniquely adds is those sections checked against **a second engine's actual run** — plan shape,
`in_rows`, the per-batch lists, the bytes. That is the claim that stays true in tasks 2 and 3, and
"only the device can see this" is an easy thing to say about a filename when it is true only of a
comparison between engines.

### Done when

Every golden regenerates to an empty diff and the payload digests are byte-identical; the case
inventory maps by prefix removal with no count changing; `cost-report` and the exec-model suite are
green; the grep gates are empty; and CI is green with no new warnings against the recorded count.
