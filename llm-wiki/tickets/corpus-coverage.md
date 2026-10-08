
# Corpus Rollout tickets

Tickets required for corpus rollout (CPU+GPU, all modes), TPC-H numbered and named queries, and 84/99 TPC-DS queries (window functions excluded). Minimum SQL functionality needs to be built for this milestone.


## Contents

- [Welford aggregation](#welford-aggregation)
  - [#225 — the device names every Welford state column by the same alias](#t225)
  - [#216 — the device's global aggregate has no Welford arm](#t216)
  - [#94 — MERGE_M2 count-child type is cuDF-version-specific](#t94)
- [Aggregates](#aggregates)
  - [#199 — a global aggregate over no arrival drops its identity row](#t199)
  - [#55 — q66: two-phase decimal aggregate ignores the partial-phase divisor cast](#t55)
  - [#65 — the device's grouping-set id is not DataFusion's value or width](#t65)
  - [#62 — a DISTINCT beside an avg or a count is refused at planning](#t62)
- [Sort / Limit](#sort--limit)
  - [#202 — a descending sort key puts its nulls on the wrong end on the device](#t202)
  - [#217 — a sort with `fetch 0` keeps every row on the device](#t217)
  - [#204 — the device's sorted merge drops its fetch when it is handed one input](#t204)
  - [#214 — a limit drops a zero-row batch on both backends](#t214)
  - [#205 — the cpu's accumulating sort and merge answer nothing over zero-row batches](#t205)
- [Scalars](#scalars)
  - [#168 — interval type can not be represented in the fbs ScalarValue](#t168)
  - [#210 — a bare decimal literal on the AST path comes back as a Float64 column](#t210)
  - [#57 — the device refuses a value-form CASE](#t57)
  - [#56 — q2: CASE-over-string-equality inside a partial-phase sum](#t56)
  - [#60 — `round(x, p > 0)` on the device differs from DataFusion by one ulp](#t60)
- [Source](#source)
  - [#186 — a limit pushed into the scan: the cpu ignores it, the device refuses it](#t186)
- [Repartitioning](#repartitioning)
  - [#206 — a float or boolean partition key is refused on the device](#t206)
  - [#240 — a timestamp partition key is refused on the device](#t240)
  - [#189 — the shuffle cannot hash a rollup's grouping-set id](#t189)
  - [#145 — Refcounted handles: stop copying every partition out of a scatter](#t145)
  - [#95 — a decimal partition key is refused on the device](#t95)
  - [#197 — the repartition arm still concatenates a child it can only be handed one of](#t197)
- [Performance](#performance)
  - [#154 — every operator exit path deep-copies its output into a fresh table](#t154)
- [Testing](#testing)
  - [#227 Check schema nullability in tests](#t227)
  - [#164 — a column ordinal reaches cuDF unchecked, and a bad one degrades rather than throws](#t164)
  - [#201 — the murmur gate proves a copy of the lane rule, not the rule](#t201)
  - [#174 — two clamps for one rule, and nothing compares them](#t174)
  - [#233 — the plan validator does not check that a pass-through node keeps its input's column count](#t233)
  - [#234 — a mid-plan limit is counted twice, by the driver and by its executor, and nothing compares them](#t234)
  - [#235 — no independent oracle checks the result goldens](#t235)

## Welford aggregation

<a id="t225"></a>
### #225 — the device names every Welford state column by the same alias

The plan declares a `stddev` or `var` state as `<out>$count`, `<out>$mean`, `<out>$m2`; the
device holds all three under `<out>`, at the init and at the merge alike, the types as declared.

The wire folds the triple into one `AggregateFuncNode` with one `alias` (`plan/aggregate.rs`,
`state_funcs`, `welford: true`), and `aggregate.cpp`'s Partial and Merge arms push each child
under it. No answer is wrong today: the merge packs the triple by offset and the finalize reads
it by ordinal; a reader resolving a state column by name would take the wrong one. The fix is
the three names on the wire, or the suffixes appended in `aggregate.cpp`. Pinned by
`bug_stddev_holds_three_identically_named_columns` (the init) and
`bug_stddev_merge_holds_three_identically_named_columns` (the merge)
in `gpu_tests/aggregate_schema_cases.rs`. 2026-09-17: the corpus's schema validator refuses
`tpch/shuffle-stddev` at its `GpuAggregate` on these names, so its row says
`schema_validation_disabled`; the cell stays enabled and its values match.

**Corpus queries:** `tpch/shuffle-stddev` (schema validation only).

**Fix proposed:** add `state_names: [string]` to `AggregateFuncNode`. `state_funcs` fills it
from the owner's `positions` in the state schema. `aggregate.cpp`'s Partial and Merge arms name
the triple from it; the Final arm, which answers one column, already names it right. Not
suffixes in C++: `$count`/`$mean`/`$m2` is DataFusion's convention, and a second copy drifts. Then both `bug_` tests expect `None`.

In the same append, a `ddof: int8` beside `state_names`, which `state_funcs` fills from
`AggStateColumns.ddof`. `aggregate.cpp` reads it where it now calls `stddev_ddof(func_name)`, and
that helper, which re-derives from the aggregate's name what the planner already holds, goes.
Not wrong today: every name the writer emits maps to the right ddof. It rides along because this
is the wire change it needs (`reports/hacks-audit.md` §5, aggregate half).

<a id="t216"></a>
### #216 — the device's global aggregate has no Welford arm

A keyless `stddev` answers one finished `Float64` on the device where the plan declares the
`[count, mean, m2]` state, so the finalize above it fails; a keyless `var` is refused outright.

`aggregate.cpp`'s grouped path honours `mergeable` and emits the triple with `MERGE_M2`; its
keyless path (`key_cols.empty()`) tests `is_stddev_name` alone and reduces that name with
`make_std_aggregation`, whatever the phase — at the init the sample stddev of the argument, at
the merge the stddev of the state's first column, the count — and the finalize project refuses
with `ColumnRef index 2 out of range (cols=1)`; a `var` name falls to `make_reduce_agg`'s
`unsupported aggregate function: var`. So `SELECT stddev(x) FROM t` and `SELECT var(x) FROM t`
are refusals on the device in every shape. Pinned in `aggregate_dimension_cases.rs` by the
four `bug_` cases `…welford_init_answers_a_finished_stddev…`, `…keyless_welford_merge…`,
`…global_stddev_finalize_is_refused…` and `…keyless_var_merge_is_refused…`; the init's one
column read at the handle by `aggregate_schema_cases.rs`'s `bug_a_global_stddev_holds_…`.

**Corpus query:** none yet; the simplest is `select stddev_samp(l_quantity), var_samp(l_quantity)
from lineitem;` (tpch).

**Fix proposed:** in `execute_aggregate`, a keyless node with any stddev or var runs whole through
the grouped path on one constant `INT32` key, dropped before return — cuDF has `M2`/`MERGE_M2`
for groupby only. The plan and recipes do not change. Empty input still answers one row, built
per aggregator: a count 0, the rest NULL. The keyless `is_stddev_name` arm goes. After #225, so
its tests can assert `holds_as_declared`.

<a id="t94"></a>
### #94 — MERGE_M2 count-child type is cuDF-version-specific
The stddev/var Merge arm (`cpp/src/operators/aggregate.cpp`, the `AggPhase::Merge` Welford
branch) casts `valid_count` to INT32 for the 25.02 GPU runtime; 25.10 and later accept only
INT64 or FLOAT64 (`group_merge_m2.cu`). The Final arm casts the same way but never runs. The
26.02 CI leg is build-only so it stays green — this bites at the next GPU-remote cuDF bump.
Switch or version-gate the type then; the comment marks the site.

The plan never sees this type: it declares every `$count` state `Int64`, and so do the columns
between calls. The INT32 lives inside the Merge call alone — cast in before `MERGE_M2`, widened
back to INT64 before it returns. On 25.10 and later the count stays INT64 throughout.

**Fix proposed:** gate at compile time. `cpp/CMakeLists.txt` passes `cudf_VERSION`'s major and
minor as `PEACOCK_CUDF_VERSION_*` defines; `aggregate.cpp` picks one `constexpr` count type from
them — INT32 before 25.10, INT64 after — and both `MERGE_M2` sites use it. No single type serves
both versions, and each cuDF version is its own build, so no runtime probe. The gate goes when
25.02 does.

## Aggregates

<a id="t199"></a>
### #199 — a global aggregate over no arrival drops its identity row

`gpu_backend/accumulate.rs:307` answers an empty lane with nothing. The CPU counterpart has a
`!self.grouped` clause and answers with the identity row — `count` is 0, not absent.

So a global aggregate whose lane received no rows disagrees between the engines: the CPU emits one
row and the device emits none. On the device that is a wrong answer, and nothing refuses it.
Shown on a device by `bug_a_global_merge_over_no_arrival_answers_nothing_on_the_device`
(`gpu_tests/aggregate_cases.rs`), which also shows the CPU's row is sum's identity, not count's:
a count merges by sum, so a merged count over nothing is NULL there where SQL says 0. Where the plan
declares that count non-nullable, the cpu refuses the NULL (`cpu_backend/accumulate.rs`,
`mark_done_and_fetch`'s `!self.grouped` clause): "Column 'count(\*)' is declared as non-nullable but
contains null values", filed as #180. The init over a zero-row batch keeps its row on both.

A second site, on both backends: the single-node shortcut (`translator/aggregate.rs:322-339`), a
keyless `GpuAggregate` over a single-batch input. When no batch arrives the node is never called
(`single_partition.rs:192,248-251`) and no row comes out — symmetric, so the cpu-vs-device
comparison cannot see it. A `GpuLimit` keeps its input's single-batch layout, so a limit over a
sort, merge or coalesce feeds it, and #214's dropped batch lands here. The rule for both sites is
one: a keyless aggregate answers one row whatever arrived.

**Corpus queries:** tpcds q96, q88 and q90 at `tp4-single`, `tp4-rowgroup` and `tp4-sized`: nine
cpu cells refused on the NULL count, their device cells behind them. An empty lane comes from
tp4-single cutting a one-row-group table into four, or from an Inner join with no build rows in a
lane. Simplest, per the 2026-09-11 fix report and unconfirmed here: `select count(*) from store
where s_store_name = 'ese';` (tpcds) at `tp4-single`. The device's dropped row and the shortcut
site have no corpus query.

**Fix proposed:** at the init, where both sites start. `GpuAggregate` moves from `Exec` to a new
category, `ExecWithDone`: per-batch calls as now, plus a done call through which a keyless lane
that received no batch emits its identity state row; a grouped one emits nothing. The row comes
from `decomposition()` (`plan/aggregates.rs`), which gains an empty-input value per state column —
`0` for `PlanAgg::Count`, NULL for the rest — so both backends build the same row. The shortcut
finalizes it (count 0, the rest NULL); a merge never meets no arrival, and its count sums to 0,
not NULL. Every aggregate sequence starts with a `GpuAggregate` (`aggregate_sequence`, its only
builder), so the init covers them all. The driver owes the done call to every lane, one that saw
no batch included — today such a lane gets no call. Sources of nothing below the init (#214's
limit, #205's sort, the joins) need no fix of their own for this; between init and merge a keyless
sequence only collapses lanes, which keeps the one-row batch. The cpu's `!self.grouped` clause, which
merges over nothing, goes, and the nine #180 cells turn on at the tp4 modes.

<a id="t55"></a>
### #55 — q66: two-phase decimal aggregate ignores the partial-phase divisor cast

Filed against the old executor. DataFusion casts `sum(decimal / int)`'s divisor to Decimal128 in
the Partial phase only. The device's Final aggregate evaluated the argument again, over the
partial state, where the cast column does not exist, and cuDF failed the cast.

Most likely stale. The wire has no Final aggregate any more: `aggregate_writer.rs` writes `Init`
as `Partial` and `Merge` as `Merge`. The division runs once, in `Partial`, where `arg_col`
(`cpp/src/operators/aggregate.cpp`) builds a computed argument over the original input. A merge
reads state columns by reference. q66's five cpu cells, which run the same translated expression,
are green. What is missing is a device run: nothing has put a summed quotient on a device.

**Corpus queries:** `tpcds/q66` — twelve `sum(<month>_sales / w_warehouse_sq_ft)` over a two-branch
union. Its five gpu cells are off (`corpus_cases.inc` gpu modes `none`). Closing #55 enables none
of them: the four multi-batch modes are held by #152, and tp1-single by #220, the cpu's join
batching. Registry row 67 tags `55 152 220`.

**Fix proposed:** no code change — a proof, then archive. Add a walk test beside
`each_lane_merges_its_own_state_before_the_cross_lane_merge_folds_them` in
`wire/gpu_tests/mod.rs`: `SUM_OF_QUOTIENTS`, a grouped `sum(l_extendedprice / l_linenumber)`
under an outer `sum`, through `assert_walk_matches_datafusion` at `TWO_LANES`. The oracle
compare on the digits is the proof: a merge that re-evaluated the quotient over state would
throw or answer different digits. Pin the `PARTIAL`, `MERGE` and `FINALIZE` counts from the
trail, as the neighbouring tests do, so the shape cannot quietly lose its merges. Green on
shad-gpu closes #55: drop `55` from registry row 67 and archive the ticket as stale. Red means
the defect is live, and the throwing call names its phase.

<a id="t65"></a>
### #65 — the device's grouping-set id is not DataFusion's value or width

A ROLLUP, CUBE or GROUPING SETS init on the device numbers the id's bits from the other end, and
holds it as `Int32` where DataFusion declares `UInt8` up to 8 keys, `UInt16` to 16, `UInt32` to
32 and `UInt64` beyond. `GROUPING()` over the full key list reads the wrong value.

`aggregate.cpp` sets bit `i` for masked key `i` and builds the column from a
`cudf::numeric_scalar<int32_t>`. DataFusion 45 folds `(acc << 1) | is_null`, first key highest
(`aggregates/mod.rs`), at the width `Aggregate::grouping_id_type` picks: a two-key rollup is 0, 1,
3 there and 0, 2, 3 here. The merge above only needs one id per set, so it is right; every other
reader is not. `GROUPING(k1, …, kn)` over the full key list plans on 45 as
`CAST(__grouping_id AS Int32)` and answers the device's bits, a silent wrong answer. The schema
validator refuses the width. Pinned by the two `bug_grouping_sets_…` cases in
`gpu_tests/aggregate_cases.rs` (whose `grouping_sets_as_exported` swaps the bits and the type),
`bug_grouping_sets_hold_an_int32_grouping_id_where_the_plan_declares_uint8`
(`gpu_tests/aggregate_schema_cases.rs`) and
`bug_a_rollup_partial_holds_an_int32_grouping_id_where_the_plan_says_uint8` (`wire/gpu_tests/mod.rs`).
`GROUPING()` over a subset or a reordering of the keys is refused on the device (#230). DataFusion
55's duplicate ordinal is #228.

**Corpus queries:** every corpus plan with a grouping-set id: `tpch/rollup-over-join` and tpcds
q5, q14, q18, q22, q77, q80. None reaches it on the device yet: each cell is off first on #152,
#189 or #220. The value shows only through `GROUPING()`, and q70 and q86, the corpus's users, are
window queries that never run. Simplest (from [`reports/corpus-fixes.md`, fix 14](../reports/corpus-fixes.md#fix14)):
`select n_regionkey, n_nationkey, grouping(n_regionkey, n_nationkey) as g, count(*) from nation
group by rollup (n_regionkey, n_nationkey);` (tpch): 31 rows with g in {0, 1, 3} on the cpu; the
device answers the five subtotal rows with g = 2.

**Fix proposed:** [fix 14 of `reports/corpus-fixes.md`](../reports/corpus-fixes.md#fix14), in C++ alone. A static
`grouping_id_column(gid, nkeys, rows)` in `aggregate.cpp` folds `gid = (gid << 1) | masked` into a
`uint64_t` and builds `numeric_scalar<uint8_t/uint16_t/uint32_t/uint64_t>` by `nkeys` ≤ 8/16/32,
throwing past 64 keys as DataFusion does. No wire change and no golden regenerates: payloads print
masks and names, never an id. The report rejects carrying the id or its type on the wire, a
frozen-surface append and a `recipe-payloads.txt` regen for a value the C++ can compute; that
changes at DataFusion 55, where the width depends on the ordinal (#228). Tests: the four `bug_`
pins flip and `grouping_sets_as_exported` goes; a rollup contract case on both engines; a walk
test running the query above; the exec model's fold (`scripts/exec_model/operators/aggregates.py`)
and its three pins, `{0, 2, 3}` to `{0, 1, 3}`. Cleaner after #189's fix, which stops hashing the
id; independent of it.

<a id="t62"></a>
### #62 — a DISTINCT beside an avg or a count is refused at planning

`avg(x), count(x), count(DISTINCT x)` does not plan: the translator refuses any aggregate that
carries DataFusion's DISTINCT flag, on both backends.

DataFusion's `SingleDistinctToGroupBy` removes the flag only when every other aggregate is `sum`,
`min` or `max`: its outer level re-applies the same function, and `avg` and `count` are not their
own merge. So q16, q94 and q95 plan, and q28 keeps the flag. `decompose`
(`planner/translator/aggregate.rs`) refuses it with `DISTINCT inside … (#62)`. Pinned by
`a_distinct_beside_a_companion_datafusion_cannot_rewrite_is_refused_naming_62`
(`planner/tests/join_refusals.rs`); the plan golden reads `refused: … (#62)`.

**Corpus queries:** `tpcds/q28`, six global `avg, count, count(DISTINCT ss_list_price)` blocks
cross-joined. Refused at every mode on both engines (registry: cpu and gpu `na`). Next, per
[`reports/corpus-fixes.md` fix 11](../reports/corpus-fixes.md#fix11) and unconfirmed: the cpu cells pass now that #163 has landed,
and the device cells meet #152, the cross join copying its build side.

**Fix proposed:** [fix 11 of `reports/corpus-fixes.md`](../reports/corpus-fixes.md#fix11), with `62-review.md`. The translator does
the rewrite DataFusion declines, as two aggregate sequences in `aggregate_sequence`. It applies
when every DISTINCT aggregate is a `count` of one shared argument `x`. The inner sequence groups
by `(keys, x)` and runs the other aggregates' inits, shuffled on `(keys, x)` where lanes split;
it emits state, no finalize. The outer sequence groups by `keys` over that state. Its init
applies each state column's merge rule from `decomposition()` (`sum` over a count or a sum,
`min`, `max`) plus `count(x)` for each DISTINCT, then merges and finalizes. The output is
DataFusion's final schema, so nothing above changes. The DataFusion partial's DISTINCT state is
ignored. Four traps, the first three still true in the code:
- Pair the outer stage's state columns with `rule.state` by position, not by tag.
  `declared_nullable` (`translator/aggregate.rs`) looks each up as `[tag]`, and this engine's own
  names (`avg(…)$sum`, `$count`) do not match, so q28's `avg` would be refused as drifted.
- Take each outer column's argument type from the inner stage's declared type at its position,
  through `state_type(merge_func, …)`. `arg_type` reads `expressions().first()` off the raw input,
  which in the outer stage is the wrong column or out of range.
- A `count` companion merged by `sum` answers NULL over an empty keyless input, where SQL says 0.
  Declare every from-state column nullable, and finalize a count companion as
  `CASE WHEN o IS NULL THEN 0 ELSE o END`; `plan/aggregates.rs`'s count finalize is the bare
  column today.
- Refuse grouping sets under a DISTINCT. Also refused: a Welford state (`stddev`, `var`), which has
  no plain merge, and a DISTINCT over two arguments (#144).

Tests: the refusal pin becomes a plan test of the two sequences. An end-to-end case, red before
the fix: `select count(ss_customer_sk), count(distinct ss_customer_sk) from store_sales where
ss_quantity < 0;` (tpcds) answers `0, 0`.

The wire's `AggregateFuncNode.distinct` goes in the same change, with the C++ guard that reads
it (`aggregate.cpp`, "DISTINCT aggregate … see #62"). Nothing sets it, before or after this fix:
the translator refuses DISTINCT and `PlanAgg` has no field for it; `aggregate_writer.rs` writes
`false`; the three `CreateAggregateFuncNode` calls in `cpp/tests/gpu/test_plan_executor.cpp`
pass `false`; every `peacock_executor_begin_plan` caller hands it `recipes.bytes()` from that
writer. Mark the field `(deprecated)` in `gpu_plan.fbs`, so `alias` and the fields after it keep
their slots and both generated sides lose the accessor. Delete the writer's `distinct: false`,
the guard, and the argument in the three gtests. FlatBuffers omits a `false` at its default, so
`recipe-payloads.txt` should not move; confirm with the payload test. The report prefers keeping
the guard with a corrected comment (`hacks-audit.md` §10); deleting it is chosen here, since
nothing can set the flag.

## Sort / Limit

<a id="t202"></a>
### #202 — a descending sort key puts its nulls on the wrong end on the device

On a descending key the device places nulls at the end the plan did not declare: `i32 DESC
NULLS LAST` comes back nulls first, and `DESC NULLS FIRST` comes back nulls last.

`sort.cpp` and the merge in `node_session.cpp` map `nulls_first` to `cudf::null_order::BEFORE`
and its absence to `AFTER`, and cuDF applies that before it flips a `DESCENDING` key. Ascending
keys are right, which is why every corpus ORDER BY has agreed: no cell's descending key carries
a null. DataFusion's default for `DESC` is nulls first, so a query sorting a nullable column
descending gets its null rows first on the cpu and last on the device. The mapping has to be
relative to the direction — `BEFORE` when `nulls_first == asc` — at both sites, since a merge
over sorted runs must order as the sort did, and the two sites have to move together: runs
sorted as the plan says under a merge that reads them the other way break cuDF's merge
precondition, and the device answers duplicated and dropped rows — a shape no plan reaches
today, since every run the merge sees was sorted by the same mapping. Pinned by `bug_a_descending_key_with_nulls_last_puts_them_first_on_the_device`
(`gpu_tests/exec_cases.rs`), the two `…_descending_key_nulls_first_puts_them_last…` merge pins
and `…_over_runs_each_carrying_a_null_duplicates_and_drops_rows…` (`gpu_tests/accumulate_cases.rs`).


<a id="t217"></a>
### #217 — a sort with `fetch 0` keeps every row on the device

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
### #204 — the device's sorted merge drops its fetch when it is handed one input

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
### #214 — a limit drops a zero-row batch on both backends

A `GpuLimit` handed a batch of zero rows emits nothing for it, on the cpu and on the device alike.

Both `LimitStream`s take their cut from the one `RowInterval::range_of` (`plan/interval.rs`),
which answers `None` when `start < stop` is false — and it is false for `n_rows == 0` whatever
the interval — so the batch is released as if it lay outside the interval. Nothing and a zero-row
batch are different arrivals downstream, as #205 says: a build side of `filter → limit → coalesce`
with zero survivors reaches `without_build` and #212's refusal for Right, Full and RightAnti,
where the same filter without the limit pads and answers. Symmetric, so the harness's cpu-vs-device
comparison is green by construction. Pinned by `bug_a_stream_of_one_zero_row_batch_is_dropped_on_both`
and its two neighbours (`gpu_tests/harness_cases.rs`), which want the zero-row batch under the
schema. Numbered past #213, which the branches above this one have taken.

**Corpus query:** none; the only full joins, tpcds q51 and q97, have their `LIMIT` at the top.
Simplest shape: `select n_name, r_name from (select * from region where r_regionkey < 0 limit 1)
r right join nation on n_regionkey = r_regionkey;` (tpch) — refused where the same query without
the `limit` answers. Unconfirmed that the filter hands the limit a zero-row batch.

**Fix proposed:** in the joins, not the limit — a join reads no batch on either side as an empty
side. Build: `without_build` pads Right, Full and RightAnti rather than refusing (#212). Probe:
the finish answers over zero keys (#173, the device's unfiltered LeftSemi/LeftMark), and the
one-call forms make their call over zero rows — a hash LeftAnti/LeftMark with a residual filter
(every build row, or every row marked false) and a nested-loop Left (the build rows padded); both
answer nothing today, on both backends, unticketed. The keyless aggregate's same gap, both
its sites, is #199's and deferred. Then this ticket drops with its three `bug_` pins.

<a id="t205"></a>
### #205 — the cpu's accumulating sort and merge answer nothing over zero-row batches

A `GpuAccumulateBatchesAndSort` or `GpuMergeSortedPartitions` whose only batches have zero rows
emits no batch on the cpu, where the device emits one of zero rows.

DataFusion's `SortExec` over zero rows yields no batch at all, and `SortedRuns::mark_done_and_fetch`
and `CpuPartitionAccumulator::accumulate_and_fetch` (`cpu_backend/accumulate.rs`) hand that empty
answer to `coalesce_or_nothing`, which reads it as the lane that received nothing. `CpuExec::exec` concatenates
the same empty answer under the declared schema and gets zero rows, and the cpu coalesce does too, so
the two cpu paths disagree with each other as well as with the device. Downstream, nothing and a
zero-row batch are different arrivals: a global merge over nothing is #199's site. Pinned by
`bug_one_zero_row_batch_sorts_to_nothing_on_the_cpu` and its two neighbours
(`gpu_tests/accumulate_cases.rs`). First corpus cell to reach it: `tpcds/q17` at `tp1-single`,
which answers zero rows — 12 bytes at the device's unload against the cpu's 0 (2026-09-17).

**Corpus queries:** `tpcds/q17` at `tp1-single`, the only enabled corpus query that answers zero
rows (`mini.result.txt`). Its device cell is off on this ticket (`corpus_cases.inc`); the device
never ran its other four modes. The 12 bytes are the device's one zero-row batch: three `Utf8`
columns, one 4-byte offset each. Simplest shape, unconfirmed on a build:
`select n_name from nation where n_nationkey < 0 order by n_name;` (tpch).

**Fix proposed:** in `cpu_backend/accumulate.rs`, skip the sort when every held batch has zero
rows. `SortedRuns::mark_done_and_fetch` and `CpuPartitionAccumulator::accumulate_and_fetch` at the
last `Done` then pass the held batches to `coalesce_or_nothing`. It concatenates them into one
zero-row batch under the schema, as the device answers. The fetch has no rows to cut. No arrival
at all stays nothing, as now. The three `bug_` pins become green `same` cases, and `tpcds/q17`'s
device cell turns on at `tp1-single`, its comment in `corpus_cases.inc` updated.

An empty answer must still have a schema. A query that answers zero rows answers them under its
declared columns, never as no batch at all: today `tpcds/q17`'s `mini.result.txt` section is a bare
`++`/`++`, with no column names or types, so nothing checks them, and DuckDB's answer
(`duckdb-result.txt`, #235) prints the header ours lacks. Beside the fix above, the unload answers a
query whose root received nothing with one zero-row batch under the sink's declared schema, on both
backends, so an answer's schema never depends on how its rows ran out. `q17`'s result section is
then regenerated with its header, and #235's empty-answer divergence goes.

## Scalars

<a id="t168"></a>
### #168 — interval type can not be represented in the fbs ScalarValue

The wire's `ScalarValue` (`flatbuffers/gpu_plan.fbs`) has no interval type, so a plan holding an
interval literal cannot cross to the device.

`testdata/tpch-queries/mixed-join.sql` joins on `l_shipdate <= o_orderdate + INTERVAL '90' DAY`.
The interval is added to a column, so constant folding cannot remove it. Every other corpus
interval folds into a date before the plan is written. `expr_writer.rs` refuses the literal with
`unsupported scalar value: IntervalMonthDayNano(...) (#168)`, and the whole plan fails. The plan
golden shows `not runnable: ... (#168) at #3`. `plan_goldens.rs` pins it both ways:
`NOT_RUNNABLE` names mixed-join, and the uncrossable set must be exactly it. The wire test
`a_payload_the_wire_cannot_carry_fails_the_plan_and_names_where` pins the refusal. The legacy
serializer refused the same literal, so the device has never run this query.

**Corpus queries:** `tpch/mixed-join`, its device cells off at all five modes
(`corpus_cases.inc`: "cannot cross to a device"). The cpu runs it at all five.

**Fix proposed:** carry a whole-day interval as a day duration. Append `DurationDays` to
`DataType`, after `Decimal128`, so no ordinal moves; its value rides `int_val`.
`serialize_scalar_value` (`wire/serialize.rs`) writes `IntervalMonthDayNano(0, d, 0)` and
`IntervalDayTime(d, 0)` as `DurationDays(d)`. An interval with months or a sub-day part stays
refused under this ticket: a month is not a fixed number of days. In `expr.cpp`, the literal arm
builds a `cudf::duration_scalar<duration_D>`, and the type map sends `DurationDays` to
`DURATION_DAYS`. The inner join's residual goes through `build_column`, and the type mismatch
already routes `date + duration` to `cudf::binary_operation`, which adds `TIMESTAMP_DAYS` and
`DURATION_DAYS` natively. `binop_output_type` echoes the left operand; it must pick the
timestamp side, so `90 days + date` answers a date. Unconfirmed on a build. Then `NOT_RUNNABLE`
and the uncrossable set go empty, the wire test takes a month interval, the golden line becomes
a plan, and mixed-join's device cells turn on.

<a id="t210"></a>
### #210 — a bare decimal literal on the AST path comes back as a Float64 column

cuDF's AST has no fixed-point literal, so `ast_scalar` (`expr.cpp`) rewrites a `Decimal128`
literal as a scaled double before the one scalar builder; under a `CAST(… AS Float64)` that is
the type the plan asked for, but a bare or unary-wrapped decimal literal is AST-able too, and
`SELECT 1.5 FROM t` then computes a `FLOAT64` column on the device where the plan declares
`Decimal128(2, 1)` — and since `decimal-precision-at-export` the export refuses it by name rather
than answering it, the AST path still computing a double. Same class as
[#191](../archive/archived-tickets.md#t191): a declared type produced as another. Pre-existing, carried
through `typed-nulls.md` by that spec's own instruction, and pinned by
`bug_a_bare_decimal_literal_is_a_float64_column_on_the_device` (`gpu_tests/exec_cases.rs`);
the walk `Literals.EveryWireTypeEitherMakesAnAstLiteralOrSaysWhyNot` names it on its
`Decimal128` row. The likely fix is `cudf_ast_can_evaluate` refusing a bare decimal literal as it already
refuses a decimal operand, so the column path builds a real `fixed_point_scalar`.

<a id="t57"></a>
### #57 — the device refuses a value-form CASE
`build_column_case` (`cpp/src/expr.cpp`) throws `value-form CASE not supported in column path`
for any `CASE x WHEN v THEN …`. The search form, `CASE WHEN x = v THEN …`, folds through
`copy_if_else` and works. `bug_a_value_case_is_refused_on_the_device`
(`src/tests/gpu_tests/exec_cases.rs`) pins the refusal, and `plan/mod.rs` lists it among the
known refusals.

Filed as a wrong answer: the value form came back all-0 or all-null, so it was reverted to a
throw. [`reports/corpus-fixes.md` (fix 7)](../reports/corpus-fixes.md#fix7) found that a misdiagnosis. The gtest built every Int64
literal as 0, because `CreateScalarValue`'s second parameter is `is_null`, and the lowering had
run q39 correctly at 8f471cc0. The guard stayed on that false measurement.

**Corpus queries:** `tpcds/q39` — `CASE mean WHEN 0 THEN NULL ELSE stdev/mean END` in a project
and `CASE mean WHEN 0 THEN 0 ELSE stdev/mean END > 1` in a filter, each twice per plan (the CTE is
read twice). All five gpu cells are off and registry row 40 tags `57` alone. At tp1-single this
is the refusal (`corpus_cases.inc:284`); the other four modes have not run on a device and may
meet [#152](joins.md#t152) next.

**Fix proposed:** [fix 7 of `reports/corpus-fixes.md`](../reports/corpus-fixes.md#fix7), about 20 lines in `build_column_case`.
Delete the throw and build the comparand once. Each WHEN becomes `binary_operation(comparand,
when, EQUAL, BOOL8)`, with a scalar fast path for a literal WHEN, feeding the search form's fold
unchanged. A NULL never matches, since `copy_if_else` reads a null condition as false, and the
first match wins. No wire or ABI change, no golden moves. Proof: a gtest in
`cpp/tests/gpu/test_plan_executor.cpp` whose comparand holds a NULL and a WHEN maps to NULL, now
that `make_int64_literal` and `make_null_literal` build real values; the `bug_` case turned into
a cpu-vs-device agreement; `tpcds/q39` enabled at tp1-single on shad-gpu. Drop the #57 clause
from `plan/mod.rs` and `57` from registry row 40 in the same change.

<a id="t56"></a>
### #56 — q2: CASE-over-string-equality inside a partial-phase sum

Filed against the old executor: the device built a cuDF AST for `sum(CASE WHEN <string equality>
…)`, and cuDF refused with binaryop "Unsupported operator", since its AST cannot compare strings.
[`reports/corpus-fixes.md`](../reports/corpus-fixes.md) traced the recorded error to the old Final phase, which evaluated the
arguments in every phase — the same root as #55.

Neither path is left. The aggregate builds no AST: only `filter.cpp` calls `cudf_ast_can_evaluate`. Its
`arg_col` (`cpp/src/operators/aggregate.cpp`) builds a computed argument with `build_column`,
which compares strings, and a merge reads state columns by reference.

**Corpus queries:** `tpcds/q2`, whose `sum(CASE WHEN d_day_name = 'Sunday' …)` repeats the shape
per weekday. Its five gpu cells are off (`corpus_cases.inc` gpu modes `none`), also held by #152 at
every mode; registry row 3 tags `56 152`. The harness test's comment counts the shape 48 times
in the corpus.

**Fix proposed:** likely fixed already. `a_grouped_sum_of_a_case_over_a_string_equality_agrees`
(`src/tests/gpu_tests/aggregate_dimension_cases.rs`) runs this shape through a device
`GpuAggregate` and matches the CPU, green since 2026-09-16. It covers the init alone, one operator,
not a plan's merges across lanes. The complete proof is `tpcds/q2` enabled at every gpu mode once
#152 lands; green there, archive #56 and drop `56` from registry row 3.

<a id="t60"></a>
### #60 — `round(x, p > 0)` on the device differs from DataFusion by one ulp

The device rounds a float to `p > 0` places with two roundings where DataFusion uses one, so about
one value in twenty at or above 1.0 comes back one ulp off: `2.7800000000000002` for `2.78`.

`expr.cpp` calls `cudf::round(col, places, HALF_UP)` for any `places`. cuDF's kernel takes `modf`
and computes `ip + round(fp·10^p)/10^p`; DataFusion computes `(x·10^p).round()/10^p`. `fl(0.78)`
puts `2 + fl(0.78)` on a midpoint of the result grid, and ties-to-even takes the upper double.
Re-emulated over q78's committed answer, exactly five `ratio` cells split under `golden_exact`;
q2 takes the same kernel seven times per row. This ticket used to describe an anti join and memory
pressure on q78 ([`reports/corpus-fixes.md` fix 6](../reports/corpus-fixes.md#fix6) found neither), and called `round` proven by q54.

**Corpus queries:** tpcds q78 and q2, both off first on #152. After #152, q78 at `gpu_tp1_single`
must differ from the golden in exactly the five `ratio` cells before this fix and in nothing
after. Simplest: `select n_nationkey, round((cast(n_nationkey as double) * 25.0) / 9.0, 2) as r
from nation;` (tpch): two rows differ on the device today.

**Fix proposed:** [fix 6 of `reports/corpus-fixes.md`](../reports/corpus-fixes.md#fix6), in `expr.cpp`. `places == 0` keeps
`cudf::round(·, 0, HALF_UP)`. Otherwise multiply by a FLOAT64 scalar `10^|p|` (exact for p ≤ 22;
`1/10^|p|` for a negative p), `cudf::round(·, 0, HALF_UP)`, divide: bit-identical to DataFusion.
The comment beside it names which kernel agrees. The cpu relays `round` to DataFusion,
untouched; no golden moves. Test: a walk test `ROUND_PLACES` running the query above at
`ONE_LANE`, red on two rows today; `scripts/exec_model/operators/expressions.py`'s docstring.
Registry: q78's row drops `60`. `round` over Float32 is #221.

## Source

<a id="t186"></a>
### #186 — a limit pushed into the scan: the cpu ignores it, the device refuses it

`SELECT * FROM lineitem LIMIT 10` answers 6,001,215 rows on the cpu at `tp1-single` and
`tp1-rowgroup`, a wrong answer. On the device every mode fails the first read: `CUDF failure …
row_groups can't be set along with skip_rows and num_rows`.

One plan shape, two halves of one design left unwritten. `architecture.md` says a limited scan
plans one lane and one batch. `source()` (`planner/translator/nodes.rs`) gives it one lane but hands
the mode's batching to `partition`, so the rowgroup modes map one batch per row group (49 for
lineitem), and nothing validates it. On the cpu, `CpuSource` never reads `node.limit`, and its
reader has no `with_limit`. At tp1 the limit sits in the scan alone; at tp4 the unload's interval
hides it, though the loader still reads everything. On the device, `scan.cpp` sets
`set_num_rows(limit)` and then the call's row groups, which cuDF refuses together, even for one
row group. Pinned by the four `bug_` cases in `gpu_tests/source_cases.rs`: sixty-four rows for ten
on the cpu, a refused read on the device. #188 filed the device half and is merged here.

**Corpus queries:** `tpch/scan-limit`, cpu off at the two tp1 modes and device off at all five.
`tpch/nested-limits`, device off at all five. Simplest: `select n_nationkey from nation limit 3;`
(tpch), 25 rows on the cpu and a refused read on the device, at every mode.

**Fix proposed:** F13 of `reports/bugfix-proposals/10-fixes-v1.md`: a limited scan is one lane
and one batch over the row groups that cover the limit, and each reader bounds that one call.
- Planner, `source()`: call `batching_for_source(t)` unconditionally, since it advances the
  counter the estimator's second pass pairs by. With a limit, map one lane with `Batching::Off`
  over the shortest prefix of the survivors whose rows reach the limit (all of them if none do);
  without one, as today. `lanes_for` loses its limit arm. `GpuLoadParquet` keeps the full
  survivor list, which the validator checks the mapping against.
- Validator, `plan/source.rs`: a limited scan whose mapping is not one lane and one batch is
  `PlanError::Invalid`, naming `source()`.
- Cpu, `cpu_backend/source.rs`: `CpuSource` takes `node.limit`, and `read_next` adds
  `with_limit`, which parquet 54 composes with `with_row_groups`.
- Device, `scan.cpp`: drop `set_num_rows`; after the read, slice the table to `limit` rows with
  `cudf::slice` when it holds more. Stats come from the returned view, so the batch prices
  exactly. `0` still means no limit: `node_writer.rs` writes `limit.unwrap_or(0)`,
  so `Some(0)` and `None` are one value on the wire. Harmless, since DataFusion's `EliminateLimit`
  turns a literal `LIMIT 0` into an empty relation before any scan exists.
- No recipe, driver, `GpuLimit` or unload change.

Tests: a cpu source case, a validator case, a translator case over `customer` (`limit 3` maps
`[[[0]]]`), a device executor case and a gtest capping one call. The end-to-end
`a_limit_slices_at_most_two_batches_and_stops_the_scan` moves its claim onto a filtered variant,
since both nested-limits scans become one batch. Ten tpch plan sections change (scan-limit and
nested-limits at five modes) and ten execution sections; `mini.result.txt` is unchanged. The
source line keeps `limit=10`, printed whenever the node carries one (`plan_text/node_text.rs`).
Its mapping becomes `partition_groups=[[[0]]]`, one row group reaching ten rows, where today it
is `[[[0,1,…,48]]]` at tp1-single and the tp4 modes and `[[[0],[1],…,[48]]]` at tp1-rowgroup.
`batches=multiple` becomes `batches=single`, and the `--- memory ---` estimate falls to one row
group, from 371 MB at tp1-single. The tp4 unload keeps its own `skip=0, fetch=10`. scan-limit's
cpu tp1 cells turn on. Its device cells then meet the decimal export (#187); nested-limits' meet
the zero-column scan and the cross join's batching (#220).

## Repartitioning

<a id="t206"></a>
### #206 — a float or boolean partition key is refused on the device

A `GpuEmitPartitions` hashing a `Float64` or a `Boolean` column is refused by the device's kernel,
where comet's hasher answers it on the cpu.

`spark_hash_partition.cu`'s type switch takes STRING, INT8-64 and DATE32 and fails on everything
else: `unsupported key column cuDF type_id=10` for a double, `11` for a boolean, `27` for a decimal
(that one is #95). Spark hashes a double as its long bits and a boolean as an int, and comet's
`create_murmur3_hashes` does both, so the cpu lane assignment is defined and the device's is a
refusal. Any `GROUP BY` or join key of either type at more than one lane reaches it. Pinned by
`bug_a_float_key_is_refused_on_the_device` and `bug_a_boolean_key_is_refused_on_the_device`
(`gpu_tests/emit_cases.rs`).

**Corpus query:** none — no `tp4` plan golden hashes a Float64 or Boolean key, and the sf1 data
has no float column. Simplest, at any `tp4` mode: `select cast(l_quantity as double) q, count(*)
from lineitem group by q;` and `select l_quantity > 25 b, count(*) from lineitem group by b;`
(tpch).

<a id="t240"></a>
### #240 — a timestamp partition key is refused on the device

A `GpuEmitPartitions` hashing a `Timestamp` column, in any unit, is refused by the device's
kernel, where comet's hasher answers it on the cpu.

`spark_hash_partition.cu`'s type switch has no `TIMESTAMP_{SECONDS,MILLISECONDS,MICROSECONDS,
NANOSECONDS}` arm, so it fails at the `default` with `unsupported key column cuDF type_id=N`.
Spark and comet hash a timestamp as its `i64` value, so the cpu lane is defined. The kernel's
own comment ("Timestamp-as-i64 → 8B") and [#95](#t95)'s text both say timestamps are covered;
the switch says otherwise. Any `GROUP BY` or join key of a timestamp type at more than one lane
reaches it. Not gated by `murmur_conformance.rs`; no pin yet.

**Corpus query:** none — tpch and tpcds use `Date32`. Simplest, at any `tp4` mode:
`select cast(o_orderdate as timestamp) t, count(*) from orders group by t;` (tpch).

<a id="t189"></a>
### #189 — the shuffle cannot hash a rollup's grouping-set id

A ROLLUP, CUBE or GROUPING SETS aggregate that shuffles is refused on the cpu: `GpuEmitPartitions
lane 0: … comet murmur3: … Unsupported data type in hasher: UInt8`. The tp1 modes do not shuffle
and pass.

`shuffle_below` (`planner/translator/aggregate.rs`) copies DataFusion's `FinalPartitioned` hash
keys, and those include `__grouping_id`, a `UInt8`. comet's murmur3 has no unsigned arm, so
`CpuEmitter::emit` refuses before a device sees the plan. Hashing the id is also wrong in itself:
the device's id differs from the cpu's in type and bits (#65), so the two engines would put
a subtotal row in different lanes. A refusal, not a wrong answer.

**Corpus queries:** `tpch/rollup-over-join` and tpcds q5, q18, q22 and q80 at `tp4-single`,
`tp4-rowgroup` and `tp4-sized`: 15 cpu cells, their device cells behind them. tpcds q77 may meet it
too once #212 stops refusing it first. Simplest: `select l_returnflag, sum(l_quantity) from
lineitem group by rollup (l_returnflag);` (tpch) at `tp4-single`.

**Fix proposed:** hash the user keys and not the id. In `aggregate_sequence`, before `tree = match
shuffle`, drop the id's ordinal from `Shuffle::ByHash`'s keys when the aggregate has grouping sets:
`keys.retain(|k| *k as usize != group.expr().len())`, guarded on `!group.is_single()`. Every row of
a user-key group then lands in one lane whatever its set, so each (keys, id) group stays whole.
The rule the plan validates, hash keys a subset of the group columns, already allows it. No hasher
arm and no C++. The tp4 plan goldens of the five queries change their emit's `hash=` and the
merge's `hashed_on`, and the tpcds q5 payload's `hash_exprs` shrinks. A planner test pins
`hash_keys == [0, 1]` for a two-key rollup at tp4. The 15 cpu cells turn on.

<a id="t145"></a>
### #145 — Refcounted handles: stop copying every partition out of a scatter
`spark_hash_partition` returns one table whose N partitions are already contiguous, and
`node_session.cpp` (~L265-272) deep-copies each range out, because a handle owns its memory.

So every shuffle copies its whole input a second time and peaks at twice the data — the concrete
form of [#91](../archive/archived-tickets.md#t91)'s repartition spike, once per aggregate and once per join side. The change:
`TableResult` (`plan_executor.h:13`) becomes a `shared_ptr<cudf::table> owner` plus a
`cudf::table_view view`, and the scatter registers N handles sharing one owner. Mechanical but
wide — 35 sites across 11 files touch `.table` / `->table`. **No ABI change**: a handle stays a
`u64`. The cost to weigh: a slice pins its whole parent, so a skewed hash leaves one hot lane
holding the pre-scatter table — the peak halves and the tail lengthens. Also unlocks
[#140](optimizer.md#t140). Tests: the GPU tiers stay byte-identical, plus a gtest releasing N−1 handles and
reading the survivor. A streamed join waits on it too: a handle is erased by its reader
(`node_session.cpp:254`), so `Input::BuildSideCopy` has no build side after the first probe batch,
and T16 refuses a second until this lands ([#152](joins.md#t152)).


<a id="t95"></a>
### #95 — a decimal partition key is refused on the device
murmur3 covers int/date/timestamp/composite/null; decimal deferred (float indefinitely).
Needed by the first shuffle on a decimal key (tpch q18 `o_totalprice`, q10 `c_acctbal`,
tpcds `i_current_price`). Dispatch by *logical* precision (≤18 → low 8 LE bytes of int128;
>18 → raw 16B LE) and thread precision through the partition FFI. Until then
`spark_hash_partition.cu`'s type switch fails with `unsupported key column cuDF type_id=27`,
which is what tpch q15 hits on `total_revenue`; #184 was filed for it and is archived as this one's duplicate. The cpu's comet
hasher takes the decimal, so the shape is a refusal on one side. Pinned by
`bug_a_decimal_key_is_refused_on_the_device` (`gpu_tests/emit_cases.rs`).

**Corpus queries:** eight hash a decimal key at every `tp4` mode — tpch q2 (`ps_supplycost`),
q10 (`c_acctbal`), q15 (`total_revenue`, precision 38), q18 (`o_totalprice`); tpcds q24, q37,
q82 (`i_current_price`), q75 (`sales_amt`, precision 31). Their `tp4` device cells are off, on
blockers that refuse first (#152); q15 and q75 need the >18 path.

<a id="t197"></a>
### #197 — the repartition arm still concatenates a child it can only be handed one of
`node_session.cpp`'s Hash-repartition arm
[concatenates](../../cpp/src/node_session.cpp#L538) `child[0]`'s handles before scattering, and
the planner puts a `GpuCoalesceAllBatches` above the merge feeding an emit, so it gets one.

The comment there said to retire the branch when the legacy modes retired. They have, so the
condition is met and nothing left in the tree can hand this arm two handles — the concat is a
copy of a single table on every call. Removing it needs a device run to prove, which is why it
is a ticket rather than part of the rename that found it.

## Performance

Tickets pulled ahead from the performance path to fix earlier

<a id="t154"></a>
### #154 — every operator exit path deep-copies its output into a fresh table
`std::make_unique<cudf::column>(view)` deep-copies the device buffer, and 21 sites under
`cpp/src/` do it — 10 in `join.cpp`, 7 in `aggregate.cpp` — mostly to a table the same
function just produced.

`execute_hash_join` is worst per exit: `cudf::gather` returns an owning table, the code copies
each column into `all_cols` (~L337, ~L342), then copies the kept ones again if the node projects
(~L376). `release()` moves instead; `scan.cpp` L103 and `join.cpp` L254 are the pattern
(`union.cpp`'s site went with its `output_schema` block in decimal-precision-at-export), and it
is C++-internal — no header, fbs, Rust or golden moves. Five kinds: whole table
freshly produced (`join.cpp` 202, 337, 342, 512, 515), mechanical; ordinal subset (`join.cpp`
211, 270, 376, 525, `filter.cpp` 41), needing an assert the ordinals are distinct; a column of
an **input** table kept in the output (`join.cpp` 259, `project.cpp` 44, `window.cpp` 46),
changing who destroys what under `NodeInputs`; a temporary that only ever needed a view
(`expr.cpp` 834, below); and `aggregate.cpp` 413, 642, 644, 678, 680, 759, 771, unresolved without
reading. Traps: a view taken before the release dangles (`ftv` ~L372), and a repeated projection
ordinal moves one column twice leaving a hole — a wrong answer, not a throw, which is why it
needs the assert and not the observation. Land before [#155](joins.md#t155).

The `expr.cpp` site is the cheapest to fix and the most expensive to leave. `build_column`'s
`ColumnRef` arm copies the whole column and the caller takes `->view()` of the copy one line
later; every consumer (`cudf::binary_operation`, `unary_operation`, the function arms) takes
a `column_view`, and the input table outlives the call. Returning `table.column(idx)` — or
resolving `ColumnRef` leaves in `build_column_binary` before recursing — needs no ownership
change. It fires once per `ColumnRef` leaf per batch on every predicate `cudf_ast_can_evaluate` rejects
(a decimal operand, a string literal, LIKE, CASE): q6's filter copies five lineitem columns per
batch (`l_shipdate` ×2, `l_discount` ×2, `l_quantity`), and q19's copies string columns, offsets
and chars. The sf40 HBM reading puts it at ~46 of the 107 GB q19's lineitem filter moves, and
17× the useful traffic at its part filter. The `And` chain's intermediate bool columns are a
separate cost — one kernel per node, which only fusion (JIT, or stitching back into the AST)
removes — and not this ticket's.

## Testing

<a id="t227"></a>
### #227 Check schema nullability in tests

Column nullability is maintained tin node's output_schema, but not tested anywhere. Start testing
it in the CPU engine, by adding this logic to declared_as() - if not null constraint is set in the
schema, check that every record batch produced does not have any nulls.

<a id="t164"></a>
### #164 — a column ordinal reaches cuDF unchecked, and a bad one degrades rather than throws

The C++ half of [#135](../archive/archived-tickets.md#t135), which the planner
closed on the Rust side by checking a reference's name against the field at its position.

`TableResult` is a `cudf::table` plus a name vector with no invariant that the two are the same
length, and the six sites indexing names use `operator[]`, so a short vector is undefined
behaviour rather than an exception — `filter.cpp` ~L42 reads `fv.column(idx)` and
`input.column_names[idx]` in one iteration and only the first is checked. Assert
`num_columns() == column_names.size()` where `TableResult` is built. Separately `expr.cpp` ~L349
returns `type_id::EMPTY` for an out-of-range `ColumnRef` instead of throwing, turning a bad
ordinal into a confusing type error further along. The third closure #135 named is unstarted and
belongs here too: a per-node type check in the GPU tiers, the only thing that would surface a
wrong-order subtree before the root. 2026-09-17: chain B's `device-schema-harness` and
`driver-output-hook` are that check for the operator and corpus tiers — every device batch held
to its node's names and `{type_id, scale}`; the two C++ items above stand.

<a id="t201"></a>
### #201 — the murmur gate proves a copy of the lane rule, not the rule
`executor/cpu_backend/gpu_tests/murmur_conformance.rs` re-derives the lane rule (seed-42 pre-fill,
comet murmur3, `pmod`) in its own `cpu_partition_ids`, so only that copy is held against the device.

The production copy is `rows_per_lane` in `executor/cpu_backend/spark_partitioning.rs`, the one
the CPU backend's repartition actually runs. A drift there — a seed, a `%` for `pmod`, a key
cast — leaves the gate green while every CPU lane assignment moves off the device's and the
goldens'. The fix is the gate calling `rows_per_lane` over the same columns and comparing lane
by lane, and the local helper going; not done in the visibility task that found it, since a
test whose subject changes is not a demotion.

<a id="t174"></a>
### #174 — two clamps for one rule, and nothing compares them
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
### #233 — the plan validator does not check that a pass-through node keeps its input's column count

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

<a id="t234"></a>
### #234 — a mid-plan limit is counted twice, by the driver and by its executor, and nothing compares them

For a mid-plan `GpuLimit`, the driver and the limit's executor each count the rows of the same
stream against the same interval, for two decisions. Both counts are live, and both agree today.

The driver adds each consumed batch's rows to `rows_seen` (`executor/driver/partitioned.rs`), and
`settle_limit` reads it to mark the limit satisfied and stop pulling from below. `LimitStream`
(`cpu_backend/accumulate.rs`, `gpu_backend/accumulate.rs`) keeps its own `seen` and applies
`interval.range_of(seen, rows)` to decide whether to keep, slice or drop each batch. For an
unload the driver makes that choice itself and hands the executor a `RowRange`; for a mid-plan
limit the choice is made twice, in two places. If the counts ever drift — a batch counted on one
side and not the other — the driver stops the scan before the limit has kept its rows, or keeps
pulling after it is done: a LIMIT returning short, or reading more than it needs, with no test
that would notice. Found by `reports/hacks-audit.md` §8.

**Corpus queries:** `tpch/nested-limits`, the one mid-plan limit, where the counts agree.

**Fix proposed:** one count. The driver already computes `range_of(rows_seen, arriving)` for an
unload and passes the `RowRange` to the executor's call; do the same for a mid-plan limit, and
make `LimitStream` stateless on both backends: release the batch where the range is `None`,
forward it where the range covers it, slice it otherwise. `seen` goes. The driver's check that a
range never needs the ABI's clamp then covers the limit too. Tests: the driver's limit tests assert
the range each call was handed, and the backends' `LimitStream` tests take a range rather than
a running count.

<a id="t235"></a>
### #235 — no independent oracle checks the result goldens

Every answer the corpus checks is checked against DataFusion or against our own goldens, which
our cpu engine wrote. A defect DataFusion shares with us — a limit it drops (#166), `NOT IN` over
NULLs (#80) — passes every tier. DuckDB answers exist only for the hand-written bare-cuDF
gtests at sf40 (`testdata/gen_duckdb_goldens.sh`, `cpp/tests/gpu/test_tpch.cpp`), never for the
engine's corpus.

**Corpus queries:** every section of `testdata/goldens/{tpch,tpcds}.sf1/mini.result.txt`: 120
queries, 8 of them skipped (4 over the 262144-byte cap, 4 not enabled).

**Fix proposed:** a DuckDB golden beside each result golden, and a Rust test comparing them.
The generator and both `duckdb-result.txt` files landed 2026-09-28; the comparison test, the
`duckdb_oracle` argument and the enum consts are what remain.
- A Python generator (`testdata/duckdb_result.py`, beside `duckdb_cost.py`) runs each query of
  `testdata/{tpch,tpcds}-queries/` over the same sf1 parquet with the DuckDB 1.5.4 CLI CI already
  pins for `generate_testdata.sh`, binding the parquet as views the way `gen_duckdb_goldens.sh`
  does, and writes `duckdb-result.txt` beside `mini.result.txt`: the
  same `== <query>` sections and table rendering, rows sorted, the same cap and skip markers. The
  session sets `default_null_order='nulls_last_on_asc_first_on_desc'` and
  `integer_division=true`, DataFusion's rules, so those two are not divergences.
- A Rust test reads both files and compares each query both have: row count, then rows as
  multisets, by column position, numbers within a relative tolerance. A declared list of
  (query, reason, ticket) holds the expected divergences, asserted both ways as `NOT_RUNNABLE`
  is: an undeclared divergence fails, and so does a declared one that stopped diverging.
- The comparison mode is a ninth `corpus_query!` argument, `duckdb_oracle`, beside `cpu_oracle`
  and `gpu_oracle`, so a query's whole coverage still reads off its line: `duckdb_exact`,
  `duckdb_approx` (the tolerance, for the float and decimal-typing divergences below),
  `duckdb_divergent_<ticket>` (the declared list, per line), `duckdb_none` (only one side answers,
  or the section is over the cap). Every existing line gains it once the generator's first run
  says which each query needs.
- The device run writes its own answers, `gpu-result.txt` beside `mini.result.txt`: the same
  sections and rendering, from the device's unload, under a regeneration variable on shad-gpu and
  pulled home as the benchmark tree is. A record, never an authority: the device still asserts
  against the cpu's `mini.result.txt`, and `test_gpu_corpus`'s check that a device run leaves the
  cpu's three goldens byte for byte stays. What it buys is a three-way comparison that can be read
  without a device — ours on the cpu, ours on the device, DuckDB — and the same comparator applied
  to `gpu-result.txt` against `duckdb-result.txt`, so the device meets an oracle other than our cpu.
- Negative tests of the corpus helpers: a wrong row in the result golden makes the helper fail
  with a result divergence, for the cpu tier's `assert_result_section` (`test_support/corpus.rs`),
  the device tier's `assert_result` under `golden_exact` and `golden_approx*`
  (`test_support/corpus_gpu.rs`), and the new DuckDB comparison. Today only the section comparator
  is tested, on strings (`tests/test_golden_format.rs`); the helpers read their golden from the
  fixed testdata path, so each takes the section as an argument, or its path, for a test to hand
  it a doctored one.
- Each oracle enum gains an `ALL` const listing its variants — `CpuOracle`
  (`test_support/corpus.rs`), `GpuResultMode` (`test_support/corpus_gpu.rs`) and the new DuckDB
  one — and a test asserts every variant is named by some `corpus_query!` line, so an unused kind
  is deleted rather than kept. Today `GpuResultMode::Skip` and `GoldenApprox` are unused: 113
  lines say `golden_exact`, 2 `golden_approx_std`, 5 `live_cpu`.

Measured by the first run (2026-09-28), over every section both files hold: no row count, string
or NULL differs. tpch 39 queries: 22 identical, 6 differ in column names only, 5 in float or decimal
digits, 6 not compared. tpcds 99: 53 identical, 12 names only, 3 formatting only, 10 digits, 1
empty answer, 2 not compared, 18 answered by DuckDB alone. `round(x, 2)` (q2) agrees on the cpu;
no tie under a LIMIT and no NULL-order difference appeared; turning DuckDB's integer division off
changes only the typing of two decimal divisions (q2, q61), no value.

Expected divergences, to declare or to normalize in the comparator:
- **Column names** (18 queries). DataFusion names an unaliased expression by its qualified text
  (`sum(lineitem.l_quantity)`), DuckDB by its own (`sum(l_quantity)`). Compared by position.
- **Decimal `avg` and division truncate at a fixed scale.** We follow DataFusion's rule, a decimal
  cut (not rounded) at its declared scale; DuckDB answers a double. On its own it is formatting:
  tpch q1's `avg_qty` is `25.522005` against `25.522005853…`. Inside an expression the truncated
  intermediates compound: tpcds q58's `ss_dev` is right to about four places, and q66 answers
  `9.282779` where truncating the true `9.2827805…` gives `9.282780` — up to 1e-6 relative. A
  candidate ticket of its own: whether to round, or declare it (tpch q1, q8,
  shuffle-additive-avg; tpcds q7, q9, q13, q18, q26, q58, q59, q61, q66, q75, q85, q90).
- **Float last digits.** Floating sums and Welford `stddev`/`var` reassociate: tpch q14 and
  shuffle-stddev, tpcds q39, below 1e-13.
- **An empty answer.** `tpcds/q17` renders with no header on our side, the cpu emitting no batch
  (#205), so its column names and types go unchecked; DuckDB prints them. Equal as zero rows; a
  candidate ticket to render the declared schema.
- **Queries only DuckDB answers** (18, tpcds): the window queries our planner refuses (#143),
  q27 and q72 (#23), q28 (#62). Not compared.
- **Queries neither side checks** (10). Over the 262144-byte cap on both: tpch q16, anti-join,
  filter-project, semi-join. Not enabled on ours: tpch q11 and q22, tpcds q24 and q54 (#190);
  DuckDB answers them. **The spec for this ticket decides what to do about the over-cap
  queries**: raise the cap, compare a digest of the normalized rows on both sides, or leave them
  unchecked and say so.
- **A real divergence** is a ticket, and its line in the declared list names it.

<a id="t262"></a>
### #262 — the DISTINCT lowering's device cells have never run

The DISTINCT lowering (distinct-companions, chain K, closes [#62](#t62)) is built and proven on
the cpu only: chain K runs without a GPU so as not to contend with chain J for one. Its plans
reach the wire — a two-stage aggregate whose outer init runs merge aggregators over state, a
`__distinct_arg` key, a narrowed grouping id — and no device has run one. A device answer could
differ from the cpu's there, and nothing would say so, because the cells are off.

**Corpus queries:** `tpch/distinct-functions`, gpu cells off at all five modes on this ticket
alone. tpcds q28 (off on #152) and `tpch/rollup-distinct` (off on #65, #189) carry it too once
those close.

**Fix proposed:** on a GPU host, once chain J leaves one free: run distinct-functions' five
device cells against the cpu golden; each one passing is enabled, each one failing gets the
ticket it fails on. Then this ticket drops from the registry row and is archived. The same run
takes q28's and rollup-distinct's cells when their own tickets have closed.
