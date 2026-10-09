
# Corpus Rollout tickets

Tickets required for corpus rollout (CPU+GPU, all modes), TPC-H numbered and named queries, and 84/99 TPC-DS queries (window functions excluded). Minimum SQL functionality needs to be built for this milestone.


## Contents

- [Welford aggregation](#welford-aggregation)
  - [#225 — the device names every Welford state column by the same alias](#t225)
  - [#216 — the device's global aggregate has no Welford arm](#t216)
  - [#94 — MERGE_M2 count-child type is cuDF-version-specific](#t94)
  - [#280 — a `stddev` or `var` under a grouping set answers one column where the plan declares three](#t280)
- [Aggregates](#aggregates)
  - [#199 — a global aggregate over no arrival drops its identity row](#t199)
  - [#55 — q66: two-phase decimal aggregate ignores the partial-phase divisor cast](#t55)
  - [#65 — the device's grouping-set id is not DataFusion's value or width](#t65)
  - [#62 — a DISTINCT beside an avg or a count is refused at planning](#t62)
  - [#264 — the device refuses a group key that is not a bare column](#t264)
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
  - [#251 — a decimal quotient truncates at its own scale and later arithmetic carries the error up](#t251)
- [Source](#source)
  - [#186 — a limit pushed into the scan: the cpu ignores it, the device refuses it](#t186)
  - [#282 — a scan with no surviving row groups is refused at planning](#t282)
- [Performance](#performance)
  - [#154 — every operator exit path deep-copies its output into a fresh table](#t154)
- [Testing](#testing)
  - [#227 Check schema nullability in tests](#t227)
  - [#164 — a column ordinal reaches cuDF unchecked, and a bad one degrades rather than throws](#t164)
  - [#174 — two clamps for one rule, and nothing compares them](#t174)
  - [#233 — the plan validator does not check that a pass-through node keeps its input's column count](#t233)
  - [#234 — a mid-plan limit is counted twice, by the driver and by its executor, and nothing compares them](#t234)
  - [#262 — the DISTINCT lowering's device cells have never run](#t262)
  - [#281 — limits' and empty-sorts' device cells have never run](#t281)
  - [#253 — the DuckDB oracle cannot record a divergence it finds on two of its paths](#t253)
  - [#254 — `data_fusion_subset` is the one cpu oracle no test can show failing](#t254)

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
`schema_validation_disabled`; the cells stay enabled and their values match. 2026-10-09: five of
them now, `stale-cells` having turned on the four device modes that a closed #183 still held off;
none failed on schema, so the row does not keep `225`.

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
branch) casts `valid_count` to INT32 for the 25.02 GPU runtime; 25.06 and later (cuDF PR #18546) accept only
INT64 or FLOAT64 (`group_merge_m2.cu`). The Final arm casts the same way but never runs. The
26.02 CI leg is build-only so it stays green — this bites at the next GPU-remote cuDF bump.
Switch or version-gate the type then; the comment marks the site.

The plan never sees this type: it declares every `$count` state `Int64`, and so do the columns
between calls. The INT32 lives inside the Merge call alone — cast in before `MERGE_M2`, widened
back to INT64 before it returns. On 25.06 and later the count stays INT64 throughout.

**Fix proposed:** gate at compile time. `cpp/CMakeLists.txt` passes `cudf_VERSION`'s major and
minor as `PEACOCK_CUDF_VERSION_*` defines; `aggregate.cpp` picks one `constexpr` count type from
them — INT32 before 25.06, INT64 from 25.06 — and both `MERGE_M2` sites use it. No single type serves
both versions, and each cuDF version is its own build, so no runtime probe. The gate goes when
25.02 does.

<a id="t280"></a>
### #280 — a `stddev` or `var` under a grouping set answers one column where the plan declares three

A `stddev` or `var` under `ROLLUP`, `CUBE` or `GROUPING SETS` is refused on the device; the cpu
answers it.

The wire folds the init's three Welford calls (`Count`, `Mean`, `M2`) into one
`AggregateFuncNode` and sets `mergeable_agg_state`. `execute_aggregate`'s grouped path honours
the flag and emits `[count, mean, m2]` (`aggregate.cpp:615-632`). Its grouping-set path
(`:363-423`) predates it: one request per function node through `make_agg`, which for a stddev
name is cuDF's `STD` and for a var name `VARIANCE`, one finished column per group. The plan
declares three state columns, so the export against the declared schema refuses the node, or the
finalize project above finds `ColumnRef index 2 out of range`; a function after it in the node
reads a shifted column. Found by the 2026-10-08 column-indexing audit; no test reaches it.

**Corpus query:** none: no tpch or tpcds query has the shape. Simplest: `select l_returnflag,
l_linestatus, stddev_samp(l_quantity), var_pop(l_quantity) from lineitem group by rollup
(l_returnflag, l_linestatus);` (tpch).

**Fix proposed:** one request builder for the grouped and grouping-set paths, so the
grouping-set path builds the Welford triple the grouped one does; the query above added as
`tpch/rollup-stddev`. Both in aggregate-arms, chain L.

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

**Corpus queries:** tpcds q96, q88 and q90, and `pbench/scalar-subquery-cross`, at `tp4-single`,
`tp4-rowgroup` and `tp4-sized`: twelve cpu cells refused on the NULL count, their device cells
behind them. An empty lane comes from
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
merges over nothing, goes, and the twelve #180 cells turn on at the tp4 modes.

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
32 and `UInt64` beyond. `GROUPING()` over the full key list reads the wrong value, silently: it
plans on 45 as `CAST(__grouping_id AS Int32)` and answers the device's bits.

`aggregate.cpp` sets bit `i` for masked key `i` and builds the column from a
`cudf::numeric_scalar<int32_t>`. DataFusion 45 folds `(acc << 1) | is_null`, first key highest
(`aggregates/mod.rs`), at the width `Aggregate::grouping_id_type` picks: a two-key rollup is 0, 1,
3 there and 0, 2, 3 here. The merge above only needs one id per set, so it is right; every other
reader is not. The schema validator refuses the width. Pinned by four `bug_` cases — two in
`gpu_tests/aggregate_cases.rs`, whose `grouping_sets_as_exported` swaps both the bits and the
type, one in `gpu_tests/aggregate_schema_cases.rs` and one in `wire/gpu_tests/mod.rs`.
`GROUPING()` over a subset or a reordering of the keys is refused on the device (#230). DataFusion
55's duplicate ordinal is #228.

**Corpus queries:** every corpus plan with a grouping-set id: `tpch/rollup-over-join`, tpcds q5,
q14, q18, q22, q77, q80, and `pbench/rollup-small-keys`, the one that reaches it on a device — all
five of its device cells off here, the validator refusing the width. The rest are off first on #152,
#212 or #220, and no longer on #189 or #206: the shuffle stopped hashing the id and the kernel has
its boolean arm. The value shows only through `GROUPING()`, and q70 and q86, its corpus users, are
window queries that never run.

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

<a id="t264"></a>
### #264 — the device refuses a group key that is not a bare column
`CudfAggregate` throws `CudfAggregate: only ColumnRef group exprs supported`
(`cpp/src/operators/aggregate.cpp:163`) for any group expression that is not a `ColumnRef`, so
`GROUP BY` over a cast, an arithmetic expression or a function call is refused at run time on the
device, at every mode. The cpu answers. The planner does not lower a group expression into a
project below the aggregate, and the recipe hands the expression through as it stands — so the
smallest fix is a planner one, and the device needs no new arm.

**Corpus queries:** `pbench/timestamp-s-key-group`
(`GROUP BY arrow_cast(f_ts_s, 'Timestamp(Second, None)')`), its five device cells off here. Not on
[#240](../archive/archived-tickets.md#t240): the timestamp key itself hashes on both engines since `repartition-keys`, which the
three `timestamp-{ms,ns,us}-key-group` rows demonstrate at all five modes. This row carried `240`
until this ticket had a number.

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

<a id="t251"></a>
### #251 — a decimal quotient truncates at its own scale and later arithmetic carries the error up

DataFusion cuts a decimal division at the scale it declared for the result rather than rounding,
and we follow it. The truncated value then feeds the rest of the expression, so the answer is
wrong in digits it printed.

Two shapes, measured against DuckDB by #235's comparison. `(x / y) * 100` multiplies the
truncation by a hundred: tpcds q58's `ss_dev` is `103.719200` against `103.71926462…`, and q61's
ratio `51.82319100` against `51.82319145…` — 45 to 97 units in our last rendered place.
`sum(x / y)` adds one truncation per row: q66's twelve `*_per_sq_foot` columns are 1 to 2 units
short. A quotient nothing consumes stays inside its scale — tpch q1's `avg_qty` is `25.522005`
against `25.522005853…` — so it is the expression around the division that loses the digits.

**Corpus queries:** `tpcds/q58` (columns 2, 4, 6), `tpcds/q61` (column 2) and `tpcds/q66`
(columns 20-31), each `duckdb_divergent(251, ...)` in `corpus_cases.inc`.

**Fix proposed:** round the quotient at its declared scale, or widen the scale a division
declares so the next operand has digits to spend. Either departs from DataFusion's convention,
so the decision comes before the code.

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
cpu tp1 cells turn on. Its device cells no longer meet the decimal export — #187 is archived and
`stale-cells` ran `filter_project`'s four cells green against it; nested-limits' still meet the
zero-column scan and the cross join's batching (#220).

<a id="t282"></a>
### #282 — a scan with no surviving row groups is refused at planning

`SELECT count(*) FROM t WHERE <a predicate every row group's statistics rule out>` does not plan,
on either backend; it should answer `0`. A parquet file with no row group at all does the same:
DuckDB writes an empty table that way.

`partition()` (`planner/translator/scan_mapping/partition.rs:26`) refuses an empty survivor list
("no surviving row groups: what an empty scan means is the caller's decision, not an empty map"),
because the wire reads an empty mapping as one unmapped partition, which reads the whole file.
No caller decides, so the plan fails. Found 2026-10-08 planning keyless-identity; distinct-companions
works around it (`ss_quantity + ss_item_sk < 0` rather than `ss_quantity < 0`).

**Corpus queries:** none: no corpus filter prunes every row group. pbench's `empty` table (chain J)
has none to begin with, so `cross-empty-build` and `outer-on-true-empty` are refused here before
the joins they test. Simplest: `select count(*) from nation where n_nationkey < 0;` (tpch).

**Fix proposed:** a scan with no survivors plans one lane that holds no batch, so its executors
make no call and the lane ends with nothing, the no-arrival case keyless-identity (chain L) makes
every node answer right. In keyless-identity.

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

<a id="t164"></a>
### #164 — a column ordinal reaches cuDF unchecked, and a bad one degrades rather than throws

The C++ half of [#135](../archive/archived-tickets.md#t135), which the planner
closed on the Rust side by checking a reference's name against the field at its position.

`expr.cpp` ~L349 returns `type_id::EMPTY` for an out-of-range `ColumnRef` instead of throwing,
turning a bad ordinal into a confusing type error further along. The third closure #135 named is
unstarted and belongs here too: a per-node type check in the GPU tiers, the only thing that would
surface a wrong-order subtree before the root. 2026-09-17: chain B's `device-schema-harness` and
`driver-output-hook` are that check for the operator and corpus tiers — every device batch held
to its node's names and `{type_id, scale}`; the C++ item above stands. 2026-10-09: the name-count
half is closed — `register_handle`, the only path to a handle number, refuses a handle whose names
do not number its columns, so the check holds however the handle was built (`refcounted-scatter`).

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

<a id="t281"></a>
### #281 — limits' and empty-sorts' device cells have never run

limits and empty-sorts (chain K) run without a GPU, beside chain J, which holds the GPU host. Their
device changes are built and not run: `scan.cpp` no longer applies a scan's limit, which a
`GpuLimit` above the scan now does (#186), and the four `bug_` source cases and three `bug_`
accumulate cases it turns into agreement cases have run on no device. A device answer could differ
from the cpu's, and nothing would say so.

**Corpus queries:** `tpch/scan-limit` and `tpch/nested-limits` at every device mode, and
`tpcds/q17` at `tp1-single`, each off on this ticket once chain K has merged.

**Fix proposed:** on a GPU host, once one is free: run the device test tier and those cells;
each cell passing is enabled, each failing gets the ticket it fails on. Then this ticket drops
from the registry rows and is archived.
<a id="t253"></a>
### #253 — the DuckDB oracle cannot record a divergence it finds on two of its paths

`duckdb_divergent(<ticket>, <positions>)` is how a corpus line records a difference the oracle
found and a ticket explains. It reaches the per-line cpu comparison and nothing else, so two
paths can find a divergence and have no way to say so. Both block tasks already on chain J's
board.

**A device answer that differs from DuckDB while the cpu's matches.** One `duckdb_oracle` per
line serves the cpu case and every `duckdb_gpu_<query>_<mode>` case, and the two values a
developer has both go red: `duckdb_exact` fails the device case, and `duckdb_divergent` fails
the cpu case, because `compare_sections` reports a named column that *agrees* as "stopped
diverging". An empty position list does the same at the row level. So the only green options are
to leave the device cell off or to change the harness. This is the shape `gpu-result.txt` is
keyed by mode for in the first place — a lane split or a shuffle defect shows per mode.

**An over-cap section whose fingerprints differ.** `duckdb_fingerprint` takes no ticket and no
column list, and `compare_sections` routes a fingerprinted section under `duckdb_divergent` to
"declare `duckdb_fingerprint`" — the error text at `test_support/fingerprint.rs` says
`duckdb_divergent does not reach this path`. So an over-cap answer is all or nothing: one
SHA-256 over the rendered rows either matches or the line cannot be green at all.
[`join-backend`](../tasks/join-backend.md) turns tpch q11's cpu cells on, and DuckDB's side of
q11 is already a 27,604-row fingerprint with no approximate column, so that task is the first to
hold a fingerprint nobody has compared.

A third shape reaches the same door: the two writers classing one column differently — a decimal
on our side against a double on DuckDB's, which the [`duckdb-oracle`](../tasks/duckdb-oracle.md)
spec's step 3 asks to be approximate and which cannot be, since neither writer can see the
other's declared type and a fingerprint no longer holds the rows to rehash. Today that is a hard
error naming both classifiers. No committed section has the shape.

**Fix proposed:** give `duckdb_divergent` a side — which of the cpu and the device diverges — and
give `duckdb_fingerprint` the optional ticket and column list `duckdb_divergent` already has, so
a triple or a per-column exemption can carry a known difference while `rows` and the remaining
columns stay checked.

**2026-10-09: `stale-cells` ran and produced no instance.** It was named here as the first task
that could, and its sixteen device cells all matched DuckDB, so the first half is still a shape
nothing has exhibited. The harness was deliberately not widened there: the task's restriction was
the sixteen cells, and a device-only divergence would have stayed off under this ticket.

**pbench moved the deadline, and widened both halves.** pbench landed ahead of `stale-cells`,
and the next task to build was `repartition-keys` — which owns `uint-key-group`, a
`duckdb_fingerprint` line over a 19,848-row answer whose cpu tp4 cells and gpu cells it both turns
on. So the decision is owed before that task, not before `stale-cells`. pbench also took the
fingerprint lines from 4 to 14, ten of the new ones its own rows whose device comparison is
`live_cpu` because the answer is over the cap; and the first half — a device answer diverging
while the cpu's matches — now has 27 pbench rows in reach, all of them DuckDB-green on the cpu
already, in a dataset that exists to make the device differ.

**Decided, 2026-10-08, so that whoever implements it need not re-litigate it.** Two changes, both
additive, neither touching a line that is green today:

- **`duckdb_divergent` gains a side**, written as a trailing non-numeric component:
  `duckdb_divergent(<ticket>, <positions…>[, <side>])` where the side is `both`, `cpu` or `device`.
  Positions are variadic — `duckdb_oracle.rs`'s `divergent` maps all of `args[1..]` through
  `number`, and `corpus_cases.inc:178` writes thirteen components — so the side cannot be the third
  one. A last component that parses as a number, or no last component at all, means `both`, which
  is today's meaning and leaves every existing line alone. The named columns must still differ **on the side
  named** and must still agree on the other, so a cell that stops diverging on the device fails
  exactly as one that stops diverging on the cpu does today. That is what makes the variant worth
  having: it narrows the claim rather than waiving it.
- **`duckdb_fingerprint` gains the optional ticket and column list** `duckdb_divergent` already
  carries: `duckdb_fingerprint(<ticket>, <positions>)`. `rows` and every column not named stay
  checked — the `nonnull` counts, the triples, and the hash recomputed over the exact columns the
  line does not except. A hash cannot be recomputed from a fingerprint, so an excepted column
  means both writers must leave it out of their hash, which makes the exception a property of the
  section rather than of the comparison: the writers read the line's exceptions. That is the part
  that costs work, and it is why this is a ticket and not a line edit.

Rejected: a tenth `corpus_query!` argument for a separate device oracle. It doubles the oracle
column on all 176 lines to express something four or five of them need, and a line's coverage
stops reading off the line. Also rejected: classing every decimal approximate so the fingerprint
needs no exception — it takes 1.5M rows of `o_totalprice` out of row-for-row checking, which is
the thing the fingerprint exists to do.

<a id="t254"></a>
### #254 — `data_fusion_subset` is the one cpu oracle no test can show failing

[#235](../archive/archived-tickets.md#t235)'s harness item was that the corpus helpers are proven to fail on a wrong answer,
and `duckdb-oracle` delivered it for two of the three `CpuOracle` variants: `results_match` and
`result_matches` were split out of their panicking wrappers and are driven by negative cases.
`CpuOracle::DataFusionSubset` routes to `assert_subset_of_unlimited` (`test_support/corpus.rs`),
which runs a live DataFusion query and takes no injectable answer, so nothing hands it a wrong
one and `every_oracle_variant_is_named_by_some_line` is all that holds it.

One corpus line uses it — `tpch/scan-limit`, an unordered `LIMIT` over lineitem whose row set is
not determined — so the exposure is small and the fix is the same shape as the other two: split
the comparison from the query, and hand the split a doctored answer that is not a subset.
