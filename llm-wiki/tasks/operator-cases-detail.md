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

### 2026-09-12 — plan task 2 done: `GpuAggregate`, `GpuAggregateBatches`

`src/tests/gpu_tests/aggregate_cases.rs`, 20 cases; `mod aggregate_cases;` after `coverage`;
`GpuAggregate` and `GpuAggregateBatches` out of `PENDING`. No production file touched. Every
declared type was read off DataFusion 45 rather than the plan: a scratch planning run (kept out of
the tree) over `dec DECIMAL(18, 2)` gave `sum` (28,2); `avg` **state `[count UInt64, sum (18,2)]`
and output (22,6)** — the plan's "(28,2)" for the avg sum is wrong, the planner's `decompose`
copies DataFusion's `avg[sum]`, which is the input's type; `stddev` `[count UInt64, mean, m2]`;
`__grouping_id` UInt8 up to eight keys (`Aggregate::grouping_id_type`).

**Green (13).** Init: `a_grouped_sum_min_max_count_agree`, `a_global_sum_min_max_count_agree`,
`the_single_node_shortcut_finalizes_an_average` (Int64 counts, as the plan wrote it),
`a_grouped_aggregate_over_zero_rows_is_zero_rows_on_both`,
`a_global_aggregate_over_zero_rows_keeps_its_identity_row` — the exec-level init over a zero-row
batch keeps its row on both, so #199 is not here. Merge: `a_merge_emits_state_at_done`,
`a_merge_with_a_finalize_emits_the_projected_columns`,
`arrivals_crossing_the_compaction_threshold_fold_the_same`, `a_count_merges_by_sum`,
`a_merge_over_no_arrival_answers_nothing_on_both`,
`a_merge_over_one_zero_row_arrival_is_zero_rows_on_both`,
`a_zero_row_arrival_among_others_changes_nothing`, `a_finalize_over_no_arrival_answers_nothing_on_both`.

**The compaction case** is five `sum_state(1_500_000, seed)` arrivals of `[key Int32, sum Int64]`,
about 18 MiB each by the shared formula: the device folds at the fourth (64 MiB crossed) and
again at done, the cpu at every arrival (1 MiB). 7.5M state rows, ~92 MB uploaded; the whole
family runs in 1.6–1.8 s on shad-gpu, so its cost is not separable and not a concern.

**`bug_` (7).**
- `bug_a_welford_init_exports_its_count_as_int64` — #163. Verbatim, schema line: cpu
  `stddev(f64)$count: UInt64`, gpu `stddev(f64): Int64`, and the device names all three state
  columns by the alias (`aggregate.cpp` names a struct's children by it). The names are not
  ticketed: the sink relabels by the declared schema, so nothing a user sees carries them.
  **Second finding, against the harness:** once the type is cast, the mean and m2 differ in the
  last digits — `128185.06597222222` vs `128185.06597222219`, `0.462499999999995` vs `0.4625` —
  Welford's update is order-dependent and a mean is not dyadic, so "floats dyadic so sums compare
  exactly" does not reach a Welford state. The corpus compares stddev under `golden_approx_std`
  at relative 1e-11; the harness has no such mode, and the case compares keys and counts exactly
  and the moments to that same 1e-11, locally. Not a ticket: no engine is wrong.
- `bug_a_welford_merge_exports_its_count_as_int64` — #163, the same at the merge's done slot
  (slot 3); the merged moments differ in the last digits the same way.
- `bug_a_decimal_sum_is_exported_at_precision_38` — #187. Verbatim: `schema differs / cpu:
  sum(dec) Decimal128(28, 2) / gpu: sum(dec) Decimal128(38, 2)`; values agree once widened.
- `bug_grouping_sets_carry_the_devices_own_id_and_type` — #65. Verbatim: `schema differs …
  __grouping_id UInt8 … / gpu … __grouping_id Int32`. Pinned as the cpu's rows with the id
  Int32 and DataFusion's `[false, true]` = 1 mapped to the device's 2 (`aggregate.cpp` sets bit
  `i` for masked key `i`; DataFusion's `group_id_array` puts the first key highest); the rows and
  sums agree, so the value divergence predicted from the C++ is confirmed by the pin passing.
- `bug_grouping_sets_over_zero_rows_are_zero_rows_under_the_devices_id_type` — #65, zero rows.
- `bug_a_decimal_average_is_refused_on_the_cpu` — #163. Verbatim: `cpu refused: the node declares
  Schema { … avg(dec) Decimal128(22, 6) … } and DataFusion answered with Schema { … avg(dec)
  Decimal128(26, 10) … }: Invalid argument error: column types must match schema types, expected
  Decimal128(22, 6) but found Decimal128(26, 10) at column index 0`. The planner's finalize —
  `Cast(sum → (22,6)) / Cast(count → (22,0))` — is typed by arrow at (26,10), and `declared_as`
  refuses. The device's answer is not read past `cpu_refuses()`; a device-side pin waits on the
  cpu accepting the shape.
- `bug_a_global_merge_over_no_arrival_answers_nothing_on_the_device` — #199, the ticket's exact
  site. Verbatim: `gpu produced no batch, cpu 1 rows`. And the cpu's one row is `count(i32) =
  NULL`, not 0: a count merges by sum, and sum over nothing is NULL — so SQL's 0 is on neither
  engine. Pinned both ways: cpu `[NULL]`, device one empty slot.

**Tickets.** No new number. #199 gains the shown site and the NULL-not-0 observation; #163 gains
the finalize's (26,10)-vs-(22,6) refusal as a second facet; #65 gains the bit order and the Int32
type, and names the pins. Counter stays 204. Every ticket the rows name reproduces (#65, #163,
#187, #199); none left unreproducible.

**Runs on shad-gpu** (neighbour at 37 GiB, every pool built):
- `20260912T065031-277431` green-form, `PCK_RUN_CPP=0 PCK_TEST_FILTER=tests::gpu_tests::aggregate_cases`:
  13 passed 7 failed (the seven above, in green form).
- `20260912T065322-278379` after the first rewrite: 17 passed 3 failed — the Welford pair on the
  moments' last digits, and #199's on the cpu's NULL where the pin said 0; both read above.
- `20260912T065636-279494` and, after dropping an unused argument, `20260912T065721-280361`:
  `peacockdb_core_gpu_lib` **20 passed 0 failed**.
- `20260912T065726-280393` guard: `every_kind_has_a_case_or_is_named_as_pending_or_excluded` passed.
- `20260912T065738-280429` rung whole, `PCK_RUN_CPP=0`, no filter: `peacockdb_core_gpu_lib`
  **133 passed** (113 + 20), `test_gpu_corpus` 8 passed.
- Local: rust-only lib 530 passed, 2 ignored; gpu `--no-run` at 0 warnings; `rustfmt --check`
  clean on `aggregate_cases.rs` and `coverage.rs`.

**For plan task 8's tidy.** `gpu_answered` and `batch_of` are now in two case files; a home in
`script.rs` (an `Outcome` method) would be the harness's, which this task may not grow.

### 2026-09-12 — plan task 3 dispatched: the accumulators, in a fresh window

Plan tasks 1-2 committed (`abd2b7d4`, and the head above). The developer that carried them hands
over here. What it settled: the rhythm is green-form, family run, verbatim failures here,
ticket, `bug_`, green, guard, rung whole with an empty filter (133 on `peacockdb_core_gpu_lib`
now); state fixtures for the aggregates live in `aggregate_cases.rs`; the planner's declared
types are read off a scratch planning run, not the plan text; two helpers (`gpu_answered`,
`batch_of`) are duplicated across the two case files and belong to `Outcome` — plan task 8's
tidy or a finding against the harness task; Welford's moments are not dyadic and compare to
1e-11 in their two cases, the corpus's own figure.

### 2026-09-12 — plan task 3 done: `GpuCoalesceAllBatches`, `GpuAccumulateBatchesAndSort`, `GpuMergeSortedPartitions`

`src/tests/gpu_tests/accumulate_cases.rs`, 23 cases; `mod accumulate_cases;` after `coverage`;
the three kinds out of `PENDING`. No production file touched; `Given::name()` never reached.

**Two departures from the plan's text, both in the fixture.** `synthetic` numbers `id` from 0 in
every batch, so the plan's `synthetic(16, 1), synthetic(24, 2)` runs tie on the merge key and
`Order::AsEmitted` would compare tie order, which neither engine contracts. The sorted runs are
instead `runs(rows, seed, n)`: one synthetic batch dealt round-robin, each run sorted by `id`, no
`id` in two, every run interleaved by the merge — local to the file, arrow's `take_record_batch`,
no harness change. And the sorted `Given` keeps `MultipleBatches` rather than the plan's
`SingleBatch`, since a lane may carry two batches (`two_batches_per_lane_all_go_into_one_merge`).

**Cases beyond the spec's rows**, added after reading `node_session.cpp`'s SPM arm, which merges
and slices only under `views.size() > 1`: `a_fetch_over_one_sorted_batch` and
`a_fetch_over_one_populated_lane` (both now `bug_`, #204), and `two_batches_per_lane…` (green).
The matrix's "`Done` before any batch" and the empty table's "lane 0 `Done` before lane 1's rows"
are one shape under `Script::Lanes` (lane order is fixed); both exist, at three lanes and two.

**Green (18).** Coalesce: `several_batches_coalesce_to_one`, `one_batch_coalesces_to_itself`,
`no_batch_coalesces_to_nothing_on_both`, `one_zero_row_batch_coalesces_to_zero_rows`,
`a_zero_row_batch_among_others_adds_nothing`. Sorted: `sorted_batches_merge_into_one_sorted_stream`,
`a_fetch_cuts_the_merged_stream`, `one_sorted_batch_is_itself`, `no_sorted_batch_is_nothing_on_both`,
`a_zero_row_batch_among_sorted_others_adds_nothing`. Merge: `three_sorted_lanes_merge_into_one`,
`two_batches_per_lane_all_go_into_one_merge`, `a_fetch_cuts_the_merged_lanes`,
`an_empty_lane_is_skipped_by_both`, `a_lane_done_before_any_batch_is_skipped_by_the_merge`,
`every_lane_done_with_nothing_is_nothing_on_both`, `a_zero_row_lane_beside_lanes_with_rows_is_skipped`,
`lane_zero_done_before_lane_one_arrives`.

**#173 has no `bug_` test here.** Every "nothing arrived" shape — no batch, every lane `Done` with
nothing — answers nothing on both: `gpu_backend/accumulate.rs` short-circuits `held.is_empty()` to
`Ok(empty)` before any call, so the C++ refusal ("a collapse with no input handles has no columns to
answer with") is never reached, and the cpu's `one_batch` does the same. The empty slot on both is
the contract the ticket states; a zero-row batch in (one, or among others) is a table on both.

**`bug_` (5).**
- `bug_a_fetch_over_one_sorted_batch_is_not_applied_on_the_device` — **#204, new**. Verbatim:
  `cpu and gpu differ: slot 1: cpu has 5 rows, gpu 16 rows`. Pinned both ways: cpu the first 5
  rows of `synthetic(16, 1)`, device the whole batch.
- `bug_a_fetch_over_one_populated_lane_is_not_applied_on_the_device` — #204. Verbatim: `cpu and
  gpu differ: slot 2: cpu has 5 rows, gpu 16 rows`. Same pin at the last `Done`'s slot.
- `bug_one_zero_row_batch_sorts_to_nothing_on_the_cpu` — **#205, new**. Verbatim: `cpu and gpu
  differ: slot 1: cpu produced no batch, gpu 0 rows`. Pinned: cpu `[[], []]`, device
  `[[], [synthetic(0, 1)]]`.
- `bug_a_fetch_over_zero_rows_is_nothing_on_the_cpu` — #205. Verbatim: `slot 1: cpu produced no
  batch, gpu 0 rows`.
- `bug_every_lane_a_zero_row_batch_is_nothing_on_the_cpu` — #205. Verbatim: `slot 3: cpu produced
  no batch, gpu 0 rows`. Pinned: cpu four empty slots, device the zero-row batch at slot 3.

**Root causes, read before ticketing.** #204: `node_session.cpp` ~293, the `views.size() > 1`
guard on `cudf::merge` + slice; one view goes to `cudf::concatenate` with no slice, and the wire
puts the fetch on the merge alone (`accumulating_sort` writes `fetch: -1`). Masked from SQL by the
per-batch `GpuSort`'s own fetch. #205: DataFusion's `SortExec` over zero rows yields no batch;
`SortedRuns::mark_done_and_fetch` and `CpuPartitionAccumulator::accumulate_and_fetch` hand the
empty `Vec` to `one_batch`, which reads it as "nothing arrived". `CpuExec::exec` concatenates the
same empty answer under the schema, which is why task 1's exec sort over zero rows was green, and
the cpu coalesce (`concat_batches` over the held zero-row batch) is green here too.

**Tickets.** #204 and #205, both Critical correctness, text in `tickets.md`; counter now 206,
Contents count 20. #173 reproduces as stated (nothing on both); nothing left unreproducible.

**The `Lanes` arm** ran on a device for the first time and needed nothing: ten cases through it,
lane order as `drive` writes it, the merge's answer at the last `Done`'s slot.

**Runs on shad-gpu** (neighbour at 37 GiB; every pool built):
- `20260912T070607-282688` green-form, `PCK_RUN_CPP=0 PCK_TEST_FILTER=tests::gpu_tests::accumulate_cases`:
  `peacockdb_core_gpu_lib` 18 passed 5 failed (the five above); `test_gpu_corpus` 0 of 8.
- `20260912T071844-284406` after the rewrite: `peacockdb_core_gpu_lib` **23 passed 0 failed**.
- `20260912T071900-284449` guard: `every_kind_has_a_case_or_is_named_as_pending_or_excluded` passed.
- `20260912T071920-284491` rung whole, `PCK_RUN_CPP=0`, empty filter: `peacockdb_core_gpu_lib`
  **156 passed** (133 + 23), `test_gpu_corpus` 8 passed.
- Local: `cargo test --features rust-only -p peacockdb-core --lib` 530 passed, 2 ignored; the gpu
  `--no-run` build 0 warnings; `rustfmt --check` clean on `accumulate_cases.rs` and `coverage.rs`.

**For plan task 8's tidy.** A third `bug_` assertion helper, `each_answers` (both sides against
hand-written slots), joins `gpu_answered` and `batch_of` as candidates for `Outcome`.

### 2026-09-12 — plan task 4 done: `GpuEmitPartitions`

`src/tests/gpu_tests/emit_cases.rs`, 15 cases; `mod emit_cases;` after `coverage`;
`GpuEmitPartitions` out of `PENDING`. No production file touched; `Given::name()` never reached.
Every case is one lane in and N out, each lane a slot: `Script::Emit` pushes one slot per output
lane per call, so a row in the wrong lane is a wrong slot.

**Four cases beyond the spec's rows**, one per remaining arm of the kernel's key-type switch
(`spark_hash_partition.cu`, which takes STRING, INT8-64 and DATE32): an `i64` key and a date key
(green), a float key and a boolean key (both `bug_`). The switch is the one thing about the scatter
a synthetic key can probe that the conformance gate does not.

**Green (12).** `four_lanes_on_an_int_key_place_every_row_the_same`,
`sixty_four_lanes_leave_most_empty_and_agree_on_all`, `null_keys_land_in_the_same_lane`,
`two_keys_combine_the_same_way`, `a_string_key_places_every_row_the_same`,
`an_int64_key_places_every_row_the_same`, `a_date_key_places_every_row_the_same`,
`each_batch_is_scattered_on_its_own`, `a_zero_row_batch_scatters_into_n_zero_row_lanes` (and
asserts the four slots), `a_batch_of_one_key_leaves_n_minus_one_lanes_empty`,
`a_batch_of_all_null_keys_lands_in_one_lane`, `a_stream_of_zero_row_rows_zero_row_is_scattered_per_batch`.
So the 1→4 and 1→64 shapes are fine on every supported key type, with nulls, with two keys, and
over zero rows: a zero-row batch comes back as four zero-row lanes on both sides.

**`bug_` (3)**, all one refusal site — `spark_hash_partition.cu:179`, the `CUDF_FAIL` in the type
switch — with the cpu answering through comet's hasher each time:
- `bug_a_decimal_key_is_refused_on_the_device` — #95. Verbatim: `gpu refused: execute_node(#1
  CudfRepartition{Hash, 1→8}): CUDF failure at:…/cpp/src/spark_hash_partition.cu:179: peacock
  spark_partition_ids: unsupported key column cuDF type_id=27 (supported: STRING, dict-encoded
  string, INT8/16/32/64, DATE32; timestamp/decimal/float partition keys pending — extend the kernel
  + re-prove comet conformance, see #18/Inc7)`. The refusal comes before any export, so #187 is not
  reached from here.
- `bug_a_float_key_is_refused_on_the_device` — **#206, new**. Verbatim: the same message with
  `CudfRepartition{Hash, 1→4}` and `type_id=10`.
- `bug_a_boolean_key_is_refused_on_the_device` — #206. The same with `type_id=11`.

**#184 is #95.** Line 179 is the type switch, and `recipe-payloads.txt`'s q15 block shows its
`#11 CudfRepartition{Hash, 1→4}` hashing `total_revenue@4`, a decimal. The harness runs 1→N green
on every supported type, so the 1-to-N shape itself is not the failure. Written onto #184 as a
dated paragraph (`active-tickets.md`); it closes with #95.

**Tickets.** #206 (Blockers for disabled coverage), text in `tickets.md`; counter now 207, Contents
count 16. #95 gains the actual message (its text claimed "decimal partition key unsupported"), the
#184 link and the pin. #187 does not reproduce from this family — the refusal is earlier — and stays
reproduced by the exec and aggregate families.

**The `Emit` arm** ran on a device for the first time and needed nothing: fifteen cases through it,
N slots per call in lane order, empty lanes exported as zero-row batches.

**Runs on shad-gpu** (neighbour at 37 GiB; every pool built):
- `20260912T072411-286609` green-form, `PCK_RUN_CPP=0 PCK_TEST_FILTER=tests::gpu_tests::emit_cases`:
  `peacockdb_core_gpu_lib` 12 passed 3 failed (the three above); `test_gpu_corpus` 0 of 8.
- `20260912T072608-287568` after the rewrite: `peacockdb_core_gpu_lib` **15 passed 0 failed**.
- `20260912T072623-287604` guard: `every_kind_has_a_case_or_is_named_as_pending_or_excluded` passed.
- `20260912T072638-287640` rung whole, `PCK_RUN_CPP=0`, empty filter: `peacockdb_core_gpu_lib`
  **171 passed** (156 + 15), `test_gpu_corpus` 8 passed.
- Local: `cargo test --features rust-only -p peacockdb-core --lib` 530 passed, 2 ignored; the gpu
  `--no-run` build 0 warnings; `rustfmt --check` clean on `emit_cases.rs` and `coverage.rs`.

`PENDING` now holds `GpuLoadParquet`, `GpuHashJoin`, `GpuCrossJoin`, `GpuNestedLoopJoin`.

### 2026-09-12 — plan task 5 done: `GpuHashJoin`

`src/tests/gpu_tests/join_cases.rs`, 85 cases (846 lines — plan task 6's cross and nested-loop
cases may need a file of their own under the 1000-line rule); `mod join_cases;` after `coverage`;
`GpuHashJoin` out of `PENDING`, `GpuCrossJoin` and `GpuNestedLoopJoin` still in it. No production
file touched; `Given::name()` never reached.

**The admitted matrix**, read off `capability()` (`plan/join.rs`): a residual filter exists for
Inner (streaming) and for LeftSemi, LeftAnti and LeftMark (single-batch probe); Left, Right and
Full with one are `Err` (#153) and RightSemi/RightAnti with one are `Err` (#159), and the recipe
writer `expect`s the capability — so those five have no row and no case. Nine types × one probe;
seven × two probes (Left and Full refuse their first batch already); `null_equals_null=true` on
Inner, LeftSemi, RightSemi, LeftAnti, LeftMark (the `false` side is every other case); the four
filtered forms; one projection; six empty shapes × nine types; and a seventh shape for the five
finishing types — the build set and the finish called with no probe call at all, which is the only
route to `finish_without_keys`: a zero-row probe still accumulates a zero-row key handle, so
"only zero-row probes then the finish" never reaches #173's site. That shape was added.

**One fixture correction, read as an engine finding first and then not.** The first green-form
run (`20260912T073155-290024`, 43/42) had every Left, Right and Full case refused on the cpu:
`the node declares Schema { … b_id: Int64, nullable: false … } and DataFusion answered with … b_id
nullable: true …: Invalid argument error: Column 'b_id' is declared as non-nullable but contains
null values`. `declared_as` builds the answer under the declared schema, and the plan's `output_of`
copied `id`'s `nullable: false` onto a side the outer join pads. The planner declares DataFusion's
join schema, which marks the padded side nullable, so the fixture now does the same (`padded`).
Not a ticket: the harness declared what no planner would.

**Green (45).** Inner: one probe, `null_equals_null`, residual, projection, zero-row build, zero-row
probe, both empty, no build. Left: no build. Right: one probe, zero-row build (**the device pads
over a zero-row build table**), zero-row probe, both empty. Full: none. LeftSemi: one probe, two
probes, `null_equals_null`, residual, all six empties, and `finishing_after_only_zero_row_probes`.
RightSemi: one probe, `null_equals_null`, zero-row build, zero-row probe, both empty, no build.
LeftAnti: `null_equals_null=true`, all six empties, and `finishing_with_no_probe_batch_answers_every_build_row`
(the device hands its build side up, as `finish_without_keys` says). RightAnti: zero-row build
(keeps every probe row), zero-row probe, both empty. LeftMark: `null_equals_null=true`, all six
empties except `empty_between`.

**`bug_` (40), by ticket.**
- **#152, build-side copy (12)** — Inner, Right, RightSemi, RightAnti × two probes, a zero-row probe
  between two, only zero-row probes. Verbatim: `gpu refused: this join's recipe copies its build
  side per probe batch and the ABI has no copy: probe batch 2 has no build side left, since the
  call for batch 1 erased it (#152)`. A zero-row second batch is refused the same way.
- **#152, probe copy (12)** — Left and Full × one probe, zero-row build, zero-row probe, both empty,
  a zero-row probe between two, only zero-row probes. Verbatim: `gpu refused: this join's recipe
  copies its probe batch — the key project keeps the keys and the join below it reads the same
  batch — and the ABI has no copy, so neither call can run without erasing the other's input
  (#152)`. So Left and Full have no device path at all, as the ticket says.
- **#173 (4)** — Left, Full, LeftSemi, LeftMark finishing with no probe batch. Verbatim (Left): `gpu
  refused: this lane's probe was empty, so its finish has no keys to join against — and what it
  owes is every build row padded with a typed NULL per probe column, which is a table of literals,
  which the frozen surface cannot make without one (#173)`; LeftSemi `… owes is no rows, which is a
  table of no rows …`; LeftMark `… every build row with a false mark, which is a table of literals
  …`. Each pin quotes both the common part and its type's owed phrase.
- **#175 (3)** — Right, Full, RightAnti with `build: None`. Both sides refuse with one message:
  `this lane's build side is empty, and what this join owes is its probe side — which takes a call
  over a build table that does not exist (#175)`; pinned on both.
- **#59 (9)** — LeftAnti × one probe, two probes, residual, a zero-row probe between two; LeftMark ×
  the same four; RightAnti × one probe. Verbatim: `cpu and gpu differ: slot 1: cpu has 2 rows, gpu
  0 rows` (LeftAnti; the two build rows with a null `key`), `slot 0: cpu has 4 rows, gpu 0 rows`
  (RightAnti; the four null-key probe rows), `slot 0: cpu has 11 rows, gpu 10 rows` (the filtered
  anti), `rows differ` for the marks — build rows 10 and 21 `false` on the cpu, `true` on the
  device. Root cause: the cpu's `HashJoinExec::try_new` takes `node.null_equals_null` for every
  type; `join.cpp` hardcodes `EQUAL` for anti and mark. Pinned as "the device's answer under
  `false` is the cpu's answer under `true`" (`device_answers_as_if_null_equals_null`), which is
  exact and covers the finish pass, the `mixed_*` filtered forms and the streamed probe alike. #59
  said this was latent; it is not.

**Tickets.** No new number; counter stays 207. #59 gains the shown divergence and the pin; #175 the
pin and the zero-row-build observation; #173 the join finish as its only reached site. #152 is at
its line cap and unchanged — its two halves reproduce exactly as its text predicts. #153 and #159
are the two unreachable rows (plan-time refusals) and stay as they are.

**The `Join` arm** ran on a device for the first time and needed nothing: `set_build`,
`probe_and_fetch` per batch, `finish_and_fetch`, and the `without_build` route with `build: None`,
each producing the slots `drive` documents.

**Runs on shad-gpu** (neighbour at 37 GiB; every pool built):
- `20260912T073155-290024` green-form: 43 passed 42 failed (the fixture's nullability plus the
  engine findings).
- `20260912T073359-290956` green-form with the fixture corrected: **45 passed 40 failed**, the
  forty above.
- `20260912T073629-292171` after the rewrite: `peacockdb_core_gpu_lib` **85 passed 0 failed**.
- `20260912T073652-292210` guard: `every_kind_has_a_case_or_is_named_as_pending_or_excluded` passed.
- `20260912T073707-292246` rung whole, `PCK_RUN_CPP=0`, empty filter: `peacockdb_core_gpu_lib`
  **256 passed** (171 + 85), `test_gpu_corpus` 8 passed.
- Local: `cargo test --features rust-only -p peacockdb-core --lib` 530 passed, 2 ignored; the gpu
  `--no-run` build 0 warnings; `rustfmt --check` clean on `join_cases.rs` and `coverage.rs`.

**For plan task 6.** `join_cases.rs` is at 846 lines with the hash join alone; cross and
nested-loop over the same `build_batch`/`probe_batch` fixture fit in ~150 lines, which is under
the cap but tight — a `nested_cases.rs` beside it is the safer split, with the fixture's two
one-liners duplicated rather than a shared module added. `gpu_refuses_with` and
`both_refuse_with` here are the fourth and fifth `bug_` helpers that belong to `Outcome`
(plan task 8's tidy).
