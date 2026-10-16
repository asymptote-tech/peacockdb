# Corpus cell estimate for the join-rewrite chain

Prediction, made before any of the chain is built, of the tpch and tpcds sf1 corpus registry after
the whole chain lands: `duckdb-oracle`, `pbench`, `repartition-keys`, `refcounted-scatter`,
`exit-copies`, `join-session-cpp`, `join-backend`, `verify-26.02`. Read-only analysis of master
`fc3b0b55` plus the uncommitted `llm-wiki/tasks/join-rewrite-design.md` and `pbench.md` as of
2026-10-07. Meant to be committed as a wiki report and scored against the real outcome, so every
prediction below is a cell count or a named cell.

Scope: the 120 `corpus_query!` lines (`peacockdb-core/tests/common/corpus_cases.inc`; 39 tpch,
81 tpcds) and their `testdata/cost-registry.csv` rows, five modes, cpu and gpu: 600 cells per
engine. The 18 registry rows with no corpus line (13 window queries on #32/#143, q70/q86, q27/q72
on #23, q28 on #62) are untouched by the chain and stay `na`/`disabled`. pbench's own rows are new
and not counted here.

Labels. **Verified** means read in code, goldens or parquet footers on this host. **Inferred**
means reasoned from those and not run. Nothing here was built or run. Every gpu prediction is about
the registry's device column, which is shad-gpu on cuDF 25.02: a gpu cell turns on only after a
shad-gpu run at its mode. CI's "26.02" leg is the 25.10a image and builds without running the
device tiers, so it cannot turn a cell on or off. `verify-26.02` runs on a temporary 26.02 host and
writes benchmark results; it does not change registry cells.

Categories, per off cell: **FLIP** (every blocker is in the chain and nothing else is expected),
**NEXT** (the in-chain blockers go, an out-of-chain defect is expected next), **STAY** (an
out-of-chain ticket already on the row keeps it off), **STALE** (a sub-case of STAY: the row's
only out-of-chain tag is the closed #183 or #187, meaning "never run past the string class"; the
design's 5.7 rule keeps those cells off, though their real blocker was #152). **RISK** is for cells
on today. Confidence: H about 0.9, M-H 0.8, M 0.7, M-L 0.5 that the stated outcome happens.

## 1. Totals

Today (verified, registry = inc, 0 mismatches): cpu 551 on / 49 off; gpu 26 on / 574 off.

| engine | on today | FLIP | NEXT | STAY | of which STALE | on after the chain (categorical) | on after (probability-weighted) |
|---|--:|--:|--:|--:|--:|--:|--:|
| cpu | 551 | 38 | 0 | 11 | 0 | 589 | about 585 |
| gpu | 26 | 431 | 13 | 130 | 41 | 457 | about 332 (26 + 306) |

- cpu FLIP 38: #190 ×20 (tpch q11, q22; tpcds q24, q54, all five modes), #189 ×15 (tpch
  rollup-over-join, tpcds q5, q18, q22, q80 at 4s/4r/4z), #212 ×3 (tpcds q77 at 4s/4r/4z). 35 at
  H, q77's 3 at M-H.
- cpu STAY 11: tpcds q88, q90, q96 at 4s/4r/4z on #199 (the cpu's NULL `count(*)`, filed as #180);
  tpch scan-limit at 1s/1r on #186.
- gpu FLIP 431 = 416 cells of the 87 in-chain-only rows + 15 cells of rows that also carry an
  out-of-chain tag where that tag does not apply to the mode: tpcds q88/q90/q96 at 1s/1r (6; #199
  is tp4-only), the 1s cell of tpch q4, q15, cross-join, nested-loop-left-join, semi-join and
  anti-join (6; their documented 1s blocker was #220), tpch q15 at 4s/4r/4z (3; #95). By
  confidence: H 11, M-H 93, M 292, M-L 35.
- gpu NEXT 13: tpch rollup-over-join ×5 (#65), tpcds q78 ×5 (#60), tpcds q32 at 4s/4r/4z (#199).
- gpu STAY 130: 89 on open out-of-chain tickets (#191 tpch q7/q8/q9 15; #65 tpcds q5, q14, q18,
  q22, q77, q80 30; #56 q2 5; #57 q39 5; #55 q66 5; #205 q17 5; #199 q88/q90/q96 tp4 9; #168
  mixed-join 5; #186 nested-limits and scan-limit 10) and 41 STALE (§6).
- Process ambiguity, the one that moves the gpu number most: if `join-backend` reads "keep those
  cells off" (design 5.7) as whole rows for the six #183 join rows, the 9 FLIP cells of q4, q15,
  cross-join, nested-loop-left-join, semi-join, anti-join become STAY (gpu FLIP 422). If it instead
  runs every #183 join cell, 25 STALE cells become FLIP at M (gpu FLIP 456). The table counts the
  middle reading: per mode, as the inc comments attribute.
- A second, plain process risk: 431 device cells is a large rollout for one task (T19 took twenty
  batches for 37 queries). If `join-backend` enables in batches and the chain closes early, the
  shortfall is process, not a defect. The prediction assumes 5.7's "every cell turns on as its last
  ticket closes, each run on shad-gpu".

## 2. Per task

| task | cpu cells on | gpu cells on | exposes | risks to cells on today |
|---|--:|--:|---|---|
| duckdb-oracle | 0 | 0 | a 9th `corpus_query!` argument on 120 lines; no cpu answer differs from DuckDB in rows, strings or NULLs (#235's run) | none to cells. Expect `duckdb_approx` for the decimal-`avg`/division and float-digit rows #235 lists, and a new ticket for fixed-scale decimal truncation if the tolerance is below 1e-6 relative (tpcds q66, q58), cited by `duckdb_divergent(<n>)` |
| pbench | 0 | 0 | its own rows only | per-dataset `small_table_bytes` must leave tpch/tpcds plans unmoved (the spec's own test) |
| repartition-keys | 15 (#189) | 0 by spec 5.3; 3 if it runs tpch q15 at 4s/4r/4z (#95, 16-byte path) | rollup-over-join's tp4 gpu cells now sit on #65, not #189 | `recipe-payloads.txt` moves for every shuffle query (`hash_key_fields`); rollup emit `hash=` loses `__grouping_id` in six tp4 plans |
| refcounted-scatter | 0 | 0 | — | 6 gpu cells: tpch q1 and shuffle-additive-avg at 4s/4r/4z (§5 R1) |
| exit-copies | 0 | 0 | — | 2 gpu cells: tpcds q84, q85 at 1s (§5 R2) |
| join-session-cpp | 0 | 0 | — | none: C++ only, nothing in Rust calls the four symbols until join-backend |
| join-backend | 23 as estimated; **48 since pbench landed** (#190 ×45, #212 ×3) — see the note under this table | 428 (+3 for q15 if repartition-keys did not run them) | NEXT: rollup-over-join #65 ×5, q78 #60 ×5, q32 #199 ×3 | 7 gpu join cells on today (§5 R3); cpu golden churn everywhere a join is; tpch nested-limits' zero-column scan (§5 R4) |
| verify-26.02 | 0 | 0 | #94 on 26.02: tpch shuffle-stddev 1s runs a `MERGE_M2` whose count child is cast to INT32 (`cpp/src/operators/aggregate.cpp`, #94); 26.02 accepts only INT64/FLOAT64 | none to the 25.02 registry |

**One figure moved after this estimate was taken (2026-10-08).** pbench added five rows carrying
`190` — `nl-inner`, `nl-left`, `nl-projection`, `nl-left-decimal` and `like-column-pattern` — so
#190 is nine registry rows and 45 cpu cells rather than four rows and 20, and join-backend flips 48
rather than 23. [#190](../tickets/joins.md#t190) carries the live list; the estimate's own numbers
are left as they were taken, since an estimate rewritten after the fact records nothing.

## 3. NEXT ISSUE — evidence

| cells | expected issue | evidence | label | conf |
|---|---|---|---|---|
| tpch rollup-over-join gpu ×5 (inc:83, csv:134) | #65: the device holds `__grouping_id` as INT32 where the plan declares UInt8, and the corpus row runs `schema_validation_enabled` | `tpch.sf1/tp1-single.plans.txt:1609-1610` declare `__grouping_id:UInt8` on `GpuAggregate` and `GpuAggregateBatches`; #65 (`tickets/corpus-coverage.md`, "holds it as Int32"); the device hook compares type ids with no exemption (`test_support/schema_validation.rs:14-29`, `device_schema.rs:62`). Its 1s cell ran and only #220 was seen (inc:80-82), on a run older than the schema hook (inferred from the T19 order; with the hook installed the refusal comes during the run, before any golden compare). The registry row does not carry 65; tpcds q5/q14/q18/q22/q77/q80 do | verified mechanism, inferred outcome | H |
| tpcds q78 gpu ×5 (inc:199, csv:79) | #60: `round(x, 2)` over Float64 is two roundings on the device; `golden_exact` sees one-ulp `ratio` cells | `tpcds.sf1/tp1-single.plans.txt:8410` `round(CAST(ss_qty@4 AS Float64) / CAST(__common_expr_1@0 AS Float64), 2)`; `cpp/src/expr.cpp:688-705` calls `cudf::round(fcol, places, HALF_UP)`; #60 re-emulated q78's committed answer: five `ratio` cells split | inferred (ticket's emulation, never run) | M-H |
| tpcds q32 gpu 4s/4r/4z (inc:282, csv:33) | #199: a keyless merge over a lane that received nothing — the cpu emits the identity row, the device emits nothing | `tpcds.sf1/tp4-single-mini.cpu.txt:4456` (and the same section in tp4-rowgroup/tp4-sized at :4073): `GpuAggregateBatches group_by=[] in_rows=[[0,0,1,0]] batch_rows=[[1],[1],[1],[1]]`; lanes 0, 1, 3 get no batch because the Inner join's build lanes are empty (`in_rows=[[0,0,12,0],…]`) and an empty Inner build owes nothing (design 4.3, 4.5 keep that drain). `cpu_backend/accumulate.rs:359` compacts over nothing for a keyless node; `gpu_backend/accumulate.rs:359-369` returns no batch without state. Scanning every tp4 cpu golden finds this shape in q32 alone | verified (code + golden), outcome inferred | H |

Next issues behind rows that STAY anyway (so a fix of the tagged ticket alone will not turn them on):
tpcds q2 → #60 (seven `round(…, 2)`); tpcds q5 → #210 (`0.00 as return_amt`, a bare Decimal128
literal on the AST path, `tpcds.sf1/tp1-single.plans.txt:4764`) besides #65; tpcds q17 and q39 →
#225 (Welford state columns `$count/$mean/$m2` on a `GpuAggregate` at every mode, rows say
`schema_validation_enabled`); tpch q7/q8/q9 → #191 only (chain B's `date-part-return-type` is held
at `blocked(done)`).

Checked and ruled out (verified unless noted):
- #202 (DESC NULLs): of the DESC-sorted queries only tpcds q34, q71 and q77 have a NULL cell in
  their result; q34 and q71 have no `LIMIT`, q77's 44 rows sit under `LIMIT 100`, and the device
  result compare sorts rows (`batches_to_sorted_str`), so no cell can see it.
- #205: the cpu-sort-over-zero-row-batches shape appears in tpcds q17 alone (scan of every
  `GpuAccumulateBatchesAndSort`/`GpuMergeSortedPartitions` in all ten cpu goldens).
- #210: q5 is the only projected bare decimal literal.
- A CASE with no ELSE and a decimal THEN (tpcds q2, q43, q59; q4, q74 in join residuals): cuDF's
  `make_default_constructed_scalar` keeps the fixed-point scale
  (`third_party/cudf/cpp/src/scalar/scalar_factories.cpp`, `default_scalar_functor`), so
  `copy_if_else` types agree. corpus-fixes "ticket to file 12" misread the other helper.
- String-vs-string join residuals (`bought_city != ca_city` in q46/q68, q64) pass
  `cudf_ast_can_evaluate` (`expr.cpp:399-419`) and ran through the sink at 1s on 25.02.
- Scalar functions in the plans: `substr`, `coalesce`, `round`, `concat`, `upper`, `lower`,
  `sqrt`, `date_part` — all have device arms (`expr.cpp:623-750`); no `CAST(… AS Utf8)`, ILIKE,
  column `substr` args, interval (bar mixed-join) or timestamp key anywhere.
- Shuffle key types at tp4 (all plans): Int64, Utf8, Int32, Date32, UInt8 (rollups only, gone with
  #189) and Decimal128 (tpch q2/q10/q15/q18, tpcds q24/q37/q82/q75 — #95, in chain). No Float or
  Boolean key.
- cuDF 26.02's two-argument `hash_join(build, cmp)` builds with `nullable_join::YES`
  (`third_party/cudf/cpp/src/join/hash_join.cu:851-857`), so the design's D14 concern does not bite.
- A conditional join over the literal `true` (q9's remapped nested loops): cuDF wraps a literal-only
  AST in `IDENTITY` (`third_party/cudf/cpp/src/ast/expression_parser.cpp:181-185`).

Not ruled out, low probability (inferred): a Float64→Decimal128 cast inside residual thresholds
(tpcds q1/q30/q81/q32/q92 `… * Float64(1.2)`, q58 `0.9 * CAST(… AS Float64)`) — arrow rounds that
cast, cuDF's rounding was not checked; a flip needs a threshold within 1e-15 of a value. And the
general one: no join query's device answer has ever been compared with the result golden
(#220 panicked first), so every FLIP below M-H carries that unknown.

## 4. STAY

| rows | ticket on the row | cells |
|---|---|--:|
| tpch q7, q8, q9 | #191 | 15 gpu |
| tpcds q5, q14, q18, q22, q77, q80 | #65 | 30 gpu |
| tpcds q2 | #56 (stale per corpus-fixes, never proven; then #60) | 5 gpu |
| tpcds q39 | #57 (then #225) | 5 gpu |
| tpcds q66 | #55 (stale per corpus-fixes; a walk pin would close it) | 5 gpu |
| tpcds q17 | #205 (then #225) | 5 gpu |
| tpcds q88, q90, q96 at 4s/4r/4z | #199 (cpu twin off; `every_device_cell_has_a_cpu_cell_at_the_same_mode`) | 9 gpu + 9 cpu |
| tpch mixed-join | #168 | 5 gpu |
| tpch nested-limits, scan-limit | #186 | 10 gpu + 2 cpu |
| STALE, §6 | #183/#187 (closed) | 41 gpu |

## 5. RISK — cells on today

Prediction: every cell on today is still on at the end (551 cpu, 26 gpu). The risks are what the
tasks must catch on their own runs.

| id | cells | what could turn them off | evidence | likelihood it shows during the task / survives to the end |
|---|---|---|---|---|
| R1 | gpu tpch q1, shuffle-additive-avg at 4s/4r/4z (6) | refcounted-scatter makes each scatter partition a view of one table; `varlen_content_bytes` (`cpp/src/node_session.cpp:229-240`) uses `strings_column_view::chars_size`, which "does not reflect a sliced parent column view" (`rapids-cuda-12.2/include/cudf/strings/strings_column_view.hpp:89-98`; the 25.10 body reads the unsliced child's last offset, `third_party/cudf/cpp/src/strings/strings_column_view.cpp:38-42`). Every partition would report the whole table's string bytes, and `batch_bytes` at the string-keyed `GpuEmitPartitions` (`tpch.sf1/tp4-single-mini.cpu.txt:13-14`, `[[483],[329],[],[791]]`) would stop matching. Same for any pass-through of a view | verified mechanism | M-H / L — design 5.4's "out_stats per partition unchanged" gtest catches it if it uses a string column |
| R2 | gpu tpcds q84, q85 at 1s (2) | exit-copies: a project that moves rather than copies an input column breaks on a repeated `ColumnRef` (#154: "a repeated projection ordinal moves one column twice leaving a hole — a wrong answer"). q84 projects `c_customer_id@0` twice (`tpcds.sf1/tp1-single.plans.txt:9368`), q85 two avgs twice; 17 tpcds plans have the shape (q6 q10 q16 q19 q35 q39 q55 q64 q69 q71 q78 q84 q85 q91 q92 q94 q95) | verified shape | L-M / L |
| R3 | gpu join cells at 1s: tpch q17, q19, nested-loop-join; tpcds q37, q82, q84, q85 (7) | the device join path is replaced and the cpu goldens they compare against are regenerated; both sides must still agree | inferred | L / L |
| R4 | cpu tpch nested-limits ×5 | design 4.1 "plan validation refuses a zero-column schema anywhere", and nested-limits has a zero-column scan, `GpuLoadParquet … projections=[] … schema=[]` under `GpuCoalesceAllBatches schema=[]` (`tpch.sf1/tp1-single.plans.txt:168-172`). The design's count ("19 `GpuProject: exprs=[]` in tpcds") does not include it, and a project placeholder cannot go below a scan | verified shape | M / L — the cpu tier goes red, so the developer must give the scan a placeholder |
| R5 | cpu goldens, answers unchanged | churn the human will see: every join section of every `.cpu.txt`/`.cost.txt` (one batch per call, no semi-family batch per probe); recipe lines (tpch 75, tpcds 656 per mode; `recipe-payloads.txt` 134 join lines); #137 `GpuFilter(<key> IS NOT NULL)` in about 73 tpcds plans at 4s/4r/4z (none in tpch: no NULL keys); `__rowcount__` in tpcds q38, q87, q88, q9, q90, q96; tpcds q9's 15 predicate-free `Left` joins (DataFusion's `ScalarSubqueryToJoin` uses `JoinType::Left, None`, `datafusion-optimizer-45/src/scalar_subquery_to_join.rs:334`) turn from `GpuCrossJoin` into `GpuNestedLoopJoin` over `true` | verified shapes | certain churn; answer change L |
| R6 | cpu tpch q16, anti-join | the `NOT IN` rewrite fires only where a key can be NULL; every row group of `orders.o_custkey`, `customer.c_custkey`, `partsupp.ps_suppkey`, `supplier.s_suppkey` reports `null_count 0` (pyarrow, this host), so neither plan moves unless the trace fails. tpcds has no `NOT IN` | verified data | L |
| R7 | cpu tpcds cells at tp4 | a #137 filter that empties a lane feeding a keyless `count(*)` would give the #180 refusal; none found | inferred | L |
| R8 | gpu tpch shuffle-stddev 1s, on 26.02 only | #94 (above); registry unaffected | #94's text; not run | H on 26.02 / n.a. for 25.02 |

## 6. STALE — #183/#187 rows

`183` stays on a row "only where the other four modes have no ticket yet" (archived #183;
inc:53 "a row keeping `183` has modes never run past the string class"). Both tickets are closed.

| rows | cells | real blocker at those modes | if run |
|---|--:|---|---|
| tpch nested-loop-join, q4, cross-join, nested-loop-left-join, semi-join, anti-join at 1r/4s/4r/4z; q15 at 1r | 25 | #152 (in chain) | FLIP, M |
| tpch aggregate-groupby, shuffle-additive, shuffle-stddev (183), filter-project (187) at 1r/4s/4r/4z | 16 | none known; no chain task touches these rows | likely FLIP if anyone runs them (outside this chain) |

## 7. Per query

Codes: F FLIP, N NEXT, S STAY, Z STALE; confidence in brackets; JB join-backend, RK
repartition-keys. D1 = tp1-single already ran end to end on a device (sink survey or T19) and its
only blocker was #220; D4 = first device run at this mode, blockers #152/#220 (#95 where the row
has it). Modes: 1s tp1-single, 1r tp1-rowgroup, 4s tp4-single, 4r tp4-rowgroup, 4z tp4-sized.
Line numbers: inc / csv. Rows with every cell on today are listed first.

| query | tickets | inc / csv | cpu off → | gpu off → | task | reason |
|---|---|---|---|---|---|---|
| tpch/q1 | — | 16 / 101 | — | — (all on; RISK R1 at 4s/4r/4z) | — | — |
| tpch/q6 | — | 13 / 106 | — | — (all on) | — | — |
| tpch/shuffle_additive_avg | — | 69 / 138 | — | — (all on; RISK R1 at 4s/4r/4z) | — | — |
| tpch/q2 | 95 152 220 | 109 / 102 | — | 1s F(M-H) · 1r F(M) · 4s,4r,4z F(M) | JB | 1s ran to its min aggregate, #220 caught it; #152 multi-batch probe; #152; #95 ps_supplycost Decimal128(15,2) key (8-byte path) |
| tpch/q3 | 152 220 | 29 / 103 | — | 1s F(H) · 1r,4s,4r,4z F(M-H) | JB | ran to the unload, #220 only (fix-10 nine); #152 only; Inner joins on Int64/Date32 keys |
| tpch/q4 | 183 220 | 84 / 104 | — | 1s F(M-H) · 1r,4s,4r,4z Z(M) | JB, — | LeftSemi; tp1-single ran, #220 was its documented blocker (inc comment); in-chain; #183 (closed): "modes never run past the string class"; design 5.7 keeps #183 rows off. Real blocker at these modes was #152 (in chain): would flip if run |
| tpch/q5 | 152 220 | 97 / 105 | — | 1s F(H) · 1r,4s,4r,4z F(M) | JB | ran; #220 in_rows at the merge; #152; five Inner joins |
| tpch/q7 | 152 191 220 | 107 / 107 | — | 1s,1r,4s,4r,4z S(H) | — | #191 (date_part Int16; chain B task held, not in this chain) |
| tpch/q8 | 152 191 220 | 108 / 108 | — | 1s,1r,4s,4r,4z S(H) | — | #191 (date_part Int16; chain B task held, not in this chain) |
| tpch/q9 | 152 191 220 | 98 / 109 | — | 1s,1r,4s,4r,4z S(H) | — | #191 (date_part Int16; chain B task held, not in this chain) |
| tpch/q10 | 95 152 220 | 30 / 110 | — | 1s F(M-H) · 1r F(M) · 4s,4r,4z F(M) | JB | ran to the sink (survey); #220; #152; #152; #95 c_acctbal Decimal128(15,2) key |
| tpch/q11 | 190 | 99 / 111 | 1s,1r,4s,4r,4z F(H) | 1s,1r,4s,4r,4z F(M-L) | JB | #190: NestedLoopJoinExec gets the node projection; never on a device (cpu #190); NLJ over a decimal predicate takes 3.6 chunked index cross; result over cap, gpu_oracle must become live_cpu |
| tpch/q12 | 152 220 | 19 / 112 | — | 1s F(H) · 1r,4s,4r,4z F(M-H) | JB | ran to the sink; #220; #152; one Inner join |
| tpch/q13 | 152 | 26 / 113 | — | 1s,1r,4s,4r,4z F(M) | JB | Left join never ran (probe copy, #152); session Left + finish pads |
| tpch/q14 | 152 220 | 17 / 114 | — | 1s F(H) · 1r,4s,4r,4z F(M-H) | JB | ran to the unload, #220 only (fix-10 nine); #152; one Inner join |
| tpch/q15 | 95 183 220 | 27 / 115 | — | 1s F(M-H) · 1r Z(M) · 4s,4r,4z F(M) | JB, RK/JB, — | ran; #220 (ticket text: [[8192,1808]]); #183 (closed), never run; #95 Decimal128(38,4) total_revenue key (16-byte path); one probe batch per lane so no #152 |
| tpch/q16 | 59 80 152 220 | 85 / 116 | — | 1s F(M-H) · 1r,4s,4r,4z F(M) | JB | ran; #220 at the merge; NOT IN rewrite does not fire (null_count 0); #152; RightAnti now honours null_equals_null (no NULL keys in data) |
| tpch/q17 | 152 | 28 / 117 | — | 1r,4s,4r,4z F(M-H) | JB | #152; Inner+residual (decimal) via pairs path |
| tpch/q18 | 95 152 220 | 86 / 118 | — | 1s F(M-H) · 1r F(M) · 4s,4r,4z F(M) | JB | ran to the sink; #220; #152; RightSemi via distinct_hash_join; #152; #95 o_totalprice Decimal128(15,2) key |
| tpch/q19 | 152 | 18 / 119 | — | 1r,4s,4r,4z F(H) | JB | #152 build copy only (corpus-fixes fix 18: "q19 x4 likely green alone") |
| tpch/q20 | 152 220 | 96 / 120 | — | 1s F(M-H) · 1r,4s,4r,4z F(M) | JB | ran; #220 join batching; #152; LeftSemi+RightSemi+residual |
| tpch/q21 | 59 80 152 220 | 100 / 121 | — | 1s F(M-H) · 1r,4s,4r,4z F(M) | JB | ran; #220 join batching; #152; LeftSemi/LeftAnti+AST residual now stream (mixed_left_semi_join), probe coalesce removed |
| tpch/q22 | 59 80 190 | 87 / 122 | 1s,1r,4s,4r,4z F(H) | 1s,1r,4s,4r,4z F(M-L) | JB | #190: NestedLoopJoinExec gets the node projection; never on a device (cpu #190); NLJ decimal non-AST + LeftAnti; needs cpu cells first |
| tpch/aggregate_groupby | 183 | 44 / 123 | — | 1r,4s,4r,4z Z(M) | — | #183 (closed) never-run modes; no chain task touches the row |
| tpch/anti_join | 152 183 220 | 57 / 124 | — | 1s F(M-H) · 1r,4s,4r,4z Z(M) | JB, — | RightAnti (NOT IN; rewrite does not fire); tp1-single ran, #220 was its documented blocker (inc comment); in-chain; #183 (closed): "modes never run past the string class"; design 5.7 keeps #183 rows off. Real blocker at these modes was #152 (in chain): would flip if run |
| tpch/cross_join | 183 220 | 45 / 125 | — | 1s F(M-H) · 1r,4s,4r,4z Z(M) | JB, — | Cross; tp1-single ran, #220 was its documented blocker (inc comment); in-chain; #183 (closed): "modes never run past the string class"; design 5.7 keeps #183 rows off. Real blocker at these modes was #152 (in chain): would flip if run |
| tpch/filter_project | 187 | 43 / 126 | — | 1r,4s,4r,4z Z(M) | — | #187 (closed) never-run modes; no chain task touches the row |
| tpch/hash_join | 152 220 | 54 / 127 | — | 1s F(H) · 1r,4s,4r,4z F(M-H) | JB | ran; #220 in_rows at the merge; #152 build copy |
| tpch/join_int | 152 220 | 263 / 128 | — | 1s F(H) · 1r,4s,4r,4z F(M-H) | JB | ran to the unload, #220 only (fix-10 nine); #152 |
| tpch/left_join | 152 | 55 / 129 | — | 1s,1r,4s,4r,4z F(M) | JB | Left never ran on a device (probe copy, #152) |
| tpch/mixed_join | 168 | 71 / 130 | — | 1s,1r,4s,4r,4z S(H) | — | #168 interval literal cannot cross the wire |
| tpch/nested_limits | 186 | 73 / 131 | — | 1s,1r,4s,4r,4z S(H) | — | #186 limit in the scan (then the zero-column scan the design does not name) |
| tpch/nested_loop_join | 183 | 58 / 132 | — | 1r,4s,4r,4z Z(M) | — | #183 (closed) never-run modes; no chain task touches the row |
| tpch/nested_loop_left_join | 183 220 | 72 / 133 | — | 1s F(M-H) · 1r,4s,4r,4z Z(M) | JB, — | NLJ Left; tp1-single ran, #220 was its documented blocker (inc comment); in-chain; #183 (closed): "modes never run past the string class"; design 5.7 keeps #183 rows off. Real blocker at these modes was #152 (in chain): would flip if run |
| tpch/rollup_over_join | 152 189 220 | 83 / 134 | 4s,4r,4z F(H) | 1s,1r,4s,4r,4z N(H) | JB, RK | #65: GpuAggregate declares __grouping_id:UInt8, device builds INT32; schema_validation_enabled refuses it (tp1-single.plans.txt rollup-over-join; corpus_cases.inc:83); #189: rollup shuffle stops hashing __grouping_id |
| tpch/scan_limit | 186 | 42 / 135 | 1s,1r S(H) | 1s,1r,4s,4r,4z S(H) | — | #186 |
| tpch/semi_join | 152 183 220 | 56 / 136 | — | 1s F(M-H) · 1r,4s,4r,4z Z(M) | JB, — | RightSemi; tp1-single ran, #220 was its documented blocker (inc comment); in-chain; #183 (closed): "modes never run past the string class"; design 5.7 keeps #183 rows off. Real blocker at these modes was #152 (in chain): would flip if run |
| tpch/shuffle_additive | 183 | 46 / 137 | — | 1r,4s,4r,4z Z(M) | — | #183 (closed) never-run modes; no chain task touches the row |
| tpch/shuffle_stddev | 183 | 70 / 139 | — | 1r,4s,4r,4z Z(M) | — | #183 (closed) never-run modes; no chain task touches the row |
| tpcds/q1 | 220 | 156 / 2 | — | 1s F(M-H) · 1r,4s,4r,4z F(M) | JB | D1; D4 |
| tpcds/q2 | 56 152 | 189 / 3 | — | 1s,1r,4s,4r,4z S(H) | — | #56 (stale, likely fixed); then #60 round(x,2) one ulp x7 per row |
| tpcds/q3 | 152 220 | 114 / 4 | — | 1s F(M-H) · 1r,4s,4r,4z F(M) | JB | D1; D4 |
| tpcds/q4 | 152 220 | 235 / 5 | — | 1s F(M-H) · 1r,4s,4r,4z F(M) | JB | D1; D4 |
| tpcds/q5 | 65 152 189 | 200 / 6 | 4s,4r,4z F(H) | 1s,1r,4s,4r,4z S(H) | RK, — | #65; also #210 (bare 0.00 Decimal literal projected, AST path makes FLOAT64); #189: rollup shuffle stops hashing __grouping_id |
| tpcds/q6 | 152 | 264 / 7 | — | 1s,1r,4s,4r,4z F(M) | JB | Left join, #152 probe copy at 1s; never ran |
| tpcds/q7 | 220 | 157 / 8 | — | 1s F(M-H) · 1r,4s,4r,4z F(M) | JB | D1; D4 |
| tpcds/q8 | 152 220 | 164 / 9 | — | 1s F(M-H) · 1r,4s,4r,4z F(M) | JB | D1; D4 |
| tpcds/q9 | 63 | 265 / 10 | — | 1s,1r,4s,4r,4z F(M-L) | JB | #63 at 1s; first run past the placeholder; 15 predicate-free Left NLJs become GpuNestedLoopJoin(true) |
| tpcds/q10 | 152 220 | 215 / 11 | — | 1s F(M-H) · 1r,4s,4r,4z F(M) | JB | D1; D4 |
| tpcds/q11 | 152 220 | 212 / 12 | — | 1s F(M-H) · 1r,4s,4r,4z F(M) | JB | D1; D4 |
| tpcds/q13 | 220 | 266 / 14 | — | 1s F(M-H) · 1r,4s,4r,4z F(M) | JB | D1; D4 |
| tpcds/q14 | 65 220 | 267 / 15 | — | 1s,1r,4s,4r,4z S(H) | — | #65 grouping id INT32 vs UInt8 under schema validation |
| tpcds/q15 | 152 220 | 122 / 16 | — | 1s F(M-H) · 1r,4s,4r,4z F(M) | JB | D1; D4 |
| tpcds/q16 | 59 80 152 220 | 177 / 17 | — | 1s F(M-H) · 1r,4s,4r,4z F(M) | JB | D1; D4 |
| tpcds/q17 | 205 | 276 / 18 | — | 1s,1r,4s,4r,4z S(H) | — | #205; then #225 (stddev state names, validation enabled) |
| tpcds/q18 | 65 189 220 | 277 / 19 | 4s,4r,4z F(H) | 1s,1r,4s,4r,4z S(H) | RK, — | #65 grouping id INT32 vs UInt8 under schema validation; #189: rollup shuffle stops hashing __grouping_id |
| tpcds/q19 | 152 220 | 163 / 20 | — | 1s F(M-H) · 1r,4s,4r,4z F(M) | JB | D1; D4 |
| tpcds/q21 | 152 220 | 134 / 22 | — | 1s F(M-H) · 1r,4s,4r,4z F(M) | JB | D1; D4 |
| tpcds/q22 | 65 189 220 | 278 / 23 | 4s,4r,4z F(H) | 1s,1r,4s,4r,4z S(H) | RK, — | #65 grouping id INT32 vs UInt8 under schema validation; #189: rollup shuffle stops hashing __grouping_id |
| tpcds/q23 | 152 220 | 252 / 24 | — | 1s F(M-H) · 1r,4s,4r,4z F(M) | JB | D1; D4 |
| tpcds/q24 | 95 190 | 279 / 25 | 1s,1r,4s,4r,4z F(H) | 1s,1r,4s,4r,4z F(M-L) | JB | #190: NestedLoopJoinExec gets the node projection; cpu #190 first; never on a device; #95 i_current_price key at tp4 |
| tpcds/q25 | 152 220 | 186 / 26 | — | 1s F(M-H) · 1r,4s,4r,4z F(M) | JB | D1; D4 |
| tpcds/q26 | 220 | 280 / 27 | — | 1s F(M-H) · 1r,4s,4r,4z F(M) | JB | D1; D4 |
| tpcds/q29 | 152 220 | 187 / 30 | — | 1s F(M-H) · 1r,4s,4r,4z F(M) | JB | D1; D4 |
| tpcds/q30 | 220 | 281 / 31 | — | 1s F(M-H) · 1r,4s,4r,4z F(M) | JB | D1; D4 |
| tpcds/q31 | 152 220 | 236 / 32 | — | 1s F(M-H) · 1r,4s,4r,4z F(M) | JB | D1; D4 |
| tpcds/q32 | 220 | 282 / 33 | — | 1s,1r F(M-H) · 4s,4r,4z N(H) | JB | 1s ran; #220 in_rows; #199: keyless merge lanes 0,1,3 get no arrival; cpu golden emits identity rows [[1],[1],[1],[1]] from in_rows [[0,0,1,0]], device emits none (gpu_backend/accumulate.rs:359-369) |
| tpcds/q33 | 152 220 | 222 / 34 | — | 1s F(M-H) · 1r,4s,4r,4z F(M) | JB | D1; D4 |
| tpcds/q34 | 152 220 | 136 / 35 | — | 1s F(M-H) · 1r,4s,4r,4z F(M) | JB | D1; D4 |
| tpcds/q35 | 220 | 290 / 36 | — | 1s F(M-H) · 1r,4s,4r,4z F(M) | JB | D1; D4 |
| tpcds/q37 | 95 152 | 123 / 38 | — | 1r,4s,4r,4z F(M) | JB | D4 |
| tpcds/q38 | 152 220 | 203 / 39 | — | 1s F(M-H) · 1r,4s,4r,4z F(M) | JB | D1; D4 |
| tpcds/q39 | 57 220 | 291 / 40 | — | 1s,1r,4s,4r,4z S(H) | — | #57 value-form CASE; then #225 (Welford state names, validation enabled) |
| tpcds/q40 | 152 220 | 145 / 41 | — | 1s F(M-H) · 1r,4s,4r,4z F(M) | JB | D1; D4 |
| tpcds/q41 | 152 | 112 / 42 | — | 1s,1r,4s,4r,4z F(M) | JB | Left (count-bug) join never ran |
| tpcds/q42 | 152 220 | 113 / 43 | — | 1s F(M-H) · 1r,4s,4r,4z F(M) | JB | D1; D4 |
| tpcds/q43 | 152 220 | 115 / 44 | — | 1s F(M-H) · 1r,4s,4r,4z F(M) | JB | D1; D4 |
| tpcds/q45 | 152 220 | 144 / 46 | — | 1s F(M-H) · 1r,4s,4r,4z F(M) | JB | D1; D4 |
| tpcds/q46 | 152 220 | 167 / 47 | — | 1s F(M-H) · 1r,4s,4r,4z F(M) | JB | D1; D4 |
| tpcds/q48 | 152 220 | 133 / 49 | — | 1s F(M-H) · 1r,4s,4r,4z F(M) | JB | D1; D4 |
| tpcds/q50 | 152 220 | 147 / 51 | — | 1s F(M-H) · 1r,4s,4r,4z F(M) | JB | D1; D4 |
| tpcds/q52 | 152 220 | 116 / 53 | — | 1s F(M-H) · 1r,4s,4r,4z F(M) | JB | D1; D4 |
| tpcds/q54 | 190 | 201 / 55 | 1s,1r,4s,4r,4z F(H) | 1s,1r,4s,4r,4z F(M-L) | JB | #190: NestedLoopJoinExec gets the node projection; cpu #190 first; never on a device |
| tpcds/q55 | 152 220 | 117 / 56 | — | 1s F(M-H) · 1r,4s,4r,4z F(M) | JB | D1; D4 |
| tpcds/q56 | 152 220 | 223 / 57 | — | 1s F(M-H) · 1r,4s,4r,4z F(M) | JB | D1; D4 |
| tpcds/q58 | 152 220 | 233 / 59 | — | 1s F(M-H) · 1r,4s,4r,4z F(M) | JB | D1; D4 |
| tpcds/q59 | 152 220 | 190 / 60 | — | 1s F(M-H) · 1r,4s,4r,4z F(M) | JB | D1; D4 |
| tpcds/q60 | 152 220 | 224 / 61 | — | 1s F(M-H) · 1r,4s,4r,4z F(M) | JB | D1; D4 |
| tpcds/q61 | 152 220 | 214 / 62 | — | 1s F(M-H) · 1r,4s,4r,4z F(M) | JB | D1; D4 |
| tpcds/q62 | 152 220 | 148 / 63 | — | 1s F(M-H) · 1r,4s,4r,4z F(M) | JB | D1; D4 |
| tpcds/q64 | 152 220 | 254 / 65 | — | 1s,1r,4s,4r,4z F(M-L) | JB | largest plan, never run on a device |
| tpcds/q65 | 220 | 292 / 66 | — | 1s F(M-H) · 1r,4s,4r,4z F(M) | JB | D1; D4 |
| tpcds/q66 | 55 152 220 | 175 / 67 | — | 1s,1r,4s,4r,4z S(H) | — | #55 (stale per corpus-fixes; not in chain) |
| tpcds/q68 | 152 220 | 173 / 69 | — | 1s F(M-H) · 1r,4s,4r,4z F(M) | JB | D1; D4 |
| tpcds/q69 | 59 80 152 220 | 225 / 70 | — | 1s F(M-H) · 1r,4s,4r,4z F(M) | JB | D1; D4 |
| tpcds/q71 | 152 | 174 / 72 | — | 1s,1r,4s,4r,4z F(M) | JB | #152 at 1s too (union probe); never ran |
| tpcds/q73 | 152 220 | 154 / 74 | — | 1s F(M-H) · 1r,4s,4r,4z F(M) | JB | D1; D4 |
| tpcds/q74 | 152 220 | 232 / 75 | — | 1s F(M-H) · 1r,4s,4r,4z F(M) | JB | D1; D4 |
| tpcds/q75 | 95 152 | 251 / 76 | — | 1s,1r,4s,4r,4z F(M-L) | JB | six Left joins, #152 every mode; #95 Decimal128(31,15) key at tp4 |
| tpcds/q76 | 152 220 | 176 / 77 | — | 1s F(M-H) · 1r,4s,4r,4z F(M) | JB | D1; D4 |
| tpcds/q77 | 65 152 212 220 | 226 / 78 | 4s,4r,4z F(M-H) | 1s,1r,4s,4r,4z S(H) | JB, — | #65 grouping id INT32 vs UInt8 under schema validation; #212: Right outer with no build batch answers through set_build(None); its rollup shuffle (UInt8 key) is #189, fixed earlier in the chain |
| tpcds/q78 | 152 | 199 / 79 | — | 1s,1r,4s,4r,4z N(M-H) | JB | #60: round(CAST(ss_qty AS Float64)/..., 2) one-ulp split; ticket says 5 ratio cells differ under golden_exact |
| tpcds/q79 | 152 220 | 155 / 80 | — | 1s F(M-H) · 1r,4s,4r,4z F(M) | JB | D1; D4 |
| tpcds/q80 | 65 152 189 220 | 234 / 81 | 4s,4r,4z F(H) | 1s,1r,4s,4r,4z S(H) | RK, — | #65 grouping id INT32 vs UInt8 under schema validation; #189: rollup shuffle stops hashing __grouping_id |
| tpcds/q81 | 220 | 293 / 82 | — | 1s F(M-H) · 1r,4s,4r,4z F(M) | JB | D1; D4 |
| tpcds/q82 | 95 152 | 124 / 83 | — | 1r,4s,4r,4z F(M) | JB | D4 |
| tpcds/q83 | 152 220 | 213 / 84 | — | 1s F(M-H) · 1r,4s,4r,4z F(M) | JB | D1; D4 |
| tpcds/q84 | 152 | 137 / 85 | — | 1r,4s,4r,4z F(M) | JB | D4 |
| tpcds/q85 | 152 | 294 / 86 | — | 1r,4s,4r,4z F(M) | JB | D4 |
| tpcds/q87 | 80 152 220 | 211 / 88 | — | 1s F(M-H) · 1r,4s,4r,4z F(M) | JB | D1; D4 |
| tpcds/q88 | 152 199 220 | 253 / 89 | 4s,4r,4z S(H) | 1s F(M-H) · 1r F(M) · 4s,4r,4z S(H) | JB, — | ran; #220 caught it; __rowcount__ now declared; #152 multi-batch at 1 lane; never ran; #199/#180 NULL count(*) from a keyless merge over an empty lane; #199 (cpu twin off; every_device_cell_has_a_cpu_cell) |
| tpcds/q90 | 152 199 220 | 188 / 91 | 4s,4r,4z S(H) | 1s F(M-H) · 1r F(M) · 4s,4r,4z S(H) | JB, — | ran to the sink (D1 list); #220; #152 multi-batch at 1 lane; never ran; #199/#180 NULL count(*) from a keyless merge over an empty lane; #199 (cpu twin off; every_device_cell_has_a_cpu_cell) |
| tpcds/q91 | 152 220 | 166 / 92 | — | 1s F(M-H) · 1r,4s,4r,4z F(M) | JB | D1; D4 |
| tpcds/q92 | 220 | 295 / 93 | — | 1s F(M-H) · 1r,4s,4r,4z F(M) | JB | D1; D4 |
| tpcds/q93 | 152 220 | 135 / 94 | — | 1s F(M-H) · 1r,4s,4r,4z F(M) | JB | D1; D4 |
| tpcds/q94 | 59 80 152 220 | 158 / 95 | — | 1s F(M-H) · 1r,4s,4r,4z F(M) | JB | D1; D4 |
| tpcds/q95 | 152 220 | 202 / 96 | — | 1s F(M-H) · 1r,4s,4r,4z F(M) | JB | D1; D4 |
| tpcds/q96 | 152 199 220 | 121 / 97 | 4s,4r,4z S(H) | 1s F(H) · 1r F(M) · 4s,4r,4z S(H) | JB, — | ran to the unload, #220 only (fix-10 nine); #152 multi-batch at 1 lane; never ran; #199/#180 NULL count(*) from a keyless merge over an empty lane; #199 (cpu twin off; every_device_cell_has_a_cpu_cell) |
| tpcds/q97 | 152 | 146 / 98 | — | 1s,1r,4s,4r,4z F(M) | JB | Full join never ran (probe copy) |
| tpcds/q99 | 152 220 | 165 / 100 | — | 1s F(M-H) · 1r,4s,4r,4z F(M) | JB | D1; D4 |

## 8. Checkable statements

Scored against the registry, the inc and the tickets once `verify-26.02` is `done`:

1. cpu: exactly the 38 FLIP cells of §1 turn on and no cpu cell turns off; cpu ends 589/600.
   The 11 left off are tpcds q88/q90/q96 at 4s/4r/4z (#199) and tpch scan-limit at 1s/1r (#186).
2. gpu: 13 NEXT cells end off with #65 (tpch rollup-over-join ×5), #60 (tpcds q78 ×5) and #199
   (tpcds q32 at 4s/4r/4z) on their rows, or with a new ticket naming the same mechanism.
3. gpu: the 89 STAY cells end off; the 41 STALE cells end off unless a task chose to run the
   #183/#187 rows (§1's process note).
4. gpu: between 422 and 456 cells are candidates depending on how #183 rows are read; of the 431
   counted, about 306 turn on (probability-weighted), so the device column ends near 330 on
   (central range 290–400). Shortfalls fall mostly at the M and M-L rows (first device run at a
   multi-lane or multi-batch mode; the never-run tpcds q6, q9, q24, q41, q54, q64, q71, q75, q97;
   tpch q11, q13, q22, left-join).
5. No gpu cell on today ends off. R1 (string bytes of sliced scatter views) is the risk most
   likely to show during `refcounted-scatter`.
6. No tpch plan golden gains a #137 filter; tpch q16 and anti-join keep their plans (no `NOT IN`
   rewrite); tpcds q9's 15 `GpuCrossJoin` lines become `GpuNestedLoopJoin` over `true`; tpch
   nested-limits' zero-column scan gains a placeholder.
7. No cpu answer in `mini.result.txt` changes in value; `duckdb-oracle` turns no cell off.
8. `verify-26.02` meets #94 on tpch shuffle-stddev (its 1s cell runs `MERGE_M2`).
