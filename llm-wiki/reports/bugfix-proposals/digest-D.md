# Digest D — #186, #188, #57, #63, #45, #46

Read at master 188c23ce. Paths relative to `/media/data/peacockdb`; `core/` = `peacockdb-core/src/`.
Every code claim below was opened; where a proposal and its review disagreed, the line cited is the
one that decided it. DataFusion 45 / parquet 54.2.1 read in the cargo registry; cuDF at
`~/cudf` (`git show v25.02.00:` for the 25.02 lines) and the 25.02 headers under
`~/data/miniforge3/envs/rapids-cuda-12.2/include/cudf`.

## #186 — the CPU backend ignores a limit pushed into the scan
- **Status after research:** live — and half of one defect with #188 (same plan shape, each backend
  wrong its own way); the two proposals choose opposite designs and cannot both land (see Contested
  and Cross-ticket).
- **Issue:** `CpuSource::new` copies file/projection/mapping/schema and never `node.limit`
  (`core/executor/cpu_backend/source.rs:37-77`); `read_next` builds the reader with
  `with_row_groups` + `with_projection` only (`:86-91`). The wire sends the field
  (`core/wire/node_writer.rs:93`) and the device honours it per call (`scan.cpp:61-63`). At tp1
  DataFusion's `LimitPushdown` erases the `GlobalLimitExec` (skip 0, fetch into the source), so the
  plan is a bare `GpuUnload` over `GpuLoadParquet(limit=10)` and 6,001,215 rows come back for 10. At
  tp4 the interval lands on the unload and hides it, but the loader still reads 6,001,215 rows for
  ten (`tp4-single-mini.cpu.txt`, `== scan-limit`). Cells off: `tpch/scan_limit` cpu × tp1-single,
  tp1-rowgroup (`corpus_cases.inc:44-46,51`; registry `:135` `186 188`). The small-table case
  reaches the wrong answer at every mode: `SELECT * FROM nation LIMIT 3` at tp4 is
  `Unload(LoadParquet(limit=3))` with no interval (`core/planner/translator/tests.rs:603-615` pins
  the shape).
- **Root cause:** two halves of one design unwritten. Executor half: parquet 54's
  `ParquetRecordBatchReaderBuilder::with_limit` composes with `with_row_groups`
  (`arrow_reader/mod.rs:221`, `apply_range` `:840-870`) and nothing calls it. Planner half:
  `architecture.md:374-379` says a limited scan "plans one lane and one batch"; `lanes_for`
  (`translator/nodes.rs:374-390`) returns one lane and `source()` (`:392-411`) hands the mode's
  batching to `partition`, so the rowgroup modes map 49 batches (`tp1-rowgroup.plans.txt`,
  `scan-limit`) — the sentence is false of the code, and `plan/source.rs:21-50` validates nothing
  about a limit. A CPU fix that only wires `with_limit` per call answers 490 rows at tp1-rowgroup.
- **Fix (as corrected by the review):** (3a) `source()` — `Some(limit)` → `(1, Batching::Off)`,
  `None` → `(lanes_for, batching)`; `batching_for_source` must still be called to advance
  `next_source` (`nodes.rs:360-368`, `pipeline.rs:50-58` counts it); `lanes_for` loses its `limit`
  arm. Take the review's F5 now, not later: map the shortest prefix of survivors whose rows cover
  the limit (six lines in the same arm), so the plan line says what the scan reads and the memory
  estimate does not jump 7.6 MB → 371 MB. (3b) `GpuLoadParquet::validate_schemas_and_partitions`
  refuses a limited scan that is not one lane and one batch; injection never re-cuts a mapping
  (`injection.rs:558-572`) and `rebuild.rs:393`'s two-lane `source(Some(7))` is debug-compared,
  never validated. (3c) `CpuSource` gains `limit: Option<usize>` and `read_next` adds
  `builder.with_limit(limit)`. (3d) the `Some(0)` wire refusal is unreachable (`EliminateLimit`):
  add its `wire/tests.rs` case or drop it. Tests: CPU source unit case
  (`cpu_backend/tests/source.rs`, `loader()` gains a limit arg), validator case, translator case
  over `customer` (proven multi-row-group by `memory_estimation.rs:579-598`), and the e2e inline
  case corrected to `SELECT count(n_name) FROM (SELECT * FROM nation LIMIT 3)` — `count(*)` is
  answered by `AggregateStatistics` (`optimizer.rs`, rule 2, before `LimitPushdown`) from `Exact(3)`
  (`limit.rs:189-197` → `stats.rs:395-405`) and refused as `PlaceholderRowExec` (#158,
  `nodes.rs:166-172`); a column count needs `null_count` Exact, which `with_fetch` blanks
  (`stats.rs:413`). `a_limit_slices_at_most_two_batches_and_stops_the_scan`
  (`test_cpu_end_to_end.rs:426-526`): its `most_offered > 2` guard goes red once both nested-limits
  scans map one batch; move the `pulled` claim onto a filtered variant and assert `pulled ==
  non-empty source lanes` (at tp4-single the unlimited `part` scan is four lanes, two non-empty,
  `nodes.rs:386`; expect `GpuLimit(5,23) → GpuLimit(0,28) → …`). Frozen surfaces: none. Goldens:
  `tp1-rowgroup`/`tp4-rowgroup` plan sections `scan-limit` and `nested-limits` (mapping + memory),
  plus the three single/sized `scan-limit` sections if the prefix mapping is taken; ten
  `.cpu.txt`/`.cost.txt` sections (two tp1 `scan-limit` sections authored for the first time);
  `mini.result.txt` expected unchanged; `recipe-payloads.txt` untouched. `corpus_cases.inc:44-51`,
  registry `:135` → cpu all five, tickets `188`; `architecture.md:374-379` true again;
  `build-test.md` corpus N +2.
- **Contested points:** (1) e2e `count(*)` case — review right, code supports it
  (`aggregate_statistics.rs:113-139` walks through the limit to the Partial agg; `read_table` leaves
  `collect_stat: true`, `datafusion/src/datasource/listing/table.rs:298`). (2) Filtered-variant
  `pulled == 2` at tp4-single — review right (`lanes_for` `Batching::Off => target_partitions`,
  `nodes.rs:386`). (3) 3d unreachable — review right; coding-style forbids untested defensive arms.
  (4) **Design vs #188** — the 188 proposal rejects this planner-forces-one-batch design ("reads the
  whole table into one device table", "unchecked for injected plans"); both objections fall: the
  prefix mapping bounds the decode, and 3b validates every tree `planner::plan` sees
  (`pipeline.rs:37,73`). Neither review read the other proposal.
- **Minimum corpus query:** `SELECT * FROM nation LIMIT 3` — bare-scan shape at all five modes, 25
  rows for 3 on the CPU, `row_groups can't be set along with skip_rows and num_rows` on the device
  (#188). Oracle-comparable form: `SELECT count(n_name) FROM (SELECT * FROM nation LIMIT 3)`. Corpus
  form `tpch/scan_limit` (lineitem, not minimal). Not in testdata as such.
- **Cells re-enabled / next wall:** `tpch/scan_limit` cpu × tp1-single, tp1-rowgroup. Its gpu × 5
  and `nested_limits` gpu × 5 stay behind #188, then #183/#187 (scan-limit) and the zero-column-scan
  wall + join chunking wall (nested-limits, see #188).
- **Overlaps and dependencies:** #188 — same defect, must be one task with one design; #158 — the
  `count(*)` probe is refused there; #142 — a future rebatcher meets 3b's refusal by name;
  `operator-cases-impl.md:1274-1286` builds a four-batch limited scan through a non-validating
  harness and should map one batch.
- **Complexity:** S — four source files under fifteen lines each, ~70 lines of tests, 4–7 plan
  sections and ten execution sections regenerated, no frozen surface. Becomes part of the M task if
  merged with #188.
- **Unticketed defects found:** `architecture.md:374-379` false at the rowgroup modes (drift, fixed
  by the task); `architecture.md:97-100` omits that the small-table rule bites only under batching
  (`nodes.rs:381-389`); `limit_interval`'s dead coalesce arm (`common.rs:137-147`, hacks-audit 4) —
  no change.

## #188 — the device refuses a read with row groups and a limit together
- **Status after research:** live; the C++ half of the #186 defect. Root cause is cuDF's contract,
  not a version quirk.
- **Issue:** `scan.cpp:61-63` sets `set_num_rows(limit)` from `CudfScan.limit`, then `:77-78` sets
  the row groups every `execute_scan_rowgroups` call names; cuDF's
  `parquet_reader_options::set_row_groups` throws `row_groups can't be set along with skip_rows and
  num_rows` when `_num_rows.has_value()` (`~/cudf/cpp/src/io/functions.cpp:805-811`, same setters in
  the 25.02/26.02 headers). The first scan call of any limited-scan plan fails. Cells off:
  `tpch/scan_limit` gpu × tp4-single, tp4-rowgroup, tp4-sized; `tpch/nested_limits` gpu × 5
  (`corpus_cases.inc:47-51,78-79`; registry `:131`, `:135`).
- **Root cause:** the design `architecture.md:374-378` describes ("one lane and one batch make the
  loader's own limit the whole answer") rests on `set_num_rows` per call, and cuDF never allowed it
  beside a row-group list, so even a one-batch plan fails on the device. The recipe publishes one
  `PerBatch` scan call (`wire/attach.rs:100-114`); `GpuSource::new` accepts exactly that recipe
  (`gpu_backend/source.rs:32-44`) and `read_next` calls once per mapping entry (`:63-91`);
  `node_session.cpp:478-480` refuses an empty list, so a list is always named.
- **Fix (as corrected by the review):** the shared core, whichever design wins: `scan.cpp` deletes
  `set_num_rows` and, after `read_parquet` and the `PEACOCK_LOG_SCAN_ROWS` block, caps the table
  read to `limit` rows with `cudf::slice` + `cudf::table(table_view)` (`copying.hpp:512`,
  `table.hpp:77` in 25.02); stats are computed by the callers from the returned view
  (`node_session.cpp:238-242, 502-505`) so the capped batch prices exactly. `0` keeps meaning no cap
  (`gpu_plan.fbs:336`). The 188 proposal's own design then adds: `remaining: Option<u64>` on
  `CpuSource` and `GpuSource`, decremented per batch, `batches.clear()` at zero; a `slice_handle` on
  a later straddling batch via a shared `sliced()` in `gpu_backend/mod.rs` (private, not
  `pub(super)`); the loader recipe gains a bare `PerStraddlingBatch` slice call in `attach.rs`
  (renders in all five tpch plan goldens); `PAYLOAD_QUERIES` (`test_plan_goldens.rs:174`, `[…; 20]`)
  gains `nested-limits` and `recipe-payloads.txt` a section;
  `a_loaders_batches_line_up_with_the_row_groups_that_made_them` (`test_corpus_goldens.rs:289-320`)
  gains a limited-loader arm — `emitted == count || Σ batch_rows == limit` under `early_exit=none`,
  `≤` otherwise (review F5, tighter than the proposal's `<=`); a one-lane validator arm in
  `plan/source.rs`. Tests: CPU source unit (limit 3 over `[[[0],[1,2]]]`), device executor twin
  driven through `GpuBackend::executors_for` (not `Session::scan`, whose `let [call]` panics on two
  calls, `test_gpu_executors.rs:183-186`) with bytes asserted only on a C++-capped batch (limit 3
  over `[[[0,1],[2]]]`) because a Rust-sliced batch is priced without string bytes (hacks-audit 1;
  both fixtures carry a `Utf8` column, `test_gpu_executors.rs:74-79`,
  `cpu_backend/tests/source.rs:31-41`); gtest `ALimitCapsWhatOneCallReturns` (`customer_scan_plan`
  gains a `limit` arg, `CreateCudfScan`'s sixth); `test_gpu_abi` case with `LoadedPlan::new(sql)`
  parameterised. Under #186's design none of the counter, recipe slice, payload section or
  golden-rule arm is needed. Frozen surfaces: none either way. Wiki: `architecture.md:374-378, :763,
  :814, :1043, :1073` under the counter design; only `:374-379`'s clause under the one-batch design.
- **Contested points:** (1) nested-limits' first device wall after the fix — review right: the
  `region` loader is `projections=[] … schema=[]` (`tpch.sf1/tp1-single.plans.txt:172`), the writer
  sends the zero-field output schema and no projection (`node_writer.rs:80-100`), `scan.cpp:37-52`
  passes `.columns({})`, cuDF's `_columns` is `std::optional` (25.02 `parquet.hpp:60,345`) and
  `select_columns` with a present empty list selects nothing (`reader_impl_helpers.cpp:1512+`), so a
  zero-column zero-row table reaches `cudf::cross_join`, which refuses `Left table is empty`
  (`cross_join.cu:45`). The proposal's join-chunking wall is second. (2) Device test bytes — review
  right (fixtures are not Int64-only). (3) Suggested corpus line `live_cpu` — review right:
  `assert_oracle_suits_the_golden` (`corpus_gpu.rs:119-142`) refuses `live_cpu` against a frozen
  section; use `golden_exact`. (4) Minimum query — `nation` is smaller and bare-scan at every mode;
  `customer` rests on the file exceeding DataFusion's 10 MB repartition threshold. (5) **Design vs
  #186** — see #186 and Cross-ticket; the 188 proposal's claim that one batch "would not have saved
  the device anyway" is true and is why the C++ cap is needed under both designs.
- **Minimum corpus query:** `SELECT n_nationkey FROM nation LIMIT 3` (2 KB, one row group, bare scan
  at all five modes): CPU 25 rows at every mode, device first call refused. `SELECT c_custkey FROM
  customer LIMIT 122883` at a rowgroup mode is the later-straddler case under the counter design
  only. Neither in testdata.
- **Cells re-enabled / next wall:** with #186: `scan_limit` cpu tp1 × 2. Refusal removed but still
  off: `scan_limit` gpu × 5 → #183 (five `Utf8View`) and #187 (four `Decimal128(15,2)`) at the
  unload; `nested_limits` gpu × 5 → (1) zero-column scan → cross join refusal (unticketed), (2) CPU
  join returns DataFusion's per-call chunks, device one table (unticketed; #185's shape per
  46-review).
- **Overlaps and dependencies:** #186 (one task); #63's `cross_product` refuses a zero-column side
  by name and is what the scan-arm ticket will feed; #183/#187 next for scan-limit; #185/#152 not
  reached (single-batch probes).
- **Complexity:** M under the counter design (two backends + C++, four test tiers on two hosts, five
  plan goldens, ten execution sections, one payload section). S–M under the one-batch design (C++
  cap + gtest + ABI test; the Rust side is #186's).
- **Unticketed defects found:** a scan projected to no columns reads a zero-column, zero-row table
  on the device (`scan.cpp:37-52`) and the cross join refuses it — needs a ticket; a CPU join
  returns a probe call's output as DataFusion chunked it and the device as one batch
  (`cpu_backend/join.rs:236-251` vs `gpu_backend/join.rs:164-180`) — needs a ticket;
  `architecture.md`'s `CudfCoalescePartitions` row says a single input "passes through" while
  `node_session.cpp:327-329` concatenates one view (63-review F6).

## #57 — value-form CASE produces wrong results on the GPU column path
- **Status after research:** misdiagnosed — real cause: the arm was withheld on a gtest whose Int64
  literals were all built as `0`; the lowering was right and had already run q39 correctly. The
  guard (`cpp/src/expr.cpp:779-780`) is live, so the ticket is a real missing arm, not a wrong
  answer.
- **Issue:** `build_column_case` throws `value-form CASE not supported in column path` whenever
  `c->expr()` is set (`expr.cpp:773-780`); the search-form fold (`:785-803`) works. The form reaches
  the device: the translator forwards the comparand (`translator/expr.rs:86-107`), the writer sets
  `CaseExprNode.expr` (`wire/expr_writer.rs:114-149`), and q39's plan carries it four times per mode
  (`tpcds.sf1/tp1-single.plans.txt:3847+`). Disables nothing today: q39 is `none, none` on #163
  (`corpus_cases.inc:298`, comment `:293-296`); registry `:40` `57 163`. Drift:
  `PlanError::Unsupported`'s doc lists "a value-form CASE (#57)" (`plan/mod.rs:52-54`) and nothing
  refuses it.
- **Root cause:** `ScalarValue` has `is_null` as field 2 (`gpu_plan.fbs:45-62`, inserted by
  7ece0548), so the generated positional constructor is `(fbb, type, is_null, bool_val, int_val,
  uint_val, float_val, …)` (`cpp/build/generated/gpu_plan_generated.h:954-967`).
  `make_int64_literal` calls `CreateScalarValue(fbb, Int64, /*bool_val=*/false, /*int_val=*/val)`
  (`test_plan_executor.cpp:50-57`): `val` lands in `bool_val`, `int_val` stays 0;
  `make_float64_literal` puts its double in `uint_val` (`:59-66`). The removed
  `ProjectValueFormCase` (25bec107) ran `CASE … WHEN 0 THEN 0 WHEN 0 THEN 0 ELSE 0 END` — every row
  0, the commit's "0/null". The removed lowering (comparand once, `EQUAL` per branch, `copy_if_else`
  fold) matches DataFusion's `case_when_with_expr` on all three rules (match, NULL never matches via
  `copy_if_else`'s null-as-false, first wins), and 8f471cc0 — the commit that introduced it —
  records q39's `cov` on a device at 1.0561770587198125 against DataFusion's …123, a stddev ULP.
- **Fix (as corrected by the review):** (A) `expr.cpp` `build_column_case`: delete the throw;
  materialise the comparand once, each WHEN becomes `binary_operation(comparand, when, EQUAL,
  BOOL8)` (scalar fast path for a literal WHEN, mirroring `:609-626`), fold unchanged; ~20 lines.
  (B) `test_plan_executor.cpp`: **after `typed-nulls` lands**, add only the value-form test — `CASE
  (CASE WHEN r_regionkey = 3 THEN NULL ELSE CAST(r_regionkey AS Int64) END) WHEN 0 THEN 100 WHEN 2
  THEN NULL ELSE -1 END` → `[100, -1, NULL, -1, -1]`, `null_count() == 1`. (C) drop the `#57` clause
  from `plan/mod.rs:53`. (D) exec model `Case` gains `comparand`, each arm's condition built as
  `Binary("==", comparand, when)` feeding the existing `fillna(False)` fold
  (`expressions.py:363-366`) — not a raw pandas `==`, which yields `<NA>` on nullable Int64; one
  model test with a null comparand. (E) registry `:40` → `163`; #57 to the archive naming the
  zeroed-literal misdiagnosis and 8f471cc0's q39 run; `build-test.md` C++ 27 → 28, Python 216 → 217,
  total 1569 → 1571. Frozen surfaces: none; no golden moves (q39's payload bytes do not change).
- **Contested points:** (1) Helper repair overlap — review right, and the overlap is larger than the
  review states: `typed-nulls.md` §3 and `typed-nulls-impl.md` Task 1 (Steps 1–5) rebuild
  `make_int64_literal`/`make_float64_literal` through `ScalarValueBuilder` at
  `test_plan_executor.cpp:49-66` **and Step 3 adds `make_null_literal(fbb, type)`** with the same
  body the 57 proposal writes — so after typed-nulls, #57's test-file delta is the new test alone.
  The proposal's 3B.2 (`FilterNation` `0 < rows < 25` → `EXPECT_EQ(rows, 10)`, `:309-310`) is not
  prescribed by typed-nulls — its Step 4 reviews moved assertions, and this one does not go red (10
  rows still satisfies the range) — so the tightening is a one-line addition either task may carry.
  (2) Mode claim — review right: date_dim at tp4-single plans four lanes with three empty
  (`nodes.rs:386`). (3) Cites — review right (`execute_node`'s catch is `gpu_executor.cpp:229-237`;
  `FilterNation`'s assertions `:309-310`).
- **Minimum corpus query:** semantic pin: `SELECT d_date_sk, CASE (CASE WHEN d_moy = 12 THEN NULL
  ELSE d_moy END) WHEN 2 THEN NULL ELSE d_moy END AS moy FROM date_dim WHERE d_year = 2001 AND CASE
  d_moy WHEN 1 THEN 0 ELSE d_moy END > 1` → 334 rows, NULL for February, 12 for December; strict
  minimum `SELECT CASE d_moy WHEN 1 THEN 0 ELSE d_moy END FROM date_dim`. All five modes
  (tp4-single: four lanes, three empty); device fails the first `CudfProject`/`CudfFilter` with the
  throw. Not in testdata.
- **Cells re-enabled / next wall:** none. q39 cpu × 5 on #163; gpu × 5 then #57 (this), then #185 at
  tp1-single and #152 at the other four (predicted from shape, unobserved).
- **Overlaps and dependencies:** `typed-nulls` (board #11, approved) must land first or this carries
  its Task 1; `operator-cases.md:29` plans `a_value_case_agrees` and lands green after this; #198 is
  typed-nulls' own; #63 shares `build_column_case` as a site only.
- **Complexity:** S — `expr.cpp` ~20 lines, one gtest ~50, one doc line, ~10 lines Python, one CSV
  cell, ticket move, three counts; one shad-gpu run of `peacock_plan_executor_tests`.
- **Unticketed defects found:** `typed-nulls.md` and `-impl.md` call the test
  `PlanExecutor.FilterNationByRegion`; it is `PlanExecutor.FilterNation`
  (`test_plan_executor.cpp:277`); a CASE with no ELSE and a `Decimal128` THEN null-fills through
  `make_default_constructed_scalar` at `scale_type{0}` and `copy_if_else` would refuse the type
  (`expr.cpp:790-795`) — no corpus query reaches it; `architecture.md:97-100` small-table sentence
  omits the batching condition.

## #63 — q9 GpuProject copy_if_else size mismatch (CASE over scalar subqueries)
- **Status after research:** live, misdiagnosed in the ticket (real cause: the cross join appends
  the device's `__rowcount__` placeholder column to the plan's columns, shifting every ordinal
  above; the failure is cuDF's type check, not a size check). Settled: 63-review finds the proposal
  sound; 56-proposal §7's "closes with #56, one observation, no code" is wrong and 56-review did not
  contest it.
- **Issue:** `execute_project` with no `exprs` emits one INT8 column `__rowcount__` of the input's
  row count (`cpp/src/operators/project.cpp:20-31`) — the device's only spelling of a zero-column
  table, since `cudf::table::num_rows()` is read off column 0 and `cudf::cross_join` refuses a
  zero-column side (`cross_join.cu:45`). `execute_cross_join` appends both name lists and drops
  nothing (`join.cpp:390-398`). q9's innermost build side is `GpuCoalesceAllBatches: schema=[]` over
  `GpuProject: exprs=[] schema=[]` (`tpcds.sf1/tp1-single.plans.txt:10174-10175`), and fifteen cross
  joins later the top project's `CASE WHEN count(*)@0 > 74129 THEN avg(…)@1 ELSE avg(…)@2 END`
  (`:10144`) reads a sixteen-column table declared fifteen. Disables nothing today: q9 is `none,
  none` on #163 (`corpus_cases.inc:278`, `:270-273`); registry `:10` `63 163`.
- **Root cause:** at `#128 CudfProject`, `is_ast_able` refuses a `CaseExprNode` (`expr.cpp:406`) →
  `build_column_case` (`:773`); ELSE = device column 2 (`avg`, DECIMAL128); cond: `infer_expr_type`
  reads INT8 off the placeholder (`:349-353`), `lt != rt` (`:433`) → column path
  `binary_operation(INT8, INT64 scalar, GREATER)`; THEN = device column 1 (`count(*)`, INT64);
  `copy_if_else(INT64, DECIMAL128, cond)` (`:801`) → `have_same_types` fails — v25.02.00
  `copying/copy.cu:367` is the `CUDF_EXPECTS(cudf::have_same_types(lhs, rhs), "Both inputs must be
  of the same type")` line, the line the parking comment (70e07dfb) recorded. Every `build_column`
  arm answers `table.num_rows()` rows (`:830-862`), so the ticket's "1-row vs other-sized" pair
  cannot be built — 56-proposal is right on that and wrong on what it implies. Nothing checks column
  count or order between device nodes (`produced()`, `gpu_backend/mod.rs:196-212`, prices from the
  declared schema).
- **Fix (as corrected by the review):** (3a) `cpp/src/peacock/operators.h`: `kRowCountColumn =
  "__rowcount__"`, `row_count_table(rows)`, `is_row_count_only(t)` (one column, that name, INT8).
  (3b) `project.cpp:19-30` → `return row_count_table(input.table->num_rows())`; drop the sentence
  "the predicate is also the guard" — nothing else evaluates it. (3c) `join.cpp`:
  `cross_product(left, right)` — first refuse a side with zero columns by name (the scan-arm shape,
  so the message points at `project.cpp` rather than cuDF's `Left table is empty`); one overflow
  check above every arm (`cross_join` gets it today only through `repeat.cu:141-144`, and
  `tile.cu:55-58` has none); both rows-only → `row_count_table(l*r)`; left rows-only →
  `cudf::tile(right, l)`; right rows-only → `cudf::repeat(left, r)`; else `cudf::cross_join` (row
  order identical: `cross_join.cu:59,62` are `repeat`/`tile`). `execute_nested_loop_join`'s
  unconditional arm: leave it — every `GpuNestedLoopJoin` in all ten goldens carries a `filter=`,
  and a literal transcription would leave `all_names` (`join.cpp:416-418`) one longer than the
  columns. Includes `<cudf/reshape.hpp>`, `<cudf/filling.hpp>`. (3d) three gtests in
  `test_plan_executor.cpp`: zero-column build side → 125 rows, one column `n_nationkey`, tile order;
  the mirror with repeat order; two zero-column sides + `count` → 25. (3e) registry `:10` → `163`;
  #63 to the archive with the corrected mechanism; drop #63 from the "#55/#56/#63" class at
  `architecture.md:61-63` and `plan/mod.rs:60-62`; `architecture.md` `CudfProject`/`CudfCrossJoin`
  rows gain the placeholder sentence. Frozen surfaces: none; no golden moves.
- **Contested points:** proposal vs 56-proposal — code supports 63: the placeholder is real
  (`project.cpp:24-30`), the cross join passes it (`join.cpp:394-396`), the plan declares no such
  column (`:10173-10175`). Review's six corrections are all minor and code-supported (overflow arm,
  guard sentence, NLJ names, zero-column refusal, per-file golden counts, `CudfCoalescePartitions`
  copies rather than passes).
- **Minimum corpus query:** silently wrong today: `SELECT CASE WHEN (SELECT count(*) FROM nation) >=
  0 THEN (SELECT max(n_nationkey) FROM nation) ELSE 0 END AS pick FROM region WHERE r_regionkey = 1`
  — CPU 24, device 25 (THEN reads the shifted `count(*)`); the three-subquery form mirrors q9's
  THEN/ELSE. All five modes, no other ticket in front (Int64 sink, single-batch probes, no avg). Not
  in testdata; recommended as `tpch/scalar-subqueries` (five plan, five cpu, five cost, one result
  section).
- **Cells re-enabled / next wall:** none of q9's ten. cpu × 5 on #163, then #180 at the tp4 modes
  (shuffled `count(*)` shape, `tp4-single.plans.txt` q9); gpu then #187 at the unload (five
  `Decimal128(11,6)`). `nested-limits` gpu stays on #188 then the scan-arm ticket, for which 3c is
  the join half.
- **Overlaps and dependencies:** #188 (scan-arm ticket feeds `cross_product`); #56/#55 (class
  sentence only — separate); #164 (declared-vs-produced width check would have caught it); #158/#173
  (the "table out of nothing" family).
- **Complexity:** S for the C++ and gtests (~45 + ~100 lines, one shad-gpu `ctest -L gpu`); M with
  the corpus line (five modes authored on two hosts).
- **Unticketed defects found:** none new beyond #188's scan arm; `architecture.md`
  `CudfCoalescePartitions` row's "passes through" is false (`node_session.cpp:327-329`).

## #45 — q24 GpuHashJoin: 'Unary cast type must be fixed-width'
- **Status after research:** stale (fixed by 257377b0, 2026-06-11, one day after filing); the
  ticket's mechanism (a join-key cast) never existed on either side.
- **Issue:** the cast is in q24's join *residual filter*, not a key:
  `filter=c_birth_country@probe:12 != CAST(upper(ca_country@build:3) AS Utf8View)`
  (`tpcds.sf1/tp1-single.plans.txt:2287`, `:2312`), the only `AS Utf8View` in any golden (two per
  tpcds mode file, none in tpch or `recipe-payloads.txt`). DataFusion 45's `upper` returns `Utf8`
  for a `Utf8View` argument (`datafusion-functions-45.0.0/src/utils.rs:34-70`) and the comparison
  coerces up. Disables nothing: q24's ten cells are on #163 (`corpus_cases.inc:283-288`); registry
  `:25` `45 163`. q24 is in the exec-model corpus.
- **Root cause:** at 2d07e908 `build_column`'s cast arm called `cudf::cast` for every target, and
  `cudf::cast` refuses a non-fixed-width target (`unary/cast_ops.cu:454`). 257377b0 added the
  string→string identity arm, now `expr.cpp:912-934` (`:921-923` returns a STRING input unchanged
  for a STRING target; `:924-925` refuses non-string → STRING), and re-enabled q24 on the legacy
  device tier, where it ran green until 3c0750ee dropped the tier. The commit message's "q24's
  `s_zip = ca_zip` join" attribution is wrong (its own golden has bare keys) and the arm's comment
  carries it (`:916-920`). No key cast is possible: `column_ordinal_of` refuses a non-column key at
  plan time (`translator/common.rs:72-83`), DataFusion wraps expression keys in a projection, and
  `execute_hash_join` accepts `ColumnRef` keys only (`join.cpp:52-63`).
- **Fix (as corrected by the review):** no engine change. (3a) a recipe-walk pin in
  `test_gpu_recipe_walk.rs` beside `INNER_JOIN` (`:630-631`) using an **explicit** cast — `SELECT
  n_name, r_name FROM nation JOIN region ON n_regionkey = r_regionkey AND n_name >
  arrow_cast(upper(r_name), 'Utf8View')` — because #183's fix (`schema_force_view_types = false`,
  183-proposal §3a, accepted by 183-review and digest-B) makes every parquet string `Utf8`, after
  which the implicit coercion emits no cast and the proposal's `text.contains("AS Utf8View)")`
  precondition is red. `ArrowCastFunc::simplify` rewrites to `Expr::Cast` whenever source ≠ target
  (`core/arrow_cast.rs:148-171`); today the plan is byte-identical to the implicit form, after #183
  it is two casts through the same identity arm. Assert the cast on the `GpuHashJoin` line
  specifically; one `HashJoin{Inner}` call in the trail; oracle compare (21 rows — EGYPT, IRAN,
  IRAQ, JORDAN sort below MIDDLE EAST). Add to the cover list (`:816-828`). (3b)
  `cpp/tests/cpu/test_executor.cpp` `AstRouting.IsAstAble` (`:81-119`): a `CAST(c@0 AS Utf8View)`
  over a STRING column is not AST-able. (3c) registry `:25` → `163`; #45 to the archive as Done,
  past tense ("carried"). (3d) `expr.cpp:916-920` comment reworded without a corpus example
  (coordinator's, comment-only); `declared-schemas.md:218` row 11 and
  `declared-schemas-derived.md:101` re-pointed from #45 to a **new ticket** for the non-string →
  STRING refusal (`expr.cpp:924-925`; a user-reachable refusal the CPU answers, so it qualifies);
  `walk-drives-every-plan.md:125` / `-impl.md:106` drop #45 from "four known refusals".
  `build-test.md` "Recipe walk on a device" 10 → 11, total +1. Frozen surfaces: none; no golden
  moves.
- **Contested points:** (1) Pin survives #183 — review right (blocking); code supports it
  (`183-proposal.md:164` predicts q24 "loses its cast"; `string_coercion` `(Utf8, Utf8) → Utf8`).
  (2) Unproven steps — review right: q19's residual compares column against a string *literal*
  (`expr.cpp:626-641` fast path); the column-column string compare (`:644-648`) has no device run in
  this tree either. (3) Semi/anti neighbour message — review right: `build_expr` throws `unsupported
  expression node type` on the `upper` (`:310-312`) before the cast arm. (4) Row 11 — review right
  per the ticket rule in `prompts.md`.
- **Minimum corpus query:** the 3a probe above, device via the walk at `ONE_LANE`; shows nothing
  wrong today (that is the finding). Through the corpus tier it dies at the unload on #183 — a join
  failure would be #45 alive. Not in testdata.
- **Cells re-enabled / next wall:** none now. q24 × 10 on #163; gpu then #152 (store_sales probes,
  four modes) and #183/#187 at tp1-single.
- **Overlaps and dependencies:** #183 — sequence-independent with the `arrow_cast` probe, but the
  archive text and `build-test.md` counts must be written knowing which lands first;
  `declared-schemas` (board #10) row 11 needs the new ticket number before it builds; #55-proposal
  asks the same edit of `walk-drives-every-plan*.md`.
- **Complexity:** S — one Rust test (~30 lines) + list entry, one C++ CPU-tier case (~20), one CSV
  cell, ticket move, two spec lines, one comment; one shad-gpu walk run and one `ctest -L cpu`.
- **Unticketed defects found:** the device refuses `CAST(<non-string> AS VARCHAR)` the CPU answers
  (`expr.cpp:924-925`) — file it; a LeftSemi/LeftAnti/LeftMark residual filter carrying any scalar
  function goes to `build_expr` unconditionally (`join.cpp:98`, `:233`) and throws on a device — no
  corpus query, worth an `operator-cases.md` line.

## #46 — q61 GPU: 'promotions' sum subtree returns the wrong value
- **Status after research:** stale (fixed by d3c92de5, 2026-06-11, the day after filing): the
  recorded 2855378.83 is q61's `promotions` with a null-propagating OR, to the cent.
- **Issue:** ticket (`tickets.md:118-122`) records device 2855378.83 vs CPU 2894907.87
  (`mini.result.txt:4704`), `total` right. Disables nothing on its own: q61 gpu × 5 are on `46 152
  187` (registry `:62`), the batch comment names #152/#183/#185 and never #46
  (`corpus_cases.inc:223-228`); tp1-single was run in T19 and refused at the unload by #187
  (`archived-tasks.md:272-281`); the other four modes stop on #152.
- **Root cause:** at 2d07e908 the column path used `LOGICAL_OR`; the promotion predicate `(dmail='Y'
  OR email='Y') OR tv='Y'` is not AST-able (string literal, `expr.cpp:415-420`) and took that path,
  so a NULL channel dropped the row. Three promotions (15, 71, 196) survive only under three-valued
  OR; 158 pass the SQL filter, 155 the null-propagating one — DuckDB over `tpcds.sf1` reproduces
  2894907.87 and 2855378.83 exactly (review re-ran the arithmetic). d3c92de5 moved both evaluators
  to `NULL_LOGICAL_AND/OR` (`expr.cpp:118-119` AST, `:513-514` column path) and re-enabled fourteen
  queries; q61 was not on the list because its device line carried #46's comment. Nothing
  promotions-only remains in the subtree (six Inner joins with `UNEQUAL`, a keyless decimal `sum`
  via one-constant-key groupby, `aggregate.cpp:278-291`).
- **Fix (as corrected by the review):** Part A: close #46 — `tickets.md:118-122` out, Contents `:18`
  17 → 16; archive entry naming d3c92de5 and the three promotions; registry `:62` → `152 187`. Part
  B: re-pin three-valued OR on a device — `testdata/tpcds-queries/filter-or-nulls.sql` (`SELECT
  p_promo_sk FROM promotion WHERE p_channel_dmail = 'Y' OR p_channel_email = 'Y' OR p_channel_tv =
  'Y'`, 158 rows, Int64 only) **plus** an AST-path twin (`WHERE p_start_date_sk = 2450347 OR
  p_end_date_sk = 2450797` — promotion 71 has a NULL end, 58 a NULL start; Int64 vs Int64 literal is
  AST-able, so it pins `fb_to_ast_op`'s `NULL_LOGICAL_OR`, which the string query never reaches).
  Order: SQL files, `corpus_query!` lines (cpu × 5, `data_fusion_exact`, `golden_exact`), regen
  cpu-tier goldens, **run the device cells on shad-gpu**, then write the registry rows with the gpu
  cells as proven — `registry.rs:274-283` asserts a row with any `disabled` cell names a ticket, so
  "gpu none, no ticket" is red on the rust-only tier. Goldens: additive sections only (five plan,
  five cpu, five cost, one result per query); `recipe-payloads.txt` unchanged. Frozen surfaces:
  none.
- **Contested points:** (1) "q61's tp1-single cell will settle the value after #187" — review right,
  code supports it: `gpu_case` runs `assert_section` before `assert_result`
  (`corpus_gpu.rs:101-111`), the section is a byte compare, and q61's section carries DataFusion's
  per-call join chunking (`tp1-single-mini.cpu.txt:2735` `batch_rows=[[803,756]]` for one probe;
  `cpu_backend/join.rs:236-251` returns the chunks, `gpu_backend/join.rs:164-180` one batch per
  probe) — the cell lands on #185's shape, the value is never compared. (2) Interim registry state —
  review right (`registry.rs:279`). (3) tp4-single "one lane and one batch" — review right:
  `promotion` at `Off` batching is four lanes, three empty (`nodes.rs:386`), and this cell would be
  the tier's first multi-lane device unload. (4) Part B pins one evaluator — review right, the AST
  twin is needed.
- **Minimum corpus query:** the string OR above (device; 158 correct, 155 under the #46 mechanism,
  missing 15/71/196) and its AST twin. Neither in testdata; both proposed as custom tpcds corpus
  lines. q61 itself can be value-checked only out of band (a hand-driven session).
- **Cells re-enabled / next wall:** Part A none; q61 gpu tp1-single → #187 then #185's shape; other
  four → #152. Part B: five new cpu cells and up to five device cells per query (tp4-single's
  empty-lane unload is the one to watch — a refusal there is a new ticket).
- **Overlaps and dependencies:** #185 — the 46-review reads #185's `in_rows` mismatch as this same
  CPU-chunking, not a merge miscount (cross-ticket); #187/#152 own q61's cells; `executor_cases.inc`
  deliberately untouched (a NULL row would move every contract case).
- **Complexity:** S — no code; ~20 doc/registry lines, two SQL files, two corpus lines, two registry
  rows, additive goldens; one shad-gpu run required before the rows can be written.
- **Unticketed defects found:** the device tier has no NULL-bearing disjunction anywhere (tpch has
  no NULLs; `executor_cases.inc` `INPUT` has none; the walk is tpch-only; gtests read
  `tpch.minimal`) — a refactor of `expr.cpp:513-514` or `:118-119` goes green; Part B is the pin. A
  lane that never calls the unload on a device (tp4-single small tables) is unexercised — Part B's
  run says whether that needs a ticket.

## Cross-ticket observations
- **#186 and #188 are one defect with two incompatible proposals.** Both agree on the shared core
  (CPU `with_limit`; C++ drops `set_num_rows` and caps the table read with `cudf::slice`). They
  split on where "across batches" lives: 186 puts it in the plan (one lane, one batch,
  validator-pinned; `architecture.md:374-379`'s own rule) and 188 in the executors (a `remaining`
  counter on both backends, a `slice_handle` recipe arm, a payload section, a golden-rule exception,
  six wiki sentences rewritten). Neither review read the other proposal. The code supports both; 186
  + the review's prefix-of-survivors mapping is the smaller mechanism (no counter, no recipe change,
  no `slice_handle` pricing hole on the loader path, no double-count pattern from hacks-audit 8) at
  the price of 4–7 plan sections and the e2e test restructure (186-review F2); 188 keeps every
  plan-tree line and the e2e test but adds machinery. The consolidator must pick one and merge the
  two into a single task.
- **An unticketed "CPU join returns DataFusion's per-call chunks, device one batch" wall is named
  three times**: 188-proposal §6 (nested-limits' cross join, proposes a ticket), 46-review F1 (q61
  after #187 lands on it), and 46-review's aside that #185's `in_rows` numbers (q96 `[[1]]` against
  34) are this chunking rather than a merge miscount. If the group-C digest of #185 agrees, #185 is
  misdiagnosed and the fix is a `concat_batches` in `CpuJoin::probe_and_fetch`
  (`cpu_backend/join.rs:236-251`), which also brings the CPU to `architecture.md`'s
  one-batch-per-call rule — and it moves every cpu golden section with a multi-chunk join.
- **A second unticketed wall, the zero-column table on a device**, spans #188 (nested-limits'
  `region` scan projected to nothing reads zero columns and zero rows, `scan.cpp:37-52`, and
  `cudf::cross_join` refuses it) and #63 (the project arm already spells it as `__rowcount__`; the
  corrected `cross_product` refuses the scan's unspelled form by name). One ticket, two arms; #63's
  fix is the join half and the scan half should emit `row_count_table(rows)`.
- **#57 and typed-nulls collide on `test_plan_executor.cpp:49-66`**: the helper repair,
  `make_null_literal` and the `FilterNation` count move are typed-nulls Task 1; #57 must land after
  it (or as its child) and then adds one test. Sequencing is the whole of the risk.
- **#45 and #183 collide on the corpus's only string-targeted cast**: #183's fix deletes the shape
  #45's pin was written against; the `arrow_cast` probe is what makes the order irrelevant.
- **Three legacy tickets are ticket-hygiene, not fixes**: #45 (Done, 257377b0), #46 (Done,
  d3c92de5), and #57's *recorded wrong answer* (a zeroed gtest literal) — each survived because the
  legacy device tier's green cells were deleted with 3c0750ee and nothing re-read the number in the
  registry. #63 is the exception in this group: still live, and misdiagnosed rather than stale.
- **Small-table rule drift** noticed by three reviews (57 F3, 46 F3, 186 F2):
  `architecture.md:97-100` says a small table "drops to one lane even at tp4"; `lanes_for` applies
  it only when batching is on (`nodes.rs:381-389`), so tp4-single plans four lanes with three empty
  for every small table. One sentence to correct.
- **Ticket-text drift**: `00-tickets.md:59-60` name the wrong file for #63 (the cross join, not the
  CASE lowering) and a non-existent mechanism for #45 (a filter cast in `expr.cpp`, not a key cast
  in `join.cpp`); `plan/mod.rs:52-54` lists #57 as a plan-time refusal it is not;
  `architecture.md:61-63` and `plan/mod.rs:60-62` file #63 with the #55/#56 phase class it does not
  belong to.
