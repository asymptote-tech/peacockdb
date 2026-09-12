# visibility — run detail

Spec: [`visibility.md`](visibility.md). Plan: [`visibility-impl.md`](visibility-impl.md).

## Chain position and branch

Chain `ENS-drop-mode-name`, task 6. Branch `ENS-visibility`, forked at `5596e4a3`, the tip of
task 5's branch. **Its PR targets `ENS-test-support`**, not master: task 5 is `done` but not
merged, so it is the parent that exists.

## How this task is dispatched

A slice is a dispatch, as in task 4. Plan tasks 1 and 2 together (baselines, the lint, the three
guard fixes); then one dispatch per demotion slice, plan tasks 3-8, each ending with the two
numbers; then plan task 9 (the registers), 10 (the wiki), 11 (the residues and the tickets), and
the final proof. The coordinator commits after each slice.

## Hosts at dispatch (2026-09-12)

- This box is Ubuntu 24.04 / glibc 2.39, reprovisioned 2026-09-11; master's `02069415` makes the
  shad-gpu patch step follow it. verda's hostname does not resolve; CPU shapes run locally.
- shad-gpu up, a neighbour holding 37 GiB of 143.7. `cpp/build`, `cpp/install` and
  `target-cudf-rapids-cuda-12.2` in the worktree are warm from task 5's cycle; `target/` is warm
  for `rust-only`. `CUDF_ROOT=/home/dmitry/data/miniforge3/envs/rapids-cuda-12.2`.
- `testdata/{tpch,tpcds}.sf1` are symlinks into `/home/dmitry/peacockdb/testdata`; the binaries'
  compile-time default root works without `PEACOCK_TESTDATA_DIR`.

## What the plan says, and what the tree measures

The spec and plan were written after task 2 and before tasks 4 and 5 ran; the chain was also
resequenced, so "task 3" in the plan is `test-layout.md` (chain task 4) and its "task 4" is
`test-support.md` (chain task 5). Measured on `5596e4a3` with `scripts/visibility-dump.py`:

- **Bare `pub` excluding `mod`: 263, and 200 outside `test_support`** — not the plan's 174. The
  work list by file: `plan/mod.rs` 92, `executor/mod.rs` 48, `wire/mod.rs` 20,
  `executor/cpu_backend/mod.rs` 14, `executor/gpu_backend/mod.rs` 12, `planner/mod.rs` 7,
  `plan_text/mod.rs` 3, `planner/translator/mod.rs` 2, `lib.rs` 2. The two backend facades are the
  26 the plan does not list: both subcomponents are declared `mod` in `executor/mod.rs`, so their
  bare `pub` items are unreachable already and `unreachable_pub` will report them the moment it is
  on — the lint's first count is not zero here, unlike the spec's expectation.
- `pub mod`: 7 — six components plus `test_support`, all in `lib.rs`. `PUB_MODULES` and
  `CROSS_COMPONENT_REACHES` are both empty; `PUB_OUTSIDE_A_MOD_RS` is `["lib.rs", "common.rs"]`.
  `test_module_layout` is a directory now — `tests/test_module_layout/{visibility,walls,privacy,
  near_miss,test_code,tree}.rs` behind `tests/test_module_layout.rs` — so the plan's single-file
  paths are found by name. It has 17 cases.
- Baseline tooling already lives in `scripts/` (plan task 1 step 1 is a check, not a move).
- Carry-over list, as the tree stands: `parquet_meta.rs`, `src/tests/end_to_end.rs` and
  `src/test_support/corpus_gpu.rs` are rustfmt-clean already; `cpu_backend/expr_physical.rs` is
  not. `test_support/` holds no `memory_limit.rs` or `mode.rs` (task 4 laid the harness out
  differently), so the `module-layout.md` contradiction the spec names is checked against what
  exists rather than the path written there. Task 5 widened `privacy.rs`'s readers
  (`declarations`, `type_aliases`, `component_imports`), so plan task 2's three guard fixes are
  measured against that code.
- Task 5's baselines and inventories: rust-only 1038 cases (`--lib` 516, layout 17), cudf 1049,
  gpu `--lib` 574; goldens 170 files; the whole package 1036 passed, 2 ignored.

## Run log

### 2026-09-12 — plan tasks 1-2 dispatched: baselines, the lint, the three guards

Board set to `building`.

### 2026-09-12 — plan tasks 1-2 done: baselines taken, the lint is on at 2, the three guards see their shapes

**Baselines** in `visibility-baselines/`, taken on `a6ad4db9` before the lint went on. Bare `pub`
excluding `mod`: 263, and 200 outside `test_support` — `plan/mod.rs` 92, `executor/mod.rs` 48,
`wire/mod.rs` 20, `cpu_backend/mod.rs` 14, `gpu_backend/mod.rs` 12, `planner/mod.rs` 7,
`plan_text/mod.rs` 3, `translator/mod.rs` 2, `lib.rs` 2. `pub mod` 7. Goldens 170 files, byte-identical
to task 5's. Inventories rust-only 1038 / cudf 1049 / gpu 574 cases, non-empty on all three;
`compare-inventory.sh` against task 5's says gpu identical and the other two off by exactly
`no_test_support_signature_names_a_component_type` (16 → 17 layout cases), which task 5 added after
taking its baseline — so identical to task 5's end state. The raw files also differ from task 5's in
sort order alone: `case-inventory.sh` sorts under the caller's locale. `visibility.txt` differs from
task 5's for the same reason, its dump predating the corpus move. Registers: `PUB_MODULES` 0,
`PUB_OUTSIDE_A_MOD_RS` 2, `CROSS_COMPONENT_REACHES` 0, `TEST_ONLY_ITEMS` 8.

**The lint.** `#![warn(unreachable_pub)]` sits under `lib.rs`'s doc block. Zero warnings on all three
shapes before it. With it, rust-only reported 910: 908 in flatc's `gpu_plan_generated.rs` — every
item there is `pub` and `wire::generated` is `mod` — and 2 in `planner/translator/mod.rs`. The
generated module already allows `unused_imports, dead_code, clippy::all` for code nobody wrote, so
`unreachable_pub` joined that list; the alternative was a work list no slice could lower. **Count now:
2 on rust-only, 2 on cudf, 2 on gpu, the same two on each** — `Translator::new` and
`Translator::translate`, `pub fn` on a `pub(crate) struct` inside `mod translator` (plan task 7's
slice). The 26 backend-facade items this file predicted are *not* reported: they are the associated
types of `impl Backend for CpuBackend` / `GpuBackend`, so rustc's effective visibility counts them
reachable through `<CpuBackend as Backend>::Source` and the lint stays quiet on them. They still count
in the dump and are demoted in the executor slice. The test profile (`cargo test`, which adds
`test-support` and `cfg(test)`) reports the same 2.

**What guards "no new warnings".** Nothing mechanical: no `-D warnings`, no `[lints]` table, no
`.cargo/config.toml`, no clippy or fmt step, no script that counts. The spec's "the crate's warning
count is already a checked baseline" is not true of the tree. The only check is the developer's
definition of done in `prompts.md` ("clean build with no new warnings"), which a reviewer reads. So the
lint's 2 would not fail CI; the coordinator decides whether a non-zero count between slices is
acceptable against that prose. Not worked around.

**The three guards**, each red on the planted shape, tree clean after (`git diff --stat` empty on the
planted file):
- `no_public_signature_names_a_type_from_a_private_module` now resolves imports through
  `private_modules` and `private_module_imports` — the latter is task 5's `component_imports` reader,
  generalised to `bound_from(text, prefixes, roots)` and shared by both — and matches a name as a
  path or, when capitalised, as a whole identifier (`private_name_in`; a lowercase name matches only
  as a path, since `layout` is a parameter as often as the module). Planted
  `pub fn layout_probe(trip: Trip) -> Trip` under `use accounting::Trip;` in `executor/driver/mod.rs`:
  green on the old reader, red on the new — `executor/driver/mod.rs:23: pub fn layout_probe(trip: Trip)
  -> Trip { names `Trip``. Nothing in the tree trips it today.
- `names_the_module` matches the module imported plain or under an alias as well as an item path.
  Pins red then green; then a temporary `PUB_MODULES` entry naming `test_cost_model.rs` with
  `use peacockdb_core::executor::cpu_backend;` planted in `test_golden_format.rs` made both halves
  red — `… is forced by peacockdb-core/tests/test_golden_format.rs, which forced_by does not name`.
- The super-climb loop is `super_chains(line) -> Vec<usize>` in `walls.rs`, pinned: it read
  `use super::super::x;` as `[2, 1]`, now `[2]`. Planted in `plan/error.rs`, the guard reports the line
  once: `plan/error.rs:17: use super::super::common; climbs 2 from depth 1`.

**Proof.** `cargo test --features rust-only -p peacockdb-core --test test_module_layout`: 17 passed,
same count as before. `--lib`: 514 passed, 2 ignored. Dump and goldens unchanged by the edits. rustfmt
clean on the four layout files and `generated.rs`; `lib.rs` carries six rustfmt hunks that predate
this task (`--config skip_children=true` on HEAD's copy shows the same six) and are not in the
spec's residue list.

Files: `peacockdb-core/src/lib.rs`, `peacockdb-core/src/wire/generated.rs`,
`peacockdb-core/tests/test_module_layout/{privacy,visibility,walls,near_miss}.rs`,
`llm-wiki/tasks/visibility-baselines/` (new, five files).

### 2026-09-12 — plan task 3 dispatched: `plan/mod.rs`, 92 items

Plan tasks 1-2 committed as `71d35418`. Two things settled there for the slices ahead: the lint's
count is a work list and not a gate — no `-D warnings`, no `[lints]`, no clippy or fmt step in
`pipeline.yml`, so the spec's "checked warning baseline" is prose in `prompts.md`'s definition of
done and nothing mechanical; and `wire/generated.rs` allows `unreachable_pub` beside the lints it
already allows for flatc's output. The 26 items in the two backend facades are `impl Backend`
associated types, reachable through the trait, so the lint never names them; the dump does, and
the executor slice demotes them.

### 2026-09-12 — plan task 3 done: `plan/mod.rs` 92 → 6, the six being `plan`'s closure

On `d8ad93b8`. Every bare `pub` in `plan/mod.rs` — items, fields, inherent methods, 170 lines —
became `pub(crate)`; then the compiler was asked what `planner::plan`'s signature forces back. It
named six, one hop at a time, each restored to `pub`: `GpuNode` and `PlanError` (`plan`'s return);
`NodeKind` and `RowInterval` (methods of the trait); `Schema` and `PartitionLayout` (fields of
`NodeKind`'s variants — an enum's variant fields are always public, so `private_interfaces` walks
them, where a struct's `pub(crate)` fields stop it). That closure is final: nothing later can narrow
a trait's methods below the trait. **Numbers: 263 → 177 bare `pub`, 200 → 114 outside
`test_support`; `plan/mod.rs` 92 → 6, 108 `pub(crate)`. Lint 2 → 2** on every shape, still
`Translator::new`/`translate`: the lint cannot fire inside a `pub mod`, so no component slice moves
it, only the translator's. Nothing outside `plan/` needed a change; the CLI builds
(`cargo build --features rust-only -p peacockdb`, into `target/`).

**Warnings the slice leaves, all named.** `private_interfaces`: 5 on every shape, none on the
surface table — `CpuExec::{sort,project,filter,aggregate}` name `GpuSort`/`GpuProject`/`GpuFilter`/
`GpuAggregate`, and `NodeExecutors::<B>::category` names `ExecutorCategory`. They clear when the
executor slice demotes those methods (`CpuExec` is in `cpu_backend/mod.rs`, a file no slice lists);
restoring the five types to `pub` instead would be a five-line edit, not taken because they are not
in the surface's closure. `private_bounds`: 0. `dead_code`, hidden until now because a `pub` item in a
`pub mod` is never checked: three `NodeRef` payloads (`MergePartitions`, `Union`, `Interleave`) that
every consumer matches with `_`, since those nodes are nothing but their children — `#[allow(dead_code)]`
with the reason at the site, keeping the enum one line per kind. Two left as findings, since a
demotion is never a deletion: **`plan::key_width` in `mod.rs` has no caller** (`aggregate.rs` calls
its own copy), and **`PartitionLayout::new` is called only from `wire/tests.rs` and
`plan/tests/joins.rs`** — test code in a production impl, which `coding-style.md` says belongs in a
child `tests` module. Both warn on every shape; the coordinator decides.

**Proof.** `cargo test --features rust-only -p peacockdb-core --test test_module_layout`: 17 passed.
`--lib`: 514 passed, 2 ignored. Goldens identical to the baseline; `case-inventory.sh rust-only`
1038 cases, `compare-inventory.sh` identical. rustfmt clean on `plan/mod.rs` (formatted with
`skip_children=true`, so the component's files did not move; five hunks, four from `pub(crate)`
pushing a signature past the width and one that predated the slice).

Files: `peacockdb-core/src/plan/mod.rs` only.

### 2026-09-12 — the two `dead_code` findings closed, and a third of the same shape

On `1cdd74a2`. The `key_width` finding was mis-stated: the caller grep ran through `head -3`
and stopped at `aggregate.rs`'s own lines, so `plan/tests/aggregate.rs`'s call was never seen.
Both findings are the same shape — a facade item whose only caller is a test — and so is
`planner::can_be_null`, which the planner slice uncovered the same way.

- `plan::key_width` and `planner::can_be_null` were delegates to `aggregate::key_width` and
  `nulls::can_be_null`, called only from `plan/tests/aggregate.rs` and
  `planner/tests/null_analysis.rs`. The implementation module owns each rule — `aggregate.rs`
  uses `key_width` itself twice, `nulls.rs` is the null analysis — and a component's tests reach
  their own implementation modules already (`wire/tests.rs` imports `super::attach::…`,
  `cpu_backend/tests/accumulate.rs` imports `cpu_backend::accumulate::State`). So the delegates
  are gone and the tests import the owner: one definition each, used.
- `PartitionLayout::new` is the private-field-reader case `coding-style.md` names: an inherent
  `impl PartitionLayout { pub(crate) fn new }` now sits in `plan/tests/mod.rs`, the way
  `cpu_backend/join/tests.rs` carries `CpuJoin::has_finish_pass`. A method resolves through the
  type, so `wire/tests.rs` keeps calling `PartitionLayout::new(1)` with no cross-component path and
  no `TEST_ONLY_ITEMS` entry; the register's both-way check is unchanged and green.

Warnings 9 → 7 on every shape (2 lint, 5 `private_interfaces`); dump 177 / 114 unchanged, since
none of the three was a bare `pub`. `--test test_module_layout` 17, `--lib` 514 + 2 ignored.

### 2026-09-12 — plan task 4 done: `planner/mod.rs` 7 → 5, the fifth being `MemoryModel`

Everything demoted first, which made the whole planner dead code in a plain build (158
warnings) — the crate's only entry into it is `plan`. Restored what the compiler named: `plan`,
`PlanKnobs`, `BatchSizing`, `SMALL_TABLE_BYTES` for the CLI, then **`MemoryModel`** as
`private_interfaces` on `planner::plan` (its return), and **`PlanKnobs`'s four fields** as
`E0451: fields target_partitions, sizing, budget and small_table_bytes of struct PlanKnobs are
private` from `peacockdb/src/main.rs:46`, where the CLI writes the struct literal. `MemoryModel`'s
fields and `SourceEstimate` stay `pub(crate)`: the CLI discards the model, and the closure walks
types, not the fields a pub struct keeps to itself. `can_be_null` went as above.

**Receipt.** `SMALL_TABLE_BYTES` demoted: `error[E0603]: constant SMALL_TABLE_BYTES is private
--> peacockdb/src/main.rs:15:55`; restored, the CLI builds (`cargo build --features rust-only
-p peacockdb`).

**Numbers: 177 → 175, 114 → 112; `planner/mod.rs` 7 → 5. Lint 2 on every shape**, unchanged
until the translator slice; `private_interfaces` 5, `private_bounds` 0, 7 warnings total on each
of the three shapes. `--test test_module_layout` 17; `--lib` 514 + 2 ignored; goldens identical;
`case-inventory.sh rust-only` 1038, identical. rustfmt clean on `planner/mod.rs` alone
(`skip_children`), `plan/tests/{mod,aggregate}.rs` and `planner/tests/null_analysis.rs`.

Files: `plan/mod.rs`, `plan/tests/mod.rs`, `plan/tests/aggregate.rs`, `planner/mod.rs`,
`planner/tests/null_analysis.rs`.

### 2026-09-12 — plan task 5 stopped for a decision: the executor closure is 33 items, and the GPU backend has no production caller

On `c2baf00b`. All 128 bare `pub` lines in `executor/mod.rs`, `cpu_backend/mod.rs` and
`gpu_backend/mod.rs` were demoted, `run` and `CpuBackend` kept, and the compiler asked what they
force. Three hops, every item restored with its reason:

1. From `run`'s signature: `Backend` (the bound), `RunReport`, `RunError`.
2. From `Backend`, a `pub` trait whose associated types are as public as it is: `Batch`,
   `SourceExecutor`, `ExecExecutor`, `BatchAccumulatorExecutor`, `PartitionAccumulatorExecutor`,
   `PartitionEmitterExecutor`, `JoinExecutor`, `UnloadExecutor` (the bounds on its associated
   types), `NodeExecutors` (what `executors_for` returns); `When` (a field of
   `RunError::BudgetExceeded`); and — as `E0446` hard errors, not warnings — `CpuBatch` and the
   eight `Cpu*` types in `cpu_backend/mod.rs` that `impl Backend for CpuBackend` binds to the
   associated types. A `pub` type's trait impl cannot bind a private type there.
3. From the seven category traits' method signatures: `Executor` (their supertrait),
   `ProbingJoin` (`JoinExecutor::Probing`'s bound, which drags `CpuProbingJoin` the same
   `E0446` way), `SourceStep`, `LaneEvent`, `Forwarder` (a `NodeExecutors` payload), `RowRange`,
   `CallStats`, `BackendError`.

Then the CLI: `E0616: field batches of struct RunReport is private` and `E0624: method
record_batch is private` (`peacockdb/src/main.rs:57-59`), so `RunReport.batches` and
`CpuBatch::record_batch` are `pub`. Receipt: `CpuBackend` demoted gives `error[E0603]: struct
CpuBackend is private --> peacockdb/src/main.rs:12:31`; restored, the CLI builds.

**The surface this leaves: `executor/mod.rs` 48 → 25, `cpu_backend/mod.rs` 14 → 8,
`gpu_backend/mod.rs` 12 → 0; dump 175 → 134, 112 → 71.** The 33 are the honest size of the
CLI's API through `run<B: Backend>`: a generic entry point makes the whole trait family and one
concrete backend's associated types public. The spec's table predicted five. `has_finish_pass`
stays `pub(crate)`. All three shapes build; the CLI builds; layout 17; `--lib` 514 + 2 ignored;
rustfmt unchanged on the three files. Nothing committed to the closure beyond what the compiler
named.

**What the demotion uncovers, and why it stops here.** `dead_code` was silent on every `pub`
item inside a `pub mod`; now it speaks, and what it names is production code whose only
readers are tests:

- **The GPU backend, whole.** `GpuBackend` is not in the surface table, so it is `pub(crate)`,
  and nothing in a non-test build names it: the CLI runs `run::<CpuBackend>` only. Under `cudf`
  and `gpu` the compiler then reports `gpu_backend/` entire — 52 warnings, every struct "never
  constructed", `GpuBatch`, `GpuContext`, `GpuBackend` among them. That is a statement about the
  CLI (no `--gpu` yet), not the engine, and it is the decision to weigh: keep `GpuBackend` `pub`
  as the engine's second entry, which the `E0446` rule then extends to `GpuBatch`, `GpuContext`
  and the eight `Gpu*` types (about 11 more items, the table grows to ~44); or hold the letter of
  the table and carry 52 warnings on the device shapes until the CLI grows the flag; or
  `#[allow(dead_code)]` at the subcomponent root with that reason.
- **`post_order_of_every_node`**, a chain of three delegates (`executor/mod.rs` → `driver/mod.rs`
  → `driver/index.rs`), called only by `planner/tests/plan_goldens.rs:899`. Slice 4's fix — the
  test imports the owner — is refused here: the owner is inside `executor::driver`, a
  subcomponent of another component, `E0603` from `planner`. It is the `TEST_ONLY_ITEMS` shape
  exactly (`has_finish_pass` is the precedent, two entries), but the innermost copy in `index.rs`
  is not a `mod.rs` and is itself unused in production, so the register alone does not close it.
- **`RunReport`'s fields** other than `batches` — `peak_bytes`, `in_flight_bytes`, `steps`,
  `calls`, `holds`, and the rest — are read only by `test_support` and the driver tests: the
  report is written for the goldens. `MemoryModel` got the same treatment in slice 4 and its
  readers were all inside the planner, so it was silent; here the readers are behind the feature.
  Either the fields stay `pub` (the report is surface; the CLI may read it) or they carry an
  `allow` naming the harness.
- **`Underestimate::ratio`**, read by `driver/accounting/tests.rs:182` alone — the slice-4 move
  into a child `tests` inherent impl closes it; not done, to keep the stop clean.

Warnings per shape as the tree stands: rust-only 7 (2 lint, 3 `post_order_of_every_node`,
`ratio`, `RunReport` fields); cudf and gpu 52. The five `private_interfaces` from slice 3 are
gone. Files: `executor/mod.rs`, `executor/cpu_backend/mod.rs`, `executor/gpu_backend/mod.rs`.

### 2026-09-12 — plan task 5 done: the executor surface is 36 items in two files, and the device backend has no production entry point

Closing the stop above with the four decisions. **Dump 175 → 137, 112 → 74; `executor/mod.rs`
48 → 28, `cpu_backend/mod.rs` 14 → 8, `gpu_backend/mod.rs` 12 → 0.** Lint 2 on every shape,
and 2 is the whole warning count on every shape.

**The closure, by file and hop.** `executor/mod.rs` (28): `run`, `CpuBackend` — the CLI's;
`Backend`, `RunReport`, `RunError` — `run`'s bound and return; from `Backend` being a `pub`
trait, its associated types' bounds `Batch`, `SourceExecutor`, `ExecExecutor`,
`BatchAccumulatorExecutor`, `PartitionAccumulatorExecutor`, `PartitionEmitterExecutor`,
`JoinExecutor`, `UnloadExecutor`, and `NodeExecutors` (what `executors_for` returns); `When`
(a `RunError` variant field); `CpuBatch` (`Backend::Batch` for `CpuBackend`, an `E0446` error
otherwise); from the category traits' methods, `Executor` (supertrait), `ProbingJoin`
(`JoinExecutor::Probing`'s bound), `SourceStep`, `LaneEvent`, `Forwarder`, `RowRange`,
`CallStats`, `BackendError`; from the report's fields, `Underestimate`, `TraceEvent`,
`EmittedBatch`; and `CpuBatch::record_batch`, which the CLI calls. `cpu_backend/mod.rs` (8):
`CpuSource`, `CpuExec`, `CpuAccumulator`, `CpuPartitionAccumulator`, `CpuEmitter`, `CpuJoin`,
`CpuProbingJoin`, `CpuUnload` — the associated types `impl Backend for CpuBackend` binds,
each an `E0446` hard error at `pub(crate)`. `has_finish_pass` stays `pub(crate)`. Receipt:
`CpuBackend` demoted gives `error[E0603]: struct CpuBackend is private -->
peacockdb/src/main.rs:12:31`.

**The four decisions as applied.**
1. `GpuBackend` stays `pub(crate)`. **The device backend has no production entry point, by the
   CLI's own account**: `peacockdb/src/main.rs` runs `run::<CpuBackend>` and its header says a
   `--gpu` flag would fail on nearly every input today. `#[cfg_attr(not(feature =
   "test-support"), allow(dead_code))]` sits on `mod gpu_backend;`, `mod gpu_batch;`, `GpuBatch`,
   `GpuBackend` and `GpuContext` in `executor/mod.rs`, with the reason once at the module
   declaration; the lint stays live in every `cargo test` shape. Confirmed: `cargo-cudf.sh build`
   and `… --features gpu` at 2 warnings, and `cargo-cudf.sh test --lib --no-run` in both shapes
   (harness on) clean of `dead_code`. One accessor the attribute did not cover, `GpuBatch::executor`,
   was read by `executor/ffi_tests/mod.rs` alone and moved there as an inherent impl;
   `--lib -- ffi_tests::` 3 passed under cudf.
2. `post_order_of_every_node` is a three-link `TEST_ONLY_ITEMS` chain: `#[cfg(test)]` on
   `executor/mod.rs` (called by `planner/tests/plan_goldens.rs`), `executor/driver/mod.rs`
   (called by `executor/mod.rs`) and `executor/driver/index.rs` (called by `driver/mod.rs`) —
   the driver does not use the walk itself; it reads `post_order` off the index it builds. The
   register accepts a non-`mod.rs` file. Both directions proven: 17 green, and with the innermost
   gate removed the check says `executor/driver/index.rs: post_order_of_every_node is registered
   as test-only and no longer carries a #[cfg(test)]; drop the entry`.
3. `RunReport`'s 18 fields are `pub`; the three types they name joined the table above.
4. `Underestimate::ratio` moved into `executor/driver/accounting/tests.rs` as an inherent impl.

**Proof.** rust-only, cudf, gpu builds and `cargo build --features rust-only -p peacockdb`:
`Finished`, 2 warnings each (the lint). `--test test_module_layout` 17; `--lib` 514 + 2 ignored;
`--lib -- executor::` 217; goldens identical; `case-inventory.sh rust-only` 1038, identical.
rustfmt clean on every touched file, each alone.

Files: `executor/mod.rs`, `executor/cpu_backend/mod.rs`, `executor/gpu_backend/mod.rs`,
`executor/driver/mod.rs`, `executor/driver/index.rs`, `executor/driver/accounting/tests.rs`,
`executor/ffi_tests/mod.rs`, `tests/test_module_layout/test_code.rs`.

### 2026-09-12 — plan task 6 dispatched: `wire/mod.rs`, 20 items, in a fresh window

Plan task 5 committed as the head above. The developer that carried slices 1-5 hands over here;
what it established for the slices ahead: the closure is what the compiler names, hop by hop, and
the reason per hop goes in the entry; a facade delegate whose only reader is a test moves to the
test module that reads it, or is registered in `TEST_ONLY_ITEMS` when the walls refuse the move;
the lint's two warnings are `Translator::new`/`translate` and clear in the translator slice.

### 2026-09-12 — plan task 6 done: `wire/mod.rs` 20 → 0, and the wire has no production caller on any shape

On `8de8a893`. Every bare `pub` in `wire/mod.rs` — 20 items and the five fields of `Call` and
`Recipe` — became `pub(crate)`, 25 one-word lines; the dump shows 25 `pub(crate)` in the file
(the 20 plus the five it already had), so demoted, not deleted. **Numbers: 137 → 117, 74 → 54;
`wire/mod.rs` 20 → 0. Lint 2 on every shape, and 2 is the whole warning count** on rust-only,
cudf, gpu, `cargo build --features rust-only -p peacockdb`, and — this is the new check — the
`--lib --no-run` test shapes on all three, both halves. The closure kept nothing: no shape and
not the CLI named a `wire` item, so `RecipePlan` and `attach_recipes` went with the rest.

**What the demotion uncovers.** With the 20 checked at last, `dead_code` reports the whole
component: 104 warnings on a plain rust-only build, 98 on cudf and gpu. Three groups, by caller:
the vocabulary (`Seq`, `AbiSymbol`, `Call`, `Recipe`, `Input`, `FbKind`, `CallPattern`,
`ProjectRole`) is read by `gpu_backend/*`, which slice 5 already marked dead until the CLI grows a
device path; the writers (`attach_recipes` and `attach`, `serialize`, `writer`, `node_writer`,
`expr_writer`, `aggregate_writer`, `join` behind it) are driven by `test_support/corpus_gpu.rs`
alone; and the reader and renderers (`check_seq_kinds`, `depth`, `render_plan_recipes`, `Payloads`,
with `read.rs`, `recipes.rs`, `fb_text.rs` behind them) by `planner/tests/plan_goldens.rs` and
`wire/tests.rs` alone. Handled as slice 5 handled `gpu_backend`: one attribute at the component with
the reason once, `#![cfg_attr(not(test), allow(dead_code))]` at the top of `wire/mod.rs`, the
declaration site being `lib.rs` and the fact being about `wire`. The code stays in every build and
is type-checked there; only the lint moves to where the callers are.

**Why `not(test)` and not slice 5's `not(feature = "test-support")`.** `cargo test` compiles the
lib twice — `(lib)` with the harness on and `cfg(test)` off, for the self dev-dependency, and
`(lib test)` — and measured with slice 5's key the `(lib)` half still reported 104 on rust-only
(`corpus_gpu.rs` is device-only and the goldens are `cfg(test)`, so nothing there calls the wire)
and 26 on cudf (the reader and renderers, golden-only). `gpu_backend` never showed this because
rust-only does not compile it. Red then green on both keys is in the run: 104 → 0 on `(lib)`.
One item stayed red under `cfg(test)` on rust-only: `RecipePlan::wire_nodes`, field and method,
whose readers (`corpus_gpu.rs:53`, `gpu_tests/mod.rs:175`) compare it with `begin_plan`'s C++
count; `#[cfg_attr(feature = "rust-only", allow(dead_code))]` on the method, one line of reason,
and the allowed method is a live root so the field's read counts.

**One guard re-pinned.** `a_private_module_is_unreachable_from_outside_the_crate`'s control probe
named `peacockdb_core::wire::Recipe` as its public path, so it went red for the right reason
(`E0603: struct Recipe is private`). The control now names `executor::CpuBackend`, in the surface
table; the negative probe (`wire::generated::…::PlanNodeKind`) and its `E0603`/`generated` check
are unchanged.

**A finding, not done.** The reader and renderers are test code by `coding-style.md`'s definition —
they exist to serve the goldens — in the `TEST_ONLY_ITEMS` shape (the caller is another component,
so the walls refuse a move). But three implementation modules sit behind the four delegates, so a
register entry alone does not close it and `#[cfg(test)]` on the modules has no sanctioned form;
left under the component attribute for the coordinator to route.

**Proof.** `--test test_module_layout` 17; `--lib` 514 + 2 ignored; `--lib -- wire::` 37; goldens
identical; `case-inventory.sh rust-only` identical to the baseline. rustfmt clean on `wire/mod.rs`
alone (`skip_children`; one hunk, `pub(crate)` pushing `render_plan_recipes` past the width) and on
`privacy.rs`.

Files: `peacockdb-core/src/wire/mod.rs`, `peacockdb-core/tests/test_module_layout/privacy.rs`.

### 2026-09-12 — plan task 7 done: `plan_text/mod.rs` 3 → 0, `translator/mod.rs` 2 → 0, and the lint reports zero

On `225386be`. All five became `pub(crate)`, and with them `Translator`'s three bare `pub` fields
(`target_partitions`, `batching`, `small_table_bytes`) on the `pub(crate)` struct; the dump shows
10 `pub(crate)` across the two files, the five plus the five they had. **Numbers: 117 → 112,
74 → 49 outside `test_support`. `unreachable_pub`: 0 on every shape, for the first time** —
`Translator::new` and `translate` were its last two — **and 0 is the whole warning count** on
rust-only, cudf, gpu, `cargo build --features rust-only -p peacockdb`, and both halves of the
`--lib --no-run` test shapes on all three. The closure kept nothing: the CLI names no `plan_text`
item (`peacockdb/src/main.rs` prints results, not plans), so `render_run` went with the rest and
the spec's table stands.

**What the demotion uncovers.** `plan_text` entire, 40 `dead_code` on a plain build — the `wire`
shape again, by caller: `render_run` and `run_text.rs` are the harness's (`test_support/corpus.rs`
on every shape, `corpus_gpu.rs` on the device ones) and the driver tests'; `render_plan`,
`render_plan_memory`, `memory.rs` and the rest of `node_text.rs`/`expr_text.rs` are the goldens
tests' (`planner/tests/plan_goldens.rs`, `plan/tests/layout_injection.rs`,
`executor/driver/tests/render.rs`) alone. Handled as slice 6 handled `wire`:
`#![cfg_attr(not(test), allow(dead_code))]` at the top of `plan_text/mod.rs` with the reason once,
keyed on `not(test)` for the same measured reason — the `(lib)` half of `cargo test` has the harness
on and `cfg(test)` off, and there only `render_run`'s path is live. The translator uncovered nothing:
`planner::plan` calls `new` and `translate`. Red then green: 40 → 0.

**Routed the same way as `wire`'s reader, not restructured.** `render_plan` and `render_plan_memory`
are golden-only, the `TEST_ONLY_ITEMS` shape with callers in three other components, and `memory.rs`
plus most of `node_text.rs` behind them; `render_run` is not — the harness is production-shaped code
behind a feature. Recorded for the signoff with the `wire` finding.

**Proof.** `--test test_module_layout` 17; `--lib` 514 + 2 ignored; `--lib -- plan_text::` 16;
`--lib -- planner::translator::` 65; goldens identical; `case-inventory.sh rust-only` identical to
the baseline. rustfmt clean on each file alone (`skip_children`; one hunk in `translator/mod.rs`,
`pub(crate)` pushing `translate` past the width; `plan_text/mod.rs` clean as written).

Files: `peacockdb-core/src/plan_text/mod.rs`, `peacockdb-core/src/planner/translator/mod.rs`.

### 2026-09-12 — plan tasks 8-9 done: the surface is 49 rows in five files, pinned by name, and both registers are gone

On `18d2dc2e`. **Plan task 8 was a check, not a change**: `common.rs` at zero bare `pub`; `lib.rs`
keeps `build_session_state` and `register_tables_for` and, past those, only `pub mod`. The dump
outside `test_support` is **49**: `lib.rs` 2, `plan/mod.rs` 6, `planner/mod.rs` 5,
`executor/mod.rs` 28, `executor/cpu_backend/mod.rs` 8 — the CLI's six plus the closures slices 3-5
named. By file:

- `lib.rs`: `build_session_state`, `register_tables_for`.
- `plan/mod.rs`: `GpuNode`, `NodeKind`, `PartitionLayout`, `PlanError`, `RowInterval`, `Schema`.
- `planner/mod.rs`: `BatchSizing`, `MemoryModel`, `PlanKnobs`, `SMALL_TABLE_BYTES`, `plan`.
- `executor/mod.rs`: `Backend`, `BackendError`, `Batch`, `BatchAccumulatorExecutor`, `CallStats`,
  `CpuBackend`, `CpuBatch`, `EmittedBatch`, `ExecExecutor`, `Executor`, `Forwarder`,
  `JoinExecutor`, `LaneEvent`, `NodeExecutors`, `PartitionAccumulatorExecutor`,
  `PartitionEmitterExecutor`, `ProbingJoin`, `RowRange`, `RunError`, `RunReport`,
  `SourceExecutor`, `SourceStep`, `TraceEvent`, `Underestimate`, `UnloadExecutor`, `When`,
  `record_batch` (`CpuBatch`'s, the CLI's), `run`.
- `executor/cpu_backend/mod.rs`: `CpuAccumulator`, `CpuEmitter`, `CpuExec`, `CpuJoin`,
  `CpuPartitionAccumulator`, `CpuProbingJoin`, `CpuSource`, `CpuUnload`.

**Demoted, not deleted.** `--items` against the baseline's `(scope, kind, name)` multiset differs
by exactly two rows, both gone: `top fn key_width` and `top fn can_be_null`, the delegates slice 4
deleted so their tests import the owner. `PartitionLayout::new`, `Underestimate::ratio` and
`GpuBatch::executor` are still in the multiset from their test modules (`plan/tests/mod.rs`,
`driver/accounting/tests.rs`, `executor/ffi_tests/mod.rs`); `post_order_of_every_node` stays
behind `#[cfg(test)]`. Nothing else moved.

**The lint, zero and armed.** 0 warnings on rust-only, cudf and gpu. `wire/node_writer.rs:80`
spelled `pub fn scan` gives `warning: unreachable `pub` item --> peacockdb-core/src/wire/
node_writer.rs:80:1 … help: consider restricting its visibility: `pub(super)``; reverted, the
file's diff is empty and the build is back at 0.

**Plan task 9.** `visibility.rs` loses `PubModule`, `PUB_MODULES`, `is_an_exempt_module`,
`every_pub_mod_exemption_is_still_forced_by_what_it_names` and its `forced_by` machinery
(`files_naming`, `walk_naming`, `workspace_members`, `names_the_module`); `walls.rs` loses
`CrossComponentReach`, `CROSS_COMPONENT_REACHES` and the register loop, so
`only_the_parent_component_names_a_subcomponent` now says every cross-component reach is a
violation. `PUB_OUTSIDE_A_MOD_RS` is `["lib.rs"]`. `near_miss.rs` drops the pins on the two
deleted readers and gains pins on the two new ones; `test_code.rs`'s comment no longer cites
`PubModule::forced_by`. Two assertions arrive in `visibility.rs`:

- `pub_mod_declares_a_component_and_nothing_else` keeps its name and its half about the rest of
  the tree (no exempt set now), and pins `lib.rs`: the unconditional `pub mod` are exactly
  `COMPONENTS` — `common`, `executor`, `plan`, `plan_text`, `planner`, `wire` — and the gated
  set is exactly `test_support` under `feature = "test-support"`, read by `gated_pub_mods` from
  the line above each declaration. Red with `mod translator;` → `pub mod translator;` in
  `planner/mod.rs`: `planner/mod.rs declares `pub mod translator;` … There is no sanctioned form.`
- `bare_pub_is_the_surface_and_nothing_else`, new: `SURFACE` is the 49 by file and name, its doc
  saying what the table is and what a new row needs; checked both ways over every file outside
  `test_support/`, test modules included. Red with `CallStats` demoted: `executor/mod.rs: the
  surface lists `CallStats` and it is not pub there`; red with `wire/mod.rs`'s `seqs` spelled
  `pub`: `wire/mod.rs: `seqs` is pub and the surface does not list it`. `bare_pub_name` reads the
  name past every keyword a declaration carries (`async`, `unsafe`, `extern "C"`, `const`, …).

**The one inventory difference, named.** The layout target stays at 17 cases; the leaf-name set
swaps `visibility::every_pub_mod_exemption_is_still_forced_by_what_it_names` for
`visibility::bare_pub_is_the_surface_and_nothing_else`. Not replaced in place: a surface check
under a name about `pub mod` exemptions would be the register's kind of lie, and the deleted case
had no reason left to exist. `compare-inventory.sh` reports exactly that hunk and nothing else.

**Proof.** `--test test_module_layout` 17, no warnings; `--lib` 514 + 2 ignored; goldens identical;
inventory as above. rustfmt clean on `visibility.rs`, `walls.rs`, `near_miss.rs`, `test_code.rs`,
each alone. `src/` untouched (`git status` shows the four test files only).

Files: `peacockdb-core/tests/test_module_layout/{visibility,walls,near_miss,test_code}.rs`.

### 2026-09-12 — plan task 10 done by the coordinator: the wiki

`coding-style.md`'s Visibility section rewritten: the four boundaries first, then the rules —
the crate's API is the CLI's and `SURFACE` is the receipt, `unreachable_pub` keeps it true,
`pub mod` in `lib.rs` only, the `test_support` signature rule unchanged — with the exemption
paragraph, both registers and the "eight items" bullet gone, the compiler/layout-test split kept,
a bullet for the two components and the subcomponent that have no production caller, and the
paragraph on why the backend types were not hoisted. `build-test.md`: the layout-rules row names
`SURFACE` and the one remaining register; the CI section records the formatting gap and what a
fix needs (plan task 11 step 4, taken here since it is prose). `architecture.md` states nothing
about visibility or the files this task changed; the analyst's reading will say whether any
sentence is falsified.

### 2026-09-12 — plan task 11 done: the residues, #201, and the final proof

On `c82897bc`. Nothing red.

**rustfmt residues.** `executor/cpu_backend/expr_physical.rs` (two hunks) and `lib.rs` (six, all
predating this task; the lint attribute sat among them) formatted alone with `skip_children`,
both `--check` clean; whitespace and wrapping only. Two files the "mode" sweep touched are not
rustfmt-clean and were left so — `expr_physical/tests.rs` (5 hunks) and `translator/expr.rs`
(1), the same counts on HEAD's copies — since they are outside the spec's residue list and the
rule is the files themselves, never the crate.

**The "mode" comments: 66 lines fixed, 101 judged legitimate** (167 comment lines carried the
word; one fixed line, `cpu_backend/mod.rs:188`, keeps a legitimate second use). Criterion: task 1
retired "batch partitioned" as the name of an execution regime with no alternative left, so a
comment saying *this mode* or *the mode's* for the engine is the retired sense, rewritten as
*the engine*; a mode as one of the five `tp<N>-<sizing>` planning shapes, DataFusion's aggregate
`Partial`/`Single` mode, a join mode of the capability matrix, or a "failure mode" is a live
noun and stays. Fixed, for example: `planner/translator/mod.rs:1` "DataFusion physical plan →
the mode's node tree" → "the engine's node tree"; `wire/aggregate_writer.rs:151` "this mode never
sends an `avg` to a device" → "the engine never sends"; `plan_text/run_text.rs:4` "the two totals
both mode families carry" → "every run carries"; `plan/mod.rs:1219` "which mode produced it" →
"which of the two produced it" (DataFusion or the engine). Left, for example:
`test_support/corpus.rs:1` "One corpus query at one mode"; `cpu_backend/mod.rs:357` "DataFusion's
`AggregateExec` in Partial mode"; `plan/mod.rs:756` "What the capability matrix says about one
join mode"; `test_support/registry.rs:262` "the exact failure mode this registry replaces";
`registry.rs:267` "after the split by execution mode", which names the binary split, not the
engine. Comment-only: every changed line outside the two formatted files carries `//`; 28 files.

**#201 filed**, Infrastructure / process, newest-first, 12 lines with two stating the problem:
`gpu_tests/murmur_conformance.rs`'s `cpu_partition_ids` re-derives the lane rule that
`cpu_backend/spark_partitioning.rs`'s `rows_per_lane` runs in production, so only the copy is held
against the device. Counter in the header now 202; Contents row 22 → 23. Not fixed.

**The contradiction.** `module-layout.md:164` says `test-layout.md` creates `src/test_support/`;
task 4 (`test-layout.md`) did, so the sentence is true, and nothing in `src/test_support/` or
`tests/common/` says otherwise. Unchanged.

**Final proof.**
- `cargo test --features rust-only -p peacockdb-core -- --test-threads=2`: **1036 passed, 0
  failed, 2 ignored**, 0 warnings, exit 0, nine result lines — `--lib` 514 + 2i, ci_coverage 8,
  corpus_goldens 20, cost_model 3, cpu_corpus 448, golden_format 26, gpu_corpus 0,
  module_layout 17, doc 0. Monitored every 2 minutes; no failure signature.
- rust-only, cudf, gpu builds and `cargo build --features rust-only -p peacockdb`: 0 warnings
  each, `unreachable_pub` 0. Dump 112 / 49; the 49 match `SURFACE` (the layout target is the
  check, and it is green).
- Goldens digest: empty diff. Inventories: rust-only 1038 and cudf 1049 each differ from their
  baselines by the one leaf-name swap from plan task 9 and nothing else; gpu 574 identical.
- `cargo-cudf.sh test -p peacockdb-core --no-run` and `… --lib --features gpu --no-run`: 0
  warnings, `Finished`.
- shad-gpu, run `20260912T025930-206003`: `--build` 0 warnings, both binaries staged;
  `--push-binaries`, `--patch --run-detached`, `--run-status` polled at 2 minutes, `FINISHED,
  exit code 0`. C++ 11 + 6 + 27 + 4 + 4 = **52 passed**; `peacockdb_core_gpu_lib` **55 passed,
  519 filtered out** (14.89 s); `test_gpu_corpus` **8 passed**; `GPU test run OK`. Every pool
  built against 103 GiB free (the neighbour at 37 GiB); no #178 entry needed.
- `grep -rn 'test-support' .github scripts`: no hits.

Files: `llm-wiki/tickets.md`, `peacockdb-core/src/lib.rs`,
`peacockdb-core/src/executor/cpu_backend/expr_physical.rs`, and 28 files with comment-only edits
under `peacockdb-core/src/` (`git status --short`: 31 files, this one excluded).

### 2026-09-12 — reviewing: PR #148 against `ENS-test-support`

Twelve commits, 57 files. Every slice's proof is in the entries above; the final one ran the
whole package, every build shape, the CLI and the device cycle on `d82a87ee`. Reviewer round 1
dispatched next.

### 2026-09-12 — review round 1: 0 blocking, 1 important, 5 nits

The reviewer modelled every guard and the dump in Python over the tree: the 49 match `SURFACE`
both ways, the item multiset differs from the baseline by the two deleted delegates alone,
goldens identical, layout 17 with the one swap, invariants hold, every `allow` the branch adds
enumerated and justified, the sweep's criterion confirmed on every changed line.

- **important** — the `post_order_of_every_node` three-link `TEST_ONLY_ITEMS` chain: the
  innermost entry (`driver/index.rs`, called from its own subcomponent's `mod.rs`) is outside the
  register's one sanctioned shape (a cross-component entry point in a `mod.rs`), and the chain is
  unnecessary — `PlanIndex` is `pub(crate)` with `pub(crate) nodes` and `IndexedNode.post_order`,
  and `PlanIndex::build` is production, so `planner/tests/plan_goldens.rs` can read the index
  directly. Fix: delete all three links and rows. → developer.
- nit — `gpu_backend/mod.rs:101`: `GpuSource`'s doc said "`pub` because" on a `pub(crate)`
  struct. Fixed by the coordinator.
- nit — `coding-style.md`: "36 of the 49" counts the three CLI-named items as forced by `run`
  (the closure is 33); "each says so at its declaration" is true of `mod gpu_backend;` but
  `wire` and `plan_text` say so at the top of their `mod.rs`; `plan_text`'s expiry is "the
  first CLI renderer", not a device caller. Coordinator's, after the numbers settle.
- nit, **taken** — `RunReport`'s 17 non-`batches` fields `pub` by decision drag `TraceEvent`,
  `Underestimate` and `EmittedBatch` onto the surface with no receipt from the CLI or the
  compiler — the only such rows, against the branch's own rule and its `MemoryModel` precedent.
  Fix: `pub(crate)` on the 17 fields with the `dead_code` attribute on the struct and the reason
  (written for the goldens and the driver tests; the CLI reads `batches`), the three types
  `pub(crate)`, `SURFACE` 49 → 46. → developer.
- nit, **taken** — `pub_mod_declarations` and `is_bare_pub_item` require a line to start with
  `pub `, so `#[attr] pub mod x;` on one line is invisible to both new assertions, and
  `pub(crate) mod translator;` opens a wall crate-wide while matching neither reader. Fix: route
  both readers through `test_code::split_attributes`; add an assertion that a `mod` declaration
  outside `lib.rs` and the test directories carries no visibility. → developer.
- nit, **taken** — `SURFACE` pins items, not fields: a `pub` field added to a surface struct
  passes every guard and the lint. Fix: count `privacy::pub_fields()` per surface file into the
  same both-way check. → developer.
