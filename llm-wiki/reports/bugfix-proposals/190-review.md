# #190 review — the CPU backend drops a nested-loop join's projection

Read at master 188c23ce (the proposal read c18e063a; the three commits between touch only
`llm-wiki/`, so every code cite below is against identical code). Paths relative to
`/media/data/peacockdb`. Nothing was built or run.

## 1. Verdict

**Needs changes** — the one-argument fix, its regression test and the ten cpu cells are sound
and I found nothing that would make them red; what is wrong is the device-cell bookkeeping in
§3c/§6/§7: q54's tp1-single gpu cell is #152 by structure (no run needed), and q11's tp1-single
gpu cell cannot pass the read-only section compare whatever a device does, so the "possible bonus
device cell each" and the run budgeted to find it should be restated.

## 2. Findings

### F1 — q54's gpu tp1-single cell is #152 by structure, not "genuinely unknown" (important)

The proposal (§3c, §6) says every join in q54 sees one probe batch at tp1-single, so #152 is
silent and a device run decides the cell. Not so: q54's plan has

    GpuHashJoin: join_type=Inner, on=[(i_item_sk@0, item_sk@2)] …        (tpcds.sf1/tp1-single.plans.txt:5097)
      GpuCoalesceAllBatches(GpuFilter(item))
      GpuMergePartitions: lanes=1                                          (:5101)
        GpuUnion: lanes=2  — catalog_sales ∪ web_sales                     (:5102)

and its recipe is `#11 CudfHashJoin{Inner}, build copy, batch` (same section, `--- recipes ---`).
A merge forwards one batch per source lane (architecture.md, "Determinism rules"), so this join's
probe arrives in two batches at every mode, and the second one has no build side to be given
(`gpu_backend/join.rs:304-316`, `build_copy`). The corpus already records this exact mechanism
for tpcds q2 — "q2 is #152 at tp1-single as well … its probe side unions web_sales and
catalog_sales, so the join sees several batches at every mode" (`corpus_cases.inc:201-203`) —
and for q71 (`:187-190`).

Correction: registry row 55 (`tpcds,1,q54`) takes `152` on all five gpu cells, honestly and
without a device run; the `corpus_cases.inc` comment for batch 13 says so. Drop q54 from the
"possibly on" list in §6.

### F2 — q11's gpu tp1-single cell cannot come on by any device run under today's device tier (important)

§3c/§6 hold out a "bonus device cell" for tpch q11 at tp1-single if the device run passes. By
reading, the run's *result* is irrelevant to the cell: the device tier asserts the whole rendered
section byte for byte against the cpu-authored one — `gpu_case` → `corpus_golden::assert_section(…,
render_run(tree, report))` (`tests/common/corpus_gpu.rs:99-103`), and `render_run` writes every
emitted batch's rows and bytes verbatim (`src/plan_text/run_text.rs:59-80`). The cpu section will
carry DataFusion's 8192-row output chunking wherever a hash join or partial aggregate emits more
than 8192 rows: q11's `GpuHashJoin on=[(s_suppkey@0, ps_suppkey@0)]` joins 800k partsupp rows
against 10k suppliers and emits ~800k rows, i.e. ~98 batches on the cpu, and every node above it
(the `GpuProject`, the `(n_nationkey, s_nationkey)` join, the partial `GpuAggregate`) inherits
that batch count. A device call answers one table (`execute_node` accepts exactly one handle,
`gpu_backend/mod.rs:217-236`; `execute_hash_join` returns one `TableResult`), so the device
section has one batch at each of those nodes. Evidence that the cpu goldens really carry the
chunking: tpch q5 at tp1-single, `GpuHashJoin on=[(o_orderkey@0, l_orderkey@0)]`:
`in_rows=[[227597],[6001215]] batch_rows=[[8192,8192,…]]` (`tpch.sf1/tp1-single-mini.cpu.txt`,
`== q5`); tpcds q48's top join at tp1-single takes 53 probe batches for the same reason. The six
device cells that pass (q6 ×5, q19 at tp1-single) are exactly the plans where nothing after a
join or aggregate exceeds 8192 rows.

So the tp1-single run can *attribute* q11's cell — and is worth one job for what it says about
the non-AST nested-loop path with a `CAST(… AS Decimal128(38,15))` filter and a float→decimal
project on a device — but it cannot enable it. The first differing line will most likely be the
`GpuAggregateBatches` `in_rows` (cpu: ~98 partial batches' rows; device: one), which is #185's
signature. Note this reading contradicts #185's sentence "every `batch_rows` entry and every byte
agrees" for q38 (whose `(d_date_sk, ws_sold_date_sk)` join is 18 cpu batches); the run settles it.

Correction: §6 "Possibly on" → "Off; the run names the ticket". §3c: registry row 111 takes `152`
for the four multi-batch modes; for tp1-single, whatever the run reports (expect #185 or a device
refusal on the decimal cast path), and the fallback without a device stays as the proposal wrote
it — `152` plus a comment saying tp1-single is unrun — since the registry refuses a disabled cell
with no ticket (`registry.rs:272-285`).

### F3 — `build-test.md:42` is stale beyond what the proposal moves (minor)

The row says "37 queries at the modes each is correct at … thirteen queries are out entirely on
#163". The registry has 26 queries with every cpu cell disabled: 23 on #163, q11 and q54 on #190,
tpcds q64 on #192. The fix's edit of that row (N 447 → 457) should not carry the wrong sentence
forward: after the fix, 24 out entirely, 23 of them on #163, one on #192.

### F4 — "the plan is the same shape at all five" is wrong for the tp4 modes (minor)

§5 says region and nation are one lane at every mode so the minimum query plans identically at all
five. At tp4-single and tp4-rowgroup the loads are four lanes and the translator inserts
`GpuMergePartitions` under the coalesce and under the probe (`tpch.sf1/tp4-single.plans.txt`,
`== nested-loop-join`). Immaterial — tp1-single is the mode named, and it is enough — but the
sentence should go.

### F5 — golden authoring has an order-of-operations trap the proposal does not name (minor)

`UPDATE_CANONICAL=1` is the whole-file form; a filtered run wants `PCK_UPDATE_SECTIONS=1`
(`tests/common/corpus_golden.rs:36-62`, and the panic at `:78-83` says so). And `mini.result.txt`
is authored only by the last mode a query declares — tp4-sized — so a developer who regenerates
q11 at tp1-single alone leaves `== q11` reading `skipped: not enabled at any mode`, and the next
verify run goes red naming it. Run all five modes (or at least tp4-sized) under the update
variable, for both queries.

### F6 — the regression test pins the drop and not the reorder (minor, optional)

`Some(vec![1, 3])` proves the width. The ticket's "uncaught half" is a projection that reorders
or drops while keeping the count; `Some(vec![3, 1])` with output `[("v", Int64), ("label", Utf8)]`
expecting `["20|c", "21|c"]` pins the ordinal mapping in ten lines. The hash-join twin has the
same gap, so this is optional rather than owed.

## 3. Claims verified

Opened and found true (line drift noted where it exists):

- `cpu_backend/join.rs:123-151` builds `NestedLoopJoinExec::try_new(…, &join_type, None)` at
  `:140-146`; the hash-join path maps `node.projection` to `Vec<usize>` at `:303-306` and passes it
  at `:313`. `one_call` (`:155-172`) takes the node's declared schema as `output`;
  `probe_and_fetch` (`:236-251`) relabels through `declared()` → `declared_as`
  (`cpu_backend/mod.rs:239-277`), whose `RecordBatch::try_new` refuses on column count before any
  value is read. Arrow's message text matches the proposal's quote.
- `GpuNestedLoopJoin.projection: Option<Vec<u32>>` (`plan/mod.rs:721-724`); `check_projection`
  called at `plan/join.rs:66-75`, defined at `:272-293` (bounds only, as described);
  `declared_width` at `plan/validate.rs:199-207`.
- Translator: `nested_loop_join` (`nodes.rs:465-520`) takes `join.left()` as build and
  `join.right()` as probe (`:492-493`), `projected(join.projection())` and
  `Schema::new(join.schema())` (`:517-518`, not 511-512); the predicate-free arm lands on
  `GpuCrossJoin` with the projection (`:483-490`); the `CrossJoinExec` arm passes `None` (`:106-116`).
- DataFusion 45: `NestedLoopJoinExec::try_new` (`nested_loop_join.rs:184-215`) accepts a
  projection for any join type; `compute_properties` projects the schema (`:288-295`);
  `with_new_children` re-passes `self.projection.clone()` (`:459-470`) so the `StreamSourceExec`
  swap in `execute_single_node` keeps it; `execute` maps `column_indices` through the projection
  (`:508-513`) and both `process_probe_batch` (`:881`) and `process_unmatched_build_batch` (`:939`)
  build from that list, so the Left form's padded rows are projected too. Projection embedding:
  `try_swapping_with_projection` (`:566-598`) tries `try_pushdown_through_join` and falls back to
  `try_embed_projection` (`projection.rs:381-433`), which returns `None` for an empty or identity
  list (`:388-397`); `join_allows_pushdown` (`:738-750`) fails the minimum query on
  `far_right_left_col_ind >= 0`; `update_join_filter` returning `None` aborts the pushdown
  (`:473-481`). `JoinSelection` runs before `ProjectionPushdown`, and `should_swap_join_order`
  does not swap region (smaller) under nation, so the minimum query's build is region and the
  plan is `GpuNestedLoopJoin{projection=[n_name@1], schema=[n_name]}`.
- Wire and device: `wire/join.rs:342-386` writes `projection`; `gpu_plan.fbs:448-458` carries it
  and `CudfCrossJoin` (`:440-443`) does not; `join.cpp:519-529` gathers the listed ordinals with
  their names, after the mask or the gather, and treats an empty vector as no projection;
  `scripts/exec_model/operators/recipe.py:298,311` apply `_project(…, node.projection)`.
- Goldens: a projecting `GpuNestedLoopJoin` appears in exactly tpch q11 (1), tpch q22 (1),
  tpcds q24 (1), tpcds q54 (2), identical at all five modes (2 + 3 per bench per mode); the other
  nested-loop shapes (`nested-loop-join`, `nested-loop-left-join`, tpcds q14 ×3) carry no
  projection; no golden shows a `GpuCrossJoin` projection. q11's crossed table is
  `[build:0 scalar, probe:0 ps_partkey, probe:1 sum]` and the projection `[1, 2]`, as described.
- Bookkeeping: `corpus_cases.inc:114` (q11, `none, none`), `:219` (q54), comments at `:103-106`
  and `:212-213`; registry rows 111 and 55 carry only `190`; `registry.rs:272-285` refuses a
  disabled cell with no ticket; `every_device_cell_has_a_cpu_cell_at_the_same_mode` is satisfied
  by the cpu cells coming on; `q11.duckdb_cost.txt` and `q54.duckdb_cost.txt` exist; the
  `mini.result.txt` and `<mode>-mini.cpu.txt` sections for both read `skipped`; the cap is
  `RESULT_GOLDEN_MAX_BYTES = 256 KiB` (`tests/common/mod.rs:37`) and an over-cap answer keeps its
  section with a marker rather than failing; the cost gate omits queries with no base `.cost.txt`
  (`cost-report/src/main.rs:1437`). Lib unit 435, corpus cpu 447 = 444 enabled cells + 3, grand
  total 1569 (`build-test.md:7,40,42`). `every_refusal_names_a_ticket_that_exists` reads both
  ticket files, so archiving #190 breaks nothing. The corpus run passes no budget
  (`corpus.rs:65`, `run::<CpuBackend>(…, None)`), so q54's 238 MB estimate is not a risk.
- hacks-audit: #190 is excluded by name (`hacks-audit.md:7-8`); no `bug_` test and no production
  branch references #190 (grep over `src/`, `tests/`, `cpp/src`, `scripts/exec_model`); the only
  records are the two `corpus_cases.inc` comments, two registry cells, and the spec mentions listed.
  The fix adds no third produced-vs-declared rule (§12) and leaves `without_build` (§4) alone.
- Regression test: `GpuNestedLoopJoin::new`'s argument order (`plan/mod.rs:731-739`), the helpers
  `greater`, `side`, `dim_columns`, `fact_columns`, `schema_of`, `drive`, `rows` all exist with the
  shapes the test uses (`cpu_backend/tests/join.rs:20-136, 342-362`); `[k, label, fk, v]` with
  `Some(vec![1, 3])` and `k > fk` over `FACT` gives exactly `c|20`, `c|21`; today `drive` panics
  in `probe_and_fetch` on the 4-vs-2 count, so the test is red before and green after. The Left
  variant's expected rows are right too.
- No enabled cpu cell, pinning test or plan golden moves: every enabled nested-loop node carries
  `projection: None`, for which the new code passes `None`; `<mode>.plans.txt` and
  `recipe-payloads.txt` are untouched. Nothing in `architecture.md` says the CPU drops a
  projection, so no sentence there is falsified.
- The q22/q24 claim: both rows are out on #163 (row 122 `59 80 97 163`, row 25 `45 163`), neither
  names #190, and both hold a projecting nested-loop join at every mode.

Not verified, and worth saying: whether q11 and q54 then match DataFusion exactly at all five
modes. q54's eight hash joins, its union-merge and its `round(CAST(… / 50 AS Float64))` project
have never run on the CPU corpus; every shape in it is exercised by an enabled query (`round` by
q2 and q78, the union-merge by q2), so a second defect is unlikely but is the run's to say.

## 4. Corrected proposal

Only §3c (the device attribution), §5 (one sentence), §6 and §7 change; §1–§4 and §8 stand.

### 3c. Corpus and registry — the gpu column

- `testdata/cost-registry.csv:55` (tpcds q54): the five `cpu_*` cells `disabled → enabled`; the
  five `gpu_*` cells stay `disabled`; `tickets` becomes `152`. No device run: the
  `(i_item_sk, item_sk)` join probes a merge over a two-lane union, so its build copy is refused
  on the second batch at every mode, exactly as tpcds q2's is. The batch-13 comment in
  `corpus_cases.inc` says this in one sentence.
- `testdata/cost-registry.csv:111` (tpch q11): the five `cpu_*` cells `enabled`; the four
  non-tp1-single gpu cells are #152 by structure (partsupp streams in 7 probe batches at
  tp1-rowgroup; the tp4 modes scatter). The tp1-single gpu cell is off whatever happens: the
  cpu-authored section carries ~98-batch outputs at the partsupp join and above, and a device
  answers one table per call. Spend one shad-gpu job on it (`PCK_TEST_FILTER=q11` over
  `test_gpu_corpus` with `tp1_single` on the gpu side) to learn what the device does with the
  non-AST nested-loop path and the float→decimal project, and name the ticket it reports
  (expect #185's section mismatch if the run completes); without a device, write `152` and say in
  the batch-6 comment that tp1-single is unrun.
- The 22 golden sections: author them by running all five modes of both queries under
  `PCK_UPDATE_SECTIONS=1` (the filtered form), since `mini.result.txt` is written only by the
  last declared mode.
- `build-test.md:42`: N 447 → 457, and rewrite the disabled list to what the registry says —
  24 queries out entirely, 23 on #163 and tpcds q64 on #192 — rather than carrying "thirteen".

### 5. Minimum corpus query

Unchanged, minus the sentence "the plan is the same shape at all five": at the tp4 modes the
loads are four lanes and merges appear under the coalesce and the probe. tp1-single is the mode.

### 6. Cells re-enabled

- Back on: `tpch/q11` × 5 and `tpcds/q54` × 5, cpu. Ten cells.
- Off, attributed by reading: all ten gpu cells of both — #152 on nine of them (q54's five, q11's
  four multi-batch modes); q11's tp1-single gpu cell is off on whatever the one device run names.
  No bonus device cell is available from this fix: the device tier can pass only a plan in which
  no join or aggregate output exceeds 8192 rows at that mode, and q11's partsupp join does.
- Stay off: `tpch/q22` × 5 and `tpcds/q24` × 5 on #163, which this fix clears on the #190 axis.

### 7. Risks and unknowns

As the proposal has them, with the second bullet replaced: the device cells are not unknown —
nine are #152 by the plan's own shape, and the tenth cannot pass the read-only compare — so the
device run is for attribution and for what it reveals about the device's decimal nested-loop
path, not for a cell. One thing the run may also show: #185's claim that the cpu and device
sections agree on every `batch_rows` entry is at odds with the 8192-row chunking the cpu goldens
carry at chained joins; if the q11 run reports a `batch_rows` difference first, that is a
sentence in #185 to correct, not a new ticket.

## 5. Complexity

**S**, agreeing with the proposal. The code is one argument; the cost is the ten-cell golden run
and the bookkeeping. The device run the proposal budgets is now optional-for-attribution rather
than required-for-a-cell, which makes it smaller, not larger.
