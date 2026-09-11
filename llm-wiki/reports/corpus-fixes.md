# Corpus fixes

Localized fixes for the tickets that keep corpus cells disabled, read at `188c23ce`. Sources:
`tickets.md`, `tasks/active-tickets.md`, `tests/common/corpus_cases.inc`,
`testdata/cost-registry.csv`, `reports/hacks-audit.md` and the code. Ordered by complexity,
lowest first; within a band by the cells the coverage table books to the fix, cpu and gpu
together, most first. The raw proposals and critiques are in
`llm-wiki/reports/bugfix-proposals/`: one file per ticket per role, `NN-proposal.md` and
`NN-review.md`, plus the consolidation rounds. Registry today: cpu 444 enabled / 156 disabled /
90 na; gpu 6 / 594 / 90. Every gpu figure is a candidate until a shad-gpu run.

## Contents

| # | fix | closes | complexity | cells back (cpu / gpu) |
|---|---|---|---|---|
| 1 | CPU nested-loop join keeps its projection | #190 | S | 10 / 0 |
| 2 | The rollup shuffle hashes user keys, never the grouping id | #189 | S | 9 / 0 |
| 3 | A keyless aggregate's identity row comes from init, on the finalizing node only | #180 | S | 9 / 0 |
| 4 | One spelling of the zero-column table on a device, at the scan and the cross join | #63, ticket 1 | S | 0 / 5 |
| 5 | `date_part` casts to the declared return type | #191 | S | 0 / 0 |
| 6 | `round(x, p > 0)` on the device is three IEEE operations, like the oracle | #60 | S | 0 / 0 |
| 7 | Value-form CASE takes the column path | #57 | S | 0 / 0 |
| 8 | Strings are declared `Utf8` because that is what both readers read | #183, #192 | M | 5 / 27 |
| 9 | Two session rules let DataFusion 45 plan q27 and q72 | #23 | M | 10 / 0 |
| 10 | The CPU join returns one batch per call, as every other executor does | #185 | M | 0 / 9 |
| 11 | `count(DISTINCT x)` with any companion lowers to two stages | #62 | M | 5 / 0 |
| 12 | A decimal shuffle key hashes as comet does, by declared precision | #184, #95 | M | 0 / 3 |
| 13 | A limited scan is one lane and one batch over the row groups that cover the limit | #186, #188 | M | 2 / 0 |
| 14 | The grouping id is DataFusion's value and width on the device | #65 | M | 0 / 0 |
| 15 | A finish with no probe keys answers from the build side it still holds | #173 part A | M | 0 / 0 |
| 16 | A whole-day interval literal crosses the wire | #168 | M | 0 / 0 |
| 17 | A state column's type is the SQL aggregate the executor runs it as | #163 | L | 115 / 0 |
| 18 | A handle can be retained, so a build side survives a streamed probe | #152 | L | 0 / 22 |

## Proposals

### 1. CPU nested-loop join keeps its projection — S

**Closes:** #190. Re-attributes nothing. Clears the same axis for tpch/q22 and tpcds/q24, which
stay on #163 (fix 17).

**Issue.** `executor/cpu_backend/join.rs:140-146` builds `NestedLoopJoinExec::try_new(build,
probe, Some(filter), &join_type, None)`. The node declares the projected schema
(`translator/nodes.rs:505-512`), so `declared_as` (`cpu_backend/mod.rs:239`) refuses on column
count at the first `probe_and_fetch`. DataFusion embeds the projection when the filter reads a
column the output drops (`nested_loop_join.rs:566-598`); the translator keeps it,
`check_projection` validates it (`plan/join.rs:66-75`), the wire writes it
(`wire/join.rs:342-386`), `join.cpp:519-529` applies it. Only the CPU constructor passes
`None`; the hash-join arm at `:303-313` passes it. Off: tpch/q11 × 5 and tpcds/q54 × 5 cpu
(registry rows 111, 55; `corpus_cases.inc:114,219`), and their 10 gpu cells behind them.

**Localized fix.** `cpu_backend/join.rs:140-146`: map `node.projection` to `Option<Vec<usize>>`
as `:303-306` does (a shared helper is fine) and pass it. Does not touch the wire, the C++, or
the cross-join arm (its projection gap is ticket 7). CPU and GPU: the CPU applies the
projection the C++ already applies. Frozen surface: none. Regression test in
`cpu_backend/tests/join.rs` beside `:365`: `Some(vec![1, 3])` over `dim × FACT` with `k > fk` →
`c|20`, `c|21`; red today (4 fields against 2); optionally `Some(vec![3, 1])` to pin the
reorder. Goldens: none regenerated; 22 sections authored (q11, q54 × 5 modes of
`.cpu.txt`/`.cost.txt`, one `mini.result.txt` section each) under `PCK_UPDATE_SECTIONS=1`.
Registry rows 111/55 cpu → enabled; gpu q54 × 5 and q11 × 4 → `152` (fix 18), q11 tp1-single →
`185` (fix 10). `corpus_cases.inc:103-106,114,212-213,219`; `build-test.md:42` N 447 → 457 and
its stale "37 queries … thirteen on #163". Order: author the sections after fix 10's regen, one
regen for both. `operator-cases-impl.md:1184-1188` (task 9) plans a `bug_` test for this;
whichever lands second deletes it. `cpu_backend/join.rs` is a task 12 file — before task 4 or
after task 13. One production line, one ~30-line test, no device run.

**Minimum corpus query.**

```sql
SELECT b.n_name FROM region a, nation b WHERE a.r_regionkey < b.n_regionkey
```

tpch sf1, tp1-single, cpu. Plans as `GpuNestedLoopJoin … projection=[n_name@1]`; refused at the
first probe call, "3 columns … 1 field"; 50 rows after. Not in testdata (`nested-loop-join.sql`
is `SELECT *`).

### 2. The rollup shuffle hashes user keys, never the grouping id — S

**Closes:** #189. Moves tpcds/q77 × tp4's next wall from #175 to this and then to #175 shape
(b), a wall; its `.inc` comment stops naming #189 as a second candidate. Makes #65's gid
divergence observable at tp4 (say so in #65).

**Issue.** `shuffle_below` (`translator/aggregate.rs:39-67`) copies DataFusion's
`FinalPartitioned` hash keys, which include `__grouping_id`
(`datafusion-physical-plan-45/src/aggregates/mod.rs:225-243, :817-818`). comet's murmur3 has no
unsigned arm, so `CpuEmitter::emit` (`cpu_backend/emit.rs:60`) refuses "Unsupported data type
in hasher: UInt8" at run time. `aggregate_sequence` consumes the `Shuffle` unchanged at
`:370-371` although it knows the gid sits at `group.expr().len()` (`:264`). The subset rule
`hashKeys ⊆ group columns` (`plan/aggregate.rs:236-256`, `architecture.md:273-275`) already
permits hashing the user keys alone; nobody implemented it. Hashing the gid could never agree
across engines anyway (fix 14). Off: tpch/rollup_over_join, tpcds/q5, tpcds/q80 ×
tp4-single/rowgroup/sized cpu (9 cells; `corpus_cases.inc:97,218,251`; registry rows 134, 6,
81) and their gpu cells behind. Golden evidence: `tpch.sf1/tp4-single.plans.txt:2738`,
`tpcds.sf1/tp4-single.plans.txt:2687,3338,8671,14962,15968`.

**Localized fix.** `translator/aggregate.rs`, directly above `tree = match shuffle` (`:370`):
if `Shuffle::ByHash { keys, .. }` and `!group.is_single()`, `keys.retain(|k| *k as usize !=
group.expr().len())`. Guard on `group.is_single()`, not `grouping_sets.is_empty()`. Optional:
refuse any remaining key `>= group.expr().len()` (`reports/hacks-audit.md:260`, last bullet).
No hasher arm on either side, no C++, no wire field. CPU and GPU: neither hasher sees the gid.
Frozen surface: none; `recipe-payloads.txt:3341-3343` (tpcds q5 `#92 CudfRepartition`)
`hash_exprs` shrinks — a deliberate `PEACOCK_REWRITE_RECIPE_BYTES=1` regen, argued in the
commit. Moves: six tp4 plan goldens, 18 sections (the emit's `hash=`/`hashed_on`, the merge's
`hashed_on`, the project and sort above); nine execution sections authored; `.result.txt`
re-stamps for the three queries; `corpus_cases.inc:97,218,251` and comments `:89-96, :210-216,
:234-237, :244-247`; registry rows 6/81/134 cpu tp4 → enabled, `189` dropped;
`build-test.md:42` (q80 clause, N +9). Pins: a planner test asserting `emit.hash_keys == [0,
1]` via `translated_at_tp4(…, 0)`; `scripts/exec_model/tests/plan_helpers.py:121` emits on
`keys`. `architecture.md:273-275` and `:294-295` become true. Helper's edits:
`walk-drives-every-plan.md:126`, `-impl.md:106`, `declared-schemas-derived.md:99` drop #189
from refusal lists; `active-tickets.md:221`. Order: before fix 17's tp4 rollout of q18/q22; fix
14 either order, this first is cleaner. Sections after fix 10's regen; the plan goldens collide
with task 10 — before task 4 or after task 13. Later cells: q18/q22 × tp4 (6) once fix 17
lands; q77 × tp4 (3) once #175 shape (b) is fixed. gpu stays: q5 #152; q80, rollup_over_join
#152/#183. ~12 planner lines, one unit test, no device run.

**Minimum corpus query.**

```sql
SELECT l_returnflag, sum(l_quantity) FROM lineitem GROUP BY ROLLUP(l_returnflag)
```

tpch sf1, cpu, tp4-single (also rowgroup, sized). Plans and validates with
`hash=[l_returnflag@0, __grouping_id@1]`; refused in `CpuEmitter::emit`; four rows after. Not
in testdata (the projected columns read 5.3 MB, just over the small-table threshold).

### 3. A keyless aggregate's identity row comes from init, on the finalizing node only — S

**Closes:** #180. Narrows #199 to the finalizing-node divergence (the device answers no row
where the CPU answers the identity) plus the mid-plan-limit route (ticket 13). Rewords #173's
`:266-267` clause.

**Issue.** `cpu_backend/accumulate.rs:366-383` `mark_done_and_fetch`, clause
`self.state.is_none() && !self.grouped` (`:372`), runs `compact()` over no input. DataFusion's
no-grouping stream emits one row from fresh accumulators, whose `sum` state is NULL;
`declared_as` refuses the NULL in `count(*)` declared `Int64, nullable=false`
(`translator/aggregate.rs:150-157`). No shuffle is involved; the ticket's mechanism is wrong.
An aggregate's identity is what its init produces over no rows (`count` → 0), not what its
merge produces (`count` merges by `sum`); and a merge-only node is an intermediate, so it owes
nothing — which is what the device does (`gpu_backend/accumulate.rs:307-320`). The empty lane
comes from tp4-single cutting a one-row-group table `[[[0]],[],[],[]]` (`lanes_for`,
`nodes.rs:381-389`) or from an Inner join whose build scattered into fewer lanes than exist
taking `NoBuild` and draining (`partitioned.rs:381-385`, `single_partition.rs:317-329`). Off:
tpcds q96, q90, q88 × tp4-single/rowgroup/sized cpu (9 cells; `corpus_cases.inc:137,206,267`;
registry rows 89, 91, 97).

**Localized fix.** `cpu_backend/accumulate.rs`: delete the `!self.grouped` clause and the
`grouped` field (`:314-316`); `AggregateBatches` gains `identity: Option<Vec<ScalarValue>>`,
set in `CpuAccumulator::aggregate` (`:65-101`) only when `group_by.is_empty() &&
finalize.is_some()`, from `plan::identity_state(node.intermediate())`; `mark_done_and_fetch`
emits it in place of a merge over nothing; a merge-only node emits nothing.
`plan/aggregates.rs`: `identity(PlanAgg, &DataType) -> Result<ScalarValue>` — Count/Mean/M2 →
zero, Sum/Min/Max → typed NULL via `ScalarValue::try_from`, MergeM2 → error.
`plan/aggregate.rs`: `identity_state(&Schema)` walking `agg_state` positions through
`decomposition(func).state`. Does not touch the device, the empty-scatter drop (task 12),
nullability, the wire, or the `GpuAggregate{final}` shortcut (ticket 13). CPU and GPU: both
emit nothing from a merge-only node over nothing; the finalizing node's identity row is the
residual #199 divergence, named. Frozen surface: none; no golden regenerated. Tests:
`cpu_backend/tests/accumulate.rs:700-748` becomes three (finalizing keyless → `Int64(0)`;
merge-only keyless → nothing; identity == init over an empty batch); e2e cases for the queries
below. Moves: `corpus_cases.inc:137,206,267` to five modes; registry 89/91/97 cpu tp4 →
enabled; nine tp4 sections authored; `build-test.md:42`; `reports/hacks-audit.md:442-460`
narrowed to the finalizing node. Order: independent of fix 17 in code; fix 11 shares the
nullability rule and follows fix 17; sections after fix 10's regen; `cpu_backend/accumulate.rs`
is a task 12 file — before task 4 or after task 13. gpu stays on #152 at tp4. Four files, under
80 lines of logic, no device run.

**Minimum corpus query.**

```sql
SELECT count(*) FROM store WHERE s_store_name = 'ese'
```

tpcds sf1, cpu, tp4-single: one row at tp1, the non-nullable NULL refusal at tp4-single (the
loader declares `MultipleBatches` whatever the batching, so the per-lane merge-only node is
inserted). All tp4 modes: `SELECT count(*) FROM store, store_sales WHERE ss_store_sk =
s_store_sk AND s_store_name = 'ese'`. Every lane empty, the finalize path: the same with `'no
such store'` → `0`. None in testdata.

### 4. One spelling of the zero-column table on a device, `__rowcount__`, at the scan and the cross join — S

**Closes:** #63 (misdiagnosed: the cross join appends the device's placeholder column and
shifts every ordinal above; cuDF fails in `have_same_types`, not a size check) and ticket 1,
the scan half. Drops #63 from the "#55/#56/#63" class at `architecture.md:61-63` and
`plan/mod.rs:60-62`.

**Issue.** `execute_project` with no `exprs` emits one INT8 column `__rowcount__`
(`cpp/src/operators/project.cpp:20-31`) — the device's only spelling of a zero-column table,
since `cudf::table::num_rows()` reads column 0. `execute_cross_join` appends both name lists
and drops nothing (`join.cpp:390-398`), so q9's fifteen cross joins over `GpuProject: exprs=[]
schema=[]` (`tpcds.sf1/tp1-single.plans.txt:10174-10175`) hand the top project a sixteen-column
table declared fifteen — a silent wrong answer; nothing between device nodes checks column
count (`produced()`, `gpu_backend/mod.rs:196-212`, prices from the declared schema — #164). A
scan projected to no columns reads a zero-column zero-row table (`scan.cpp:37-52`; a present
empty `.columns({})` selects nothing) and `cudf::cross_join` refuses "Left table is empty"
(`cross_join.cu:45`). Keeps off nothing today (q9 sits on #163, registry row 10 `63 163`);
nested_limits gpu × 5 hits the scan arm right after fix 13.

**Localized fix.** (a) `cpp/src/peacock/operators.h`: `kRowCountColumn = "__rowcount__"`,
`row_count_table(rows)`, `is_row_count_only(t)` (one column, that name, INT8). (b)
`project.cpp:19-30` → `return row_count_table(input.table->num_rows())`. (c) `scan.cpp`: when
the projection is empty (the writer sends the zero-field schema and no projection,
`node_writer.rs:80-100`), read no columns and return `row_count_table(n)`, `n = min(Σ num_rows,
limit)` where `limit > 0`, else the sum — `num_rows` per named row group from
`cudf::io::read_parquet_metadata(source_info{paths}).rowgroup_metadata()`
(`cudf/io/parquet_metadata.hpp:184,250,270` on 25.02). The early return skips fix 13's
`cudf::slice` cap, so it honours the limit itself (`SELECT 1 FROM lineitem LIMIT 10` plans a
zero-column scan with `limit=10`). (d) `join.cpp`: `cross_product(left, right)` — refuse a side
with zero columns by name; one overflow check above every arm; both rows-only →
`row_count_table(l*r)`; left rows-only → `cudf::tile(right, l)`; right rows-only →
`cudf::repeat(left, r)`; else `cudf::cross_join` (same row order: `cross_join.cu:59,62`). Leave
`execute_nested_loop_join`'s arm (every `GpuNestedLoopJoin` in the goldens carries a
`filter=`). CPU and GPU: the CPU's zero-column `RecordBatch` and the device's rows-only table
answer the same rows. Frozen surface: none; no golden moves. Tests, gtests in
`test_plan_executor.cpp`: zero-column build → 125 rows in tile order, the mirror in repeat
order, two zero-column sides + `count` → 25, a `projections=[]` scan with and without a
`limit`. Registry row 10 → `163`; `architecture.md` `CudfProject`/`CudfCrossJoin` rows gain the
placeholder sentence. Order: after fix 13 for the nested_limits cells; the gtests share
`test_plan_executor.cpp` with task 11's helper repair (`:49-66`) — after task 13. Cells:
nested_limits gpu × 5 with fixes 13 and 10; q9 stays on #163 (fix 17). ~60 C++ lines in four
files, ~120 lines of gtest, one shad-gpu `ctest -L gpu`; M if the corpus line below is added.

**Minimum corpus query.**

```sql
SELECT CASE WHEN (SELECT count(*) FROM nation WHERE n_regionkey >= 0) >= 0
            THEN (SELECT max(n_nationkey) FROM nation WHERE n_regionkey >= 0) ELSE 0 END AS pick
FROM region WHERE r_regionkey = 1
```

Silently wrong today: CPU 24, device 25 at all five modes (THEN reads the shifted `count(*)`).
The `WHERE` in each subquery is load-bearing: over a bare table `AggregateStatistics` answers
as a `PlaceholderRowExec`, which the planner refuses (#158). The CPU answers at tp1-single,
tp1-rowgroup, tp4-rowgroup and tp4-sized today and at tp4-single only after fix 3 (the keyless
count over `nation` maps `[[[0]],[],[],[]]` there and the merge-only lanes hit #180). Not in
testdata; recommended as `tpch/scalar-subqueries` (16 goldens) — added before fix 3, its
tp4-single cpu cell carries `180`. The scan arm: `tpch/nested_limits` on a device after fix 13
(in testdata).

### 5. `date_part` casts to the declared return type — S

**Closes:** #191. Re-attributed: tpch q7, q9 (#183 → #191 once fix 8 lands, → #185 once this
has).

**Issue.** `cpp/src/expr.cpp:659-660` returns `cudf::datetime::extract_datetime_component` as
is; cuDF answers every component in `int16_t`; the unload refuses "expected Int32 but found
Int16". The wire carries the declared type — `ScalarFunctionExprNode.return_type`
(`gpu_plan.fbs:198-214`, written at `wire/expr_writer.rs:150-179`) — and the only C++ reader is
`infer_expr_type` (`:395-397`) for AST routing; the `date_part` arm never reads it.
`is_ast_able` refuses every scalar function (`:403-406`), so the column path is the only path.
Off: tpch/q8 gpu tp1-single (registry row 108 `152 191`).

**Localized fix.** In the `date_part` arm: `cudf::cast(component->view(),
cudf::data_type{fb_to_type_id(sf->return_type())})` — two lines, unconditional (DataFusion 45
types every field but `epoch` as `Int32`, `date_part.rs:142-155`; `epoch` is refused at
`:658`). The comment cites the `CastExprNode` arm (`:912-935`) as precedent. CPU and GPU: the
device honours the declared type the CPU relays from DataFusion. Frozen surface: none; no
golden moves. Test in `test_gpu_executors/exec.rs`: a `GpuProject` of `date_part('YEAR',
Date32(9204))` through `one_node`, asserting `Int32` and 1995 × 6; red today. Reword
`test_inc2_conformance.rs:175-178`'s doc (it names `extract_year` as why an INT16 key exists).
`architecture.md`: two table rows (`## cuDF options`, the flat-buffers table).
`build-test.md:21` 31 → 32. Order: independent in code; cell accounting after fix 8.
`declared-schemas.md:219` (task 10) and `operator-cases.md` (task 9) expect this as a `bug_`
test — whichever lands second deletes it. Task 4 moves both test files this edits and task 11
edits `expr.cpp` — before task 4 or after task 13. Cells: none from the existing corpus — q8
tp1-single sheds `191` and stops on #185 (fix 10); its other four modes stay on #152 (fix 18).
Adding the query below as `tpch/extract-year.sql` is the only green device cell this fix
produces (5 cpu + 5 gpu). Two C++ lines, one ~30-line device test, one shad-gpu run; M if the
corpus query is added.

**Minimum corpus query.**

```sql
SELECT extract(year FROM o_orderdate) AS o_year FROM orders WHERE o_orderkey < 8
```

All five modes, gpu; scan → filter → project → unload, refused at the unload today. Not in
testdata.

### 6. `round(x, p > 0)` on the device is three IEEE operations, like the oracle — S

**Closes:** #60 (misdiagnosed: no anti join, no memory; a one-ulp split on `round(float, p >
0)`). Files ticket 2, the DESC-NULLs defect found beside it.

**Issue.** `cpp/src/expr.cpp:707-722` calls `cudf::round(fcol, places, HALF_UP)` for any
`places`. cuDF's kernel is `modf`, then `ip + round(fp·10^p)/10^p` (`round.cu:108-117`);
DataFusion's is `(x·10^p).round()/10^p` (`round.rs:169-170`). Two roundings against one: they
differ by one ulp on about one value in twenty at or above 1.0 (`fl(0.78)` puts `2 + fl(0.78)`
on a midpoint of the result grid and ties-to-even takes the upper double). Re-emulated over
q78's committed answer, exactly five rows split, which `ryu` renders as `2.7800000000000002`
under `golden_exact`. q2 takes the same kernel seven times per row. Keeps off nothing on its
own — q78 and q2 refuse on #152 first, then #187.

**Localized fix.** `expr.cpp:722`: `places == 0` → `cudf::round(·, 0, HALF_UP)`; otherwise
multiply by a FLOAT64 scalar `10^|p|` (exact for p ≤ 22; `1/10^|p|` for negative p),
`cudf::round(·, 0, HALF_UP)`, divide — bit-identical to the oracle; the comment at `:703-706`
names which kernel agrees. The CPU relays `round` to DataFusion
(`cpu_backend/expr_physical.rs:110-121`), untouched. Frozen surface: none; no golden moves.
Test: `test_gpu_recipe_walk.rs` const `ROUND_PLACES` (the query below) and
`round_with_places_lands_on_the_oracles_double` at `ONE_LANE` — red today on two rows, 23
equal; the coverage list. Also lands `bug_a_desc_key_puts_its_nulls_last` in
`test_gpu_executors/exec.rs` beside `:80-124` with ticket 2's number above it. #60's body
rewritten naming q78 and q2; registry row 79 `60 97 152` → `152`; `build-test.md` walk 10 → 11;
`scripts/exec_model/operators/expressions.py:237` docstring. Order: task 11 edits `expr.cpp`
(different lines); task 13 rewrites the walk's `PROVEN`/coverage list — before task 4 or after
task 13. The DESC-NULLs fix is its own S change at two sites (ticket 2). Cells: none; q78 stays
on #152 then #187. After both, its `gpu_tp1_single` under `golden_exact` must differ in exactly
the five `ratio` cells before this and in nothing after. Twelve C++ lines, one walk case, one
shad-gpu walk run.

**Minimum corpus query.**

```sql
SELECT n_nationkey, round((CAST(n_nationkey AS DOUBLE) * 25.0) / 9.0, 2) AS r FROM nation
```

tpch sf1, every mode, both backends; the device via the walk; two rows differ today. Not in
testdata.

### 7. Value-form CASE takes the column path — S

**Closes:** #57 (misdiagnosed: the recorded wrong answer came from a gtest whose Int64 literals
were all built as 0 — `CreateScalarValue`'s positional `is_null` — and the lowering had already
run q39 correctly at 8f471cc0). Drops the "#57" clause from `plan/mod.rs:52-54` (nothing
refuses it).

**Issue.** `build_column_case` throws `value-form CASE not supported in column path` whenever
`c->expr()` is set (`cpp/src/expr.cpp:773-780`); the search-form fold at `:783-796` works. The
form reaches the device: q39's plan carries it four times per mode. A guard added on a false
measurement, then kept. Keeps off nothing today (q39 sits on #163; registry row 40 `57 163`).

**Localized fix.** `expr.cpp` `build_column_case`: delete the throw; materialise the comparand
once; each WHEN becomes `binary_operation(comparand, when, EQUAL, BOOL8)` (scalar fast path for
a literal WHEN, mirroring `:609-626`); the fold unchanged — ~20 lines; NULL never matches via
`copy_if_else`'s null-as-false, first wins. The CPU relays to DataFusion. Frozen surface: none;
no golden moves. Test, after task 11 lands (it rebuilds
`make_int64_literal`/`make_float64_literal` and adds `make_null_literal` at
`test_plan_executor.cpp:49-66`): `CASE (CASE WHEN r_regionkey = 3 THEN NULL ELSE
CAST(r_regionkey AS Int64) END) WHEN 0 THEN 100 WHEN 2 THEN NULL ELSE -1 END` → `[100, -1,
NULL, -1, -1]`, `null_count() == 1`. Exec model: `Case` gains `comparand`, each arm
`Binary("==", comparand, when)` feeding the `fillna(False)` fold (`expressions.py:363-366`);
one model test with a null comparand. Registry row 40 → `163`; #57 archived naming the
zeroed-literal misdiagnosis; `build-test.md` C++ 27 → 28, Python +1. Order: after task 11 — the
helper repair is that task's Task 1 and this adds one test; `operator-cases.md:29`
`a_value_case_agrees` (task 9) lands green after this. Cells: none. q39 cpu × 5 on #163 (fix
17); gpu then #185 at tp1-single (fix 10), #152 elsewhere (fix 18). ~20 C++ lines, one ~50-line
gtest, ~10 Python lines, one shad-gpu `peacock_plan_executor_tests` run.

**Minimum corpus query.**

```sql
SELECT d_date_sk, CASE (CASE WHEN d_moy = 12 THEN NULL ELSE d_moy END) WHEN 2 THEN NULL ELSE d_moy END AS moy
FROM date_dim WHERE d_year = 2001 AND CASE d_moy WHEN 1 THEN 0 ELSE d_moy END > 1
```

334 rows; strict minimum `SELECT CASE d_moy WHEN 1 THEN 0 ELSE d_moy END FROM date_dim`. gpu,
any mode; the device fails the first `CudfProject`/`CudfFilter` with the throw. Not in
testdata.

### 8. Strings are declared `Utf8` because that is what both readers read — M

**Closes:** #183 (misdiagnosed: the plan inherits `Utf8View` from a DataFusion parquet-reader
option this engine never runs, not from a type rule) and #192 (misdiagnosed: arrow 54's
`Utf8View` `take`/`concat` kernels multiply buffer-reference lists along q64's seventeen-join
build chain — Π⌈R/8192⌉ references per string column, 24 bytes each; those columns exist only
because of the option). Re-attributed by reading: 29 of #183's 60 rows → #187 (D1); tpch q7, q9
→ #191 (fix 5), or #185 (fix 10) once fix 5 has landed.

**Issue.** `GpuExport::unload` concatenates decoded IPC batches against the sink's declared
schema (`gpu_backend/mod.rs:179-183`) and refuses "expected Utf8View but found Utf8".
`lib.rs:52` registers tables through `ParquetFormat::default()`, whose
`schema_force_view_types` defaults to `true` (`datafusion-common-45/src/config.rs:430`), so
`infer_schema` runs `transform_schema_to_view` and every string field is declared `Utf8View`.
The CPU reads `Utf8` and hides it with a per-batch cast (`cpu_backend/source.rs:112-139`
`as_declared`); cuDF has one string type (`expr.cpp:87-89`) and exports `Utf8`
(`gpu_executor.cpp:74`). Off: 60 registry rows' gpu cells (tp1-single wherever #152 does not
refuse first; all five modes for the ten rows without `152`); q64 cpu × 5, killed at 13 GB RSS
(registry row 65; `corpus_cases.inc:268`).

**Localized fix.** `lib.rs::build_session_state`:
`config.options_mut().execution.parquet.schema_force_view_types = false`; `read_table` builds
`ParquetFormat::default().with_options(ctx.state().default_table_options().parquet)` so the
rule reaches `register_parquet` fixtures. Scaffolding removed: `as_declared`'s cast arm becomes
a refusal (drop the `cast` import at `source.rs:12`); the `Utf8View`/`BinaryView` arms of
`spark_partitioning.rs:53-71 hash_keys` go. `result_text.rs:92 schema_digest` stays names-only
(the walk exports raw through `peacock_result_from_handle`; a typed digest reddens five walk
tests, and D1 does not change that). Not the rejected approach: `casts.md` predicted and cast
at the unload; this removes the divergence at its declaration and casts nowhere. CPU and GPU:
both read `Utf8`, the plan declares `Utf8`. Frozen surface: none (same fb enum, different
value). Test in `planner/translator/schema_tests.rs` (declared source types == the reader's
file types); four assertions flip `Utf8View → Utf8`. Goldens: all ten `.plans.txt`,
`recipe-payloads.txt` bytes and digests under both regen variables; execution, cost and result
goldens expected byte-identical, with one thing to read — `src/common.rs:107-119` prices `Utf8`
content as `offsets[rows] - offsets[0]` and `Utf8View` as Σ over valid values only, which
differ where a `Utf8` array's null slots carry bytes (`nullif`, a slice), so read the regen
diff for string columns with NULLs first. Registry: 31 rows re-ticketed by reading, 29 need a
run; `build-test.md:44`. #192's half: after this and fix 10 land, measure peak RSS and wall
time per mode on a 15 GiB host, then `corpus_cases.inc:268` q64 → five cpu modes, registry row
65 cpu → enabled with gpu `152 183 185 187`, eleven sections authored, #192 archived naming
DataFusion 45 / arrow 54. Caveat: later DataFusion versions add a parser option typing VARCHAR
and literals as `Utf8View`; an upgrade under #23 must switch it off too. Order: independent in
code of fix 10 and D1; q64's re-enable after the measured run. Collides on the ten `.plans.txt`
with task 10 — after task 13; if task 10 has landed, delete
`bug_a_declared_utf8view_is_exported_as_utf8`. S code (two production lines, ~40 lines of
scaffolding removal, one test); M for eleven golden regenerations, a measured q64 run and a
29-row shad-gpu rollout in about six batches. Cells: cpu q64 × 5 if the run fits. gpu alone:
tpch/shuffle_stddev, nested_loop_join × 5. With fix 10: cross_join, nested_loop_left_join, q4 ×
5 each, q15 tp1 × 2. With D1 as well: aggregate_groupby, shuffle_additive × 5 each; anti_join
and semi_join × 1 each, tp1-single only — at the other four modes the merged `orders` lanes
scatter several probe batches per lane (`tpch.sf1/tp4-single-mini.cpu.txt` `== anti-join`) and
the RightAnti/RightSemi call copies its build per probe batch (`gpu_backend/join.rs:304-314`) —
#152, fix 18; the registry row's `183`-only attribution is incomplete.

**Minimum corpus query.**

```sql
SELECT r_name FROM region
```

tp1-single, gpu — `GpuUnload` over one loader, refused at the unload. The committed stand-in is
`tpch/cross_join` (off at all five on `183` alone). For #192: q64 at tp1-single, cpu (in
testdata).

### 9. Two session rules let DataFusion 45 plan q27 and q72 — M

**Closes:** #23 for q27 and q72 without an upgrade. q70/q86 are `rank() OVER` window queries
the translator refuses on any DataFusion (`nodes.rs:159-163`, #143 archived) — never runnable;
#23's "unblock q70/q86" and #65's "(q70/q86 after #23)" are corrected. #23 is rewritten to name
the two rules as the scaffolding an upgrade removes at most one of. The two rules share a
ticket number and `build_session_state` and nothing else.

**Issue.** Four rows `plan_status=fail`, all fifteen cells `na` (registry 28, 71, 73, 87). q27:
`SanityCheckPlan` refuses a `SortPreservingMergeExec` over a `UnionExec` with `Child-0 order:
[]` (`tpcds.sf1/tp1-single.plans.txt:2633`) — `try_add_ordering`'s `constants.is_empty() &&
ordering_satisfy(..)` guard (`datafusion-physical-expr-45/src/equivalence/properties.rs:2261`)
skips the direct check once any branch contributed a constant, and a branch whose sort keys are
all constant adds no ordering, so the union's ordering drops to `[]`. q72: `Cannot coerce
arithmetic expression Date32 + Int64` (`:7431`) — `TypeCoercion` defers to arrow's
`add_wrapping`, whose date arm has no integer case (`arrow-arith/src/numeric.rs:645-700`); no
DataFusion or arrow version read adds one, so "46+ fixes it" is unsupported for q72.

**Localized fix.** On the one session every planning site builds
(`lib.rs::build_session_state`): (1) `planner::MergeInputSort`, a `PhysicalOptimizerRule`
inserted before `SanityCheckPlan` (`with_physical_optimizer_rules` on
`base.state().physical_optimizers().to_vec()`; `with_physical_optimizer_rule` appends after the
gate and is wrong): under a `SortPreservingMergeExec` whose input does not satisfy its
ordering, add a `SortExec` with the merge's fetch and `preserve_partitioning`; inert wherever
the sanity check passes. The translator then emits `GpuMergeSortedPartitions → GpuSort →
GpuUnion` with no change. (2) `planner::DateDayArithmetic`, a `FunctionRewrite` before
`TypeCoercion`: `Date32 ± integer` → `CAST(CAST(d AS Int32) ± CAST(n AS Int32) AS Date32)` —
days, DuckDB's meaning; every piece crosses the wire and `expr.cpp:912-935` routes the cast.
CPU and GPU: both run the plan DataFusion now produces. Frozen surface: none. Tests: two `bug_`
tests on a plain session (a two-branch union with a selected constant; `SELECT DATE
'2000-01-01' + 5`) that go red the day DataFusion no longer needs the rule; two positive
planner tests; two e2e cases (`sum(n_nationkey)` per branch; `sum(l_quantity)` over
`l_receiptdate > l_shipdate + 5`). Goldens: q27 and q72 sections in the five tpcds `.plans.txt`
(refusal → tree); `recipe-payloads.txt` unchanged; execution sections for every cpu mode the
two are enabled at. Registry rows 28/73 plan cells → enabled; q27 cpu on `163` until fix 17,
gpu `152 183`; q72 cpu enabled where the run passes, gpu `152 183`; rows 71/73 drop archived
`97`. `architecture.md` Planning gains one clause naming the two rules. Order: fix 8's option
must stay off across any upgrade; the q72 rewrite avoids intervals because of fix 16; #166 is
the one concrete reason for an upgrade this fix does not supply. After fix 17 for q27's cells;
q72's sections after fix 10's regen; plan goldens collide with task 10 — after task 13. S code
(two ~40-line rule files, ~10 lines in `lib.rs`, six tests); M task — ten plan sections and a
measured q72 run whose oracle may not fit CI. Cells: plan cells q27 × 5, q72 × 5, and ten cpu
cells out of the `na` pool. q27 cpu × 5 once fix 17 has landed (this repo's q27 is a `UNION
ALL` of three `GROUP BY`s over one CTE, not a `ROLLUP`, so no grouping id; its third branch is
a keyless `avg` over the join — fix 3's shape at tp4). q72 cpu: decided by a run — its
FROM-order first join (`catalog_sales ⋈ inventory`, #20) is paid by the oracle too, and a 15
GiB runner may not fit the tp1 modes (fallback: a #192-style disabled line); the tp4 modes have
no known wall (not #180: its counts are grouped). gpu on #152/#183.

**Minimum corpus query.**

```sql
-- q27's defect (g must be selected)
SELECT r, n, g FROM (SELECT n_regionkey r, n_nationkey n, 0 g FROM nation
                     UNION ALL SELECT NULL, NULL, 1 FROM nation) t ORDER BY r, n LIMIT 10;
-- q72's defect
SELECT count(*) FROM lineitem WHERE l_receiptdate > l_shipdate + 5;
```

Both refused by DataFusion before the engine's planner, every mode, both backends; neither in
testdata.

### 10. The CPU join returns one batch per call, as every other executor does — M

**Closes:** #185 under its corrected title ("a CPU join returns a probe call's output as
DataFusion chunked it; the device returns one table"). Re-attributes to #185: the "batch-split
wall" named in the #152, #187, #191, #190, #46, #47 and #188 records; q11 tp1-single (fix 1),
q8 tp1-single (fix 5), q61/q77/q78 tp1-single after D1, and every #183/#187 row with a join.

**Issue.** `executor/cpu_backend/join.rs:236-250` `probe_and_fetch` and `:254-265`
`finish_and_fetch` return `declared(...)` (`:269-273`), one `CpuBatch` per `RecordBatch`
DataFusion's stream yielded. DataFusion 45 bounds a hash join's matched pairs per chunk at
`batch_size` = 8192 (`joins/hash_join.rs:1472-1479`) and appends unmatched rows for outer
types; `CrossJoinExec` emits one batch per build row. The device answers one table per call
(`gpu_backend/join.rs:164-179`, `:185-198`). The device tier compares the CPU-authored section
byte for byte (`tests/common/corpus_gpu.rs:101-105`) and panics before `assert_result`, so no
device join cell can pass and no result of one was ever compared. The CPU join violates three
`architecture.md` invariants (one batch per call per lane; self-bounding queues; batch
boundaries a pure function of the plan) that `CpuExec::exec` (`cpu_backend/mod.rs:173-201`)
keeps. `in_rows` is the driver's Σ over popped batches and is right on both engines; the merge
#185's title blamed was never wrong. Evidence, `tpcds.sf1/tp1-single-mini.cpu.txt:4927+` (`==
q93`): the `Right` join takes one probe batch (`in_rows=[[287867],[2880404]]`) and emits 36
batches; the per-batch `GpuAggregate` emits 36 partials totalling 7486; on the device one call
→ one table → 7169 groups. Same shape at tpch `hash-join` (733 chunks of 8192 over one probe),
`cross-join`, q4. Off on this alone: nine gpu tp1-single cells (tpcds q38 q48 q87 q88 q93 q96;
tpch q3 q14 join_int; registry rows 39 49 88 89 94 97 103 114 128); behind every other device
join cell.

**Localized fix.** `cpu_backend/join.rs` only. `declared(batches, schema) -> Vec<CpuBatch>`
becomes `one_declared(batches, schema) -> CpuBatch`: each chunk through `declared_as`, then
`concat_batches` (zero chunks → `RecordBatch::new_empty(schema)`); used at `:250` and `:265`;
the early returns for `per_call: None` and `finish: None` stay; the doc says "whatever
DataFusion chunked", never "8192". Not touched: the driver, the GPU backend,
`GpuAggregateBatches`, the `Vec` return type on the trait, `without_build`. CPU and GPU: the
same batch list at every join, at every mode. Frozen surface: none. Test in
`cpu_backend/tests/join.rs` with `SessionConfig::new().with_batch_size(1)`: Inner → one batch;
cross join → one where DataFusion yields three; LeftAnti finish → one. Goldens: all ten
`<mode>-mini.cpu.txt` and ten `.cost.txt` regenerated (dataset host, `UPDATE_CANONICAL=1`),
read for two invariants — `output_rows` unchanged except at partial aggregates above joins
(q93: 7486 → 7169) and merges' `in_rows`; `mini.result.txt` read per query rather than expected
unchanged (a `Float64` sum above a formerly chunked join may move low bits; no sort preserves
tie order, `architecture.md:715-720`); `nested-limits` loses `abandoned=[92]`, so
`test_golden_format` gains a case for the parser arm (`golden_text.rs:277`); every `.cost.txt`
figure falls slightly and the gate is `new > old`. Memory: `concat_batches` is a 2× transient —
tpch `hash-join` at tp1-single is 194 MB, so ~390 MB; fine on a 15 GiB host, and why q64's fit
(fix 8) is measured after this lands. Registry: nine rows drop `185`, `gpu_tp1_single` enabled
only after a shad-gpu run; `corpus_cases.inc` join comments; `build-test.md:44`; #185 archived
with the corrected diagnosis; `architecture.md` gains no sentence — three become true. Order:
first of every device registry pass; before fixes 1, 2, 3, 9 and 17 author new cpu sections
(one regen). Same file as task 12's `without_build` edit (`:185`, a different function) and six
of its ten `.cpu.txt` — a rebase, not a design conflict, but why this lands before task 4 or
after task 13. Code S (one file, ~15 lines, one ~50-line test); the ten-file cpu regen and a
never-compared device run make it M. Cells: 9 gpu tp1-single candidates, confirmed only by a
run (a cell failing past the join gets a fresh ticket); the gate for every other device join
cell.

**Minimum corpus query.**

```sql
SELECT count(*) FROM supplier s JOIN nation n ON s.s_nationkey = n.n_nationkey
```

tpch sf1, any mode, both backends. Today CPU: `GpuHashJoin batch_rows=[[8192,1808]]`,
`GpuAggregate [[1,1]]`, `GpuAggregateBatches in_rows=[[2]]`; device `[[10000]]`, `[[1]]`,
`[[1]]`. After: both read the device's. Not in testdata (nearest: `tpch/join_int`, off on `152
185`).

### 11. `count(DISTINCT x)` with any companion lowers to two stages, the outer running merge aggregators over the inner's state — M

**Closes:** #62 (misdiagnosed in location: a plan-time refusal at
`translator/aggregate.rs:118-124`; no backend ignores a flag — the wire never sets `distinct`,
`aggregate_writer.rs:177`, and the C++ guard `aggregate.cpp:143-155` is unreachable; keep the
guard, fix its comment — `reports/hacks-audit.md:220`). Files ticket 11 and makes #144's
message arrive.

**Issue.** DataFusion removes DISTINCT only via `SingleDistinctToGroupBy`, whose precondition
is that every companion is sum/min/max; q28's `avg(x), count(x), count(DISTINCT x)` leaves the
flag on and `:119` refuses. q28 is out of the corpus entirely (registry row 29 `plan_status`
cells `disabled`; five `.plans.txt` carry the `(#62)` refusal; pinned by
`test_planner_join_refusals.rs:104-117`). The IR has no distinct aggregator by design
(`architecture.md:297-323`): a distinct argument becomes a group key of an inner aggregate. The
lowering is DataFusion's today, and where it declines nobody lowers. This engine separates init
from merge (`count` merges by `sum`), so the outer stage can run each companion's merge
aggregators over the inner's state. A kernel fix (`nunique`) cannot work: per-batch `nunique`
has no merge.

**Localized fix.** One file, `translator/aggregate.rs`. (a) `decompose` takes
`(Arc<AggregateFunctionExpr>, InitFrom)` with `enum InitFrom { Values, State(&AggStateColumns)
}`; under `State` the declared fields are the input schema's at `cols.positions`, paired
positionally with `rule.state` (not through `declared_state`, `:71-89`, whose tag lookup does
not match the `avg(…)$sum` names); init calls become the merge funcs over those columns
(`Merge::Combined` → error); nullability `true` for every `State` column (fix 3's rule). (b)
Split `aggregate_sequence` (`:242-388`) into `Stage` + `sequence(input, stage, shuffle)`, the
shortcut generalized to "one lane and a single batch" (no committed golden has a lanes=1
merge-only node, so nothing moves). (c) `distinct_argument`: refuse grouping sets,
multi-argument, two different distinct arguments (#144), a Welford companion. (d)
`distinct_stages`: inner groups on `G ++ [d]` (reuse the ordinal if `d` is already a key),
companions' inits, no finalize; outer groups on `G`, companions as `State`, the distinct one as
`Values` over a non-distinct twin built with `AggregateExprBuilder`; inner takes DataFusion's
shuffle, outer `Shuffle::None` (valid by the subset rule, `plan/aggregate.rs:236-258`). A
`Count` companion under `State` finalizes as `CASE WHEN o IS NULL THEN 0 ELSE o END`. CPU and
GPU: both run the same two-stage plan; no backend change. Frozen surface: none
(`recipe-payloads.txt` does not move; five tpcds `.plans.txt` q28 sections go from refusal to
tree). Tests: `test_planner_join_refusals.rs:104-117` replaced by the three new refusals;
translator tests (grouped and keyless at four lanes via `translated_at_tp4(sql, 0)`, `d`
already a key, the `avg` companion); e2e cases with `count` companions over a NULL-bearing
column plus the empty-filter keyless case; one walk case at `TWO_LANES`. Registry row 29 plan
cells enabled, cpu/gpu disabled on `163 152 187`, `62` dropped from rows 17, 95, 96, 116;
`corpus_cases.inc` q28 at `none, none`; comments at `plan/mod.rs:52-53`,
`aggregate_writer.rs:149-158`, `aggregate.cpp:143-149`; `architecture.md:311-315`;
`build-test.md:42`. Order: after fix 17 — same fifteen lines of `decompose` (`:148-157`), and
fix 17's `arg_type` from `aggregate.expressions()` is wrong under `State` (the argument type is
the inner's declared type at `cols.positions[i]`). Plan goldens collide with task 10 and the
walk case with task 13 — after task 13. The exec model already models this lowering
(`test_end_to_end.py:383`). One production file, ~+190/−15, seven tests, one device case, five
plan goldens one section each; the `sequence` split goes through the function every corpus
aggregate uses and is proven by an unchanged golden set. Cells: q28's five plan cells now; q28
cpu × 5 after fix 17 (the `avg`); gpu × 5 behind #152, #187, #163.

**Minimum corpus query.**

```sql
SELECT l_returnflag, count(l_quantity), count(DISTINCT l_quantity)
FROM lineitem WHERE l_shipdate < DATE '1992-02-01' GROUP BY l_returnflag;
-- the keyless form, and the empty-filter case: 0, 0
SELECT count(ss_customer_sk), count(DISTINCT ss_customer_sk) FROM store_sales WHERE ss_quantity < 0;
```

All five modes, both backends; refused by `planner::plan` at `:119` today. Not in testdata.

### 12. A decimal shuffle key hashes as comet does, by declared precision — M

**Closes:** #184 (misdiagnosed: the shuffle key is a decimal and the kernel has no decimal arm
— #95 reached by a corpus query; the 1→N shape is routine) and #95 together. The fbs shape is
D2.

**Issue.** `spark_hash_partition.cu:163-185`'s key-type switch lists STRING/INT32/INT64 (after
normalising INT8/16, TIMESTAMP_DAYS, DICTIONARY32); `default:` is the `CUDF_FAIL` at `:179`.
q15's `#11 GpuEmitPartitions: hash=[total_revenue@4]` is over `Decimal128(38,4)`
(`tpch.sf1/tp4-rowgroup.plans.txt:795`). comet dispatches by declared precision — `p ≤ 18`
hashes the unscaled value as 8 LE bytes, wider as 16
(`datafusion-comet-spark-expr-0.6.0/src/hash_funcs/utils.rs:107-146, 299-304`); the CPU passes
`Decimal128` arrays straight to comet. cuDF's `data_type` is `{id, scale}` with no precision,
and the repartition arm reads ordinals only (`node_session.cpp:388-402`). Off: q15 gpu at the
three tp4 modes (the tp1 plans have no emit — those cells are #183 alone). Latent for seven
more decimal-keyed shuffles (tpch q10 q18 q2; tpcds q24 q37 q82 at p ≤ 18, q75 at (31,15)).

**Localized fix.** Carry per-key precision to the kernel. Recommended: append
`hash_key_precisions: [uint8]` to `CudfRepartition` in `gpu_plan.fbs` (parallel to
`hash_exprs`; 0 for a non-decimal key), written by `node_writer.rs` from the emit node's
declared schema, read by the repartition arm, which also checks each decimal column's scale
equals the declared scale (a scale drift would misplace silently). C++: `partitioning.hpp`
gains `struct HashKey { column; decimal_precision }`, both entry points take `vector<HashKey>`;
a `spark_hash_decimal128_col_kernel` hashing `width` LE bytes of the `__int128_t` (8 if p ≤ 18,
else 16); `gpu_executor.cpp`'s conformance hook derives precision from the Arrow C-data
`format` string (`d:P,S`), so the ABI signature is unchanged. The proposal wrote
`PlanNode.output_schema` for the emit node instead — the mechanism `wire-schema.md` was
rejected for; the field above carries exactly the fact the kernel cannot compute. CPU and GPU:
the same murmur3 bytes per key on both. Frozen surface: fbs append (no ABI symbol); the public
C++ header `partitioning.hpp` changes signature (three in-repo callers). Tests, red before:
three `*_match_comet_live` gates in `test_inc2_conformance.rs` ((15,2), (38,4), composite); one
placement test through the wire in `test_gpu_executors/join.rs` (a recipe-built scatter on a
cast decimal key, per-lane rows against `create_murmur3_hashes` — a multiset compare cannot see
a wrong width); the walk query on `l_discount` in `PROVEN`; one gtest. Goldens:
`recipe-payloads.txt` bytes and digests for every shuffle query (both regen variables); no
plan, execution, cost or result golden. Registry row 115 `183 184` → `183`;
`corpus_cases.inc:26-32`; `build-test.md:44` and count rows (murmur3 10 → 13, cuDF smoke 5 → 6,
walk +1, executors +1); #184 and #95 archived; `tickets.md:775-776`'s #195 bullet;
`architecture.md:824, 960-972` and the flat-buffers table. Order: fix 2 (`UInt8` refused by the
same switch's CPU neighbour) is separate. Task 4 moves both test files; task 9 plans this
scatter as a `bug_` test; `declared-schemas.md` §4 defers "schemas on the wire" and the
`schema_text` drift (`fb_text::schema_text:221-233` prints a bare enum for a decimal field —
fix it here if `output_schema` is chosen). After task 13. ~10 files, ~200 LOC in two languages,
three shad-gpu binaries plus the payload regen on the symlinked host. Cells: q15 × tp4 (3 gpu)
with fixes 8 and 10 — at every tp4 mode the lower join's probe is one batch per lane and the
top join's probe is the one-row `max` (`tpch.sf1/tp4-single-mini.cpu.txt` `== q15`,
`batch_rows=[[],[1],[],[]]`), so #152 is not on the path, and the sink is strings plus
`Decimal128(38,4)`; a run decides. Latently unblocks the seven rows above.

**Minimum corpus query.**

```sql
-- 8-byte path, Decimal128(15,2)
SELECT l_discount, sum(l_quantity) FROM lineitem GROUP BY l_discount;
-- 16-byte path; a subquery, because a computed key is refused at the Partial aggregate (aggregate.cpp:162)
SELECT x, sum(l_quantity) FROM (SELECT l_extendedprice * l_discount AS x, l_quantity FROM lineitem) GROUP BY x;
```

tp4 modes, gpu; both plan, run on the CPU, and die at the first `CudfRepartition` call with
`type_id=27`. Neither in testdata; q15 at tp4 is the committed carrier (16-byte path).

### 13. A limited scan is one lane and one batch over the row groups that cover the limit; each reader bounds that one call — M

**Closes:** #186 and #188 as one defect. Closes `reports/hacks-audit.md:49` (production bug 2)
by noting its unreachability — a `Some(0)` scan limit never exists, `EliminateLimit` plans
`LIMIT 0` as an empty relation; one line at `node_writer.rs:93`. Makes
`architecture.md:374-379` true (false at the rowgroup modes today). Exposes ticket 1 (fix 4)
for nested_limits.

**Issue.** `CpuSource::new` never reads `node.limit` (`cpu_backend/source.rs:37-77`);
`read_next` builds the reader with `with_row_groups` + `with_projection` only (`:86-91`), so at
tp1 `SELECT * FROM lineitem LIMIT 10` returns 6,001,215 rows; at tp4 the unload's interval
hides it but the loader still reads everything. On the device `scan.cpp:61-63` sets
`set_num_rows(limit)` and `:77-78` sets the row groups every `execute_scan_rowgroups` call
names; cuDF's `set_row_groups` throws "row_groups can't be set along with skip_rows and
num_rows" (`cudf/cpp/src/io/functions.cpp:805-811`, same on 25.02/26.02) — the first scan call
of any limited plan fails. Two halves of one design unwritten: `architecture.md:374-379` says a
limited scan plans one lane and one batch; `lanes_for` (`nodes.rs:374-390`) returns one lane
but `source()` (`:392-411`) hands the mode's batching to `partition`, so the rowgroup modes map
49 batches, and `plan/source.rs:21-50` validates nothing about a limit; parquet 54's
`with_limit` composes with `with_row_groups` and nothing calls it. Off: tpch/scan_limit cpu ×
tp1-single, tp1-rowgroup (registry row 135 `186 188`); scan_limit gpu × 5; nested_limits gpu ×
5 (row 131). The planner-one-batch design is chosen over a per-lane counter: it is the rule the
page states, it has one owner (the counter adds a `remaining` on both loaders, a `slice_handle`
recipe arm and a payload section), and the prefix mapping bounds the decode at every mode,
where the counter still decodes all 49 lineitem row groups at the three `Off`-batching modes.

**Localized fix.** (a) `translator/nodes.rs::source`: `let batching = batching_for_source(t);`
called unconditionally (it advances `next_source`); then `match config.limit { Some(limit) =>
(1, Batching::Off, prefix), None => (lanes_for(t, &scan.groups), batching, &scan.groups) }`,
where `prefix` is the shortest prefix of `scan.groups` (survivor order) whose cumulative `rows`
≥ `limit`; `lanes_for` loses its `limit` arm (`:375-381`). (b)
`plan/source.rs::validate_schemas_and_partitions`: a limited scan whose mapping is not one lane
and one batch is `PlanError::Invalid` — checked on every tree `planner::plan` sees
(`pipeline.rs:37,73`), and the injection's `drained` leaves a one-lane mapping alone. (c)
`cpu_backend/source.rs`: `CpuSource` gains `limit`; `read_next` adds
`builder.with_limit(limit)` before `build`. (d) `cpp/src/operators/scan.cpp`: delete `:61-63`;
after `read_parquet`, before the decimal widening, `if (limit > 0 && rows > limit)` slice the
view to `{0, limit}`; `NodeStats` are computed by the callers from the returned view
(`node_session.cpp:238-242, 502-505`), so the capped batch prices exactly. No recipe change, no
`slice_handle`, no counter; the driver and `GpuLimit` untouched. CPU and GPU: the plan
constrains a limited scan to one call; each reader bounds that call natively; the same rows and
the same one batch on both. Frozen surface: none. Tests: source, validator and translator cases
(`customer`: `LIMIT 122883` maps `[[[0,1]]]`, `LIMIT 3` maps `[[[0]]]`), a device executor
twin, gtest `ALimitCapsWhatOneCallReturns` (`test_plan_executor.cpp:1094`), a `test_gpu_abi`
case; `a_limit_slices_at_most_two_batches_and_stops_the_scan`
(`test_cpu_end_to_end.rs:426-526`) loses its `most_offered > 2` guard and asserts `pulled ==
non-empty source lanes` on a filtered variant. Goldens: ten tpch plan sections (`scan-limit`
and `nested-limits` × 5, mappings → `[[[0]]]`, with their `--- memory ---`); ten
`.cpu.txt`/`.cost.txt` sections (two tp1 `scan-limit` sections authored for the first time);
`mini.result.txt` and `recipe-payloads.txt` unchanged. `corpus_cases.inc:44-51`; registry rows
135 and 131; `build-test.md` corpus N +2; `architecture.md:374-379` and its four `set_num_rows`
sentences (`:377-378`, `:814`, `:1043`, `:1073`). Order: after fix 10's regen; fix 4 for
nested_limits on a device. In the window before task 4 with fixes 10, 1, 2 and 3 — but its C++,
gtest and `test_gpu_abi` case collide with task 11 and task 4 at the rebase level. Cells:
scan_limit cpu × 2. Refusal removed but still off: scan_limit gpu × 5 → fix 8 and D1;
nested_limits gpu × 5 → the first device run fails at the `region` scan (ticket 1, fix 4), then
the cross join's 5 chunks are fix 10's — with fixes 4 and 10, 5 cells.

**Minimum corpus query.**

```sql
SELECT n_nationkey FROM nation LIMIT 3
```

Bare-scan shape at all five modes: CPU 25 rows for 3, device first call refused.
Oracle-comparable: `SELECT count(n_name) FROM (SELECT * FROM nation LIMIT 3)`. In testdata:
`tpch/scan_limit` (lineitem).

### 14. The grouping id is DataFusion's value and width on the device — M

**Closes:** #65 for every reader but q70/q86 (a wall: window functions, #143). Drops `65` from
nine registry rows (co-attributions). Files ticket 10.

**Issue.** `cpp/src/operators/aggregate.cpp:388-392, :414-415` folds `gid |= (1 << i)` into an
`int32_t` and materialises INT32; DataFusion folds `(acc << 1) | is_null` MSB-first
(`aggregates/mod.rs:1274`) at `UInt8/16/32/64` by key count (`logical_plan/plan.rs:3223-3233`).
A two-key rollup is 0, 1, 3 on the CPU and 0, 2, 3 on the device, four bytes against a declared
one. The wire carries masks and NULL placeholders and no per-set id, so the C++ invents value
and width; its comment says "only has to be distinct per set" — true for the merge above it,
false for every other reader: the `GROUPING()` value, and the declared `UInt8` against the
device's `INT32` (#164, `declared-schemas`). Nothing pins the C++ side (the walk's ROLLUP
compares after the projection drops the id; `SumByKeyAndGroupingId` manufactures an Int64 id).
Keeps off nothing directly; a precondition for any device rollup cell (behind
#152/#183/#175/#163).

**Localized fix.** `aggregate.cpp`: a static `grouping_id_column(gid, nkeys, rows)` picking
`numeric_scalar<uint8_t/16/32/64>` by `nkeys <= 8/16/32`; the loop folds `gid = (gid << 1) |
masked` into a `uint64_t`; `:414-415` calls it; `nkeys > 64` throws (DataFusion's own limit).
CPU and GPU: the same id bytes on both. Frozen surface: none; no golden regenerates (payloads
print masks and names, never an id). Tests: a new `executor_cases.inc` case `SumOverRollup`
(`sum(v) GROUP BY ROLLUP(k, v)`, nine rows, the middle set `1` not `2`) with an arm in both
`emitted` matches (`test_cpu_executors.rs` beside `:245`, `test_gpu_executors/contract.rs`
beside `:158`) — red on the device twice; a recipe-walk test `GROUPING` beside `ROLLUP`
(`test_gpu_recipe_walk.rs:640`) running the query below at `ONE_LANE` — the one pin on the
reading path. Python: `aggregates.py:264-271` fold, `test_operators.py:548,566`,
`test_end_to_end.py:310` (`{0, 2, 3}` → `{0, 1, 3}`) — three pins, CI red otherwise. Docs:
`architecture.md:276` ("materializes an INT32 constant") and `:284-286`; `build-test.md:19` 10
→ 11, `:20` twelve rows, `:5`; `aggregate.cpp:329-335, :384-387`;
`translator/tests.rs:653-655`; `executor_cases.inc:50`; `tickets.md:19, :278-283, :720`. Order:
fix 2 first is cleaner (the gid then reaches no hasher); #164 — after this every rollup stops
being its first false positive; task 13 owns the walk file and task 4 moves the test files —
before task 4 or after task 13. A `GROUPING()` over a proper subset or reordering of the rollup
keys also needs an unsigned literal and shift ops in `expr.cpp` (ticket 10); the exact-key-list
form is `CAST(__grouping_id AS Int32)` and works. Code S (~+20/−10 C++ lines); M as a task: ten
files, a contract case on both engines, a walk test, three Python pins, a C++ build and two
shad-gpu binaries.

**Minimum corpus query.**

```sql
SELECT n_regionkey, n_nationkey, GROUPING(n_regionkey, n_nationkey) AS g, count(*)
FROM nation GROUP BY ROLLUP(n_regionkey, n_nationkey)
```

tpch sf1; integer keys and an Int64 count so #183/#187 cannot fire; one lane at every mode. CPU
31 rows with g ∈ {0, 1, 3}; the device answers the five subtotal rows with `g = 2` — a silent
wrong answer. Vehicle: the walk at `ONE_LANE`. Not in testdata.

### 15. A finish with no probe keys answers from the build side it still holds — M

**Closes:** #173 part A; narrows #173 to the LeftMark-through-a-merge shape and part B (walls).
Files ticket 3, the silent wrong answer #173's review found. Does not touch #199's
mid-plan-limit route (ticket 13): a `LimitStream` keeping a zero-row spare to emit at done so a
keyless `GpuAggregate{final}` shortcut above it would fire is the "building around a bug" shape
in `coding-style.md`, against `architecture.md:194` ("streams and holds nothing") and the
`GpuLimit` recipe row at `:773`.

**Issue.** `gpu_backend/join.rs:209-238` `finish_without_keys` refuses
Left/Full/LeftSemi/LeftMark by name and hands the raw build up for LeftAnti, ignoring
`projection` (`reports/hacks-audit.md:243`). On both engines the one-call finishing types —
filtered LeftSemi/LeftAnti/LeftMark (`plan/join.rs:427-435`, `answers_in_one_call`) and
nested-loop Left — publish no at-done call, so `finish_and_fetch` returns nothing at
`gpu_backend/join.rs:186-188` / `cpu_backend/join.rs:255-257`: with build rows and no probe
batch a filtered LeftAnti owes every build row and emits none. A mid-plan `GpuLimit` whose
interval names no row emits nothing (`gpu_backend/accumulate.rs:420-424`), one way a join above
meets an empty probe. Every place a stream becomes nothing already holds a typed table: the
finish still holds the build handle (`GpuProbingJoin::build`, `:136`) and the at-done
pad/narrow projects read exactly the build schema (`wire/join.rs:113-135, :261-322`). Keeps off
zero registry cells (none carries `173`).

**Localized fix.** Device: `gpu_backend/join.rs` + `gpu_backend/backend.rs:128-136` (thread
`build: SchemaRef`): `finish_without_keys` seeds the at-done projects from the build handle —
LeftAnti/Left/Full = the build, LeftSemi = `slice_handle(build, 0, 0)`, LeftMark refused (needs
a keys-schema table). Land LeftAnti-narrow and LeftSemi now; Left/Full after task 11 or as
`bug_` tests naming #198 — the pad project's bare numeric NULL is `is_ast_able`
(`project.cpp:45-49`, `expr.cpp:414`), `build_expr` builds it valid (`:157-255`) and a
Decimal128 as FLOAT64 (`:212-225`). Both engines: give the one-call finishing types an at-done
answer over no probe batch — build rows → every build row (anti) / nothing (semi); a projecting
filtered LeftAnti needs its narrow applied there too, and how (a narrow the executor applies
from the node's `projection`, or an at-done `Narrow` call the recipe publishes — which moves
`recipe-payloads.txt`) is the one design choice left open. The CPU's LeftSemi finish over no
keys is `[0]` already (`process_unmatched_build_batch`). Does not touch the driver, the scatter
drop (task 12), the wire, `LimitStream` on either backend. Frozen surface: none; no golden
unless the narrow takes the recipe route. Tests: `test_gpu_executors/join.rs:335-378` stays;
new Left / LeftSemi / LeftAnti-with-projection / LeftMark-refused cases and CPU twins beside
`cpu_backend/tests/join.rs:600-618`; the ticket-3 case (filtered LeftAnti, `set_build` then
`finish_and_fetch`, no probe → every build row) red today on both. Docs:
`architecture.md:476-479` and `:854-858`; `test_gpu_executors/accumulate.rs:296-329` doc drops
"(#173)"; `cpu_backend/tests/accumulate.rs:66-72`. The `self.build.take()` arm becomes
`ok_or_else`. Order: the Left/Full arms after task 11, and unreachable from a planned tree
until fix 18 anyway; part B (the driver climb through a non-owing join) is #175 Change 1's
sibling and stays a wall; task 9 plans `bug_` tests here. Edits `gpu_backend/join.rs`, which
task 12 also edits (`without_build`, `:103`), and `cpu_backend/join.rs` — before task 4 or
after task 13. Two backends, ~80 lines, a `backend.rs` signature change, ~8 tests on both, one
shad-gpu run; M for the two backends and the signature, not for any regen. Cells: none;
`operator-cases.md:59`'s `bug_` row shrinks to LeftMark-with-no-probe.

**Minimum corpus query.**

```sql
-- device, tp4 modes: LeftSemi with a key-project-only probe; every probe key NULL lands in one
-- lane, three lanes reach finish_without_keys and refuse "no rows … (#173)"; expected 0
SELECT count(*) FROM orders
WHERE o_orderkey IN (SELECT CASE WHEN l_quantity > 100 THEN l_orderkey END FROM lineitem);
-- both backends, any mode: the limit emits nothing; the device refuses at the finish, the CPU answers 5
SELECT count(*) FROM region r
LEFT JOIN (SELECT n_regionkey FROM nation LIMIT 5 OFFSET 100) n ON r.r_regionkey = n.n_regionkey;
```

Neither in testdata. If DataFusion swaps the second to Right it is #175's residual; after part
A the device pads every build row from the handle it holds, so no limit change is needed for
it.

### 16. A whole-day interval literal crosses the wire — M

**Closes:** #168 (ticket drift: the writer substitutes nothing any more — the whole plan fails
and the golden reads `not runnable:`). Files ticket 9, two refusals left by name.

**Issue.** `tpch/mixed-join`'s residual `l_shipdate BETWEEN o_orderdate AND o_orderdate +
INTERVAL '90' DAY` keeps an `IntervalMonthDayNano` literal; `serialize_scalar_value` falls to
`unsupported scalar value` (`wire/serialize.rs:100-102`), `expr_writer.rs:32-37` wraps it with
`(#168)`, `attach_recipes` fails, five plan goldens print `not runnable`. One missing spelling
on the wire. On the device the residual takes the column path (`is_ast_able` refuses a binary
holding a literal whose `fb_to_type_id` is `EMPTY`, `expr.cpp:429-430`), where
`build_column_binary`'s rhs-literal path (`:610-617`) would do `binary_operation(timestamp_D,
duration_D scalar, ADD, TIMESTAMP_DAYS)` — epoch-day addition, what arrow does for a whole-day
interval. `build_scalar` (`:453-496`) has no arm. Off: mixed_join gpu × 5 (registry row 130
`116 168`); no cpu cell. The only unfolded interval in either bench.

**Localized fix.** Append `IntervalMonthDayNano` to `enum DataType` and
`interval_months/days/nanos` to `table ScalarValue` in `gpu_plan.fbs` (appended: no ordinal
moves, no existing payload byte changes); `serialize.rs` writes whole-day intervals and refuses
months or sub-day parts with the `unsupported scalar value:` prefix kept (`wire/tests.rs:630`
asserts on it); `convert_data_type` maps `Interval(MonthDayNano)`; `expr_writer.rs` drops the
`(#168)` wrap; `fb_text::scalar_text` renders the triple; `build_scalar` returns a
`duration_scalar<duration_D>`, refusing months/nanos by name. Plus: refuse an interval literal
inside a `LeftSemi | LeftAnti | LeftMark` join filter at write time in `wire/join.rs` — those
residuals go through `build_expr`'s AST ungated (`join.cpp:98, :233`) and the AST has no
`timestamp + duration`. CPU and GPU: the CPU already answers through DataFusion; the device now
receives the literal. Frozen surface: the fbs moves (appended member and fields; no ABI symbol;
no existing wire byte). Pins that move: `test_plan_goldens.rs:375-380` `uncrossable == ["tpch
mixed-join"]` → `is_empty()`; `NOT_RUNNABLE` (`:400`) empties; `PAYLOAD_QUERIES` gains
mixed-join after the digest-verify run; `wire/tests.rs:578-632` re-pointed at a month interval;
two doc examples. Device: a walk test
`a_join_residual_that_adds_days_to_a_date_answers_on_the_device` (the only end-to-end proof).
Goldens: mixed-join's `--- recipes ---` in five `.plans.txt`, a new `recipe-payloads.txt`
section. Registry row 130 → `116 152 187`; `corpus_cases.inc:80-82`; #168 archived;
`declared-schemas.md:226-228` becomes false — frozen spec, note it in the archive entry. Order:
task 11 replaces `build_expr`'s literal arm with a dispatch to `build_scalar` — either order is
one line, but an `expr.cpp` collision: before task 4 or after task 13. Fix 9's q72 rewrite
avoids intervals because of this. ~25 production lines in five files, ~100 lines of tests, an
fbs append, five golden sections, the payload golden under both variables on the symlinked
host, one shad-gpu cycle for the walk plus a device corpus filter run. Cells: none directly;
mixed_join's five gpu cells move from "cannot cross" to hash_join's state — tp1-single on #187
(confirm with one `PCK_TEST_FILTER=mixed_join` device run), the other four on #152.

**Minimum corpus query.**

```sql
SELECT count(*) FROM orders WHERE o_orderdate + INTERVAL '90' DAY < DATE '1993-01-01'
```

All five modes, gpu; `not runnable` at plan time today at `#1`; not in testdata.
`tpch/mixed-join` is the committed carrier.

### 17. A state column's type is the SQL aggregate the executor runs it as; `avg`'s finalize keeps the divide's declared scale — L

**Closes:** #163 for the CPU; narrows it to the device's Welford count (Int64 exported vs
UInt64 declared — the `declared-schemas` catalog's row) plus the absent produced-vs-declared
validator. Closes the unticketed CPU avg-finalize scale wall (`expr_physical.rs:55-59` drops
`out_type`) that would have stopped 19 of 23 queries one step later. Precondition for fix 11,
for q27 under fix 9, and for q18/q22 × tp4 under fix 2.

**Issue.** `avg`'s state is declared `[count: UInt64, sum: Decimal128(p,s)]` from DataFusion's
`Avg::state_fields` (`translator/aggregate.rs:127-157`), but this engine runs `avg` as `sum` +
`count` (`plan/aggregates.rs:64-67`), whose answers are `Int64` and `Decimal128(p+10,s)`;
`check_state_layout` (`cpu_backend/mod.rs:467-491`) refuses at executor construction.
`decompose` types state by borrowing the accumulator DataFusion planned — right where that
accumulator runs (`sum`/`min`/`max`/`count`, the Welford triple), wrong for a per-column split
that runs other SQL aggregates. Off: 23 queries × 5 modes, cpu and gpu (tpch q1 q17 q22
shuffle_additive_avg; tpcds q1 q6 q7 q9 q13 q14 q17 q18 q22 q24 q26 q30 q32 q35 q39 q65 q81 q85
q92) — 115 cpu cells, the largest cpu blocker. Second wall, by reading: once the state agrees,
every decimal avg (19 of 23) refuses at the finalize project — `finalize()` casts the numerator
to `(p+4,s+4)` before dividing (`plan/aggregates.rs:80-101`), the CPU lowers `Expr::Binary` to
a bare `BinaryExpr` dropping `out_type` (`expr_physical.rs:55-59`, the `..` pattern), arrow's
decimal `Div` answers at `s_num+4`, so `(19,6)/(19,0)` → `(23,10)` and `declared_as` refuses a
scale change.

**Localized fix.** (a) `plan/aggregate.rs`: `state_type(PlanAgg, &DataType)` asks DataFusion's
registry (`all_default_aggregate_functions`) for the per-column aggregate's `return_type`;
`decompose` (`translator/aggregate.rs:148-157`) uses it for `Merge::PerColumn` decompositions
and keeps `field.data_type()` for `Merge::Combined` (Welford); nullability stays DataFusion's
`state_fields` (avg's count nullable — a non-nullable one would import #180's shape into every
keyless avg). (b) `plan/aggregates.rs` `AggFunc::Avg` arm: drop the numerator cast, keep the
denominator cast to `Decimal128(p,0)`; the CPU then answers `(29,6)` and `widened_decimal`'s
same-scale-wider arm narrows to `(19,6)`; the device already pre-scales from
`out_decimal_scale` (`expr.cpp:585-600`), so the removed cast was a redundant kernel there. CPU
and GPU: both declare the type the merge aggregator produces; the device's Welford count stays
the named residual. Frozen surface: none — recipe bytes move in three payload sections via
`aggr_input_schema` types and one fewer cast (a deliberate `PEACOCK_REWRITE_RECIPE_BYTES=1`
regen, argued in the commit). Tests: `schema_tests.rs:146-221` two tests; a CPU test built
through `plan::finalize()` covering init → merge `(35,2)` → finalize. Moves: all ten
`.plans.txt` (every avg state/finalize line); `recipe-payloads.txt` tpch q22, tpcds q14, q39;
up to 115 cpu sections authored; registry 23 rows and their `corpus_cases.inc` lines; q17/q39
(`stddev_samp`) declared `data_fusion_approximate` like `shuffle_stddev`;
`architecture.md:48-50, :246-247, :356-360`; `build-test.md:5, :42`. `widened_decimal`'s
narrowing arm is what makes (b) land, and D1 moves it if taken — coordinate. Order: before fix
11 (same fifteen lines; its `InitFrom::State` arm inherits `state_type(merge_func, inner
type)`); fix 3 either order; fix 2 for q18/q22 × tp4; sections after fix 10's regen; plan
goldens collide with task 10 — after task 13. M for the code (four files, ~45 lines, ten plan
goldens, three payload sections, no device run); L as one branch because of the authoring and
triage of up to 115 sections (T19 needed twenty batches for 37 queries). Recommended split: one
branch with the code, goldens and a handful of cells; enablement in batches. Cells: 109 cpu
alone — 20 queries × 5, q14 × 5 (its rollup is one lane over `GpuInterleave: lanes=1` and never
shuffles; registry row 15 carries no `189`), q18/q22 × tp1; q18/q22 × tp4 (6) with fix 2.
q17/q39 under the approximate oracle. gpu: all 115 stay off (#152 at multi-batch modes;
#183/#187 at tp1-single).

**Minimum corpus query.**

```sql
SELECT avg(s_acctbal) FROM supplier
```

tpch sf1, CPU, tp1-single then tp4-single. Today: refused at construction ("column 1 is UInt64
in the declared state and Int64 in the one DataFusion's accumulators produce"). After (a)
alone: refused at the finalize, `(19,6)` vs `(23,10)`. After (a)+(b): one row, DataFusion's
digits. For a real four-lane merge at every tp4 mode: `SELECT avg(l_quantity) FROM lineitem`.
Not in testdata. On a device: crosses, refused at the unload (#187 → D1).

### 18. A handle can be retained, so a build side survives a streamed probe — L

**Closes:** #152. Re-attributes the 79 rows' remaining cells to #183 (fix 8), #187 (D1), #185
(fix 10) and the cpu-side #175/#180/#189 rows. A prefix of `refcounted-tables.md` (chain
ENS-refcounted-tables, state `new`), which then becomes a C++ cost change (#145) behind the
same symbol.

**Issue.** The device refuses the second probe batch of every streaming join
(`gpu_backend/join.rs:304-314` `build_copy`) and the first probe batch of a Left or Full join
(`:293-299` `copy_of`, unconditional — the key project names `Input::BatchCopy`,
`wire/join.rs:75-79`). `NodeSession::Impl::registry` holds a `unique_ptr<cudf::table>`
(`node_session.cpp:176`); every read arm moves and erases (`:272, :369, :456-457`;
`slice_handle :530`); `take_input`/`execute_one` take inputs by value
(`operators/dispatch.cpp:102-110`). None of the 16 ABI symbols yields a handle without
consuming one. The recipe names `BuildSideCopy` per probe batch whenever `probe_streams` and
`BatchCopy` for the Left/Full key project; the CPU makes both as `Arc` clones
(`cpu_backend/join.rs:239, :247`). `build_copy` hands the first probe batch the original handle
and refuses the second, which is why the build-side semi family runs on a device at all today.
Off: every gpu cell of 79 registry rows except q19 tp1-single, and tpch anti_join/semi_join at
every mode but tp1-single (four probe batches per lane at tp4-single, `tp4-single-mini.cpu.txt`
`== anti-join`), which the registry attributes to `183` alone.

**Localized fix.** One additive C ABI symbol `int peacock_handle_retain(PeacockExecutor*,
uint64_t handle, uint64_t* out)` — `peacock_gpu.h` after `slice_handle` (`:181`), body in
`gpu_executor.cpp` shaped like `result_from_handle`, `NodeSession::retain` after `:542` doing
`make_unique<cudf::table>(view)` (a deep copy until #145; a new id, since
`peacock_handle_release` is idempotent), one `extern` in `peacockdb-ffi/src/lib.rs`.
`gpu_backend/join.rs`: delete `copy_of`, `build_copy`, the `probes` field; the
`BatchCopy`/`BuildSideCopy` arms of `make` (`:251`, `:256-259`) call `retain` and leave the
original in place; a join with no finish releases the build through `GpuBatch::Drop` at end of
stream. Accounting unchanged (`gpu_backend/backend.rs:287-293`). CPU and GPU: both keep the
build across probe calls; the CPU's `Arc` clone and the device's retain answer the same recipe
input. Frozen surface: the C ABI gains its seventeenth symbol, additively; fbs and wire: none.
Goldens: none — `Input` names stay. Tests: a `HandleRetain` gtest suite in
`test_plan_executor.cpp` (over the 1000-line cap — split it); `test_gpu_abi.rs` +1-2;
`test_gpu_executors/join.rs:53-88` and `:249-312` flip from refusal pins to positives (after
task 11, or as `bug_` tests naming #198); `test_gpu_recipe_walk.rs` `resolve` (`:212-224`) gets
arms for the copy inputs and a join arm that loops over probe batches (red today with `unknown
input handle`); its one-probe-batch assert (`:441`) goes. Docs: `architecture.md:448-457, :771,
:791-800, :838-846, :885-890` (ABI count; reconcile with task 8's `peacock_handle_from_arrow`),
`build-test.md` rows 21-22. Scaffolding removed: `reports/hacks-audit.md:560` and `:585`.
Order: fix 10 first or every join row re-attributes to it; fix 8/D1 first for the
string/decimal sinks; task 11 before the Left/Full rows are enabled. Task 12 must not turn a
kept build-side empty into a probe call; tasks 9 and 13 plan `bug_` tests on #152 — whichever
lands second deletes them; task 4 moves the test files — after task 13. #136 and #145 are the
costs left behind, not walls. M as code — seven files, ~60 lines added / 40 deleted, no golden
regenerated. L as a task: the 79-row registry pass is sixteen shad-gpu rounds; split as for fix
17 — code, tests and q19 × 4 in one branch, enablement in batches. Cells: q19 × 4 likely green
alone; q13 × 5 and q97 × 5 with fix 10 and after task 11 (both pad unmatched build rows through
a bare numeric NULL that #198 builds valid, so a typed zero is counted and fails `IS NULL`
until then — as for every Left/Full row in the 79); anti_join, semi_join × 4 each with fixes
10, 8 and D1. The rest land on #183 (49 rows), #187 (8), #185 (9) and the cpu rows.

**Minimum corpus query.**

```sql
-- half one: Inner, the second probe batch
SELECT n.n_nationkey, c.c_custkey FROM nation n JOIN customer c ON c.c_nationkey = n.n_nationkey
WHERE c.c_acctbal > 9990;
-- half two: Left, the first probe batch
SELECT c.c_custkey, o.o_orderkey FROM customer c LEFT JOIN orders o ON o.o_custkey = c.c_custkey
WHERE c.c_custkey < 20;
```

tpch sf1, gpu. Half one: tp1-single passes; tp1-rowgroup, tp4-single (no small-table demotion
under `Off`, so `customer`'s two row groups merge into two probe batches) and tp4-rowgroup
refuse at the second probe call; tp4-sized iff the mapping shows two batches. Half two: refused
at every mode at the first probe call. Neither in testdata; carriers `tpch/q19` (Inner),
`tpch/left_join` (Left).

## Decisions for the human

### D1 — #187: the device export pulled back to the declared decimal precision at the unload, by the CPU's `declared_as` rule

A booked fix in the first round; now a decision. It is the cast-at-the-unload half of the
rejected `casts.md`, and the repo carries both a sentence that rejects that shape and one that
authorises it.

**Reading one — the rejection applies.** `archive/archived-tasks.md:286` (casts.md):
"Predicting the export type at plan time and casting at the unload builds the divergence into
the plan; the operator harness reports it instead." `:161` (wire-schema.md): the divergence is
"to be found by the operator harness and fixed at its source, not carried as a per-column
width." `188c23ce` rewrote `operator-harness-impl.md:148-151` to expect a per-column precision
on `TableResult` — a C++-side fix — and `operator-harness.md:58` reads "#187, open and owned by
no task". D1 maps every decoded device batch through `declared_as` at the unload
(`gpu_backend/mod.rs:176-183`) — the same site casts.md cast at. The divergence still happens
on the device path and is absorbed at the boundary, which is the shape the rejection names. The
precision default is a cuDF-25.02 limitation that 26.02 lifts (`column_metadata.precision`,
`rapids/include/cudf/interop.hpp:110`), so it is inherent only to the verification floor, not
to cuDF.

**Reading two — `declared-schemas.md:136-137` authorises it.** "The question it genuinely did
ask, does the data fit the declared precision, is a value check that does not belong in a
schema catalog. `declared_as` already asks it on the CPU side, and that is where the production
fix will start." D1 is that function applied to the device stream and nothing else: no
plan-time prediction, no `exports=` rendering, no reason enum — the three things
`coding-style.md`'s case names as what went wrong on the casts branch. The narrow decimal cast
is `safe: false`, so it is a value check, not a relabel; every other type difference
(Int16/Int32, Utf8/Utf8View) still refuses. And "fix it at its source" has no cheap form on
shad-gpu: on 25.02 `column_metadata` is `{name, children_meta}` only
(`rapids-cuda-12.2/include/cudf/interop.hpp:108-119`), `export_table_to_ipc`
(`gpu_executor.cpp:54-56`) builds `{name}` and `cudf::to_arrow_schema` labels every decimal128
at 38 (`to_arrow_schema.cpp:117`), so a C++ fix means rewriting the Arrow C-data `format`
string (`d:38,2` → `d:15,2`) by hand from a precision the C++ must first be handed — by the
wire (the rejected route) or a new ABI symbol. The harness's view is untouched either way: the
walk and the catalog export through `peacock_result_from_handle`, not the unload.

**Three shapes.** (i) D1 as written — Rust, `declared_as` at the unload: move `declared_as` and
`widened_decimal` (`cpu_backend/mod.rs:239-275`, `:499-506`) into `executor/declared.rs`,
`pub(crate)`, two facade items in `executor/mod.rs` (`check_state_layout` at `:467-491` also
calls `widened_decimal`); at `gpu_backend/mod.rs:176-183` map every decoded batch through
`declared_as` before `concat_batches`. Three unit cases; one device test in
`test_gpu_executors/exec.rs` (a project casting `v` to `Decimal128(15,2)`, red today). No
frozen surface, no golden (`Decimal128` is 16 bytes at any precision). Registry row 126 →
enabled × 5 after a run. `declared-schemas.md:7-17` and `tasks.md:95` ("the device path has no
equivalent") become false — frozen spec, flag it. Moves `widened_decimal`, which fix 17 reads —
coordinate; task 4 moves `tests/test_gpu_executors/` — after task 13. S code (six files, ~150
LOC with tests), M to close. (ii) A declared precision handed to the C++ (ABI symbol or
`TableResult` field), the C++ patching the C-data format string on 25.02 and setting
`column_metadata.precision` on 26.02 — the shape `operator-harness-impl.md:148-151`
anticipates. (iii) Neither: #187 stays open as `bug_` tests (`operator-cases-impl.md:70` plans
every decimal case that way) until 26.02 is the floor, then (ii) without the patch.

**Cells at stake.** With (i) or (ii): filter_project gpu × 5 alone; aggregate_groupby,
shuffle_additive × 5 each with fix 8; anti_join, semi_join × tp1-single with fixes 10 and 8 —
17, booked at step 8 of the coverage table. Beyond the table: nine tp1-single rows (tpch
hash_join, q2; tpcds q16 q33 q61 q77 q90 q94 q95) move to #185 (fix 10) or stay on #187; fix
8's 29 re-read rows, fix 18's eight decimal-sink rows and anti_join/semi_join × 4 stay on #187
otherwise; every #163 row whose sink is a decimal `avg` (most of the 115) lands on #187 after
fixes 17, 18 and 10. A narrow decimal in the sink is the common case, so whichever shape is
taken gates most of the gpu column past step 10. Minimum query: `tpch/filter_project`
(`l_quantity:Decimal128(15,2)`), any mode, gpu — in testdata, off at all five on `187` alone;
the unload refuses "expected Decimal128(15, 2) but found Decimal128(38, 2)". Sums that declare
(38,4) pass by coincidence (q6, q19).

### D2 — #184/#95 (fix 12): which fbs append carries the decimal key's precision

Fix 12 stays a fix — both shapes close it — but the shape is the human's: `hash_key_precisions:
[uint8]` appended to `CudfRepartition` (recommended: it carries exactly the fact the kernel
cannot compute), or `PlanNode.output_schema` for the emit node (the proposal's shape, and the
mechanism `wire-schema.md` was rejected for), or neither (drop comet-exactness on decimal keys
and change the conformance gate on both sides). Both appends move `recipe-payloads.txt` for
every shuffle query. Cells at stake: q15 × tp4 (3 gpu) and the seven latent decimal-keyed rows.
Details in fix 12.

## Not fixes

### Stale tickets, closed with a pin

- **#55** (q66's partial-phase divisor cast) — unreachable since the aggregate sequence
  (6123eb06) and the wire's `Merge` mode with the C++ positional state read (78400912): the
  divide runs once, in `Partial`, via `arg_col` → `build_column_binary`'s decimal arm
  (`aggregate.cpp:447-456`, `expr.cpp:589-600`); merge calls take state column refs
  (`translator/aggregate.rs:167-189`); the wire never writes `Final`
  (`aggregate_writer.rs:78-81`). Pin: `test_gpu_recipe_walk.rs` const `SUM_OF_QUOTIENTS` (a
  two-level `sum(price / l_linenumber)`) at `TWO_LANES`, asserting 4 `Aggregate{merge:false}`,
  6 `Aggregate{merge:true}`, 4 finalize projects — the first device run of the grouped
  `arg_col` arm over a computed argument. Registry row 67 → `152 183`;
  `walk-drives-every-plan.md:126`, `-impl.md:106`, `declared-schemas-derived.md:59,96` drop
  #55; `build-test.md:19` +1.
- **#56** (q2's CASE-over-string-equality in a partial sum) — the recorded error was the Final
  phase at fe046022 evaluating `args` in every phase, not the Partial and not the AST
  (`is_ast_able` refuses a `CaseExprNode` and a string literal, `expr.cpp:405-407, :419`).
  Pins: `SUM_BY_FLAG_CASE` beside `SUM_BY_FLAG` (`:634`) at `TWO_LANES` (2 inits, 4 merges, one
  `PlainProject`); `AstRouting.IsAstAble` arms in `cpp/tests/cpu/test_executor.cpp:81-119`
  (helpers through `fb::ScalarValueBuilder` — `CreateScalarValue`'s second parameter is
  `is_null`). Registry row 3 (tpcds q2) → `152`; `archived-tickets.md:24-27` "two files" →
  three. Land with #55 (walk N 10 → 12) at the head of task 13 or before task 4.
- **#47** (q77 GPU 40 rows vs CPU 45) — stale by history: e5d2c0e7 (grouping-set expansion) and
  d3c92de5 (`null_policy::INCLUDE`) post-date the filing commit 2d07e908 and are in HEAD;
  today's CPU answer is 44 (`tpcds.sf1/mini.result.txt` `== q77`; the DuckDB profile records
  `TOP_N` 44), and 44 − 5 = 39 matches the recorded loss modes. Pin: `ROLLUP_OVER_LANES`
  (nation ∪ region with a CASE-made NULL id, `GROUP BY ROLLUP(channel, id)`) and
  `a_rollup_over_a_union_folds_its_totals_across_lanes` at `ONE_LANE`, 13 rows, 2 inits / 3
  merges / 1 finalize. Archive #47 only once that pin is green on shad-gpu, in the same step.
  Registry row 78 drops `47` and archived `97`; `build-test.md` walk +1. q77's cells stay
  (#187/#152 gpu; #175 → fix 2 cpu tp4).
- **#45** (q24 join-key cast to string) — stale: 257377b0 added the string→string identity arm
  (`expr.cpp:912-934`); the cast was q24's residual filter `c_birth_country !=
  CAST(upper(ca_country) AS Utf8View)`, never a key (`column_ordinal_of` refuses a non-column
  key at plan time; `execute_hash_join` accepts `ColumnRef` keys only). Pin: a walk test beside
  `INNER_JOIN` with an explicit `arrow_cast(upper(r_name), 'Utf8View')` (fix 8 removes the
  implicit cast) — the cast on the `GpuHashJoin` line, one `HashJoin{Inner}` call, 21 rows; an
  `IsAstAble` arm: `CAST(c@0 AS Utf8View)` over STRING is not AST-able. Registry row 25 →
  `163`; `expr.cpp:916-920` comment reworded; `declared-schemas.md:218` row 11 and
  `declared-schemas-derived.md:101` re-pointed to ticket 6;
  `walk-drives-every-plan.md:125`/`-impl.md:106` drop #45.
- **#46** (q61's `promotions` sum) — stale: d3c92de5 moved both evaluators to
  `NULL_LOGICAL_AND/OR` (`expr.cpp:118-119`, `:513-514`); the recorded 2855378.83 is q61's
  `promotions` under a null-propagating OR to the cent (promotions 15, 71, 196 survive only
  under three-valued OR; DuckDB reproduces both figures). `tickets.md:118-122` out; registry
  row 62 → `152 187`. Pin, two tpcds corpus lines: `filter-or-nulls.sql` (`SELECT p_promo_sk
  FROM promotion WHERE p_channel_dmail = 'Y' OR p_channel_email = 'Y' OR p_channel_tv = 'Y'`,
  158 rows, Int64 sink — the column path) and an AST twin (`WHERE p_start_date_sk = 2450347 OR
  p_end_date_sk = 2450797`, pinning `fb_to_ast_op`'s `NULL_LOGICAL_OR`). Order: SQL files →
  `corpus_query!` (cpu × 5, `data_fusion_exact`, `golden_exact`) → cpu regen → device run →
  registry rows written with gpu cells as proven (`registry.rs:274-283` refuses a disabled cell
  with no ticket). `promotion` is one row group, so the tp4-single cell would be the tier's
  first multi-lane device unload over three empty lanes — a refusal there is a new ticket. The
  device tier has no NULL-bearing disjunction anywhere today.
- **#57**'s recorded wrong answer is stale (a zeroed gtest literal), but its guard is live →
  fix 7.

### Walls

- **#175 shape (b) — q77's `Right` whose build is a drained Inner join** (`Project ←
  AggregateBatches ← Aggregate ← Project ← HashJoin{Inner}` with no scatter between; the
  `store` scatter leaves lane 2 empty, the Inner takes `NoBuild` and drains,
  `single_partition.rs:187-192`, and the Right is asked at `NoBuild` before it can learn no
  probe row will arrive). Task 12's build-child climb stops at the drop it guards and never
  reaches it. What it wants: an `OwedProbe` lane state (`owes_probe_when_build_empty` on
  `IndexedNode`, `LaneState::OwedProbe`/`LaneCall::OwedProbe`, delete
  `JoinExecutor::without_build` and both backend arms, `Calls.empty_build_answers_nothing` at
  `cpu_backend/join.rs:40-43, :79, :99, :163`, the mock knob `mock.rs:85-91, :149, :519-525`,
  the injection gate `injection.rs:576-615, :664-667, :767-777`; refuse only a probe batch with
  rows) — or #173 B's climb through non-owing joins and merges with one spare zero-row batch
  per lane. Both are outside `empty-build.md`'s Restriction ("code changes are limited to the
  conditional drop"), both buy q77 × 3 cpu only after fix 2, and the climb costs q77 lane 2
  about six calls where `OwedProbe` costs none. Not proposed: the human amends the spec or
  splits a second one. The residual is what #175 keeps after task 12 — narrow it, do not close
  it.
- **#173's LeftMark-through-a-merge shape**: a LeftMark finish over a merge whose every lane
  drained still refuses after fix 15 (it needs a keys-schema table nothing holds). Narrow #173
  to it.
- **#65 for q70/q86**: never — `rank() OVER` window queries the translator refuses on any
  DataFusion (`nodes.rs:159-163`, #143 archived); the 13 window rows are out by design. #23's
  and #65's sentences about them are corrected in fixes 9 and 14.
- **#136** (the build re-hashed per probe batch) and **#145** (an O(1) retain): costs fix 18
  leaves behind, not walls for a cell; `refcounted-tables.md`'s second half.
- **A typed `schema_digest`** (`tests/common/result_text.rs:92`): owned by nobody; the recipe
  walk's raw export (`test_gpu_recipe_walk.rs:166-179`) bypasses the unload where D1's cast
  would live, so a typed digest reddens five walk tests after both fix 8 and D1. It needs the
  walk's export relabelled through `declared_as` once task 4 moves the walk into `src/`. A test
  gap, not a ticket.

### Duplicates of approved tasks

- **#175 → `empty-build.md` (task 12).** Its Change 2 (one derived index field, one conditional
  drop) is the task and buys tpch/q16 cpu × 3. Four facts to amend before dispatch: (i) q77 is
  shape (b) and the spec's climb never reaches it — the spec cannot close q77; (ii) the golden
  delta is zero existing sections, not 244 (no kept empty under the guard in any enabled
  section); (iii) `operators/join.cpp` needs a zero-row guard on all four semi/anti arms on
  cuDF ≥ 25.10 (`:122-124, :143-145, :159-161, :176-178`; `filtered_join.cu:105-130` launches
  `grid_size(build.num_rows())` = 0; the free `cudf::left_*_join` is `[[deprecated]]` on 25.10
  and absent on 26.02; shad-gpu's 25.02 proves only the `#else` path) — outside the spec's "not
  touched" list; (iv) `mini.result.txt:453` q16 `mode=tp1-rowgroup` → `tp4-sized`. Its impl
  plan: Task 2's `SetBuild`-not-`NoBuild` test cannot go red under `MockAcc`
  (`driver/mock.rs:421-430` emits a batch regardless) — assert the emitter's `Emit` count or
  make the mock emit nothing; Task 4's expectation is inverted. CPU unit: Right, RightAnti and
  Full over `set_build(RecordBatch::new_empty)`. Registry: q77 `175 → 189` only after a hand
  run records the hasher signature. Author q16's three sections after fix 10's regen. Rename
  the three refusal-pinning tests to `bug_` (`reports/hacks-audit.md:511`) before the fix, not
  during. Do: amend, then build.
- **#198 → `typed-nulls.md` (task 11).** Precondition for fix 7 (shares
  `test_plan_executor.cpp:49-66` — the helper repair, `make_null_literal`), for fix 15's
  Left/Full arms, for fix 18's positive Left test, and for every Left/Full row fix 18 enables
  (until it lands their pad is a typed zero and the result goldens go red). Two corrections:
  the spec's and #198's sentence "a bare literal short-circuits to `build_scalar` at
  `build_column:830`" describes `build_column`'s entry, which a project reaches only for string
  literals (`project.cpp:45-49` asks `is_ast_able` first, `expr.cpp:414` returns true for a
  non-string literal); and the spec names `PlanExecutor.FilterNationByRegion` — the test is
  `PlanExecutor.FilterNation` (`test_plan_executor.cpp:277`). A Decimal128 pad column stays
  FLOAT64 after it by design ("One arm differs on purpose") — ticket 5. Do: build as approved;
  note both.
- **Tasks 9, 10, 13 (`operator-cases`, `declared-schemas`, `walk-drives-every-plan`).** They
  plan `bug_` tests for #152, #175, #187, #191, #184, #190, #57 and refusal lists naming #189,
  #55, #45. Every fix above that lands after one deletes the test it makes green; landing
  before means the test is never written. The five stale pins and fix 6 append to the walk's
  `PROVEN`/coverage list that task 13 rewrites — ship them at the head of task 13 or before
  task 4. Task 8 (`operator-harness`) adds `peacock_handle_from_arrow`; fix 18 adds
  `peacock_handle_retain` — reconcile the ABI count in `architecture.md` once.
- **#152 → `refcounted-tables.md` (chain ENS-refcounted-tables, `new`, not approved).** Fix 18
  is its strict prefix (§2 the symbol, §3 retain per call, §4 minus the rename, the walk half
  of §6) with `retain` as a deep copy; #145 becomes the second half. The spec is committed and
  frozen — carving it and renaming the chain is the human's edit. Dispatched as written
  instead, fix 18 is subsumed and the `…Again` rename moves ten plan goldens.
- **#178 → `rmm-pool-budget` (task 3):** not a corpus ticket; independent of everything here.
- **Chain placement, shared by all: before task 4, or after task 13 — never between.** Task 4
  (`test-layout`) moves the files fixes 5, 12, 14, 16, 18, D1 and the five stale pins edit
  (`tests/test_gpu_executors/`, `test_inc2_conformance.rs`, the walk, `tests/common/`); task 10
  (`declared-schemas`) regenerates the `.plans.txt`/`recipe-payloads.txt` that fixes 2, 8 and
  17 regenerate; task 11 (`typed-nulls`) edits `cpp/src/expr.cpp` (fixes 5, 6, 7, 16) and
  `test_plan_executor.cpp` (fixes 4, 13); task 12 (`empty-build`) edits `cpu_backend/join.rs`,
  `gpu_backend/join.rs` and `cpu_backend/accumulate.rs`, which fixes 1, 3, 10 and 15 edit, and
  regenerates six `.cpu.txt`, which fix 10 regenerates (all ten); task 13
  (`walk-drives-every-plan`) rewrites `test_gpu_recipe_walk.rs`, which fixes 6, 14 and the five
  pins edit. The practical consequence: with tasks 3–13 approved and the chain unattended,
  nothing here lands on master unless the human holds the chain after task 3 (fix 10 first,
  then fixes 1, 2, 3 and 13 — cpu-only, one regen; fix 13's C++, its gtest and its
  `test_gpu_abi` case rebase against tasks 11 and 4) or waits for task 13.
  ENS-refcounted-tables may not run beside the layout chain (`tasks.md`).

## Tickets to file

Production behaviour only — a wrong answer, a crash, a refusal, a leak.

1. **A scan projected to no columns reads a zero-column, zero-row table on the device**
   (`cpp/src/operators/scan.cpp:37-52` passes `.columns({})`; cuDF's `_columns` is
   `std::optional` and a present empty list selects nothing), and `cudf::cross_join` refuses
   "Left table is empty" (`cross_join.cu:45`). One ticket with two arms alongside #63's
   placeholder. Exposed by fix 13 (nested_limits gpu × 5 — its first device run after fix 13
   fails here, and that failure is this ticket, not a new one), removed by fix 4.
2. **A DESC sort key puts its NULLs at the wrong end on a device.** `operators/sort.cpp:45-46`
   and `node_session.cpp:314-315` map `nulls_first` to `null_order::BEFORE`/`AFTER` regardless
   of direction; cuDF applies `null_precedence` and then flips for `DESCENDING`
   (`cudf/table/row_operators.cuh:422-437`; its own Python sends `asc ^ first`); arrow's
   `nulls_first` is positional and DataFusion defaults `DESC` to `nulls_first = true`
   (`order_by.rs:107`), which the CPU relays (`cpu_backend/mod.rs:522-527`). A silent wrong
   answer for any `ORDER BY <nullable> DESC [LIMIT n]` with NULLs, every mode; inert for q78,
   for the two enabled device cells and for the nine #185 candidates (no NULL sort keys). Fix,
   its own S change: `BEFORE` iff `nulls_first == asc` at both sites, `architecture.md:1044`
   row. Fix 6 lands the `bug_` test.
3. **The one-call finishing join types emit nothing where they owe every build row.** Filtered
   LeftSemi/LeftAnti/LeftMark (`plan/join.rs:427-435`, `answers_in_one_call`) and nested-loop
   Left publish no at-done call, so with build rows and no probe batch `finish_and_fetch`
   returns nothing on both engines (`gpu_backend/join.rs:186-188`,
   `cpu_backend/join.rs:255-257`) — a filtered LeftAnti owes every build row. Not in the sf1
   corpus (no finishing join has an empty probe lane in any enabled golden). Removed by fix 15.
4. **`filtered_join` over a zero-row table launches zero blocks on cuDF ≥ 25.10** in all four
   semi/anti arms of `join.cpp` (over the build for RightSemi/RightAnti, over the probe keys
   for LeftSemi/LeftAnti/LeftMark; `filtered_join.cu:105-130`, `grid_size(build.num_rows())`).
   Latent for any zero-row build or keys; unprovable on shad-gpu (25.02 takes the guarded
   `#else` path). The guard lands with task 12 (amendment iii); a `bug_` test only the 25.10 CI
   leg could redden.
5. **A Left/Full pad project's typed NULL is a typed zero, and a Decimal128 one is a FLOAT64
   column.** A bare numeric NULL literal is `is_ast_able` (`project.cpp:45-49`,
   `expr.cpp:414`), `build_expr` builds every scalar valid (`:157-255`) and a Decimal128 as
   double (`:212-225`). The first half is #198's (task 11); the Decimal128 half survives it by
   design and is a type divergence at every Left/Full pad with a decimal probe column — its own
   ticket and `bug_` test. Exposed by fix 18 (Left/Full reach a device for the first time).
6. **`CAST(<non-string> AS VARCHAR)` is refused on the device** (`expr.cpp:924-925`) where the
   CPU answers. A user-reachable refusal; `declared-schemas.md:218` row 11 and
   `declared-schemas-derived.md:101` need its number (re-pointed from #45).
7. **A projecting cross join drops its projection on both backends.** `GpuCrossJoin.projection`
   (`plan/mod.rs:686-689`) is dropped by `CpuJoin::cross` and absent from `CudfCrossJoin`
   (`gpu_plan.fbs:440-443`) — a plan declaring N columns answers more. No corpus query has one;
   the sibling of #190 (fix 1), exposed by task 9.
8. **A LeftSemi/LeftAnti/LeftMark residual filter carrying a scalar function throws on a
   device.** Those residuals go to `build_expr` unconditionally (`join.cpp:98, :233`) and
   `build_expr` has no scalar-function arm (`:310-312`) — a refusal the CPU does not make. Fix
   16 refuses the interval-literal case of the same path at write time; the scalar-function
   case stays. Worth an `operator-cases` line.
9. **Two interval refusals fix 16 leaves by name:** `interval + date` with the literal on the
   left declares `DURATION_DAYS` through `binop_output_type` (`expr.cpp:618-625`) and cuDF
   refuses at run time; a bare interval literal in a project reaches `build_expr`'s `default:`
   and throws (`:250-253`). No corpus shape.
10. **`GROUPING()` over a proper subset or reordering of the rollup keys cannot be evaluated on
    a device.** DataFusion rewrites that form with `bitwise_and`/`bitwise_shift_*`
    (`datafusion-optimizer-45/src/analyzer/resolve_grouping_function.rs:218-240`);
    `build_scalar`/`build_expr` have no unsigned literal (`expr.cpp:453-497, :157-245`) and
    `fb_to_binop` no `BitwiseShiftLeft/Right` (`:498-521`) although `gpu_plan.fbs:85-86` and
    the Rust IR carry them. The exact-key-list form (any count, one key included) is
    `CAST(__grouping_id AS Int32)` (`:210-216`) and works. A refusal; exposed by fix 14's
    reading path.
11. **Two plan-time refusals fix 11 introduces by name** (replacing the blanket `(#62)` one):
    grouping sets under a distinct aggregate; a Welford companion of a distinct aggregate (no
    init-form `merge_m2` on either engine). Plus the pre-existing one it makes visible: a
    coercion cast makes a non-Column distinct argument (`avg(DISTINCT int_col)`) refused at run
    time by the C++ as any `GROUP BY <expr>` is (`aggregate.cpp:163`).
12. **A CASE with no ELSE and a `Decimal128` THEN null-fills at `scale_type{0}`** through
    `make_default_constructed_scalar`, and `copy_if_else` refuses the type
    (`expr.cpp:790-795`). A refusal; no corpus query reaches it.
13. **Add to #199, not a new number:** a `GpuAggregate{final}` shortcut over a lane with no
    batch emits no row on both engines — reachable from a mid-plan limit that dropped
    everything. Not fixed here: the node that owes the row is the finalizing aggregate (fix 3's
    rule); the shortcut is an exec-category node with no at-done pass, so giving it one is a
    driver change no fix here makes. #199 keeps it beside the finalizing-node divergence fix 3
    names.

Not tickets — drift the registry pass under fix 10 sweeps: archived `97` on 32 rows,
`115`/`116`/`32`/`143` on others; `62` on four rows that run (tpcds q16 q94 q95, tpch q16);
`65` as a co-attribution on nine rows; `55`/`56` on rows held by #152/#183 alone;
`47`/`46`/`45`/`60`/`57`/`63` per the closures above; `183` alone on tpch anti_join/semi_join,
which also carry #152 (four modes) and #187. Ticket text to correct at close: #175 names q21
(q16/q77); #173 names three sites that emit nothing rather than refuse and misses ticket 3;
#185's title and mechanism; #60's mechanism; #47's 45/40 (44/39); #158's "cannot fire with a
JOIN" (false for a cross join — `AggregateStatistics` answers `count(*)` over `region, nation`
from `Exact × Exact`); #198's short-circuit sentence; #23's q70/q86; #65's "(q70/q86 after
#23)"; #187's "two engine rules disagree"; #183's "DataFusion's type rule"; #192's "no budget
stops it"; #184's "1→4 shape"; #57's "all-0/null"; #63's "1-row vs other size"; #45's "join-key
cast"; #168's "the writer substitutes". Pages: `build-test.md:42` ("37 queries", "thirteen on
#163" — 23; q90 missing from the #180 clause), `:44` (the blocker list lacks #191, #168), `:5`
totals; `architecture.md:97-100` (the small-table rule bites only under batching),
`:273-275`/`:294-295` (true after fix 2), `:276`/`:284-286` (after fix 14), `:374-379` and the
four `set_num_rows` sentences (after fix 13), `:476-479`/`:606-607`/`:855-857` (after fix 15
and task 12), the `CudfCoalescePartitions` row, `:1044` (after ticket 2).

## Coverage

Landing order, not page order. Each row's number assumes the rows above it have landed; every
gpu number is a candidate that needs one shad-gpu run per row. cpu starts at 444 enabled / 156
disabled / 90 na; gpu at 6 / 594 / 90. The order honours the chain rule above: fixes 10, 1, 2,
3 and 13 fit the window before task 4 (cpu-only, one regen; fix 13's gtest and `test_gpu_abi`
case rebase against tasks 11 and 4); everything that regenerates `.plans.txt` or
`recipe-payloads.txt` (fixes 8, 17), edits `tests/test_gpu_executors/` (D1) or
`test_plan_executor.cpp` (fix 4) waits for task 13. Step 8 is conditional on D1; the paragraph
below the table says what happens without it.

| step | cpu back | gpu back | cumulative cpu / gpu | assumption |
|---|---|---|---|---|
| 1. fix 10 (#185) | 0 | 9 | 0 / 9 | tpcds q38 q48 q87 q88 q93 q96, tpch q3 q14 join_int at tp1-single: their T19 runs completed through the unload, so no #183/#187; results never compared |
| 2. fix 1 (#190) | 10 | 0 | 10 / 9 | q11, q54 × 5; sections authored after step 1 |
| 3. fix 2 (#189) | 9 | 0 | 19 / 9 | q5, q80, rollup_over_join × tp4 |
| 4. fix 3 (#180) | 9 | 0 | 28 / 9 | q88, q90, q96 × tp4 |
| 5. fix 13 (#186/#188) | 2 | 0 | 30 / 9 | scan_limit × tp1; `with_limit` over an empty projection (nested-limits' `region`) read, not run; nested_limits' first device run after this fails at the `region` scan — ticket 1, not a new one |
| 6. tasks 4–13 as approved | 3 | 0 | 33 / 9 | task 12 lands q16 × tp4 after the four spec amendments; task 11 has no cell of its own, but q13, q97 and every Left/Full row in fix 18's 79 answer wrongly on a device until it lands (the pad's bare NULL is built valid, so a typed zero is counted by `count(o_orderkey)` and fails `IS NULL`) |
| 7. fix 8 (#183/#192) | 5 | 27 | 38 / 36 | cpu: q64 × 5 if the measured run (after fix 10, whose concat transient it must include) fits 15 GiB. gpu alone: shuffle_stddev, nested_loop_join × 5; with fix 10: cross_join, nested_loop_left_join, q4 × 5, q15 tp1 × 2 (q15's joins assumed one chunk per call); tp4 shapes of the small-table joins (four lanes, three empty, merged to one) unrun on a device |
| 8. D1 (#187) — if taken | 0 | 17 | 38 / 53 | filter_project × 5 alone; aggregate_groupby, shuffle_additive × 5 with fix 8; anti_join, semi_join × tp1-single only with fixes 10 and 8 (their other modes scatter several probe batches per lane — step 11); every `safe: false` cast fits its declared precision |
| 9. fix 4 (#63 + ticket 1) | 0 | 5 | 38 / 58 | nested_limits × 5 with fixes 13 and 10; the mid-plan `slice_handle` path and the vanished `abandoned` line behave on a device |
| 10. fix 17 (#163) | 115 | 0 | 153 / 58 | 20 queries × 5, q14 × 5 (a one-lane rollup, no shuffle), q18/q22 × 5 (tp4 via step 3); q17/q39 under the approximate oracle; no third wall (q9's tp4 `count(*)` shape is step 4's) |
| 11. fix 18 (#152) | 0 | 22 | 153 / 80 | q19 × 4 alone; q13, q97 × 5 with fix 10 and after task 11; anti_join, semi_join × 4 (tp1-rowgroup, tp4-single, tp4-rowgroup, tp4-sized) with fixes 10, 8 and D1. The other ~65 rows' cells land on fix 8/D1/fix 10 first and are unknown until run |
| 12. fix 9 (#23) | 10 | 0 | 163 / 80 | plan cells q27, q72 × 5, and ten cpu cells out of the `na` pool: q27 × 5 after steps 10 and 4 (the keyless third branch at tp4); q72 × 5 decided by a run — tp1 × 2 only if engine and oracle fit a 15 GiB runner, tp4 × 3 with no known wall in front (not #180: its counts are grouped) |
| 13. fix 11 (#62) | 5 | 0 | 168 / 80 | q28 cpu × 5 (na today) after step 10; plan cells × 5 |
| 14. fix 12 (#184/#95) | 0 | 3 | 168 / 83 | q15 × tp4 with fixes 8 and 10: one probe batch per lane at both joins at every tp4 mode, string and `Decimal128(38,4)` sink; a run decides |
| 15. fixes 5, 6, 7, 14, 15, 16 | 0 | 0 | 168 / 83 | none from the existing corpus. Optional rows: fix 5's `extract-year` +5/+5; #46's two `filter-or-nulls` queries +10/≤+10; fix 6's `ROUND_PLACES`, fix 4's `scalar-subqueries`, fix 12's decimal-key query, fix 13's `nation LIMIT 3` each +5/≤+5 |
| — wall | 3 | 0 | 171 / 83 | q77 cpu × tp4 (#175 shape (b) or #173 B, then fix 2) |
| — never | — | — | — | q70, q86 and the 13 window rows (75 na cells per engine after steps 12 and 13 turn q27, q28, q72's 15 into cells) |

cpu: 156 disabled + 15 na (q27, q28, q72) = 171 reachable — 168 by the fixes above, 3 walled
(q77); na 90 → 75. gpu: 83 candidates of 594, 17 of them conditional on D1; of the rest, 115
are #163 rows (fixes 17, 18, 8/D1 and 10, then a run each), roughly 330 are #152 rows' other
modes and tp1-single cells with a string or decimal sink (fixes 18, 8/D1 and 10, then a run
each), and the remainder sit behind cpu-disabled cells. Without D1 (or shape (ii)): step 8's 17
do not land, step 11's anti_join/semi_join × 4 each stay on #187 (their sinks carry
`o_totalprice:Decimal128(15,2)`), the nine tp1-single rows D1 re-tickets to #185 stay on #187,
fix 8's 29 re-read rows and fix 18's eight decimal-sink rows stay on #187 — 83 → 58 booked, and
every decimal-sink row in the 115 and the ~330 lands on #187 rather than on its next wall. The
one fact every gpu number rests on: a device join cell has never passed the section compare, so
"candidate" means the plan's batch lists now agree by construction, not that anyone has seen it
green.
