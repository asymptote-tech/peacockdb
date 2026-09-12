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

### 2026-09-12 — plan task 2 done: `Given`, `synthetic`, the comparator

Three new leaves under `peacockdb-core/src/tests/` — `given.rs`, `synthetic.rs`, `compare.rs` —
declared in `src/tests/mod.rs` as `pub(crate) mod` beside `injection` and `executor_cases`, and
named by path (`crate::tests::given::Given`, `crate::tests::synthetic::synthetic`, …). Not the
plan's `pub(crate) use` re-exports: `nothing_re_exports_with_pub_use` refuses the literal `pub use`
and its reason — the item declared in one place and named from another — covers the crate-wide
form too. `lib.rs` gates `mod tests` on `#[cfg(test)]`, so none of the three carries a gate of
its own, and every item is `pub(crate)` or private.

- **The lift.** `Given` (private `kind`; `of(schema, batches)`, `of_columns(fields)`,
  `with_layout(schema, layout)`) and `columns(fields) -> Schema` are the cpu executor tests'
  `Given`/`schema_of`, moved. In `executor/cpu_backend/tests/` the copies are gone: `Given::of(&X)`
  → `Given::of_columns(&X)`, `Given::of_schema(s)` → `Given::of(s, BatchLayout::MultipleBatches)`,
  `schema_of(` → `columns(`; the three local helpers that built `Given { kind: … }` by struct
  literal (`backend.rs` `given`/`given_schema`, `join.rs` `side`) now delegate to `Given::of`. Four
  bindings named `columns` would have shadowed the function and were renamed (`fields` in
  `state_of`, `given`, `side`, `source.rs`; `filter_columns` at `join.rs`'s three
  `let (filter, …)` sites). The `Given`s in `wire/tests.rs` and `plan/tests/mod.rs` are untouched:
  neither has the three-method shape. `executor::cpu_backend::tests::` is 65 passed before and 65
  after, 0 warnings.
- **`synthetic.rs`**: `schema()`, `synthetic(rows, seed)`, `decimals(rows, seed)`, `prefixed`, as
  the plan wrote them (`from_iter` over `Option` builds every array type on the pinned arrow).
  Red first — `cannot find function synthetic/decimals/prefixed` — then 5 green.
- **`compare.rs`**: `Slot`, `Order`, `same`, `assert_same`, as the plan wrote them plus a
  `names_and_types` free function. Red first — `cannot find function same`, `undeclared type
  Order` (22 errors) — then green. `the_asserting_form_panics_with_the_reason` (`should_panic`)
  is beyond the plan's list: `assert_same` has no caller until task 4 and warned `never used`;
  this test was written after the wrapper, since the wrapper is three lines around `same`.

New cases (14): `tests::synthetic::{a_synthetic_batch_is_the_same_batch_twice,
every_column_but_id_carries_a_null_and_id_carries_none,
zero_rows_is_a_batch_with_the_schema_and_nothing_else, decimals_carry_an_id_and_a_decimal_with_nulls,
prefixing_renames_every_column_and_touches_no_value}`; `tests::compare::{
a_slot_whose_value_differs_is_named_with_the_slot,
a_slot_both_sides_left_empty_is_equal_and_one_side_empty_is_named, nullability_alone_is_not_a_difference,
a_slot_whose_type_differs_fails_before_any_row_is_read, a_differing_row_count_is_named_as_a_count,
a_differing_slot_count_is_named_before_any_slot_is_compared, any_order_sorts_and_as_emitted_does_not,
a_slot_of_several_batches_is_one_table, the_asserting_form_panics_with_the_reason}`.

The comparator's messages, as `same` produces them (read once with a throwaway printing test,
removed):

    value    slot 0: rows differ\n  cpu:\n<pretty table>\n  gpu:\n<pretty table>
    type     slot 0: schema differs\n  cpu: Field { name: "i32", data_type: Int32, … }\n  gpu: … data_type: Int64 …
    rows     slot 0: cpu has 8 rows, gpu 9 rows
    slots    cpu produced 2 slots, gpu 1 slot
    gpu none slot 0: gpu produced no batch, cpu 4 rows
    cpu none slot 0: cpu produced no batch, gpu 0 rows
    assert   panic: cpu and gpu differ: slot 0: cpu has 8 rows, gpu 9 rows

Results:

    cargo test --features rust-only -p peacockdb-core --lib                          528 passed, 2 ignored (514 + 14), 0 warnings
    cargo test --features rust-only -p peacockdb-core --lib -- executor::cpu_backend::tests   65 before, 65 after
    cargo test --features rust-only -p peacockdb-core --test test_module_layout      17 passed
    cargo test --features rust-only -p peacockdb-core --test test_ci_coverage        8 passed
    cargo build --features rust-only -p peacockdb-core                               0 warnings (a non-test build never sees src/tests/)
    CUDF_ROOT=… scripts/cargo-cudf.sh test -p peacockdb-core --lib --features gpu --no-run   exit 0, 0 warnings
    rustfmt --check on src/tests/{given,synthetic,compare,mod}.rs and cpu_backend/tests/*.rs  clean
