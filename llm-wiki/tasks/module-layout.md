# 2 — a component's API is its mod.rs

Kind: production

Second of four, after [`drop-mode-name.md`](drop-mode-name.md) and before
[`test-layout.md`](test-layout.md) and [`test-support.md`](test-support.md). None may run beside the
four in [`tasks.md`](tasks.md) — rebasing across them is a whole-tree conflict.

`peacockdb-core/src/batch_partitioned/**` moves to `peacockdb-core/src/`, laid out as components
whose whole API is declared in `mod.rs` with implementation behind private modules. The previous
task deliberately left this directory alone so that every file moves once, here, rather than twice.

Today 170 top-level items are `pub` and eight of them are named by another crate. This task does not
change that — it changes where they are declared and what may reach past them.
`plan_batch_partitioned` becomes `planner::plan` and `batch_partitioned_driver` becomes
`executor::run` as their modules acquire those names; `peacockdb/src/main.rs` moves with them.

### Five components

The IR is neither planner nor executor: it is what passes between them, and eleven non-test
consumers span both halves. Giving it to either makes that half's internals a dependency of the
other.

```
src/lib.rs
src/common.rs              the row-byte formula, today memory.rs
src/plan/                  what a plan is
src/wire/                  what crosses the FFI
src/planner/               making both
src/executor/              running them
src/plan_text/             rendering any of it
```

`wire/` is separate from `plan/` because they are two contracts, not one. A plan is the tree the
planner builds and the driver walks; a recipe is the menu of parameterized kernels that crosses to
the C++ side, and `architecture.md` already treats it as its own subject under that name.

**It holds everything that knows what a flat buffer looks like** — the vocabulary (`Recipe`, `Seq`,
`FbKind`, `Call`, `CallPattern`, `Input`, `ProjectRole`, `AbiSymbol`, `RecipePlan`), the writers
that are `recipe/` today, the reader, the two renderers that are `plan_text/fb_text.rs` and
`plan_text/recipes.rs` today, and the generated module.

That last one forces the shape. Eleven files name `crate::generated` — nine in `recipe/`, plus both
of those renderers — and they use **73 distinct generated types** between them, so flatc's output
cannot be walled off unless its callers are inside the same wall. `plan_text/recipes.rs` belongs
with them because it parses the buffer itself (`flatbuffers::root_with_opts::<fb::GpuPlan>`); it
takes `render_plan_recipes` and `Payloads` with it, and `plan_text/mod.rs` asks `wire` for the
`--- recipes ---` section. The memory section is not the same case: `MemoryModel` is a plain struct
the renderer can walk without knowing the wire format.

With all eleven inside, `wire/mod.rs` exposes `Recipe`, `Seq`, `attach_recipes`, `payload_text` and
`render_plan_recipes`, and the 7,336 lines flatc emits are private to one component.

**Keep the `#[allow(unused_imports, dead_code, clippy::all)]`** that sits on the module today, and
say why in a comment. It is cosmetic while the module is `pub` — everything is externally reachable,
so `dead_code` cannot fire. Private, every generated type the crate does not name becomes dead code,
and there are hundreds.

`planner` therefore has no `recipes` subcomponent: writing a recipe is a fact about the wire format,
not about planning, and `attach_recipes` walks a finished tree.

`plan_text` is a peer because it renders a plan tree, a run report, the memory model and the
recipes, and because it holds the one real edge into the driver — `run_text.rs` reads `RunReport`
and `PlanIndex`. It sits above both.

### Where everything goes

| Today | Goes to | Because |
|---|---|---|
| `nodes/` | `plan/` | eleven non-test consumers across both halves |
| `node.rs` | `plan/mod.rs` | `GpuNode`, `RowInterval` — the trait the nodes implement |
| `layout.rs`, `schema.rs`, `expr.rs`, `aggregates.rs` | `plan/`, as implementation modules | one vocabulary, not four. As subcomponents every other component would have to reach through their walls to name `Expr` or `Schema`; declared in `plan/mod.rs` they are one facade |
| `validate.rs` | `plan/` | structural properties of a tree, not of planning; also removes the one executor-to-planner edge, `driver/partitioned.rs:28` |
| `error.rs` | split | `PlanError` to `plan/mod.rs`, `RunError` and `When` to `executor/mod.rs` |
| `RowGroupMeta`, `Batching`, `ScanMetadata` | `plan/` | `GpuLoadParquet` stores the mapping verbatim |
| `recipe/` entire, `plan_text/{fb_text,recipes}.rs`, and `lib.rs`'s `generated` | `wire/` | the eleven files that name `crate::generated` use 73 of its types; it cannot be private unless they are inside with it |
| `partitioner.rs`, `parquet_meta.rs`, `gpu_rowgroup_prune.rs` | `planner/translator/scan_mapping/` | all three entry points have one caller, and all three calls are inside `Translator::source`, a 20-line function (`translate/mod.rs:449,451,458`). As a peer of `translator` it would be the design's only sibling-subcomponent edge |
| `MemoryModel` | `planner/mod.rs` | already half of what `plan()` returns |
| `RunReport`, `PlanIndex`, `ROOT` | `executor/mod.rs` | what running a plan produces, plus the post-order addressing the recipes and the FFI share — not private driver bookkeeping |
| `expr_translate.rs` | `planner/translator/` | sole consumer is `translate/` |
| `nulls.rs` | `planner/` | a refusal the planner makes; sole caller `plan.rs` |
| `estimator.rs` | `planner/memory_estimation/` | as proposed |
| `executor.rs`, `backend.rs` | `executor/mod.rs` | the seven category traits and `Backend`; their cycle disappears when they share a file |
| `batch.rs`, `cpu_batch.rs`, `gpu_batch.rs` | `executor/` | `Batch` and the two implementations |
| the three `#[cfg(not(feature = "rust-only"))]` declarations in `batch_partitioned/mod.rs` | `executor/mod.rs` | `gpu_backend`, `gpu_batch` and `GpuBatch` — see below |
| `forwarder.rs` | `executor/` | routing with no backend and no executor — the driver owns it |
| `expr_physical.rs`, `spark_partitioning.rs` | `executor/cpu_backend/` | one consumer each |
| `driver/accounting.rs` | stays under `executor/driver/` | see below |
| `memory.rs` | `src/common.rs` | four consumers across planner, executor and both backends |
| `config.rs` | deleted | see below |

Result:

```
plan/mod.rs           GpuNode, RowInterval, PlanError, the eighteen nodes,
                      Expr, Schema, NodeKind, PartitionLayout, AggFunc, PlanAgg,
                      RowGroupMeta, Batching, ScanMetadata,
                      NodeRef, ExecutorCategory, category_of, node_name
plan/common.rs        check_column_refs, check_merge_keys, input_layout,
                      input_schema, rebase_through_projection
plan/{exec_ops,accumulators,aggregate,join,partition_ops,source,union,unload}.rs
plan/{expr,layout,aggregates}.rs
plan/validate.rs

wire/mod.rs           Recipe, Seq, FbKind, Call, CallPattern, Input, ProjectRole,
                      AbiSymbol, RecipePlan, Payloads, attach_recipes,
                      render_plan_recipes, payload_text, node_at, depth,
                      check_seq_kinds
wire/generated.rs     private — the include!, 7,336 flatc lines, 73 named types
wire/{node_writer,join,aggregate_writer,expr_writer,writer,read,
      fb_text,recipes}.rs

planner/mod.rs        plan(), PlanKnobs, BatchSizing, SMALL_TABLE_BYTES, MemoryModel
planner/nulls.rs
planner/{translator,memory_estimation}/
planner/translator/scan_mapping/

executor/mod.rs       Backend, the seven traits, Batch, CpuBatch, GpuBatch,
                      RunError, When, CallStats, RowRange, BatchForwarder,
                      RunReport, PlanIndex, ROOT
executor/{cpu_batch,gpu_batch,forwarder}.rs
executor/{driver,cpu_backend,gpu_backend}/

plan_text/
```

`plan/mod.rs` lands near 980 lines, measured: 561 of declarations with their doc comments, 120 for
the registry and its four exhaustive 18-arm matches, 272 of one-line delegations over 32 inherent
blocks, and the module header. It is the crate's central index and the one file a newcomer should
read, and it holds no logic — the one-expression rule is what keeps it a header rather than a
module. `wire/mod.rs` is about 300 — the vocabulary plus five delegating entry points. Nothing else
passes 400.

### A component whose API is conditionally present

`executor/` is the one component that does not have a fixed surface: `gpu_backend`, `gpu_batch` and
`GpuBatch` are `#[cfg(not(feature = "rust-only"))]` today, and the declarations move with them. So
`executor/mod.rs` carries the cfg on the declarations themselves, not only on the `mod` lines —
`GpuBatch`, `GpuBackend` and `GpuContext` exist in two of the three feature shapes and not the
third.

Two consequences. The layout test must read the cfg rather than the item, or it will report a
missing declaration under `rust-only` and a surplus one otherwise. And the `use common;` line at the
end of `executor/mod.rs` must not pull in anything device-only, since `common.rs` is compiled in
every shape — the rust-only tier boundary is exactly what a shared `common.rs` is able to breach.

### Why accounting is not its own subcomponent

`ResidentAccountant` is a field on the partitioned driver and is passed by mutable reference into
the lane state machine at four sites in `single_partition.rs`. `Held<T>` is the driver's in-flight
batch representation, `Slot` an index into its executor slots, `Trip` a `StepError` variant. Four of
the five types are driver internals; only `Underestimate` faces outward, through `RunReport`.

A sibling subcomponent would have to declare all four as API. As `executor/driver/accounting.rs`,
a private implementation module, they stay `pub(crate)` behind the driver's wall and nothing outside
`driver` can name them.

`ResidentAccountant` is the name of the thing. Retire "the enforcer" and "resident enforcer" as
second spellings for it, in the wiki and anywhere else — the type both accounts and enforces, and
enforcement is the smaller half.

### config.rs is dismantled

`MemoryLimit` moves to `src/test_support/`, which [`test-layout.md`](test-layout.md) creates for the
corpus harness that reads it; until that task lands it sits in `tests/common/` beside `bp_mode.rs`. `TargetPartitions`, `TARGET_PARTITIONS`
and `BATCH_STRESS_BUDGET` are named by nothing outside the file's own unit test and go. The module
doc describes `tp8-standard` device labels, which the legacy-mode drop retired.

## The visibility rules

For `coding-style.md`, replacing nothing that is there today.

- A component or subcomponent is a directory with `mod.rs`. Its whole API — structs, enums, traits,
  functions, constants — is declared there. Nowhere else in the component carries `pub` or
  `pub mod`.
- Implementation modules are declared `mod x;` and their items are `pub(crate)`. The module's own
  privacy is the boundary: a path through a private module is refused whatever the item says, so
  `plan::exec_ops` cannot be named from outside `plan` and `pub(super)` is not needed.
- **A subcomponent is declared `mod`, not `pub mod`** — `mod recipes;` in `planner/mod.rs`, never
  `pub mod recipes;`. `pub mod` would make `planner::recipes::Recipe` nameable crate-wide and the
  subcomponent wall would exist only on paper. What a sibling component needs is declared in the
  component's own `mod.rs`; that is what the four type moves in the table above are for.
- `lib.rs` declares the components `pub mod`, and they are the only `pub mod` in the crate.
- **Nesting may go three deep** where the innermost earns it: `planner/translator/scan_mapping/` is
  720 lines behind three entry points. The same rule applies at each level — `mod`, not `pub mod`.
  A directory with a one-item facade and a hundred lines behind it is an implementation module
  wearing a directory; the test is whether the body justifies the wall.
- `pub use` is not allowed. Inline the declaration into `mod.rs`, or into `common.rs` for what the
  implementation modules share. A child reaches into its parent; a parent never re-exports a child.
- A body in `mod.rs` is one expression. Declarations, and delegations of exactly one line.
- A struct keeps its inherent `impl`, and that block lives in `mod.rs` with one-line bodies. A trait
  is for two or more implementors. A trait per struct would also break every `const fn` and
  associated const, which trait items cannot be.
- An implementation module may implement any trait for a type declared in its own component's
  `mod.rs`, and may define free functions the `mod.rs` delegates to. It may not declare types or
  traits that form the component's API.
- Absolute `crate::` paths across a component boundary, `super::` only within one.
- `mod.rs` and `common.rs` have no length limit for this task. A limit is set afterwards from what
  they weigh.

### What that buys, exactly

Three claims, and only the first two are the compiler's.

- **A component is reachable only through its `mod.rs`.** Enforced: every implementation module is
  private, so naming one from outside is `E0603: module is private`.
- **A subcomponent is reachable only through its own `mod.rs`, and only from inside its parent
  component.** Enforced by the same mechanism, once the declaration is `mod` rather than `pub mod`.
- **Only the parent component's own code may use a subcomponent.** Enforced *across* components.
  Not enforced *within* one: Rust's rule is "the module and its descendants", and sibling
  subcomponents are descendants of the parent. There is no visibility level meaning "my parent but
  not my siblings" — `pub(super)`, `pub(crate)` and `pub(in path)` all give the same set.

**The layout as placed has no sibling-subcomponent edge left.** A sweep for calls from one
subcomponent into another found exactly one, `translator` into `scan_mapping`, and the answer was
that `scan_mapping` was misplaced rather than that the rule needed an exception. `driver` never
calls a backend — it is generic over `Backend` — and `memory_estimation` takes only types.

So two things fall to the layout test: sibling reach between implementation modules, which is the
same gap one level down, and where a `pub` appears at all, which nothing in rustc checks. Both are
readable from the tree, in the idiom `test_ci_coverage.rs` already uses. Without that test none of
these rules can go red.

## What the visibility sweep finds

170 top-level `pub` items in `src`, plus 142 `pub` methods and associated consts.

- **8** are named by another crate, and all eight by one file — `peacockdb/src/main.rs`, the CLI,
  which is the only workspace member that depends on `peacockdb-core` at all. They are
  `build_session_state`, `register_tables_for`, `plan_batch_partitioned`, `PlanKnobs`,
  `BatchSizing`, `SMALL_TABLE_BYTES`, `CpuBackend` and `batch_partitioned_driver`. That is the
  crate's real API: register tables, plan a query, run it on a backend.
- **108** are `pub` only because `peacockdb-core/tests/*.rs` are separate crates.
- **54** are named by nothing outside the crate and lose `pub` — among them `Batching`, `ColumnRef`,
  `SortOrder`, `UnaryOp`, `Translator`, `MemoryModel`, `JoinCapability`, `Forwarder`,
  `logical_size_from_schema`, `estimate`, `partition`, `translate_expr`. Eleven of the 54 are used
  only inside their own file and lose `pub` entirely: `Decomposition`, `EmittedBatch`,
  `SourceEstimate`, `StateFunc`, `all_row_groups`, `position_of`, `wire_nodes`, and the four
  `config.rs` items that are being deleted.
- 85 `pub(super)` become `pub(crate)` once the enclosing `pub mod` loses its `pub`.
- 30 `pub use` are inlined.

One leak the item count does not show, because it is a module rather than an item:
`pub mod generated` in `lib.rs` exports the whole flatc surface — 7,336 lines, 73 types named
internally and every other one reachable. It becomes `mod generated;` private to `wire/`.

The 108 stay `pub` **in this task**, and [`test-layout.md`](test-layout.md) removes them by moving
the eleven targets that force them down into `src/`. Doing that here would mean one diff in which a
layout mistake and a coverage regression look alike, so it waits — and by then every one of those
imports has already been rewritten, which is most of the work.

## Renames that fall out

- **The node `GpuJoin` becomes `GpuHashJoin`.** It is the equi-join, it serializes to
  `CudfHashJoin`, and it sits beside `GpuCrossJoin` and `GpuNestedLoopJoin` — one of three
  unqualified for no reason. The executors keep the category names `CpuJoin` and `GpuJoin`, which is
  the scheme every sibling follows and is accurate: both run all three join nodes.
  13,246 golden lines carry the old name. It is display text, so the recipe digests do not move;
  sed, then regenerate to confirm the sed rather than to author it.
- **`memory.rs` becomes `common.rs`.** It is a byte formula, not memory management, and it collides
  with `plan_text/memory.rs`, which renders the `--- memory ---` section.
- **`gpu_rowgroup_prune` loses its `gpu_`.** It runs on the CPU and serves both backends.

**`CpuUnload` and `GpuExport` stay as they are.** They are one thing under two names, differing by
backend, which the style guide's "the same thing carries the same name everywhere" reads against —
but neither collides with anything, and any fix costs more than it buys: `GpuUnload` is taken by the
plan node, and renaming `CpuUnload` to match `GpuExport` moves a name nobody is confused by. Noted
as a considered keep so the next reader does not re-derive it.

## Tests

The four tiers already exist and none of them moves.

- **Module unit** — `#[cfg(test)] mod tests { … }` inline in an implementation module. 13 sites,
  unaffected.
- **Component and subcomponent** — `#[cfg(test)] mod tests;` declared beside the implementation
  modules, files under `component/tests/`. 11 sites. Being a descendant of the component they see
  its private items but not an implementation module's, which is the boundary respected.
  `nodes/tests/`, `cpu_backend/tests/`, `driver/tests/`, `translate/tests.rs` and `recipe/tests.rs`
  are already this.
- **Crate integration** — `tests/*.rs`, held to the component API. Eleven files name an
  implementation module today: `cpu_backend::{accumulate,emit,join,source,backend}`,
  `gpu_backend::{accumulate,emit,join,backend}`, `nodes::{aggregate,join}`. Every type they reach is
  legitimately component API, so this is an import rewrite — `cpu_backend::CpuAccumulator` — and no
  test relocates.

`test_cpu_executors` and `test_gpu_executors` construct backend executors directly, so the executor
constructors stay public for tests. That is the executor contract and is fine; list them in the
component `mod.rs` deliberately rather than by accident.

Eleven of the eighteen targets then move down into `src/` in [`test-layout.md`](test-layout.md),
which is also where test code stops sharing a file with production code. Nothing here should be
written to make that harder — in particular, do not fold a test helper into a production module to
shorten an import.

## The wiki this moves

`architecture.md` is not prose about the code — it is prose *anchored to* the code, and the anchors
move. 29 of its lines carry a Rust path or module name: 37 path references in total, some as
markdown links to `../peacockdb-core/src/batch_partitioned/recipe/*.rs`, most as bare
`node_writer.rs` / `join.rs` / `scheduler.rs` in running text. Seven are in the wire-format section
alone, which is also where the largest move lands.

Correcting them is this task's, not a later cleanup's — `prompts.md` makes keeping those two pages
true the same commit's duty as the change. The work is four kinds, and only the first is mechanical.

- **Paths in links and backticks**: rewrite to the new component. `batch_partitioned/recipe/` becomes
  `wire/`, `batch_partitioned/translate/` becomes `planner/translator/`, `batch_partitioned/nodes/`
  becomes `plan/`, and the crate-root files land where the placement table says.
- **Directory names in prose**, which read as facts rather than links: "the recipe writer
  (`batch_partitioned/recipe/`)", "the types are in `batch_partitioned/`", "`driver/`,
  `partitioned.rs` owns the tree". These do not grep the same way as the links and have to be read
  for.
- **Sentences the reorganization falsifies**, which carry no path at all. "A `Gpu` name with no
  `Cudf` is one of this mode's own plan nodes" survives; the Execution section's "the types are in
  `batch_partitioned/` and the code is what they are" needs the new home; and the whole framing of
  the mode as *a* mode rather than *the* engine is the phase-1 rewrite, 185 hits across `llm-wiki`.
- **`coding-style.md` gains the visibility rules** — the section drafted above goes in whole, and
  its Small-files bullet's examples (`batch_partitioned/nodes/`, `batch_partitioned/cpu_backend/`)
  become the new paths. Its Names section loses the `test_inc2_conformance` exception paragraph in
  the next task, not this one.

`build-test.md` carries 19 such lines; correct the paths here and leave its test table alone —
[`test-layout.md`](test-layout.md) restructures it, and doing it twice means doing it wrong once.

## Validation

Nothing here may change what the engine computes, so the bar is the opposite of the previous task's:
**no golden may move at all**, and the one exception is quarantined in its own commit.

### Baselines, before the first move

1. `sha256sum` over `testdata/goldens/`.
2. `cargo test -p peacockdb-core --lib -- --list` and one `--list` per integration target. Tests do
   not move in this task, so every one of these must come back byte-identical at the end.
3. A dump of every `pub` and `pub(crate)` item with its declaring file — the visibility baseline the
   sweep is compared against.
4. Warning counts from clean builds in all three feature shapes.

### The `GpuHashJoin` commit is quarantined

It goes first, alone, and it is the only commit in the task whose diff touches `testdata/goldens/`.
Sed the 13,246 lines, then regenerate and require an empty diff — the regeneration confirms the sed
rather than authoring it. **Plain `UPDATE_CANONICAL=1`, never with `PEACOCK_REWRITE_RECIPE_BYTES`:**
without the second variable `test_plan_goldens` compares the committed payload digests against the
bytes it just built and fails naming the file; with it, it rewrites them, and the digest agrees
with itself having proved nothing. **Every later commit must show zero golden changes in `git diff --stat`,**
and that is the single most valuable check in the task: a golden that moves after this point means
the layout changed behaviour.

### One commit per component

In this order: `plan_text` and `executor/driver` first, because they are already close to the target
shape and prove the pattern cheaply; then `wire`, which is the largest single move and the one that
makes `generated` private; then `plan`; then `planner`; then the backends. After each, run the lib
unit tests plus `test_plan_goldens` — the cheap tier — so a break is localized to the
component that caused it rather than found at the end across a 138-file diff.

### Per-commit checks

- **The case inventory is byte-identical.** Tests do not move here, so any `--list` difference is a
  test that stopped compiling into its target — the most likely silent failure in the whole task.
- **Three builds, not one**: `--features rust-only`, default, and the C++-linked build. The
  rust-only tier boundary is the invariant most easily broken by a move, because pulling one type
  into a shared `mod.rs` or `common.rs` can drag an FFI type into a rust-only path, and it fails at
  link rather than at review. `executor/` is the component to watch: its API is conditionally
  present, so its `common.rs` compiles in every shape while `GpuBatch` does not.
- **The visibility sweep is mechanical, not a reading.** A script enumerates every `pub` and
  `pub(crate)` with its declaring file and asserts: no bare `pub` or `pub mod` outside a `mod.rs`;
  no `pub use`; no `pub(super)`; subcomponents declared `mod`, not `pub mod`; and the item set
  unchanged from the baseline, since this task moves declarations and does not remove any. Run it
  at every component commit.
- **Re-run task 1's strip-and-rematch after each slice.** Its residue gate excludes by line, not
  by match, so a survivor spelling anywhere on a line hides real residue sharing it. That was
  latent when task 1 closed; this task moves the files those 170 lines live in, which is exactly
  the motion that turns it live.
- **Drop the gate's `':!peacockdb-core/src'` exclusion once the directory is gone.** It exists only
  to spare `src/batch_partitioned/`, and this task removes that name. Left in place it hides the
  whole crate: task 1's completeness pass found four residues inside that tree precisely because
  nothing read it, and after this move the exclusion would blind the gate to everything the task
  touched. Run the gate without it, and expect the three mapping sites plus whatever `README.md`
  and `source.py` still say about `ParquetBatchPartitioner`.
- **Every grep in this task takes `--untracked`.** `git grep` does not see untracked files, so a
  sweep run before staging is blind to exactly the files being moved — in task 1 a gate reported
  clean while residue sat in four renamed files. This task moves every file in the crate, so the
  blindness is total until each slice is staged. Run the sweeps after `git add`, or with
  `--untracked`, and never before a move.
- **`git diff -M --summary` reports renames**, not delete-plus-add. A file reported as both changed
  more than half its content, which a path rewrite and an import fix should not do.

### The layout test must be seen red

For each rule it claims — a `pub` outside a `mod.rs`, a `pub mod` subcomponent, a `pub use`, a
sibling implementation module reaching another, a `crate::`-less cross-component path — construct
the violation, watch it fail, revert. A guard nobody has seen fail is a guard nobody knows is wired
up, and `test_ci_coverage.rs` is the worked example of doing this properly in this repo.

### Then the full suite, once

Per `coding-style.md` a behaviour-preserving refactor is verified with a representative case per
mode per binary plus the golden and meta tier; the full corpus runs once, at the end, on verda.
Check what a package-wide command actually sweeps before running it — `--features rust-only` selects
a build, not a tier.

### If a golden moves

It is not a golden to regenerate. It means the move changed behaviour, and the diff names where: a
plan line is the planner, a `--- memory ---` figure is the estimator, a payload digest is the recipe
writer. Bisect by component commit — that is what the one-commit-per-component rule buys.

## Done when

The crate builds clean under all three feature shapes with no new warnings against the recorded
count; every golden after the `GpuHashJoin` commit is untouched; the case inventory is
byte-identical; bare `pub` and `pub mod` appear only in `mod.rs` files, with no `pub use` and no
`pub(super)` anywhere in `src`; the layout test exists and has been seen red on each rule; the
visibility rules are in `coding-style.md` and `architecture.md`'s paths are correct; and CI is green.

## Completeness signoff

Solved under its constraints, with three named shortcuts and no bandaids.

1. "bare `pub` and `pub mod` appear only in `mod.rs` files" is not met as written. Nine `pub mod`
   sit outside `lib.rs` with 60 bare `pub` items behind them, every one forced by a separate test
   crate. Each is registered in `test_module_layout.rs` with the files that force it, checked in
   both directions so an entry outliving its reason goes red, and the whole exemption expires in
   task 3. `pub use` and `pub(super)` are genuinely zero.
2. The case inventory is not byte-identical. `config.rs`'s two unit tests moved to
   `test_golden_format` at net zero, which dismantling `config.rs` authorizes, and the layout test
   this task delivers gained an eleventh case. `TargetPartitions`' label round-trip is gone with
   the type; `MemoryLimit` coverage is preserved. No golden moved after the quarantined rename.
3. The device evidence transfers by argument, not by a run on the head: every `src/` change above
   the GPU-green head is import order, a doc comment moved onto the right struct, and two comment
   lines. The 170 goldens carry recipe payload digests and are byte-identical, so the wire format
   the device consumes did not move.

Two known blind spots are stated where a developer meets them rather than only here: the
cross-component reader misses whitespace before `::`, and `forced_by`'s reverse half misses a
plain module import. The spec's promised follow-up — a length limit for `mod.rs` set from what
they weigh — is not set, and `coding-style.md` defers it with no owner.
