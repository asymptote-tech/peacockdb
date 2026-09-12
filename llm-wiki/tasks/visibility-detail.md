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
