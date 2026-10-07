# exec_model — design

A Python model of the engine's execution model (`llm-wiki/architecture.md`) and, on top of it,
a join-order optimizer for the engine's own plans. The runtime settles scheduling and accounting
rules where they are cheapest to argue with; the optimizer reads the planner's plan goldens,
rewrites them and runs them on pandas over the sf1 tables. It is judged on CPU, by the costs of
its own runs. How to run it is [`README.md`](README.md); the DPhyp crate it calls is
[`peacockdb-dphyp/design.md`](../../peacockdb-dphyp/design.md).

Where the Rust engine and this model disagree and the model is the one that is wrong, the Rust
is right: the synthetic scan's chunking (`source.partition_row_groups`) overshoots a lane's
share by a whole group, where `scan_mapping/partition.rs::balanced_chunks` stops where stopping
is closer. That class is not a defect against the engine, and the engine is not changed to match
a model it has outgrown.

## Folders, and the imports that run against them

    optimizer/ ──► plans/ ──► operators/ ──► engine/     a folder imports those to its right

    against it:  engine/adaptive     ──► plans/, optimizer/observed
                 plans/engine_nodes  ──► optimizer/cardinality, optimizer/stats

`engine/` is the runtime: the plan structure, the traits, both drivers, the scheduler, the
accountant. `operators/` implements its traits over pandas. `plans/` turns an engine plan's text
into a prototype tree and runs it; it is neither runtime nor optimizer. `optimizer/` reads
statistics, estimates and rewrites plans. `errors.py` sits at the root, since every folder raises
from it.

Two modules import upward, and both stay:

- `engine/adaptive.py` imports `plans` (`engine_ir`, `engine_expr`, `engine_plan.EngineNode`)
  and `optimizer.observed`. Its events name the engine join they are about, because the replan
  rewrites the engine plan, and it reads a build's keys off that join's `on=`. The key NDV it
  reports is `observed_ndv`, the per-lane count the engine's executor would make at
  `build_done`. Moving the driver into `optimizer/` would put a driver outside the runtime;
  keeping the events in prototype terms would need a second key description on every join node.
  `observed.py` imports pandas only, so no cycle forms.
- `plans/engine_nodes.py` imports `optimizer.cardinality.estimate` and `optimizer.stats` for
  `estimated_fanouts`, a hash join's output rows per probe row, which sizes its scratch: an
  estimate attached at build time reaches the executor through its node, and the trait stays as
  it is. Its one caller today is `tests/optimizer/test_cost.py`; `run.py` builds every join at
  the constant fanout 1, as the engine's estimator has it. The helper could move into
  `optimizer/` with nothing else changing, which is the natural time to do it once a
  production caller appears.

Every other cross-folder import points the way the diagram does.

## From plan text to the optimized run

    <bench>.sf1/<mode>.plans.txt
      │ plans/engine_plan       the tree, field values verbatim
      ▼
    EngineNode tree ──► optimizer/cardinality   every node's estimate
      │
      │ optimizer/dynamic_filters   probe plans run first; scans re-mapped;
      │                             each probed build kept as a GpuMemorySource
      ▼
    multijoin.clusters ──► dphyp (C_out) ──► join_order.orient ──► disassembly
      │                                                               │
      ▼                                                               ▼
    replan.run_adaptive ◄────────────────────────────────── the rewritten tree
      │ engine/adaptive: BuildDone per join, before its build is set
      ├── within 2× of the estimate ──► run on to the answer
      └── further off, nothing else started ──► stop; every build made so far
                                               becomes a GpuMemorySource; back to
                                               clusters with those sizes known

`optimizer/pipeline.run_optimized` composes the steps in that order, the order
`run_adaptive`'s `kept` parameter is shaped for, and returns what each did as an
`OptimizerReport` (`optimizer/report.py`, rendered by `report_text.py`). Each step also has a
corpus suite of its own. The mechanics are in each module's docstring; what follows is why each
step has its shape.

- **The planner's own plans** (`plans/`), not a lowering of the SQL written for the prototype: a
  second lowering is a second reading of the planner, and the optimizer would then be tuned to a
  plan the engine never makes. Reading the golden back and printing a rewritten tree in the same
  text (`engine_plan.plan_text`, `engine_expr`) also makes the plan diff in `.optimizer.txt` the
  engine's own format.
- **Estimates** (`cardinality.py`, `estimator.py`) follow DuckDB's model — uniform values over
  [min, max], independence, containment over the domain two keys share, 0.2 where statistics are
  silent — because DuckDB is the reference the costs are compared with, and because the
  statistics here (footers and the sidecar) hold nothing richer: no histograms, no samples. Two
  passes, so two keys of one equivalence class are counted once.
- **Dynamic filters** (`dynamic_filters.py`) run the build side first as a plan of its own and
  prune the fact scan's row groups by the keys it made, rather than pushing the keys into the
  probe scan at run time as DuckDB does (#16): a scan's lanes and batches are fixed when the plan
  is made, from its row groups, so pruning means mapping the survivors again before the run, and
  every plan here is a tree, which a build feeding both its join and a scan's mapping is not. The
  join hold already finishes the build before any probe batch moves, so running it first costs
  nothing, and its output is kept as a `GpuMemorySource` the main plan reads, so
  the side is read once. Only a scan ordered on the key qualifies (Σ(max − min) / (MAX − MIN) under
  1.5): on an unordered column every row group's range spans the keys and nothing prunes.
  Survivors are re-mapped by the planner's own policy, so the pruned plan is one the planner
  would have made.
- **Clusters** (`multijoin.py`) are maximal trees of inner hash joins only: inner joins alone
  commute and associate freely, and only hash joins carry the key edges DPhyp takes. A filter
  roots a relation even between inner joins, because an `IS NULL` there is what turns a left join
  into an anti join. Columns are `(relation, ordinal)` identities rather than positions, since a
  reordered tree renumbers every position.
- **Disassembly** (`disassembly.py`) derives each new join's wiring as the translator would
  rather than carrying the old plan's exchanges, which were placed for keys the new join may not
  use. A join over the same relations as one the plan had keeps that join's lanes — DataFusion's
  choice to collect a small build. `test_disassembly_corpus` holds every cluster of every golden,
  rebuilt in its own order, to the layout the engine printed, and `test_engine_reassembled` to the
  answers.

### DPhyp on C_out

Each cluster goes to DPhyp (`optimizer/dphyp.py`, the `peacockdb-dphyp` crate over its C ABI)
with its key edges only: a hash join needs a key, and two sets only a residual connects would be
a nested loop. The residual goes on the first join that has all its relations below it. DPhyp
asks the cost of each connected set it reaches, once; `join_order.SetEstimates` answers with
`cardinality`'s formulas over one canonical order, so a set has one estimate however DPhyp
reached it.

- **Why DPhyp.** It is exact and enumerates each connected csg–cmp pair once, so its work is the
  number of connected pairs, never cross products. On the prototype's key edges, which are all
  between two relations, it enumerates exactly what DPccp would; it also takes hyperedges, which
  is what predicates over three or more relations need. A greedy order is not optimal, and the
  clusters here are small enough not to need one: of the 500 runs in the committed corpus that
  called DPhyp, 5 hit the budget — tpcds q64's 18-relation clusters, at every mode.
- **The budget.** `MAX_PAIRS` is 10,000 connected pairs, DuckDB's. Past it the cluster keeps the
  plan's order and is only oriented again; the greedy fallback DuckDB has is not built.
- **Why C_out in bytes.** A set's cost is its rows times the width of what the joins above and
  the cluster's output need of it (`optimizer/cost.py`). Every term is a join's output, so
  per-kind weights would cancel between two orders: the cost needs estimates and no calibration.
  Bytes rather than rows, because a reorder changes what each join carries, and bytes are what
  the engine's cost counts.
- **What is reported.** Every set DPhyp priced is kept in the report (`JoinOrder.priced`) and not
  printed: printed, they would be 97 % of q64's section, most from calls the budget stopped, and
  they move by thousands of lines on any estimator change.

### Orientation after DPhyp

The crate decides no sides; `join_order.orient` does, bottom up, after DPhyp returns. For an
inner join both orientations emit the same rows, so C_out is a function of the set and DPhyp can
price sets alone. Which side builds is not: it depends on how many batches the probe arrives in
— a function of the subtree's shape and lanes — because while #152 is open the build is copied
once per probe batch, and a probe not hashed onto the join's lanes is shuffled first, cut once
per lane. Folding that into DPhyp would give one set several costs. So each join builds its
cheaper side: build bytes × (1 + copies × probe batches).

### The adaptive run and replan

`engine/adaptive.AdaptiveDriver` reports `BuildDone` when a join's build side has emitted on
every lane and before the join sets it: true rows, bytes and the key NDV. `optimizer/replan.py`
stops the run where the build is more than `DEFAULT_THRESHOLD` (2.0) times off its estimate,
turns every build made so far into a `GpuMemorySource`, optimizes again with their sizes known,
and runs a new driver.

- **Why only on builds.** A build is the one point where a true size is known while nothing
  above has consumed it: it is materialized one batch per lane, its key NDV countable exactly
  per lane, and its join has probed nothing. A stream's size is known only when it ends.
  The driver also has `BuildExceeds`, a build passing a byte cap; `run_adaptive` sets no caps.
- **Nothing runs twice.** A stopped run's `extraction` is the build just done and those set but
  not probed. Where any other node has made a call, the replan is refused and the run goes on,
  recorded as a refusal: cutting below a started accumulator would run its input again.
- **It ends.** Each replan turns at least one build into a source measured exactly, whose own
  `BuildDone` then matches its estimate, so a plan replans at most once per join.

## run.py and the cost

`run.py` runs one task per (query, mode) in a process pool: the plan as planned, then the
pipeline, the optimized answer held to the planned one (`plans/answers.same_answer`). The
planned answer is DuckDB's (`test_engine_answers`), and no rule touches what sorts the rows, so
the planned run is the oracle and no DuckDB is needed. The parent alone writes, after every task:
one writer, no lock. Sections merge as the engine's goldens do (`corpus_golden.rs`): a run without
`--query` owns its files, a filtered run changes only its sections.

Cost is the engine's own function, `plans/cost_model.py`: `test_support/cost_model.rs` read from
`testdata/cost_model.conf`, summed and rounded as the Rust is, and pinned by deriving every
committed `-mini.cost.txt` again from its `cpu.txt`. It is applied to two prototype runs:

- **Only two prototype runs compare.** The prototype's rows match the engine's where no rule
  changed the plan, but its bytes are pandas' `memory_usage(deep=True)`: decimals are float64,
  dates `datetime64[ns]`, integers holding NULLs float64, and another pandas version moves every
  cost line. Its planned cost over the engine's figure has median 0.80 (tpch) and 0.85 (tpcds).
- **Optimized is all the work.** The final plan's run, every dynamic filter's probe plan and
  every run a replan stopped. q64's optimized `storage_read_bytes` equals its planned run's at
  every mode, so nothing is read twice.
- **Memory sources are free.** A memory source hands back a build a probe or a stopped run made
  and was priced for; reading it again costs nothing by nature, not by a multiplier
  (`cost_model.FREE`), and the ram-to-vram transfer it would cost on the device is a 0.0
  placeholder in the conf. Any other kind the conf lacks raises, as the engine panics.
- **The final ratio is projected.** `cost_report.html` takes, per query, the min over modes of
  E × (O / P) / D: E the engine's `-mini.cost.txt` figure, O / P the prototype's optimized over
  planned at that mode, D DuckDB's `duckdb_cost=`. O / D would carry the prototype's byte scale
  into the comparison, at its extreme in tpch `scan-limit` (#186): the prototype's scan stops at
  the pushed-down limit, as `scan.cpp`'s `set_num_rows` intends (#186), while the engine's CPU
  source reads the table,
  so the planned cost is 3.4e-6 of the engine's. The prototype contributes only its own ratio.
  Where the engine has no figure at any mode run (tpch q11 and q22, tpcds q24 and q54) there is
  no final ratio.

The committed run: 600 (query, mode) runs, none failed. Optimized is cheaper in 55 tpch runs and
379 tpcds runs, dearer in 6 — tpch q18 at `tp1-rowgroup` by 1 % (a flip and a replan after a
build of 57 rows against 300,000 estimated), tpcds q5 at every mode by under 0.03 % — and equal
in the rest. The final ratio is within the published report's 1.4 for 17 of 20 tpch rows and 64
of 79 tpcds rows. Bytes do not see the prototype's own time: q64 replans 7 times at every mode
and is cheaper at each (0.83–0.93), yet slower in wall time, which is Python estimation and
DPhyp over 30 calls rather than anything in the plan.

## The runtime

`engine/` models the rules `architecture.md` states under "The scheduling rule", "Early exit at
a limit" and "Memory accounting"; `partitioned_driver.py` and `scheduler.py` are the Python of
`executor/driver/`. Min-height-first with ties leftmost makes it a push model; queues stay at one
batch per lane once a join in its build phase holds its whole probe subtree. The naive rescan
survives as a test oracle, `tests/rescan.py`, checked pick by pick in five suites. Traits are
declarations only: the driver tests drive mocks (`tests/engine/mocks.py`), since what they test
is which node runs when, and the operator tests drive the real thing, both through
`BackendSelector`. Every plan in `test_end_to_end.py` runs under a real resident budget, so the
accountant is engaged rather than dormant.

What the model established, each pinned by the test named:

- The push behaviour falls out of the height rule alone —
  `test_a_batch_is_carried_to_the_root_before_the_next_one_is_produced`.
- The queue bound needs one rule, the transitive join hold: without it a two-sided shuffle join
  put its whole probe input (32 batches) in queues before the first `set_build`; with it, 4.
  `test_the_hold_is_transitive_over_the_whole_probe_subtree` and its neighbours in
  `test_scheduling.py`; each goes red with the hold dropped, restricted to the direct child, or
  weakened from any lane to all lanes.
- With the hold the scheduling half of the bound cannot go red; what the bound still guards is
  emission discipline, at most one batch per call per output lane —
  `test_the_queue_bound_assertion_is_live`.
- Multi-child forwarders drain left first: a source with nothing is skipped, and under
  min-height the leftmost child is the one holding a batch. Only `GpuMergePartitions` rotates —
  `test_a_multi_child_forwarder_skips_a_pending_source_instead_of_waiting`.
- There is no `Pending`: runnability is decided before any call, so a node with nothing to do is
  not chosen. `Exhausted` is the per-lane `finished` flag.
- Executor constructors take their lane (`ExecutorBackends` holds `Callable[[lane], Executor]`):
  a loader finds its row groups by lane. Cross-lane categories are built once and get `None`.
- A join's build lane delivers exactly one batch, an empty one included, so
  `GpuCoalesceAllBatches` emits one even from nothing.
- Whole-tree facts are validated in `plan.py`; what a node needs of its children is its
  `_validator`, composed from `operators/validation.py`, where the message can name the fix.
- A join's scratch is sized by its output, which `scratch_bytes(n_rows, n_bytes)` cannot derive;
  the executor is built from the node, so a fanout attached at plan time reaches it and the trait
  is unchanged. Model ≥ measured is therefore not an invariant: the accountant's contract is to
  fail cleanly when the accounted peak passes the budget.
- The executor total is cached and refreshed per call, not summed: summing forces the accountant
  to hold every executor, which the Rust driver cannot.

### The step cap

`run()` raises `DriverError` once the steps or the calls pass `step_cap()`, the calls the run
owes so far: a source lane's batches (`max_batches`), one call per batch queued below the root,
and the closing call each readiness index ends with. Each is owed before or in the step that
makes its call, so a run that progresses ends with calls at most the cap, equal when every source
returns all it declares. Both counts are needed: a driver that picks nodes and calls nothing
makes steps and no calls; a source producing past its count makes calls nothing owed, which
steps alone miss over several lanes, since one step runs every lane. Prefetches are not calls:
counting them false-trips honest runs.

The cap grows with the batches queued because the plan's shape cannot bound them. Each
`GpuEmitPartitions` turns a batch into one per lane and nothing compacts them (#139), so stacked
shuffles multiply batches in the row-group modes. tpch q8 at `tp4-rowgroup` stacks six on its
probe path: (62 source batches + 192 lanes summed over its 63 nodes) × 63 nodes is 16,002, and it
takes 137,760 steps and 169,049 calls — the cap at its end. The same fragmentation is the full
corpus run's tail: tpch q9's planned run at `tp4-rowgroup` takes 818 s, its optimized run 14 s.
That time is the driver's Python per batch — per-row hash digests and `memory_usage(deep=True)`
— not pandas' own operations, which is why pandas stays.

### The limit

Both lowerings exist. Feeding only the sink, `skip`/`fetch` are `GpuUnload`'s and the driver
counts rows across lanes, per batch releasing the handle, narrowing the call to a row range, or
passing it whole; once satisfied the subtree is held for good. Anywhere else the limit is a node
over the one-partition input the planner guarantees, slicing only the two batches that straddle
the interval, and marked done as it is held. `test_limit.py` asserts on the unload calls, since
only they tell a limit from a filter applied after the transfer.

### Layout injection

An answer is a function of the rows, not of how they were divided.
`operators/injection.LayoutInjector` rebuilds a plan at a named preset — lanes, row groups, batch
sizes, hash placement from well spread to everything in one lane, zero-row batches injected at a
seeded probability. It rebuilds rather than edits, because a node's partitioning is baked into a
closure: every builder in `nodes.py` records its call (`Recipe`). A join is re-partitioned only
when both sides are hashed on its keys, and every placement is a pure function of the key
columns, so a plan that works only under a well-spread hash is broken, not unlucky.

## The operators

`operators/` keeps what the engine's plans need, and the name-addressed forms that two kinds of
test still drive.

- **Columns by position.** `engine_ir` resolves ordinals to frame names (`name@ordinal` where a
  schema repeats a name). A join renames its inputs to `Positional.joined` and picks its
  projection by ordinal from the join type's own output; it declares its hash where every row
  keeps its keys unpadded (`nodes._join_distribution`); a one-to-one node carries its child's hash
  to where `sources` puts it (`_copied_layout`, `_kept_hash`).
- **Each join type's own output** (`joins.own_output`): build alone for left semi and anti, build
  and the mark for left mark, probe alone for right semi and anti, both otherwise. `HashJoin`
  covers nine types: per-call emission for the probe-local half, a finish pass over `matched` for
  the build-preserving half, and a single-batch probe as the whole join. Null keys are held out
  unless `null_equals_null`. `probe_schema` pads an outer finish that saw no probe batch. Nested
  loop is Inner and Left; cross streams.
- **A residual filter does not by itself stop a probe streaming.** What forbids it is the finish
  pass, which sees accumulated keys, and a keys-only table cannot evaluate a predicate over both
  sides. A filtered Inner, or any probe-local type, streams: each output row is decided by the
  whole build and this batch, the filter included. That is the capability matrix's shape —
  `test_a_residual_filter_rides_the_join_on_both_backends`.
- **The planner's aggregate forms** (`aggregates.PlanAggregate`, `PlanCall`): init calls
  sum/count/min/max/mean/m2, merge calls sum/min/max/merge_m2 (Welford, Chan's merge); grouping
  sets with DataFusion's id, first key in the high bit; `final` expressions. A global aggregate
  over no rows is one row, an aggregate with no calls is a DISTINCT, the null group is kept, a sum
  over nulls is NULL. `PlanAggregateBatches` compacts on a doubling threshold and emits one typed
  batch even from no input.
- **Real row groups.** `parquet_scan` reads a file's own row groups as the planner mapped lanes
  to batches (`source.row_group_ranges`, `TableSource`, with fetch-ahead). A scan limit sits on
  one lane. `MemorySource` is a materialized build, one batch per lane.
- **Expression and hash details.** Comparisons are three-valued; LIKE becomes a regex over its
  two metacharacters; literals broadcast; cast, round, substring and date_part follow the C++. A
  shuffle key hashes as its SQL value — 5.0 in an integer column holding a NULL lands where 5
  does — and NULL key columns are skipped, so all-NULL keys share one lane, as comet's hash does.
  crc32 stands in for murmur3: co-location needs a deterministic hash, not comet's bits.

**pandas inside cuDF's vocabulary.** The operators are written against what pandas and cuDF
share, and `operators/frame.py` names the five rules: no index, no `apply` or callables,
explicit null placement on every sort, explicit null equality on every join, concatenation of
identical columns only. Each has a test in `test_operators.py` that fails if the pandas default
stands in.

**The recipe emulation.** `test_join_capability.py` runs every join type on two backends that
share no join code: `joins.py` with pandas, and `recipe_join.py`, which answers each call by
emitting FlatBuffers nodes in the legacy vocabulary and calling `execute_node(seq, handles)`
against `recipe.py` — the fbs structs field for field, `NodeSession`'s consume-on-use registry,
and `cpp/src/operators/join.cpp` branch for branch over `cudf_calls.py`. It answers whether the
frozen surface can run every join type with a streamed probe: yes, at the cost of a build copy
per probe batch (#152, counted, not estimated). It also found that an outer join with a residual
filter is wrong in the shipping C++ (#153): reproducing a code path faithfully is what shows its
defect. Two guards read `join.cpp` and fail when it names a join type or a cuDF call the model
lacks. The recipe backend does not join by position, so it serves only this test, through the
name-based join classes, which are also the positional ones' base. `LayoutInjector` keeps the
synthetic `nodes.scan`, `rebatch` and the degenerate hash placements; the driver tests keep
`tests/engine/mocks.py`.

### DuckDB, the oracle

The corpus suites hold every answer to DuckDB's. It runs the query's own text over the same
parquet, so it catches a reading of the SQL that a hand-written pandas equivalent would share.
A compare asserts what SQL determines: the rows as a multiset and the ORDER BY columns by
position (`plans/answers.matches_oracle`). Whole rows by position would fail a tie: in tpch q11
two parts come to 223626.0 exactly, and which prints first is not the query's to say. Tables are
read whole: both benchmarks are clustered by date, so a row prefix is one quarter of 1992 and two
tables sampled independently join to nothing.

## Statistics

`optimizer/stats.py` takes NDV, composite NDV and a string's mean length from the dataset's
sidecar (`testdata/stats/<dataset>.json`, written by `testdata/gen_stats.py` with DuckDB) and
everything else — rows, min/max, null counts — from the parquet footer over the row groups a
scan reads, so a pruned scan's statistics are its survivors'. NDV comes from the sidecar because
a footer's distinct count, where DuckDB writes one at all, is per row group, and those do not add
up to a table's; counting it at plan time would be a full scan, so the sidecar counts it once per
dataset.

- **The fingerprint.** Each table's sidecar entry records its rows and a sha256 over every flat
  column's per-row-group null count and raw min/max. A table whose file does not match is
  refused, table by table: never a guess, since NDV = rows is the error the sidecar removes.
  Vector columns (`*_embedding`) are left out of both the NDVs and the hash, so one sidecar
  fits tpch generated with synthetic or external embeddings. The fingerprint cannot catch every
  stale NDV — values can change between unchanged extremes — so `test_stats_sidecar` recounts
  the sidecars from the data, byte for byte.
- **Decimals** are bounded from the stored integer and the scale: pyarrow before 23 cannot
  decode a decimal kept as INT32/INT64, which is how DuckDB writes them.
- **Intermediate NDV at a build** (`optimizer/observed.py`), the only statistics an intermediate
  result has. A build at `build_done` is materialized one batch per lane, so its distinct keys are
  counted per lane, never across lanes. Lanes hashed on columns among those counted hold disjoint
  values and the sum is exact; otherwise the NDV lies between the largest lane's count and the
  sum. Every build side in the goldens is on one lane or hashed on its own keys, so a join's own
  key NDV is exact; the bounds are for its other columns. `cardinality.measured` clamps the
  estimated NDV into those bounds and takes the true rows and NULL shares.

## The estimator's known limits

`scripts/exec_model/testdata/goldens/<bench>/tp1-single.cardinality.txt` holds every join's
estimate against the engine's rows, so a moved estimate is a named line. tpcds q64 is where the
estimator is worst: 37 joins, median q-error 4.88, 5 within 2×; the rest of tpcds is 599 joins,
median 1.21, 73.3 % within 2×. Each half of q64 (store_sales for 1999, then 2000, 18 joins each)
misses by independence and default assumptions, none of them a correlation:

| node | estimate | true | why |
|---|---|---|---|
| `cs_ui` HAVING `sum(cs_ext_list_price) > 2 * sum(refunds)` | 3,572 | 17,157 | an aggregate against an aggregate: DuckDB's default 0.2, true 0.96 |
| `cs_ui ⋈ (store_returns ⋈ store_sales)` | 57,142 | 279,021 | carries it: q 4.88 |
| the FK joins to `date_dim` (`d_year = 1999`), store, customer, two more dates, `cd1` | 11,373 throughout | 53,265 → 48,301 | each keeps every row in the estimate; the truth loses 0.7–3.5 % per join to NULL foreign keys |
| `cd1.cd_marital_status != cd2.cd_marital_status` | ×0.20 | ×0.79 | a column against a column: the default 0.2; five values, so independence would give 0.80 |
| promo, `hd` ×2, `ca` ×2, `ib` ×2 | 2,275 | 38,096 → 37,840 | the product of the above: q 16.6 (1999), 17.1 (2000) |
| item: six colours, `i_current_price` in [65, 74] | 105.6 | 12 | price read as uniform over 0.09–99.99 keeps 9.0 %, truly 0.92 %: skew, no histogram |
| `ss_item_sk = i_item_sk` | 13.3 | 37 / 2 | the item overestimate half cancels the rest: q 2.77 and 6.67 |
| the root, `cs1 ⋈ cs2` | 2.7 | 2 | q 1.33 |

So three limits: no histograms, so a skewed column's range is read as uniform; a comparison of
two aggregates, or of two columns, is the default 0.2 whatever the NDVs say; and a semi or anti
join's residual scales the matched share as if each row had one partner. Fixing the second alone
exposes the third. Read as one value of the larger NDV, `a != b` gives q64's residual 0.80 and
tpcds a median of 1.26 against 1.29, but tpch q21's anti join goes from q 2.82 to 4,141: it is a
self-join, each row matches itself and `l_suppkey != l_suppkey` always rejects that pair, which
independence cannot see. It also exposes an aggregate's NDV under functional dependence (q46,
q68: an aggregate keyed on `ss_ticket_number` with three columns it determines is estimated at
its input, 123 thousand against 10.7 thousand). Scaling each equi-join by its keys' non-NULL
shares fixes q93 (10.7 → 1.03) and worsens 86 tpcds joins against 64 better, q64's middle
joins most, since they are under already.

## What the prototype does not model

- **Concurrency.** The scheduler names what may run together — every lane of the picked node, the
  source lanes to read ahead — and the driver runs them one after another in one Python thread.
  A prefetch is a call made in the step, not overlap.
- **Time on the device.** Nothing is timed. `optimizer/call_cost.py` fits a per-call model from
  the sf40 calibration record, `fixed + slope × volume` per kind, which is what lanes and batch
  size would be chosen by where C_out sees no difference; no rule reads it yet.
- **The plan-time resident estimate** (`estimated_max_resident_size`), which the engine derives
  in Rust; and window functions, which the planner refuses (#143).

Device bytes, the replanning's own cost, and statistics past a finished build are above, where
each matters.
