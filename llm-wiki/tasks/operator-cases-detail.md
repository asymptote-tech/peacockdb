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
and `--run-status`; the guard is `tests::gpu_tests::coverage`, `tests::gpu_tests` runs the
harness's own cases (the filter is intersected with `--list gpu_tests::`, so it selects only
`crate::tests::gpu_tests::*`), and an **empty** `PCK_TEST_FILTER` runs the rung whole (81 on
`peacockdb_core_gpu_lib` at the fork, `test_gpu_corpus` 8).

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

### 2026-09-12 — plan task 1 done: `GpuFilter`, `GpuProject`, `GpuSort`

`src/tests/gpu_tests/exec_cases.rs`, 32 cases; `mod exec_cases;` after `coverage` in
`gpu_tests/mod.rs`; the three kinds out of `PENDING` in `coverage.rs`. No production file touched;
`Given::name()` was never reached, so `given.rs` is untouched too.

**Green (25).** Filter: `a_filter_on_an_int_keeps_the_same_rows`, `a_filter_on_a_string_keeps_the_same_rows`,
`a_null_predicate_drops_the_row_on_both`, `a_filter_also_projects`, `every_row_passing_is_the_input`,
`no_row_passing_is_zero_rows_under_the_schema`, `a_filter_over_a_zero_row_batch_is_zero_rows_on_both`,
`a_zero_row_batch_between_two_with_rows_is_filtered_on_its_own`. Project: `a_column_copy_is_the_column`,
`int_arithmetic_agrees`, `float_arithmetic_agrees_bit_for_bit`, `a_cast_widens_an_int_on_both`,
`a_search_case_agrees`, `like_agrees`, `a_scalar_function_agrees`,
`a_project_over_a_zero_row_batch_is_zero_rows_on_both`,
`a_zero_row_batch_between_two_with_rows_is_projected_on_its_own`. Sort:
`an_ascending_sort_emits_the_same_order`, `a_descending_sort_emits_the_same_order`,
`nulls_first_then_by_id` (asc), `nulls_last_then_by_id` (asc), `a_fetch_keeps_the_same_top_n`,
`each_batch_is_sorted_on_its_own`, `a_sort_over_a_zero_row_batch_is_zero_rows_on_both`,
`a_zero_row_batch_between_two_with_rows_is_sorted_on_its_own`. So no exec shape reaches #173: a
zero-row batch through all three answers a zero-row batch on both.

**`bug_` (7).**
- `bug_a_cast_to_text_is_refused_on_the_device` — **#203, new**. Verbatim: `gpu refused:
  execute_node(#1 CudfProject): [in CudfProject] cast to STRING from a non-string type not
  supported in column path`. The plan's one cast case was split: the Int32→Int64 half is green.
- `bug_a_value_case_is_refused_on_the_device` — #57. Verbatim: `gpu refused: execute_node(#1
  CudfProject): [in CudfProject] value-form CASE not supported in column path`.
- `bug_a_typed_null_in_arithmetic_is_the_column_on_the_device` — #198. `i32 + NULL::Int32`: cpu all
  null, device equal to `i32` (null only at row 4, 9, … where `i32` is). Pinned against `input()`'s
  columns 0 and 2.
- `bug_a_typed_null_literal_is_a_column_of_zeros_on_the_device` — #198. A bare `NULL::Int64` in the
  select list: cpu all null, device all `0`. #198's text said the bare literal short-circuits to
  `build_scalar` and is null; that holds inside `build_column` only — `project.cpp` asks
  `is_ast_able` first, and a numeric literal is AST-able. The ticket's sentence is corrected.
- `bug_decimal_arithmetic_is_exported_at_precision_38` — #187. Verbatim: `schema differs / cpu:
  … doubled Decimal128(19, 2) … halved Decimal128(24, 6) / gpu: … doubled Decimal128(38, 2) …
  halved Decimal128(38, 6)`. Pinned as the cpu's slot with the two columns cast to precision 38:
  the values agree, so the declared `out_type` does set the device's scale and only the exported
  precision moves. Declared types read off arrow-arith 54.2.1's `decimal_op` (what DataFusion 45's
  `get_result_type` calls) and confirmed by running it: add (19,2), divide by `2.00` (24,6).
- `bug_a_descending_key_with_nulls_last_puts_them_first_on_the_device` — **#202, new**. `i32 DESC
  NULLS LAST, id`: cpu `475, 455, 454, …` then the nulls; device the twelve null rows first, then
  `475, 455, …`. Pinned against arrow's `lexsort_to_indices` with `nulls_first` flipped.
- `bug_a_descending_key_with_nulls_first_puts_them_last_on_the_device` — #202, the mirror, added
  after the first showed: `i32 DESC NULLS FIRST, id`: cpu the nulls first, device the nulls last.
  Root cause read in cuDF 25.02's `row_operators.cuh` (`device_row_comparator`, line ~648): the
  per-column `null_order` is applied inside `element_comparator` and the result is *then* flipped
  for a `DESCENDING` column, so `sort.cpp`'s `nulls_first ? BEFORE : AFTER` is right for ascending
  keys only. `node_session.cpp`'s merge maps the same way and is named in the ticket.

**Tickets.** #202 (Critical correctness) and #203 (Blockers), text in `tickets.md`; counter now 204,
Contents counts 18 and 15. #198's second paragraph corrected as above. No ticket a case here could
not reproduce: #57, #187 and #198 all reproduce.

**Runs on shad-gpu** (neighbour at 37 GiB; every pool built):
- `20260912T063144-267856` green-form, `PCK_RUN_CPP=0 PCK_TEST_FILTER=tests::gpu_tests::exec_cases`:
  `peacockdb_core_gpu_lib` 24 passed 6 failed (the six above, `nulls_last_then_by_id` then being
  the descending case); `test_gpu_corpus` 0 of 8.
- `20260912T063615-268953` after the rewrite: 31 passed 1 failed
  (`a_descending_key_with_nulls_first_then_by_id`, the mirror, green-form).
- `20260912T063659-269825` family: `peacockdb_core_gpu_lib` 32 passed 0 failed.
- `20260912T063709-269859` guard, `PCK_TEST_FILTER=tests::gpu_tests::coverage`: 1 passed. Its
  red-both-ways showing is task 8's; not repeated.
- `20260912T063718-269897` `PCK_TEST_FILTER=tests::gpu_tests` with the C++ suites: lib 58 passed
  (see the note below), C++ 12 / 6 / 27 / 4 / 4 passed.
- `20260912T063911-270825` `PCK_RUN_CPP=0`, no filter: `peacockdb_core_gpu_lib` **113 passed**
  (81 + 32), `test_gpu_corpus` 8 passed.
- Local: `cargo test --features rust-only -p peacockdb-core --lib` 530 passed, 2 ignored;
  the gpu-feature `--no-run` build 0 warnings; `rustfmt --check` clean on `exec_cases.rs` and
  `coverage.rs`.

**Note for the next dispatch.** `PCK_TEST_FILTER=tests::gpu_tests` does not run the rung whole:
`rung-args.sh` intersects the developer's filter with the rung's `--list gpu_tests::`, and only
`crate::tests::gpu_tests::*` contains that substring — 26 at the fork plus the family — while the
rung's 81 also holds `executor::…::gpu_tests::*`. The rung whole is an empty `PCK_TEST_FILTER`.
