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
