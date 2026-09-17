# device-schema-harness — run record

Chain B, task 5. Branch `ENS-device-schema-harness` off `ENS-date-part-return-type` at
`60f04207`; PR targets `ENS-date-part-return-type`. Tasks 1–4 are `done` (PRs #158–#161 green)
awaiting the human's merge; the chain sits on master `0a338ead`.

## Dispatch 1 — 2026-09-17

- Hosts: **verda down**; rust-only proofs local. **shad-gpu up**, 0 MiB held. Caches warm.
- Pre-dispatch: `peacock_handle_schema` is in the header and externed in `peacockdb-ffi`
  (task 2), with its two gtests; nothing in Rust calls it yet. Chain E's join-cases builders
  are in the tree. What the four fixes left: no view type anywhere (task 1); the export is
  told each decimal's precision and `Device::fetch` takes the declared schema (task 2); every
  count is `Int64` and a decimal `avg`'s sum is `(p + 10, s)` (task 3); `date_part` answers
  `Int32` (task 4). The join-batching ticket is #220; the next free ticket number is 225.
- The spec's one hard rule for this task: **no spot-check expectation comes from a device
  run** — the eleven rows are written from the plan goldens and DataFusion before the first
  cycle, and a row the device contradicts becomes a `bug_` with a ticket, never a rewritten
  expectation. The coordinator reads every spot-check with that question at review.
- Routing: the developer works `device-schema-harness-impl.md` — the projection and its unit
  tests (rust-only), `schema_of` over the handle, the family suites one at a time with a
  shad-gpu cycle each, the walk hook and the eleven spot-checks, then the record.

## Developer notes

### The eleven spot-check expectations, written before the first device cycle

Source: the walk's own SQL planned rust-only at the walk's knobs (`planner::plan`, sf1,
`OneBatchPerLane`, 1 lane; 2 for `AVG_BY_FLAG`) and rendered with `render_plan` — the same
text the plan goldens hold, for the exact query rather than its nearest corpus twin. The
rendered node lines are quoted per row; the projection is `device_type_of`. Three rows differ
from the spec's table, each because the plan says so, none because a device did:

| # | query | after | expected `DeviceSchema` | source line (rendered plan) |
|---|---|---|---|---|
| 1 | `AVG_BY_FLAG` | `CudfAggregate{Partial}` | `[l_returnflag STRING, avg(lineitem.l_quantity)$sum DECIMAL128 s=2, avg(lineitem.l_quantity)$count INT64]` | `GpuAggregate: … schema=[l_returnflag:Utf8, avg(lineitem.l_quantity)$sum:Decimal128(25,2), avg(lineitem.l_quantity)$count:Int64]`; state order `[$sum, $count]` is `aggregates.rs` `decomposition(Avg)` |
| 2 | `AVG_BY_FLAG` | `CudfAggregate{Merge}` (all four) | the same | the lower `GpuAggregateBatches` line, same `schema=` |
| 3 | `AVG_BY_FLAG` | `CudfProject{finalize}` | `[l_returnflag STRING, avg(lineitem.l_quantity) DECIMAL128 s=6]` | the root `GpuAggregateBatches: … final=[CAST(… AS Decimal128(19,6)) as avg(lineitem.l_quantity)] … schema=[l_returnflag:Utf8, avg(lineitem.l_quantity):Decimal128(19,6)]` |
| 4 | `SUM_BY_FLAG` | `CudfAggregate{Partial}` | `[l_returnflag STRING, sum(lineitem.l_quantity) DECIMAL128 s=2]` | golden `tp4-single.plans.txt` `== aggregate-groupby` (same SQL): `GpuAggregate: … schema=[l_returnflag:Utf8, sum(lineitem.l_quantity):Decimal128(25,2)]` |
| 5 | `ROLLUP` | `CudfAggregate{Partial}` | plan: `[l_returnflag STRING, l_linestatus STRING, __grouping_id UINT8, sum(lineitem.l_quantity) DECIMAL128 s=2]`; written as `bug_` from the start: the device holds `INT32` at position 2 (#65) | `GpuAggregate: … grouping_sets=[[], [l_returnflag@1], [l_returnflag@1, l_linestatus@2]] … schema=[l_returnflag:Utf8, l_linestatus:Utf8, __grouping_id:UInt8, sum(lineitem.l_quantity):Decimal128(25,2)]` |
| 6 | `PROJECT_OVER_FILTER` | `CudfFilter` | `[c_custkey INT64]` — **not** the spec's `[c_custkey INT64, c_nationkey INT64]`: the filter projects its predicate column away, and `c_nationkey` is `Int32` in the sf1 parquet anyway (`join-int` golden) | `GpuFilter: predicate=c_nationkey@1 = 3, projection=[c_custkey@0], … schema=[c_custkey:Int64]` |
| 7 | `PROJECT_OVER_FILTER` | `CudfProject` | `[doubled INT64]` | `GpuProject: exprs=[c_custkey@0 * 2 as doubled], … schema=[doubled:Int64]` |
| 8 | `INNER_JOIN` | `CudfHashJoin{Inner}` | `[r_name STRING, n_name STRING]` — **not** the spec's `[n_name, r_name] plus keys`: DataFusion builds on `region` and the join's projection drops both keys; the `GpuProject` above the join is what restores the SELECT order | `GpuHashJoin: join_type=Inner, on=[(r_regionkey@0, n_regionkey@1)], projection=[r_name@1, n_name@2], … schema=[r_name:Utf8, n_name:Utf8]` |
| 9 | `MAX_OF_SUMS` | the inner `CudfProject{finalize}` (the one after `Aggregate{Merge}`) | `[l_returnflag STRING, sum(lineitem.l_quantity) DECIMAL128 s=2]` — **not** the spec's `[per_flag]`: the finalize emits keys and finals under the aggregate's output names; `per_flag` is the plain `GpuProject` above it | `GpuAggregateBatches: group_by=[l_returnflag@0], … final=[sum(lineitem.l_quantity)@1], … schema=[l_returnflag:Utf8, sum(lineitem.l_quantity):Decimal128(25,2)]`, then `GpuProject: exprs=[sum(lineitem.l_quantity)@1 as per_flag]` |
| 10 | `MAX_OF_SUMS` | the outer `CudfAggregate{Partial}` (the second `merge: false` call) | `[max(per_flag) DECIMAL128 s=2]` | `GpuAggregate: group_by=[], aggs=[max(per_flag@0) as max(per_flag)], final=[max(per_flag)@0], … schema=[max(per_flag):Decimal128(25,2)]`; `state_type(Max)` is the input's type |
| 11 | `SEMI_JOIN` | `CudfHashJoin{LeftSemi}` | `[c_custkey INT64]` | `GpuHashJoin: join_type=LeftSemi, on=[(c_custkey@0, o_custkey@0)], filter=…, projection=[c_custkey@0], … schema=[c_custkey:Int64]` |

Outcome column is filled after the walk cycle, per row, below.

### Outcome of the eleven, after the walk cycle (run 2, `PCK_TEST_FILTER=wire::gpu_tests`)

Rows 1–4 and 6–11 green as written; row 5 holds as the `bug_` it was written as: the device's
`__grouping_id` is `INT32` (#65). No expectation was edited after a device run. Test names, in
table order: `an_avg_partial_holds_a_string_key_a_scale_2_sum_and_an_int64_count`,
`an_avg_merge_holds_the_partials_state_unchanged`,
`an_avg_finalize_holds_a_string_key_and_a_scale_6_average`,
`a_sum_partial_holds_a_string_key_and_a_scale_2_sum`,
`bug_a_rollup_partial_holds_an_int32_grouping_id_where_the_plan_says_uint8`,
`a_filter_holds_the_one_column_its_projection_keeps`, `a_project_holds_the_int64_it_computed`,
`an_inner_join_holds_its_two_projected_strings_build_side_first`,
`the_inner_finalize_of_nested_aggregates_holds_the_key_and_the_scale_2_sum`,
`the_outer_max_holds_one_scale_2_decimal`, `a_semi_join_holds_the_build_sides_int64_key`.
The spec's table rows 6, 8 and 9 read differently from the plan than the spec wrote them (see
the source column above); the spec is frozen, so the correction lives here.

### How the pieces sit

- `test_support/mod.rs` declares `TypeId`, `DeviceType`, `DeviceSchema`, `device_type_of`,
  `device_schema_of`, `device_divergence` (bare `pub`, arrow types only) and the two handle
  reads `schema_at(executor, handle)` / `schema_of(&GpuBatch)` (`pub(crate)`, since they name
  an FFI pointer and a component type; `no_test_support_signature_names_a_component_type`
  scans bare `pub` alone). Bodies are `test_support/device_schema.rs`'s; its unit tests sit in
  `device_schema/tests.rs` under `#[cfg(test)] mod tests;`. The spec's `pub` signatures inside
  `device_schema.rs` would have failed `a_components_api_is_declared_in_its_mod_rs`, so the
  facade is in `mod.rs` as every component's is.
- `Session::schema_of(handle)` in the walk and `Device::schema_of(&batch)` both reach
  `schema_at`; the walk's `on_call` hook fires in `Walk::make` after every `execute_node` with
  the handles still resident. `held(sql, knobs)` reads every handle after every call and the
  eleven tests select by `FbKind` and position, so a hook that matched nothing cannot pass:
  each asserts the exact count of matching calls (two partials at two lanes, four merges).
- `script.rs`: `drive` is generic over what `down` lowers a batch to; `run_both` is unchanged
  in behaviour. `divergences_on_device(node, script)` runs the script on the device alone and
  reads each output handle where it sits (the sink's export is never made), returning one
  `Option<String>` per handle in slot order; `assert_holds_as_declared` demands at least one
  handle and no divergence, so a script producing no handle cannot pass vacuously.
- Outside the spec's Scope table, each one line and for a reason: `GpuBatch::executor()` in
  `executor/mod.rs` (the read needs the batch's session; `consume` was the only way out and
  it releases nothing but hands the handle over) and the deletion of the same accessor from
  `executor/ffi_tests/mod.rs`, which only its own cases read; `peacockdb-ffi/src/lib.rs`'s doc
  comment on `peacock_handle_schema`, which said nothing in Rust calls it. `architecture.md`
  (Interfaces) still says `handle_schema` is "which nothing in Rust calls yet" — the
  coordinator's page; falsified by this branch.
- Existing case files: no case body touched. Forty-five builders in nine `*_cases.rs` files
  went `fn` → `pub(crate) fn` (`git diff` over them shows only `pub(crate)` additions), so the
  schema suites reuse `keyed`/`hash_join_keyed` as the spec asks rather than copying them.
  `pub(super)` is refused by `nothing_is_pub_super`, so `pub(crate)` it is.
- The gpu-feature build carries one pre-existing warning this task did not touch:
  `unused import: AsArray` in `aggregate_dimension_cases.rs:9`.
- `#[cfg_attr(not(all(test, feature = "gpu")), allow(dead_code))]` sits on the two handle reads
  in `test_support/mod.rs`: their only callers are the device test rung, and the default,
  rust-only and plain-gpu lib builds all warned without it.

### Device runs (shad-gpu, 0 MiB held before each; every pool built)

- Run 1, `PCK_TEST_FILTER=_schema_cases` — `peacockdb_core_gpu_lib`: `test result: FAILED. 132
  passed; 2 failed; 0 ignored; 0 measured; 986 filtered out`; `test_gpu_corpus`: `running 0
  tests`. The two reds were the grouped Welford init and merge: types as declared, all three
  state columns named `stddev(f64)` by the device. Root cause `aggregate.cpp` (Partial and
  Merge arms push each child under the one wire `alias`; `state_funcs` folds the triple into
  one `AggregateFuncNode`). Ticket #225; both cases became `bug_` asserting the divergence.
- Run 2, `PCK_TEST_FILTER=wire::gpu_tests` (same binary) — `test result: ok. 21 passed; 0
  failed; 0 ignored; 0 measured; 1099 filtered out`: the ten old walk tests and the eleven.
- Run 3, `PCK_TEST_FILTER=_schema_cases` after the pins — `test result: ok. 134 passed; 0
  failed; 0 ignored; 0 measured; 986 filtered out`.
- Run 4, unfiltered — five C++ binaries `PASSED` (12, 6, 44, 4, 4); `peacockdb_core_gpu_lib`
  `test result: ok. 535 passed; 0 failed; 0 ignored; 0 measured; 585 filtered out`;
  `test_gpu_corpus` `test result: ok. 28 passed; 0 failed`.
- Run 5, unfiltered, on the final tree (the lint attribute on the two handle reads changed
  after run 4) — the same five C++ binaries `PASSED`; `peacockdb_core_gpu_lib` `test result:
  ok. 535 passed; 0 failed; 0 ignored; 0 measured; 585 filtered out`; `test_gpu_corpus`
  `test result: ok. 28 passed; 0 failed`.

### Counts (`--list`)

rust-only `--lib` 582 (566 + 16 projection tests); gpu rung `gpu_tests::` 535 (390 + 134
schema cases + 11 spot-checks). Per file: harness 4, source 8, exec 33, accumulate 7, emit 6,
join 38, nested 6, aggregate 32. Six `bug_` pins among the 134 and one among the eleven.

### Tickets

- #225 opened (Welford state names); pins named on #65, #207 and #216 for the new cases.

## Reviewing — 2026-09-17

Dispatch 1 committed as `9b3c8a79` on `d1b4d41e`, pushed; PR #162 against
`ENS-date-part-return-type`, base verified. For the reviewer: the facade items live in
`test_support/mod.rs` rather than `device_schema.rs` because the layout rule puts a component's
API in its `mod.rs`; 45 builders in existing `*_cases.rs` files went `fn` → `pub(crate) fn` and
nothing else in them moved; `GpuBatch::executor()` is one production accessor, with the same
test-only accessor deleted from `ffi_tests`; three spot-check rows read differently from the
plan than the spec's table wrote them and were recorded as read, before any device run.
