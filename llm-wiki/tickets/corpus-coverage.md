
# Corpus Rollout tickets

Tickets required for corpus rollout (CPU+GPU, all modes), TPC-H numbered and named queries, and 84/99 TPC-DS queries (window functions excluded). Minimum SQL functionality needs to be built for this milestone.


## Contents

- [Aggregates](#aggregates)
  - [#267 (tp4 single-mode aggregate) a Single-mode aggregate over a scan split into lanes is refused at tp4](#t267)
  - [#289 (Welford count wrap) a Welford merge on cuDF 25.02 wraps a count past 2^31 silently](#t289)
- [Sort / Limit](#sort--limit)
  - [#217 (sort fetch 0) a sort with `fetch 0` keeps every row on the device](#t217)
  - [#204 (merge drops fetch) the device's sorted merge drops its fetch when it is handed one input](#t204)
- [Testing](#testing)
  - [#227 (schema nullability tests) Check schema nullability in tests](#t227)

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

