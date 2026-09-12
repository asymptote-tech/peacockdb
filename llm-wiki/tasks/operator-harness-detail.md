# operator-harness — run detail

Spec: [`operator-harness.md`](operator-harness.md). Plan: [`operator-harness-impl.md`](operator-harness-impl.md).

## Chain position and branch

Chain `ENS-drop-mode-name`, task 8. Branch `ENS-operator-harness`, forked at `2cb92f63`, the tip of
task 6's branch — task 7 is a prototype whose branch is never merged, so task 6 is the parent that
exists. **Its PR targets `ENS-visibility`.** Task 7's report lives only on
`ENS-sink-divergence-survey` (`git show ENS-sink-divergence-survey:llm-wiki/reports/sink-divergence.md`);
this task does not need it.

## How this task is dispatched

The plan's eight tasks, one dispatch each, the coordinator committing after each: 1 the C++ symbol
and its extern; 2 the rust-rung helpers (`Given`, `synthetic`, the comparator with its red cases);
3 the `attach.rs` arm; 4 `Device` and the round trip; 5 `run_both` and `GpuUnload`; 6 `GpuLimit`;
7 the kind registry and its guard; 8 the pages and the handoff. Every task from 4 on needs
shad-gpu for its proof.

## Hosts at dispatch (2026-09-12)

Ubuntu 24.04 / glibc 2.39 box; verda's hostname does not resolve; shad-gpu up with a neighbour
at 37 GiB of 143.7; `cpp/build`, `cpp/install` and `target-cudf-rapids-cuda-12.2` warm from task
7's cycle; `target/` warm for `rust-only`; `CUDF_ROOT=/home/dmitry/data/miniforge3/envs/rapids-cuda-12.2`.

## What the plan says, and what the tree measures

The spec and plan were written before task 6 ran. Three things they say are refused by rules on
this tree, and the sanctioned form exists for each:

- The spec's "`Writer` gains one `pub(super)` method inside `wire/`" and the plan's
  `pub(super) fn leaf`: `nothing_is_pub_super` refuses `pub(super)`; the form is `pub(crate)`.
- The plan's `use crate::executor::gpu_backend::GpuBackend;` (line 1163): `gpu_backend` is `mod`,
  declared private since task 4, and its facade items are declared in `executor/mod.rs`; the path
  is `crate::executor::GpuBackend`. `GpuBackend`, `GpuBatch` and `GpuContext` are `pub(crate)`
  behind `cfg_attr(not(feature = "test-support"), allow(dead_code))` since task 6.
- The plan's comment at line 1274 cites `post_order_of_every_node`, which task 6 deleted;
  `PlanIndex::build` and `index.nodes[i].post_order` are what a test reads now.
- Task 6 pinned the crate surface: `SURFACE` in `test_module_layout/visibility.rs` refuses any new
  bare `pub` in `src/` outside `test_support`, and `pub mod` outside `lib.rs`. Everything this task
  adds under `src/tests/` is `pub(crate)` or private; the FFI extern lives in `peacockdb-ffi`,
  outside that crate. `TEST_ONLY_ITEMS` (`test_module_layout/test_code.rs`) is the register for an
  item that must keep a `#[cfg(test)]` outside a test path.
- The layout test's test-code rules: a test module is named for the rung it needs
  (`gpu_tests` for the device) and gated for it; `src/tests/gpu_tests/` is that shape.
- Baselines that hold: rust-only package 1036 passed / 2 ignored (`--lib` 517 after task 7's
  three, but this branch forks before task 7: 514 + 2 ignored), `test_module_layout` 17,
  `test_ci_coverage` 8; shad-gpu `peacockdb_core_gpu_lib` 55 under `gpu_tests::`, `test_gpu_corpus`
  8, C++ 52.

## Run log

### 2026-09-12 — plan task 1 dispatched: the device adopts an Arrow batch

Board set to `building`.

### 2026-09-12 — plan task 1 done: the device adopts an Arrow batch

Both facts the plan rests on held: `cudf::from_arrow` at `gpu_executor.cpp:340` inside
`peacock_spark_partition_ids`; handles allocated inline as `impl_->next_handle++` then
`impl_->registry.emplace` (`node_session.cpp:244,335,423,466,507,540`); the session owned by
`struct peacock_executor` (`gpu_executor.cpp:35`). No `HandleRegistry` type has appeared, so
`adopt` went on `NodeSession`. `TableResult` still carries only `table` and `column_names` — no
per-column precision to fill from the decimal format string, so that plan clause did not apply.

- `int peacock_handle_from_arrow(peacock_executor_t* executor, const void* schema, const void*
  array, uint64_t* out_handle)` — declared after the conformance hook in
  `cpp/include/peacock_gpu.h`, defined at the end of `cpp/src/gpu_executor.cpp`. Null argument
  → 1 with no message; no session → 1 with `no plan loaded (call peacock_executor_begin_plan
  first)` in `peacock_last_error`; a failed import → 1 with the exception's message and the
  session left standing. Success → 0 and the handle.
- `uint64_t NodeSession::adopt(TableResult result)` — declared after `slice_handle` in
  `cpp/src/plan_executor.h`, defined after `slice_handle`'s definition in `cpp/src/node_session.cpp`.
- The extern in `peacockdb-ffi/src/lib.rs`, directly after `peacock_spark_partition_ids` inside
  `unsafe extern "C"`. It is gated by the enclosing `pub mod raw`'s
  `#[cfg(not(feature = "rust-only"))]`; no per-item gate.
- No `AbiSymbol` names it; `peacockdb-core/src/` is untouched.
- One test beyond the plan's five files: `PeacockGpu.HandleFromArrowNeedsASession` in
  `cpp/tests/cpu/test_executor.cpp`, beside `ExecutorNullOut` — the null-output and no-session
  refusals, which run under `ctest -L cpu` with no device. Red first (`'peacock_handle_from_arrow'
  was not declared in this scope`), green after. The adoption itself needs a live session and a
  cuDF table, so its proof is plan task 4's round trip on shad-gpu.
- `git clang-format` refuses an unstaged tree, so `clang-format --lines=<range>` was run over
  exactly the changed ranges of the five C++ files; the two signatures took the 100-column wrap.

Results:

    ctest --test-dir cpp/build -L cpu            100% tests passed, 0 tests failed out of 1 (12 gtests, was 11)
    scripts/build-test-shadgpu.sh --build        exit 0, 0 warnings; staged test_gpu_corpus and peacockdb_core_gpu_lib
    nm -D target-cudf-rapids-cuda-12.2/debug/build/peacockdb-ffi-c2ff8be8e892f44d/out/lib/libpeacock_gpu.so | grep handle_from_arrow
                                                 0000000000052ab0 T peacock_handle_from_arrow
    nm -D cpp/install/lib/libpeacock_gpu.so      0000000000046af0 T peacock_handle_from_arrow
    cargo build --features rust-only -p peacockdb-core                        exit 0, 0 warnings
    CUDF_ROOT=… scripts/cargo-cudf.sh build -p peacockdb-core --features gpu  exit 0, 0 warnings
    rustfmt --check --edition 2024 peacockdb-ffi/src/lib.rs                   clean

For plan task 8: `cpp/tests/cpu/test_executor.cpp` gained one gtest, so `build-test.md`'s "C++
CPU/FFI unit" row is 12 and the header's C++ figure moves by one.
