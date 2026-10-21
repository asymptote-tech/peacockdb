
# Corpus Rollout tickets

Tickets required for corpus rollout (CPU+GPU, all modes), TPC-H numbered and named queries, and 84/99 TPC-DS queries (window functions excluded). Minimum SQL functionality needs to be built for this milestone.


## Contents

- [Aggregates](#aggregates)
  - [#267 (tp4 single-mode aggregate) a Single-mode aggregate over a scan split into lanes is refused at tp4](#t267)
  - [#289 (Welford count wrap) a Welford merge on cuDF 25.02 wraps a count past 2^31 silently](#t289)
- [Sort / Limit](#sort--limit)
  - [#217 (sort fetch 0) a sort with `fetch 0` keeps every row on the device](#t217)
  - [#204 (merge drops fetch) the device's sorted merge drops its fetch when it is handed one input](#t204)
  - [#214 (limit drops empty batch) a limit drops a zero-row batch on both backends](#t214)
- [Testing](#testing)
  - [#227 (schema nullability tests) Check schema nullability in tests](#t227)
  - [#174 (two clamps, one rule) two clamps for one rule, and nothing compares them](#t174)
  - [#233 (pass-through col count) the plan validator does not check that a pass-through node keeps its input's column count](#t233)

## Aggregates

<a id="t267"></a>
### #267 (tp4 single-mode aggregate) a Single-mode aggregate over a scan split into lanes is refused at tp4
DataFusion plans an aggregate `mode=Single` where its input is one partition. The planner can map
that input to several lanes, and `aggregate_sequence` (`translator/aggregate.rs`) then finalizes
on every lane with no merge below it. Validation refuses the plan: "a finalizing merge over several
lanes needs its input hashed on a subset of its group columns". The Final arm merges first; the
Single arm passes `Shuffle::None` and does not.

The small-table rule hides it: a source under 5 MB plans one lane, so a larger one is needed.

**Corpus queries:** `pbench/not-in-uncorrelated` at the three tp4 modes, where the `NOT IN`
rewrite's `count(lit(1i32))` over `sub` is the Single aggregate. pbench turns the small-table rule off, so
`sub` gets four lanes. Its tp1 cells answer on both engines. Found when join-backend lifted #160,
which refused first.

<a id="t289"></a>
### #289 (Welford count wrap) a Welford merge on cuDF 25.02 wraps a count past 2^31 silently
On cuDF before 25.06, `merge_m2_input` (`aggregate.cpp`) casts the INT64 state count to INT32 with
`cudf::cast`, which wraps, and `group_merge_m2` sums it in INT32: a group with more than
2,147,483,647 non-NULL values gets a wrong or NULL stddev or var, with no error.

Since welford-device a keyless `stddev(x)` is one group, so `SELECT stddev(l_quantity) FROM
lineitem` reaches it at about sf360; no dataset here does (sf40 lineitem is 240M rows). The cpu's
count is `u64` and answers right. cuDF 25.06 and later sum in INT64 (`kMergeM2CountType`).

**Corpus query:** none.

**Fix proposed:** on a cuDF older than 25.06, refuse by name a merge whose count column's maximum
exceeds `INT32_MAX`, as the join session refuses past its `size_type` limits. The check goes with
25.02.

## Sort / Limit

<a id="t217"></a>
### #217 (sort fetch 0) a sort with `fetch 0` keeps every row on the device

`GpuSort` with `fetch: Some(0)` answers zero rows on the cpu and the whole batch on the device.

`sort.cpp` applies its slice under `sort->fetch() > 0`, and the wire writes `-1` for no fetch
(`node_writer.rs`, `fetch_of`), so zero is a fetch the device reads as none. The merge in
`node_session.cpp` tests `>= 0` and is right. `LIMIT 0` under an `ORDER BY` is the SQL shape;
DataFusion usually plans it away, so no corpus cell reaches it. Pinned by
`bug_a_fetch_of_zero_keeps_every_row_on_the_device` (`gpu_tests/exec_cases.rs`).

**Corpus query:** none, and likely none possible: `select l_orderkey from lineitem order by
l_orderkey limit 0;` (tpch) is the shape, but DataFusion is expected to turn `LIMIT 0` into an
empty relation before a sort exists. Unconfirmed; a plan-golden run of that query settles it.

**Fix proposed:** `sort.cpp:54` tests `sort->fetch() >= 0`, as `node_session.cpp`'s merge does.
Safe: every `CudfSort` writer sets `fetch` explicitly — `fetch_of`'s `-1` for none, the
accumulating sort's `-1`, and the three C++ tests. The `bug_` pin flips to a green case.

<a id="t204"></a>
### #204 (merge drops fetch) the device's sorted merge drops its fetch when it is handed one input

`CudfSortPreservingMerge` with a `fetch` over a single input table answers every row: 16 where the
plan asked for 5. A merge with no sort keys drops it the same way.

`node_session.cpp`'s collapse arm merges and slices only when there are keys and
`views.size() > 1`. Anything else falls to the plain `cudf::concatenate`, which applies no fetch.
Both accumulating sorts reach it: an `AccumulateBatchesAndSort` lane that received one batch, and
a `MergeSortedPartitions` with one populated lane. The wire puts the fetch on the merge alone
(`accumulating_sort` writes `fetch: -1` per batch). Pinned by
`bug_a_fetch_over_one_sorted_batch_is_not_applied_on_the_device` and
`bug_a_fetch_over_one_populated_lane_is_not_applied_on_the_device` (`gpu_tests/accumulate_cases.rs`).
#118 filed the same fallback from the comment audit and is archived as its duplicate.

**Corpus queries:** none can see it. The translator copies the fetch onto the per-batch `GpuSort`
(`per_batch_sort`, `planner/translator/nodes.rs`), so every input reaches the merge with at most
n rows and the missing cut drops nothing. All 324 fetch-carrying sort nodes in the plan goldens
sit on a capped input: 322 on a `GpuSort` with the same fetch, and `tpcds/q23` at the tp1 modes on
a union of two accumulating sorts with `fetch=100` (2026-09-28). A merge whose fetch its input
lacks, or whose input is not a sort (`sort_preserving_merge`'s `None` arm), would answer too many
rows.

**Fix proposed:** in `node_session.cpp`, move the fetch slice out of the merge arm and after
both arms: whenever `spm` is set and `spm->fetch() >= 0`, slice `result.table` to
`min(fetch, rows)`. One slice for every path. The two `bug_` pins become green `same` cases, and
`architecture.md`'s three notes on this exception drop.

<a id="t214"></a>
### #214 (limit drops empty batch) a limit drops a zero-row batch on both backends

A `GpuLimit` handed a batch of zero rows emits nothing for it, on the cpu and the device alike.
Both `LimitStream`s cut with `RowInterval::range_of` (`plan/interval.rs`), which answers `None`
when `start < stop` is false — always, at `n_rows == 0` — so the batch is released as if outside
the interval. Pinned by `bug_a_stream_of_one_zero_row_batch_is_dropped_on_both` and its two
neighbours (`gpu_tests/harness_cases.rs`). Joins read no batch as an empty side, and a keyless
init answers its identity row whatever arrived ([#199](../archive/archived-tickets.md#t199)), so
nothing sees the difference in rows.

**Corpus query:** none.

**Fix proposed:** a zero-row batch inside the interval is forwarded as it is; the three pins go
green as `same` cases.

## Testing

<a id="t227"></a>
### #227 (schema nullability tests) Check schema nullability in tests

Column nullability is maintained tin node's output_schema, but not tested anywhere. Start testing
it in the CPU engine, by adding this logic to declared_as() - if not null constraint is set in the
schema, check that every record batch produced does not have any nulls.

**Half landed in [`pbench`](../tasks/pbench.md), and the half that is left is the device's.**
`nulls_where_none_declared` sits beside `device_divergence` in `test_support/schema_validation.rs`
and `held_to_declaration` runs both; which half runs is the explicit argument
`NullsHeld::{Unread, PerColumn(&[usize])}`, the cpu flavour passing counts read off the arrow
batch and the device flavour `Unread`. Nothing exports the device's count today:
`peacock_handle_schema` carries the schema message alone. cuDF does hold it —
`column_view::null_count()` is a stored member, and this repo's C++ already reads it in
`expr.cpp` and `aggregate.cpp` — so the rest of this ticket is plumbing, not a new capability.
Two routes: a null-count entry point beside `peacock_handle_schema`, or materialising through
the existing `peacock_result_from_handle` and counting in Rust. The first is worth the C++
because the second copies a resident table to count its nulls, and the gpu hook already holds
both arguments it would need. Either way the device flavour then passes `PerColumn` and nothing
else changes. Not type-checkable without cuDF, so it wants a host with a card.

<a id="t174"></a>
### #174 (two clamps, one rule) two clamps for one rule, and nothing compares them
A limit keeps a row range of each batch, and the two backends clamp that range in their own code.
`RowRange::clamp` (`peacockdb-core/src/executor/row_range.rs`) returns `(offset, length)`; its one
caller is `CpuUnload::unload`. C++ `clamp_row_range` (`cpp/src/node_session.cpp`) returns
`(begin, end)`; `slice_handle` and `peacock_result_from_handle` share it. The rule is one: `begin =
min(offset, n)`, `take = min(length, n − begin)`, the subtraction keeping the `u64::MAX` to-the-end
sentinel from overflowing.

No test reads both. The four Rust cases (`executor/row_range/tests.rs`) and the ten C++ cases
(`cpp/tests/cpu/test_executor.cpp`, `ClampRowRange`) each prove one side, in shapes that cannot be
compared as written. Both docs name the risk: the two answering differently "would be a divergence
no test of either one alone could see". They agree today, line for line. The claim that landed
with the second clamp, "RowRange::clamp is now the one clamp", is what this corrects.

A third copy sits in the driver's test mock: `MockUnload::unload`
(`executor/driver/tests/mock.rs`) clamps with its own `min` arithmetic rather than calling
`RowRange::clamp`. Every row count in `driver/tests/limit.rs` is a fact about that private copy,
so the driver's limit tests would stay green if the shipped clamp broke
(`reports/hacks-audit.md` §7).

**Fix proposed:** one case table both suites read, the instrument `src/tests/executor_cases.rs`
already is for operators. A text file of `offset length rows → begin end` lines, `max` spelling
the sentinel, beside the gtest in `cpp/tests/cpu/`. The Rust test embeds it with `include_str!`
and checks `RowRange::clamp` after mapping `(offset, length)` to `(offset, offset + length)`. The
`ClampRowRange` gtest reads it through a path CMake passes as a compile definition. The fourteen
existing cases move into the file, deduplicated, and the per-side literals go. Both run on the cpu
tier, with no FFI and no device. A case added once then reaches both clamps, and a drift on
either side fails that side's test on the shared line. And the mock's copy goes:
`MockUnload::unload` calls `rows.clamp(batch.rows as u64)`, so the driver's limit tests run the
production rule. One line; `driver/tests/limit.rs`'s counts stay as they are.

<a id="t233"></a>
### #233 (pass-through col count) the plan validator does not check that a pass-through node keeps its input's column count

A node that carries its input's columns — Sort, CoalesceAllBatches, AccumulateBatchesAndSort,
Limit, MergePartitions, EmitPartitions, MergeSortedPartitions — passes `validate` while declaring
fewer or more columns than its input has. A Filter without a projection is not among them:
`declared_width` checks it against its input.

`declared_width` (`plan/validate.rs`) checks the column count only for the nodes that compute a
width, and hands these kinds to `types_across_the_edge`. That function zips the node's fields with
its input's and compares the pairs, and `zip` stops at the shorter list: a sort declaring three
columns over a five-column input passes both checks. Unreachable from the planner, whose
constructors derive these schemas from their input. The gap is open to the other callers:
`validate` is public so that a test rewriting a planned tree (`plan/tests/layout_injection.rs`,
the validator's own tests) runs the planner's check, and a rewrite that drops a column goes
unnoticed there. Found by `reports/hacks-audit.md` §6.

**Corpus queries:** none; no planned tree has the shape.

**Fix proposed:** in `types_across_the_edge`, compare the two field counts before the zip and
return `PlanError::Invalid` naming the node and both counts, in the style of `declared_width`'s
message. Test, in `plan/validate/tests.rs`: a hand-built sort declaring one column over a
two-column source is refused, and the same sort declaring both columns passes.
