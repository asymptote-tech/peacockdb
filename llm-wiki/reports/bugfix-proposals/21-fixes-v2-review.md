# Review of 20-fixes-v2.md

Read at master `188c23ce`, read-only. Opened: every file:line the eight S fixes cite, the
M/L fixes' load-bearing cites, the vendored `datafusion-*-45.0.0`, `arrow-*-54.2.1` and
`parquet-54.2.1` crates, the cuDF 25.02 headers, the tpch/tpcds plan and execution goldens,
`recipe-payloads.txt`, `cost-registry.csv`, `corpus_cases.inc`, the four wiki pages, the
hacks audit, the board, the three rejected specs, `empty-build.md`, `typed-nulls.md`,
`declared-schemas.md`, and `10-fixes-v1.md` beside `11-fixes-v1-review.md`. Nothing built or
run; one pyarrow read of `lineitem.parquet`'s footer to check a byte count.

## 1. Verdict

**Needs changes**, all small. The thirteen first-round findings were applied, and the two
applied with a corrected detail are correct (q27 is a `UNION ALL`, not a rollup; `hash-join`
at tp1-single is 194 MB). The eight S fixes are implementable from the text: each minimum
query plans and reaches the code it names. What is wrong is one sequencing contradiction —
the coverage table's landing order puts four fixes that regenerate the plan goldens or edit
`test_plan_executor.cpp` between task 3 and task 11, which the list's own chain rule forbids
— and a handful of wrong sentences: a false "placement divergence" motivation shared by F2
and F8, a misdescribed trigger for ticket 10, three cell misattributions, and two minimum
queries whose "today" claim fails at one mode.

## 2. Findings

### F-1 — important — the coverage table's landing order cannot be executed under the chain rule

Evidence. The chain-placement paragraph (Duplicates of approved tasks, last bullet) says every
fix lands "before task 4, or after task 13 — never between", and that the only work fitting
the pre-task-4 window is "F10 first, then F1/F2/F3/F12's sections — all cpu-only, one regen".
The table then orders: 6 F11, 7 D1, 8 F4, 9 F18, **10 task 11**, 11 F17, **12 task 12**, 13
F13. F11 and F18 regenerate all ten `.plans.txt` and `recipe-payloads.txt`
(`declared-schemas.md:91,297-299` — task 10 moves the same files); D1 edits
`tests/test_gpu_executors/exec.rs` (task 4 moves it); F4 adds gtests to
`cpp/tests/gpu/test_plan_executor.cpp` (`typed-nulls.md:137` — task 11 rewrites `:49-66`).
None of the four can sit between task 3 and task 11, so steps 6-9 must follow steps 10 and
12, not precede them. The totals do not move; the sequence does. F12 is in the window but
its gtest (`customer_scan_plan`, `test_plan_executor.cpp:1094`) and `test_gpu_abi` case
collide with task 11 and task 4 at the rebase level — say so, and drop "F12's sections",
which reads as if only sections land: F12's C++ and its device tests are in the window too.

Correction. Reorder the table: 1-5 as now (F10, F1, F2, F3, F12); one row "tasks 4-13,
task 12 lands q16 × tp4: +3 cpu"; then F11, D1 (conditional), F4, F18, F17, F13, F14,
F5-F9/F15/F16, wall, never. Cumulative 168 / 80 unchanged.

### F-2 — minor — F4's minimum query meets #180 at tp4-single on the CPU today

Evidence. The count subquery is keyless over `nation`, one row group; at tp4-single
`lanes_for` returns 4 under `Batching::Off` (`translator/nodes.rs:383-389`) so the mapping is
`[[[0]],[],[],[]]`, the loader declares `MultipleBatches` (`plan/source.rs:65-67`), and
`aggregate_sequence` inserts the per-lane merge-only `GpuAggregateBatches`
(`translator/aggregate.rs:360-367`). Lanes 1-3 reach `mark_done_and_fetch` with no state
(`cpu_backend/accumulate.rs:372`), the merge's `sum(count)` over nothing is NULL, and
`declared_as` refuses it against `count(*): Int64, nullable=false` — exactly F3's issue. So
"CPU 24 … all five modes … no other ticket in front" holds at four modes today and at
tp4-single only after F3 (step 4, before F4 at step 8). The device answers 25 at all five
(its merge-only node emits nothing, `gpu_backend/accumulate.rs:307-310`).

Correction. State the mode: tp1-single/tp1-rowgroup/tp4-rowgroup/tp4-sized today, tp4-single
after F3. If `tpch/scalar-subqueries` is added before F3, its tp4-single cpu cell carries
`180`.

### F-3 — minor — "After F2 a device tp4 rollup would place subtotal rows by a different gid hash" is false (F2 cells row, F8 issue)

Evidence. F2's own fix removes the grouping id from the emit's key list
(`keys.retain(|k| *k != group.expr().len())`), so after F2 no hasher on either engine reads
the gid; before F2 the CPU refuses `UInt8` in `CpuEmitter::emit` (`cpu_backend/emit.rs:60`,
`spark_partitioning.rs:53-71`) and no device rollup runs at tp4 at all. There is no state in
which the gid's value decides a lane. F8's real observables are the `GROUPING()` value
(F8's minimum query) and the declared `UInt8` against the device's `INT32` (#164,
`declared-schemas`).

Correction. Strike the sentence from both; F8's issue keeps the value/width divergence and
the ordering note "F2 first is cleaner" stays for the reason it gives.

### F-4 — minor — ticket 10 and F8 misdescribe when `GROUPING()` needs shifts

Evidence. `datafusion-optimizer-45/src/analyzer/resolve_grouping_function.rs:210-216`: when
the `GROUPING` arguments are exactly the group-by expressions in order — any count, one key
included — the rewrite is `CAST(__grouping_id AS Int32)`, which `expr.cpp:911-934` casts on
the column path. The `bitwise_and` / `bitwise_shift_*` path (`:218-240`) is taken for a proper
subset or a reordering: `GROUPING(a)` under `ROLLUP(a, b)`, or q70/q86's
`grouping(i_category) + grouping(i_class)`. Ticket 10's substance stands — `build_scalar`
(`expr.cpp:453-497`) has no unsigned arm and `fb_to_binop` (`:498-521`) no shift — but
"`GROUPING(x)` over one key" is the case that works.

Correction. Ticket 10: "`GROUPING()` over a proper subset or reordering of the rollup keys".
F8 cells row: the same.

### F-5 — minor — F18 attributes q14 × tp4 to #189; q14's rollup never shuffles

Evidence. `tpcds.sf1/tp4-single.plans.txt` and `tp4-rowgroup.plans.txt`: the emits that hash
`__grouping_id` belong to q18, q22, q5, q77, q80 only. q14's rollup sits over
`GpuInterleave: lanes=1` (three one-lane nested-loop branches), so its final aggregate is one
lane and `shuffle_below` never sees a hash. Registry row 15 carries `65 97 163`, no `189`.

Correction. F18 alone: 109 cpu (20 × 5 + q14 × 5 + q18/q22 × tp1); F2's later gain 6
(q18, q22 × tp4), not 9. Table totals unchanged.

### F-6 — minor — F16 books zero cells; q15 × tp4 (3 gpu) are candidates once F11, F10 and F16 have landed

Evidence. `tpch.sf1/tp4-single-mini.cpu.txt`, `== q15`: the lower join's probe is one batch
per lane (`batch_rows=[[2484],[2496],[2458],[2562]]`) and the top join's probe is the
one-row `max` (`[[],[1],[],[]]`) over a coalesced build, so no lane sees a second probe
batch and #152 is not on the path. The sink is three strings (F11) and
`total_revenue: Decimal128(38,4)` (no #187). F16's cells row says "none flip" and step 15
books 0.

Correction. F16: q15 × tp4 candidates after F11 + F10; +3 at step 15 (gpu 80 → 83), with
the usual "a run decides".

### F-7 — minor — F1's minimum query answers 50 rows, not 60

Evidence. Five nations per region, `r_regionkey < n_regionkey`: for a nation in region k
there are k smaller region keys, so Σ over 25 nations = 5 × (0+1+2+3+4) = 50. The query
does plan as claimed: `try_pushdown_through_join` refuses (no left column survives,
`projection.rs:738-748`) and `try_embed_projection` embeds `[n_name]`
(`nested_loop_join.rs:566-598`), and `declared_as` refuses "3 columns … 1 field" on the
CPU. Only the count is wrong.

### F-8 — minor — F4 (c) must honour the scan's limit, and should name the metadata call

Evidence. F4 (c) returns `row_count_table(rows)` for an empty projection; F12 (d) caps a
scan's table with `cudf::slice(view, {0, limit})` after `read_parquet`. An early return in
(c) skips (d), so a rows-only scan carrying a limit — `SELECT 1 FROM lineitem LIMIT 10`
plans a zero-column scan with `limit=10` — would answer every row. `nested-limits` cannot
show it (`region`: 5 rows under `limit=23`). The row count is
`cudf::io::read_parquet_metadata(source_info{paths}).rowgroup_metadata()[rg]["num_rows"]`
summed over the row groups the call names (`cudf/io/parquet_metadata.hpp:184,250,270`);
the "(or a one-column read)" alternative decodes a column to count it.

Correction. (c) computes `min(Σ num_rows, limit)` where `limit > 0`, or falls through to
(d)'s cap; one gtest with a zero-column limited scan.

### F-9 — minor — the chain-placement paragraph misfiles F4's collision

Evidence. It lists F4 among the fixes that edit `cpp/src/expr.cpp`; F4 edits
`operators.h`, `project.cpp`, `scan.cpp`, `join.cpp` and `test_plan_executor.cpp` (F4's own
text: "no line collision" with `expr.cpp`). The collision with task 11 is the gtest file.

Correction. Move F4 from the `expr.cpp` list to the `test_plan_executor.cpp` list beside F12.

### F-10 — minor — F5's proposed `extract-year` line is "+5/+5" without the caveat the list states elsewhere

Evidence. `WHERE o_orderkey < 8` prunes `orders` to one row group at plan time, so at
tp4-single the loader maps `[[[0]],[],[],[]]` and the unload takes four lanes, three empty —
the shape the #46 closure calls "the tier's first multi-lane device unload over three
empty lanes — a refusal there is a new ticket". F5's "the only green device cell this fix
produces (5 cpu + 5 gpu)" books the tp4-single gpu cell as green.

Correction. "+5 cpu, +4 gpu, tp4-single a run decides", or carry the caveat.

### F-11 — minor — F3's minimum query text

"12 rows at tp1" reads as the answer; `count(*)` is one row (12 is `store`'s row count,
and `'ese'` matches a subset of it). Say "one row at tp1, the non-nullable NULL refusal at
tp4-single". The query does reach the code: the merge-only node is inserted at tp4-single
because the loader declares `MultipleBatches` whatever the batching (F-2 above), and
`FilterExec` statistics are inexact so `AggregateStatistics` cannot answer it.

### F-12 — minor — F8 is an M task by the list's own banding

`aggregate.cpp`, a new `Shape` variant in `executor_cases.inc` with arms in
`test_cpu_executors.rs` and `test_gpu_executors/contract.rs`, a walk test, three Python
pins, five doc sentences — ten files — and two shad-gpu binaries plus a C++ build. F5 is
banded S with one device run and "M if the corpus query is added"; F10 is "S code, M for
the device run". F8 is the same shape as F10's M. Code S, task M; the split F17/F18
recommend applies.

## 3. First-round findings, as applied

All thirteen were taken, and each is applied where the review asked:

- F-1 → D1 with the three shapes, step 7 conditional, the no-D1 arithmetic below the
  table. Verified the rejection quotes (`archived-tasks.md:161, :286`) and
  `declared-schemas.md:136-137`.
- F-2 → step 7 = 17 (anti_join, semi_join × tp1-single only; the tp4-single emit's
  `[[92337,91970,92436,98520],…]` verified in `tp4-single-mini.cpu.txt`), step 10 = task 11,
  step 11 = 22. Sum 9 + 27 + 17 + 5 + 22 = 80.
- F-3 → step 13 = 10 with q27's conditions; closing line 156 + 15 = 171, 168 + 3, na 90 →
  75. The corrected detail is right: `testdata/tpcds-queries/q27.sql` is a `UNION ALL` of
  three `GROUP BY`s over one CTE, so F2 is not on q27's path.
- F-4 → F9 is M, part C gone, ticket 13 carries it to #199.
- F-5 → "before task 4, or after task 13" with the practical consequence (but see F-1: the
  table was not re-sequenced to match).
- F-6 → F12 lists `:377-378`, `:814`, `:1043`, `:1073`; the two-decisions bullet reduced.
- F-7 → the `drained` sentence, the bug-1 "not a discriminator" line, and the `pulled`
  formula ("three, not two") are all in F12.
- F-8 → F10 carries the tie-order caveat and the transient; the corrected figure is right
  (`tp1-single-mini.cpu.txt:813`, `output_bytes=193539184`, 733 chunks of 8192).
- F-9 → F11 carries the null-slot byte caveat against `common.rs:107-119`.
- F-10 → #47 archives only after `ROLLUP_OVER_LANES` is green on shad-gpu.
- F-11 → F13 says the two rules share a ticket and `build_session_state` and nothing else.
- F-12 → F17 is "M as code, L as a task" with the split.
- F-13 → F12's cells row names the `region` scan failure as ticket 1.

## 4. Grouping check

- F1 (#190) — holds. Cross-join projection (ticket 7) is a separate mechanism.
- F2 (#189) — holds; F-3 is a sentence, not a regrouping.
- F3 (#180, narrows #199) — holds. Verified no keyless aggregate over an empty lane in any
  enabled tp4 execution golden, so "no golden regenerated" holds.
- F4 (#63 + ticket 1) — holds; one placeholder, three sites. Verified an empty projection
  never embeds into a filter or a join (`try_embed_projection` returns `None` on an empty
  index list, `projection.rs:388-390`), so project, scan and cross join are the whole set.
- F5 (#191) — holds.
- F6 (#60 + DESC-nulls filed) — holds. The DESC-nulls defect is real: the comparator
  applies `null_precedence` and then flips for `DESCENDING`
  (`cudf/table/row_operators.cuh:422-437`), and both sites map `nulls_first` to `BEFORE`
  regardless (`sort.cpp:45-46`, `node_session.cpp:314-315`).
- F7 (#57) — holds; the value form survives the simplifier (only search-form with boolean
  branches is rewritten, `expr_simplifier.rs:1383-1410`) and reaches the plan
  (`tpcds.sf1/tp1-single.plans.txt:3854-3855`).
- F8 (#65) — holds; DataFusion folds MSB-first (`aggregates/mod.rs:1274`) into
  `UInt8/16/32/64` by key count (`logical_plan/plan.rs:3223-3233`); the C++ folds LSB-first
  into `int32_t` (`aggregate.cpp:388-392, :414-415`).
- F9 (#173 A + finishing types) — holds.
- F10 (#185) — holds. q19 tp1-single passes today because its join emits one chunk
  (`in_rows=[[485],[128371]] batch_rows=[[121]]`), which is the same rule.
- F11 (#183 + #192) — holds; one option, two symptoms, one measured gate.
- F12 (#186 + #188) — holds. Verified `count(n_name)` over the limited scan is not answered
  from statistics: `AggregateStatistics` runs before `LimitPushdown`, reads
  `GlobalLimitExec::statistics()`, and `Statistics::with_fetch` blanks the column
  statistics (`stats.rs:357-412`), so `Count::value_from_stats` returns `None`
  (`count.rs:321-349`).
- F13 (#23 q27/q72) — holds by ticket; the two rules are unrelated, and the list says so.
- F14 (#62), F15 (#168), F16 (#184 + #95), F17 (#152), F18 (#163) — hold as in v1.
- D1, D2 — decisions, correctly placed.
- Stale closures, walls, duplicates — hold; nothing new found against them.

## 5. Ordering check

The list order stands. The landing order (the table) changes per F-1:

1. F10, F1, F2, F3, F12 — the pre-task-4 window, cpu-only regen, F12's gtest and ABI
   case rebased against tasks 11 and 4.
2. Tasks 4-13 as approved; task 12 lands q16 × tp4 (+3 cpu).
3. F11, then D1 if taken, then F4 — the plan/payload regen and the `test_gpu_executors`
   edits, after task 10 and task 4.
4. F18 (after task 10's golden move), F17 (after task 11 for the Left/Full rows), F13
   (after F18), F14 (after F18), then F5-F9, F15, F16 in any order.
5. F8 before any device rollup cell is enabled; F2 before F8 as the list says.

## 6. Verified

- F1: `cpu_backend/join.rs:139-145` passes `None`; `:302-316` passes the projection;
  `nodes.rs:505-512` keeps `projected(join.projection())`; the regression test's
  `Some(vec![1, 3])` over `[k, label, fk, v]` is `c|20`, `c|21`.
- F2: `shuffle_below` at `aggregate.rs:39-67`; the gid sits at `group.expr().len()`
  (`:264`); `:370` consumes the shuffle; the subset rule at `plan/aggregate.rs:236-256`;
  DataFusion's `FinalPartitioned` distribution is `group_by.input_exprs()` with the gid
  appended (`aggregates/mod.rs:225-243, :817-818`); `recipe-payloads.txt:3343` is q5's
  `#92` with `__grouping_id@2`; the five tpcds and one tpch gid-hashing emits per tp4 file;
  `l_returnflag + l_quantity` column chunks total 5,302,991 bytes, above `SMALL_TABLE_BYTES`
  = 5,242,880; `translated_at_tp4` exists (`translator/tests.rs:46`).
- F3: `mark_done_and_fetch` at `cpu_backend/accumulate.rs:366-383`; `grouped` set at
  `:96`; the device's arm at `gpu_backend/accumulate.rs:307-320`; the existing pin
  `a_global_aggregate_that_received_nothing_still_owes_its_identity_row`
  (`cpu_backend/tests/accumulate.rs:700-748`) is merge-only (`finalize: None`), so F3's
  rewrite is the right target; `NoBuild` → `Draining` at `single_partition.rs:317-329`;
  the scatter drop at `partitioned.rs:381-385`.
- F4: `project.cpp:20-31`; `scan.cpp:37-52` (`projected_names = all_names`, empty);
  cuDF `_columns` is `std::optional<std::vector<std::string>>` (`parquet.hpp:60,345`);
  `join.cpp:390-398`; q9's `GpuProject: exprs=[] schema=[]` under the innermost cross join
  (`tpcds.sf1/tp1-single.plans.txt:10174-10175`); the translator turns a predicate-free
  `NestedLoopJoinExec` (which is what `scalar_subquery_to_join.rs:334` builds — a `Left`
  join with no ON) into `GpuCrossJoin` (`nodes.rs:483-491`), so the minimum query takes
  q9's shape.
- F5: `expr.cpp:659-660`; `infer_expr_type` reads `return_type` at `:395-397`;
  `is_ast_able` refuses scalar functions at `:403-406`; `fb_to_type_id` at `:74-96`;
  the writer at `expr_writer.rs:150-179`; DataFusion types every part but `epoch` as
  `Int32` (`date_part.rs:142-155`).
- F6: `expr.cpp:707-722`; DataFusion's `(x * 10^p).round() / 10^p` (`round.rs:166-170`).
- F7: `build_column_case` throw at `expr.cpp:773-778`; the search-form fold at `:783-796`.
- F8: as in the grouping check; `fb_to_type_id` maps `UInt8..UInt64`; a `UInt8` literal is
  written by `serialize.rs:44` but has no reader in `build_scalar`.
- F10: `declared` at `cpu_backend/join.rs:269-273`, its two callers at `:250` and `:265`;
  `CpuExec::exec` concatenates (`mod.rs:177-205`); the device answers one handle per probe
  call (`gpu_backend/join.rs:164-179`); `corpus_gpu.rs:101-105` asserts the section before
  the result; `nested-limits`' cross join is five chunks with `abandoned=[92]`; `semi-join`
  and `rollup-over-join` at tp1-single carry the 8192 chunking.
- F11: `lib.rs:52` uses `ParquetFormat::default()`; `schema_force_view_types` defaults
  `true` (`config.rs:430`); `as_declared` casts at `source.rs:112-139`.
- F12: `CpuSource::new` and `read_next` never read `node.limit` (`source.rs:37-91`);
  `lanes_for` returns 1 for a limit and `source()` hands the mode's batching to `partition`
  (`nodes.rs:374-396`); `plan/source.rs:21-50` validates nothing about a limit;
  `scan.cpp:61-63` sets `num_rows`, `:77-78` the row groups; `PushDownLimit` keeps the
  logical limit above a scan it pushed into (`push_down_limit.rs:94-109`), so
  `GlobalLimitExec` exists when `AggregateStatistics` runs; the e2e test's `pulled == 2` and
  `most_offered > 2` at `test_cpu_end_to_end.rs:426-526`; `RowGroupMeta { index, rows,
  bytes }`; registry rows 131 and 135.
- F13: `try_add_ordering`'s guard at `properties.rs:2253-2275`.
- Registry rows 6, 55, 81, 89, 91, 97, 111, 134 and `corpus_cases.inc` lines 97, 114, 137,
  206, 218, 219, 251, 267 are the queries the S fixes name.
- Coverage: cpu 156 disabled + 15 na = 171; 10+9+9+2+5+115+3+10+5 = 168; gpu
  9+27+17+5+22 = 80 with no cell counted twice (anti_join/semi_join: 1 at step 7, 4 at
  step 11); na 18 rows × 5 = 90, 15 rows after F13/F14 = 75.
