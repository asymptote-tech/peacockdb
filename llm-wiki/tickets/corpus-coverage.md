
# Corpus Rollout tickets

Tickets required for corpus rollout (CPU+GPU, all modes), TPC-H numbered and named queries, and 84/99 TPC-DS queries (window functions excluded). Minimum SQL functionality needs to be built for this milestone.


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
row and the device emits none. A wrong answer rather than a refusal, and nothing refuses it.
Shown on a device by `bug_a_global_merge_over_no_arrival_answers_nothing_on_the_device`
(`gpu_tests/aggregate_cases.rs`), which also shows the CPU's row is sum's identity, not count's:
a count merges by sum, so a merged count over nothing is NULL there where SQL says 0. The init over
a zero-row batch keeps its row on both.

A second site, on both backends: the single-node shortcut (`translator/aggregate.rs:322-339`), a
keyless `GpuAggregate` over a single-batch input. When no batch arrives the node is never called
(`single_partition.rs:192,248-251`) and no row comes out — symmetric, so the cpu-vs-device
comparison cannot see it. A `GpuLimit` keeps its input's single-batch layout, so a limit over a
sort, merge or coalesce feeds it, and #214's dropped batch lands here. The rule for both sites is
one: a keyless aggregate answers one row whatever arrived. No corpus query is known to reach it.

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
sequence only collapses lanes, which keeps the one-row batch.

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
of them: the four multi-batch modes are held by #152, tp1-single by #183 (the plan's string keys
are `Utf8View`). Registry row 67 tags `55 152 185`; `185` looks like a typo for `183`.

**Fix proposed:** no code change — a proof, then archive. Add a walk test beside
`each_lane_merges_its_own_state_before_the_cross_lane_merge_folds_them` in
`wire/gpu_tests/mod.rs`: `SUM_OF_QUOTIENTS`, a grouped `sum(l_extendedprice / l_linenumber)`
under an outer `sum`, through `assert_walk_matches_datafusion` at `TWO_LANES`. The oracle
compare on the digits is the proof: a merge that re-evaluated the quotient over state would
throw or answer different digits. Pin the `PARTIAL`, `MERGE` and `FINALIZE` counts from the
trail, as the neighbouring tests do, so the shape cannot quietly lose its merges. Green on
shad-gpu closes #55: drop `55` from registry row 67 and archive the ticket as stale. Red means
the defect is live, and the throwing call names its phase.

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

<a id="t214"></a>
### #214 — a limit drops a zero-row batch on both backends

A `GpuLimit` handed a batch of zero rows emits nothing for it, on the cpu and on the device alike.

Both `LimitStream`s take their cut from the one `RowInterval::range_of` (`plan/interval.rs`),
which answers `None` when `start < stop` is false — and it is false for `n_rows == 0` whatever
the interval — so the batch is released as if it lay outside the interval. Nothing and a zero-row
batch are different arrivals downstream, as #205 says: a build side of `filter → limit → coalesce`
with zero survivors reaches `without_build` and #175's refusal for Right, Full and RightAnti,
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

**Corpus query:** `tpcds/q17` at `tp1-single`, its device cell off on this ticket. The 12 bytes
are the device's one zero-row batch: three `Utf8` columns, one 4-byte offset each. Simplest:
`select n_name from nation where n_nationkey < 0 order by n_name;` (tpch).

**Fix proposed:** `SortedRuns::mark_done_and_fetch` and the merge's `accumulate_and_fetch` skip
the sort when every held batch has zero rows and hand the held batches to
`coalesce_or_nothing`, which answers one zero-row batch under the schema, as the device does.
The empty-arrival arm stays nothing. The three `bug_` pins flip to green cases.

## Scalars

<a id="t191"></a>
### #191 — the device exports Int16 for an extracted year the plan declared Int32

`tpch/q8` at `tp1-single`: `the exported stream is not the sink's rows: expected Int32 but found
Int16 at column index 0`. That column is `o_year`, an `extract(year from o_orderdate)` — DataFusion
types it `Int32` and the device answers `Int16`.

**Not [#187](../archive/archived-tickets.md#t187), and merging them would lose the distinction.** That one is the
device *widening* a decimal, to 38 whatever the declaration says. This is the device *narrowing* an
integer, to the natural width for a year rather than to a maximum. Opposite direction, different
type family, and a fix for either says nothing about the other.

Not new behaviour either, only newly reached: `extract_year -> INT16` was already on record as a
place where the DataFusion type is an imperfect proxy for the cuDF one. What is new is a corpus
query whose unload sees it.

One cell, `tpch/q8` at `tp1-single` — which is the only mode that gets far enough to reach the
unload, the other four stopping at [#152](../tickets.md#t152). Pinned at the project by
`bug_a_year_extracted_from_a_date_is_exported_as_int16` (`gpu_tests/exec_cases.rs`).
`utf8-everywhere`'s rollout, 2026-09-16, added `tpch` q7 and q9 at the same mode, so three
registry rows carry it.

**Corpus queries:** `tpch/q7`, `q8` and `q9` — the only corpus queries that extract a date
field. All three stop here at `tp1-single`; their other four modes stop earlier, at #152.
Simplest: `select extract(year from o_orderdate) from orders;` (tpch).

<a id="t210"></a>
### #210 — a bare decimal literal on the AST path comes back as a Float64 column

cuDF's AST has no fixed-point literal, so `ast_scalar` (`expr.cpp`) rewrites a `Decimal128`
literal as a scaled double before the one scalar builder; under a `CAST(… AS Float64)` that is
the type the plan asked for, but a bare or unary-wrapped decimal literal is AST-able too, and
`SELECT 1.5 FROM t` then computes a `FLOAT64` column on the device where the plan declares
`Decimal128(2, 1)` — and since `decimal-precision-at-export` the export refuses it by name rather
than answering it, the AST path still computing a double. Same class as
[#191](tickets/corpus-coverage.md#t191): a declared type produced as another. Pre-existing, carried
through `typed-nulls.md` by that spec's own instruction, and pinned by
`bug_a_bare_decimal_literal_is_a_float64_column_on_the_device` (`gpu_tests/exec_cases.rs`);
the walk `Literals.EveryWireTypeEitherMakesAnAstLiteralOrSaysWhyNot` names it on its
`Decimal128` row. The likely fix is `is_ast_able` refusing a bare decimal literal as it already
refuses a decimal operand, so the column path builds a real `fixed_point_scalar`.

<a id="t57"></a>
### #57 — the device refuses a value-form CASE
`build_column_case` (`cpp/src/expr.cpp`) throws `value-form CASE not supported in column path`
for any `CASE x WHEN v THEN …`. The search form, `CASE WHEN x = v THEN …`, folds through
`copy_if_else` and works. `bug_a_value_case_is_refused_on_the_device`
(`src/tests/gpu_tests/exec_cases.rs`) pins the refusal, and `plan/mod.rs` lists it among the
known refusals.

Filed as a wrong answer: the value form came back all-0 or all-null, so it was reverted to a
throw. `reports/corpus-fixes.md` (fix 7) found that a misdiagnosis. The gtest built every Int64
literal as 0, because `CreateScalarValue`'s second parameter is `is_null`, and the lowering had
run q39 correctly at 8f471cc0. The guard stayed on that false measurement.

**Corpus queries:** `tpcds/q39` — `CASE mean WHEN 0 THEN NULL ELSE stdev/mean END` in a project
and `CASE mean WHEN 0 THEN 0 ELSE stdev/mean END > 1` in a filter, each twice per plan (the CTE is
read twice). All five gpu cells are off and registry row 40 tags `57` alone. At tp1-single this
is the refusal (`corpus_cases.inc:284`); the other four modes have not run on a device and may
meet [#152](joins.md#t152) next.

**Fix proposed:** fix 7 of `reports/corpus-fixes.md`, about 20 lines in `build_column_case`.
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
`reports/corpus-fixes.md` traced the recorded error to the old Final phase, which evaluated the
arguments in every phase — the same root as #55.

Neither path is left. The aggregate builds no AST: only `filter.cpp` calls `is_ast_able`. Its
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

<a id="t145"></a>
### #145 — Refcounted handles: stop copying every partition out of a scatter
`spark_hash_partition` returns one table whose N partitions are already contiguous, and
`node_session.cpp` (~L265-272) deep-copies each range out, because a handle owns its memory.

So every shuffle copies its whole input a second time and peaks at twice the data — the concrete
form of [#91](#t91)'s repartition spike, once per aggregate and once per join side. The change:
`TableResult` (`plan_executor.h:13`) becomes a `shared_ptr<cudf::table> owner` plus a
`cudf::table_view view`, and the scatter registers N handles sharing one owner. Mechanical but
wide — 35 sites across 11 files touch `.table` / `->table`. **No ABI change**: a handle stays a
`u64`. The cost to weigh: a slice pins its whole parent, so a skewed hash leaves one hot lane
holding the pre-scatter table — the peak halves and the tail lengthens. Also unlocks
[#140](#t140). Tests: the GPU tiers stay byte-identical, plus a gtest releasing N−1 handles and
reading the survivor. A streamed join waits on it too: a handle is erased by its reader
(`node_session.cpp:254`), so `Input::BuildSideCopy` has no build side after the first probe batch,
and T16 refuses a second until this lands ([#152](#t152)).


<a id="t95"></a>
### #95 — a decimal partition key is refused on the device
murmur3 covers int/date/timestamp/composite/null; decimal deferred (float indefinitely).
Needed by the first shuffle on a decimal key (tpch q18 `o_totalprice`, q10 `c_acctbal`,
tpcds `i_current_price`). Dispatch by *logical* precision (≤18 → low 8 LE bytes of int128;
>18 → raw 16B LE) and thread precision through the partition FFI. Until then
`spark_hash_partition.cu`'s type switch fails with `unsupported key column cuDF type_id=27`,
which is what [#184](tasks/active-tickets.md#t184)'s q15 hits on `total_revenue`. The cpu's comet
hasher takes the decimal, so the shape is a refusal on one side. Pinned by
`bug_a_decimal_key_is_refused_on_the_device` (`gpu_tests/emit_cases.rs`).

**Corpus queries:** eight hash a decimal key at every `tp4` mode — tpch q2 (`ps_supplycost`),
q10 (`c_acctbal`), q15 (`total_revenue`, precision 38), q18 (`o_totalprice`); tpcds q24, q37,
q82 (`i_current_price`), q75 (`sales_amt`, precision 31). Their `tp4` device cells are off, on
blockers that refuse first (#152, #184); q15 and q75 need the >18 path.

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
needs the assert and not the observation. Land before [#155](#t155).

The `expr.cpp` site is the cheapest to fix and the most expensive to leave. `build_column`'s
`ColumnRef` arm copies the whole column and the caller takes `->view()` of the copy one line
later; every consumer (`cudf::binary_operation`, `unary_operation`, the function arms) takes
a `column_view`, and the input table outlives the call. Returning `table.column(idx)` — or
resolving `ColumnRef` leaves in `build_column_binary` before recursing — needs no ownership
change. It fires once per `ColumnRef` leaf per batch on every predicate `is_ast_able` rejects
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

The C++ half of [#135](archive/archived-tickets.md#t135), which the planner
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

**Fix proposed:** one case table both suites read, the instrument `src/tests/executor_cases.rs`
already is for operators. A text file of `offset length rows → begin end` lines, `max` spelling
the sentinel, beside the gtest in `cpp/tests/cpu/`. The Rust test embeds it with `include_str!`
and checks `RowRange::clamp` after mapping `(offset, length)` to `(offset, offset + length)`. The
`ClampRowRange` gtest reads it through a path CMake passes as a compile definition. The fourteen
existing cases move into the file, deduplicated, and the per-side literals go. Both run on the cpu
tier, with no FFI and no device. A case added once then reaches both clamps, and a drift on
either side fails that side's test on the shared line.
