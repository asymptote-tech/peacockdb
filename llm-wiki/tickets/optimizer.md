
# Tasks for the optimizer project

<a id="t73"></a>
### #73 — Cost-based optimizer (CBO) umbrella
Move physical planning from static heuristics to cost-based, validated against the DuckDB
cost oracle and goldens. In scope: adaptive filter placement, join enumeration (#20),
stats (#19, the load-bearing prereq), runtime filters (#16).

<a id="t101"></a>
### #101 — No CSE / CTE materialization: identical subexpressions recomputed N times
**Priority: post-MVP**

DataFusion inlines every CTE reference and peacock re-scans each copy. Worst: tpcds q23
(CTEs ×5/×3/×2 → 6 scans), q4/q11 (`year_total` ×6/×4), q31; tpch q15 (`revenue0` ×2),
q2. Direction: CTE materialization or physical CSE; at minimum make the cost model aware.

The mechanism a materialized CTE needs — one producer, N consumers of the same batch
stream — is the one [#16](#t16) has to build first for dynamic filters, and it is cheap in
a streamed-batch model and expensive in a single-resident-table one. Sequence them
that way round.

<a id="t20"></a>
### #20 — Join enumeration: DPccp/DPhyp cost-based tree reshaping
DataFusion 45 has no join enumerator — trees come out in FROM-clause order, and ~70/99
TPC-DS queries have 4+ joins (q64 ≈ 18 tables). Implement DPccp (extend to DPhyp, IKKBZ
fallback beyond 14 tables) as a logical rule after PushDownFilter, cost = Σ intermediate
cardinality. Blocked by #19. Landing rewrites all plan goldens.

<a id="t71"></a>
### #71 — GPU scan: no predicate pushdown into the cuDF read
Partly addressed: stats-based row-group pruning exists
(`planner/translator/scan_mapping/rowgroup_prune.rs` → cuDF `set_row_groups`, parity with
ParquetExec). Remaining: serialize the predicate itself
into the cuDF `read_parquet` filter AST (page pruning / pre-filter during decode),
multi-file scans, dynamic ranges (#16). Cause of red widget ratios on selective queries.

<a id="t19"></a>
### #19 — the planner has no cardinality estimate, and the memory model pays for it
Widths are facts and source rows are facts — the schema, and the `rows`/`bytes` a scan reads
off its surviving row groups at plan time (`planner/translator/scan_mapping/parquet_meta.rs`).
What a query
does to them is guessed: `estimator.rs::rows` has a filter pass every row, an aggregate emit
one group per input row, and a join emit its larger side.

It shows twice, both in `estimate`. An accumulator is charged what it holds whatever the batch
size, so an aggregate's state is priced at its whole input — tpch q1 groups six million rows
into four and is billed six million — and that total comes off the budget before anything is
divided, so `share_per_source` and every batch size derived from it come out smaller than the
plan needs. `rows_are_certain` is what keeps the error one-directional: only the part of the
accumulator total that rests on facts can refuse a plan, so a guess shrinks batches and never
rejects a query.

Not part of this any more: build/probe order is DataFusion's own JoinSelection over its own
nodes, which nothing of ours now sits between — the 55-of-103 measurement this ticket opened
with was of a wrapper tree that is deleted. #147 is the mechanism a real estimate would arrive
through; #73 and #20 are what it unlocks.

<a id="t16"></a>
### #16 — Dynamic / runtime filters: build-side keys → probe-side scan
Star-schema fact scans read 100% of rows while the joined dimension is filtered to ~30%. Build
an IN-set / min-max (later Bloom) at build completion and feed the probe-side GpuScan.

Applies to 76/99 TPC-DS queries; validate on q3/q19/q33. Best after #19. **Design it as the
groundwork for CTEs, not a join-to-scan special case.** A dynamic filter is the first thing here
whose producer has two consumers: the build side feeds the join, and it also feeds a replanning
consumer that turns those keys into a predicate on a scan below. That is a diamond, and every
plan model here is a tree. What serves it — a fork handing one batch stream to N consumers, plus
a consumer that plans rather than executes — is what a materialized CTE needs ([#101](#t101))
and what [#147](#t147) calls refinement in flight. Do it after the streamed lanes, which
suit the shape: with refcounted handles ([#145](#t145)) a tee costs nothing on the device, the
accountant already models a fork's residency as the slowest consumer's backlog, and a consumer
blocking its producer is the join hold, one rule already mutation-tested. A diamond in the plan
is then routing rather than scheduling.

<a id="t75"></a>
### #75 — Refactor duckdb_cost.py: separate cost formula from extraction
**Priority: low** — not started (867 lines, 29 top-level defs as of 2026-08-05), and it buys
readability, not behaviour.

~870 lines where the ~80-line cost formula hides inside JSON parsing, predicate parsing
and row-group pruning. Shape: preprocessor → flat per-node intermediate
(op/rows_read/bytes_read/out_rows/out_bytes/breaker) → one-line cost. Pure refactor:
`.duckdb_cost.txt` numbers must not move.

<a id="t146"></a>
### #146 — aggregate shaping beyond the fixed sequence
**Priority: low** — each part optimizes an already-correct plan and needs the same estimate.

The aggregate sequence applies one shape everywhere — per-batch init, merge per lane,
shuffle, finalizing merge — right where group cardinality is far below row count.

Three shapes it cannot express or choose. **(a) A merge accepting raw rows.**
`GpuAggregateBatches` requires pre-aggregated state, so the per-batch `GpuAggregate` is
mandatory: a lane with few batches pays B groupbys where one over the concatenation would do,
and a non-reducing aggregate pays an init that shrinks nothing. A raw-accepting merge holds
rows where a state merge holds groups, so it fits only the non-reducing case. **(b) A loader
emitting one batch per partition**, where its consumer materializes the whole partition anyway
— decide it from the subtree beneath the loader. **(c) The cardinality estimate all three
want** — does this aggregate reduce? — which the constant estimators (#19) cannot answer and
which gates [#141](#t141). Land after #19.

<a id="t158"></a>
### #158 — an aggregate DataFusion answers from statistics reaches no executor
`SELECT count(*) FROM nation` never reaches an `AggregateExec`: DataFusion's
`AggregateStatistics` rule answers it from parquet metadata and emits `PlaceholderRowExec`
holding the result.

The mode refuses it at plan time, so nothing here runs it. No corpus query reaches it either,
since all 31 using `count(*)` carry a WHERE, GROUP BY or JOIN and the rule cannot fire.

The fix is small on the CPU and unavailable on a device: the node is a source of constant rows,
and a table of literals made from no input is what the frozen surface has no call for — the same
wall as [#173](#t173) and [#175](#t175). T17 was to have discharged it and did not: writing the
CPU half alone makes the oracle answer a query the device refuses, and the oracle is what the
device is checked against. Waits on the make-a-table-of-literals call all three want.

<a id="t142"></a>
### #142 — split large batches
Nothing downstream of the loader can split a batch: minimum load granularity is one row
group, `GpuCoalesceAllBatches` before a join build side can exceed any budget, and the
planner deliberately still produces a plan — `executor/driver/accounting.rs` then trips at run time and
the query dies cleanly. Recourse options, deferred until better estimators and adaptive execution: a split
operator (needs a C++ slice-to-handles entry point), or adaptive replanning on trip (re-plan
with more partitions or smaller batches) — the second being the only one that would make a trip
anything other than the end of the query.

Both checks abort today, pre-call and post-call alike, and the pre-call one refuses on an
estimate rather than on a fact. Recording it and letting the call proceed is the cheaper
recourse and is deliberately not taken: `RunError::BudgetExceeded` now carries which check
tripped, so something can branch on it, but there is nowhere to record into — `RunReport` has no
trip log, and `Underestimate` is the precedent for what one would look like. Related: #91.

