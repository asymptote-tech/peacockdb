# test-support — run detail

Spec: [`test-support.md`](test-support.md). Plan: [`test-support-impl.md`](test-support-impl.md).

## Chain position and branch

Chain `ENS-drop-mode-name`, task 5. Branch `ENS-test-support`, forked at `2a3245df`, the tip of
task 4's branch. **Its PR targets `ENS-test-layout`**, not master: task 4 is `done` but not merged,
so it is the parent that exists.

## How this task is dispatched

Two dispatches: plan tasks 1-2 together (the baselines are five commands and the move is 698
lines), then plan task 3 in a fresh window, so the proof is re-measured by someone who did not
make the move. Each ends by appending here.

## Hosts at dispatch (2026-09-11)

- **verda**: answers but refuses the key (`Permission denied (publickey)`) — reprovisioned; local
  runs for the CPU shapes.
- **shad-gpu** up, `llm-gpu0h200`, a neighbour holding 37 GiB of the 143.7. Needed only if the
  device corpus binary's path changes — it does, since `corpus_gpu.rs` moves, so one cycle at the
  end proves `test_gpu_corpus` still runs its 8.
- `CUDF_ROOT=~/data/miniforge3/envs/rapids-cuda-12.2` on this host.

## What task 4 left for this task

- The eight forced items: `GpuNode`, `validate` (`plan/mod.rs`), `RecipePlan`, `attach_recipes`
  (`wire/mod.rs`), `RunReport`, `GpuBackend`, `GpuContext` (`executor/mod.rs`), `render_run`
  (`plan_text/mod.rs`) — plus, task 4's completeness reading found, two methods on one of them
  (`RecipePlan::wire_nodes`, `RecipePlan::bytes`, named by `corpus_gpu.rs`).
- Bare `pub` excluding `mod`: 242, 200 outside `test_support`. `pub mod`: 7. Registers: 0 / 0.
  `TEST_ONLY_ITEMS`: 8.
- `tests/common/mod.rs` re-exports the moved harness from `peacockdb_core::test_support` with
  `pub use`; `corpus.rs`, `corpus_gpu.rs`, `corpus_golden.rs`, `result_text.rs`, `cost_model.rs`
  and `corpus_cases.inc` are what remain there.
- Rust-only package: 1035 passed, 2 ignored. `--lib` 514. Inventories 1037 / 1048 / 574.

## Run log

### 2026-09-11 — plan tasks 1-2 dispatched: baselines, and the move

Board set to `building`.
