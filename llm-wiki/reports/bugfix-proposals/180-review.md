# #180 — review of the proposal

Read at master c18e063a. Paths relative to `/media/data/peacockdb`; DataFusion cites are the
vendored 45.0.0 sources under `~/.cargo/registry/src/*/`. Nothing built or run.

## 1. Verdict

**Needs changes** — the root cause, 3a and 3b are right and touch nothing frozen, but section 5's
"minimum" query is not the minimum: a keyless aggregate over a one-row-group table reaches the
same call at tp4-single with no join at all, and section 1 names only the scatter as the way a lane
under a keyless aggregate comes to be empty.

## 2. Findings

### F1 — the minimum corpus query is not minimal, and section 1 names one of two producers of the empty lane (important)

**What is wrong.** The proposal's Query A joins `store` to `store_sales` (2.88M rows) to make three
lanes of the per-lane merge empty by scatter. At `tp4-single` the same three lanes are empty without
any join: the mode is `OneBatchPerLane` (`peacockdb-core/tests/common/mode.rs:60-64`), which is
`Batching::Off`, and `lanes_for` (`src/planner/translator/nodes.rs:377-390`) then returns
`target_partitions` whatever the table's size — the small-table rule "bites only while batching is
on". A one-row-group table is cut `[[[0]],[],[],[]]` (`scan_mapping/partition.rs:44-47`: "chunks
are empty only where there were fewer survivors than lanes"), and a lane with no row groups
produces no batch at all (q93's tp4 golden: `reason` load `batch_rows=[[35],[],[],[]]`, the filter
above it `in_rows=[[35,0,0,0]]`, `testdata/goldens/tpcds.sf1/tp4-single-mini.cpu.txt:7462-7466`;
q96's own `store` load at `tp4-single.plans.txt:19895` is the same shape). So

    SELECT count(*) FROM store WHERE s_store_name = 'ese'

plans at tp4-single as `GpuLoadParquet store lanes=4 [[[0]],[],[],[]]` → `GpuFilter lanes=4` →
`GpuAggregate{count(1)} lanes=4` → the per-lane merge-only `GpuAggregateBatches{sum(count(*))}`
→ `GpuMergePartitions` → the finalizing node (translator `aggregate.rs:360-377`: `Collapse` from
DataFusion's `CoalescePartitionsExec`, `regrouped` true at four lanes over a multiple-batch init;
the round-robin repartition DataFusion inserts leaves no node, `nodes.rs:193`). Lanes 1–3 of the
merge-only node get `MarkDone` with nothing (`driver/single_partition.rs:178-179`) and fail with
the ticket's message. Twelve rows read instead of 2.88M, and no join, no scatter, no `NoBuild`.
The statistics shortcut (#158) does not fire because of the filter.

It does not reproduce at tp4-rowgroup or tp4-sized — there the small-table rule gives `store` one
lane, so the scatter is the only producer of an empty lane, and Query A is needed for those two
modes. Section 1's "A lane under a keyless aggregate is empty at tp4 whenever an Inner join's build
side scattered into fewer lanes" is therefore one of two routes; the proposal knows the second
(it names "an empty `partition_groups[lane]`" under "What it deliberately does NOT touch" as why
the path survives #173) but does not connect it to today's corpus or to the test the developer
writes first.

**Correction.** Section 5 gets three queries: the filter-only one above (tp4-single, exposes 3a
with no join), Query A (all three tp4 modes), Query B (all-empty, exposes 3b). Section 1 states
both producers: a source lane with no row groups at tp4-single, and a scatter that landed nothing
in the lane at every tp4 mode. The e2e cases in section 3 gain the filter-only query; it is the
one whose red run takes seconds.

### F2 — Query B plans only because its literal happens to sort inside the column's min/max (minor)

`s_store_name = 'no such store'` becomes the scan's pruning predicate, and the planner prunes row
groups at plan time (`scan_mapping/rowgroup_prune.rs:94-134`). A literal outside the stored
min/max of `s_store_name` prunes the file's only row group, and `partition()` then refuses
("no surviving row groups", `scan_mapping/partition.rs:25-31`) — a `PlanError`, not the run-time
shape the query exists for. `'no such store'` survives because `n` sorts between the store names'
first letters; the proposal's "both plan today, nothing refuses them" is true and unexplained.
State the constraint in section 5 so a developer who changes the literal does not read the
refusal as a second bug. A filter on a non-statistics predicate (e.g. `s_store_sk < 0` also prunes;
`s_store_name LIKE 'zz%'` does not) is the safer spelling.

### F3 — one more comment falsified by #180 is not in the sweep (minor)

`src/executor/cpu_backend/tests/accumulate.rs:358-361`, on `a_merge_that_received_nothing_emits_nothing`:
"The shape that would owe a row — a global aggregate's identity row, count 0 rather than no row —
has one lane and no scatter above it, so its lane is never the empty one." #180 is exactly that
lane being empty, and after 3a the sentence is wrong in the other direction (the shape that owes a
row is the finalizing node, and the merge-only node above a scatter owes nothing). Add it to the
section-3 comment list.

### F4 — the proposed `mark_done_and_fetch` doc is eleven lines (minor)

`coding-style.md:17-21` caps a declaration comment at ten. Drop the second sentence of the first
paragraph (how the lane comes to be empty is the tests' and the wiki's to say) or fold the second
paragraph into `plan::identity_state`'s own doc, which already carries the count/sum point.

### F5 — `identity()` panics where it could return (minor)

The proposed `identity()` returns `Result<_, PlanError>` and maps `new_zero`'s error, but uses
`null_of` (`plan/aggregates.rs:155-158`), which `expect`s. Both arms should be the same shape:
`ScalarValue::try_from(data_type).map_err(...)`. `null_of` stays as it is for `finalize`, whose
call sites are exhaustive on the output types.

### F6 — `build-test.md:42` carries a count the fix moves (minor)

The row's last column is `447`; nine cells come back. Whether the figure is cells or test
functions, it is the tier's declared size and the proposal edits the row's clauses without it.

### F7 — the single-node shortcut risk can be stated more tightly (minor)

Section 3's "does NOT touch" list says the `GpuAggregate{final}` shortcut over a `SingleBatch`
lane that received nothing emits no row on both engines, "not reachable from the corpus". True,
and narrower than it reads: it is not reachable from a filter at all. An empty *batch* traverses
(`cpu_backend/mod.rs:173-176`, `driver/tests/flow.rs:80`), so a grouped merge fed zero-row batches
emits a zero-row batch (`accumulate.rs:371-382`: pending non-empty → `compact` → `state = Some`
→ `one_batch` over a held batch), the shortcut Exec runs on it, and DataFusion's no-grouping
partial answers `count = 0` (`no_grouping.rs:136-148`). Only a lane with no batch — a mid-plan
limit that dropped everything at one lane — reaches it, and no corpus query has an aggregate over
a mid-plan limit (`testdata/tpch-queries/nested-limits.sql` is a cross join by design, per its own
comment). Say that, so nobody files it as reachable.

### F8 — test 1 needs a schema the helpers cannot build (minor, first-hour)

`schema_of` (`cpu_backend/tests/mod.rs:61-68`) makes every field nullable and `state_of`
(`:137-152`) annotates Welford only. The proposal's test 1 wants `count(v): Int64, nullable=false`
with a `Count` annotation — hand-built, or a helper taking `(name, type, nullable)` plus an
`AggStateColumns`. Not a flaw in the plan; a line the developer should expect.

## 3. Claims verified

Opened and found true (file:line as the proposal cites, unless noted):

- The failing call: `mark_done_and_fetch` at `cpu_backend/accumulate.rs:371-372` runs
  `compact()` when `state.is_none() && !grouped`; `compact` (`:351-364`) runs the merge over no
  input and relabels through `declared` → `declared_as` (`cpu_backend/mod.rs:239-275`), whose
  `RecordBatch::try_new` at `:269` is what produces arrow's "declared as non-nullable but contains
  null values"; `call_failed` (`driver/partitioned.rs:825-830`) prefixes the node and lane.
- DataFusion emits one row from fresh accumulators over an empty no-grouping stream
  (`no_grouping.rs:136-148`, `aggregates/mod.rs:1198-1216`); `SumAccumulator::state` is its
  `evaluate`, `None` over nothing (`sum.rs:293-295`); count's state is `Int64, nullable=false`
  (`count.rs:163-167`) and it is the only one — sum `:203-207`, avg `average.rs:155-167`, stddev
  `:112-125`, variance `:112-118` all `true`, min/max take the `AggregateUDFImpl` default
  (`datafusion-expr-45.0.0/src/udaf.rs:428-433`, `true`). The translator copies
  `field.is_nullable()` at `translator/aggregate.rs:150-157`.
- `count` merges by `sum`: `plan/aggregates.rs:58-63`; `merge_aggregates` refuses `Count` as a
  merge aggregator (`cpu_backend/mod.rs:426-437`).
- The trace: CPU scatter emits N batches with typed empties (`cpu_backend/emit.rs:56-69`); the
  driver drops zero-row scatter outputs (`partitioned.rs:381-385`); `one_batch` answers nothing
  for an empty coalesce (`accumulate.rs:138-141`); `NoBuild` (`single_partition.rs:181-183,
  317-329`), `empty_build_answers_nothing` true for Inner (`plan/join.rs:452-461`),
  `without_build` returns `Ok` (`cpu_backend/join.rs:184-192`); `MarkDone` is called on a lane
  that received nothing (`single_partition.rs:178-179`, `avail.done[0]`).
- Keyless aggregates take `Shuffle::Collapse` → `merged` (a `GpuMergePartitions`), no emit
  (`translator/aggregate.rs:370-377`); the per-lane merge-only node is emitted at
  `:360-368` when `lanes > 1` and the init is multiple-batch.
- The device's twin emits nothing for an empty lane, merge-only or finalizing
  (`gpu_backend/accumulate.rs:307-320`).
- Goldens: every keyless merge-only `GpuAggregateBatches` in the six tp4 `.cpu.txt` files
  (tpch.sf1 and tpcds.sf1, three modes each) has four populated lanes — 36 nodes, all
  `batch_rows=[[1],[1],[1],[1]]` — and every keyless finalizing node in all ten `.cpu.txt` files
  has a non-empty `in_rows`; tpch.sf40 has no execution goldens. No enabled section moves under
  3a or 3b. q93's tp4 golden shows the empty-lane shape under a grouped merge
  (`tp4-single-mini.cpu.txt:7448-7454`).
- One-row build sides: q96's `store` filter `output_rows=1` (`tp1-single-mini.cpu.txt:5089+`),
  q90's `web_page` filter `output_rows=1` twice, q88's `store` filter eight times; q88 has eight
  keyless four-lane merges at tp4.
- At tp1 a zero-row filter answer is an empty batch (`cpu_backend/mod.rs:173-176`), so the init
  runs and the count is 0; Query B answers 0 at tp1 today.
- Cells and registry: `corpus_cases.inc:134-137` (q96), `:197-206` (q90, the q2 sentence at
  `:201-203` is separate), `:258-267` (q88); `cost-registry.csv` lines 89/91/97 carry
  `152 180 185`, `152 180 187`, `152 180 185` with the three `cpu_tp4_*` columns disabled;
  `build-test.md:42` names q96 and q88 and not q90; `.result.txt` sections for the three read
  `mode=tp1-rowgroup` and q93's reads `mode=tp4-sized`.
- `ScalarValue::new_zero` exists (`datafusion-common-45.0.0/src/scalar/mod.rs:1139`) and covers
  Int64/UInt64/Float64; `to_array_of_size` at `:2222`; `null_of` at `plan/aggregates.rs:155`;
  `welford_owners` at `plan/aggregate.rs:70`; `state_funcs` delegation at `plan/mod.rs:1236`;
  `Decomposition`'s "that exception is the whole content" at `:387-390`; the `grouped` field and
  its doc at `accumulate.rs:314-316`.
- Welford identity `(0, 0.0, 0.0)` (`variance.rs:264-271`); avg `(count 0, sum NULL)`
  (`average.rs:289-293`); the Welford finalize's CASE returns NULL at count 0 for both ddof
  (`plan/aggregates.rs:111-138`).
- `identity_state`'s walk is sound: the translator pushes state fields in `rule.state` order and
  annotates positions `state_at..state_at+len` (`translator/aggregate.rs:150-157, 198-204`), and
  the finalizing node's `intermediate` carries `decomposed.annotations` (`:283`), so
  `decomposition(func).state.zip(positions)` pairs each column with its own aggregator.
- No test outside the one being rewritten builds a keyless finalizing `GpuAggregateBatches` and
  drives it through `CpuAccumulator::aggregate`: `cpu_backend/tests/accumulate.rs:246,434`,
  `tests/backend.rs:84`, `tests/test_cpu_executors.rs:266,345`, `wire/tests.rs:409,439` are all
  grouped; `plan/tests/mod.rs:219,516` never build an executor; `tests/common/rebuild.rs:82`
  copies `intermediate()` with its annotations. The injected set's only keyless per-lane merge
  (tpcds/q97) sits above a hash shuffle, so `Drain` repopulates its lanes, and its Full join
  switches the degenerate-hash dimension off (`tests/common/injection.rs:583-590`).
- The existing pinning test (`cpu_backend/tests/accumulate.rs:700-748`) declares a nullable
  `count(v)`, builds a merge-only body, and asserts a row count — it passes on the NULL row.
- The accountant compares only `scratch_bytes` after a consuming call
  (`driver/accounting.rs:184-200`); an identity row from nothing held trips nothing.
- Wiki: `architecture.md` "The aggregate sequence" (`:213-260`) says nothing about what a node
  owes over no input; the sentences at `:700` ("an empty lane emits no batch at all") and
  `:855-859` (the three refusals) stay true. `tickets.md` #173 at `:266-267` and #199 at `:44-52`
  read as quoted. hacks-audit's finding 3 (`:442-460`) names the `!self.grouped` clause as the
  deliberate divergence; the proposal keeps it, narrowed.

## 4. Corrected proposal — only the sections that change

### 1. Issue — "Where the corpus reaches it", replace the first sentence

A lane under a keyless aggregate is empty at tp4 by either of two routes, and both put no batch —
not an empty one — into the per-lane merge. At tp4-single a table with fewer row groups than lanes
is cut `[[[0]],[],[],[]]` (`lanes_for`, `translator/nodes.rs:377-390`, ignores the small-table
rule while batching is off), and a lane with no row groups produces nothing, so the init above it
never runs. At every tp4 mode an Inner join's build side scattered into fewer lanes than there are
leaves the join lane `NoBuild`, draining its probe and emitting nothing (the chain the proposal
already traces). q96/q90/q88 take the second route at all three modes; at tp4-single their
dimension tables also show the first at the load (`tp4-single.plans.txt:19895`).

### 3. Localized fix — additions to the comment sweep

- `cpu_backend/tests/accumulate.rs:358-361`: the doc on `a_merge_that_received_nothing_emits_nothing`
  → "The shape that owes a row is the node that finalizes a global aggregate; a merge-only node
  above a scatter, or above a source lane with no row groups, owes nothing (#180)."
- The `mark_done_and_fetch` doc trimmed to ten lines (F4).
- `identity()` returns the `try_from` error instead of `expect`ing (F5).
- `build-test.md:42`: the tier's count beside the clauses (F6).
- Test 1 builds its schema by hand or through a `(name, type, nullable)` helper (F8).

### 5. Minimum corpus query — three, not two

Query 0 — tp4-single only, no join, exposes 3a (`Coverage::ModesOnly` runs the other four modes
green today and after):

    SELECT count(*) FROM store WHERE s_store_name = 'ese'

`store` is one row group; at tp4-single it is cut into four lanes with three empty, the filter and
init never run on those three, and the per-lane `GpuAggregateBatches{sum(count(*))}` fails on each
with the ticket's message. At tp4-rowgroup and tp4-sized the small-table rule plans `store` at one
lane and the query is green — which is why Query A is still needed.

Query A — as proposed (every tp4 mode, via the scatter).

Query B — as proposed, with the constraint stated: the literal must survive the scan's row-group
pruning (`rowgroup_prune.rs:94-134`) or `partition()` refuses at plan time
(`partition.rs:25-31`). `'no such store'` survives on min/max alone; a value outside the column's
range is a `PlanError`, not this ticket.

### 7. Risks — the shortcut note, tightened

The `GpuAggregate{final}` shortcut over a `SingleBatch` lane that received no batch is reachable
only from a mid-plan limit that dropped every batch of its one lane; a filter that keeps nothing
still emits a batch, the grouped merge above it emits a zero-row batch, and the shortcut counts 0
correctly. No corpus query has an aggregate over a mid-plan limit. Same family as #199, not
reachable today, not filed.

## 5. Complexity

**S**, agreeing. The code is four files and under 80 lines with real logic; nothing on the wire,
the C++, or a frozen surface moves; no existing golden section moves (read, all 36 merge-only and
every finalizing keyless node). What makes it more than trivial is verification surface rather
than code: nine corpus cells authored across three `.cpu.txt` and three `.cost.txt` files (q88's
eight subqueries over `store_sales` at three modes is the long run), the `.result.txt` re-stamp,
the cost-report gate's first look at three newly enabled cells, and a comment sweep across two
test files and three wiki pages. If the developer's host is not the dataset-matrix one, the golden
authoring is the step that stalls, not the fix.
