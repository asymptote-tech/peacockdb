# #63 — review of 63-proposal.md

Read at master 188c23ce, nothing built or run. cuDF checked at `third_party/cudf` (25.10) and
`git -C ~/cudf show v25.02.00:…` (the checkout itself sits at 25.06a-397, so every 25.02 cite
below was read through `git show`, not the working tree).

## 1. Verdict

**Sound.** The mechanism is real and reproducible by reading: the device's `__rowcount__`
placeholder rides through `cudf::cross_join` into a table one column wider than the plan
declares, the CASE at q9's top project reads the shifted ordinals, and `copy.cu:367` at
v25.02.00 is the type check, not the size check. 56-proposal.md §7 is wrong about #63 and
56-review.md did not contest it (no `#63` in that file); the consolidator should take this
proposal's reading. Six minor corrections, none blocking, none important; the fix as written
re-enables no existing cell and says so.

## 2. Findings

1. **`cross_product`'s `tile` arm loses the overflow refusal `cross_join` has today** — minor.
   `cudf::cross_join` calls `detail::repeat` first (`cross_join.cu:59`), and `repeat.cu:141-144`
   (25.02 and 25.10) refuses `rows > max / count`; `tile.cu:55-58` has no such check and
   multiplies `in_num_rows * count` in `size_type`. 3c checks overflow only in the
   both-rows-only arm, so the `l_rows_only` arm is the one place a product past 2³¹ rows
   stops being a named refusal. Correction: hoist one check above all four arms —
   `if (l > 0 && r > max / l) throw …` — and drop the per-arm copy.

2. **"the predicate is also the guard that would catch the next such operator" (3b) is not
   true** — minor. Nothing outside `cross_product` evaluates `is_row_count_only`, and no site
   asserts a `__rowcount__` column is alone in its table; an appending operator that forgot
   to drop it would pass exactly as the cross join does today. Correction: delete the
   sentence. (A real guard would be one line where `NodeSession::execute_node` registers an
   output — throw if `__rowcount__` sits beside another column — but that is scope past the
   ticket; say the predicate detects, not guards.)

3. **The nested-loop instruction is incomplete** — minor (unreachable today). 3c says the
   unconditional arm "should call `cross_product` after its existing LEFT-with-empty-right
   throw". `execute_nested_loop_join` keeps `all_names` built from both sides at
   `join.cpp:416-418` and `full_table` as a `unique_ptr<cudf::table>` (`:420`), and the
   projection tail at `:522-534` indexes `all_names[idx]`; assigning only the table leaves the
   names one longer than the columns. The compiler stops the literal transcription
   (`TableResult` ≠ `unique_ptr<table>`), so the developer will notice. Every
   `GpuNestedLoopJoin` in all ten plan goldens carries a `filter=` (grep: 12 tpch, 18 tpcds,
   zero without), so the arm is unreachable from today's planner. Correction: either leave
   `execute_nested_loop_join` alone and say why in 3c, or spell out
   `auto crossed = cross_product(std::move(left), std::move(right)); full_table =
   std::move(crossed.table); all_names = std::move(crossed.column_names);`.

4. **`cross_product` should refuse a zero-column side by name** — minor. The fallthrough
   for a side with no columns at all — the scan arm 188-review F1 describes,
   `scan.cpp:45-52` — is still `cudf::cross_join`, whose message is "Left table is empty"
   with no node, no operator and no pointer at the placeholder rule. Correction: before the
   arms, `if (left.table->num_columns() == 0 || right.table->num_columns() == 0) throw
   std::runtime_error("CudfCrossJoin: a side with no columns reached the cross product; the
   device spells a zero-column table as one `__rowcount__` column (project.cpp), and this
   input was not built that way")`. One line, and it names the fix for the scan ticket
   rather than cuDF's boilerplate.

5. **§5's query is not minimal, and its golden arithmetic undercounts** — minor. Two
   subqueries give the same silently wrong answer with one cross join fewer:
   `SELECT CASE WHEN (SELECT count(*) FROM nation) >= 0 THEN (SELECT max(n_nationkey) FROM
   nation) ELSE 0 END AS pick FROM region WHERE r_regionkey = 1` — device table
   `[__rowcount__, count(*), max]`, ELSE a literal Int64 column, cond reads INT8 0 ≥ 0, THEN
   reads column 1 = 25, CPU 24. The three-subquery form mirrors q9's THEN/ELSE shape, which
   is a fair reason to keep it; say so. Separately, "one each to the cpu, cost and result
   goldens" is per file, not per section: `<mode>-mini.cpu.txt` and `.cost.txt` are one file
   per mode (build-test.md, Golden files), so a new corpus line appends five cpu sections,
   five cost sections and one result section.

6. **Two citations are imprecise** — minor. (a) The §2 trace calls `#3
   CudfCoalescePartitions` a "passthrough" at `node_session.cpp:285`; `:285` copies
   `owned[0].column_names`, and the table is `cudf::concatenate(views)` over one view
   (`:327-329`) — same columns, same names, a copy rather than a pass. architecture.md's
   `CudfCoalescePartitions` row says "a single input has nothing to collapse and passes
   through", which the code does not do either; not this fix's sentence to correct, but do
   not cite it as evidence. (b) hacks-audit is cited as `gpu_backend/mod.rs:178` for the
   unload's type refusal; on this tree it is the `concat_batches(&self.schema, …)` at
   `mod.rs:174-179`, whose text is "the exported stream is not the sink's rows: …", which is
   where §5's two-subquery-no-CASE form would land.

## 3. Claims verified

- **The producer.** `cpp/src/operators/project.cpp:20-31`: `exprs()` null or empty → one
  INT8 `make_column_from_scalar` of `input.table->num_rows()` named `__rowcount__`. The
  writer emits `exprs: Some(empty)` for `GpuProject: exprs=[]` (`wire/node_writer.rs:127-166`),
  so the arm is what runs; the recipe golden confirms `#2 CudfProject` is called per batch
  (`tpcds.sf1/tp1-single.plans.txt:10243+34`).
- **The consumer.** `join.cpp:390-398`: `cudf::cross_join(left, right)`, names of left then
  right appended, nothing dropped. `CudfCrossJoin` carries nothing (`gpu_plan.fbs:440-443`).
- **The plan declares no placeholder.** `tp1-single.plans.txt:10174-10175`
  `GpuCoalesceAllBatches: schema=[]` over `GpuProject: exprs=[] schema=[]` over the reason
  filter; `:10173` `GpuCrossJoin: schema=[count(*):Int64]`; `:10144` the CASE reads `@0/@1/@2`
  against a declared fifteen-column table. Nothing checks column count or order between
  nodes on the device — `produced()` (`gpu_backend/mod.rs:196-212`) prices from the
  declared schema and never reads the table; the unload's `concat_batches` is the first
  reader and it sits above the CASE. So the CASE sees the shift; the unload catches it only
  for a query with no expression above the join (a bare `ColumnRef` project, which fails
  there as "expected Int64 but found Int8" — #183's site and text, as §5 says).
- **The failure is the type check.** `git show v25.02.00:cpp/src/copying/copy.cu` lines
  362-368: `:362` mask size, `:365-366` `lhs.size() == rhs.size()`, `:367-368`
  `have_same_types`. `__LINE__` inside a multi-line macro invocation is the macro-name line
  under gcc (checked with `gcc -E` on a three-line file in the scratchpad), so a
  `CUDF_EXPECTS` reported at 367 is the type check; under the closing-paren convention 367
  would be no check at all. Either way it is not the size check the ticket guessed.
- **The trace at `#128`.** `expr.cpp:403-451` `is_ast_able` returns false for a
  `CaseExprNode` (`:406`) → `build_column_case` (`:773`); ELSE built first (`:788-789`);
  `cond`: `infer_expr_type` reads the device column's type (`:349-353`, INT8) against the
  Int64 literal, `lt != rt` (`:433`) → column path → `build_column_binary` (`:609-616`)
  `binary_operation(INT8 col, INT64 scalar, GREATER, BOOL8)`; `then` = device column 1 =
  `count(*)` INT64; `copy_if_else(INT64, DECIMAL128, cond)` (`:801`) → type error. The
  single-WHEN fold means it is the first and only `copy_if_else`.
- **Every `build_column` arm answers `table.num_rows()` rows** (`expr.cpp:830-834`,
  `:839-857`, `:861-862`, `:579-632`, `:797-802`), so 56-proposal's "the size pair cannot be
  built" is right and beside the point.
- **The legacy engine had the same shape.** `git show 70e07dfb:cpp/src/plan_executor.cpp`
  `:1153-1166` the same placeholder, `:2046-2049` the unconditional NLJ lowered to
  `cudf::cross_join`; the parking comment at
  `70e07dfb:peacockdb-core/tests/test_gpu_executor.rs:375-379` says `copying/copy.cu:367`.
- **The placeholder is needed and the count paths read it right.** `cudf::cross_join`
  refuses a zero-column side (`cross_join.cu:45-46`, both versions);
  `aggregate.cpp:196-204` materializes `count(1)`'s literal to `tv.num_rows()` rows via
  `build_column`, `:271-275` counts `size − null_count`, `:207` and `:455` read `column(0)`
  for the no-arg form. `slice_handle` keeps names (`node_session.cpp:534`); `CudfUnion`
  would carry a placeholder too (`union.cpp:37-56` casts `min(cols, fields)` = 0 columns,
  then concatenates) — so the cross product and the NLJ's cross product are the only
  appending operators, as claimed.
- **q9's is the only zero-column table under a join in either corpus.** Every other
  `GpuProject: exprs=[]` in `tpcds.sf1/tp1-single.plans.txt` (lines 3715, 9554, 9693, 9712,
  9731, 9750, 9769, 9788, 9806, 9823, 10180, 10193, 10206, 10219, 10232, 10455, 10472,
  10993) sits directly under `GpuAggregate: aggs=[count(1) …]`; the only other `schema=[]`
  node is nested-limits' region scan (`tpch.sf1/tp1-single.plans.txt:171-172`), the scan
  arm 188-review F1 owns. q88/q90/q96's cross joins pair one-column `count(*)` results and
  never see a placeholder, which is why q90 reached #187 on a device
  (`cost-registry.csv:91`).
- **`tile`/`repeat` are what `cross_join` builds.** `cross_join.cu:59` `repeat(left, R)`,
  `:62` `tile(right, L)`; `tile.cu:55` and `repeat.cu:141` return `empty_like` for a zero
  count or zero rows, matching `cross_join.cu:49-57`; public signatures with stream/mr
  defaults exist at v25.02.00 (`reshape.hpp:77-81`, `filling.hpp:150-154`). Row order
  therefore matches `cross_join` minus the dropped column.
- **The tests are buildable as described.** `WholePlan` (`test_plan_executor.cpp:182-212`)
  numbers post-order and runs children first; `ScanNationProjected` (`:246`),
  `AggregateCount` (`:506`, `args=0` → `column(0)`), `get_scalar_value<int32_t>` (`:119`),
  and an existing test reading nation keys as int32 (`:860`). Tile order `i % 25` and repeat
  order `i / 5` are as asserted. No C++ test touches `cross_join` today (grep of
  `cpp/tests/`), and neither enabled device cell (q6, q19) has a cross join or an empty
  project, so the change can regress nothing enabled.
- **Registry, corpus, tickets.** `cost-registry.csv:10` carries `63 163`;
  `corpus_cases.inc:278` `none, none` with the comment at `:270-273` naming #163 alone;
  `registry.rs:272-281` requires a ticket on a row with disabled cells, which `163` keeps;
  `tickets.md:19` and `:315-319`; `architecture.md:62` and `plan/mod.rs:61` file #63 with
  #55/#56 under re-derived coercions, which it is not. `PlanNode.output_schema` is
  `gpu_plan.fbs:608` and `writer.rs:102,131` leave it `None`. The recipe walk refuses
  `CrossJoin` at `test_gpu_recipe_walk.rs:789`. `operator-cases.md:38,60` are the rows
  named. `185-review.md:143` says what the proposal quotes.
- **CPU side.** `arrow-select-54.2.1/src/concat.rs:270-277` sums row counts over an empty
  schema; `cpu_backend/join.rs:111-120` is DataFusion's `CrossJoinExec`; nested-limits is
  enabled cpu × 5 (`corpus_cases.inc:87`, registry `:131`).
- **tp4 prediction.** `tpcds.sf1/tp4-single.plans.txt` (q9 section, +28..+50) shows the
  `count(*)` chain `GpuAggregate(4 lanes) → GpuAggregateBatches(4) → GpuMergePartitions →
  GpuAggregateBatches` — #180's shuffled-count shape; and reason at
  `partition_groups=[[[0]],[],[],[]]` under `GpuMergePartitions → GpuCoalesceAllBatches`, so
  the placeholder still arrives as one batch. Shape confirmed, outcome unobserved, as §7
  says.

## 4. Corrected proposal

Only 3c and 3b change; everything else stands.

### 3b — one sentence

Drop "so the predicate is also the guard that would catch the next such operator". The
predicate detects a rows-only side; nothing asserts the invariant elsewhere.

### 3c — `cross_product`

```cpp
static TableResult cross_product(TableResult left, TableResult right) {
  if (left.table->num_columns() == 0 || right.table->num_columns() == 0)
    throw std::runtime_error(
        "CudfCrossJoin: a side with no columns reached the cross product; the device "
        "spells a zero-column table as one __rowcount__ column (project.cpp) and this "
        "input was not built that way");
  const auto l = left.table->num_rows();
  const auto r = right.table->num_rows();
  if (l > 0 && r > std::numeric_limits<cudf::size_type>::max() / l)
    throw std::runtime_error("CudfCrossJoin: the product exceeds cuDF's column size");
  const bool l_rows_only = is_row_count_only(left);
  const bool r_rows_only = is_row_count_only(right);
  if (l_rows_only && r_rows_only) return row_count_table(l * r);
  if (l_rows_only) return {cudf::tile(right.table->view(), l), std::move(right.column_names)};
  if (r_rows_only) return {cudf::repeat(left.table->view(), r), std::move(left.column_names)};
  auto out = cudf::cross_join(left.table->view(), right.table->view());
  auto names = std::move(left.column_names);
  names.insert(names.end(), right.column_names.begin(), right.column_names.end());
  return {std::move(out), std::move(names)};
}
```

The overflow check is one, above every arm, matching what `cross_join` refuses today
through `repeat`. The zero-column refusal names the scan-arm ticket's shape rather than
leaving cuDF's "Left table is empty".

`execute_nested_loop_join`: leave it as it is, and say in 3c that its unconditional arm is
unreachable (every `GpuNestedLoopJoin` in the goldens carries a filter; architecture.md,
"Cross join vs nested-loop join") so the placeholder cannot reach `join.cpp:432`. If it is
changed anyway, `all_names` must be taken from the `TableResult` `cross_product` returns.

### 5 — corpus line

Keep the three-subquery form if the aim is q9's shape; otherwise the two-subquery form in
finding 5 is the minimum that is silently wrong on a device. Either way: five plan
sections, five `.cpu.txt` sections, five `.cost.txt` sections, one `mini.result.txt`
section.

## 5. Complexity

**S** as proposed, for the C++ and the gtest; **M** with the corpus line. Agrees with §8.
The fix cannot move a golden or an enabled cell, and its whole cost is one gtest run on
shad-gpu plus, with the corpus line, a cpu-tier authoring run at five modes.
