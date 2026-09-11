# #185 — review of the proposal

Read at master `188c23ce` (only `llm-wiki/` moved since the proposal's `c18e063a`; every code
path and golden cited is byte-identical). No build, no run. Paths relative to
`/media/data/peacockdb`; `core/` = `peacockdb-core/src/`, `tests/` = `peacockdb-core/tests/`.

## 1. Verdict

**Needs changes.** The re-diagnosis (the CPU join hands DataFusion's chunking to the driver, the
device answers one table per call, and `in_rows` is the driver's engine-agnostic sum) is correct
and the one-file fix is right; but the first "minimum corpus query" would not plan (#158 takes it
before any executor), and "cells re-enabled" rests on results the device tier never compared.

## 2. Findings

1. **The first minimum query is refused at plan time.** `SELECT count(*) FROM region, nation` is a
   keyless `count(*)` over a `CrossJoinExec`, and DataFusion 45's `AggregateStatistics` rule answers
   that from statistics: `CrossJoinExec::statistics()` multiplies the two sides' row counts
   (`datafusion-physical-plan-45.0.0/src/joins/cross_join.rs:387-395`), `Precision::multiply` keeps
   `Exact × Exact = Exact` (`datafusion-common/src/stats.rs:146-148`), parquet row counts are Exact
   (`ListingOptions::new` defaults `collect_stat: true`, which is why `SELECT count(*) FROM nation`
   is #158), and the rule then replaces the aggregate with a `ProjectionExec` over
   `PlaceholderRowExec` (`datafusion-physical-optimizer-45.0.0/src/aggregate_statistics.rs:49-85`).
   The planner refuses that naming #158 — `tests/test_planner_join_refusals.rs:118-128` pins the
   single-table form. Ticket #158's "the rule cannot fire ... JOIN" (`tickets.md:235-236`) is true
   of hash joins, whose cardinality estimate is Inexact, and not of a cross join. The proposal's own
   claim "plans today, runs on both engines, returns 125" was not checkable without a run and is
   wrong. **Severity: important** — the template asks for exactly this. **Correction:** drop the
   cross-join form. The second query, `SELECT count(*) FROM supplier s JOIN nation n ON
   s.s_nationkey = n.n_nationkey`, is the minimum: a hash join's statistics are Inexact so the
   aggregate survives, `supplier` is one row group (`tp4-single-mini.cpu.txt` shows
   `partition_groups=[[[0]],[],[],[]]`) so one probe batch at every mode, DataFusion's join
   selection makes the 25-row `nation` the build side, and 10000 output rows chunk as
   `[8192, 1808]`. If a cross-join reproduction is wanted, `SELECT * FROM region, nation` is already
   the corpus's `tpch/cross-join` (`tp1-single-mini.cpu.txt:785-796`, `batch_rows=[[25,25,25,25,25]]`
   against one table on the device), reaching the unload directly — its device cell is off on #183
   because of the string columns, not on this.

2. **"All of which already run to completion on the device with matching results" is not
   evidenced.** `gpu_case` asserts the section before the result (`tests/common/corpus_gpu.rs:101-111`):
   `assert_section` panics on the first differing line, so `assert_result` never ran for any of the
   nine cells. The ticket's own "no result comparison at any tolerance could see it" is reasoning
   about `in_rows`, not an observation. The device's answers for q38 q48 q87 q88 q93 q96 q3 q14
   join_int at tp1-single have never been compared against `mini.result.txt`. **Severity:
   important** — it changes what the developer owes: section 6 is a prediction, and the task has
   to include a shad-gpu run of `test_gpu_corpus` with the nine cells flipped, keeping off (with a
   fresh ticket) any cell that fails past the join. Also say so in the archived correction of the
   ticket, since the ticket's "every `batch_rows` entry and every byte agrees" is likewise a reading
   of `line_difference`'s first-line output (`tests/common/golden_text.rs:159-187`) and is contradicted
   by the code: the device join returns at most one batch per call (`core/executor/gpu_backend/join.rs:164-179`),
   so its join line cannot match a 34-entry `batch_rows`.

3. **"One `RecordBatch` per `batch_size` (8192) rows of join output" overstates the rule.**
   `batch_size` bounds the matched pairs one `lookup_join_hashmap` call returns
   (`hash_join.rs:1472-1479`); an outer type then appends the range's unmatched probe rows, so a
   Right join's chunks are far larger — q93's Right join emits 36 chunks of ~82K rows
   (`tpcds.sf1/tp1-single-mini.cpu.txt:4954`) — and a build-side semi finish yields empties per
   probe range plus the answer, 19 chunks over 3.79M keys for q4 (`tpch.sf1/tp1-single-mini.cpu.txt:131-132`).
   Nothing in the fix depends on the constant, but the doc comment the fix adds must say "whatever
   DataFusion chunked", not "8192-row chunks", or it is wrong on the day it lands. **Severity:
   minor.**

4. **A third `architecture.md` sentence becomes true, and it is the one a reviewer will quote.**
   Determinism rules: "Batch boundaries are a pure function of the plan: the loader's come from the
   committed mapping, Exec ops are 1:1, accumulators emit at defined points." Today the CPU join's
   boundaries come from a session config (`build_session_state`, `core/lib.rs:25-36`, leaves
   `batch_size` at 8192) and DataFusion's probe loop, not from the plan. No wording change needed;
   list it with the two the proposal names so the completeness pass does not report it as drift.
   The "optionally one sentence" addition to Joins should go: the page grows only when a human
   asks (`prompts.md`, Coordinator, "growth is not"). **Severity: minor.**

5. **The stated reason for leaving the driver guard out is wrong.** The check
   `a_build_side_that_produced_two_batches_is_an_error` (`core/executor/driver/tests/flow.rs:251-258`)
   scripts the *accumulator* under the build side and trips `single_partition.rs:193-197`, which
   reads the build child's queue; a guard on a join's own `probe_and_fetch`/`finish_and_fetch`
   returning more than one batch would sit in `LaneCall::Probe`/`Finish` and touch no mock rule. The
   choice to leave the guard for a hardening task is still fine; the collision argument is not.
   **Severity: minor.**

6. **The cost-gate risk is answerable now.** `DiffRow::is_regression` is `new > old`
   (`cost-report/src/main.rs:1195`). Every figure the fix moves falls: fewer per-batch roundings in
   `type_structural_size` (`core/common.rs:23-59`), fewer partial-aggregate rows, `array_content_size`
   telescopes (`common.rs:85-89`) so varlen totals do not move. Nothing is flagged; strike the risk.
   **Severity: minor.**

7. **`abandoned` leaves every committed golden.** After the fix `nested-limits` consumes its one
   115-row cross-join batch (`tp1-single-mini.cpu.txt:872-876`, and the same at every mode), so the
   parser arm `tests/common/golden_text.rs:277` and the `+ abandoned` term of the conservation law
   (`tests/test_corpus_goldens.rs:268-270`) are exercised by no corpus file. `driver/tests/render.rs:132`
   covers the renderer, not the test-side parser, and `test_golden_format.rs` has no `abandoned`
   case. One string case there keeps the arm covered. **Severity: minor.**

8. **Complexity is M, not S** — see section 5.

Nothing blocking. No frozen surface is touched, no enabled cell breaks (q6 has no join; q19's join
emits one chunk of 121 rows, `tp1-single-mini.cpu.txt:645`, so `concat_batches` over one chunk is a
zero-copy slice — `arrow-select-54.2.1/src/concat.rs:219-221` — and its golden does not move), no
NULL, decimal or nullability semantics change: `declared_as` still runs per chunk, `concat_batches`
re-validates through `RecordBatch::try_new` against the declared schema exactly as `CpuExec::exec`
already does (`core/executor/cpu_backend/mod.rs:186-194`), and an empty probe or finish yields one
typed empty batch (`concat.rs:281-283`), which is what a device's kernel returns.

## 3. Claims verified

- `CpuProbingJoin::probe_and_fetch` (`core/executor/cpu_backend/join.rs:236-250`) and
  `finish_and_fetch` (`:254-265`) return `declared(...)` — one `CpuBatch` per DataFusion chunk
  (`:269-273`); `run_node` → `execute_single_node` collects every batch the stream yields
  (`cpu_backend/single_node.rs:51-56`).
- The device's `probe_and_fetch` ends with `out.extend(prior)` and `finish_and_fetch` with
  `prior.into_iter().collect()` — at most one batch per call (`gpu_backend/join.rs:164-198`).
- `CpuExec::exec` concatenates and its doc states the one-out contract (`cpu_backend/mod.rs:173-210`);
  every CPU accumulator emits through `one_batch` (`cpu_backend/accumulate.rs:138, 164, 201, 274,
  376-382`); the source concatenates (`cpu_backend/source.rs:95`). The join is the only multi-output
  executor on the CPU.
- The driver queues each element of the returned `Vec` and records `batch_rows` per element
  (`driver/partitioned.rs:307-313`, `:797-807`), and `in_rows` as Σ `Held::rows()` over popped
  batches (`:596-614`, `:809-811`; `Held::rows` at `driver/accounting.rs:49-51`). A `GpuBatch`'s
  `num_rows` is the ABI's `NodeStats.rows` from `tv.num_rows()` (`cpp/src/node_session.cpp:463-464`).
  `test_corpus_goldens.rs:250-284` asserts `consumed + abandoned == emitted`.
- DataFusion 45 reads `batch_size` from the session config (`hash_join.rs:820`) and passes it to
  `lookup_join_hashmap` (`:1472-1479`); `build_session_state` leaves it at the default.
- `line_difference` prints the first differing line plus "(+N more lines)" (`golden_text.rs:159-187`).
- Golden figures: q96 `in_rows=[[34]]`, join chunks `[8192×33, 3143]` (`tpcds.sf1/tp1-single-mini.cpu.txt:5089-5124`);
  q93 `in_rows=[[7486]]`, merge `output_rows=7169` (`:4927-4940`); q3 `[8192,8192,8192,5943]`,
  `in_rows=[[21242]]` (`tpch.sf1/tp1-single-mini.cpu.txt:83-98`); q14 at tp4-sized `[[3,3,3,3]]` with
  `batch_rows=[[1,1,1],…]` (`tp4-sized-mini.cpu.txt:831-843`); cross join one batch per build row
  (`:785-790`, `:872-876`); nested-loop Left `[[50,1]]` (`:898-904`); q4 LeftSemi `[0×18, 52523]`
  (`:131-132`). At tp1-single every multi-batch lane in both files is a join, a merge/union, or a
  1:1 node (project, filter, aggregate, sort, unload) above one — counted by node kind over both
  goldens.
- `every_row_a_node_emitted_was_consumed_or_abandoned`, `the_root_emitted_the_rows…` and
  `a_limit_slices_at_most_two_batches_and_stops_the_scan` (`test_cpu_end_to_end.rs:427-483`:
  `UnloadRange <= 2`, `NextBatch == 2`, `satisfied` non-empty, `peak_queued[limit] <= 1`) all hold
  with one cross-join batch; `settle_limit` marks satisfaction on the consuming call regardless of
  whether anything remains to abandon (`partitioned.rs:565-574`).
- `corpus_cases.inc` lines 27-33, 142-149, 210-216, 222-228, 260-264, 272-275 carry the #185
  comments; registry rows 39, 49, 88, 89, 94, 97, 103, 114, 128 carry `185`; all nine have frozen
  `mode=` sections in `mini.result.txt`, so `golden_exact` is admissible; a row with disabled cells
  keeps `152`, satisfying `registry.rs:274-283`.
- `peak_queued` is in the report and not in the golden (`plan_text/run_text.rs`); only `scan-limit`
  and `nested-limits` exit early in either corpus, neither through a join's probe scan.
- The C++ empty projection emits a placeholder column of the input's row count
  (`cpp/src/operators/project.cpp:20-31`) and the Rust side prices from the declared zero-column
  schema, so q96/q88/q14's `GpuProject exprs=[]` reports the same rows and bytes on both engines.
- The regression test design is sound: `with_batch_size(1)` is what DataFusion's own tests use
  (`hash_join.rs:1670-1673`), `CrossJoinExec` emits one batch per build row whatever the size, and a
  LeftSemi/LeftAnti finish yields zero-row chunks plus the answer; fixtures `dim()`/`FACT`/`fact()`
  and `ctx()` exist (`cpu_backend/tests/join.rs:15-55`, `tests/mod.rs:84-86`); `rows_of`/`drive`
  flatten, and `finished.is_empty()` at `:233` and `:562` are for `finish: None`.
- hacks-audit: nothing there is #185's; its second-section item 11 is the six-cell count
  (`hacks-audit.md:613-624`); `cpu_backend/join.rs` past line 140 is in "What I did not read".

## 4. Corrected proposal

Only the sections that change.

### 5. Minimum corpus query

    SELECT count(*) FROM supplier s JOIN nation n ON s.s_nationkey = n.n_nationkey;

tpch sf1, any mode. `nation` (25 rows) is the build side; `supplier` is one row group, so the probe
is one batch at every mode; a hash join's statistics are Inexact so `AggregateStatistics` leaves the
aggregate alone. CPU today: `GpuHashJoin batch_rows=[[8192,1808]]`, `GpuAggregate
batch_rows=[[1,1]]`, `GpuAggregateBatches in_rows=[[2]]`. Device: `[[10000]]`, `[[1]]`, `[[1]]`.
After the fix both engines read `[[10000]]`, `[[1]]`, `[[1]]`.

Not `SELECT count(*) FROM region, nation`: a keyless `count(*)` over a cross join of two unfiltered
parquet tables is answered from statistics and the planner refuses the `PlaceholderRowExec` naming
#158. The cross-join shape is already in the corpus as `tpch/cross-join` (`SELECT * FROM region,
nation`, five batches of 25 on the CPU against one on the device), off on #183 for its strings.

### 6. Cells re-enabled

Candidates, to be confirmed by one shad-gpu run of `test_gpu_corpus` with the cells flipped: gpu
tp1-single of tpcds q38 q48 q87 q88 q93 q96 and tpch q3 q14 join_int. For each, the device today
fails `assert_section` at the merge's `in_rows`, and `assert_result` has therefore never run — so
whether the device's answer matches `mini.result.txt` is unknown for all nine. The fix removes the
only divergence visible in the goldens' arithmetic; a cell that then fails on something else stays
off with a fresh ticket naming it, and the count in `build-test.md` is whatever the run leaves on.
Stay off regardless: their other four modes on #152, and q96/q88's tp4 modes on #180.

### 7. Risks and unknowns

Keep the proposal's list except the cost gate (answered: the gate is `new > old` and nothing rises)
and add:

- The device's results for the nine cells are uncompared to date (finding 2). A wrong answer there
  is a new ticket, not a reason to widen this fix.
- The doc comment on the join must not name 8192 (finding 3).
- `abandoned` disappears from every committed golden; add a `test_golden_format` case for the
  parser arm so the conservation law's second term stays covered (finding 7).

### 8. Complexity

**M.** Code is S: one file, one helper, two call sites, one regression test. What makes the task M
is around it — a full cpu-corpus regeneration at five modes on a dataset host, whose diff touches
almost every section with a join and has to be read for two invariants (`output_rows` unchanged at
every node but partial aggregates and merges' `in_rows`; `mini.result.txt` byte-identical), then a
shad-gpu run whose outcome for nine never-compared cells is a prediction, with a triage loop if any
cell fails past the join.

## 5. Complexity

M, for the reasons in 4.8. The proposal's S counts the code and discounts the regeneration and the
unverified device run that the definition of done requires.
