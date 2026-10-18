# exec_model — the optimizer's rules

Every rule the join-order optimizer applies, when it applies, and the function that applies it.
Why each rule has its shape is [`design.md`](design.md); how to run it is [`README.md`](README.md).
Paths are under `scripts/exec_model/`; a constant's value is in [Constants](#constants).

## The pipeline

`optimizer/pipeline.py`: `filtered` is everything before the run, `run_optimized` adds the run.

1. **Plan text.** `plans/engine_plan` reads the engine's `<mode>.plans.txt` into `EngineNode`s.
2. **Names.** `names.aliased` numbers a table the plan reads twice (`date_dim#2`); `multijoin.named`
   gives each cluster its id, `MJ<n>`. Both feed the report alone: no rule reads them.
3. **Dynamic filters, first pass**, on the plan as written (`dynamic_filters.apply`,
   `Builds.FILTERED`).
4. **Join order.** `join_order.ordered`, with the first pass's builds measured
   (`replan.kept_estimates`): DPhyp on C_out for each cluster, then `orient`, then
   `disassembly.reassembled`.
5. **Dynamic filters, second pass**, on the ordered plan (`Builds.FILTERED_OR_JOINED`). Nothing is
   ordered again before the run; only a replan orders again (step 6).
6. **The adaptive run.** `replan.run_adaptive`; the plan reads both passes' builds from memory.
   Where a build misses its estimate, the run replans: step 4 again, never steps 3 or 5.

## When a dynamic filter fires

`optimizer/dynamic_filters.py`. A dynamic filter runs a join's build side first, as a plan of its
own, and drops the probe-side scan's row groups whose footer range can hold none of its keys.

`candidates` takes a `(join, key pair, scan)`, each key pair and each scan the key reaches on its
own, when all of these hold (`_join_candidates`):

- **The join.** A `GpuHashJoin` whose type emits no unmatched probe row (`PRUNES_PROBE`). A
  `Left…` type keeps the build side, so `Left`, `LeftSemi`, `LeftAnti` and `LeftMark` qualify, as
  do `Inner` and `RightSemi`. `Right`, `Full` and `RightAnti` never do; nested-loop and cross
  joins never do.
- **The build side** (`_admitted`). First pass: it holds a `GpuFilter` and no join (hash,
  nested-loop or cross). Second pass: it holds a `GpuFilter`, a join, or both. A bare scan never
  qualifies. No estimate gates a build. A build probed once is a memory source, neither filter
  nor join, so it is not probed again.
- **The scan.** The probe key lifts ([below](#where-a-key-lifts)) to a `GpuLoadParquet` column,
  the scan reads at least 2 row groups, and it is ordered on the column: `Σ(max_g − min_g) / (MAX −
  MIN)` over its groups' footer ranges is below `ORDERED_BELOW` (`_clustering`). The score is 1 on
  an ordered column and about the group count on an unordered one. A group without statistics is
  left out of it. There is no score, so no candidate, when no group has statistics, when `MAX =
  MIN`, or when a range is not on a number line (`as_number`: integers, floats, decimals, dates;
  strings have none).

### Values or a range

`summarize` reduces the probe plan's build keys to their min, their max, whether one is NULL, and
the distinct values where they are at most `IN_LIMIT`. `row_group_survives` keeps a group whose
footer range meets [min, max] and, where the values are known, holds one of them. A group without
statistics, or with non-numeric ones, stays. A build with no non-NULL key drops every group, save
under the NULL rule below. Several candidates on one scan keep the intersection of their survivors.

### NULL keys

A join matches a NULL key to a NULL key where the plan says `null_equals_null`, whatever its
type. A join that equates NULLs stays a candidate; where its build holds a NULL key, that candidate
keeps every row group (`_round`), since the NULL matches the NULL probe keys any group may hold.

## Where a key lifts

`lift` follows the probe key's output ordinal down to a scan column:

| node | the key passes |
|---|---|
| `GpuEmitPartitions`, `GpuMergePartitions`, `GpuCoalesceAllBatches` (`_PASS`) | unchanged |
| `GpuFilter` | through its `projection`, where it has one |
| `GpuProject` | where its output at that position is a plain column reference; a computed one stops it |
| `GpuHashJoin`, any type | through its `projection`, then `joins.own_output`, into the side the column comes from; a mark column stops it |
| `GpuUnion`, `GpuInterleave` (`_BRANCHES`) | into every branch at the same ordinal |
| `GpuLoadParquet` | the end: this scan, this column |
| anything else | stops: aggregates, sorts, limits, nested-loop and cross joins, memory sources |

A semi or anti join outputs one side, so the key lifts only into that side. A branch that cannot
lift stays whole and the others are pruned; the branches share one probe, each scan pruned on its
own footers. Why this keeps every answer: `design.md`, "Why a non-inner join passes the key".

## What a probe does and costs

- **The probe plan** is the candidate join's whole build side (`probe_plan`). `filtered` runs it
  through `replan.run_adaptive`, as the main plan: a build inside it that misses its estimate
  replans there ([When a run replans](#when-a-run-replans)), and its memory sources take names no
  other run of the query has given (`taken`). Candidates that share a build share one probe.
- **Its output** replaces the build side as a `GpuMemorySource` named `p<n>` (`names.sourced`,
  `engine_plan.memory_source`): the side's lanes, hash and schema, one batch per lane. The main
  plan reads it and does not run the side again.
- **Its size is measured** (`cardinality.measured`): true rows and NULL shares, each NDV clamped
  into the bounds its per-lane counts give. DPhyp and the replan trigger read that.
- **Its cost.** `run.py` prices the optimized run as the final run, plus each probe plan's last
  run, plus every run a replan stopped, in a probe or in the main run (`OptimizedRun`). Reading a
  memory source costs nothing (`plans/cost_model.FREE`).
- **Its limit.** A probe that prunes nothing still materialises its build, and the main run reads
  it exactly, so that build gives the main run no miss of its own. The cluster around it is ordered
  again only where another build misses. tpcds q84 is the corpus case (`design.md`, "Probe plans
  run adaptively").
- **Pruned scans are re-mapped** onto their own lanes (`remap`, after the engine's
  `scan_mapping/partition.rs`): survivors in contiguous chunks balanced by rows, then one batch per
  lane or per row group; the sized mode keeps its batches less the pruned groups (`Batching`).

### Rounds

`apply` runs in rounds until no candidate is left. `_ready` takes the candidates whose build holds
neither another candidate's join nor a scan another ready candidate prunes. The rest wait, so
their probe reads the inner build from memory and the scan already pruned. The topmost ready join
waits for nothing, so a round is never empty; each round turns builds into sources, which no
longer qualify, so a pass ends. Names `p<n>` continue across rounds and passes.

## Clusters

`optimizer/multijoin.py`. A cluster is a maximal tree of joins inside which the order may change.

- **Joins that enter** (`is_reorderable`): a `GpuHashJoin` of any type but mark, with or without a
  residual filter, as the engine runs every one. The cluster grows through `_WIRING`, through
  projects that compute nothing (`_picks_columns`), and through the planner's null-key filter
  (`_is_null_key_shuffle`): `IS NOT NULL` alone on columns a `GpuEmitPartitions` hashes and the join
  keys on, on a side `NULL_KEY_DROPPABLE` names, of a join not holding NULL equal. So tp1 and tp4
  hold the same clusters; a query's own `IS NOT NULL` there stays a filter.
- **Walls.** Anything else roots a relation, a leaf of DPhyp's hypergraph: a mark join (its mark
  column is no relation's); any other `GpuFilter`, even between joins (an `IS NULL` there makes a
  left join an anti join); a computing project, nested-loop and cross joins, scans, aggregates,
  unions, sorts, limits and memory sources. `clusters` finds the clusters inside relations too,
  outermost first.
- **Columns** are `(relation, ordinal)` identities (`RelColumn`), never positions.
- **Edges, inner joins only** (`_Assembly._edges`): one per relation pair a join's keys connect,
  and one hyperedge per residual, over the relations it reads on each side.
- **Edges with a non-inner join** (`_Assembly.multijoin`): one per join, its whole predicate, keys
  and residual together. Its sides are the join's TES (`conflicts.eligibility`), the kept side on
  the left; `conflicts.logical` reads a `Right…` join as its `Left…` mirror. The TES is CD-A
  (Moerkotte, Fender, Eich, SIGMOD 2013, §5): the relations the predicate reads, plus, for each
  join below that may not swap past this one by `_assoc`, `_l_asscom` or `_r_asscom`, that join's
  side which would be left behind. A predicate is strong when it has no residual and no
  `null_equals_null`: a residual's rejection of NULLs is not read.
- **Ids** (`named`): `MJ<n>` per query in `clusters`' order, on every join of the cluster
  (`EngineNode.mj`); disassembly gives it to the joins it makes, and a replan's plan keeps its ids.

## The cost DPhyp is given

`join_order.ordered` calls `dphyp.solve`, the `peacockdb-dphyp` crate over its C ABI, once per
cluster. DPhyp gets the relations `0..n−1` and the edges that carry keys: a set joined only by a
residual would need a nested loop. Its callback is `cost(mask)` in `ordered`, which returns
`SetEstimates.cost(mask)` and records the set for the report. DPhyp asks it once per connected
set, never for one relation (which costs 0), and a tree costs the sum over its joins:

    cost(S)  = rows(S) × width(needed(S))        bytes
    C_out(T) = Σ cost(S) over the joins S of tree T

- **`rows(S)`** is a cardinality estimate, below, and the cost's only estimate: the width comes
  from the types, the sidecar's string lengths and the NULL shares.
- **`needed(S)`** (`SetEstimates.needed`): the cluster's output columns from S's relations, plus
  each key or residual column of S read by an edge reaching both into and out of S.
- **`width`** (`cost.row_width`): per column, 1/8 byte of validity plus the Arrow type's size
  (`_FIXED`; a string counts its offset), plus, for a string, its sidecar mean length × (1 − NULL
  share). A string the plan computed has no known length and counts its offset alone.
- **Units** are bytes, with no calibration: every term is a join's output, so per-kind weights
  would cancel between two orders.

### Where rows come from

- **A relation** takes the plan's own estimate of its root (`cardinality.estimator`), made in two
  passes so two keys of one equivalence class count once. Rows, min/max and NULLs are the footers'
  over the row groups a scan reads, so a pruned scan counts only its survivors; NDV is the
  sidecar's (`stats.py`). A filter multiplies rows by its selectivity, pins the NDV of each column
  it restricts and thins the rest by Cardenas (`Estimator._filter`). An aggregate's rows are the
  product of its key NDVs, capped at its input. A memory source is its measured estimate.
- **A set** is estimated over one canonical split, so it has one estimate however DPhyp reached
  it. Inner joins only: the highest relation that leaves the rest connected (`_join`). With a
  non-inner join: the highest relation an edge joins to a set the edges make, else the first two
  such sets (`_split`), joined as the edge's type has it (`_applied`).
- **Two parts are joined** by `Estimator.equi_join`; `cardinality.typed` applies the join type.

## Which selectivities enter it

The cost is cardinality × width, and the cardinality reads these selectivities. A filter below a
relation enters by its predicate; a join's own residual inside the cluster enters only as a flat
`DEFAULT` (0.2), its predicate not read. The plan's estimate prices a residual by its predicate
(`Estimator._joined_selectivity`): that is what a wall join inside a relation gets, and what the
replan trigger compares a build against.

L is the side estimated first, R the other: the build in the plan's estimate, a non-inner edge's
kept side in a set's. `spread = max(NDV_L, NDV_R, domain, 1)`.

| factor | rule | where |
|---|---|---|
| filter below a relation | DuckDB's model: an equality is 1/NDV of the non-NULL rows, 0 outside [min, max]; an OR of equalities on one column its literals over NDV; a range its share of NDV evenly spaced values; AND multiplies, OR is a + b − ab, NOT 1 − s; `IS NULL` the NULL share; `DEFAULT` where statistics are silent (LIKE, column against column, a computed operand, a string range) | `estimator.selectivity` |
| join containment | `inner = rows_L × rows_R / spread × residual` | `Estimator.equi_join` |
| key NDV | keys of one class count once; several classes, the sidecar's composite NDV where all are one table's, else the product; capped at the side's rows | `Estimator._key_ndv` |
| domain | per class, each base NDV thinned to the share of its own range the other's covers, the larger of the two; several classes, the larger whole-table key NDV where the keys are a table key, else the product; 0 where a key has no base column | `Estimator._domain` |
| matched shares | over the key domain: `m_L = NDV_R / spread × residual`, `m_R = NDV_L / spread × residual` | `Estimator.equi_join` |
| non-inner types | semi keeps `rows × m` of its side, anti `rows × (1 − m)`; `Left` adds `rows_L × (1 − m_L)`, `Right` `rows_R × (1 − m_R)`, `Full` both; mark keeps every row | `cardinality.typed` |
| residual of a join in the cluster | `DEFAULT` per residual edge the join applies, its predicate not read | `SetEstimates._join`, `_applied` |

### The budget

DPhyp may enumerate `MAX_PAIRS` connected pairs, DuckDB's budget. Past it `dphyp.Unsolved` is
raised with reason `budget`, and the cluster keeps the plan's order (`disassembly.baseline`),
oriented again. There is no greedy fallback. Any other refusal raises.

## Orientation

`join_order.orient`, bottom up over DPhyp's tree after it returns.

- **An inner join** builds the side with the lower `bytes(build) × (1 + copies × batches(probe))`,
  `copies` = 1. The term no longer prices a copy, since the session builds once; it stays because
  pricing the build as the session holds it measured worse (tpch q9), and what it should price is
  #294's question. `bytes` is a relation's rows × its whole row width, or a joined subtree's
  `cost(S)`. `batches(probe)` (`_fed`) is what the probe emits (`batches`, off its layout) where the
  join runs on one lane or the probe is a relation already on the join's lanes hashed exactly on its
  keys (`_hashed_on_keys`); otherwise it is shuffled first, `min(batches × lanes, max(rows, 1))`. A
  join's output always counts as shuffled, and a join emits one batch per probe batch. A join runs
  on the lanes the plan's join over the same relations had (`disassembly.planned_lanes`), else on
  the mode's (`pipeline.MODES`). On a tie DPhyp's left side builds.
- **A non-inner join** keeps the plan's type and build side (`_plan_builder`): it builds the TES
  side the plan's join built.

## Disassembly

`optimizer/disassembly.py`: `disassemble` turns an oriented tree back into `GpuHashJoin`s.

- **Type** (`join_type_of`): inner, or the edge's non-inner type, mirrored (`conflicts.MIRRORED`)
  where the kept side probes. Orientation keeps the plan's build side, so the plan's type comes
  back.
- **Keys** from every edge between the two sides (`_predicates`); they must agree on
  `null_equals_null`, and a join with no key is refused. A residual goes on the first join whose
  sides hold all it reads.
- **Projection**: what the joins above and the cluster's output need (`_join`).
- **Wiring** as the translator derives it (`_wired`): each side onto the join's lanes and, past
  one lane, shuffled onto its keys (`GpuMergePartitions`, then `GpuEmitPartitions`) unless hashed
  on them already; the build one batch per lane (`GpuCoalesceAllBatches`). A shuffle on keys the
  plan's null-key filters tested (`MultiJoin.nullable_keys`) goes through that filter again, on
  the sides `NULL_KEY_DROPPABLE` names for the join type. Past one lane the
  output is hashed per `_HASHED_BY`, each only where those keys are output: `Inner` on its build
  keys, else its probe keys; `Left`, `LeftSemi`, `LeftAnti` on the build keys; `Right`,
  `RightSemi`, `RightAnti` on the probe keys; `Full` on neither.
- **The root** keeps the plan root's schema, names included. Where a node above reads its hash
  before the next exchange (`_EXCHANGES`) — a hash join, a final aggregate, an enclosing
  cluster's join whose keys cover the hash the relation declares, or what consumes a probe plan
  (`multijoin.clusters`, `_reads_hash`, `_hash_joined_on`, `MultiJoin.hash_read_above`) — a new
  root on another hash or lane count is shuffled back onto the plan root's.

## When a run replans

`optimizer/replan.py` over `engine/adaptive.py`, in the main run and in every probe plan.

- **The event.** `AdaptiveDriver._new_events` raises `BuildDone` once a join's build side has
  emitted on every lane, before the join sets its build: true rows, bytes, key NDV. Every join with
  a build raises it: hash, nested-loop, cross. `BuildExceeds` exists too; `run_adaptive` sets no
  caps, so it never fires.
- **The trigger** (`run_adaptive`'s `stops`, `BuildMiss.q_error`): `q = max(t / e, e / t)`, `t` the
  build's true rows and `e` the estimate of the join's build child under the current plan, every
  memory source so far known, both floored at 1. The run stops where `q > DEFAULT_THRESHOLD`.
- **Refusal** (`AdaptiveDriver.extraction`). The run stops only where every node that has made a
  call (prefetches aside) lies in a build being taken or is its join. Taken are the builds done
  whose join has made no call, and those set whose join has probed nothing. Otherwise the miss is
  recorded in `refused` and the run goes on: a replan would run started work again.
- **What is re-optimized** (`run_adaptive`, `_with_memory_sources`). Each build taken becomes a
  measured `GpuMemorySource` named `m<n>`; `join_order.ordered` runs again over the whole plan —
  every cluster, every source known, DPhyp, orientation, disassembly — and a new driver runs it.
  Dynamic filters do not run again; cluster ids are kept.
- **It ends.** A memory source is estimated at its true size, so a build that misses holds a scan
  or a join no source covers. The replan puts it under one, and a reorder adds no join, so the
  scans and joins outside sources fall with every replan: a run replans at most once per scan or
  join that no memory source covers when it starts.

## Constants

| name | value | file | read by |
|---|---|---|---|
| `PRUNES_PROBE` | Inner, Left, LeftSemi, LeftAnti, LeftMark, RightSemi | `optimizer/dynamic_filters.py` | `candidates` |
| `_PASS` | Emit, Merge, CoalesceAllBatches | `optimizer/dynamic_filters.py` | `lift` |
| `_BRANCHES` | `GpuUnion`, `GpuInterleave` | `optimizer/dynamic_filters.py` | `lift` |
| `ORDERED_BELOW` | 1.5 | `optimizer/dynamic_filters.py` | `_join_candidates` |
| fewest row groups | 2 | `optimizer/dynamic_filters.py` | `_join_candidates` |
| `IN_LIMIT` | 50, DuckDB's `dynamic_or_filter_threshold` | `optimizer/dynamic_filters.py` | `summarize` |
| `_WIRING` | Coalesce, Emit, Merge | `optimizer/multijoin.py` | `_Assembly._collect` |
| `_EXCHANGES` | Emit, Merge, MergeSortedPartitions, Unload | `optimizer/multijoin.py` | `_reads_hash` |
| `MIRRORED` | Inner, Full; Left↔Right, the semi and anti pairs | `optimizer/conflicts.py` | `logical`, `multijoin.is_reorderable`, `disassembly.join_type_of` |
| `DEFAULT` | 0.2 | `optimizer/estimator.py` | `_Estimate`, `join_order.SetEstimates` |
| `_FIXED`, validity | Arrow bytes per value; 1/8 byte | `optimizer/cost.py` | `row_width` |
| `MAX_PAIRS` | 10,000 | `optimizer/join_order.py` | `ordered` |
| `copies` | 1 | `optimizer/join_order.py` | `orient`, `ordered` |
| `MODES` | 1 lane at tp1-*, 4 at tp4-* | `optimizer/pipeline.py` | `mode_shape` |
| `_HASHED_BY` | per join type, above | `optimizer/disassembly.py` | `_Disassembly._join` |
| `NULL_KEY_DROPPABLE` | (build, probe) per join type, as `plan::null_key_droppable` | `operators/join_types.py` | `multijoin._Assembly._collect`, `disassembly._Disassembly.part` |
| `DEFAULT_THRESHOLD` | 2.0 | `optimizer/replan.py` | `run_adaptive` |
| q-error floor | 1 row | `optimizer/report.py` | `BuildMiss.q_error` |
| `FREE` | `GpuMemorySource` | `plans/cost_model.py` | `CostModel.price` |
