# operator-cases — run detail

Spec: [`operator-cases.md`](operator-cases.md). Plan: [`operator-cases-impl.md`](operator-cases-impl.md).

## Chain position and branch

Chain `ENS-drop-mode-name`, task 9. Branch `ENS-operator-cases`, forked at `35694e6a`, the tip of
task 8's branch. **Its PR targets `ENS-operator-harness`.**

## How this task is dispatched

The plan's eight tasks, one family per dispatch, the coordinator committing after each: 1 exec
(filter, project, sort); 2 the aggregates; 3 the accumulators; 4 the emitter; 5 the hash join;
6 cross and nested-loop; 7 the scan; 8 `PENDING` goes, the pages, the handoff. Every family needs
shad-gpu, and a case that diverges is run green-form first, read, ticketed, then pinned as `bug_`.

## Hosts at dispatch (2026-09-12)

Ubuntu 24.04 / glibc 2.39 box; verda's hostname does not resolve; shad-gpu up with a neighbour
at 37 GiB of 143.7; `cpp/build`, `cpp/install` and `target-cudf-rapids-cuda-12.2` warm from task
8's cycle; `target/` warm for `rust-only`; `CUDF_ROOT=/home/dmitry/data/miniforge3/envs/rapids-cuda-12.2`.
The device recipe task 8 settled: `scripts/build-test-shadgpu.sh --build`, `--push-binaries`,
`--patch`, then `PCK_RUN_CPP=0 PCK_TEST_FILTER=tests::gpu_tests::<family>_cases … --run-detached`
and `--run-status`; the guard is `tests::gpu_tests::coverage`, and `tests::gpu_tests` runs the
rung whole (81 on `peacockdb_core_gpu_lib` at the fork, `test_gpu_corpus` 8).

## What the plan says, and what the tree has

- **Paths.** The plan's file opening imports `crate::tests::{columns, decimals, prefixed,
  synthetic, Given, Order}`; task 8 declined `pub use` re-exports (`nothing_re_exports_with_pub_use`),
  so the names are `crate::tests::given::{Given, columns}`, `crate::tests::synthetic::{synthetic,
  decimals, prefixed, schema}`, `crate::tests::compare::Order`, and `super::script::{Script,
  run_both, Outcome}`; `operator_case!` is visible to any module declared after `coverage` in
  `gpu_tests/mod.rs`. Facade items via `crate::executor::{…}`; every item `pub(crate)` or private.
- **The fixture** is as the plan says: `synthetic` ordinals `0 id Int64`, `1 key Int32`, `2 i32`,
  `3 i64`, `4 f64`, `5 s Utf8`, `6 d Date32`, `7 b Boolean`, null in every column but `id`;
  `decimals` `0 id`, `1 dec Decimal128(18, 2)`.
- **Two notes from task 8's completeness readings.** The lifted `Given` does not override
  `GpuNode::name()`, so the first error message or render over one shows the registry panic
  ("a plan node outside the registry reached a consumer of it") instead of a name — one line in
  `src/tests/given.rs` when first reached, which is test code and inside this task's scope. And
  `Script`'s `Lanes`, `Emit`, `Join` and `Source` arms have compiled but never run on a device;
  the first family to drive each is the first to prove its arm — a failure there is a finding
  against the harness task, recorded here, not a helper added.
- **`Script` carries `#[allow(dead_code)]`** for those four variants; it leaves with the family
  that constructs the last of them.
- **A limit cannot reach #173**; task 8 named the shapes that can: an accumulator's done over
  nothing, a merge lane of only `Done`, a join side of zero rows, an exec over a zero-row batch.
- **Tickets** take the next free number after #201 (`tickets.md`'s header counter).

## Run log

### 2026-09-12 — plan task 1 dispatched: `GpuFilter`, `GpuProject`, `GpuSort`

Board set to `building`.
