
# Corpus Rollout tickets

Tickets required for corpus rollout (CPU+GPU, all modes), TPC-H numbered and named queries, and 84/99 TPC-DS queries (window functions excluded). Minimum SQL functionality needs to be built for this milestone.


## Contents

- [Aggregates](#aggregates)
  - [#264 (computed group key) the device refuses a group key that is not a bare column](#t264)
  - [#267 (tp4 single-mode aggregate) a Single-mode aggregate over a scan split into lanes is refused at tp4](#t267)
  - [#289 (Welford count wrap) a Welford merge on cuDF 25.02 wraps a count past 2^31 silently](#t289)
- [Sort / Limit](#sort--limit)
  - [#202 (desc sort null order) a descending sort key puts its nulls on the wrong end on the device](#t202)
  - [#217 (sort fetch 0) a sort with `fetch 0` keeps every row on the device](#t217)
  - [#204 (merge drops fetch) the device's sorted merge drops its fetch when it is handed one input](#t204)
  - [#214 (limit drops empty batch) a limit drops a zero-row batch on both backends](#t214)
- [Scalars](#scalars)
  - [#168 (interval literal on wire) interval type can not be represented in the fbs ScalarValue](#t168)
  - [#210 (AST decimal literal) a bare decimal literal on the AST path comes back as a Float64 column](#t210)
  - [#56 (q2 CASE in partial sum) q2: CASE-over-string-equality inside a partial-phase sum](#t56)
  - [#60 (round() `round(x, p > 0)` on the device differs from DataFusion by one ulp](#t60)
  - [#251 (decimal quotient scale) a decimal quotient truncates at its own scale and later arithmetic carries the error up](#t251)
- [Testing](#testing)
  - [#227 (schema nullability tests) Check schema nullability in tests](#t227)
  - [#174 (two clamps, one rule) two clamps for one rule, and nothing compares them](#t174)
  - [#233 (pass-through col count) the plan validator does not check that a pass-through node keeps its input's column count](#t233)
  - [#253 (DuckDB divergence record) the DuckDB oracle cannot record a divergence it finds on two of its paths](#t253)
  - [#254 (untestable subset oracle) `data_fusion_subset` is the one cpu oracle no test can show failing](#t254)
  - [#287 (row order unchecked) no corpus compare checks the order an `ORDER BY` asks for](#t287)

## Aggregates

<a id="t264"></a>
### #264 (computed group key) the device refuses a group key that is not a bare column
`CudfAggregate` throws `CudfAggregate: only ColumnRef group exprs supported`
(`execute_aggregate`, `cpp/src/operators/aggregate.cpp`) for any group expression that is not a `ColumnRef`, so
`GROUP BY` over a cast, an arithmetic expression or a function call is refused at run time on the
device, at every mode. The cpu answers. The planner does not lower a group expression into a
project below the aggregate, and the recipe hands the expression through as it stands — so the
smallest fix is a planner one, and the device needs no new arm.

**Corpus queries:** `pbench/timestamp-s-key-group`
(`GROUP BY arrow_cast(f_ts_s, 'Timestamp(Second, None)')`), its five device cells off here. Not on
[#240](../archive/archived-tickets.md#t240): the timestamp key itself hashes on both engines since `repartition-keys`, which the
three `timestamp-{ms,ns,us}-key-group` rows demonstrate at all five modes. This row carried `240`
until this ticket had a number.

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

<a id="t202"></a>
### #202 (desc sort null order) a descending sort key puts its nulls on the wrong end on the device

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

## Scalars

<a id="t168"></a>
### #168 (interval literal on wire) interval type can not be represented in the fbs ScalarValue

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
### #210 (AST decimal literal) a bare decimal literal on the AST path comes back as a Float64 column

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
`Decimal128` row. **Corpus queries:** `tpcds/q5`, whose `0.00 as return_amt` and `net_loss` the
schema hook refuses as FLOAT64 at every mode. The likely fix is `cudf_ast_can_evaluate` refusing
a bare decimal literal as it already refuses a decimal operand, so the column path builds a real `fixed_point_scalar`.

<a id="t56"></a>
### #56 (q2 CASE in partial sum) q2: CASE-over-string-equality inside a partial-phase sum

Filed against the old executor: the device built a cuDF AST for `sum(CASE WHEN <string equality>
…)`, and cuDF refused with binaryop "Unsupported operator", since its AST cannot compare strings.
[`reports/corpus-fixes.md`](../reports/corpus-fixes.md) traced the recorded error to the old Final phase, which evaluated the
arguments in every phase — the same root as #55.

Neither path is left. The aggregate builds no AST: only `filter.cpp` calls `cudf_ast_can_evaluate`. Its
`arg_col` (`cpp/src/operators/aggregate.cpp`) builds a computed argument with `build_column`,
which compares strings, and a merge reads state columns by reference.

**Corpus queries:** `tpcds/q2`, whose `sum(CASE WHEN d_day_name = 'Sunday' …)` repeats the shape
per weekday. Its five gpu cells are off, on [#60](#t60)'s `round` as measured: every per-node
section matches the cpu's and the answer's sums agree, so nothing of this shape showed; registry row
3 tags `56 60`. The harness test's comment counts the shape 48 times
in the corpus.

**Fix proposed:** likely fixed already. `a_grouped_sum_of_a_case_over_a_string_equality_agrees`
(`src/tests/gpu_tests/aggregate_dimension_cases.rs`) runs this shape through a device
`GpuAggregate` and matches the CPU, green since 2026-09-16. It covers the init alone, one operator,
not a plan's merges across lanes. The complete proof is `tpcds/q2` enabled at every gpu mode once
#152 lands; green there, archive #56 and drop `56` from registry row 3.

<a id="t60"></a>
### #60 (round() `round(x, p > 0)` on the device differs from DataFusion by one ulp

The device rounds a float to `p > 0` places with two roundings where DataFusion uses one, so about
one value in twenty at or above 1.0 comes back one ulp off: `2.7800000000000002` for `2.78`.

`expr.cpp` calls `cudf::round(col, places, HALF_UP)` for any `places`. cuDF's kernel takes `modf`
and computes `ip + round(fp·10^p)/10^p`; DataFusion computes `(x·10^p).round()/10^p`. `fl(0.78)`
puts `2 + fl(0.78)` on a midpoint of the result grid, and ties-to-even takes the upper double.
Re-emulated over q78's committed answer, exactly five `ratio` cells split under `golden_exact`;
q2 takes the same kernel seven times per row. This ticket used to describe an anti join and memory
pressure on q78 ([`reports/corpus-fixes.md` fix 6](../reports/corpus-fixes.md#fix6) found neither), and called `round` proven by q54.

**Corpus queries:** tpcds q78 and q2, every gpu cell off on this ticket. Measured on an L40S
(join-backend's task 14): q78 differs from the golden in exactly the five `ratio` cells at all
five modes, `2.7800000000000002` among them, and DuckDB says `2.78`; q2 matches every per-node
section and differs in its answer, its row count and column sums agreeing to the printed digit. Simplest: `select n_nationkey, round((cast(n_nationkey as double) * 25.0) / 9.0, 2) as r
from nation;` (tpch): two rows differ on the device today.

**Fix proposed:** [fix 6 of `reports/corpus-fixes.md`](../reports/corpus-fixes.md#fix6), in `expr.cpp`. `places == 0` keeps
`cudf::round(·, 0, HALF_UP)`. Otherwise multiply by a FLOAT64 scalar `10^|p|` (exact for p ≤ 22;
`1/10^|p|` for a negative p), `cudf::round(·, 0, HALF_UP)`, divide: bit-identical to DataFusion.
The comment beside it names which kernel agrees. The cpu relays `round` to DataFusion,
untouched; no golden moves. Test: a walk test `ROUND_PLACES` running the query above at
`ONE_LANE`, red on two rows today; `scripts/exec_model/operators/expressions.py`'s docstring.
Registry: q78's row drops `60`. `round` over Float32 is #221.

<a id="t251"></a>
### #251 (decimal quotient scale) a decimal quotient truncates at its own scale and later arithmetic carries the error up

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

<a id="t253"></a>
### #253 (DuckDB divergence record) the DuckDB oracle cannot record a divergence it finds on two of its paths

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
### #254 (untestable subset oracle) `data_fusion_subset` is the one cpu oracle no test can show failing

[#235](../archive/archived-tickets.md#t235)'s harness item was that the corpus helpers are proven to fail on a wrong answer,
and `duckdb-oracle` delivered it for two of the three `CpuOracle` variants: `results_match` and
`result_matches` were split out of their panicking wrappers and are driven by negative cases.
`CpuOracle::DataFusionSubset` routes to `assert_subset_of_unlimited` (`test_support/corpus.rs`),
which runs a live DataFusion query and takes no injectable answer, so nothing hands it a wrong
one and `every_oracle_variant_is_named_by_some_line` is all that holds it.

One corpus line uses it — `tpch/scan-limit`, an unordered `LIMIT` over lineitem whose row set is
not determined — so the exposure is small and the fix is the same shape as the other two: split
the comparison from the query, and hand the split a doctored answer that is not a subset.

<a id="t287"></a>
### #287 (row order unchecked) no corpus compare checks the order an `ORDER BY` asks for

Every corpus compare sorts both answers' rows before comparing them. `batches_to_sorted_str`
(`test_support/result_text.rs`) pretty-prints a result and sorts its data lines, and the cpu
oracles (`data_fusion_*`) and the device's (`golden_*`, `live_cpu`) all compare that text. So a
query that ends in `ORDER BY` and answers the right rows in the wrong order passes on both
engines. An ordering bug is caught only where it changes which rows survive, as under
`ORDER BY … LIMIT n`. [#202](#t202), a descending key's nulls on the wrong end on the device, is
one that an order-blind compare misses wherever no limit cuts the misplaced rows.

**Corpus queries:** every query whose answer DataFusion orders at the root — most of TPC-H and
TPC-DS end in `ORDER BY`.

**Fix proposed:** where the oracle's physical plan has a sort at its root, keep the multiset
compare and add an order check: our rows must be non-decreasing in the sort's keys, with their
directions and null placement. Ties stay free, so a stable and an unstable sort both pass. The
same check serves both engines, since the device compares against the cpu's answer.
