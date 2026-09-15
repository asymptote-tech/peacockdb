# peacockdb architecture

The code is authoritative: where this page and the code disagree, fix the page rather than the
reading, and say so.

Pipeline: SQL → DataFusion logical/physical plan → the engine's node tree
(`peacockdb-core/src/plan/`) → a recipe plan in the FlatBuffers vocabulary
(`flatbuffers/gpu_plan.fbs`) → the C++/cuDF executor, one node at a time. One tree runs on
either backend: `CpuBackend` relays a call to DataFusion, `GpuBackend` makes it through the
C ABI.

A **lane** holds a *stream of batches* rather than one resident table, and that is the whole of
why the engine is shaped this way. Load → filter at 1% selectivity → aggregate into few groups materializes
the whole scan before the filter runs if a lane is one table; with batches only the aggregate's
state stays resident, so the query fits a budget the table does not.

## Contents

[Planning](#planning) · [The node set](#the-node-set) · [Joins](#joins) ·
[Execution](#execution) · [The wire format](#the-wire-format) ·
[Interfaces](#interfaces) · [Rehash and the comet hash](#rehash-and-the-comet-hash) ·
[C++ executor layout](#c-executor-layout) · [Column indexing](#column-indexing) ·
[cuDF options](#cudf-options) · [Node display](#node-display) ·
[Multi-GPU notes](#multi-gpu-notes-cudf-2602) ·
[Cost model and the DuckDB oracle](#cost-model-and-the-duckdb-oracle)

## Planning

`planner::plan()` takes DataFusion's physical plan, built at the target lane count by the
caller, and **translates** it into the engine's node vocabulary (`planner/translator/`).
Translation, not annotation: a 1:1 wrapper carries
DataFusion's execution semantics along with it, and this model's semantics are different at
every node.

**It translates twice**, because the two halves define each other: a batch size needs the
tree it flows through, and the mapping needs a batch size. The first pass seeds every source
with the smallest batch the mapping can express and exists only to be priced; the second
plans at the sizes the estimator solved for. The tree's shape does not move between them —
lane counts come from row counts and pushed-down limits, never from the batch size — and the
two passes address sources by the order translation reaches them, so a second pass reaching a
different number of sources is a plan-time error rather than a tail planned at the seed size.
Only the budgeted mode pays for the second pass; the other two are the plan already.

What DataFusion is reused for is its planning, never its execution:

- physical expression planning and type coercion — the coercions it resolved and the
  decimal precision and scale it derived;
- per-aggregate state schemas (sum+count for avg, Welford's triple for stddev), read off
  `AggregateExpr::state_fields()`. The *split* is ours, since a batched lane needs a
  per-batch init and a merge whatever the lane count;
- grouping-set expansion, which arrives as an ordinary `__grouping_id` column;
- row-group pruning, which hangs off `ParquetExec` statistics.

The layer makes a conscious decision per DataFusion node kind, and an unrecognized one is a
plan-time error naming it — never a silent pass-through. Expressions are translated the same
way, kind by kind, into the engine's own IR (`plan/mod.rs`), because a column
reference is an ordinal into a child whose column order the engine decides. Ordinals rebase at
every node the layer inserts, so a per-branch cast project or an inserted merge shifts every
reference above it.

An aggregate's argument expression is evaluated against a different table in each phase — the
init sees input columns, the merge sees state columns — which is the whole of the #55/#56/#63
bug class: an expression written for one phase is not valid against the other, and the C++
finds out at run time.

**Targeted unit tests are the coverage; the plan goldens are not.** One test per node kind,
per expression kind, per planner rule — the shuffle insertion, the limit lowering, the
aggregate split, the mapping, the small-table drop — each from the smallest plan that reaches
it. A golden over a whole TPC-DS query goes red when anything upstream moves and names a file
rather than a rule.

### Modes and knobs

Two knobs, and five modes over them. **Lane count** is `target_partitions`. **Batching** is an
enum rather than a magic target value, and it is two enums because a mode asks for a shape
before there is a number: `BatchSizing` is what a mode names, `Batching` what the partitioner
gets — `Off` one batch per chunk, `PerRowGroup` one per row group, `Sized { target_batch_bytes }`
the estimator's number.

| Mode | Lanes | Batching | Budget |
|---|---|---|---|
| `tp1-single` | 1 | `Off` | no |
| `tp1-rowgroup` | 1 | `PerRowGroup` | no |
| `tp4-single` | 4 | `Off` | no |
| `tp4-rowgroup` | 4 | `PerRowGroup` | no |
| `tp4-sized` | 4 | `Sized` | **yes** |

There is no `tp1-sized`: at one lane a source takes essentially the whole budget, so the
sized form collapses to `Off` and the mode carries no signal. One mode takes a budget, and it
records the tier in-band in its `--- memory ---` section since the label does not carry it;
the other four reproduce from the data alone.

Even with batching off the loader declares `MultipleBatches` — no downstream node may assume
one batch per lane. The count is known at plan time, so saying so is available and is
[#170](tickets.md#t170).

**The small-table rule is per region, and a region ends at the nearest shuffle.** A source
reading fewer than `planner::SMALL_TABLE_BYTES` drops to one lane even at tp4, measured in bytes
actually read — the parquet column-chunk total over the projected columns of the surviving row
groups. Rows would misjudge it both ways: a narrow table of many rows reads less than a wide
table of few, so the threshold is a property of the scan and the same table can be above it in
one query and below it in another.

Demoting a region changes the lowering, not a number, and the two halves of a shuffle answer to
different things. The `GpuMergePartitions` goes, since there is nothing to merge. The
`GpuEmitPartitions` stays whenever its consumer still wants n lanes hashed on its keys — a
small dimension table joins a four-lane fact table by emitting into four lanes, having merged
nothing — and goes only when the consumer is itself at one lane. So the two batching modes
differ in plan shape and not only in the loader's mapping, and a small dimension table is where
to look for the difference.

### The row-group mapping

The row-group → (lane, batch) mapping is computed once, at plan time, by a pure policy function
(`planner/translator/scan_mapping/`), and everything else consumes its output: the loader
stores it verbatim, the plan golden renders it verbatim as `partition_groups=[…]`, validation
checks the declared lane count against it. One fact, one owner.

Survivors split into `n` contiguous chunks balanced by row count — a chunk ends where taking
the next group would land further from its share than stopping does, rather than where the
share is first reached, which overshoots by a whole group. Within a chunk, consecutive row
groups pack greedily into batches while bytes stay under target; a single row group over target
is still its own batch, so the minimum granularity is one row group and the planner always
produces a plan.

**The balance bound holds for uniform row groups, and contiguity is why it is not universal.**
Max−min lane rows ≤ one row group is true of what a parquet writer emits — one group size per
file, a short last group — and tight there rather than loose. Row groups differing by orders of
magnitude within one file beat it, because a contiguous chunk cannot step over a large group to
balance around it. Contiguity is the stronger rule and stays; the bound is the property it buys
on real files, so it is asserted with that qualifier.

`bytes` is the parquet column-chunk total over the projected columns, not rows × a width
derived from types: a varchar's width is a property of the data and the file metadata already
holds it. Type widths are for columns the plan creates rather than reads.

### Batch sizing

`target_batch_bytes` is derived, not configured. The budget is what the hardware fixes; batch
size is how the planner spends it, so an estimator pass (`planner/memory_estimation.rs`)
solves for it per source.

The walk starts at each source and follows its batch upward to the nearest accumulator. Every
node in between is a per-batch transform, so what it holds is proportional to the source batch,
and the **amplification** is a product along the path: filter selectivity, the width change
across a project, a join's cardinality, and the lane count in force at that point. A source's
figure is the maximum over its path, not the value at either end — a batch is rarely widest
where it starts, and above a merge point it costs one lane's worth rather than n.

Accumulators end the walk because they are where residency stops scaling with batch size: a
join's build side holds a whole relation, an aggregate's state one row per group. Those are
constants, so they come off the budget first.

```
Σ_sources  amplification_s × lanes_s × batch_bytes_s   ≤   budget − Σ held_by_accumulators
```

The remainder is split **equally** across sources, each dividing by its own amplification; a
proportional split would hand the most budget to the source already producing the most bytes.

Four limits on the number it produces.

- The sum over sources is an upper bound, not a peak: the driver runs one node at a time, so
  two sources are rarely at their widest in the same instant.
- If the constants alone exceed the budget, the planner refuses only where the constant
  **cannot be an overestimate**. A build side is its input's rows. An aggregate's state rests
  on a cardinality estimate, and with today's trivial estimators ([#19](tickets.md#t19)) that is
  one row per input row — tpch q1 groups 6M rows into four and is modelled at 3.8 GB. Refusing
  on that would turn "we do not know" into "you cannot run this", so it goes in the memory
  section as the worst case it is and `ResidentAccountant` decides at run time.
- The output is a target, not a bound: the mapping is quantized to whole row groups.
- The inputs are estimates. Underestimating amplification produces a batch too large and a query
  the accountant kills; overestimating produces one too small and a query that is merely slower,
  so the derived target rounds down onto a coarse grid — a drifting estimate should not
  regenerate every golden.

## The node set

| Node | Category | Semantics |
|---|---|---|
| `GpuLoadParquet` | Source | reads survivor row groups per the mapping; `next_batch()`; honours a pushed-down limit |
| `GpuFilter`, `GpuProject` | Exec | 1:1 per batch. A filter also projects, and carries that projection |
| `GpuSort` | Exec | sorts each batch independently, optional per-batch `fetch`; output `BatchSorted` |
| `GpuAccumulateBatchesAndSort` | BatchAccumulator | accumulates sorted batches, one `cudf::merge` at done, `fetch` applied; one batch out, stream-sorted. Ranged emission is [#138](tickets.md#t138) |
| `GpuMergeSortedPartitions` | PartitionAccumulator | N sorted lanes → 1, all k·m batches into one `cudf::merge`, `fetch` applied |
| `GpuCoalesceAllBatches` | BatchAccumulator | concatenates a lane's batches into one at done |
| `GpuMergePartitions` | BatchForwarder | N lane streams → 1, forwarding each batch as visited, round-robin; accumulates nothing, no backend calls |
| `GpuEmitPartitions` | PartitionEmitter | 1 → N per batch by hash scatter; one call per input batch |
| `GpuAggregate` | Exec | aggregates one batch: init aggregators, plus the finalize where it is also the single-node shortcut |
| `GpuAggregateBatches` | BatchAccumulator | merges pre-aggregated batches; compacts on a doubling threshold; emits at done |
| `GpuHashJoin` | Join | the capability matrix below |
| `GpuCrossJoin`, `GpuNestedLoopJoin` | Join | two inputs, both one lane; broadcast variants are [#140](tickets.md#t140) |
| `GpuUnion`, `GpuInterleave` | BatchForwarder | lane relabeling only. Union sums its inputs' lane counts and clears the hash; interleave takes output lane p from lane p of each input and so preserves it |
| `GpuLimit` | BatchAccumulator, mid-plan only | an interval over a **one-lane** stream; streams and holds nothing |
| `GpuUnload` | Unload | `GpuBatch` in, `CpuBatch` out, over a row range the driver supplies; carries a root-adjacent limit's interval |

### The sort decomposition

A `SortExec` becomes a per-batch `GpuSort` plus an accumulator above it, because sorting each
batch leaves the batches individually ordered and collectively not. Which accumulator depends on
what the parent needs: one lane's stream sorted is `GpuAccumulateBatchesAndSort`, a
`SortPreservingMergeExec`'s N-into-1 is `GpuMergeSortedPartitions`, and both merge at done, so
both are pipeline breakers.

**The `fetch` comes from DataFusion alone**, which pushes a `LIMIT` into the sort, so a top-N
usually reaches us with no limit node in the plan at all. It is replicated onto every stage of
the decomposition — `GpuSort(fetch=n)` per batch, then the accumulator, then the merge — which
is sound because top-n distributes over concatenation, and is what makes a top-N
memory-bounded: each stage holds at most n rows per live batch instead of its whole input.
Skipping it on the accumulator would make a one-lane `ORDER BY … LIMIT 10` sort the entire
stream to return ten rows.

### The aggregate sequence

**An aggregate node carries no phase.** It declares what it computes — aggregators over its own
input, and optionally finalizing expressions over the results — and the planner emits the parts
each position needs. Every aggregate decomposes into three declared parts, each ordinary IR:

- **init** — aggregators over raw rows, emitting *state* columns. One aggregate may emit
  several: `avg` emits sum and count, `stddev` emits Welford's count, mean and m2.
- **merge** — aggregators over state columns, emitting the same state schema. Not the same
  functions as init: a `count` merges by `sum`, and Welford state merges by `merge_m2`.
- **finalize** — one expression per output column over the merged state: `avg` a divide,
  `stddev` a `CASE` over a `sqrt`, the rest a rename.

A node with no finalize list emits state; a node with one emits finalized columns. Nothing else
distinguishes the positions, so the single-node shortcut is not a third case — it is init
aggregators and finalize expressions on the same node.

| Aggregate | init (over rows) | state | merge (over state) | finalize |
|---|---|---|---|---|
| `sum(x)` | `sum(x)` | `o` | `sum(o)` | `o` |
| `avg(x)` | `sum(x)`, `count(x)` | `o$sum`, `o$count` | `sum`, `sum` | `o$sum / o$count` |
| `count(x)`, `count(*)` | `count(x)` non-nulls, or rows | `o` | **`sum(o)`** | `o` |
| `stddev(x)`, `stddev_pop(x)` | `count`, `mean`, `m2` | `o$count`, `o$mean`, `o$m2` | **`merge_m2`** | `CASE WHEN o$count − ddof <= 0 THEN NULL ELSE sqrt(o$m2 / (o$count − ddof)) END` |
| `var(x)`, `var_pop(x)` | as stddev | as stddev | **`merge_m2`** | the same without the `sqrt` |
| `max(x)`, `min(x)` | `max(x)` / `min(x)` | `o` | the same | `o` |

Three rows carry the weight. **`count` merges by `sum`** — the one place where naming the merge
separately is the difference between a right and a wrong answer. **`avg` is two state columns**,
never a mean of means. And **the Welford pair merges by `merge_m2`**, which the IR names because
the combine is not a per-column reduction: it needs the count-weighted mean and the cross term.
`ddof` is 1 for the sample forms and 0 for the population ones.

The registry lives in `plan/mod.rs` as two enums — `AggFunc`, what SQL asked
for, and `PlanAgg`, what a node runs — with state names and types from DataFusion's
`state_fields()` so our split cannot drift from the split it planned. Adding an aggregate is a
row there rather than an arm in C++; an aggregate that cannot be decomposed at all (a true
median) is an absent decomposition and a planner that declines to split the phase.

References render `name@ordinal`. Inside `aggs` the ordinal indexes the node's input; inside the
finalize it indexes the node's own intermediate table, `[group keys…, state columns…]`, so a
finalize can reach a group key or the `__grouping_id`. The name is checked against the declared
schema at that position rather than merely displayed.

Shortcuts: a one-lane single-batch input needs one `GpuAggregate` carrying both lists; a
one-lane input skips the merge and emit; a single-batch-per-lane input skips the first
`GpuAggregateBatches`. The shuffle is skipped only for one-lane inputs or keyless aggregates —
skipping on small key cardinality needs estimators that do not exist ([#141](tickets.md#t141)).

**DataFusion's partial-aggregation probe must be off**, and structurally so: an `AggregateExec`
in Partial mode stops grouping where the groups are nearly as numerous as the rows, which is
sound only because a Final stage regroups downstream. Here nothing does — the init emits state
and the merge is a Partial too — so duplicate keys reach the finalize as extra rows. Both
backends take a context with the threshold at `usize::MAX`. It is a silent wrong answer rather
than a failure: `GROUP BY ss_customer_sk, ss_item_sk` returns 2,797,913 rows against
DataFusion's 2,764,744.

### Grouping sets

No new node type. The set masks ride on the init aggregate (`grouping_sets`, `null_exprs`,
`null_names`) and everything above sees `__grouping_id` as an ordinary group column, so the
merge groups on keys + gid and the shuffle still hashes the keys alone. The rule that permits
that is `hashKeys ⊆ group columns` — subset, not equality — and equal group keys always carry
equal user keys, so co-location holds.

The gid is a real column: the expansion materializes an INT32 constant per set and appends it
after the group keys and before the aggregate outputs. Its rendering is asymmetric on purpose —
the init's `group_by` does not list it, because there it is a tag being synthesized, while every
node above lists it as an ordinary key. A projection over the final drops it again, without
which the query returns a column it never asked for.

A masked column is a typed NULL rather than an absent one, so every set shares a schema and sits
in one `cudf::table` distinguished by the gid. The ids are the bitmask of each set's **masked**
positions — a two-key rollup gives 0, 2, 3 — which is distinct per set and not DataFusion's
`GROUPING()` encoding ([#65](tickets.md#t65)).

Not a Spark-style expand: the C++ runs k groupbys over the same input and concatenates the k
results, so the peak is the input plus the sum of the per-set outputs rather than k times the
input. **It must be one batch rather than k**, and the reason is the driver: no executor may
return more than one batch per call per output lane, which is the queue bound the whole
flow-control argument rests on.

A rollup's last set masks every key, so those rows hash on nothing and land in the single lane
`pmod(seed, N)` — [#137](tickets.md#t137)'s shape, and one row, the grand total.

### DISTINCT lowers to grouping

`DISTINCT` is never a property of an aggregator: no `aggs` entry carries a flag and the wire's
`distinct` field is never set. The state of a distinct aggregate is *the set of its distinct
values*, and the only way this IR represents a set of values is as the rows of a grouped table.
So the distinct argument becomes an extra group key on an inner aggregate, and the aggregate
that consumed it becomes an ordinary one over the deduplicated rows.

`SELECT DISTINCT` and the set operations that lower to dedup arrive as an aggregate with group
keys and no aggregators, and take the ordinary sequence with an empty `aggs` list — correct
because dedup is idempotent and associative, so no finalize is needed either. One distinct
argument with `sum`/`min`/`max` companions arrives already rewritten by DataFusion's
`SingleDistinctToGroupBy` as two aggregates, each decomposing as any other.

Any other companion is refused at plan time ([#62](tickets.md#t62)). DataFusion refuses it too,
and its reason is a limitation of its rewrite rather than of the shape: it re-applies *the same
function* at the outer level, which is sound only where `f(f(x))` is `f(x)`. Our decomposition
has already separated init from merge — a `count`'s merge aggregator *is* `sum` — so the
restriction lifts, and closing #62 is a planner rewrite rather than a distinct-aware kernel.

Nulls fall out correctly in both directions, which is worth checking rather than assuming
because the two cases want opposite things. `SELECT DISTINCT` keeps a null as a value, since the
dedup groups it like any other key under `null_policy::INCLUDE`. `count(DISTINCT x)` must not
count it, and does not: the inner dedup produces one null row and the outer `count(x)` counts
non-nulls. Multiple distinct arguments over *different* expressions need a gid-multiplying
expand this lowering cannot express — [#144](tickets.md#t144), whose gid is not the ROLLUP one.

### Compaction runs on a doubling threshold

`GpuAggregateBatches` holds pre-aggregated state, and when it folds that state down decides
whether the sequence is memory-bounded. Both obvious policies are wrong in one regime:
compacting on every arrival keeps the state at group cardinality but re-scans it once per batch,
so where the groups are disjoint the work is quadratic in the batch count; never compacting
holds the whole input whenever cardinality is high.

So the node holds arrivals until they cross a byte threshold, compacts once, then sets the
threshold to twice what that compaction left behind. A low-cardinality aggregate leaves a small
state and the threshold never moves. A high-cardinality one leaves a state the size of its
input, so the threshold doubles away: compactions land at geometrically growing sizes and total
re-scan work is linear in the rows that pass through. Residency then grows, which is the honest
answer for that shape, and `ResidentAccountant` is the backstop ([#142](tickets.md#t142)).

**The shuffle beneath a final aggregate is coalesced first.** `GpuMergePartitions` forwards its
L lanes' batches without concatenating, so without a `GpuCoalesceAllBatches` between the two the
emit would scatter once per arriving batch and produce L×N of them. The reason is batch *shape*
rather than residency — all L pre-shuffle batches are resident either way. What it buys is N
batches at about G/N rows instead of L×N at G/(L·N), and one allocation per output lane instead
of L ([#145](tickets.md#t145) removes those copies altogether); what it costs is a concat over
the smallest data in the plan, this point being post-partial-aggregate. A join's probe-side
shuffle is **not** coalesced: its input is unbounded, and streaming past the build side is the
whole point.

### Every cast is explicit

No executor may change a type the plan did not ask it to. A type appearing at a node's output
that its input and its expressions do not account for is a defect whichever side of the boundary
invented it, and the plan golden prints the declared schema per node, so it is one a reader can
see.

Seven coercions are plan nodes rather than something an executor infers: `avg`'s decimal input
and its finalize divide, `count`'s widening to INT64, the stddev/var operands, union branch
types, a decimal divide's numerator, and `round`'s operand. Each is a `CastExprNode` the planner
emits — the aggregate ones inside the finalize expressions, the union ones as per-branch
projects, the expression ones at the point of use.

Two stay in C++ with a reason. The loader's decimal width is the source honouring the output
schema it already declares, since cuDF's parquet reader picks the narrowest fixed_point width
while DataFusion uses Decimal128 throughout. Hash key normalization feeds the hash alone and
never reaches a returned value — a cast that cannot change an answer is not one the plan needs
to carry.

### The limit lowering rule

A per-batch `GpuLimit` call cannot be correct: the wire node's skip/fetch are frozen per seq, so
every batch would be truncated to the same bounds and the right bound for the last batch is a
runtime value no frozen node can carry.

**A scan carrying a pushed-down limit plans one lane and one batch.** Where DataFusion pushes
the bound into the source it erases the limit node, and it is safe there because its scan is one
partition. Our lane count is our own decision, so four lanes each honouring `limit=3` would
answer with twelve rows — and `CudfScan.limit` becomes `set_num_rows` on every scan call, so B
batches would answer with B × limit. One lane and one batch make the loader's own limit the
whole answer.

**Root-adjacent** — feeding only `GpuUnload`, the common case — **there is no limit node**: the
interval becomes the unload's, which is where it belongs, since a limit over a stream about to
leave the device is a statement about which rows are worth moving across the boundary. The
driver then owns three behaviours keyed on that node. A batch entirely outside the interval is
never unloaded, its handle released where it stands — trimming after unload ships an unbounded
`skip` prefix across PCIe to drop it. The two batches straddling the ends are unloaded with a
row range. And once satisfied, the whole plan is held and pulls cease.

**Mid-plan** it is a real `GpuLimit` over a **one-lane** input, since an interval over N lanes
names no rows; the node checks that itself, because a statement about its child's layout belongs
where the node can name the fix. Its input is deliberately not required to be a single batch:
requiring that would put a `GpuCoalesceAllBatches` underneath, and a limited subquery under a
join would read the whole table to answer for a hundred rows. It streams and holds nothing —
outside the interval a batch is released uncalled, inside it is forwarded untouched, and only
the two straddling batches are sliced.

**Intervals nest.** Two on one root-to-leaf path are legal, and each counts the stream it is
handed. DataFusion's `combine_limit` merges the adjacent form, so only a limited subquery under
a join, or a limit at the root, reaches this layer as two intervals.

## Joins

The build side is always left and one batch per lane (the planner inserts a
`GpuCoalesceAllBatches`); the streamable side is always right. Translation swaps sides where
DataFusion chose otherwise, remapping the join type and restoring output column order with a
project.

### Capability matrix

| Mode | Also covers | What it becomes |
|---|---|---|
| **Inner** | multi-key and composite keys; `null_equals_null=true`; a residual filter, which still streams since every emitted row is decided by (build, this batch) | `GpuHashJoin{Inner}`, probe streams, no finish |
| **Right outer** (probe side preserved) | a DataFusion Left-outer the swap moved | `GpuHashJoin{Right}`, probe streams, no finish — a probe row unmatched in this batch is unmatched everywhere, because the build side is complete before the first call |
| **Left outer** (build side preserved) | a DataFusion Right-outer after the swap | `GpuHashJoin{Left}`, probe streams **with finish**; the accumulated probe keys are resident until it runs |
| **Full outer** | — | `GpuHashJoin{Full}` — Left's finish, Right's per-call emission |
| **Build-side semi family** — `LeftSemi` | `LeftAnti`, `LeftMark`; the filtered forms, which take a single-batch probe | probe streams with finish, and **the per-call join disappears**: a probe call is only the key project, so the build side is untouched until the finish consumes it |
| **Probe-side semi family** — `RightSemi` | `RightAnti` | probe streams, no finish — membership in a complete build side is a per-row question |
| **Cross join** | — | `GpuCrossJoin`, both inputs one lane |
| **Nested-loop Inner** | a predicate that is not AST-able, which takes the cross-then-mask path | `GpuNestedLoopJoin{Inner}`, probe streams |
| **Nested-loop Left** | — | `GpuNestedLoopJoin{Left}` with a **single-batch probe**: the finish trick accumulates keys and a predicate join has none |

**A merge is not something a join asks for; it is what a shuffle needs.** `GpuEmitPartitions`
takes a one-lane input, so any plan that hash-partitions a side puts a `GpuMergePartitions`
under the emitter. A side already co-located on the join keys needs neither node. Cross and
nested-loop joins are the exception, and there it is the join asking: with no key to co-locate
on, both inputs must be one lane.

**Equal lane counts are not co-partitioning.** DataFusion picks `CollectLeft` for two small
tables and emits no repartition, while both loaders still produce N lanes — so lane p of one
side holds nothing that must match lane p of the other, and joining lane-wise would silently
drop matches. Translation therefore checks the hash and not the count: unless both sides are
scattered on their own join keys, in key order, both merge to one lane. That is the broadcast
shape [#140](tickets.md#t140) removes.

**An interleave needs its branches to agree on lane count**, and they may not: output lane p
comes from lane p of each input, so branches with different lane counts have no such
correspondence and the node becomes a `GpuUnion` instead. tpcds q77 is the case — its store and
web branches stay four lanes hashed on their channel key while the catalog branch is a cross
join, which asks both its inputs onto one lane, so the union declares 4+1+4.

**Three shapes are refused at plan time.** Left, Right or Full with a residual filter, because
`execute_hash_join` applies the filter after the outer gather and so demotes the ON condition to
a WHERE ([#153](tickets.md#t153)) — a live defect in the C++ executor, not a limitation of the
planner.
RightSemi or RightAnti with a residual filter, because no swapped `mixed_*` variant exists; the
fix is orientation rather than code. And a nested-loop join that is neither Inner nor Left,
which the C++ rejects outright.

### What a streamed probe costs

`execute_node` erases every handle it reads, and no node duplicates one. So a build side probed
by B batches is needed B times and exists once, which is [#152](tickets.md#t152). Per probe
batch: the probe-local types (Inner, Right, RightSemi, RightAnti) need **one build-side copy**;
Left and Full that plus a copy of the probe batch, since the join consumes the batch and the
key accumulation needs it too; the build-side semi family **none at all**, its probe calls
never touching the build side. None of those copies can be taken — the surface has no symbol
for one, and `slice_handle` moves rather than copies — so the count predicts where a device
refuses: the probe-local types after one batch, Left and Full outright.

**What the finish pass computes.** "Which build rows matched at least once" is the one fact a
streamed probe loses, and it cannot cross the ABI, which returns a table and row counts. So the
lane keeps the probe keys and, at done, one `left_anti_join` (or `left_semi_join`) against them
answers the question in a single call. The build side is therefore built once more at finish, a
cost recorded with the ABI-change alternatives in [#136](tickets.md#t136).

Keys rather than probe rows, because the alternative is not a smaller concat — it is the
single-batch probe, which changes the join rather than its inputs. One probe batch means one
call, and one call means the whole join *result* materializes as one table; on a fan-out join
that result is the biggest thing in the lane. Accumulating keys keeps the output leaving a batch
at a time and holds only the key columns between calls.

**The finish computes it with the same semantics as a single call**, not a variant:
`null_equals_null` rides on the node and reaches the finish join too, hardcoded `EQUAL` for anti
and mark included, three-valued `NOT IN` trap and all (#80, #59). A lowering that quietly fixed
the null semantics would not be a substitute for the join it replaces.

**A lane whose probe produced no keys is the executor's problem, not the concat's.** A concat of
nothing throws ([#173](tickets.md#t173)), so the finish has to answer from the build side alone
— LeftAnti over an empty key table is every build row, which is what the CPU does and what a
device must be made to do.

### Cross join vs nested-loop join

Both are a join with no equality to hash, and which one DataFusion plans — and so which node
the translator meets — is decided by whether there is a predicate at all.

- **No join predicate ⇒ `CrossJoinExec`.** `SELECT * FROM region, nation` — a full cartesian
  product. In the corpus every case pairs a one-row aggregate result with another under no
  condition, e.g. tpcds q61 putting `sum(ss_ext_sales_price) as promotions` beside `total` so
  it can divide them.
- **A predicate that is not an equijoin ⇒ `NestedLoopJoinExec`**, carrying that predicate as
  its `filter`: `… WHERE a.r_regionkey < b.n_regionkey` becomes a `GpuNestedLoopJoin` with
  `filter=n_regionkey@1 > r_regionkey@0`.

The second is worth recognizing as a shape rather than an accident, because a `HAVING` against
a scalar subquery lands there: tpch q11's `having sum(…) > (select sum(…) * 0.000002 …)` is a
1×N join with an inequality, and the planner has nowhere else to put it. (Rewriting that into a
broadcast filter is the optimization #27 was archived for.)

## Execution

### Traits

The types are declared in `plan/mod.rs` and `executor/mod.rs` — the node vocabulary in the
first, the batch and executor traits in the second — and the code is what they are; what follows
is why they have the shape they do.

**Layout and schema live inside `NodeKind`** rather than as two `Option`s that must be `None`
together: a sink structurally has neither, everything else always has both, and there is nothing
left for a caller to get wrong. `PartitionLayout` carries the lane count, the key distribution
(Spark murmur3, seed 42, or not specified), the sort order and the batch layout. `SortOrder` is
two-valued on purpose — a whole-stream order is `BatchSorted` meeting `SingleBatch`, derived by
`is_stream_sorted()`, so nothing can disagree about it. `Schema` carries column types plus the
annotations a consumer can check: sort column, group key, aggregator, two-phase state.

`GpuNode::validate_schemas_and_partitions` runs **before** the generic structural rules, because
a node knows what it needs of its children and can name the fix: a limit over four lanes should
read "the planner inserts `GpuMergePartitions` below it", not "this category is 1:1 per lane".

**A batch is one table's worth of rows** — `num_rows()` and `byte_size()`, nothing else.
`CpuBatch` wraps an Arrow `RecordBatch`; `GpuBatch` wraps a `u64` handle plus the session
reference its `Drop` needs. Ownership is by move: every executor method takes its batch by
value, so reuse after consumption is a compile error rather than a run-time throw, and neither
batch type is `Clone` — a future dual consumer writes an explicit copy. A handle consumed by an
FFI call skips `Drop`, because C++ erased it.

**Executors are fused call interfaces**: every state transition emits in the same call, so there
is no wrong interleaving to construct and output timing is a pure function of the call sequence.
Seven categories — Source, Exec, BatchAccumulator, PartitionAccumulator, PartitionEmitter, Join,
Unload — one associated type each on `Backend`, so the driver is generic over the backend and
the GPU path monomorphizes: a `GpuBatch` is its `u64` with no box and no vtable, and backend
choice is a turbofish at the entry point rather than a selector consulted per node.

**Illegal calls are unrepresentable rather than checked.** Every method that ends a protocol
consumes `self`, so probing before `set_build`, calling `set_build` twice, probing after finish
and accumulating after done are compile errors; the source's consuming step removes the driver's
own `finished` flag, which would otherwise duplicate the executor's exhaustion and could
disagree with it. Two illegal states stay checked at run time on purpose: an `emit` returning
other than the plan's lane count (N is a plan value, so const generics do not apply) and a
second `Done` for one lane, which would need per-lane state in the type for no gain.

`GpuUnload` needs its own category because it is the one operator whose output type is not
`B::Batch`, and the one whose call signature the limit rule reaches into: the row range is a
call argument, since the count it derives from is cross-lane while an unload instance is per
lane.

**A call can fail, and failing ends the query.** Every executor method returns a `Result`, and
the error carries a message and no kind, because there is one response to all of them: stop. The
driver adds the node and the lane and fails the query — no retry with a smaller batch, which is
[#142](tickets.md#t142)'s adaptive future.

The C ABI is what that rests on. `execute_node` resets the session on any exception, dropping
the plan and every resident intermediate, so after a failure no handle is usable, while
`handle_release` is null-guarded and releasing into a reset session is a no-op. The driver
therefore needs no teardown: it stops scheduling, and the failure site releases the batch it
was handed, exactly where the successful path would have.

`resident_bytes` and `scratch_bytes` stay infallible, the line being between a method that does
work and one that reports a number the executor already holds. An accountant handed a failure
instead of a figure has nothing to do with it: zero stops the check enforcing anything, unbounded
kills a query over a reporting hiccup, and skipping the check disables the guard silently.

**Executor construction is the backend's**, as `Backend::executors_for(ctx, node, post_order,
lane)`. It cannot be a generic method on `GpuNode`, which is a trait object, and it is the
better placement anyway, since a node describes what it computes and stops knowing backends
exist. It returns a `Result` because this match is where "does this backend implement this
node" is answered, and that question has a no. `post_order` is how a GPU executor finds its
node's recipe, keyed that way because the FFI addresses nodes that way.

Lane-scoped categories get one instance per (node, lane); `PartitionAccumulator` and
`PartitionEmitter` get one per node, since they are the cross-lane points. A `BatchForwarder` is
neither — it has no backend and no executor, so the driver owns its rotation directly, and one
arm serves merge, union and interleave: a visit to output lane p cycles its source list in
order, forwarding one batch per visit, skipping empty sources and retiring finished ones.

### The scheduling rule

Two drivers, both single-threaded, push-based and deterministic. In `executor/driver/`,
`partitioned.rs` owns the tree, the queues and the three cross-lane categories;
`single_partition.rs` owns one lane of one lane-scoped node as a state machine; and
`scheduler.rs` decides what runs next from plain numbers, with no backend, batch or executor in
sight.

Every node carries a **height** (distance to the root) and an **order** (pre-order index). A
node is **runnable** when any of its lanes can make progress: a source always can, another node
once that lane's inputs hold a batch or are known finished. Among runnable nodes the driver
takes the smallest height, breaks ties leftmost, and runs **every lane** of that node.

The push behaviour falls out rather than being programmed. The moment a node produces a batch
its parent is runnable at a strictly lower height, so the batch is carried up before anything
below produces again; it stops only at an accumulator or the sink. With N lanes the unit that
moves is one batch per lane — running every lane of the chosen node is what keeps lanes
progressing together.

**Queues need no cap.** A producer's out-queue is drained by its parent before the producer runs
again, so queues are self-bounding at one batch per lane. One shape breaks that on its own:
**a join in its build phase holds back its whole probe subtree**, transitively, since without
the hold the probe side piles into a queue nothing will drain. It cannot deadlock — plans are
trees, so a join's build subtree is disjoint from its probe subtree and is never held by this
rule, and completing the build is what lifts the hold. Nested joins resolve outermost-first.

Livelock has the same answer: the one thing that can block a batch is a join waiting on its
other side, and orienting the tree so the build side is the left child removes the wait, because
at equal heights the leftmost node wins and the build subtree drains first.

Hash skew needs no mechanism. A lane that receives nothing is never runnable, and empty scatter
outputs are dropped at the emitter, so nothing empty traverses a chain.

The schedule is maintained incrementally: a rank order from (height, order) computed once, a
ready bitset over it, per-node ready-lane counters, and hold counters — counters rather than
flags, since one node can sit in two joins' probe subtrees. A pick is the lowest set bit. A step
re-checks the node that ran and its parent and nothing else, because nothing else can have
changed. A naive rescan survives as a test-only oracle compared pick by pick, since an
incremental schedule that disagrees with it is wrong by definition. A Python model of these
same rules — the scheduler, both drivers, the accountant, operators over pandas — is
`scripts/exec_model/`, which is where a rule is cheapest to argue with; build-test.md says what
it runs.

The unit that becomes ready is not always an output lane: a `PartitionAccumulator` has one
output lane and becomes ready one *input* lane at a time, and an emitter reads a single input
lane whatever it emits. Counting output lanes there gives a driver that never schedules a
merge's later lanes.

### Early exit at a limit

A `GpuUnload` carrying a root-adjacent interval is the one node the driver special-cases. Its
executor is per lane and the count is across lanes, so the count cannot live in the executor;
and nothing can signal "done" from below, because satisfaction is a fact about rows that have
already passed rather than about the next call.

The driver keeps one row count per such node, summed over every lane, and the node is satisfied
once it reaches `skip + fetch`; a pure offset has no such point and is never satisfied. A
satisfied node makes its **whole subtree** non-runnable, transitively, so the scan stops being
scheduled and pulls cease through merges and emits alike.

That is a join's hold with the direction reversed — a join's lifts when the build completes, a
limit's never lifts — so the two share a release path that drops every in-flight batch, which is
why a run can legitimately end with lanes not done and queues non-empty. A satisfied node is
marked done as it is held, so the hold cannot strand its parent; `LIMIT 0` is the case that
forces it.

### Memory accounting

Prevention at plan time, detection at run time.

**Plan time** is the estimator above, rendered per node in each plan golden's `--- memory ---`
section. Because `GpuMergePartitions` polls round-robin, all N lanes are live at once, so the
estimator charges the full multi-lane section — N × (per-lane executor state + one in-flight
batch) between the loader and the merge point.

**Run time** the driver keeps a running total incrementally — `resident` is Σ `byte_size` over
the driver-held in-flight batches plus Σ cached `resident_bytes()` over live executors.

Per call: pre-check `resident + scratch_bytes(rows, bytes)` against the budget; execute; remove
consumed inputs, add outputs at actual `byte_size()`, refresh that one executor's figure, and
post-check. The three calls that consume their executor skip the post-call read and forget the
slot instead, a consumed executor holding nothing. `CallStats.scratch_bytes` is measured — the
CPU directly, a device through RMM hooks — so model quality is observable and under-estimates
are recorded with their magnitude.

Four things in `executor/driver/accounting.rs` are load-bearing.

- **The executor total is a cache refreshed one slot at a time**, never a sum over live
  executors — which would force the accountant to hold references to executors the driver owns
  mutably.
- **A batch's size is read once, when it is held, and the same figure is released.** An Arrow
  batch recomputes its size by walking every array, so a second read is a second chance to
  disagree, and the total would drift with nothing going red.
- **Holds and releases are counted, not netted.** A total back at zero is also what releasing
  something never held would leave behind. This is the invariant on the early-exit path, where
  a satisfied limit ends the run with queues full and every one of those batches released.
- **The peak is an observation and the checks are the enforcement**, and they do not see the
  same total: the peak is raised where a batch enters a queue, while the post-check runs after
  the emitting executor's slot has been refreshed or forgotten. A buffering node holds its state
  and its output alive together for the length of one call — exactly what `cudf::concatenate`
  does — so its transient raises the peak and no check sees it. A budget below a reported peak
  can therefore complete: after the call, the state really is gone.

**What prices a transient is the pre-check, so pricing it is an obligation.** An accumulator's
`scratch_bytes` on its emitting call must include the output it is about to build; the model may
consult `&self`, which is what that permission is for. A model that returns zero there is not a
cheap call, it is a guard switched off, and it fails open.

**Model ≥ measured is not an invariant.** `scratch_bytes` rests on a cardinality figure for a
join and assumed selectivity for a filter, so it will sometimes come in low. The accountant's
contract is "fail cleanly when an accounted total at a check point exceeds budget", not "the
budget is never exceeded" and not "the peak stays under it".

Four rules were measured rather than designed, over the whole corpus under a 2 GiB accountant;
the cases are in [`archive/designs.md`](archive/designs.md).

- **`resident_bytes()` is a total for the accountant to check, never a numerator for a per-row
  cost** — only the executor knows which part scales with build rows. Dividing it mispriced one
  call at 2.0 TB and declined a query whose whole run peaked at 11.5 MB.
- **A build-preserving join's residency grows with the probe side**, since it holds key columns
  for every probe row seen, per lane. The CPU backend never pays it, so it cannot price it.
- **A memory bound asserted at one partitioning asserts about one shape of arrival**: only a
  streamed probe accumulates, and two corpus queries passed at one layout and failed at two.
- **Zero rows is not zero bytes, and a zero peak is a defect.** A batch of no rows still costs
  its schema; an empty lane emits no batch at all, which is a different thing.

### Determinism rules

Batch boundaries are a pure function of the plan: the loader's come from the committed mapping,
Exec ops are 1:1, accumulators emit at defined points. The remaining scheduling freedoms are
pinned.

- **Every `BatchForwarder` lane cycles its source list in order** — for `GpuMergePartitions`,
  round-robin over lanes by index, skipping the empty and retiring the finished. Chosen over
  draining lane by lane deliberately: it keeps "a lane makes progress alongside the others"
  true, at the cost of N live lanes, which the estimator charges and a parallel driver would
  cost anyway.
- **`cudf::merge` tie order**: input tables are passed lane-major, lane 0's batches in stream
  order, then lane 1's, regardless of arrival order.
- **No sort here preserves tie order.** DataFusion's `lexsort_to_indices` and cuDF's
  `sorted_order` are both unstable, so which of two tied rows an ordered `LIMIT` returns is
  decided by neither engine's contract. The accumulating sort still orders and slices rather
  than taking a top-N, because a heap's selection moves with arrival and a slice's does not.
- Order pinning is part of *result* determinism, not just golden stability: float aggregation
  sums in stream order, so an unpinned order changes low bits.
- **A root-adjacent limit counts across lanes**, so which rows an unordered `LIMIT` returns
  depends on the order batches reach the sink. Inserting a merge would not change that — it is
  round-robin, so it interleaves lanes too — and an ordered limit is unaffected, since a sort
  delivers one lane and one batch before the sink sees anything.

**These rules pin execution for a given plan, not across plans.** Two plans for one query may
legitimately return different rows where the SQL does not determine them, which is what an
unordered `LIMIT` is. Results are compared row-sorted, so emission order is not part of the
contract; what must hold is that one plan run twice gives one answer, byte for byte.

## The wire format

**The flat buffers** are the serialized plan (`flatbuffers/gpu_plan.fbs`) — the only thing
the C++ side ever sees of a query. Wherever this page says "the flat buffers", "the wire
format" or "serialized", that is what it means.

What crosses is not the plan tree. It is a menu of parameterized kernels whose nodes exist
to be addressed: the recipe writer (`wire/`) emits one node per call a driver will make, and
each node's recipe publishes the post-order sequence numbers its calls name. The vocabulary
is frozen: a kernel takes a whole input and answers with a whole table, and a driver that
wants less asks for it by calling more often rather than by changing the
node. [What the frozen surface costs](#what-the-frozen-surface-costs) is the bill.

Two spellings, and the prefix is the tell: `Cudf*` is a flat-buffer node table, the thing
the C++ dispatches on, with `GpuPlan` as the root table wrapping them. A `Gpu` name with no
`Cudf` is one of the engine's own plan nodes and never crosses.

Three of the fifteen wire kinds have no writer: `CudfCoalesceBatches` (batching is the engine's
own and needs no node), `CudfLimit` (a limit is a row range on the export) and `CudfWindow` (no
window function here yet, #143). They stay because the kernels behind them do.

**Statement order is the wire format**: FlatBufferBuilder is a no-interning bump arena, so
reordering writes changes bytes even with identical values, and
[`goldens/recipe-payloads.txt`](../testdata/goldens/recipe-payloads.txt) pins each
payload's bytes with a digest beside it. Regenerating it to silence a red defeats its purpose.

### From node to seqs

The mapping from a plan node to the seqs it addresses, and to the calls a driver makes:

| Node | seqs emitted | driven as |
|---|---|---|
| `GpuLoadParquet` | `CudfScan` | `execute_scan_rowgroups(seq, row_groups…)` once per batch, overriding the node's own list |
| `GpuFilter`, `GpuProject`, `GpuAggregate` | the same-kind node | generic map arm, one call per batch |
| `GpuSort` | `CudfSort` | map arm per batch; per-batch `fetch` for a top-N |
| `GpuAccumulateBatchesAndSort` | `CudfSort` + `CudfSortPreservingMerge` | per-batch sorts, then one merge call at done |
| `GpuMergeSortedPartitions` | `CudfSortPreservingMerge` | one merge call over all sorted handles, lane-major |
| `GpuCoalesceAllBatches` | `CudfCoalescePartitions` | one collapse call over the lane's batch handles |
| `GpuAggregateBatches` | `CudfCoalescePartitions` + `CudfAggregate{Merge}`, plus a `CudfProject` where it finalizes | one concat and one aggregate per compaction and again at done; the project runs once, at done |
| `GpuEmitPartitions` | `CudfRepartition(Hash, 1→N)` | repartition arm, one call per batch → N handles |
| `GpuHashJoin` | `CudfHashJoin`, plus the finish seqs — key project, concat, anti/semi join, pad project | map arm per (lane, probe batch); the build handle would need copying before each, since the call consumes it (#152) |
| `GpuCrossJoin`, `GpuNestedLoopJoin` | the same-kind node | one map-arm call |
| `GpuLimit` | none | `slice_handle` on the two straddling batches, nothing on the rest — the bounds are runtime values |
| `GpuMergePartitions`, `GpuUnion`, `GpuInterleave` | none, beyond the union's cast projects | routing in the driver, zero FFI calls |
| `GpuUnload` | none | `result_from_handle` per handle over the driver's row range; batches outside an interval are released without a call |

Three facts about the C++ side are what make this drivable. `execute_node` is stateless per seq
— the only state is the handle registry, inputs are consumed per call, outputs get fresh handles
— so calling one seq once per batch is legal. The collapse arm concatenates whatever k handles
it is passed, the merge arm merges any k>1 sorted handles, and the repartition arm scatters into
the plan-declared N; none of them cross-checks handle counts against the plan tree. And stats
come back per output handle per call, so a per-node figure is this side's fold over its calls.

**Every aggregate merges as state and finalizes in a project**, with no exception, so both
engines evaluate the same expression and agree by construction rather than by two
implementations happening to match. Two appended fbs values buy that: `UnaryOp.Sqrt`, so a
finalize can be written, and `AggregateMode.Merge`, so a merge can be only a merge — cuDF's
`MERGE_M2` is otherwise reachable only from an arm that finalizes on the same call, and these
plans stack two merges, per lane and then across lanes.

Three additive ABI symbols exist for what a frozen node cannot carry, since an fb node's fields
are plan constants and these are decided per call:

- `peacock_executor_execute_scan_rowgroups` — the scan arm otherwise emits every map entry in
  one call, so incremental loading is impossible.
- a row interval on `peacock_result_from_handle` — a root-adjacent limit would otherwise export
  whole batches and drop rows on the CPU, shipping an unbounded `skip` prefix over PCIe.
- `peacock_executor_slice_handle` — a mid-plan limit would otherwise hold every row ahead of the
  ones it wants, because frozen bounds are only correct against a table starting at row 0 of the
  stream. `OFFSET 1000000 LIMIT 10` would hold a million rows to return ten.

The two limit symbols are not interchangeable: one produces a result, the other a handle.

### From flat buffer to cuDF call

One row per wire node kind: what the plan hands the C++ side, and the cuDF it turns into.

The middle column is the fields that change what the call does — `input` / `left` / `right`
are the tree and are not repeated, and a field nothing reads is called out, because a wire
field with no consumer reads as a knob (#132).

| Node | What steers it | The cuDF it becomes |
|---|---|---|
| [`CudfScan`](../flatbuffers/gpu_plan.fbs) | `file_paths`, `projection`, `limit`, and the row groups — which every load supplies per call (`execute_scan_rowgroups`, how one node loads a batch at a time) rather than in the node, leaving `row_groups` and `batches[p]` read but unwritten; `batch_size` **is read by nobody** (#132) | [`scan.cpp`](../cpp/src/operators/scan.cpp) — `cudf::io::read_parquet(opts)`, with `.columns(projected)`, `set_row_groups(...)` and `set_num_rows(limit)` set on `opts` first |
| [`CudfFilter`](../flatbuffers/gpu_plan.fbs) | `predicate`, `projection` | [`filter.cpp`](../cpp/src/operators/filter.cpp) — `cudf::compute_column(tv, predicate)` for the mask, then `cudf::apply_boolean_mask(tv, mask->view())` |
| [`CudfProject`](../flatbuffers/gpu_plan.fbs) | `exprs`, `aliases` | [`project.cpp`](../cpp/src/operators/project.cpp) — `cudf::compute_column(tv, ast)` per AST-able expr; a bare `ColumnRef` is a column copy, and LIKE/CASE/scalar functions take `build_column` instead |
| [`CudfAggregate`](../flatbuffers/gpu_plan.fbs) | `mode` (Partial/Final/FinalPartitioned/Single/SinglePartitioned/Merge), `group_exprs`, `aggr_funcs` (each with its out decimal scale and `distinct`), `grouping_sets`, `mergeable_agg_state`, `aggr_input_schema` | [`aggregate.cpp`](../cpp/src/operators/aggregate.cpp) — `gb.aggregate(requests)` over [`groupby{keys, null_policy::INCLUDE}`](../cpp/src/operators/aggregate.cpp); with no group keys it is [`cudf::reduce`](../cpp/src/operators/aggregate.cpp) to one row |
| [`CudfHashJoin`](../flatbuffers/gpu_plan.fbs) | `join_type`, `keys`, `filter` + `filter_columns` (residual), `null_equals_null`, `projection` | [`join.cpp`](../cpp/src/operators/join.cpp) — `cudf::inner_join` / `left_join` / `full_join(left_keys, right_keys, kJoinNulls)`; semi/anti take [`left_semi_join` / `left_anti_join`](../cpp/src/operators/join.cpp), or their `mixed_*` forms when a residual filter must be evaluated during the join |
| [`CudfCrossJoin`](../flatbuffers/gpu_plan.fbs) | nothing — the node is its two inputs | [`join.cpp`](../cpp/src/operators/join.cpp) — `cudf::cross_join(ltv, rtv)` |
| [`CudfNestedLoopJoin`](../flatbuffers/gpu_plan.fbs) | `join_type`, `filter` + `filter_columns`, `projection` | [`join.cpp`](../cpp/src/operators/join.cpp) — `cudf::cross_join`, then [`apply_boolean_mask`](../cpp/src/operators/join.cpp) over the filter evaluated on the crossed table |
| [`CudfSort`](../flatbuffers/gpu_plan.fbs) | `exprs` (`asc`, `nulls_first` per key), `fetch`, `preserve_partitioning` | [`sort.cpp`](../cpp/src/operators/sort.cpp) — `cudf::sorted_order(keys, orders, null_orders)` then `cudf::gather`, and [`cudf::slice`](../cpp/src/operators/sort.cpp) when `fetch` makes it a top-N |
| [`CudfCoalesceBatches`](../flatbuffers/gpu_plan.fbs) | `target_batch_size` — **read by nobody** (#132) | [`dispatch.cpp`](../cpp/src/operators/dispatch.cpp) — `execute_passthrough`: the child's table, untouched. A GPU node is one materialized table, so there is no batching to do |
| [`CudfCoalescePartitions`](../flatbuffers/gpu_plan.fbs) | nothing | [`node_session.cpp`](../cpp/src/node_session.cpp) — `cudf::concatenate(views)` over the input partitions; a single input has nothing to collapse and passes through |
| [`CudfRepartition`](../flatbuffers/gpu_plan.fbs) | `kind`, `num_partitions`, `hash_exprs` (key ordinals) | [`node_session.cpp`](../cpp/src/node_session.cpp) — `spark_hash_partition(tv, key_cols, n)`, ours rather than cuDF's murmur3, then [`cudf::slice`](../cpp/src/node_session.cpp) per partition into an owning table |
| [`CudfSortPreservingMerge`](../flatbuffers/gpu_plan.fbs) | `exprs`, `fetch` | [`node_session.cpp`](../cpp/src/node_session.cpp) — `cudf::merge(views, key_cols, orders, null_orders)`, k-way and order-preserving; a concat fallback with no keys or one input (#118) |
| [`CudfUnion`](../flatbuffers/gpu_plan.fbs) | `inputs`, `interleave`, `output_schema` | [`union.cpp`](../cpp/src/operators/union.cpp) — `cudf::concatenate(views)`, after [`cudf::cast`](../cpp/src/operators/union.cpp) retypes each branch column to the declared output type (#41) |
| [`CudfLimit`](../flatbuffers/gpu_plan.fbs) | `skip`, `fetch` | [`limit.cpp`](../cpp/src/operators/limit.cpp) — `cudf::slice(tv, {skip, end})`, and the whole table returned untouched when the range covers it |
| [`CudfWindow`](../flatbuffers/gpu_plan.fbs) | `window_exprs` (partition keys, order keys, frame bounds, out decimal scale) | [`window.cpp`](../cpp/src/operators/window.cpp) — `cudf::grouped_rolling_window(keys, arg, preceding, following, min_periods, agg)`, which preserves input row order |

Two things recur. **A node handed one input reaches no kernel** where all it does is change
the layout rows sit in — one table has no layout to change — and **three nodes need more than
one call**, because cuDF has no fused form for filter's mask-then-apply, sort's
order-gather-slice, or union's cast-then-concatenate.

The nested-loop join is the one to read separately rather than filing beside filter. It
materialises the **full cartesian product** first and only then evaluates its predicate over
it, so it is three calls whose first is the expensive one — which is why broadcast joins
([#140](tickets.md#t140)) would change the shape rather than the constant.

### What the frozen surface costs

The wire vocabulary and the C++ operator set are kept as they are, which has a price. Each cost
below has a smallest unfreeze that removes it; deciding them together is
[#155](tickets.md#t155), because three of them are removed by more than one change.

| Cost | Why the surface causes it | The unfreeze | What that costs |
|---|---|---|---|
| **A build-side copy per probe batch** (#152) | `execute_node` erases the handles it reads and nothing duplicates one | [#145](tickets.md#t145): `TableResult` becomes a shared owner plus a view | no ABI change — a handle stays a `u64`; 35 call sites across 11 files |
| **A probe-batch copy on Left/Full** (#152) | two consumers, one handle: the join needs the batch and the key project needs it too | the same refcount, or a node allowed to return its input beside its output | the second form is an fbs *semantics* change with no ABI change |
| **The build side re-hashed per probe batch** (#136) | `CudfHashJoin` is stateless per call, so B batches means B builds; refcounting removes the copy and not this | a join session: begin, probe, finish, holding one `cudf::hash_join` | three symbols and session state keyed by id — the largest change here |
| **Probe keys held resident, plus an extra join at finish** (#136) | "which build rows matched" cannot cross an ABI that returns a table and row counts | a match bitmap out-param, or the join session above | the bitmap is a one-argument delta; either deletes the key accumulation and the finish join |
| **A new symbol per runtime-varying parameter** | an fb node's fields are plan constants, so anything decided per call cannot ride the node | per-call overrides: one `execute_node` variant taking an override struct | one symbol instead of three, and the next such field costs nothing |

**Three refusals are a different kind of cost**: nothing on the surface makes a table out of
nothing. A collapse of no handles, a merge of no runs and a finish whose probe produced no keys
refuse by name ([#173](tickets.md#t173)); a Right, Full or RightAnti lane whose build side was
empty owes its probe rows padded and cannot make them ([#175](tickets.md#t175)); and
`PlaceholderRowExec` is a table of literals with no input at all ([#158](tickets.md#t158)).

The unfreeze is one call — a table of a schema and a literal row count. What makes it worth
deciding rather than deferring is that the CPU answers all three, so each is a shape where the
oracle disagrees with the engine it is checking.

**The ABI is already more general than the node semantics.** `execute_node` writes into
`out_handles` with an `out_cap` and an `out_count`, so k outputs are expressible today and the
repartition arm uses that. What forbids a node from emitting two things is the fbs vocabulary,
where every node kind means one output — so "return the input beside the output" is an fbs
semantics change with no ABI change at all.

**One cost is not about the surface.** Every operator exit path deep-copies its columns into a
fresh table where a move would do ([#154](tickets.md#t154)) — for a join with a projection,
twice over. An engine running a node once per query would pay that once per node; this one pays
it once per node per *batch*, which is what makes it worth a ticket.

## Interfaces

The **public C++ surface is two headers**: [`peacock_gpu.h`](../cpp/include/peacock_gpu.h)
and [`partitioning.hpp`](../cpp/include/peacock/partitioning.hpp). Everything under
`cpp/src/peacock/` is private to the library, with `plan_executor_internal.h` alongside for
what the tests reach into. Items called *de facto* below have no trait or abstract base, yet
other code is written against them, so changing one breaks a caller that never named it. The
Rust side's own traits — `Backend`, the executor families, `GpuNode` — are in
[Execution](#traits) above, beside the reasons for their shape.

The ABI is seventeen symbols in five groups: lifecycle (`peacock_gpu_version`,
`peacock_executor_create` / `_destroy`, `peacock_last_error`, `peacock_result_free`); the
node-by-node session (`begin_plan`, `execute_node`, `handle_release`, `end_plan`); the three
per-call entry points (`execute_scan_rowgroups`, `slice_handle`, `result_from_handle`);
instrumentation (`install_rmm_pool`, `set_node_timing`, `measure_timing_floor_us`); and two
test hooks: `peacock_spark_partition_ids`, which runs the murmur3 kernel over one Arrow C-data
batch so the Rust side can compare it against comet's, and `peacock_handle_from_arrow`, which
adopts one such batch into the live session as a handle so the operator harness can hand an
executor a table it wrote.

Three conventions the signatures do not carry:

- **A row range is `[offset, offset+length)`**, `UINT64_MAX` meaning to the end, an offset
  past the end empty, an overrun clamped. It is the same convention on `slice_handle` and
  `result_from_handle`, which are otherwise the two halves of the limit rule — one produces
  a handle, the other a result.
- **Instrumentation is process-global, off by default, and nothing in this workspace turns
  any of it on.** Without the pool every cuDF intermediate is a `cudaMalloc`/`cudaFree` round
  trip ([#148](tickets.md#t148)); the gtest binaries install it from their own `main()`, and
  this symbol exists for a Rust caller that cannot include the C++ header. Node timing makes
  `execute_node` synchronize the default stream at every measurement boundary, which is what
  makes `time_us` execution rather than kernel submission and also what serializes what cuDF
  would otherwise pipeline. The floor is what an empty timed region costs, and it is never
  subtracted: a node at or below it is unresolved, not cheap.
- **`peacock_executor_create` takes a byte limit it does not enforce.** Residency is the
  Rust driver's accounting; see [Memory accounting](#memory-accounting).

**[`NodeSession`](../cpp/src/plan_executor.h)** — *de facto*, and what the node-by-node FFI
entry points are thin wrappers over. Nodes are addressed by canonical post-order sequence,
the same order the recipe writer numbers in, so child handles align across the boundary;
input handles are consumed and `out_stats` is filled per output partition. PIMPL, so the
header exposes no cuDF internals; the handle registry lives in that `Impl` and is not a type
of its own (below).

**[`TableResult` / `NodeStats`](../cpp/src/plan_executor.h)** — the two value types every C++
path returns. `NodeStats` carries only what C++ alone can measure — rows, var-length content
bytes, and a time that is zero unless timing is on. The byte formula itself lives in Rust
(`src/common.rs`) so the two engines cannot drift.

**[`NodeInputs` and the operator dispatch](../cpp/src/peacock/operators.h)** — the contract
every operator translation unit shares: one `execute_*` per wire node kind, plus `take_input`
to resolve a child and `execute_one` to run a node over inputs given by value. `NodeInputs`
is a parameter rather than a thread-local, and that is deliberate: an anonymous-namespace
thread-local forks when the file is split, so one half would read the other's inputs
(coding-style.md carries the case).

**[`peacock::partitioning`](../cpp/include/peacock/partitioning.hpp)** — the second public
header: `spark_partition_ids` and `spark_hash_partition`, our own bit-exact Spark-murmur3 at
seed 42, because cuDF ships only standard murmur3. Both take the stream and memory resource
as trailing defaults, which is what makes them usable off device 0.

**[`ExprContext`](../cpp/src/peacock/expr.h)** — *de facto*. cuDF AST nodes hold references,
so something must own every sub-expression for the lifetime of the call; that ownership IS
the interface. `build_expr` takes an optional column map, which is how a mixed join's filter
ordinals are remapped onto its LEFT and RIGHT tables.

**[`GpuWorker` / `WorkerPool`](../cpp/tests/gpu/multi_gpu.hpp)** — *de facto*, test-only, and
the one place the multi-GPU rules are encoded as a type: a cuDF or cuVS object must be
destroyed on its owning device's thread, so every device gets a worker thread and a
persistent stream, and work reaches a device only by `submit`.

### The handle registry has no type

The C++ side keeps intermediates alive behind opaque `u64` handles, and that is not a class.
It is two fields inside the private [`NodeSession::Impl`](../cpp/src/node_session.cpp) — an
`unordered_map<uint64_t, TableResult>` and a monotonic `next_handle` — with allocation,
lookup, consume-on-read and erase written inline at every site that touches them.

So the consume-once rule the FFI documents ("input handles are CONSUMED") holds by convention
at each site rather than by construction, and only at run time: reading an already-consumed
handle throws `unknown input handle`, and `execute_one` throws when a node consumes a different
number of inputs than it was given.

A `HandleRegistry` with `insert` / `take` / `borrow` would put the rule in one place and make
double-consumption unrepresentable rather than merely detected. Nothing needs it yet, but no
type is holding this together.

## Rehash and the comet hash

A shuffle is `GpuEmitPartitions`, and both backends run it, so which lane a row lands in has to
be the same number on each. The hash is **Spark's murmur3 as implemented by comet**, seed 42,
on both. Neither available implementation would do: DataFusion's repartition uses ahash, and
cuDF exposes standard murmur3, which differs from Spark's spec in multi-column combine and null
handling.

So placement is identical by construction rather than by agreement. The CPU side calls comet's
`create_murmur3_hashes` (`executor/cpu_backend/spark_partitioning.rs`), the GPU side owns a
bit-exact kernel (`spark_hash_partition.cu`) and reuses cuDF only for the scatter, and a live
gate (`peacock_spark_partition_ids`, `cpu_backend/gpu_tests/murmur_conformance.rs`) proves the two agree over the
same bytes.

## C++ executor layout

`cpp/src/`: `gpu_executor.cpp` (the C FFI), `node_session.cpp` (the post-order index, the
handle registry, and the multi-partition dispatch — scan-map emission, collapse, k-way merge,
hash repartition, 1:1 map), `expr.cpp` (expression and AST building),
`spark_hash_partition.cu` (the murmur3 kernel), and `operators/` (one `execute_*` per wire
node kind plus `dispatch.cpp` with the `run_op` switch).

`execute_one` enforces **consumed == provided**: a node handed inputs must consume all of
them, or it ran against inputs the caller did not give it. That check is what makes the
positional `NodeInputs` contract safe, since nothing else names which child a `take_input`
resolves.

## Column indexing

Nothing in the flat buffers names a column to read. Every reference is an ordinal into the
child's output table, so a node's correctness depends on the child having produced its
columns in exactly the order the planner assumed.

Where the ordinals come from and where they land:

| Reference | Written by | Read by |
|---|---|---|
| `ColumnRef.index` in any expression | [`expr_writer.rs`](../peacockdb-core/src/wire/expr_writer.rs), off the ordinal `planner/translator/expr.rs` read from DataFusion's `Column::index()` | [`build_expr`](../cpp/src/expr.cpp) for the AST path, [`build_column`](../cpp/src/expr.cpp) for the column path |
| `projection` index lists on filter and join | [`node_writer.rs`](../peacockdb-core/src/wire/node_writer.rs), [`join.rs`](../peacockdb-core/src/wire/join.rs) | [`filter.cpp`](../cpp/src/operators/filter.cpp), [`join.cpp`](../cpp/src/operators/join.cpp) — gather by ordinal, and the name list is indexed with the same ordinal |
| join key pairs, `on=[(l@0, r@0)]` | [`join.rs`](../peacockdb-core/src/wire/join.rs) | [`join.cpp`](../cpp/src/operators/join.cpp) — ColumnRef only, anything else throws |
| `JoinFilterColumn{side, index}` | [`join.rs`](../peacockdb-core/src/wire/join.rs) | [`expr.cpp`](../cpp/src/expr.cpp) — remaps a filter-schema ordinal onto the mixed join's LEFT/RIGHT tables |
| sort keys, hash keys, group keys | [`node_writer.rs`](../peacockdb-core/src/wire/node_writer.rs), [`aggregate_writer.rs`](../peacockdb-core/src/wire/aggregate_writer.rs) | [`sort.cpp`](../cpp/src/operators/sort.cpp), [`node_session.cpp`](../cpp/src/node_session.cpp), [`aggregate.cpp`](../cpp/src/operators/aggregate.cpp) |

`cpp/src/` holds 22 `->index()` reads and 48 `.column(…)` calls, so this is the engine's most
common operation and the one with the least ceremony around it.

### What guards it, and what does not

One real backstop, and it is not ours: **cuDF bounds-checks column access**.
`table_view::column(i)` is `_columns.at(i)`, so an out-of-range ordinal throws rather than
reading garbage, and the FFI surfaces the message. What it is not is *informative* — the
message is `vector::at` boilerplate with no node, operator or ordinal, because the check sits
three layers below the code that had the context. Two explicit checks in `expr.cpp` do better
and worse: the column path throws with the ordinal and the column count, while the
type-inference helper returns `type_id::EMPTY` and turns a bad ordinal into an unhelpful type
error further along. One arity check, on the Final-stage aggregate, catches a state width that
disagrees with the arity expected — width, not order. The FlatBuffers verifier checks that
offsets and vectors are well formed and has no idea what an ordinal means.

Two things nothing guards, and [#164](tickets.md#t164) carries the fixes.

**Column names are a parallel array with no invariant.** `TableResult` is a `cudf::table` plus
a `std::vector<std::string>` with no assertion that the two are the same length, and the six
sites indexing names use `operator[]` — so a short names vector is undefined behaviour rather
than an exception. `filter.cpp` reads the checked `column(idx)` and the unchecked
`column_names[idx]` in one loop iteration, and the checked read happening first is luck.

**Nothing checks that a child's column *order* is what the plan assumed.** The per-node golden
records the node line, its lane and batch lists, rows and bytes — not the column list and not
the types, and the bytes cannot help because both engines compute them from the plan's schema
precisely so they cannot drift. So a node emitting the right columns in the wrong order gives
identical per-node numbers on both engines, and the divergence surfaces only at the root, for
a query whose corpus line names a result golden or an oracle.

## cuDF options

cuDF's defaults are not SQL's, and they are not DataFusion's. Every option below is a place
where taking the default would produce a plausible wrong answer rather than an error, so each
one is either set explicitly or carried in the flat buffers — and the ones carried in the flat buffers are carried
precisely so cuDF cannot infer something the CPU side did not.

| Option | Set at | Value | What the default would do |
|---|---|---|---|
| `parquet_reader_options` | [`scan.cpp`](../cpp/src/operators/scan.cpp) | `.columns(projected)`, `set_row_groups(map ∥ pruned)`, `set_num_rows(limit)` | read every column and every row group; the row-group list is also how a partition reads only its own slice |
| `cudf::order`, `cudf::null_order` | [`sort.cpp`](../cpp/src/operators/sort.cpp), [`node_session.cpp`](../cpp/src/node_session.cpp) | per key from the flat buffers's `asc` / `nulls_first` | cuDF has no notion of the query's ORDER BY; the two sites must agree or a k-way merge would order differently from a sort |
| `cudf::null_equality` | [`join.cpp`](../cpp/src/operators/join.cpp) ×9 | see the table below | `EQUAL` — NULL keys match, inventing rows SQL excludes |
| `cudf::out_of_bounds_policy` | [`join.cpp`](../cpp/src/operators/join.cpp) | `NULLIFY` on the side that can be unmatched, `DONT_CHECK` otherwise | `DONT_CHECK` reads the `JoinNoneValue` sentinel (`INT32_MIN`) as an index and faults with `cudaErrorIllegalAddress` |
| `cudf::null_policy` (groupby) | [`aggregate.cpp`](../cpp/src/operators/aggregate.cpp), [grouping sets](../cpp/src/operators/aggregate.cpp) | `INCLUDE` | `EXCLUDE` silently drops the NULL group — tpcds q15's NULL `ca_zip` row disappears |
| `cudf::null_policy` (rolling count) | [`window.cpp`](../cpp/src/operators/window.cpp) | `EXCLUDE` for `COUNT(col)`, `INCLUDE` for `COUNT(*)` | one of the two is always wrong: `COUNT(*)` counts rows, `COUNT(col)` counts non-nulls |
| decimal scale | [`aggregate.cpp`](../cpp/src/operators/aggregate.cpp), [`union.cpp`](../cpp/src/operators/union.cpp), [`window.cpp`](../cpp/src/operators/window.cpp) | `data_type{id, -out_decimal_scale}` from the flat buffers | cuDF would re-derive a scale per operation and drift from DataFusion's |
| binary-op output type | [`expr.cpp`](../cpp/src/expr.cpp) | boolean for predicates, else the wider input; division pre-scales the numerator to hit the flat buffers's `out_decimal_precision/scale` | cuDF promotes by its own rule, which is not SQL's decimal arithmetic |
| hash seed / algorithm | [`spark_hash_partition.cu`](../cpp/src/spark_hash_partition.cu) | our own Spark-murmur3, seed 42, cuDF only for the scatter | cuDF ships standard murmur3, whose partition numbers differ from comet's — see [Rehash and the comet hash](#rehash-and-the-comet-hash) |
| IPC export | [`gpu_executor.cpp`](../cpp/src/gpu_executor.cpp) | column names as `column_metadata`; DECIMAL32/64 cast up to DECIMAL128 | unnamed columns, and narrow decimals that the Rust arrow-ipc reader rejects outright |
| stream + memory resource | everywhere in the single-GPU path | `cudf::get_default_stream()`, current device resource | fine on device 0 and wrong anywhere else — the multi-GPU rules are in [Multi-GPU notes](#multi-gpu-notes-cudf-2602) |

### What the Rust side puts in the flat buffers

Half of the table above is not a choice the C++ side makes — it reads a value the planner
already computed and the recipe writer wrote down. That is deliberate: an option carried in
the flat buffers cannot be re-derived differently by the two engines, so anything where
cuDF's own inference could drift from DataFusion's is serialized rather than inferred. The
writers are all under [`wire/`](../peacockdb-core/src/wire/), so the paths below are relative
to it.

| Flat-buffer field | Written by | Taken from | Becomes |
|---|---|---|---|
| `CudfHashJoin.null_equals_null` | `join.rs` | the node's own flag, which the planner set from `HashJoinExec::null_equals_null()` | `cudf::null_equality` (except anti/mark, below) |
| `JoinFilterColumn{side, index}` | `join.rs` | the join filter's `ColumnIndex` list | `cudf::ast::`<br>`table_reference::LEFT` / `RIGHT`,<br>plus an ordinal |
| `SortExpr.asc`, `.nulls_first` | `node_writer.rs` | the node's sort keys, from `PhysicalSortExpr::options` | `cudf::order`,<br>`cudf::null_order` |
| `CudfSort.fetch`,<br>`CudfSortPreservingMerge.fetch` | `node_writer.rs` | the node's `fetch`, `-1` where there is none | a post-sort / post-merge slice |
| `BinaryExpr`<br>`.out_decimal_precision/scale` | `expr_writer.rs` | the expression's declared output type | the binop output type, and division pre-scales to hit it |
| `CudfAggregate.mode` | `aggregate_writer.rs` | the phase: `Partial` builds state from values, `Merge` merges state into state. Never `Final`, which would also finalize, and a finalize here is a project both engines evaluate | which cuDF aggregation runs, whether state columns are merged, and whether the result is state or a value |
| `CudfRepartition.hash_exprs`,<br>`num_partitions` | `node_writer.rs` | the emit node's keys and lane count | key ordinals and N for<br>`spark_hash_partition` |
| `CudfScan.limit` | `node_writer.rs` | the source's pushed-down limit | `parquet_reader_options::set_num_rows` |
| `AggregateFuncNode`<br>`.out_decimal_precision/scale` | `aggregate_writer.rs`, at zero | nothing: decomposition means no `avg` reaches a device, so the scale rides the finalize divide's own pair | **nothing** here, deliberately, and the writer says why |
| `CudfScan.batch_size`,<br>`CudfCoalesceBatches`<br>`.target_batch_size` | nobody | — | **nothing** — no C++ code reads either (#132) |
| `CudfScan.row_groups`,<br>`.batches` | nobody | — | `set_row_groups`, but no plan reaches it: every load names its own groups per call |

Three shapes are worth separating here. Most rows carry a value the GPU must not recompute —
decimal scales above all, since cuDF derives its own per operation and DataFusion's is what
the result is compared against. The last three rows are different: a field left at its
default on purpose; a pair no writer sets and no reader reads, which is the wire-format
surface #132 is about; and a pair the C++ still reads that no plan fills, because the row
groups a batch loads are a per-call value and ride the call instead.

### Join types and NULL key equality

`null_equals_null` travels in the flat buffers per join, mirroring DataFusion: `false` (the SQL default)
means a NULL key matches nothing, `true` means NULL = NULL, which is what a set operation
lowered to a join needs. Whether a join type actually honours it is the interesting part.

| Join type | cuDF call | `null_equality` |
|---|---|---|
| Inner | `inner_join` | from the flat buffers |
| Left | `left_join` | from the flat buffers |
| Full | `full_join` | from the flat buffers |
| Right | `left_join` with sides swapped, indices swapped back | from the flat buffers |
| LeftSemi | `left_semi_join`, `filtered_join::semi_join`, or `mixed_left_semi_join` with a residual filter | from the flat buffers |
| RightSemi | the same, sides swapped; a residual filter is rejected | from the flat buffers |
| LeftAnti | `left_anti_join`, `filtered_join::anti_join`, or `mixed_left_anti_join` | **hardcoded `EQUAL`** |
| RightAnti | the same, sides swapped | **hardcoded `EQUAL`** |
| LeftMark | `left_semi_join`-shaped, emitting one row per left row plus a boolean mark | **hardcoded `EQUAL`** |
| Inner / Left, non-equi | `conditional_inner_join` / `conditional_left_join`, or an AST boolean mask | n/a — the predicate decides |

Three things that table is worth reading for.

**Semi honours the flag and anti does not**, deliberately. `x IN (…)` and `EXISTS` are ordinary
three-valued predicates, so `UNEQUAL` is right and tpcds q33 needs it; a set operation lowered
to a semi join asks for `EQUAL` and gets it (q14). Anti is not symmetric: `x NOT IN (…, NULL)`
is never true for any x, which is neither `EQUAL` nor `UNEQUAL` — no cuDF setting implements
it, so anti and mark stay `EQUAL` until the planner distinguishes `NOT IN` from `NOT EXISTS`
(#80, #59).

**The equi-join default is the one that bites silently.** cuDF's `EQUAL` invents rows the SQL
oracle excludes, and the symptom is not an error but a count or sum one too large — tpcds q50,
q6 and q81 each inflated a downstream aggregate before `join_nulls` was threaded through.

**A residual filter is not optional on semi/anti.** The key-only cuDF calls ignore it, so a
LeftAnti on the key alone collapses to zero rows; those joins must take the `mixed_*` variants
that evaluate the AST during the join (TPC-H q21 is the case). RightSemi and RightAnti reject
a filter outright, because no swapped `mixed_*` variant exists.

## Node display

**There are two node lines, and the difference is which golden it is in.** A plan line is
`<Name>: <node fields>, lanes=N, batches=single|multiple[, hashed_on=…][, sorted_on=…],
schema=[name:type, …]`. An execution line drops the schema, adds `output_rows` and
`output_bytes`, and carries a second line beneath it — `in_rows` nested by child then by that
child's lane, `batch_rows` and `batch_bytes` by this node's lane then by batch, plus
`abandoned` where a run left something behind. Indentation draws the tree in both. The files
themselves are in [build-test.md](build-test.md).

**Every column reference renders `name@ordinal`.** The ordinal is authoritative and the name
comes from the declared schema at that position, so a reader can follow a reference without
holding the child's column order in their head, and a name that disagrees with its ordinal is a
visible defect rather than an invisible one.

**Layout replaces the lane count.** `lanes=N, batches=…` on every node, plus `hashed_on=[…]` and
`sorted_on=[…]` where the layout carries them — those two print only when specified, so their
absence reads as the fact it is. What a parent may assume is exactly this, so the line says it.

**Every node carrying a `fetch` prints it.** A merge that turns 80 rows into 10 says so on its
own line rather than leaving the number to be inferred from the sort beneath it. Same for the
aggregate's `aggs` and `final` lists, the loader's `partition_groups`, and a limit's interval
wherever it lives. A source renders its whole mapping as one nested structure —
`partition_groups=[[[0,1],[2,3]],[[4],[5,6,7]]]`, lanes outermost, batches within them, row
groups innermost — verbatim what the partitioner returned, because which batch sits in which
lane is the property worth reading and a lane count beside a batch count does not carry it.

**Types are a plan fact.** The declared schema per node is what makes the explicit casts
legible: a `Decimal128(38, 6)` in a finalize means nothing without the state column's declared
scale beside it. It checks nothing — a golden records what the planner declared, and the
declaration is exactly what a wrong type would move. Comparing a declared type against the
expression that produces it is [#163](tickets.md#t163), and the C++ half is
[#164](tickets.md#t164).

**Estimates go in a `--- memory ---` section per query, not on the node line.** They churn where
plan shapes do not — an estimator change, then #19's statistics, then #147's refinement — so on
the node line every such change would rewrite every line and a reader could not tell a shape
change from a number. In their own section the tree stays byte-identical and the diff says which
it was.

## Multi-GPU notes (cuDF ≥26.02)

Hard-won constraints for the multi-GPU C++ path, which lives entirely in
`cpp/tests/gpu/test_multi_gpu_*` — no engine path reaches a second device today.

- Every cudf op on a worker pinned to GPU≠0 needs a **device-local stream**;
  `cudf::get_default_stream()` is device-0-bound ("invalid device ordinal" otherwise).
- A cudf/cuVS device object must be **destroyed on its owning device's worker thread** — hence
  the worker-per-GPU pool; release partitions and results on-worker before teardown.
- Per-device **RMM pools** with persistent per-worker streams are what make cheap queries
  scale. Pool dealloc is stream-ordered, so an object outliving a transient stream frees on a
  dead stream and crashes; and `set_per_device_resource(g, nullptr)` resets the pointer map but
  not the ref map, so teardown must also call `reset_per_device_resource_ref(g)`.
- Benchmarking several queries in one process is flaky at G≥2 (process-global cudf stream state
  across WorkerPool teardowns) — one query per process (see build-test.md).

## Cost model and the DuckDB oracle

Both numbers the widget compares are bytes, and the point of the pairing is that they are the
same bytes: how much data the query had to move. build-test.md has how each file is produced.

**Peacock cost** is a re-reading of the execution golden rather than a second measurement:
each node's `output_bytes` is binned into a category and multiplied by that category's weight
from `testdata/cost_model.conf`. Every real category is 1.0 today, so the total is Σ
`output_bytes`; the weights exist so a phase can be priced without moving the goldens that
record it. Three placeholder phases sit at 0.0 and one category names no node at all — kept
because dropping a category rewrites the line list of every committed cost golden.

**The DuckDB oracle** runs each query twice — a deterministic profile with join-filter
pushdown off, then a pass reading only the dynamic-filter bounds — and combines them with
parquet row-group statistics. `storage_read_total` is deliberately in the same units as a
source node's `output_bytes`, decoded Arrow bytes of the surviving row groups' referenced
columns, which is what makes the ratio apples-to-apples rather than a scan count against a
byte count.

**The widget** takes the query's section at the last mode its cpu run is enabled at, and
renders green at a ratio ≤ 1.4. Directional signal, not a benchmark: nothing here is timed.
