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
