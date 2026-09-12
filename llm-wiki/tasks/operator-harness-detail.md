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

### 2026-09-12 — plan task 3 done: a leaf outside the registry emits no seq

Red first. With the arm absent, `attach_recipes` over a `GpuFilter` (and a `GpuLimit`) whose child
is `crate::tests::given::Given` panics in `emit`'s `as_node_ref`:

    panicked at peacockdb-core/src/plan/mod.rs:1125:26:
    a plan node outside the registry reached a consumer of it

Green with two edits, both in `peacockdb-core/src/wire/`:

- `attach.rs` — `emit` opens with `let Some(node_ref) = try_as_node_ref(node) else { writer.leaf();
  return Ok(None); };` and matches `node_ref`; the import swaps `as_node_ref` for
  `try_as_node_ref`. Every other arm is untouched, `attach_recipes`' signature and every other
  `wire/mod.rs` item unchanged.
- `writer.rs` — `pub(crate) fn leaf(&mut self) -> Seq` (the plan's `pub(super)`, which
  `nothing_is_pub_super` refuses): the stub, numbered by `push` and left in the pool for the parent
  to `take`. The `CudfScan`-of-nothing that `stub` built inline now comes from a private
  `scan_of_nothing() -> (Seq, offset)` shared by both, so `leaf` reads the seq from `push`'s return
  rather than `next_seq - 1`. `leaf` has a production caller (`emit`), so `wire/mod.rs`'s
  `cfg_attr(not(test), allow(dead_code))` is not what keeps it quiet. Nothing reads the returned
  seq yet; the signature is the plan's.

The two tests are at the end of `wire/tests.rs`, the `wire` component's rust-rung unit module
(`#[cfg(test)] mod tests;` — `attach_recipes` needs no FFI). They name
`crate::tests::given::{Given, columns}` by path because this file's own `Given` (a join side over
Int64 columns, with `name() = "GpuGiven"`) is a different shape and stays:
`wire::tests::a_leaf_outside_the_registry_emits_no_seq_and_its_parent_takes_a_stub` (filter over
a given leaf: `get(0)` is `None`, `get(1)` is one call targeting seq 1, `wire_nodes() == 2`) and
`wire::tests::a_seqless_operator_over_a_given_leaf_is_a_plan_of_one_stub` (limit over a given
leaf: `get(0)` is `None`, `get(1)` is the bare `SliceHandle` call, `wire_nodes() == 1`).

Results:

    cargo test … --lib -- outside_the_registry seqless_operator   2 failed (the panic above) → 2 passed
    cargo test … --lib -- wire::                                  39 passed (37 + 2)
    cargo test … --lib -- planner::tests::plan_goldens            19 passed, incl. the_payload_golden_carries_what_each_call_hands_the_executor
    testdata/goldens/recipe-payloads.txt                          sha256 61301d49…4b4e, identical to HEAD; `git status testdata/` clean; PEACOCK_REWRITE_RECIPE_BYTES unset
    cargo test --features rust-only -p peacockdb-core --lib       530 passed, 2 ignored (528 + 2), 0 warnings
    cargo test … --test test_module_layout                        17 passed
    cargo build --features rust-only -p peacockdb-core            0 warnings
    CUDF_ROOT=… scripts/cargo-cudf.sh test -p peacockdb-core --lib --features gpu --no-run   exit 0, 0 warnings
    rustfmt --check wire/{attach,writer,tests}.rs                 clean

### 2026-09-12 — plan task 4 done: `Device`, and the round trip

New: `peacockdb-core/src/tests/gpu_tests/{mod,device,harness_cases}.rs`. `src/tests/mod.rs`
declares it `#[cfg(all(test, feature = "gpu"))] mod gpu_tests;` — not the plan's
`#[cfg(feature = "gpu")]`: `a_test_module_is_named_for_its_rung` requires that literal gate on any
`mod gpu_tests`, whatever its parent's gate, so the `test` in it is redundant under `lib.rs`'s
`#[cfg(test)] mod tests;` and mandatory to the layout test. Everything inside is `pub(crate)` or
private; the facade items come from `crate::executor::{Batch, CpuBatch, GpuBatch, GpuContext,
RowRange}`, never `executor::gpu_backend::…`.

- `device.rs` — `Device { ctx: GpuContext }`: `open(node)` is `attach_recipes` → `executor_create`
  at `GPU_BUDGET` (the constant `gpu_backend/gpu_tests`' `Session::open` uses) → `begin_plan` on
  the bytes, asserting the node count the device reports equals `wire_nodes()`; `ctx()`;
  `upload(&RecordBatch) -> GpuBatch` through `to_ffi` of a `StructArray` and
  `peacock_handle_from_arrow`, priced as `CpuBatch::byte_size`; `fetch(GpuBatch, RowRange) ->
  Option<RecordBatch>` through `peacock_result_from_handle`, `None` at `len == 0`, the schema read
  off the `StreamReader` rather than the first batch (the plan's first check); `Drop` ends the plan
  and destroys the executor. `error_of` reads `peacock_last_error`.
- `harness_cases.rs` — the plan's five cases plus `the_session_holds_the_plan_of_one_stub_it_loaded`
  (`ctx().recipes` is one wire node, `get(0)` None, `get(1)` Some): `ctx()` has no caller until task
  5 and warned `never used`, and a case that reads what `open` loaded is the honest answer.

Red first: with the cases and the module declaration in place and no `device.rs`, the gpu-rung
compile fails `error[E0432]: unresolved import super::device` (and `E0583: file not found for
module device` with the declaration) — `Device` does not exist. There is no device on this box, so
the case-level red the coordinator asked for (a fetch before an upload; the clamp) cannot be run
here; the cases' first execution was the device run below, and every one passed on it.

Cases (6): `tests::gpu_tests::harness_cases::{the_session_holds_the_plan_of_one_stub_it_loaded,
an_uploaded_batch_comes_back_as_itself, a_row_range_ships_those_rows_in_order,
a_range_past_the_end_ships_nothing, a_range_over_the_end_is_clamped,
zero_rows_round_trip_as_zero_rows_under_the_schema}`. `Date32`, `Boolean`, `Utf8`, `Float64`,
`Int32`/`Int64` all round-trip through `cudf::from_arrow` and the IPC export exactly; the zero-row
export carries one zero-row batch under the schema.

shad-gpu (neighbour at 37 GiB of 143.7; no pool warning in either log):

    run 20260912T045412-244057  PCK_RUN_CPP=0 PCK_TEST_FILTER=tests::gpu_tests::harness_cases  rc 0
      peacockdb_core_gpu_lib   6 passed; 0 failed; 590 filtered out
      test_gpu_corpus          0 passed; 8 filtered out
    run 20260912T045433-244101  PCK_RUN_CPP=0, no filter (the rung whole)                       rc 0
      peacockdb_core_gpu_lib   61 passed; 0 failed; 535 filtered out   (55 + 6)
      test_gpu_corpus          8 passed; 0 failed

Results here:

    CUDF_ROOT=… scripts/cargo-cudf.sh test -p peacockdb-core --lib --features gpu --no-run   exit 0, 0 warnings
    <staged lib> --list gpu_tests:: (LD_LIBRARY_PATH per build-test.md)                      61 cases, the six above by name
    scripts/build-test-shadgpu.sh --build; --push-binaries --patch                            exit 0, 0 warnings
    cargo test --features rust-only -p peacockdb-core --lib                                   530 passed, 2 ignored (unchanged: the rust rung sees no device type)
    cargo test … --test test_module_layout / --test test_ci_coverage                          17 / 8
    rustfmt --check src/tests/mod.rs src/tests/gpu_tests/{mod,device,harness_cases}.rs        clean

`test_ci_coverage`'s guard is `each_rung_has_its_ci_line_and_the_cli_is_built`: it asserts the
staged lib binary is the one the run loop hands `rung=gpu_tests::`, and says nothing per module —
cargo's filter is a substring, so `tests::gpu_tests::harness_cases::…` is reached by that line as
it stands, and the guard stays green.

### 2026-09-12 — plan task 5 dispatched: `run_both`, and `GpuUnload` through `executors_for`

Plan tasks 1-4 are committed; the developer that carried them hands over here. What it settled
for the slices ahead: `src/tests/mod.rs` declares `#[cfg(all(test, feature = "gpu"))] mod gpu_tests;`
because `a_test_module_is_named_for_its_rung` wants the literal; facade items are named through
`crate::executor::{…}` and `crate::tests::{given,synthetic,compare}::…`, no `pub use`; every item
`pub(crate)` or private; the device proof runs as `PCK_RUN_CPP=0 PCK_TEST_FILTER=<module path>
scripts/build-test-shadgpu.sh --run-detached` then `--run-status`, after `--build --push-binaries
--patch`; the rung whole is 61 on `peacockdb_core_gpu_lib` and 8 on `test_gpu_corpus`.
