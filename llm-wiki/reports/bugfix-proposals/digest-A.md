# Digest A — aggregate state typing, empty-lane identity rows, grouping-set ids, DISTINCT, partial-phase aggregate paths

Tickets #163, #180, #189, #65, #62, #55, #56. Read at master 188c23ce. Paths relative to
`/media/data/peacockdb`. Every contested point below was settled by opening the code both sides
cite; "code supports" names the side the code agrees with. Nothing built or run.

## #163 — a declared type is never checked against the expression that produces it

- **Status after research:** live. Two walls, the second unwritten anywhere until the proposal.
- **Issue:** `avg`'s state is declared `[count: UInt64, sum: Decimal128(p,s)]` from DataFusion's
  `Avg::state_fields` (`planner/translator/aggregate.rs:127-157`), but this engine runs `avg` as
  `sum` + `count` (`plan/aggregates.rs:64-67`), whose answers are `Int64` and `Decimal128(p+10,s)`.
  `check_state_layout` (`cpu_backend/mod.rs:467-491`) refuses at executor construction, on the CPU,
  so the whole query is out: 23 queries × 5 modes, cpu and gpu (tpch q1 q17 q22
  shuffle_additive_avg; tpcds q1 q6 q7 q9 q13 q14 q17 q18 q22 q24 q26 q30 q32 q35 q39 q65 q81 q85
  q92; `corpus_cases.inc:36,83,285,298,…`, registry 23 rows). Second wall, by reading: once the
  state agrees, every *decimal* avg (19 of 23) refuses at the finalize project — `finalize()` casts
  the numerator to `(p+4,s+4)` before dividing (`plan/aggregates.rs:80-101`), the CPU lowers
  `Expr::Binary` to a bare `BinaryExpr` and drops `out_type` (`cpu_backend/expr_physical.rs:55-59`,
  the `..` pattern), arrow's decimal `Div` answers at `s_num+4` (`arrow-arith-54.2.1/src/numeric.rs:791-819`),
  so `(19,6)/(19,0)` → `(23,10)`, and `declared_as` (`cpu_backend/mod.rs:239-275`) refuses a scale change.
- **Root cause:** `decompose` types state by borrowing the accumulator DataFusion planned, which is
  right where that accumulator runs (`sum`/`min`/`max`/`count`, the Welford triple on the CPU) and
  wrong for a per-column split that runs other SQL aggregates. The finalize is the same defect from
  the other side: planner-invented IR whose `out_type` the CPU never applies.
- **Fix (as corrected by the review):** (3a) `plan/aggregate.rs`: new `state_type(PlanAgg, &DataType)`
  asking DataFusion's registry (`all_default_aggregate_functions`) for the per-column aggregate's
  `return_type`; `decompose` (`translator/aggregate.rs:148-157`) uses it for `Merge::PerColumn`
  decompositions and keeps `field.data_type()` for `Merge::Combined` (Welford); nullability stays
  DataFusion's `state_fields` (avg's count nullable — a non-nullable one would import #180 into
  every keyless avg). (3b) `plan/aggregates.rs` `AggFunc::Avg` arm: drop the numerator cast, keep
  the denominator cast to `Decimal128(p,0)`; CPU then answers `(29,6)` and `widened_decimal`'s
  same-scale-wider arm narrows to `(19,6)`; the device already pre-scales from `out_decimal_scale`
  (`cpp/src/expr.cpp:585-600`), so the removed cast was a redundant kernel. Frozen surfaces: none
  (recipe *bytes* move in three payload sections via `aggr_input_schema` types and one fewer cast —
  deliberate `PEACOCK_REWRITE_RECIPE_BYTES=1` regen). Moves with it: all ten `<mode>.plans.txt`
  (every avg state/finalize line, not the Welford `$count` lines); `recipe-payloads.txt` tpch q22,
  tpcds q14, q39; 23 cpu sections × 5 modes authored; registry 23 rows; `corpus_cases.inc` lines and
  comments; `schema_tests.rs:146-221` two tests; new CPU test built through `plan::finalize()` (not a
  hand-written divide) covering init → merge `(35,2)` → finalize; `architecture.md:48-50, :246-247,
  :356-360`; `build-test.md:5, :42`; `wire/expr_writer/tests.rs:375-376` comment; #163 narrowed to
  the device's Welford count (Int64 exported vs UInt64 declared) plus the absent validator.
- **Contested points:**
  - q17/q39 carry `stddev_samp` (`testdata/tpcds-queries/q17.sql:6-15`, `q39.sql:17`) and the
    Welford merge reassociates floats; the corpus precedent is `shuffle_stddev` at
    `data_fusion_approximate` (`corpus_cases.inc:70-84`). Code supports the review: declare both
    approximate from the start; q39's `stdev/mean > 1` filter is a row-set risk no tolerance absorbs.
  - Minimum query's tp4 shape: `lanes_for` (`translator/nodes.rs:373-389`) applies the small-table
    rule only when batching is on; supplier.parquet is 793,644 B < 5 MiB (`planner/mod.rs:37`), one
    row group. Code supports the review: one lane at tp1-*, tp4-rowgroup, tp4-sized; four lanes with
    three empty at tp4-single only.
  - The "average > 10^13 errors" risk is vacuous (an average is bounded by its input's max). Review right.
  - Complexity M vs L: code is ~45 lines; the branch authors up to 106 cells. See below.
- **Minimum corpus query:** `SELECT avg(s_acctbal) FROM supplier` (tpch sf1), CPU, tp1-single then
  tp4-single. Today: refused at construction, "column 1 is UInt64 in the declared state and Int64 in
  the one DataFusion's accumulators produce". After 3a alone: refused at the finalize, `(19,6)` vs
  `(23,10)`. After 3a+3b: one row, DataFusion's digits. For a real 4-lane merge at every tp4 mode:
  `SELECT avg(l_quantity) FROM lineitem`. Not in testdata. Device: crosses, refused at the unload (#187).
- **Cells re-enabled / next wall:** up to 106 cpu cells: 20 queries × 5, plus q14/q18/q22 at tp1-*
  only (their rollup gid hits #189 at tp4). q17/q39 under the approximate oracle. All 23 gpu cells
  stay off (#152 at multi-batch modes, #183/#187 at tp1-single). Possible third walls: #180's shape
  in q9's keyless scalar subqueries at tp4; anything unrun.
- **Overlaps and dependencies:** #189 blocks q14/q18/q22 × tp4 next. #62 edits the same fifteen lines
  of `decompose` and inherits `avg$count`'s type — land #163 first. #180's identity rule reads the
  same state schema (avg identity = `[NULL, 0]`, finalize NULL, matches DataFusion). #55/#56 share
  the init/merge/finalize path but touch no line here.
- **Complexity:** L as one branch (M for the code: four files, ~45 lines, ten plan goldens, three
  payload sections; the 106-cell authoring and triage is the size, T19 needed twenty batches for 37
  queries). M if the enablement is split off.
- **Unticketed defects found:** the CPU avg finalize scale wall (`expr_physical.rs:55-59` drops
  `out_type`) — fixed by 3b, never filed; the device's Welford count exports Int64 against a UInt64
  declaration (stays in the narrowed #163); `aggregate.cpp:582-597` `is_avg && Merge` arm is dead
  (wire never names `avg`) — cosmetic, no ticket.

## #180 — a shuffled count(*) merges to nullable against a non-nullable declaration

- **Status after research:** live — misdiagnosed in mechanism (real cause: no shuffle is involved;
  the CPU's per-lane merge-only `GpuAggregateBatches` *invents* an identity row for a lane that
  received no batch by running its merge — `sum` — over nothing, and `sum` of nothing is NULL).
- **Issue:** `mark_done_and_fetch` (`cpu_backend/accumulate.rs:366-383`, clause
  `self.state.is_none() && !self.grouped` at `:372`) calls `compact()` (`:351-364`) over no input;
  DataFusion's no-grouping stream emits one row from fresh accumulators, `SumAccumulator::state` =
  NULL; `declared(merged, &self.held)` → `declared_as` → `RecordBatch::try_new` refuses the NULL in
  `count(*)` declared `Int64, nullable=false` (copied from `Count::state_fields`,
  `translator/aggregate.rs:150-157`). Cells: tpcds q96, q90, q88 × tp4-single/rowgroup/sized, cpu
  (and so gpu) — 9 cells (`corpus_cases.inc:137,206,267`; registry 89, 91, 97).
- **Root cause:** the identity of an aggregate is what its *init* produces over no rows (`count`→0),
  not what its *merge* produces (`count` merges by `sum`, `plan/aggregates.rs:58-63` — the one row of
  the registry where the two differ). And the obligation sits on the wrong node: a merge-only node
  is an intermediate; an absent contribution and an identity contribution merge to the same answer,
  which is what the device already does (`gpu_backend/accumulate.rs:307-320`). Only the node that
  finalizes a global aggregate owes the SQL row. Two producers of the empty lane: at tp4-single a
  one-row-group table is cut `[[[0]],[],[],[]]` (`lanes_for` ignores the small-table rule with
  batching off, `translator/nodes.rs:373-389`); at every tp4 mode an Inner join whose build side
  scattered into fewer lanes than exist drains its probe and emits nothing (`partitioned.rs:381-385`
  drops empty scatter outputs; `single_partition.rs:317-329` `NoBuild`; `plan/join.rs:452-461`).
- **Fix (as corrected by the review):** `cpu_backend/accumulate.rs`: delete the `!self.grouped`
  clause and the `grouped` field (`:314-316`); `AggregateBatches` gains
  `identity: Option<Vec<ScalarValue>>`, set in `CpuAccumulator::aggregate` (`:65-101`) only when
  `group_by.is_empty() && finalize.is_some()` from `plan::identity_state(node.intermediate())`
  (the `&Schema` there carries `agg_state` annotations); `mark_done_and_fetch` uses it in place of
  a merge over nothing, and a merge-only node emits nothing. `plan/aggregates.rs`: `identity(PlanAgg,
  &DataType)` — Count/Mean/M2 → zero, Sum/Min/Max → NULL via `ScalarValue::try_from` (not the
  `expect`ing `null_of`), MergeM2 → error. `plan/aggregate.rs`: `identity_state(&Schema)` walking
  `agg_state` positions through `decomposition(func).state`; `plan/mod.rs` delegation. Ten-line doc
  cap on `mark_done_and_fetch`. Frozen surfaces: none (nullability unchanged, no wire byte moves).
  Moves with it: rewrite `cpu_backend/tests/accumulate.rs:700-748` into three tests (finalizing
  keyless → `Int64(0)`; merge-only keyless → nothing; identity == init over an empty batch for count
  and stddev), schema built by hand (`schema_of` makes everything nullable); e2e cases for the three
  queries below; `corpus_cases.inc:137,206,267` to five modes, comments `:134-136, :197-200,
  :258-259`; registry 89/91/97 `cpu_tp4_*` enabled, `180` dropped; nine tp4 sections authored
  (`.cpu.txt`/`.cost.txt`), `.result.txt` mode re-stamp; `build-test.md:42` (two #180 clauses, N);
  `tickets.md` #173 `:266-267` and #199 `:44-52` reworded; `accumulate.rs:358-361` test doc;
  hacks-audit finding 3 (`:442-460`) names the `!self.grouped` clause as a deliberate divergence —
  the fix keeps it narrowed to the finalizing node.
- **Contested points:**
  - Minimum query: proposal's Query A joins store_sales; the review's `SELECT count(*) FROM store
    WHERE s_store_name = 'ese'` reaches the same call at tp4-single with 12 rows and no join. Code
    supports the review (`lanes_for` with `Batching::Off` returns `target_partitions`; q93's tp4
    golden shows `batch_rows=[[35],[],[],[]]`). It does not reproduce at tp4-rowgroup/sized (small-
    table rule), so Query A is still needed there.
  - Query B's literal must survive row-group pruning or `partition()` refuses at plan time — review
    right, minor.
- **Minimum corpus query:** Query 0 (tp4-single only, no join): `SELECT count(*) FROM store WHERE
  s_store_name = 'ese'`. Query A (all tp4 modes): `SELECT count(*) FROM store, store_sales WHERE
  ss_store_sk = s_store_sk AND s_store_name = 'ese'`. Query B (all lanes empty, exposes 3b):
  the same with `'no such store'`. tpcds sf1, CPU; all plan today and fail at run with the arrow
  message; tp1 modes answer correctly. None in testdata.
- **Cells re-enabled / next wall:** 9 cpu cells. gpu cells of the three rows stay off: #152 at tp4,
  #185 (q96, q88) / #187 (q90) at tp1-single. Query B on a device would answer no row — that is #199,
  now exactly and only the finalizing-node divergence.
- **Overlaps and dependencies:** #163 — the identity for `avg` reads the same annotations; the
  nullability rule ("the SQL aggregate the executor runs it as") is stated identically by both.
  #62 review F2 finds a *new* reach of the same NULL that this fix does not cover (a `GpuAggregate`
  shortcut over an *empty batch*, not a missing one). #173/#175 own the empty-scatter drop that is
  one of the two triggers; this fix is correct with or without it. #199 narrows.
- **Complexity:** S (four files, <80 lines of logic, no golden regenerated; nine sections authored,
  q88's eight subqueries at three modes is the long run).
- **Unticketed defects found:** keyless `sum`/`min`/`max` over an empty lane silently emit a NULL
  identity row today (right by luck of nullability) — dissolved by this fix. The `GpuAggregate{final}`
  shortcut over a lane with *no batch* emits no row on both engines — reachable only from a mid-plan
  limit that dropped everything, no corpus query has it; same family as #199, not filed.
  `build-test.md:42` names q96 and q88 on #180 but not q90 — drift.

## #189 — the shuffle cannot hash a rollup's grouping-set id

- **Status after research:** live.
- **Issue:** the translator copies DataFusion's `FinalPartitioned` hash keys verbatim
  (`shuffle_below`, `translator/aggregate.rs:39-67`, the `ByHash` record at `:58`), and DataFusion's
  final `group_by.input_exprs()` include `__grouping_id` (`datafusion-physical-plan-45.0.0/src/
  aggregates/mod.rs:225-243, :817-818`). comet's murmur3 has no unsigned arm, so `GpuEmitPartitions`
  refuses at run time on the CPU: "Unsupported data type in hasher: UInt8". Cells: tpch
  rollup_over_join, tpcds q5, q80 × tp4-single/rowgroup/sized — 9 cpu cells (`corpus_cases.inc:97,
  218,251`; registry 6, 81, 134). Golden evidence: `tpch.sf1/tp4-single.plans.txt:2738`,
  `tpcds.sf1/tp4-single.plans.txt:2687,3338,8671,14962,15968`.
- **Root cause:** `aggregate_sequence` consumes the `Shuffle` unchanged (`:370-371`) although it
  already knows the gid exists and where it sits (`key_columns` at `:264`, `group.is_single()`).
  The wiki (`architecture.md:273-275`) and the validator (`plan/aggregate.rs:236-256`, the subset
  rule) already state the intended rule — hash the user keys alone — and the translator never
  implemented it. Hashing the gid also could never agree across engines: the device's gid is INT32
  with LSB-first bits (`aggregate.cpp:388-392,414`), the CPU's UInt8 MSB-first (#65).
- **Fix (as corrected by the review):** one rebinding in `aggregate_sequence`, directly above
  `tree = match shuffle` (`:370`): if `Shuffle::ByHash` and `!group.is_single()`, `keys.retain(|k|
  *k as usize != group.expr().len())`. Guard on `group.is_single()`, not `grouping_sets.is_empty()`
  — `grouping_sets` is moved into the body at `:323`/`:340`. Optional: refuse any remaining key
  `>= group.expr().len()` (closes hacks-audit finding 12's last bullet). No hasher arm on either
  side; no C++; no wire field. Frozen surfaces: none (the `hash_exprs` *content* of tpcds q5's
  `#92 CudfRepartition` payload shrinks — `recipe-payloads.txt:3341-3343` regen with
  `PEACOCK_REWRITE_RECIPE_BYTES=1`, argued in the commit). Moves with it: six tp4 plan goldens
  (1 section per tpch file, 5 per tpcds — 18 sections: emit `hash=`/`hashed_on`, final merge
  `hashed_on`, the project and sort above gain `hashed_on`); nine execution sections authored
  where `skipped:` sat; `.result.txt` mode re-stamps; `corpus_cases.inc:97,218,251` and comments
  `:89-96, :210-216, :234-237, :244-247`; registry rows 6/81/134; `build-test.md:42` (q80 clause,
  N 447→456); new planner test asserting `emit.hash_keys == [0, 1]` via `translated_at_tp4(…, 0)`;
  #189 to archive, #190's cross-ref at `active-tickets.md:221`; `walk-drives-every-plan.md:126`,
  `-impl.md:106`, `declared-schemas-derived.md:99` drop #189 from refusal lists (helper's edit);
  companion `scripts/exec_model/tests/plan_helpers.py:121` emits on `keys`.
- **Contested points:** placement of the rebinding — the proposal's "after `grouping_sets` is
  built, before `:370`" does not compile past `:340`; code supports the review's `!group.is_single()`
  guard. Section counts (proposal "six sections each"): review's 1+5 per file is what `grep -c`
  gives. q5's registry row carries `65 97 152 189`, not `183` — review right, wording only.
  `SinglePartitioned` never carries a gid on DF 45 (`combine_partial_final_agg.rs:132` requires
  equal groups; `as_final()` never equals a grouping-set partial) — review closes the proposal's
  risk as a fact. None of these changes the fix.
- **Minimum corpus query:** `SELECT l_returnflag, sum(l_quantity) FROM lineitem GROUP BY
  ROLLUP(l_returnflag)` (tpch sf1), CPU, tp4-single (also tp4-rowgroup/sized). Plans and validates
  today with `hash=[l_returnflag@0, __grouping_id@1]`; refuses in `CpuEmitter::emit`
  (`cpu_backend/emit.rs:60`) with the comet message. Expected four rows. Not in testdata; the two
  projected columns read 5,310,789 B, just over the small-table threshold (aggregate-groupby's golden).
- **Cells re-enabled / next wall:** 9 cpu cells. gpu cells stay off: q5 on #152; q80 and
  rollup_over_join on #152/#183. q77 × tp4 stays on #175 (its `.inc` comment should stop naming #189
  as a second candidate). q14/q18/q22 × tp4 are behind #163 first, then this.
- **Overlaps and dependencies:** #65 — complementary, no shared line; with #189 first the gid never
  reaches a hasher and #65 becomes about the unload and expressions only; with #65 first a device
  tp4 rollup would `CUDF_FAIL` on UINT8 exactly where the CPU refuses. #163 — q14/q18/q22 × tp4
  need both. #137 — after the fix the grand-total row lands in `pmod(seed, N)` as
  `architecture.md:294-295` already says (false today).
- **Complexity:** S (~12 lines in one planner file, one unit test, mechanical golden churn confined
  to the listed sections; nine cpu sections authored, minutes).
- **Unticketed defects found:** `architecture.md:273-275` and `:294-295` are false today (drift the
  fix makes true). hacks-audit finding 12: nothing checks a shuffle's keys are below
  `key_columns`. Unsigned parquet columns as hash keys stay a run-time refusal on both engines — no
  corpus schema has one.

## #65 — __grouping_id encoding doesn't match DataFusion's GROUPING()

- **Status after research:** live; disables nothing today. Wall behind #23 + #143 for q70/q86 (both
  window queries), and behind #152/#183/#175/#163 for every device rollup cell. Registry's nine `65`
  rows are co-attributions, nowhere the deciding one.
- **Issue:** the C++ expansion folds `gid |= (1 << i)` into an `int32_t` and materialises INT32
  (`cpp/src/operators/aggregate.cpp:388-392, :414-415`); DataFusion's `group_id_array` folds
  `(acc << 1) | is_null` MSB-first at `UInt8/16/32/64` by key count (`datafusion-physical-plan-45.0.0/
  src/aggregates/mod.rs:1267-1285`; `grouping_id_type`, `datafusion-expr/…/plan.rs:3223-3233`). A
  two-key rollup is 0, 1, 3 on the CPU and 0, 2, 3 on the device, and the column is four bytes
  against a declared one (`translator/aggregate.rs:261-266` reads the gid field off the partial's
  schema; every plan golden prints `__grouping_id:UInt8`). Three would-be readers all sit behind
  other tickets: a `GROUPING()` projection (q70/q86, #23/#143), the unload (`gpu_backend/mod.rs:179`
  refuses the type), the hasher (#189, and no device tp4 rollup cell on #152).
- **Root cause:** the wire carries masks and NULL placeholders and no per-set id and no output schema
  (`wire/aggregate_writer.rs:41-64`, `gpu_plan.fbs:393-407`), so the C++ invents the value and width;
  its comment (`:384-387`) says "only has to be DISTINCT per set", true for the merge above it and
  false for every other reader. Nothing pins the C++ side: the walk's ROLLUP compares after the
  projection drops the id; `SumByKeyAndGroupingId` manufactures the id with an Int64 literal
  (`test_gpu_executors/contract.rs:158-165`); the Python model pins the wrong rule by name.
- **Fix (as corrected by the review):** `aggregate.cpp`: a static `grouping_id_column(gid, nkeys,
  rows)` picking `numeric_scalar<uint8_t/16/32/64>` by `nkeys <= 8/16/32`; the loop folds
  `gid = (gid << 1) | masked` into a `uint64_t`; `:414-415` calls the helper; optional `nkeys > 64`
  throw mirroring DataFusion's own limit. New `executor_cases.inc` case `SumOverRollup`
  (`sum(v) GROUP BY ROLLUP(k, v)`, nine rows `k|v|__grouping_id|sum(v)`, the middle set `1` not `2`)
  with an arm in both `emitted` matches (`test_cpu_executors.rs` beside `:245`,
  `test_gpu_executors/contract.rs` beside `:158`) — red on the device twice (Int32 vs UInt8 at the
  export, `a|NULL|2|12`). Required, not optional: a recipe-walk test `const GROUPING` beside
  `ROLLUP` (`test_gpu_recipe_walk.rs:640`) running the section-5 query at `ONE_LANE` — the one pin
  on the *reading* path (finalize column copy, `cudf::cast`). Python: `aggregates.py:264-271`
  fold + `import functools` (or a loop), `test_operators.py:548,566`, **and**
  `test_end_to_end.py:310` (`{0, 2, 3}` → `{0, 1, 3}`) — three pins, not two; runs in cost-report CI.
  Frozen surfaces: none. No golden regenerates (the payload prints masks and names, never an id).
  Moves with it: `architecture.md:276` ("materializes an INT32 constant") and `:284-286`
  ("0, 2, 3 … not DataFusion's"); `build-test.md:19` walk N 10→11, `:20` "Eleven rows" → twelve,
  `:5` totals; `aggregate.cpp:329-335, :384-387` comments; `translator/tests.rs:653-655` sentence;
  `executor_cases.inc:50` expect doc; registry rows 6/15/19/23/68/71/78/81/87 lose `65`;
  `tickets.md:19, :278-283, :720`.
- **Contested points:** proposal says "nothing else reads the value" in the Python model — false,
  `scripts/exec_model/tests/test_end_to_end.py:310` asserts `{0, 2, 3}` (opened; review right,
  CI red at first push otherwise). Proposal says "#65 was outside the audit's scope
  (`hacks-audit.md:8-9`)" — those lines are the exclusion list and #65 is not on it; the audit did
  not *read* the expansion (`:372`). Review right. `__grouping_id` under `cpp/` hits four comments
  plus `:431`, not `:431` only — conclusion (no scaffolding) holds.
- **Minimum corpus query:** `SELECT n_regionkey, n_nationkey, GROUPING(n_regionkey, n_nationkey) AS
  g, count(*) FROM nation GROUP BY ROLLUP(n_regionkey, n_nationkey)` (tpch sf1; integer keys and an
  Int64 count so #183/#187 cannot fire; nation is one lane at every mode). CPU answers 31 rows with
  g ∈ {0,1,3} at every mode; the device answers the five subtotal rows with `g=2` — a silent wrong
  answer. Vehicle: the recipe walk at `ONE_LANE`. Whether DF 45 plans a select-list `GROUPING()` is
  settled at the first run (the rewrite fires on an `Aggregate`'s `aggr_expr`; q70/q86 die on a call
  inside a window). Not in testdata.
- **Cells re-enabled / next wall:** none directly. Precondition for any device rollup cell (behind
  #152/#183/#175/#163) and for q70/q86 (behind #23, #143, plus two device gaps: no unsigned literal
  in `build_scalar`/`build_expr` (`expr.cpp:453-497, :157-245`) and no `BitwiseShiftLeft/Right` in
  `fb_to_binop` (`:498-521`) although the fbs and the Rust IR carry them — single-key `GROUPING(x)`
  needs both). A corpus line over `nation` would be the first device rollup cell: an M-sized
  separate decision.
- **Overlaps and dependencies:** #189 (above, complementary; either order, #189 first is cleaner).
  #164 — after the fix every rollup stops being its first false positive. #144 — a different gid.
  #23/#143 — the queries that would read the value.
- **Complexity:** S (~+20/−10 C++ lines, one contract case ~70 lines, one walk test, six Python
  lines, five sentences; a device build and `test_gpu_executors` + `test_gpu_recipe_walk` on shad-gpu).
- **Unticketed defects found:** no unsigned literal and no shift-op arm in `cpp/src/expr.cpp`
  though `gpu_plan.fbs:85-86` carries them (needed by `GROUPING(x)`); `nkeys > 64` wraps silently
  on the device unless the throw is added; `SumByKeyAndGroupingId`'s device half keeps an Int64 id
  because a UInt8 literal cannot be built on a device.

## #62 — count(DISTINCT) ignores the DISTINCT flag in GpuAggregate

- **Status after research:** live — misdiagnosed in location (real cause: a plan-time refusal in
  the translator, `translator/aggregate.rs:118-124`; no backend "ignores" the flag — the wire never
  sets it, `aggregate_writer.rs:177`, and the C++ guard `aggregate.cpp:143-155` is unreachable).
- **Issue:** DataFusion removes DISTINCT only via `SingleDistinctToGroupBy`, whose precondition is
  that every companion is sum/min/max (`single_distinct_to_groupby.rs:86-91`); a `count`/`avg`
  companion — q28's `avg(x), count(x), count(DISTINCT x)` — leaves the flag on and `:119` refuses.
  q28 is out of the corpus entirely (registry `:29` `plan_status=fail`, all five `.plans.txt` carry
  the `(#62)` refusal); pinned by `test_planner_join_refusals.rs:104-117`.
- **Root cause:** the IR has no distinct aggregator by design (`architecture.md:297-323`): a distinct
  argument becomes a group key of an inner aggregate. Today the lowering is DataFusion's, not ours;
  where DataFusion declines, no one lowers. DataFusion's limit is inherent to re-applying the *same*
  function; this engine already separates init from merge (`count` merges by `sum`), so the outer
  stage can run each companion's merge aggregators over the inner's state. A kernel fix (cuDF
  `nunique`) cannot work: per-batch `nunique` has no merge and the CPU's distinct state is a `List`.
- **Fix (as corrected by the review):** one file, `translator/aggregate.rs`. (a) `decompose` takes
  `(Arc<AggregateFunctionExpr>, InitFrom)` with `enum InitFrom { Values, State(&AggStateColumns) }`;
  under `State` the declared fields are the input schema's at `cols.positions`, **paired
  positionally with `rule.state`** — not through `declared_state` (`:71-89`), whose `[sum]`/`[count]`
  tag lookup does not match our `avg(…)$sum` names and would refuse q28's `avg` companion at a new
  line; init calls become the merge funcs over those columns (`Merge::Combined` → error);
  **nullability `true`** for every `State` column (its producer is an init-form `sum`/`min`/`max`,
  #180's rule), not the inner field's. (b) split `aggregate_sequence` (`:242-388`) into
  `Stage` + `sequence(input, stage, shuffle)`, the shortcut generalized to "one lane and a single
  batch" (no committed golden has a lanes=1 merge-only node, so nothing moves). (c)
  `distinct_argument`: refuse grouping sets (new ticket), multi-argument, two different distinct
  arguments (#144), a Welford companion (new ticket; no init-form `merge_m2` on either engine).
  (d) `distinct_stages`: inner groups on `G ++ [d]` (reuse the ordinal if `d` is already a key),
  companions' inits, no finalize; outer groups on `G`, companions as `State`, the distinct one as
  `Values` over a non-distinct twin built with `AggregateExprBuilder`; inner takes DataFusion's
  shuffle, outer `Shuffle::None` (valid by the subset rule, `plan/aggregate.rs:236-258`, and
  `regrouped_key_distribution` carrying the hash on `G`). For a `Count` companion under `State`,
  the finalize is `CASE WHEN o IS NULL THEN 0 ELSE o END` (the identity a sum-form merge cannot
  supply over zero rows; `UnaryOp::IsNull` exists on both device paths). Frozen surfaces: none
  (`recipe-payloads.txt` does not move — q28 adds no call shape; five tpcds `.plans.txt` q28
  sections go from refusal to tree). Moves with it: `test_planner_join_refusals.rs:104-117` replaced
  by #144/Welford/grouping-set refusals; translator tests (grouped and keyless at four lanes via
  `translated_at_tp4(sql, 0)`, `d` already a key, the `avg` companion explicitly); e2e cases with
  `count` companions over a NULL-bearing column plus the empty-filter keyless case; one walk case at
  `TWO_LANES`; registry `:29` plan cells enabled, cpu/gpu disabled on `163 152 187`, `62` dropped from
  `:17,:95,:96,:116`; `corpus_cases.inc` q28 at `none, none`; comments at `plan/mod.rs:52-53`,
  `aggregate_writer.rs:149-158`, `aggregate.cpp:143-149` (hacks-audit finding 10: keep the guard,
  fix the comment); `architecture.md:311-315`; `build-test.md:42` (38 queries, fourteen on #163).
- **Contested points:** F1 — `declared_state` (`:71-89`) matches by DataFusion's tag suffix; the
  inner's names are ours. Code supports the review: positional pairing is required. F2 — outer
  companion nullability copied from `count`'s non-nullable inner field while the producer is `sum`;
  `check_state_layout` compares types only (`cpu_backend/mod.rs:467-491`). Code supports the review:
  keyless `count(x), count(DISTINCT x)` over a filter that keeps nothing refuses at tp1 (the empty
  batch traverses, the outer shortcut's DataFusion partial emits one row with `sum = NULL`), or
  answers NULL where SQL says 0 once the flag is fixed — unnamed by the proposal. F3 — same lines
  as #163; see dependencies.
- **Minimum corpus query:** `SELECT l_returnflag, count(l_quantity), count(DISTINCT l_quantity) FROM
  lineitem WHERE l_shipdate < DATE '1992-02-01' GROUP BY l_returnflag`, and the keyless form
  (tpch sf1), all five modes, both backends; today refused by `planner::plan` at `aggregate.rs:119`
  before a backend exists. Plus the review's empty-filter case `SELECT count(ss_customer_sk),
  count(DISTINCT ss_customer_sk) FROM store_sales WHERE ss_quantity < 0` → `0, 0`. Not in testdata.
- **Cells re-enabled / next wall:** q28's five plan cells now; q28 cpu × 5 with #163 (the `avg`);
  q28 gpu × 5 behind #152 (`GpuCrossJoin` copies its build side), #187, #163.
- **Overlaps and dependencies:** #163 must land first — same fifteen lines of `decompose`
  (`:148-157`), and #163's `arg_type` from `aggregate.expressions()` is wrong under `State` (the
  argument type is the inner's declared type at `cols.positions[i]`). #180 — the nullability rule is
  the same; F2 is a reach of the same NULL #180's fix does not cover. #144 — the message it says the
  planner gives arrives with this fix. Exec-model already models this lowering
  (`test_end_to_end.py:383`), uncited by the proposal.
- **Complexity:** M (one production file, ~+190/−15 with the corrections, seven tests, one device
  case, five plan goldens one section each; the `sequence` split goes through the function every
  corpus aggregate uses and is proven by an unchanged golden set).
- **Unticketed defects found:** a coercion cast makes the common non-Column `d` (`avg(DISTINCT
  int_col)`), refused at run time by the C++ as any `GROUP BY <expr>` is (`aggregate.cpp:163`) —
  pre-existing. Stale `62` tags on four registry rows.

## #55 — q66: two-phase decimal aggregate ignores the partial-phase divisor cast

- **Status after research:** stale — unreachable since the aggregate sequence (6123eb06, T4/T5
  translation layer) and the wire's `Merge` mode with the C++ positional state read (78400912).
  Needs one device pin; no engine change.
- **Issue:** the ticket records a Final phase re-evaluating the init's argument
  (`sum(x / CAST(w_warehouse_sq_ft))`) against the partial state, where the cast column does not
  exist. Today: `decompose` translates the argument once against the partial's input and attaches
  it to init calls only (`translator/aggregate.rs:139-142, :159-165`); merge calls take
  `Expr::column(state_at + offset)` (`:167-189`); the wire writes only `Partial`/`Merge`
  (`wire/aggregate_writer.rs:78-81`, exhaustive on `Phase`); the C++ reads a merge's input
  positionally — `reads_state` (`aggregate.cpp:509`), `req.values = tv.column(in_off)` (`:706`) —
  and the keyless path reads `args` only when `!is_final` (`:184-215`; `Merge` is not `is_final`,
  `:138-139`, and a merge's arg is a `ColumnRef` at its own position). q66's cpu cells are green at
  all five modes (`corpus_cases.inc:193`); its gpu cells are off on #152 (four modes) and #183
  (tp1-single, `Utf8View` keys). Registry `:67` carries `55 152 183` — `55` holds nothing off.
- **Root cause:** three layers, each sufficient, make the recorded mechanism unbuildable. The divide
  runs once, in `Partial`, via `arg_col` → `build_column` (`aggregate.cpp:447-456`) →
  `build_column_binary`'s decimal arm (`expr.cpp:589-600`), numerator pre-scaled from
  `out_decimal_scale`; the digits agree with arrow's `Div` (`numeric.rs:791-819`) by the same
  integer quotient and truncation — the arm `an_average_finalizes_to_the_digits_the_oracle_computes`
  already proves on a device. Git confirms the old shape: at fe046022 `get_values_col` evaluated
  `args` in every phase.
- **Fix (as corrected by the review):** (3a) `test_gpu_recipe_walk.rs`: `const SUM_OF_QUOTIENTS`
  (below) and a test at `TWO_LANES` asserting `== 4` `Aggregate{merge:false}`, `== 6`
  `Aggregate{merge:true}` (inner per lane + across the shuffle, outer across it alone — the loader
  declares `MultipleBatches` whatever the batching, `plan/source.rs:65-71`, so the per-lane merge is
  emitted; `SUM_BY_FLAG` at `:685-696` pins the same shape), `== 4` finalize projects; add the query
  to the kinds list `:816-828`. (3b) registry `:67` → `152 183`; #55 to the archive under Stale
  citing the three layers and fe046022; `tickets.md:19` contents. (3c) `walk-drives-every-plan.md:126`,
  `-impl.md:106`, `declared-schemas-derived.md:59,96` drop #55 from the refusal lists (helper's
  edit; the "aborts the walk today" claim is inherited, never observed); `build-test.md:19` walk N
  10→11, `:5` totals; `test-layout.md:177` count. Leave `plan/mod.rs:61`, `architecture.md:60-63`,
  `frame.py:29` (rationale, resolves to the archive). Frozen surfaces: none. No golden regenerates.
- **Contested points:** proposal's plan model ("no per-lane merge at `OneBatchPerLane`", "both
  aggregates take the shortcut at `ONE_LANE`") — code supports the review (`source.rs:65-71`,
  `plan/aggregate.rs` inherits the layout, `aggregate.rs:361`); the proposed `>= 2` would pass but
  describes a plan the translator does not produce. Evidence attributions: "tpcds q19 at tp1-single"
  has no device cell (`corpus_cases.inc:181`); the enabled q19 is tpch's and keyless, as is q6
  (`tp1-single.plans.txt:758, :1300`, `group_by=[]`), so the *grouped* `arg_col` arm over a computed
  argument has never run on a device — review right; the probe is its first run. Landing: two
  approved tasks own the walk file (`test-layout.md:177` moves it; `walk-drives-every-plan.md:126`
  lists #55 as a refusal to observe) — make 3a the first item of task 13 or sequence it before task 4.
- **Minimum corpus query:** `SELECT l_linenumber, sum(price / l_linenumber) AS price_per_line,
  sum(quantity / l_linenumber) AS qty_per_line FROM (SELECT l_linenumber, l_suppkey,
  sum(l_extendedprice) AS price, sum(l_quantity) AS quantity FROM lineitem GROUP BY l_linenumber,
  l_suppkey) x GROUP BY l_linenumber` (tpch sf1), device via the walk at `TWO_LANES`. Plans today
  at every mode; predicted seven rows equal to DataFusion's to the sixth digit; a red at a
  `CudfAggregate{Partial}` reopens the ticket (or is the grouped `arg_col` arm itself — read the
  trail first). The one-level form `sum(l_extendedprice / l_linenumber)` reaches the same
  composition with half the calls; the inner level is kept so the numerator is a cuDF `SUM` output
  (q66's provenance) and the outer lands on `(38,6)`. Not in testdata.
- **Cells re-enabled / next wall:** none now; q66 gpu × 5 stay on #152 / #183. Dropping `55` means
  q66 comes back with no further triage when those clear. A q66-on-device run must read its failure
  seq: `#50`/`#52` = #55 alive; the unload = #183; anywhere else (24 CASE inits, union casts, four
  joins, a `Utf8View` sort) says nothing about #55.
- **Overlaps and dependencies:** #56 — same three layers, same closure shape, same file, same
  `build-test.md:19` bump and the same two owning tasks; land the two pins together. #163 — no
  shared line (q66's divide is DataFusion's own `BinaryExpr`, typed by DataFusion; `state_type(Sum,
  Decimal128(38,6))` is today's type). #63 — the third member of the class, closable the same way
  but behind #163 (q9).
- **Complexity:** S (one walk constant and test ~25 lines, one CSV cell, one archive entry, five
  page lines; one shad-gpu run of the walk).
- **Unticketed defects found:** `walk-drives-every-plan.md:126` / `-impl.md:106` /
  `declared-schemas-derived.md:59,96` carry an unverified claim that #55 aborts the walk today.
  `00-tickets.md`'s code-path column names a Final phase that does not exist on this wire.

## #56 — q2: CASE-over-string-equality inside a partial-phase sum

- **Status after research:** stale — same closure as #55 (unreachable since 6123eb06 / 78400912);
  the recorded error was the *Final* phase at fe046022 (`plan_executor.cpp:1132-1145` then evaluated
  `args` in every phase, and q2 kept a Partial/Final pair at tp1 because the union sums its inputs'
  partitions), not a Partial and not the AST. Needs one device pin and one CPU-tier routing pin.
- **Issue:** q2's fourteen `sum(CASE WHEN d_day_name = 'Sunday' THEN sales_price END)` sit in two
  grouped `CudfAggregate{Partial}` nodes (`tpcds.sf1/tp1-single.plans.txt == q2`). Today the CASE
  is evaluated once, in the init, by `arg_col` → `build_column` (`aggregate.cpp:447-455, :731`);
  `is_ast_able` refuses a `CaseExprNode` outright (`expr.cpp:405-407`) and a string literal on
  either side of a binary (`:419`), so the argument never enters `compute_column`; the chain is
  `build_column_case` (`:773-803`, no-else null fill `:790-795` from `make_default_constructed_scalar`
  with scale preserved) over `build_column_binary`'s column-scalar arm (`:609-616`) → `copy_if_else`
  → groupby SUM. q2's cpu cells are green at five modes; its gpu cells are off on #152 alone (the
  probe under the init unions web_sales and catalog_sales — two batches at every mode,
  `corpus_cases.inc:201-203, :207`). Registry `:3` carries `56 152`. #187 is not on tpcds/q2
  (the `q2` under #187 is tpch/q2, registry `:102`) — 00-tickets row corrected.
- **Root cause:** as #55: the merge takes state column refs (`translator/aggregate.rs:167-189`), the
  wire never writes `Final` (`aggregate_writer.rs:78-81`), the C++ reads a merge's input
  positionally (`aggregate.cpp:564, :577, :706`). The composition ran once by inference: T19 batch
  12 recorded #152 for q2 at tp1-single, and under the smallest-height rule the first probe
  batch's output climbs `#11` → `#12 Partial` → the accumulator before the second probe call
  refuses; a live #56 would have failed at `#12` first. The raw error text is not in the tree.
- **Fix (as corrected by the review):** (3a) `test_gpu_recipe_walk.rs`: `const SUM_BY_FLAG_CASE`
  (below) beside `SUM_BY_FLAG` (`:634`) and a test at `TWO_LANES` asserting `== 2` inits, `== 4`
  merges (the shape `:684-698` already pins, plus one `PlainProject` for the aliases, in `PROVEN`);
  its doc says it is the first device run of the grouped computed-argument arm and of the no-else
  fill; add to the kinds list `:816-828`. (3b) `cpp/tests/cpu/test_executor.cpp` `AstRouting.IsAstAble`
  (`:81-119`): two arms — `string col = string literal` → false, `CASE` → false — with helpers
  built through `fb::ScalarValueBuilder` (the positional `CreateScalarValue`'s second parameter is
  `is_null`, `typed-nulls.md:76-93`); extend the header comment `:78-79`. (3c) registry `:3` → `152`;
  #56 to the archive citing fe046022 and the three layers; `archived-tickets.md:24-27` "two files"
  → three while open. (3e) `build-test.md:19` walk N 10→11, `:5` totals (C++ stays 11 — the arms
  sit inside an existing case). Leave `plan/mod.rs:61`, `architecture.md:60-63`, `frame.py:29`.
  Frozen surfaces: none. Nothing regenerates.
- **Contested points:** proposal's proven list says "a computed init argument through `arg_col` →
  `build_column`: q19's and q6's, both on the device" — both are keyless (`tp1-single.plans.txt:758,
  :1300`, `group_by=[]`) and take `get_values_col` at `aggregate.cpp:184-215` (the branch at `:231`);
  the grouped `arg_col` non-`ColumnRef` arm (`:452`, three lines identical to `:203`) has no device
  run. Code supports the review. The "unproven" list (no-else arm, composition) contradicts the
  next paragraph; both are one inference from T19. `build_copy` (`gpu_backend/join.rs:304-313`) is
  today's refusal, not necessarily T19's (`archived-tasks.md:1825-1840`: q11 took 336 probe batches
  then); the inference holds under either.
- **Minimum corpus query:** `SELECT l_linenumber, sum(CASE WHEN l_returnflag = 'A' THEN
  l_extendedprice ELSE NULL END) AS a_price, sum(CASE WHEN l_returnflag = 'R' THEN l_extendedprice
  ELSE NULL END) AS r_price FROM lineitem GROUP BY l_linenumber` (tpch sf1), device via the walk at
  `TWO_LANES`; the `ELSE NULL` is dropped before translation (q2's golden prints `THEN … END`), so
  the C++ takes the null-fill arm. Plans today at every mode; predicted seven rows equal to the
  cent. Through the corpus tier it would fail at the unload on #187 (`(25,2)` exported at 38) —
  itself proof the Partial returned 0. Not in testdata.
- **Cells re-enabled / next wall:** none now; q2 gpu × 5 stay on #152 at every mode. Behind #152 and
  not this ticket's: the per-batch lists at the joins (CPU chunks 2,153,556 join rows into 8192-row
  batches; the device emits one table per probe call — the divergence 152-review F2 names) and,
  unrun on a device, the final `Decimal128(17,2) / Decimal128(17,2)` → `CAST … AS Float64` → `round`.
- **Overlaps and dependencies:** #55 (same closure, same file, same page counts, same owning tasks
  — land together; two pins make the walk N 12). #57 — different in kind (a real unimplemented
  value-form CASE behind the `:779` guard; q39 behind #163). #63 — same mechanism one level up
  (an expression evaluated against a table it was not written for); q9 behind #163.
- **Complexity:** S (one walk test ~25 lines, two gtest arms + helpers ~35 lines, one CSV cell, one
  archive entry, one build-test row; one shad-gpu walk run and one `ctest -L cpu`).
- **Unticketed defects found:** `string_col = string_col` with no literal passes `is_ast_able`
  (`expr.cpp:419-435`) and would reach `compute_column`; no corpus query puts that on a device and no
  test pins either outcome. `ProjectSqrtThroughTheColumnPath`'s literals go through the one-position-
  off helper (`typed-nulls.md`), invisible because both literals are 0. `archived-tickets.md:24-27`
  says the widget resolves through two files; `cost-report/src/main.rs:452-456` reads three.

## Cross-ticket observations

- **One rule, stated by three tickets:** a state column's type *and* nullability are those of the
  SQL aggregate the executor runs it as, never the accumulator DataFusion planned or the field the
  column was read from. #163 fixes the type (`decompose`), #180 fixes what an empty lane owes
  (`identity_state`), #62 must apply both under its `InitFrom::State` arm (positional pairing,
  nullable `true`, `state_type(merge_func, inner type)`). Land #163 → #180 → #62 in that order; #62
  and #163 edit the same fifteen lines (`translator/aggregate.rs:148-157`).
- **The same NULL reached three ways:** a merge-only node over *no batch* (#180, fixed there); a
  finalizing node over no batch (#180's 3b); a `GpuAggregate` shortcut over an *empty batch* whose
  aggregator is sum-form over a non-nullable declaration (#62 review F2 — only exists after #62's
  lowering, needs the CASE finalize or a residual ticket beside #199). Keyless `sum`/`min`/`max`
  over an empty lane silently emit a NULL identity row today and are right by luck.
- **The gid pair:** #189 takes the gid out of the hash; #65 makes its value and width DataFusion's.
  No shared line, either order works, #189 first is cleaner (the gid stops reaching any hasher, so
  #65 is about the unload and expressions only). After both, `architecture.md:273-275, :276,
  :284-286, :294-295` become true; the first and last are false today with nothing fixing them.
- **The stale pair:** #55 and #56 are the same closure — the aggregate sequence (6123eb06) and the
  wire's `Merge` mode (78400912) made the legacy Final-phase re-evaluation unbuildable. Both close
  with a recipe-walk pin in `test_gpu_recipe_walk.rs`, both bump `build-test.md:19` (walk N 10→12
  together) and `:5`, and both collide with two approved tasks that own the walk (`test-layout.md`
  task 4 moves the file; `walk-drives-every-plan.md` task 13 lists #55 among refusals it will
  observe). Ship them as one item at the head of task 13 or before task 4. #63 is the third member
  of the class and closable the same way, but q9 is behind #163 on both engines. The grouped
  `arg_col` arm over a computed argument (`aggregate.cpp:452`) has never run on a device — both
  proposals claimed otherwise; both pins are its first run.
- **Registry drift shared by the group:** `62` on four rows that run (q16 q94 q95 tpch q16); `65`
  on nine rows as a co-attribution; `55`/`56` on rows held off by #152/#183 alone; `97` archived on
  q5's row. `build-test.md:42` says "37 queries", "thirteen on #163" (23), names q96/q88 on #180 but
  not q90; `:5` totals move with every enabled cell and every walk test. Every fix in this group
  edits that line; consolidate the edit.
- **Two contradictions between proposals:** #163's `arg_type` derivation from
  `aggregate.expressions()` is wrong under #62's `State` arm (62-review F3) — reconciled above.
  #180's proposal and #189's review both reason about a lane the hash misses under a *grouped*
  final merge: it answers nothing rather than a NULL identity row, so #189's cells do not meet
  #180 — consistent, worth writing down once.
- **Minimum-query pattern the reviews corrected twice:** at tp4-single the small-table rule is off,
  so any one-row-group table (store, supplier, nation) plans four lanes with three empty
  (`translator/nodes.rs:373-389`). It is the cheapest empty-lane generator in the tree (#180's Query
  0) and the reason #163's supplier query has a different shape at tp4-single than at the other
  tp4 modes.
