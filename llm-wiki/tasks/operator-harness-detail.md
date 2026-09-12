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

### 2026-09-12 — plan task 5 done: `run_both`, and `GpuUnload` through `executors_for`

New: `peacockdb-core/src/tests/gpu_tests/script.rs`, declared `mod script;` in `gpu_tests/mod.rs`.
`Script` (the plan's seven variants), `Outcome { cpu, gpu }` with `same(Order)`, `gpu_refuses()`
and `cpu_refuses()`, `run_both(node, Script) -> Outcome`, and the private generic `drive<B:
Backend>` with one arm per category — as the plan wrote them, with three adjustments:

- `GpuBackend` and `CpuBackend` come from `crate::executor::{…}` (the plan's
  `executor::gpu_backend::GpuBackend` path is behind a `mod` wall); `Slot`, `Order`, `same` from
  `crate::tests::compare`.
- The root's post-order is counted by a local `size(node) - 1`, as the plan does. The comment cites
  `PlanIndex::build` instead of the deleted `post_order_of_every_node`: `build` → `index::walk` asks
  `category_of` of every node, and `category_of` → `as_node_ref` panics on a `Given` leaf, so the
  index cannot be read for a tree with a stub in it.
- `Script` carries one `#[allow(dead_code)]` with its reason above it: only `Exec` (the refusal
  case) and `Unload` are constructed here; `GpuLimit` (plan task 6) constructs `Accumulate` and
  `operator-cases.md` the rest, and the attribute leaves with them. Precedent: the per-variant
  allows on `NodeRef` (`plan/mod.rs`). The alternative — five arms deleted and re-added by later
  tasks — would have left `drive` a two-arm match named as the harness.

The refusal: `run_both` opens with `assert_eq!(category_of(node), script.category(), "the script's
shape is not the node's category")`, before the cpu drive and before `Device::open`, so the check
runs with no device. Its message, as a wrong-shaped script over an unload produces it:

    assertion `left == right` failed: the script's shape is not the node's category
      left: Unload
     right: Exec

Red first, on this box with the staged gpu binary (`LD_LIBRARY_PATH` per build-test.md): with
`run_both` written without the check, `a_script_of_another_shape_is_refused_before_either_backend_runs`
failed on `drive`'s catch-all — `executors_for answered Unload for a script of another shape` —
and went green with the assert added. The two `Outcome` tests in `script.rs`
(`a_one_sided_refusal_is_read_by_its_message`, `same_names_the_side_that_refused`) were written
in the same edit as the three-line accessors they exercise, on task 2's precedent for
`the_asserting_form_panics_with_the_reason`; they build an `Outcome` by hand and need no device.

Cases (9): `tests::gpu_tests::harness_cases::{a_script_of_another_shape_is_refused_before_either_backend_runs,
an_unload_hands_the_whole_batch_over_on_both_backends, an_unload_over_a_range_hands_those_rows_over,
an_unload_clamps_a_range_over_the_end_the_same_way, an_unload_of_a_range_past_the_end_is_zero_rows_on_both,
an_unload_of_a_zero_row_batch_is_zero_rows_under_the_schema_on_both,
a_range_over_a_zero_row_batch_is_zero_rows_on_both}` and `tests::gpu_tests::script::{
a_one_sided_refusal_is_read_by_its_message, same_names_the_side_that_refused}`. Each unload case
builds `GpuUnload::new(Given::of(synthetic schema, MultipleBatches), None)` over `synthetic(rows, 2)`
and runs `Script::Unload { batch, rows }` through `executors_for` on both backends — `CpuUnload`'s
`covers`/`clamp` slice against `GpuExport`'s `peacock_result_from_handle` range — then
`Outcome::same(Order::AsEmitted)`.

**No divergence and no one-sided failure.** Every unload case agreed on the first device run: the
whole batch, a range inside it, a range clamped at the end, a range past the end (0 rows on both,
under the declared schema — `GpuExport` builds `RecordBatch::new_empty` at `len == 0` and
`CpuUnload` slices to nothing), a zero-row batch, and a range over a zero-row batch. So no ticket
and no `bug_` test from this task; `gpu_refuses`/`cpu_refuses` have their hand-built test and no
device caller yet.

shad-gpu (neighbour at 37 GiB of 143.7; no `rmm` line in either gate log):

    run 20260912T050223-246916  PCK_RUN_CPP=0 PCK_TEST_FILTER=tests::gpu_tests  rc 0
      peacockdb_core_gpu_lib   15 passed; 0 failed; 590 filtered out   (task 4's 6 + these 9)
      test_gpu_corpus          0 passed; 8 filtered out
    run 20260912T050241-246978  PCK_RUN_CPP=0, no filter (the rung whole)      rc 0
      peacockdb_core_gpu_lib   70 passed; 0 failed; 535 filtered out   (61 + 9)
      test_gpu_corpus          8 passed; 0 failed

Results here:

    CUDF_ROOT=… scripts/cargo-cudf.sh test -p peacockdb-core --lib --features gpu --no-run   exit 0, 0 warnings
    <staged lib> --list gpu_tests::                                                          70 cases, the nine above by name
    <staged lib> gpu_tests::harness_cases::a_script_of_another_shape gpu_tests::script::     3 passed on this box (no device touched)
    scripts/build-test-shadgpu.sh --build; --push-binaries --patch                            exit 0, 0 warnings
    cargo test --features rust-only -p peacockdb-core --lib                                   530 passed, 2 ignored, 0 warnings (unchanged)
    cargo test … --test test_module_layout / --test test_ci_coverage                          17 / 8
    rustfmt --check --edition 2024 src/tests/gpu_tests/{mod,script,harness_cases}.rs          clean

For plan task 8: the gpu block's `--lib -- gpu_tests::` figure is 70 after this task (55 + 6 + 9).

### 2026-09-12 — plan task 6 done: `GpuLimit`

`harness_cases.rs` gains `limit_over(skip, fetch, rows, batches)` and the plan's nine cases as
written, each a `GpuLimit::new(Given::of(synthetic schema, MultipleBatches), RowInterval { skip,
fetch })` over a stream of `synthetic(rows, 10 + i)` batches, run as `Script::Accumulate(stream)`
through `executors_for` on both backends — one slot per `accumulate_and_fetch` and one for
`mark_done_and_fetch` — then `Outcome::same(Order::AsEmitted)`. `Script::Accumulate` has its first
constructor; the `#[allow(dead_code)]` on `Script` is on the enum, not per variant, so it stays for
`Lanes`, `Emit`, `Join` and `Source`, and its reason now names `operator-cases.md` alone.

Cases (9): `tests::gpu_tests::harness_cases::{an_interval_inside_one_batch_slices_that_batch,
an_interval_straddling_two_batches_slices_both, batches_entirely_outside_the_interval_produce_nothing,
a_skip_alone_drops_the_prefix_and_keeps_the_rest, a_stream_of_several_batches_is_cut_at_the_same_two_edges,
a_stream_of_one_zero_row_batch_answers_nothing_on_both, a_zero_row_batch_inside_a_stream_counts_no_rows_on_both,
a_stream_of_nothing_but_zero_row_batches_answers_nothing_on_both,
an_interval_no_batch_reaches_answers_nothing_on_both}`.

**No divergence and no one-sided failure**; every case agreed on its first device run, so no ticket
and no `bug_` test. Two things worth knowing for the operators ahead:

- Both `LimitStream`s (`cpu_backend/accumulate.rs`, `gpu_backend/accumulate.rs`) take their range
  from the one `RowInterval::range_of`, so the two edges cannot disagree by construction; what the
  device side proves is `slice_handle` over the range and the release of an uncalled batch, and
  that a slot both sides leave empty compares equal.
- **#173's shape is not reachable through a limit.** `range_of` answers `None` for a zero-row batch
  (`start < stop` is false at `n_rows == 0`) and never a zero-length range, so a zero-row input is
  released uncalled on both sides and the device is never asked to slice or ship an empty table.
  Every zero-row route here — one zero-row batch, one inside a stream, nothing but zero-row batches
  — ends in an empty slot on both, not a zero-row batch. The route to #173 is an operator that
  *calls* the device on nothing: an accumulator's done over no arrivals, a join side with no rows.

No red reachable on this box beyond the compile of the new names: the cpu drive of every case
passes here (it runs before `Device::open`), and the device is where the comparison happens.

shad-gpu (neighbour at 37 GiB of 143.7; no `rmm` line in either gate log):

    run 20260912T050618-248271  PCK_RUN_CPP=0 PCK_TEST_FILTER=tests::gpu_tests  rc 0
      peacockdb_core_gpu_lib   24 passed; 0 failed; 590 filtered out   (15 + these 9)
      test_gpu_corpus          0 passed; 8 filtered out
    run 20260912T050631-248316  PCK_RUN_CPP=0, no filter (the rung whole)      rc 0
      peacockdb_core_gpu_lib   79 passed; 0 failed; 535 filtered out   (70 + 9)
      test_gpu_corpus          8 passed; 0 failed

Results here:

    CUDF_ROOT=… scripts/cargo-cudf.sh test -p peacockdb-core --lib --features gpu --no-run   exit 0, 0 warnings
    <staged lib> --list gpu_tests::                                                          79 cases, the nine above by name
    scripts/build-test-shadgpu.sh --build; --push-binaries --patch                            exit 0, 0 warnings
    cargo test --features rust-only -p peacockdb-core --lib                                   530 passed, 2 ignored (unchanged)
    cargo test … --test test_module_layout / --test test_ci_coverage                          17 / 8
    rustfmt --check --edition 2024 src/tests/gpu_tests/{script,harness_cases}.rs              clean

For plan task 8: the gpu block's `--lib -- gpu_tests::` figure is 79 after this task.

### 2026-09-12 — plan task 7 done: the kind registry and its guard

New: `peacockdb-core/src/tests/gpu_tests/coverage.rs`, declared `#[macro_use] mod coverage;` first in
`gpu_tests/mod.rs` so `operator_case!` reaches `harness_cases`. `Covers { kind, case }` under
`inventory::collect!`; `operator_case! { Kind, fn name() { … } }` submits the entry and defines the
`#[test]` from one declaration; `EXCLUDED` (the three forwarders, permanent); `PENDING`; and the
guard `every_kind_has_a_case_or_is_named_as_pending_or_excluded`. All as the plan wrote them, with
one addition: the `unknown` check reads `Covers::case` and reports `(kind, case)`, so a mistyped
kind is found by the case that wrote it (and the field has a reader).

**Where it runs.** `inventory` collects per binary and the entries are submitted by gpu-rung
cases, so the guard is gpu-rung too: in a rust-only binary the registry would be empty and every
kind "missing". No rust-rung half exists. The guard itself touches no device, so it runs on this
box against the staged binary — which is how the reds below were taken. Its oracle for "every
kind" is `crate::tests::rebuild::every_kind` (hand-built nodes over fake parquet paths, nothing
read), which `a_node_rebuilt_over_its_own_children_is_the_node_it_was` holds to eighteen in the
rust rung — so a nineteenth `NodeRef` kind reaches this guard through that fixture.

**Converted:** the six `GpuUnload` and nine `GpuLimit` cases. **Plain `#[test]` and why:** the
five round-trip cases and `the_session_holds_the_plan_of_one_stub_it_loaded` exercise `Device`,
not a node kind; `a_script_of_another_shape_is_refused_before_either_backend_runs` exercises the
harness's refusal (and is `should_panic`, which the macro does not take). A comment at the unload
block says so.

**PENDING as it stands** (18 kinds − 3 excluded − `GpuUnload`, `GpuLimit` = 13): `GpuLoadParquet`,
`GpuFilter`, `GpuProject`, `GpuSort`, `GpuCoalesceAllBatches`, `GpuAccumulateBatchesAndSort`,
`GpuAggregate`, `GpuAggregateBatches`, `GpuHashJoin`, `GpuCrossJoin`, `GpuNestedLoopJoin`,
`GpuEmitPartitions`, `GpuMergeSortedPartitions`.

Red first, on this box with the staged gpu binary. The guard was written before any case was
converted, so the registry was empty:

    kinds with no case and not pending: ["GpuLimit", "GpuUnload"]

then green after the conversion. Then each check shown red by one edit, quoted, and the file
restored (`diff` identical) and re-run green:

    (a) "GpuLoadParquet" removed from PENDING     kinds with no case and not pending: ["GpuLoadParquet"]
    (b) "GpuLimit" added to PENDING               listed as pending or excluded, but has a case: ["GpuLimit"]
    (c) "GpuUnion" removed from EXCLUDED          kinds with no case and not pending: ["GpuUnion"]
    (d) "GpuWindow" added to PENDING              listed, but not a kind: ["GpuWindow"]
    (e) one limit case declared as GpuWindow      cases naming a kind that is not one, as (kind, case):
                                                  [("GpuWindow", "an_interval_no_batch_reaches_answers_nothing_on_both")]

shad-gpu (neighbour at 37 GiB of 143.7; no `rmm` line in either gate log):

    run 20260912T051321-252945  PCK_RUN_CPP=0 PCK_TEST_FILTER=tests::gpu_tests  rc 0
      peacockdb_core_gpu_lib   25 passed; 0 failed; 590 filtered out   (24 + the guard)
      test_gpu_corpus          0 passed; 8 filtered out
    run 20260912T051332-252989  PCK_RUN_CPP=0, no filter (the rung whole)      rc 0
      peacockdb_core_gpu_lib   80 passed; 0 failed; 535 filtered out   (79 + 1)
      test_gpu_corpus          8 passed; 0 failed

Results here:

    CUDF_ROOT=… scripts/cargo-cudf.sh test -p peacockdb-core --lib --features gpu --no-run   exit 0, 0 warnings
    <staged lib> --list gpu_tests::                                                          80 cases, tests::gpu_tests::coverage::every_kind_has_a_case_or_is_named_as_pending_or_excluded among them
    scripts/build-test-shadgpu.sh --build; --push-binaries --patch                            exit 0, 0 warnings
    cargo test --features rust-only -p peacockdb-core --lib                                   530 passed, 2 ignored (unchanged)
    cargo test … --test test_module_layout / --test test_ci_coverage                          17 / 8
    rustfmt --check --edition 2024 src/tests/gpu_tests/{mod,coverage,harness_cases}.rs        clean

For plan task 8: the gpu block's `--lib -- gpu_tests::` figure is 80 after this task; the
harness's cases are 15 operator cases + 6 device/harness cases + 2 `Outcome` tests + the guard.

### 2026-09-12 — plan task 8 done: the whole proof, and what operator-cases inherits

HEAD `bd2758a4`; the pages are the coordinator's (`build-test.md`: the "Harness helpers" row at 14 and
the "Operator harness" row at 25, cpu `--lib` 532, gpu `gpu_tests::` 80, C++ 67, grand total 1620;
`architecture.md`: seventeen symbols). Every line below is from a run at that HEAD.

    cargo test --features rust-only -p peacockdb-core --test test_ci_coverage           8 passed — the guard without a workflow edit
    cargo test --features rust-only -p peacockdb-core --test test_module_layout         17 passed
    rustfmt --check --edition 2024 <the 21 files under src/tests/, src/wire/, cpu_backend/tests/ and peacockdb-ffi/src/lib.rs the branch touched since 2cb92f63>
                                                                                        clean; none needed a format
    cargo test --features rust-only -p peacockdb-core -- --test-threads=2               1052 passed, 0 failed, 2 ignored; 9 result lines; 0 warnings; exit 0
      --lib 530 + 2i · test_ci_coverage 8 · test_corpus_goldens 20 · test_cost_model 3 · test_cpu_corpus 448
      · test_golden_format 26 · test_gpu_corpus 0 · test_module_layout 17 · doc 0
    sha256sum -c llm-wiki/tasks/visibility-baselines/goldens.sha256                     170 OK, 0 otherwise; `git status testdata/` clean (recipe-payloads.txt 61301d49…4b4e)
    CUDF_ROOT=… scripts/cargo-cudf.sh test -p peacockdb-core --no-run                    exit 0, 0 warnings
    CUDF_ROOT=… scripts/cargo-cudf.sh test -p peacockdb-core --lib --features gpu --no-run  exit 0, 0 warnings
    ctest --test-dir cpp/build -L cpu                                                   100% passed, 1 of 1; 12 gtests from 5 suites (HandleFromArrowNeedsASession among them)
    scripts/build-test-shadgpu.sh --build; --push-binaries; --patch                     exit 0 each, 0 warnings
    scripts/build-test-shadgpu.sh --run-detached (no filter, PCK_RUN_CPP unset); --run-status
      run 20260912T052510-260094  rc 0  "GPU test run OK"; neighbour at 37 GiB of 143.7
        peacock_cpu_tests 12 · peacock_gpu_tests 6 · peacock_plan_tests 27 · peacock_tpch_tests 4 · peacock_tpchv_tests 4  = 53 C++
          (the dispatch's 52 predates plan task 1's twelfth cpu gtest; the page's C++ row says 12)
        four `[rmm] pool on a discrete device` lines: 1.0, 1.0, 69.0, 30.0 GiB reserved of 103.0 free; no "could not be built"
        peacockdb_core_gpu_lib   80 passed; 0 failed; 535 filtered out
        test_gpu_corpus          8 passed; 0 failed

#### For `operator-cases.md`'s developer

- **No decimal reached the device in this task.** `decimals(rows, seed)` exists (`Decimal128(18, 2)`
  with nulls, beside an `id`) and its only caller is `synthetic.rs`'s own
  `decimals_carry_an_id_and_a_decimal_with_nulls`. Every device case here ran `synthetic`, whose
  schema is Int32, Int64, Float64, Utf8, Date32, Boolean, `key`, `id` — all of which round-trip
  through `cudf::from_arrow` and the IPC export exactly, at zero rows too. The first decimal case
  is expected to be #187's `bug_` test (the device exports every decimal at precision 38), pinned
  through `Outcome::same` failing on `schema differs` — not a cast in the comparator.
- **Arrow constructors on the pinned version** (`arrow 54.2.1` via `datafusion 45`):
  `Int64Array::from_iter_values`, `Int32Array/Int64Array/Float64Array/StringArray/Date32Array/
  BooleanArray::from_iter` over `Option<T>` (nulls in every column but `id`),
  `Decimal128Array::from_iter(...).with_precision_and_scale(18, 2)`; `RecordBatch::try_new`,
  `RecordBatch::new_empty`; `concat_batches`, `lexsort_to_indices`, `take_record_batch`, `cast`;
  `StructArray::from(Vec<(Arc<Field>, ArrayRef)>)` + `to_ffi` for the upload, `StreamReader` for
  the fetch.
- **No case became a `bug_` test.** Every unload and limit case agreed on its first device run. Had
  one diverged: the same defect → the existing ticket (#187 for a decimal's precision, #173 for a
  table from nothing, #183/#191 for the other known type divergences); a new defect → a new
  ticket, at most fifteen lines in `tickets.md`; and a `bug_<what it does wrong>` test asserting the
  wrong slot (or `outcome.gpu_refuses()`'s message) with the ticket number in a comment above it.
- **The guard was shown red five ways** (task 7's entry quotes each): the empty registry before any
  conversion; a kind removed from `PENDING`; a covered kind added to `PENDING`; a forwarder removed
  from `EXCLUDED`; a non-kind on `PENDING`; a case declaring a non-kind. All on this box against the
  staged gpu binary — the guard touches no device.
- **The thirteen pending kinds:** `GpuLoadParquet`, `GpuFilter`, `GpuProject`, `GpuSort`,
  `GpuCoalesceAllBatches`, `GpuAccumulateBatchesAndSort`, `GpuAggregate`, `GpuAggregateBatches`,
  `GpuHashJoin`, `GpuCrossJoin`, `GpuNestedLoopJoin`, `GpuEmitPartitions`,
  `GpuMergeSortedPartitions`. A case for one of them is written as `operator_case! { Kind, fn … }`
  and the kind leaves `PENDING` in the same change, or the guard's `stale` check goes red; when the
  list is empty, delete it and the check that reads it.
- **`Script` variants still unconstructed:** `Lanes`, `Emit`, `Join`, `Source` (`Exec` is constructed
  only by the refusal case, so its arm has not run on a device either). The `#[allow(dead_code)]`
  on `Script` leaves with them. A `Source` script needs a `GpuLoadParquet` over a parquet the test
  writes — the cpu executor tests' `source.rs` is the pattern — and is the one case shape whose
  "synthetic data only" is a file rather than a `RecordBatch`.
- **Why a limit cannot reach #173, and which shapes can.** `RowInterval::range_of` answers `None` at
  `n_rows == 0` and never a zero-length range, so a zero-row batch is released uncalled on both
  sides and the device is never asked to slice or ship an empty table; a zero-row route through a
  limit ends in an empty slot, which the comparator calls equal. #173's shape is an operator that
  *calls* the device on nothing: an accumulator's `mark_done_and_fetch` over no arrivals (coalesce,
  the accumulating sort, the state merge), a merge lane that saw only `Done`, a join whose build or
  probe side is a zero-row batch, an exec handed a zero-row batch. Each is its own case, and a
  one-sided `Err` there is `outcome.gpu_refuses()` pinned by message under #173.
- **The device-run recipe**, each a foreground call under `timeout`, with `CUDF_ROOT` exported:
  1. `scripts/cargo-cudf.sh test -p peacockdb-core --lib --features gpu --no-run` — 0 warnings.
  2. `--list gpu_tests::` on `target-cudf-rapids-cuda-12.2/debug/deps/peacockdb_core-<hash>` with
     `LD_LIBRARY_PATH=<target-cudf-…/debug/build/peacockdb-ffi-*/out/lib>:$CUDF_ROOT/lib` — the
     new names; the same binary runs any `gpu_tests::` case that touches no device (the guard, the
     refusal, the `Outcome` tests) right here.
  3. `scripts/build-test-shadgpu.sh --build`, then `--push-binaries --patch`.
  4. `PCK_RUN_CPP=0 PCK_TEST_FILTER=tests::gpu_tests scripts/build-test-shadgpu.sh --run-detached`,
     then `--run-status` polled every 20–30 s until `FINISHED`; the per-case lines are in
     `shad-gpu:/home/info/peacockdb/.run-state/gate.log`.
  5. Once more with no filter for the rung whole (`PCK_RUN_CPP=0`), and at a handoff with
     `PCK_RUN_CPP` unset for the C++ too. Check `nvidia-smi` on the host first; a
     `[rmm] pool of N GiB could not be built` line at the top of a C++ binary's log is #178, not a
     bug in the binary.

`git status --short`: only this file.

### 2026-09-12 — reviewing: PR #149 against `ENS-visibility`

Eight slices committed; the last entry is the whole proof on `bd2758a4`. Reviewer round 1 next.

### 2026-09-12 — review round 1: 0 blocking, 0 important, 2 nits — completing

The reviewer read the diff as one change: both production edits are the sanctioned pair and
nothing else moved; the symbol is test-only by contract and its failure paths leave the session
standing; the new arm is unreachable from a production plan (every registry kind resolves
through `try_as_node_ref`); the harness's red cases fail by name; `Device::Drop` leaks no session
or handle on a failed case; the kind guard modelled red all five ways with no bypass constructed
(`3 excluded + 13 pending + 2 covered = 18`, the oracle pinned elsewhere); counts reconcile to the
pages; comment caps counted. Nits: the `Script` attribute's comment said `Exec` was not
constructed here (it is, by the refusal) — fixed by the coordinator; `Writer::leaf` returns a
seq nobody reads — kept, the plan's signature. Board to `completing`; the completeness pass is
two readings dispatched together.

### 2026-09-12 — completeness pass, the analyst's reading

Branch `ENS-operator-harness` at `1c27cec6` read as one diff against `ENS-visibility` (`2cb92f63`),
30 files, asked what it does not contain. The three commits after the proof at `bd2758a4` touch
the wiki and one doc comment, so the proof stands at HEAD.

**0 blocking, 1 important.** No sentence in `architecture.md` is falsified.

#### The one important finding

**`GpuLimit` over no batch at all is missing.** The spec's "Empty inputs are separate cases
everywhere, here and in task 9, one per shape — a zero-row batch, no batch at all, one side or one
lane empty" names three shapes; for a limit only the first two apply, and the branch has the
first in three forms (one zero-row batch, one inside a stream, nothing but zero-row batches) and
the second not at all. The shape is `Script::Accumulate(vec![])` — `mark_done_and_fetch` over
nothing, both `LimitStream`s answering an empty vector and the comparator calling the empty
slot equal. `harness_cases.rs:242`'s `limit_over` cannot build it: it reads the schema off
`stream[0]`. One case, built the way `empty_plan()` gets its schema
(`Schema::new(synthetic(0, 0).schema())`), registered under `GpuLimit` with `operator_case!`;
the guard's counts do not move. Task 9 will construct the same script shape for coalesce, the
accumulating sort and the state merge ("no batch at all", "no arrival"), so a helper that takes
a schema rather than a first batch is what it will need anyway.

#### What the branch was asked for and has — checked item by item

- *Cases in this task.* Round trip: whole, a row range, past the end, zero rows — all four,
  plus the clamp. `GpuUnload`: whole, ranged, clamped, zero-row batch, range over a zero-row
  batch — all five, plus past-the-end. `GpuLimit`: the eight named shapes, plus one zero-row
  batch alone. Every operator case is `run_both(...).same(Order::AsEmitted)` on both backends
  through `executors_for`; none became a cpu-only assertion. The round trip's zero-row case
  is a comparison, not an early return: the C++ takes the whole-table arm at `num_rows == 0`
  (`gpu_executor.cpp:306`, `begin == end && num_rows > 0` is false), so the stream ships with
  its schema and one zero-row batch, `fetch` returns `Some`, and `assert_same` compares it;
  `None` is reachable only for a non-empty table under an empty range, which
  `a_range_past_the_end_ships_nothing` pins.
- *Verification bar.* Comparator red cases (9, each by its reason); shad-gpu green at
  `bd2758a4` (80 under `gpu_tests::`, 8 corpus, 53 C++); the guard red five ways and green;
  rust-only `--lib` 530 + 2 ignored with no device type in the rust rung (compiles under
  `rust-only`); `test_ci_coverage` 8 with no workflow edit; `build-test.md` rows. Nothing on the
  bar is unproven.
- *"No new CI line" proof.* `test_ci_coverage` asserts the workflow hands
  `peacockdb_core_gpu_lib` the `gpu_tests::` filter and knows no module; `test_module_layout`
  asserts every `mod gpu_tests` carries the gpu gate. The two together, plus cargo's substring
  filter, are why `tests::gpu_tests::…` is reached; the shad-gpu listing (80) is the direct
  evidence. Proven, by the pair rather than by one guard seeing the module.
- *The kind guard.* 18 `NodeRef` variants (`plan/mod.rs:1086-1108`); `EXCLUDED` 3, covered 2,
  `PENDING` 13 — the lists agree with `every_kind` minus the exclusions minus the two, and every
  one of the 18 kinds is a *root* of some `every_kind` fixture, which is what the guard reads.
- *Reusable by `operator-cases.md`.* All seven `Script` arms are present and type-checked
  against the trait signatures in `executor/mod.rs` (`drive` is monomorphized for both backends
  by `run_both`, so `Lanes`, `Emit`, `Join`, `Source` compile; none has run on a device).
  `decimals` and `prefixed` are `pub(crate)`; `Order::AsEmitted` exists and is the only order
  any device case has used — `Order::Any`'s sort path is unit-tested only. `operator_case!`
  is reachable from any module declared after `#[macro_use] mod coverage;`, which is first in
  `gpu_tests/mod.rs`. `Given::with_layout` takes any `PartitionLayout` (lanes, sort, hash), so a
  merge's N-lane sorted child and a co-partitioned join side are expressible. Two stub leaves
  under a join both `leaf()` into the writer's pool and the join `take`s them (`writer.rs:73`);
  the root's post-order is `size - 1`, which is the recipes index. `Device::open`'s budget is
  `GPU_BUDGET`, which `peacock_executor_create` does not enforce and no GPU executor reads;
  the compaction thresholds task 9 must size for live in the executors, not the context.
- *`build-test.md`'s two rows.* Counts reconcile: `--lib` 516→532 (+14 helpers, +2 wire), gpu
  55→80 (+25: 22 in `harness_cases`, 2 in `script`, 1 guard), C++ 66→67, total 1578→1620.
  The "Operator harness" prose matches the cases. The `test-support` paragraph is still true:
  it describes `src/test_support/`, which the feature gates; the new harness code is under
  `src/tests/`, gated by `lib.rs`'s `#[cfg(test)]`, and reaches `test_support` only for
  `GPU_BUDGET`, the way the paragraph says.
- *Coverage regression.* The `Given` lift changed no assertion: the eight cpu backend test files
  hold the same `#[test]` counts before and after (65 cases).

#### `architecture.md` — sentences checked, none falsified

- "Interfaces": "The ABI is seventeen symbols in five groups: lifecycle (…); the node-by-node
  session (…); the three per-call entry points (…); instrumentation (…); and two test hooks:
  `peacock_spark_partition_ids`, … and `peacock_handle_from_arrow`, which adopts one such batch
  into the live session as a handle so the operator harness can hand an executor a table it
  wrote." True: 17 declarations in `peacock_gpu.h`, 17 externs in `peacockdb-ffi`, 5+4+3+3+2.
- "The wire format" and "From node to seqs": no sentence says every node has a recipe or that
  the writer refuses a node outside the registry. "the recipe writer (`wire/`) emits one node
  per call a driver will make" was already loose for stubs and structural unions, and a `Given`
  leaf is the same stub the forwarder's parent slot already took. Not newly falsified.
- "The handle registry has no type": "two fields inside the private `NodeSession::Impl` … with
  allocation, lookup, consume-on-read and erase written inline at every site that touches them."
  `adopt` (`node_session.cpp:544`) is one more inline `next_handle++` / `registry.emplace` site,
  so the sentence and its point stand; "Nothing needs it yet" still holds.
- "What the frozen surface costs": "**Three refusals are a different kind of cost**: nothing on
  the surface makes a table out of nothing." Not falsified — the new symbol makes a table out of
  an Arrow batch a caller supplies, no recipe can name it, and the three refusals are about what
  an operator can do mid-plan — but it is the one sentence the branch comes nearest to, and the
  coordinator may want a clause saying the test hook is outside it.
- "Traits" (`GpuBatch` wraps a handle; a consumed handle skips `Drop`) and "From node to seqs"
  ("`execute_node` is stateless per seq — the only state is the handle registry") are unchanged
  in truth.

#### Notes for the signoff and for task 9 — not findings

- Deviations from the spec's letter the signoff should name: `Writer::leaf` is `pub(crate)`, not
  `pub(super)` (the layout test refuses `pub(super)`); the kind registry and guard live in
  `src/tests/gpu_tests/coverage.rs`, not `src/tests/`, because `inventory` collects per binary
  and a rust-rung guard would see an empty registry — so the guard runs on shad-gpu only;
  `Script` carries `#[allow(dead_code)]` until task 9 constructs the other four variants.
- The guard's universe is the root names of `every_kind()`, and nothing in the gpu rung pins
  that to 18; the rust-rung count the file's comment cites is a *tree-reach* count. Today the
  two coincide. A 19th kind added to `every_kind` only as a child would satisfy the rust-rung
  pin and never be asked for a case here. One `assert_eq!(kinds.len(), 18)` closes it; cheap,
  and not a defect today.
- The lifted `Given` does not override `GpuNode::name()`, so `name()` on it panics through the
  registry ("a plan node outside the registry reached a consumer of it") — the wire tests'
  `Given` says `"GpuGiven"`. No harness path calls it today (`recipes.rs:109`'s render does);
  a task 9 error path that names a child will hit it.
- `build-test.md`: the "Operator harness" row sits in the gpu block's *subcomponent* group,
  while `src/tests/gpu_tests/` is *crate integration, internal* by the page's own tiering; the
  "Harness helpers" row says "null in every column" (every column but `id`) and "dyadic in
  every float" (a property of the generator, asserted by no test). Wiki-only; fixable in the
  signoff commit.
- No decimal, no `Order::Any`, and no `Exec`/`Lanes`/`Emit`/`Join`/`Source` script has reached
  a device. All are task 9's by the spec's split; the handoff entry above says so.

### 2026-09-12 — completeness pass, the reviewer's reading: 0 blocking, 1 important

Read on `1c27cec6` without the analyst's list. Held to the spec and confirmed: two production
edits only; `stub()` is the same builder sequence so the payload bytes cannot move, and
`testdata/`/`.github/` are untouched; the new arm is unreachable from a planned tree (every
one of the 18 kinds downcasts through `try_node_ref_of`); the harness never names `run` or a
forwarder, and a forwarder handed to it is refused before either backend runs; exact comparison
through `ArrayData` equality; no divergence is credible for the seqless three and no route was
dodged — #187, #183 and #173 are each unreachable for a stated reason; the kind guard red five
ways with `3 + 13 + 2 = 18`; rung discipline; no test deleted or weakened (the cpu backend
tests keep 21/6/1/4/16/12/0/5); counts reconcile; caps counted; `clang-format` and `rustfmt`
clean. The one important: `build-test.md` filed the harness row under the gpu block's
*subcomponent* heading, and the helpers row said "null in every column" where the fixture's own
test says all but the id — both fixed by the coordinator. Deviations for the signoff confirmed
and extended (the guard lives in `gpu_tests/`, two rows not one, `leaf`'s unread return). A
note for task 9: the lifted `Given` does not override `GpuNode::name()`, so the first error
message or render over one shows the registry panic instead of a name.
