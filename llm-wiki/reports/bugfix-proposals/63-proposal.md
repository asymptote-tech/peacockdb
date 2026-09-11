# #63 — proposal: the cross join leaks the device's row-count placeholder into the plan's ordinals

Read at master 188c23ce. Paths relative to `/media/data/peacockdb`. cuDF read from
`third_party/cudf` (25.10) and `/home/dmitry/cudf` at tag `v25.02.00` (what shad-gpu runs), each
cite naming which. Nothing built or run.

**Verdict on the standing claims.** `56-proposal.md` §7 says #63 "shares #56's mechanism (an
expression evaluated against the wrong table) and closes the same way behind #163 — one
observation, no code". Half right. It is right that every `build_column` arm answers
`table.num_rows()` rows, so the "1-row branch vs other-sized branch" pair the ticket describes
cannot be built on this tree (§2). It is wrong about what failed and about the closure: the line
the original diagnosis cites is cuDF's *type* check, not its size check; the cross-joined table the
CASE reads on a device is not the plan's table — it carries an undeclared leading column the
device invented for a zero-column node — and that shifts every ordinal by one, which lands a
`count(*)` where the CASE expects an `avg`, at exactly that line. That is a live C++ defect,
reproducible by reading on today's tree, fixed in `join.cpp`, and pinned by a gtest that goes red
today. It is not #56's class (a phase re-evaluation) and not #57's (an unimplemented arm behind a
guard); `architecture.md:62` and `plan/mod.rs:61` file it with #55/#56 and should not.

## 1. Issue

The device answers a cross join whose one side is a zero-column node with one column more than
the plan declares, so every column ordinal above the join is off by one.

- Producer: `cpp/src/operators/project.cpp:19-30`. A `CudfProject` with no `exprs` emits one
  INT8 column named `__rowcount__` of the input's row count. The comment says it exists so a
  `count(*)` above can read a row count, and that is the one consumer it was written for.
- Consumer that leaks it: `cpp/src/operators/join.cpp:390-398`, `execute_cross_join`, which is
  `cudf::cross_join(left, right)` with the two name lists appended. The placeholder rides along as
  column 0 of the output, declared nowhere.
- Where it surfaces: `cpp/src/expr.cpp:801`, the `copy_if_else` in `build_column_case`, reached
  from the top-level `CudfProject`'s CASE. With the shift, THEN resolves to the `count(*)` column
  (Int64) and ELSE to the `avg` the plan meant for THEN (Decimal128), and cuDF refuses the pair:
  `Both inputs must be of the same type`.

tpcds q9's shape (`testdata/goldens/tpcds.sf1/tp1-single.plans.txt:10142-10243`): fifteen
`GpuCrossJoin`s, each build side a `GpuCoalesceAllBatches` over the chain so far and each probe a
one-row keyless `GpuAggregateBatches`; the innermost build side (`:10174-10177`) is
`GpuCoalesceAllBatches: schema=[]` over `GpuProject: exprs=[], schema=[]` over the `reason`
filter — one row, no columns. The top `GpuProject` (`:10144`) reads
`CASE WHEN count(*)@0 > 74129 THEN avg(…)@1 ELSE avg(…)@2 END` five times over the declared
fifteen-column, one-row table. On a device that table has sixteen columns.

What it disables (00-tickets.md row, corrected): `tpcds/q9` gpu × 5, `testdata/cost-registry.csv:10`
carries `63 163`; `corpus_cases.inc:278` has both mode lists `none` and the comment at `:270-273`
attributes the query to #163 alone. The row's code-path column, "`cpp/src/expr.cpp` CASE lowering
(broadcast scalar-subquery branches)", names the wrong file: the CASE lowering is correct and the
defect is the cross join. Two corrections to the "what it disables" column: q9's device cells
would not come back with this fix alone — after #163 clears the cpu, the next device wall is
[#187](../../llm-wiki/tasks/active-tickets.md#t187) at the unload (five `Decimal128(11,6)` outputs,
which the device exports at 38), and at the three tp4 modes the cpu cell itself is #180's shape
(a keyless `count(*)` merged across four lanes, `tp4-single.plans.txt:18542`, the same shape as
q88/q90/q96). And the class is wider than q9: `tpch/nested-limits` puts a zero-column *scan*
under a cross join (`tpch.sf1/tp1-single.plans.txt:172`, `projections=[]`), which
`188-review.md` F1 already found refuses on a device for the sibling reason (the scan reads no
columns and so no rows, `scan.cpp:46-52`); that is the scan arm of the same defect and is that
review's proposed ticket, not this one.

## 2. Root cause

**The recorded failure is a type mismatch, not a size mismatch.** The comment that parked q9
under #63 (commit 70e07dfb, the legacy `test_gpu_executor_tpcds.rs`) reads: "a GpuProject cuDF
failure (copying/copy.cu:367) building its top-level CASE of 15 scalar-subquery comparisons
(copy_if_else over a 1-row scalar vs an empty branch)". At cuDF `v25.02.00` — shad-gpu's version
then and now — `cpp/src/copying/copy.cu:362-368` is the three-input `copy_if_else`'s preamble:
`:362` the mask size, `:365-366` `lhs.size() == rhs.size()` ("Both columns must be of the same
size"), `:367-368` `have_same_types(lhs, rhs)` ("Both inputs must be of the same type"). Line 367
is the type check. The parenthetical was a guess at what a size failure would mean; the line says
the branches had different types.

**A size mismatch inside `build_column_case` cannot be built.** `build_column(expr, table)`
(`expr.cpp:820-942`) returns `table.num_rows()` rows in every arm: a literal is
`make_column_from_scalar(sc, table.num_rows())` (`:832`); a `ColumnRef` is a copy of
`table.column(idx)` (`:847-855`), and a `table_view`'s columns are one length; the AST arm is
`compute_column(table, ast)` (`:862`); `build_column_binary` (`:579-632`) is a `binary_operation`
over columns built from the same table or a scalar, and cuDF refuses unequal column sizes there;
the unary, LIKE, cast, scalar-function and CASE arms each map one column to one of the same
length. So the fold at `:797-802` always sees three columns of one size, and 56-proposal.md's
reading of that is right. The `else`-less fill at `:790-795` is sized from `last_then->size()`,
the same number.

**What is not the same as the plan's table is the table.** Trace of q9's innermost chain on a
device, seqs from the golden's `--- recipes ---`:

    #2  CudfProject{exprs=[]} over the reason filter     → 1 row, 1 col  [__rowcount__:INT8]   (project.cpp:19-30)
    #3  CudfCoalescePartitions, one handle               → passthrough, names kept              (node_session.cpp:285)
    #7/#9/#10  count(*) over store_sales, merged, finalized → 1 row, 1 col [count(*):INT64]
    #11 CudfCrossJoin(#3, #10)                           → 1 row, 2 cols [__rowcount__, count(*)] declared [count(*)]   (join.cpp:394-396)
    #12 CudfCoalescePartitions                           → passthrough
    #19 CudfCrossJoin(#12, avg_disc)                     → 3 cols, declared 2
    …
    #127 CudfCrossJoin                                   → 16 cols, declared 15
    #128 CudfProject{CASE …}                             → build_column_case (expr.cpp:773)

At `#128`, `CASE WHEN count(*)@0 > 74129 THEN avg(…)@1 ELSE avg(…)@2 END`:

- `is_ast_able` (`expr.cpp:403-451`) says no for a `CaseExprNode` (`:406`), so
  `build_column_case` (`:773`). ELSE present → `result = build_column(ColumnRef@2)` — device column
  2 is `avg(ss_ext_discount_amt)`, DECIMAL128 scale −6, the plan's THEN.
- `cond = build_column(count(*)@0 > 74129)`: `infer_expr_type` reads the device column's type
  (`:349-353`) — INT8, the placeholder — against the INT64 literal, `lt != rt` (`:433`) → the column
  path → `binary_operation(INT8 col, INT64 scalar, GREATER, BOOL8)` (`:609-616`), one row, false.
- `then = build_column(ColumnRef@1)` — device column 1 is `count(*)`, INT64.
- `copy_if_else(then INT64, result DECIMAL128, cond)` (`:801`) → `have_same_types` false →
  `cudf::data_type_error("Both inputs must be of the same type")`, `copy.cu:367`.

The same trace held on the legacy executor: at 70e07dfb `plan_executor.cpp:1160-1163` is the same
placeholder, `:851-895` the same `build_column_case`, and the join was an unconditional
`NestedLoopJoin` lowered to `cudf::cross_join` (`:2046-2049`). Same shift, same line.

**Why the placeholder exists, and why it is the right representation.** cuDF cannot spell "n rows,
no columns": `cudf::table::num_rows()` is read off the first column and is 0 with none
(`third_party/cudf/cpp/include/cudf/table/table.hpp:93,202`, `src/table/table.cpp:49-51`), and
`cudf::cross_join` refuses a zero-column input outright (`src/join/cross_join.cu:45`, "Left table
is empty"). The CPU has no such limit — arrow's `concat_batches` sums row counts over an empty
schema (`arrow-select-54.2.1/src/concat.rs:271-277`) and `nested-limits` is green at all five cpu
modes over exactly this shape. So the device needs a column to carry the count, and the plan
declares none there, so no ordinal can ever name it. The count(*) consumer reads it correctly:
`count(1)`'s literal argument is materialized to `tv.num_rows()` rows (`aggregate.cpp:196-204`,
`:271-275`), and the no-arg form reads `tv.column(0)` (`:207`, `:455`). The collapse keeps its name
(`node_session.cpp:285`), a slice keeps it (`:534`), the stats read the right row count
(`:334`). The one operator that appends its inputs' columns into a wider table is the cross join
(and the nested-loop join's cross product, `join.cpp:432,463`), and that is the one place the
placeholder must not pass through. Nothing checks column counts between nodes on a device
(architecture.md, "What guards it, and what does not"), which is why the leak is silent until an
expression trips on a type.

**Judging the #56 claim, line by line.** "Every CASE branch is a `ColumnRef` into one 1-row
cross-joined table" — true of the plan, false of the device's table, which has sixteen columns.
"No `build_column` arm answers other than `table.num_rows()` rows" — true, and irrelevant to the
failure, which is `have_same_types`. "Its closure is one observation, no code" — a device run of
q9 after #163 would reproduce the ticket's text at `#128`, so the observation would reopen it.
"q9 sits behind #163 on both engines, so the observation is a `GpuProject` case, not a corpus run"
— the case that shows it is a `CudfCrossJoin` over an empty `CudfProject`, and it needs no
corpus query or #163: §3d.

## 3. Localized fix

Three C++ files, one gtest file, no Rust, no ABI, no `.fbs`, no wire bytes. The placeholder stays
as the device's spelling of a zero-column table; it gets one name and one predicate, and the cross
product drops it.

### 3a. `cpp/src/peacock/operators.h` — name the placeholder once

After `execute_passthrough`:

```cpp
// A node with rows and no columns has no cuDF spelling — `cudf::table` reads its row count
// off column 0 — so the device carries the count in one INT8 column of this name and no
// other. The plan declares no column there, so no ordinal names it; the cross product is
// the one operator that appends its inputs' columns and it drops the placeholder, since
// leaving it shifts every ordinal above.
constexpr const char* kRowCountColumn = "__rowcount__";
TableResult row_count_table(cudf::size_type rows);
bool is_row_count_only(const TableResult& t);
```

### 3b. `cpp/src/operators/project.cpp` — the producer uses the name

Replace `:19-30` with `return row_count_table(input.table->num_rows());` and define beside
`execute_project`:

```cpp
TableResult row_count_table(cudf::size_type rows) {
  cudf::numeric_scalar<int8_t> zero(0, true);
  std::vector<std::unique_ptr<cudf::column>> columns;
  columns.push_back(cudf::make_column_from_scalar(zero, rows));
  return {std::make_unique<cudf::table>(std::move(columns)), {kRowCountColumn}};
}

bool is_row_count_only(const TableResult& t) {
  return t.table->num_columns() == 1 && t.column_names.size() == 1 &&
         t.column_names[0] == kRowCountColumn &&
         t.table->view().column(0).type().id() == cudf::type_id::INT8;
}
```

The old comment ("DataFusion emits one feeding count(*)") goes: the header carries the reason,
and the count(*) consumer was the assumption that failed. Exactly-one-column is the invariant —
a placeholder never sits beside a real column unless an appending operator forgot to drop it —
so the predicate is also the guard that would catch the next such operator.

### 3c. `cpp/src/operators/join.cpp` — the cross product drops a row-count side

Add `#include <cudf/reshape.hpp>` (`cudf::tile`) and `#include <cudf/filling.hpp>`
(`cudf::repeat`); both are in 25.02 and 25.10 (`reshape.hpp:80`, `filling.hpp:150`). Above
`execute_cross_join`:

```cpp
// cudf::cross_join is repeat(left, R) beside tile(right, L). A row-count side has no
// columns to contribute, so it leaves only the other half, at its own row count.
static TableResult cross_product(TableResult left, TableResult right) {
  const bool l_rows_only = is_row_count_only(left);
  const bool r_rows_only = is_row_count_only(right);
  const auto l = left.table->num_rows();
  const auto r = right.table->num_rows();
  if (l_rows_only && r_rows_only) {
    const int64_t rows = static_cast<int64_t>(l) * r;
    if (rows > std::numeric_limits<cudf::size_type>::max())
      throw std::runtime_error("CudfCrossJoin: the product exceeds cuDF's column size");
    return row_count_table(static_cast<cudf::size_type>(rows));
  }
  if (l_rows_only)
    return {cudf::tile(right.table->view(), l), std::move(right.column_names)};
  if (r_rows_only)
    return {cudf::repeat(left.table->view(), r), std::move(left.column_names)};
  auto out = cudf::cross_join(left.table->view(), right.table->view());
  auto names = std::move(left.column_names);
  names.insert(names.end(), right.column_names.begin(), right.column_names.end());
  return {std::move(out), std::move(names)};
}

TableResult execute_cross_join(const fb::CudfCrossJoin*, NodeInputs* in) {
  auto left = take_input(in);
  auto right = take_input(in);
  return cross_product(std::move(left), std::move(right));
}
```

Row order is byte-identical to `cudf::cross_join` minus the dropped column: `tile(right, L)` is
what `cross_join` builds for its right half (`cross_join.cu:62`), `repeat(left, R)` its left half
(`:59`). Empty sides agree too — `tile` and `repeat` return `empty_like` for a zero count or a
zero-row input (`reshape/tile.cu:55`, `filling/repeat.cu:141`), which is `cross_join`'s own rule
(`cross_join.cu:49-57`). The overflow check mirrors `repeat.cu:144`, which `tile` lacks.

The unconditional nested-loop arm (`join.cpp:421-432`) is the same product and should call
`cross_product` after its existing LEFT-with-empty-right throw; its two filtered arms (`:463`
masks the crossed table by `ltv.num_columns()`, `:479` runs a conditional join over both views)
would shift the same way with a zero-column side but no plan puts one there — a predicate needs a
column on each side — so they stay as they are.

### 3d. `cpp/tests/gpu/test_plan_executor.cpp` — three cases, named for the shape, red today

Built the way `ProjectRename` (`:630`) and `AggregateCount` (`:506`) are: `make_plan_node`,
`WholePlan`, `tpch.minimal`. `region` has 5 rows, `nation` 25.

- `ACrossJoinOverAZeroColumnBuildSideEmitsOnlyTheProbesColumns` — left
  `CudfProject{exprs=0, aliases=0}` over the `region` scan, right the `nation` scan projected to
  `{0}` (`n_nationkey`, as `ScanNationProjected` `:246` does). Assert `num_columns() == 1`,
  `num_rows() == 125`, `column_names[0] == "n_nationkey"`, and the tile order: row 0 is 0, row 24
  is 24, row 25 is 0, row 124 is 24 (`get_scalar_value<int32_t>`). Today: two columns, the first
  `__rowcount__`.
- `ACrossJoinOverAZeroColumnProbeSideEmitsOnlyTheBuildsColumns` — the mirror; repeat order: rows
  0–4 are 0, row 5 is 1, row 124 is 24.
- `ACrossJoinOfTwoZeroColumnSidesKeepsTheRowCount` — both sides empty projects, a `count`
  `CudfAggregate{Single}` with no args above, as `AggregateCount` builds it. Assert 25 (5 × 5).
  Today this passes by accident (a two-placeholder table counts by column 0); it is the case that
  pins the both-sides arm, so it goes in with the other two.

The first is the regression test for #63: the shape q9 puts on a device, without q9's fifteen
copies, its `avg`s (#163, #187) or its `store_sales` scans.

### 3e. Registry, ticket, pages

- `testdata/cost-registry.csv:10`: `63 163` → `163`. The device's next wall is named by the next
  run, per `corpus_cases.inc`'s "run rather than declared" convention; §6 predicts it.
- `llm-wiki/tickets.md:315-319`: #63 to `archive/archived-tickets.md` as Done, with the corrected
  mechanism in two lines: the recorded line was cuDF's type check; the cross join appended the
  device's row-count placeholder to the plan's columns and shifted every ordinal above. Contents
  row `:19` drops `#63` and its count.
- `llm-wiki/architecture.md:61-64` and `peacockdb-core/src/plan/mod.rs:60-64`: the "#55/#56/#63"
  class is expressions evaluated against the wrong phase's table; #63 was never that. Drop it
  from both (the analyst's completeness pass names the sentence; the comment is the developer's).
- `architecture.md:816` (`CudfProject` row): add "a project of no columns emits one INT8
  `__rowcount__` column, the device's spelling of a zero-column table". `:819` (`CudfCrossJoin`
  row): "`cudf::cross_join(ltv, rtv)`; a row-count side leaves `tile`/`repeat` of the other".
- `00-tickets.md:59` (scratch): the code-path column names the wrong file.

### What it deliberately does not touch

- The scan arm: `scan.cpp:46-52` reads every column for an empty projection, which for a node
  with zero fields is `.columns({})` and a zero-column, zero-row table (188-review.md F1). Making
  it emit `row_count_table(rows)` — rows from `cudf::io::read_parquet_metadata` over the row
  groups of the call, or one column read and dropped — is that review's ticket; `cross_product`
  is what it will feed. `tpch/nested-limits` stays off on #188 and then that ticket.
- The filter's `projection` (`filter.cpp:34`, `size() > 0` means "keep all"): the third arm of the
  class if the translator ever emits a filter projecting to nothing; no golden has one.
- `TableResult`, `NodeStats`, `node_session.cpp`, the Rust driver and backends, the wire writers,
  the exec model (`recipe.py:314-320` models an empty project as a zero-column pandas table and
  no model plan lowers one; a model of the placeholder belongs with the scan ticket, not here).
- `build_column_case`, `is_ast_able`, `binop_output_type`: all correct on this path.
- The recipe walk's `CrossJoin` refusal (`test_gpu_recipe_walk.rs:789`, #152) and
  `build_copy`'s first-batch hand-over (`gpu_backend/join.rs:304-313`, hacks-audit second-pass
  finding 9): the corpus tier runs each of q9's cross joins once because its probes are single
  batches, and this fix relies on that no more than every other cross join does.
- `operator-cases.md:38,60` (`GpuCrossJoin`: "two batches; with a projection"; empty shapes):
  the helper's spec, approved to build. When it builds, "a build side of no columns" and "a
  probe side of no columns" belong on that row, and "projecting to no columns" on `GpuProject`'s
  (`:29`); 3d does not wait for it.

### CPU and GPU agreement

The CPU runs DataFusion's `ProjectionExec` (an empty projection keeps `row_count`) and
`CrossJoinExec` (`cpu_backend/join.rs:111-117`), which broadcasts a zero-column build side as
rows and no columns — `nested-limits`' five green cpu cells and its 20-row result are the proof.
After 3c the device emits the same columns in the same left-major order at every node of the
chain, which is the "Column indexing" invariant the page says nothing checks between nodes; 3d
checks it for this operator.

### Hacks-audit

Neither pass names `project.cpp`, `join.cpp`'s cross product or the placeholder (the first pass
did not read either file past the null-equality region, the second read `expr.cpp`'s
`build_column` only). Nothing to remove. `185-review.md:143` records the placeholder as what
keeps `GpuProject exprs=[]`'s rows and bytes equal on both engines; that stays true.

## 4. Alternatives rejected

- Close as stale, as 56-proposal.md proposes for #63 — the failure recurs at the ticket's line on
  today's tree; a device run after #163 would reopen it with the same text.
- `TableResult` gains a `rows` field and the placeholder goes — general, but every reader of
  `num_rows()` (five stats sites in `node_session.cpp`, the collapse, `slice_handle`, the count
  paths in `aggregate.cpp`, the export) and every zero-column producer must learn a second
  representation; M/L, and it fights cuDF's own spelling where 3c uses it.
- A `bool` flag on `TableResult` beside the column — two spellings of one fact that can
  disagree, plus propagation at the collapse and the slice; the name already rides everywhere
  `column_names` is copied. Keep as the fallback if a reviewer refuses a name-keyed predicate.
- An `EMPTY`-typed placeholder, so the tag is a type no plan can produce — `cudf::concatenate`
  handles it (`concatenate.cu:518-526`) but `cudf::column(column_view)` has no arm for it
  (`column.cu:168-262`, no `void` overload), so the slice's and the merge's owning copies would
  throw. Unproven; INT8 has run on the legacy engine for every count(*) query.
- Write `PlanNode.output_schema` (the field exists, `gpu_plan.fbs:608`; `writer.rs:102` leaves it
  `None`) so the C++ can compare declared widths — every payload's bytes move
  (`recipe-payloads.txt` regenerates under its deliberate variable), and it is #164's
  declared-versus-produced check, not this fix.
- Planner: never project to nothing under a cross join — changes plan shape and ordinals on
  both engines for a shape the CPU already answers, and regenerates goldens for nothing.
- Rust device executor: skip the `CudfCrossJoin` call when the build side is declared zero-column
  with one row (the identity on the probe batch) — dodges the C++ defect, leaves
  `nested-limits`' five-row build side unanswered, and makes the device's call count differ
  from the recipe's.
- Strip by ordinal in the cross join using the fb node's declared widths — `CudfCrossJoin`
  carries nothing (`gpu_plan.fbs:440-443`); adding fields is a schema change.

## 5. Minimum corpus query

Against tpch sf1, the shape without q9's `avg`s so no other ticket stands in front:

    SELECT CASE WHEN (SELECT count(*) FROM nation) >= 0
                THEN (SELECT max(n_nationkey) FROM nation)
                ELSE (SELECT min(n_nationkey) FROM nation) END AS pick
    FROM region WHERE r_regionkey = 1;

Plans at all five modes today and nothing refuses it: three keyless aggregates over `nation`
(25 rows, so one lane at tp4 and no #180-shaped merge), three cross joins with single-batch
probes (so #152's second batch never comes), an `Int64` sink (no #183, no #187), no `avg` (no
#163), and the region side projected to nothing — the same `GpuProject: exprs=[]` q9 has at
`:10175`. The CPU answers 24 (`count(*)` is 25, so THEN, the max key). The device today: the
condition reads the placeholder (0 ≥ 0, true), THEN reads device column 1, the `count(*)` — 25,
an Int64 the unload accepts. **A silently wrong answer**, which is the sharper form of the defect
than q9's refusal: `data_fusion_exact` at every mode goes red on the value alone. After 3c: 24.

The smallest refusing form, two subqueries and no CASE —
`SELECT (SELECT max(n_nationkey) FROM nation) m, (SELECT min(n_nationkey) FROM nation) n FROM
region WHERE r_regionkey = 1` — fails at the unload with `expected Int64 but found Int8`, which is
#183's site and text and would be misfiled there.

Recommended as a corpus line, `tpch/scalar-subqueries.sql`, cpu × 5 `data_fusion_exact`, gpu × 5
`golden_exact`, registry row `tpch,1,scalar_subqueries,…,ok,cross_join corr_subquery,` — it is
the only cell in either corpus that puts a zero-column side under a cross join on a device
before #163 and #187 land. It adds no fb kind or call shape (`CudfCrossJoin`, an empty
`CudfProject`, keyless `CudfAggregate` and the collapse are all in `recipe-payloads.txt`), so
`PAYLOAD_QUERIES` does not change; it appends five sections to the plan goldens and one each to
the cpu, cost and result goldens, authored by the cpu tier, and moves no existing byte.

## 6. Cells re-enabled

- None of `tpcds/q9`'s ten. The cpu cells stay off on #163 at every mode and, by q88/q90/q96's
  precedent, #180 at the three tp4 modes after it. The device cells then meet #187 at the unload
  (five `Decimal128(11,6)` outputs). This fix removes the wall between #163 and #187 — the one a
  run would otherwise report as #63's text at `#128 CudfProject` — and drops `63` from the row.
- `tpch/nested-limits` gpu × 5: stays off on #188, then the scan-arm ticket 188-review F1 asks
  for; 3c is the half of that fix the cross join needs.
- New, if §5's line is added: `tpch/scalar-subqueries` cpu × 5 and gpu × 5, all on.

## 7. Risks and unknowns

- The trace in §2 is by reading, not observed: no device run of q9 on this engine is recorded
  (its cells were never enabled), and the legacy run's text survives only as a line number.
  3d's first run is the observation; if the first case is green before 3c, the mechanism is
  something else and this proposal is wrong.
- `binary_operation(INT8 column, INT64 scalar, GREATER, BOOL8)` at `expr.cpp:616` is assumed
  supported (cuDF's compiled comparisons take mixed integer widths); if it throws instead, the
  failure is one step earlier in the same `CudfProject` and the fix is unchanged.
- The name-keyed predicate: a plan could in principle alias a lone `TINYINT` column
  `__rowcount__` on one side of a cross join and have it dropped. No SQL in either corpus can;
  the flag alternative closes it at the cost of propagation sites.
- 3c's `tile`/`repeat` are read from the 25.10 tree and the 25.02 headers; the `empty_like`
  arms and the overflow check are 25.10 source, assumed unchanged since 25.02 (both functions
  predate 0.20).
- §5's plan shape is predicted from q9's, not rendered: that `r_regionkey = 1` projects region to
  nothing and that the three subqueries join in select-list order. Either differing changes the
  wrong value the device returns today, not whether it is wrong.
- Whether `tpcds/q9` at tp4 really meets #180 is inferred from q88/q90/q96's plan shape; the
  registry keeps `163` alone until a run says.

## 8. Complexity

**S.** Three files under `cpp/src` (~45 lines: a constant and two declarations, a factory and a
predicate, a helper and two includes), one gtest file (~100 lines, three cases), one CSV cell,
one ticket to the archive, two page rows and one sentence, one code comment. No C ABI, no
`.fbs`, no wire bytes, no declared-schema contract; `recipe-payloads.txt` and every existing
golden section are untouched. Cost: one `ctest -L gpu` on shad-gpu. Adding §5's corpus line
makes it M — five plan sections and three execution sections to author on two hosts — and is
the only way a corpus cell proves the fix before #163 and #187 land.
