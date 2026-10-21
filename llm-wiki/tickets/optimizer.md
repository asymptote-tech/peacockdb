
# Tasks for the optimizer project

## Contents

- [#73 (CBO umbrella) Cost-based optimizer (CBO) umbrella](#t73)
- [#101 (CSE/CTE reuse) No CSE / CTE materialization: identical subexpressions recomputed N times](#t101)
- [#140 (broadcast joins) broadcast joins (1:N partition broadcast)](#t140)
- [#170 (one-batch lane shortcuts) a source whose lanes each hold one batch could say so, and three shortcuts would fire](#t170)
- [#141 (small-key shuffle skip) the planner cannot skip the shuffle for small group-key sets](#t141)
- [#139 (post-filter coalesce) GpuCoalesceBatches(target): compact post-filter fragments](#t139)
- [#147 (PlanEstimates tree) PlanEstimates: a tree the planner emits and the runtime refines](#t147)
- [#20 (join enumeration) Join enumeration: DPccp/DPhyp cost-based tree reshaping](#t20)
- [#71 (GPU scan pushdown) GPU scan: no predicate pushdown into the cuDF read](#t71)
- [#19 (cardinality estimates) the planner has no cardinality estimate, and the memory model pays for it](#t19)
- [#16 (runtime filters) Dynamic / runtime filters: build-side keys → probe-side scan](#t16)
- [#75 (duckdb_cost refactor) Refactor duckdb_cost.py: separate cost formula from extraction](#t75)
- [#146 (aggregate shaping) aggregate shaping beyond the fixed sequence](#t146)
- [#158 (stats-answered aggregate) an aggregate DataFusion answers from statistics reaches no executor](#t158)
- [#142 (split large batches) split large batches](#t142)
- [#138 (ranged sorted merge) add ranged sorted merge](#t138)
- [#298 (DuckDB scan under limit) the DuckDB oracle charges a full scan under a limit DuckDB stops early](#t298)
- [#299 (DuckDB oracle accounting) the DuckDB oracle undercounts: a column a scan only filters on, a string's content, a merge join's output](#t299)
- [#300 (outer-join filter infer) a filter on an outer join's preserved side is not inferred onto the other side through the join's equality](#t300)
- [#301 (prototype pandas widths) the prototype measures its runs in pandas bytes, so its costs and the final ratio carry a per-query width error into the port](#t301)
- [#290 (outer/anti join key NDV) an outer or anti join's key NDV is cut to the other side's, though its rows are not](#t290)
- [#291 (prototype NaN as NULL) the prototype holds a float NaN as a NULL, so a NaN key groups with NULLs and never joins](#t291)
- [#294 (orientation build term) orientation picks a build by a term that prices nothing the join session does](#t294)
- [#295 (flat residual estimate) a residual inside a cluster is estimated at a flat 0.2, where the plan's estimator reads its predicate](#t295)
- [#297 (unpriced null-key filter) a reassembled null-key filter is priced by no order DPhyp compares, so tpcds q24 runs dearer than planned](#t297)

<a id="t73"></a>
### #73 (CBO umbrella) Cost-based optimizer (CBO) umbrella
Move physical planning from static heuristics to cost-based, validated against the DuckDB
cost oracle and goldens. In scope: adaptive filter placement, join enumeration (#20),
stats (#19, the load-bearing prereq), runtime filters (#16).

<a id="t101"></a>
### #101 (CSE/CTE reuse) No CSE / CTE materialization: identical subexpressions recomputed N times
**Priority: post-MVP**

DataFusion inlines every CTE reference and peacock re-scans each copy. Worst: tpcds q23
(CTEs ×5/×3/×2 → 6 scans), q4/q11 (`year_total` ×6/×4, though pushdown makes the copies
cheap: see below), q31, q2 (`wscs` ×2), q59 (`wss` ×2),
q65 (the `sa`/`sc` subquery ×2, written twice without a `WITH`); tpch q15 (`revenue0` ×2). Direction: CTE materialization or physical CSE; at minimum make the cost model aware.

The mechanism a materialized CTE needs — one producer, N consumers of the same batch
stream — is the one [#16](#t16) has to build first for dynamic filters, and it is cheap in
a streamed-batch model and expensive in a single-resident-table one. Sequence them
that way round.

**What it costs after join-optimizer.** Four queries' final ratios in `cost_report.html` remain
above 1.4, and in each one, most of the gap is a CTE computed again per reference. DuckDB builds
each CTE once (`CTE` / `CTE_SCAN` in its profile). The join order can't remove the copy:
optimized ÷ planned is 1.00 for tpcds q2 and q59. Figures below are tp1-single, from
`scripts/exec_model/testdata/goldens/{tpch,tpcds}/tp1-single.{cpu,costs}.txt` and
`testdata/goldens/*/<q>.duckdb_cost.txt`. "One copy" means the optimized cost minus the
duplicate subtree's nodes, an estimate.

| query | final ratio | duplicated CTE | per copy | optimized | one copy | DuckDB |
|---|---|---|---|---|---|---|
| tpch q15 | 2.03 | `revenue0` ×2 | lineitem load 11.8 MB, filter 5.4 MB, aggregate | 36.1 MB | ≈ 18.6 MB | 25.4 MB |
| tpcds q2 | 2.53 | `wscs` ×2 | ws ∪ cs load 34.6 MB, ⋈ date_dim 67 MB, project 67 MB | 420 MB | ≈ 210 MB | 209 MB |
| tpcds q59 | 1.98 | `wss` ×2 | store_sales load 69 MB, ⋈ date_dim 108 MB, project 108 MB | 587 MB | ≈ 293 MB | 347 MB |
| tpcds q23 | 2.73 | `frequent_ss_items` ×2 | ss ⋈ date_dim ⋈ item 86 MB, project 86 MB, 1.49M-group aggregate on `substr(i_item_desc)` 89 MB → 52 rows | 1.95 GB | — | 818 MB |

With one copy, q15, q2 and q59 come in at or below DuckDB's cost (≈ 0.75, 1.0, 0.85). In q2
and q59 the fact table joins only date_dim in each copy, and no dynamic filter narrows the
fact scan (q59's month filter applies only after the aggregate). DuckDB's dynamic filters
prune nothing there either.

q23 still costs more with one copy of `frequent_ss_items`, ≈ 1.55 GB, about 1.9× DuckDB. Two
causes outside this ticket remain:
- `best_ss_customer` (the corpus's text) has `FROM store_sales, customer, max_store_sales` and
  `HAVING sum(...) > 0.5 * max(tpcds_cmax)`, so the 1-row `max_store_sales` is crossed in
  below the group-by, in both engines. Ours crosses it onto the 2.75M store_sales ⋈ customer
  rows: 88 MB of cross join per copy, 176 MB of `cuda_join_bytes` for the two. DuckDB's join
  order crosses it onto customer, 100k rows (`CROSS_PRODUCT` 2.4 MB), before the hash join with
  store_sales. A cross join is a wall to our multi-join (`optimizer.md`, "Walls"), so DPhyp
  never places it.
- Projections charged as passthrough: 536 MB in q23. DuckDB streams projections at nothing.

tpcds q14 (1.83) and q31 (1.89) also have duplicated CTEs, but reuse alone does not close
their gap. The figures below are tp1-rowgroup, the mode their final ratio comes from.
Optimized includes the probe plans and the stopped runs.

| query | final ratio | engine ÷ prototype planned | optimized | DuckDB |
|---|---|---|---|---|
| tpcds q14 | 1.83 | 1.14 | 1.25 GB | 782 MB |
| tpcds q31 | 1.89 | 1.29 | 113 MB | 77.3 MB |

| component | q14 ours | q14 DuckDB | q31 ours | q31 DuckDB |
|---|---|---|---|---|
| storage + scan | 442 MB | 333 MB | 49.9 MB | 43.6 MB |
| hash join | 438 MB | 356 MB | 30.2 MB | 25.3 MB |
| passthrough | 366 MB | 0 | 32.5 MB | 0 |
| aggregate | 8.5 MB | 91 MB | 0.8 MB | 8.4 MB |

- **q14:** `cross_items` and `avg_sales` are inlined once per channel. The plan reads each
  fact table 7 times: 3 for `avg_sales`, 3 for `cross_items`, 1 for the channel. It also
  reads item 15 times and date_dim 21 times. DuckDB builds each CTE once and reads each fact
  table 3 times. The three replans keep each channel's `avg_sales` aggregate and semi-join
  build as memory sources, but the stopped runs that produce them still pay 543 MB. That cost
  shows up in storage and hash join, about 190 MB more than DuckDB together. Passthrough,
  366 MB, is the largest single excess.
- **q31:** DataFusion pushes each reference's `d_qoy` / `d_year` filter into its own copy of
  `ss` and `ws`. The 6 copies are therefore not identical, and a plain CSE would not match
  them. Each copy prunes its fact table to one quarter: store_sales 3/24 row groups, with
  group 23 read by all three copies. DuckDB pushes the OR of the references' filters
  (`d_qoy IN (1,2,3) AND d_year = 2000`) into the one shared CTE, reading 5/24 row groups,
  then filters per reference. Reuse here would need the same: the OR of the consumers'
  predicates pushed into the shared producer. The copies cost little: storage is 6 MB over
  DuckDB's, hash join 5 MB over. Most of the gap is 32.5 MB of passthrough. The 1.29 engine ÷
  prototype factor then raises a prototype ratio of 1.47 to the final 1.89.

tpcds q65 (final ratio 1.41, at tp1-rowgroup) has no `WITH`. The subquery that sums
store_sales by store and item is written twice: once as `sc`, and once inside `sb`, which
averages it per store. DuckDB finds the two copies identical on its own: its profile has one
store_sales scan under a `CTE` node, read by 2 `CTE_SCAN`s. We read store_sales twice
(`store_sales#1`, `#2`), each pruned by its own dynamic filter to 6/24 row groups, and join and
aggregate each copy. The replan keeps the joined result as a memory source, but the stopped run
pays for both copies. Against the prototype, q65 is at 1.12 (89.9 MB vs 80.2 MB). The engine ÷
prototype factor of 1.26 raises that past the 1.4 line. Physical CSE would catch q65 where
CTE-only reuse would not.

tpcds q4 (final ratio 1.49, at tp1-single) is listed above as a worst case, but once DataFusion
has pushed its filters down, duplication costs it little. `year_total` is a union of the three
channels, and it is referenced 6 times, each with its own `sale_type` and `dyear` filter.
DataFusion pushes both into each copy: `sale_type` keeps one branch of the union, `dyear` one
year. Each dynamic filter then prunes that copy's fact table to the year. So each fact table is
read twice, once per year. We read store_sales 6 + 7 row groups, catalog_sales 4 + 3 and
web_sales 2 + 2. DuckDB builds the CTE once over both years and reads 11, 6 and 3. The
duplicated work is only the row groups both years share. A CTE built once would read about the
same, and building it before the per-reference filters would read more. q4 is at 1.33 against
the prototype (1.12 GB vs 844 MB), times 1.12 engine ÷ prototype. By component:

| component | ours | DuckDB |
|---|---|---|
| storage + scan | 246 MB | 336 MB |
| hash join | 404 MB | 332 MB |
| aggregate | 42 MB | 176 MB |
| passthrough | 430 MB | 0 |

The passthrough is the whole gap, and this ticket does not reach it. q11 has the same shape
(`year_total` ×4, two channels). Reuse for a CTE whose references filter it differently has to
keep each reference's filter pushed into the shared producer, as DuckDB does for q31. Otherwise
it reads more than the copies do.

<a id="t140"></a>
### #140 (broadcast joins) broadcast joins (1:N partition broadcast)
Deferred by the design. Lets one partition (small dimension side) be broadcast to all N
partitions of the other side without shuffling the big side; also unblocks partitioned
cross/nested-loop joins. The blocker is consume-once: a GPU handle feeds exactly one
call, so a broadcast build needs either an explicit device copy (`GpuBatch::copy()` — 
keep `!Clone` so the cost stays visible at call sites) or a C++-side non-consuming/
refcounted handle. Interacts with #136's persistent-build option, which would solve both
at once.

**Corpus queries:** the cost is visible in pbench's `tp4-single` plan goldens, where the
small-table rule is off so `fact` really does hold four lanes. Three joins collapse all four to
one, each by a different route, and `pbench_shows_three_kinds_of_join_collapsing_four_lanes_to_one`
(`planner/tests/plan_goldens.rs`) counts the `GpuMergePartitions` that do it: `nl-inner` because a
keyless join merges both sides, `cross-projection` the same, and `sparse-probe-left` because
DataFusion plans it `CollectLeft`, which this engine merges rather than broadcasts — the shape
this ticket removes.

<a id="t170"></a>
### #170 (one-batch lane shortcuts) a source whose lanes each hold one batch could say so, and three shortcuts would fire

The loader declares `MultipleBatches` unconditionally
([architecture.md](architecture.md#modes-and-knobs)), so no downstream node may assume one
batch per partition. That was incremental simplicity rather than a missing fact: `scan_mapping/partition.rs`
computes the row-group → (partition, batch) mapping once at plan time and everything downstream
consumes it verbatim, so the batch count per lane is `partition_groups[lane].len()` — known in all
three batching forms, `Sized` included, since the planner cuts by bytes and the loader only
executes what it was handed.

The condition is that every lane holds exactly one batch, `SingleBatch` being a property of the
node rather than of a lane: a source with lanes of one and two batches stays `MultipleBatches`.

Saying it fires shortcuts the aggregate sequence already specifies: a 1-partition single-batch
input needs one `GpuAggregate` carrying both `aggs` and `final`, and a single-batch-per-partition
input skips the first `GpuAggregateBatches`. Join build sides need nothing new — `translator/nodes.rs`
already elides their coalesce when the input is `SingleBatch`. So the change is one declaration
and the plans get smaller by themselves. Every plan golden moves, which is its real cost.

<a id="t141"></a>
### #141 (small-key shuffle skip) the planner cannot skip the shuffle for small group-key sets
v1 skips `GpuMergePartitions` + `GpuEmitPartitions` around an aggregate only when the
input is already one partition or the aggregate is keyless. Skipping when the key set is
merely small (collapse to one partition, run `GpuAggregateBatches[final]` once, avoid the
shuffle) needs a cardinality estimate that does not exist — the estimators are constants
(#19). When stats land, add the rule and regenerate the affected plan goldens.

<a id="t139"></a>
### #139 (post-filter coalesce) GpuCoalesceBatches(target): compact post-filter fragments
Dropped from v1. After a selective filter, batches shrink to a few rows and every
downstream kernel pays per-launch overhead on each fragment. A `BatchAccumulator` that
concatenates to a minimum target size (DataFusion semantics: merge only, never split),
streaming out one batch whenever the threshold is crossed. `cudf::concatenate` via the
existing collapse arm — no C++ change; target size from the same budget rule that sizes
loader batches. The T0 prototype has the node
(`scripts/exec_model/operators/accumulators.py`, `ReBatchToTarget`) so the drivers are
shown to tolerate one at any tree position; it also splits, which the ticket's node does
not need, because the prototype uses it to make a stream's batches any shape.

<a id="t147"></a>
### #147 (PlanEstimates tree) PlanEstimates: a tree the planner emits and the runtime refines
The planner's `target_batch_bytes` walk already computes a per-node maximum resident size and
throws all but one number away. Keep it, as a tree shaped like the plan, one estimate per node.

`ParquetBatchPartitioner` emits it beside the row-group mapping. Nothing in the plan's
executability depends on it, but a wrong estimate is not free: too low and the query dies at the
accountant's `scratch_bytes` pre-check, or as a cuda OOM below that. Neither is a wrong answer,
and #142 handles both gracefully later; a better estimate makes fewer queries reach either. Two
consumers, neither existing yet. **Placement** moves subtrees onto the CPU where the GPU cannot
hold them — the `Backend` trait already makes that a matter of choosing per node. **Refinement
in flight** extrapolates from one batch actually read, since the estimates otherwise rest on
constants (#19). The first version rewrites only what needs no replanning, a still-reading
loader's remaining batch sizes; later revisions may replace the plan outright, killing
in-progress GPU work and rebuilding the driver rather than editing the running tree — which is
why the driver owns no state a caller must survive it.


<a id="t20"></a>
### #20 (join enumeration) Join enumeration: DPccp/DPhyp cost-based tree reshaping
DataFusion 45 has no join enumerator — trees come out in FROM-clause order, and ~70/99
TPC-DS queries have 4+ joins (q64 ≈ 18 tables). Implement DPccp (extend to DPhyp, IKKBZ
fallback beyond 14 tables) as a logical rule after PushDownFilter, cost = Σ intermediate
cardinality. Blocked by #19. Landing rewrites all plan goldens.

<a id="t71"></a>
### #71 (GPU scan pushdown) GPU scan: no predicate pushdown into the cuDF read
Partly addressed: stats-based row-group pruning exists
(`planner/translator/scan_mapping/rowgroup_prune.rs` → cuDF `set_row_groups`, parity with
ParquetExec). Remaining: serialize the predicate itself
into the cuDF `read_parquet` filter AST (page pruning / pre-filter during decode),
multi-file scans, dynamic ranges (#16). Cause of red widget ratios on selective queries.

<a id="t19"></a>
### #19 (cardinality estimates) the planner has no cardinality estimate, and the memory model pays for it
Widths are facts and source rows are facts — the schema, and the `rows`/`bytes` a scan reads
off its surviving row groups at plan time (`planner/translator/scan_mapping/parquet_meta.rs`).
What a query
does to them is guessed: `estimator.rs::rows` has a filter pass every row, an aggregate emit
one group per input row, and a join emit its larger side.

It shows twice, both in `estimate`. An accumulator is charged what it holds whatever the batch
size, so an aggregate's state is priced at its whole input — tpch q1 groups six million rows
into four and is billed six million — and that total comes off the budget before anything is
divided, so `share_per_source` and every batch size derived from it come out smaller than the
plan needs. `rows_are_certain` is what keeps the error one-directional: only the part of the
accumulator total that rests on facts can refuse a plan, so a guess shrinks batches and never
rejects a query.

Not part of this any more: build/probe order is DataFusion's own JoinSelection over its own
nodes, which nothing of ours now sits between — the 55-of-103 measurement this ticket opened
with was of a wrapper tree that is deleted. #147 is the mechanism a real estimate would arrive
through; #73 and #20 are what it unlocks.

<a id="t16"></a>
### #16 (runtime filters) Dynamic / runtime filters: build-side keys → probe-side scan
Star-schema fact scans read 100% of rows while the joined dimension is filtered to ~30%. Build
an IN-set / min-max (later Bloom) at build completion and feed the probe-side GpuScan.

Applies to 76/99 TPC-DS queries; validate on q3/q19/q33. Best after #19. **Design it as the
groundwork for CTEs, not a join-to-scan special case.** A dynamic filter is the first thing here
whose producer has two consumers: the build side feeds the join, and it also feeds a replanning
consumer that turns those keys into a predicate on a scan below. That is a diamond, and every
plan model here is a tree. What serves it — a fork handing one batch stream to N consumers, plus
a consumer that plans rather than executes — is what a materialized CTE needs ([#101](#t101))
and what [#147](#t147) calls refinement in flight. Do it after the streamed lanes, which
suit the shape: a `TableResult` shares its columns ([#145](../archive/archived-tickets.md#t145)),
so a tee costs nothing on the device, the
accountant already models a fork's residency as the slowest consumer's backlog, and a consumer
blocking its producer is the join hold, one rule already mutation-tested. A diamond in the plan
is then routing rather than scheduling.

<a id="t75"></a>
### #75 (duckdb_cost refactor) Refactor duckdb_cost.py: separate cost formula from extraction
**Priority: low** — not started (867 lines, 29 top-level defs as of 2026-08-05), and it buys
readability, not behaviour.

~870 lines where the ~80-line cost formula hides inside JSON parsing, predicate parsing
and row-group pruning. Shape: preprocessor → flat per-node intermediate
(op/rows_read/bytes_read/out_rows/out_bytes/breaker) → one-line cost. Pure refactor:
`.duckdb_cost.txt` numbers must not move.

<a id="t146"></a>
### #146 (aggregate shaping) aggregate shaping beyond the fixed sequence
**Priority: low** — each part optimizes an already-correct plan and needs the same estimate.

The aggregate sequence applies one shape everywhere — per-batch init, merge per lane,
shuffle, finalizing merge — right where group cardinality is far below row count.

Three shapes it cannot express or choose. **(a) A merge accepting raw rows.**
`GpuAggregateBatches` requires pre-aggregated state, so the per-batch `GpuAggregate` is
mandatory: a lane with few batches pays B groupbys where one over the concatenation would do,
and a non-reducing aggregate pays an init that shrinks nothing. A raw-accepting merge holds
rows where a state merge holds groups, so it fits only the non-reducing case. **(b) A loader
emitting one batch per partition**, where its consumer materializes the whole partition anyway
— decide it from the subtree beneath the loader. **(c) The cardinality estimate all three
want** — does this aggregate reduce? — which the constant estimators (#19) cannot answer and
which gates [#141](#t141). Land after #19.

<a id="t158"></a>
### #158 (stats-answered aggregate) an aggregate DataFusion answers from statistics reaches no executor
`SELECT count(*) FROM nation` never reaches an `AggregateExec`: DataFusion's
`AggregateStatistics` rule answers it from parquet metadata and emits `PlaceholderRowExec`
holding the result.

The mode refuses it at plan time, so nothing here runs it. The rule fires wherever the input's
row count is exact, not only over a bare table: a cross join multiplies two exact counts, so
`select count(*) from region, nation` is refused too, and so is a `count(*)` over a `LIMIT`
subquery (`Exact(3)` for `limit 3`). A WHERE, a GROUP BY or an equi-join makes the count inexact
and stops it. No corpus query reaches it: all 31 using `count(*)` filter, group or equi-join.
Tests that want a count over a limit or a cross join use `count(<column>)`, as #186's and #63's
do.

The fix is small on the CPU and unavailable on a device: the node is a source of constant rows,
and a table of literals made from no input is what the frozen surface has no call for — the same
wall as [#173](../archive/archived-tickets.md#t173) and [#175](../archive/archived-tickets.md#t175). T17 was to have discharged it and did not: writing the
CPU half alone makes the oracle answer a query the device refuses, and the oracle is what the
device is checked against. Waits on the make-a-table-of-literals call all three want.

<a id="t142"></a>
### #142 (split large batches) split large batches
Nothing downstream of the loader can split a batch: minimum load granularity is one row
group, `GpuCoalesceAllBatches` before a join build side can exceed any budget, and the
planner deliberately still produces a plan — `executor/driver/accounting.rs` then trips at run time and
the query dies cleanly. Recourse options, deferred until better estimators and adaptive execution: a split
operator (needs a C++ slice-to-handles entry point), or adaptive replanning on trip (re-plan
with more partitions or smaller batches) — the second being the only one that would make a trip
anything other than the end of the query.

Both checks abort today, pre-call and post-call alike, and the pre-call one refuses on an
estimate rather than on a fact. Recording it and letting the call proceed is the cheaper
recourse and is deliberately not taken: `RunError::BudgetExceeded` now carries which check
tripped, so something can branch on it, but there is nowhere to record into — `RunReport` has no
trip log, and `Underestimate` is the precedent for what one would look like. Related: #91.

<a id="t138"></a>
### #138 (ranged sorted merge) add ranged sorted merge
`GpuAccumulateBatchesAndSort` and `GpuMergeSortedPartitions` run one `cudf::merge` over all
sorted inputs and materialize the whole output, so the local peak is inputs + output.

cuDF has no streaming merge. The hand-rolled alternative is a ranged merge: pick split keys,
`cudf::slice` each sorted batch at `upper_bound` boundaries (zero-copy views), merge range by
range, emit and release each. That bounds the output term to one range; the inputs stay resident
either way, so the win is at most ~2x on the sort's local peak. Do it only if sort peaks bind
after the mode ships — it also unlocks multi-batch output from merge nodes.

Landing it re-introduces a third `SortOrder` state. The enum is two-valued today because every
node that orders a whole stream emits one batch, so "stream sorted" is `BatchSorted` meeting
`SingleBatch` and is derived rather than declared. Ranged emission produces the one shape that
breaks that — a stream ordered across several batches — so it must add `PartitionSorted` back
and teach the limit-after-sort validation to accept it alongside the derived form.

<a id="t298"></a>
### #298 (DuckDB scan under limit) the DuckDB oracle charges a full scan under a limit DuckDB stops early
`duckdb_cost.py`'s `scan_cost` reads a scan's rows as every surviving row group's (`rows_fetched`),
so a scan a `STREAMING_LIMIT` stops is charged as read in full. tpch `scan-limit` profiles 2,048
rows scanned of lineitem's 6,001,215, and its golden charges all 49 row groups, 1.04 GB of
`storage_read_total`: the engine's 1.05 GB reads as a ratio of 1.01, where DuckDB read one row
group. tpch `nested-limits` is the same through pass 1: DuckDB's default plan reads `part` by
`rowid>=5 AND rowid<=44` (pass 2's bounds, a late-materialization semi join), pass 1 turns that
off, and the golden charges the full 200,000-row scan, 4.83 MB of its 4.85 MB.

Fix: under a `STREAMING_LIMIT`, charge the lesser of the pruned row groups and those covering
`operator_rows_scanned`; a `rowid` range from pass 2 bounds the scan the same way. tpcds q28, q32,
q38 and q97 hold scans under a limit too, each scanning its whole table: check their goldens
hold, and the two tpch ratios become comparable.

<a id="t299"></a>
### #299 (DuckDB oracle accounting) the DuckDB oracle undercounts: a column a scan only filters on, a string's content, a merge join's output
`duckdb_cost.py` charges a DuckDB scan's `storage_read_total` as the decoded bytes of the columns
it references, in the row groups that survive: `compute_scan_pruning` takes `Projections ∪
filter_columns(Filters) ∪ filter_columns(dyn_filter)`. A filter DuckDB evaluates inside the scan
does not emit its column, so a column used only by the filter counts only if `filter_columns`
finds it. `filter_columns` takes an identifier followed by `>=`, `<=`, `>`, `<` or `=`, or one
inside `CAST(… AS`. It finds nothing in `o_comment !~~ '…'` (NOT LIKE), `p_name ~~ '…'` (LIKE),
`prefix(p_type, …)`, `contains(…)`, `suffix(…)`, `x IS NULL`, `x IS NOT NULL`, `x IN (…)` or
`x != 5`. (`<>` works only because its `<` matches.) When the table has no parquet
statistics, `scan_cost` falls back to rows read × the scan's output row width, which leaves out
every filter-only column whatever the operator. The engine's `GpuLoadParquet` charges every column
it loads, filter columns included. So the two sides are measured on different bases exactly where
a filter is not a plain comparison, and DuckDB's cost reads low.

Corpus queries the committed goldens undercount:

| query | column left out | filter | size, est. |
|---|---|---|---|
| tpch q13 | `orders.o_comment` | NOT LIKE | ≈ 85 MB, measured: DuckDB's `bytes_read` 24.4 MB (2 key columns), the prototype's load 108.8 MB |
| tpch q9 | `part.p_name` | LIKE | ≈ 7 MB (200k × ~33 B) |
| tpch q2 | `part.p_type` | LIKE / `suffix` | ≈ 5 MB (200k × ~25 B) |
| tpch q16 | `supplier.s_comment` | NOT LIKE | ≈ 0.6 MB |
| tpcds q76 | `ss_store_sk`, `ws_ship_customer_sk`, `cs_ship_addr_sk` | IS NULL | ≈ 40 MB |
| tpcds q44 | `ss_addr_sk` | IS NULL | ≈ 23 MB |

tpch q13's final ratio in `cost_report.html` is 1.70, and the optimizer changes nothing: the plan
already has DuckDB's shape. All of the gap is `o_comment`. With it counted, DuckDB's cost is
≈ 179 MB, and the ratio is ≈ 0.86 on the engine's figure, ≈ 0.92 on the prototype's. The other
rows have smaller shares, and every ratio they appear in reads high by them.

**A string's content is not counted where DuckDB materialises it.** The materialised side of
the cost (`node_materialized`) is DuckDB's `output_bytes`, its `result_set_size`. That counts a
string as its 16-byte `string_t` header, whatever its length. It also counts an integer at
DuckDB's own width (int32 4 B). The storage side counts Arrow bytes, offsets plus content
(`compute_pruning`'s decoded bytes), and so does the engine for every node. In tpch
`cross-join` (`SELECT * FROM region, nation`), DuckDB's `CROSS_PRODUCT` is 9,500 B for 125
rows, 76 B/row (3 ints × 4 + 4 strings × 16). The engine's `GpuCrossJoin` is 22,898 B,
183 B/row, because `r_comment` and `n_comment` run ~70–110 characters. DuckDB's scans show the
same split: nation 40 B/row materialised (4 + 16 + 4 + 16), 2,442 B read. Every query whose
pipeline breakers carry strings reads low on DuckDB's side by their content: joins and
aggregates carrying names, addresses and comments (tpch q2, q10, q18), and string group keys
(tpcds q23's `substr(i_item_desc, 1, 30)`).

**A piecewise merge join is charged its inputs, not its output.** For an inequality join DuckDB
plans `PIECEWISE_MERGE_JOIN`, which `duckdb_cost.py` puts in `BUILD_INPUT_OPS` with the
aggregates and sorts: it is charged its children's `output_bytes`. `HASH_JOIN`,
`NESTED_LOOP_JOIN` and `CROSS_PRODUCT` (`JOIN_BUILD_OPS`) are charged their own output plus
any non-scan child's. The engine charges its `GpuNestedLoopJoin` its output. tpch
`nested-loop-join` and `nested-loop-left-join` (`region` against `nation` on `<` / `>`): DuckDB
charges the merge join 1,180 B, its two inputs, against its 3,800 / 3,876 B output. The engine
charges 9,041 / 9,477 B.

Together with the string widths, these two are most of the three named queries' ratios in
`cost_report.html`: cross-join 3.63, nested-loop-join 4.10, nested-loop-left-join 4.26. All
three plans have DuckDB's shape.

Fix:
- Take a filter's columns as every identifier in the `Filters` / `dyn_filter` string that names a
  column of the scanned table, read from the parquet schema `compute_pruning` already opens.
  Today the column is taken only from the left of a comparison. Ranges for pruning still come
  from `parse_range_filters`; only the referenced set grows.
- The no-statistics fallback: add the filter-only columns' widths, or warn that the read is
  short.
- Strings: price a materialising operator's string columns as Arrow does, offsets plus content.
  The 16-byte header leaves the content out. The content can come from the parquet column's
  decoded size per row (already read for storage), carried up by column name through the
  operators that pass the column on. A string an operator computes (`substr`, `concat`) has no
  stored size: use the profile's own size for it, and say so. Decide whether integers move to
  the engine's 8 B in the same change, and record it in `design.md` beside "Forwarders are
  free on both sides".
- Merge join: move `PIECEWISE_MERGE_JOIN` from `BUILD_INPUT_OPS` to `JOIN_BUILD_OPS`, charged
  its output plus its non-scan children's, as the other joins are. Check whether
  `IE_JOIN`, `ASOF_JOIN` and `POSITIONAL_JOIN` appear in any committed profile, and place them
  the same way.
- Tests in `test_duckdb_cost.py` beside `test_filter_columns_includes_cast`: one per filter form
  above, q13's orders scan charged with `o_comment`, a string-carrying cross product charged its
  content, and a merge join charged its output.
- Regenerate with `gen_duckdb_cost.sh --extract-only` (needs the sf1 parquet under `testdata/`),
  then `cost_report.html`. The filter fix moves only the six rows above. The string fix moves
  every query that materialises a string, so list the ratios that cross 1.4 either way.

<a id="t300"></a>
### #300 (outer-join filter infer) a filter on an outer join's preserved side is not inferred onto the other side through the join's equality
tpcds q78 filters `ss_sold_year = 2000` and left-joins `ws` and `cs` on `ws_sold_year =
ss_sold_year` and `cs_sold_year = ss_sold_year`, plus item and customer keys. A row of `ws` or
`cs` from another year can never match, so `ws_sold_year = 2000` and `cs_sold_year = 2000` hold
on the right sides. Each year is a group key of its CTE, `d_year`, so the filter can go on down
to date_dim and, as a dynamic filter, to each fact table's `*_sold_date_sk`. DuckDB infers this:
its profile has date_dim `filter=[d_year=2000]` three times. It prunes store_sales to 7/24 row
groups, web_sales to 2/6 and catalog_sales to 3/12, and aggregates one year per channel.

Our plan carries the filter into `ss` only. The dynamic filter on the plan as written has a
single candidate, `d_date_sk -> store_sales` (24 → 7 row groups): the other two date_dim reads
hold no predicate. `ws` aggregates every year (`m1`, 647,541 rows, `d_year` NDV 6), and
`catalog_sales` is loaded whole, 1,441,548 rows and 80.7 MB at tp1-rowgroup. Then it goes
through the catalog_returns join, the `IS NULL` filter, the date_dim join and the aggregate, a
year's worth of work done ~4 times over. q78's final ratio is 1.82 at tp1-rowgroup: 1.37
against the prototype (1.20 GB vs 875 MB), times 1.34 engine ÷ prototype. Cutting ws and cs to
one year saves ≈ 0.5–0.6 GB of the prototype's 1.2 GB, an estimate from the per-node bytes,
which puts q78 under DuckDB.

The join-order work can't do this: DPhyp reorders the joins it is given, and the dynamic filter
needs a filtered build to start from. The rule: for `L ⟕ R ON L.a = R.b` with a filter `f(L.a)`
on L (above the join or pushed into L), add `f(R.b)` to R. For `L ⟗ R` add nothing. For an
inner join add it both ways (DataFusion's filter pushdown may already do the inner case; check).
Then the existing pushdown carries it through R's aggregate, where `b` is a group key, down to
the scan.

Fix: the inference, in a logical rewrite before physical planning or upstream in DataFusion's
`PushDownFilter`. Tests: q78's plan has a `d_year = 2000` filter under each of the three CTEs,
and its answer is unchanged against DuckDB at every mode. Sweep the corpus for other outer joins
whose preserved side carries an equality filter on a join key, and check the goldens of each
query whose plan moves.

<a id="t301"></a>
### #301 (prototype pandas widths) the prototype measures its runs in pandas bytes, so its costs and the final ratio carry a per-query width error into the port
The prototype's decisions are priced in the engine's widths. `optimizer/cost.py`'s `row_width`
sizes a column as `type_structural_size` does: Decimal128 16 B, Date32 4 B, a string's offset 4 B
plus its mean length from the sidecar, ⅛ B of null bitmap. Its runs are measured in pandas
bytes: an operator's output is `memory_usage(index=False, deep=True)` (`operators/source.py`,
`operators/joins.py`), and `plans/cost_model.py` prices those bytes. Per value:

| type | prototype run | engine |
|---|---|---|
| decimal | 8 (float64) | 16 (decimal128) |
| date | 8 (datetime64) | 4 (date32) |
| string | len + 8 (large_string offsets) | len + 4 |
| int64 | 8 | 8 |
| null bitmap | 0 | ⅛ per column |

These rules reproduce the loads exactly: tpch q1's lineitem is 58 B/row in the prototype and
78.875 in the engine (4 decimals, 2 one-character strings, a date). tpch q15's supplier is
899,771 B against 784,783.

How much the error scales a query depends on its column mix. That is why the prototype's
planned cost over the engine's has median 0.85 (`design.md`) but a wide spread. tpcds q5 reads
only int keys and decimals: ×1.52 at its scans, ×1.68 at its union projections (4 decimal
columns of 6), ×1.58 overall, 762 MB engine against 482 MB prototype. So the prototype puts q5's
optimized run at 0.99 of DuckDB's cost, and the final ratio puts it at 1.56. Every figure taken
from a prototype run is off by the query's own factor: `.costs.txt`, the
probe and stopped shares, the measured comparisons a rule was kept or dropped on (orientation
priced as the session holds it, tpch q9), the 1.4 line's colours. The final ratio, E × (O / P) /
D, cancels the factor only where it is uniform across the plan. A rule that removes
decimal-heavy work and keeps string-heavy work, or the reverse, is projected wrong by the ratio
of the two parts' factors.

When the optimizer is rewritten into the engine:
- Measure in the engine's bytes, which the engine's own run gives for free. Don't port
  `memory_usage(deep=True)` or anything that sizes a batch from a pandas frame. A memory
  source's size and a probe's or stopped run's cost come from the engine's batches.
- Retire the projection. The final ratio becomes the engine's optimized cost over DuckDB's,
  E_opt / D, with no O / P.
- Re-measure every decision the prototype kept on a measured byte comparison, on the engine's
  runs over the same sample. Decimal-heavy and string-heavy queries first: tpcds q5, q2 and q59
  (decimal), and tpch q13 and q9 (strings). Write down any that turn around.
- Pin it: a test that a scan's measured output bytes equal `row_width` × rows for a
  statistics-exact relation (a scan with no filter). Estimate and measurement then share one unit.

Until then, read a prototype figure next to its query's engine ÷ prototype factor (E / P in
`cost_report.html`). Separate from this: the engine holds TPC-DS's `decimal(7,2)` as decimal128,
16 B, where DuckDB materialises it as int32, 4 B (24 B/row against the engine's 48.5 for q5's
store_sales). Part of every decimal-heavy ratio is that choice in the engine, not the plan.

<a id="t290"></a>
### #290 (outer/anti join key NDV) an outer or anti join's key NDV is cut to the other side's, though its rows are not
`_matched_keys` (`scripts/exec_model/optimizer/cardinality.py`) cuts both sides' key NDV to the
smaller side's for every join type, so what reads the key above an outer or anti join undercounts.
A left join of 1,000 keys against 10 is 1,000 rows with key NDV 10, true 1,000; a left anti join
is 990 rows, NDV 10. The rows are right since #289; the NDV is not. tpch q13 groups by `c_custkey`
over `customer ⟕ orders`: the final aggregate is estimated at 99,996 groups, orders' customer
count, against 150,000. Only an inner or semi join's output holds just the matched values; a
preserved side keeps its own NDV, and an anti join's is at most its own less the matched ones.

<a id="t291"></a>
### #291 (prototype NaN as NULL) the prototype holds a float NaN as a NULL, so a NaN key groups with NULLs and never joins
pandas has one missing value for a float column, so the prototype reads a NaN as a NULL
(`operators/frame.py`, "Known divergences"). DuckDB and the device equate NaN with NaN as a key; the
prototype folds the NaN group into the NULL one and drops NaN keys from a join. pbench's
`float32-key-group` and `float64-key-group` answer 51 groups against DuckDB's 52, and
`float64-key-join` 772,357 rows against 779,611, at all five modes; the other 53 or 54 of pbench's
queries match. The engine's lines for the three are commented out on #243.

Fix: hold NULL apart from NaN in the prototype's frames — a nullable float dtype whose `<NA>` is the
NULL and whose NaN is a value — from `tables.typed` through the group keys and the hash join's null
hold-out.

<a id="t294"></a>
### #294 (orientation build term) orientation picks a build by a term that prices nothing the join session does
Orientation picks a join's build by a term that prices nothing the session does. It builds the
side with the lower `bytes(build) × (1 + copies × batches(probe))`, `copies` = 1, a term written
for a build copied per probe batch. The session builds once and copies nothing, holding the build
and its hash table (`plan::session_bytes`, 16 bytes a row); the cost model has no per-batch term
for the probe either. The term no longer prices a copy; it stays because pricing the build as the
session holds it measured worse.

That pricing, bytes plus 16 bytes a row, won on tpch q18 at tp1-rowgroup (0.465 to 0.363) and
tpcds q64 at tp4-rowgroup (0.549 to 0.499), but moved 23 of 130 sample runs (26 queries at five
modes) for a net change near zero, and made tpch q9 dearer at every mode by 2–5 %. What the term
should price — the probe's own per-batch cost beside the build's, or something else — is open.

<a id="t295"></a>
### #295 (flat residual estimate) a residual inside a cluster is estimated at a flat 0.2, where the plan's estimator reads its predicate
DPhyp's cost model keeps DuckDB's default share, 0.2, of a join's rows for every residual edge
(`join_order.py`, `estimator.DEFAULT`); the plan's cardinality estimate reads the same predicate
through `estimator.selectivity` (`cardinality._joined_selectivity`). Since a non-inner join with a
residual enters a cluster, the flat share decides orders.

tpch q21: the semi join on `l_orderkey` with the residual `l_suppkey <> l_suppkey` is estimated at
31,348 rows in the second replan and comes out at 151,237 (q-error 4.8). DPhyp runs it before the
orders join on the estimate, and a third replan follows. q21 goes from 0.726 to 0.733 at tp1 and
0.640 to 0.642 at tp4; tpcds q16 and q94, the sample's other residual semi joins, do not move.

Fix: estimate a residual edge by its predicate, as the plan's estimator does, over the columns of
the two relation sets it joins.

<a id="t297"></a>
### #297 (unpriced null-key filter) a reassembled null-key filter is priced by no order DPhyp compares, so tpcds q24 runs dearer than planned
Reassembly puts the plan's null-key filter back below each shuffle of a key the plan filtered, and
DPhyp's C_out prices joins alone, so no order's cost holds that filter.

tpcds q24 at the three tp4 modes: the plan filters `ss_store_sk` and `ss_customer_sk` after the join
with store_returns. DPhyp's order joins store_sales to store and customer first, so the filter lands
on the store_sales shuffle (`disassembly`'s `_wired`). Against the plan the optimized run pays 187.7
MB more filter and saves 52.6 MB of hash join, 67.1 MB of scatter and 61.6 MB of passthrough: 6.3 MB
dearer, ratio 1.009. q54 at tp4 pays 10.8 MB more filter than its plan and stays cheaper.

Fix: price a reassembled filter in DPhyp's cost, the bytes of each side shuffled on a filtered
key, or keep the filter where the plan had it.
