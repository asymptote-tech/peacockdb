# Archived task specs

Specs for tasks whose PR has merged, newest first. Each is the contract the work was
done against, kept verbatim -- including the amendments and corrections made mid-task,
since those are the part a later reader cannot reconstruct from the diff.

The batch-partitioned rollout's task list is the first entry below, archived whole on
2026-09-08: T0 through T19 and T21 are done, T20 is now [#195](../tickets.md#t195) and T22 is
obsolete. The design it was the plan for was folded into
[`architecture.md`](../architecture.md) the same day, which is where the shapes these tasks
built are described; this list is what a number in a commit message resolves to.

The entry after it merged 2026-08-20 as PR #126, opened against ENS-bp-plan-skeleton and
retargeted to master when that base merged.



---

<!-- archived from llm-wiki/tasks/drop-legacy-modes.md -->

# Drop the legacy execution modes

Delete the six legacy execution modes — CPU full-table at tp1 and tp8, CPU partitioned,
GPU all-at-once, GPU full-table, GPU partitioned — with the planning they rest on, the
tests, the goldens, the benchmark records and the widget columns that describe them. What
stays is the batch-partitioned planner and its two backends.

## What goes

- **Rust**: `executors/` (five mode classes, the node-by-node driver, the streaming
  driver, the resident enforcer), `operators/` (the 16 `Gpu*Exec` wrappers and their
  serializers), `gpu_rule.rs` (both physical optimizer rules), `plan_serializer.rs`,
  `resident.rs`, `cpu_executor.rs`, `gpu_executor.rs`, `node_executor.rs`, and
  `CpuExecutor` in `lib.rs`.
- **C++**: `execute_plan.cpp` and the `peacock_execute` ABI entry point. `execute_node`
  stops being a recursive driver and becomes what it always was on the node path: the
  resolver that hands an operator its next already-resident input.
- **Tests**: the fourteen legacy targets and the harness that only they used
  (`common/exec_mode.rs`, `common/benchmark.rs`, `common/gpu_cases.inc`).
- **Goldens**: every `.plan.txt`, every `<query>.<mode>-<tp>-<tier>.{cpu,cost,result}.txt`
  and `plan_bytes.sha256`; `testdata/benchmark-results/` with the harness that wrote it.
- **Widget**: the legacy table, the six legacy registry columns, and the second PR
  comment the four tables needed.

## What is kept, and where it moved

The batch-partitioned side reached into the legacy tree in four places, so each moves
rather than dies: the per-node DataFusion runner (`cpu_backend/single_node.rs`), the three
Arrow-to-wire helpers the recipe writers share (`recipe/wire.rs`), `parquet_table_name`
(`parquet_meta.rs`), and the small-table threshold, which two test files spelled
separately and is now `plan::SMALL_TABLE_BYTES`.

## Coverage this removes and does not replace

Two guards were defined as "the recipe writer against the legacy writer" and cannot
survive it: `every_field_the_legacy_writer_sets_is_set_here_or_declared_a_difference` and
the seven expression cases in `recipe/expr_writer/tests.rs`. What they added over the rest
was a second, independent producer of the same bytes; the payload digest still pins what
this writer writes. Say so in the PR rather than quietly.

The C++ operator gtests keep their coverage: `test_plan_executor.cpp` drives its
hand-built plans through `NodeSession` node by node, the way the driver does.

## Verification bar

CPU: the rust-only suite and `ctest -L cpu` locally, `cargo test -p cost-report`.
GPU: the five staged targets on shad-gpu. Both green before the PR.

**Merged 2026-09-08 as PR #140**, three commits against master. What the review added beyond
the spec: thirteen loose root files had ridden into the deletion commit and the repo root is
now ignored by extension; the free `execute_node` became `take_input`, since it executes
nothing and shared a name with the one that does; the CLI was rewritten onto the planner and
nothing compiled it, so CI builds it and `test_ci_coverage` guards that. The GPU job's rust
loop was found printing nothing and guarding nothing — its `cat` and `grep` segfaulted under
an exported glibc — which is where the per-command library path, the crash detector and the
single-GPU concurrency group came from.

---

<!-- archived from llm-wiki/tasks/batch_partitioned_executor.md -->

# batch-partitioned executor: the implementation plan (T0–T22)

**Closed 2026-09-08, and archived whole.** Every task here is done except two: T20 became
[#195](../tickets.md#t195) and T22 is obsolete as written (see its entry). It is kept verbatim
because commits, reviews and the archived specs below name these numbers, and this is where a
reader resolves them; what the tasks built is described in
[`architecture.md`](../architecture.md).

Tasks in dependency order, and the numbers now ascend with it. T13 is the one that does not:
it landed early, because both drivers over a mock backend needed none of T9–T12, and it keeps
its number because commits and reviews already name it. T21 sits out of order for the same
reason — it was split off T14 after it had been narrowed — and the tail of the list runs
T20, T22 because 21 is spent. T11 and T12 were retired in the same
renumbering — their work is T15 and T16 — so a number is never reused and an older reference
still resolves. Each task is one developer hand-off with its own proving tests.
Legacy tests stay green throughout — every task that touches shared code runs the
affected legacy subsets (one query per mode/tier per binary plus the rust-only tier, per
build-test.md).

~~**T0 — Python prototype of the whole execution model**~~ (done). All node types and both drivers
in Python, operators built with pandas, plans hand-built (no DataFusion, no planner) — an
emulation of tree execution whose purpose is to settle the push model before any Rust
exists. Lives in [`scripts/exec_model/`](../../scripts/exec_model/README.md); its tests run
in CI (cost-report, plus the TPC-H set in dataset-matrix, which has the generated sf1).

Done — struck through, and folded into this document where it changed a decision:

- ~~the trait set, both drivers, and the memory enforcer with the accounting formula~~;
- ~~the scheduling rule~~ — height, order, min-height-first with leftmost ties, every lane
  of the chosen node; the Drivers section is rewritten from it;
- ~~the backpressure rules~~ — a join in its build phase holds its whole probe subtree; a
  satisfied limit holds its whole subtree for good. Both were findings, not designs;
- ~~queues need no cap~~ and ~~`Pending` does not exist~~ — the draft's two flow-control
  mechanisms, both dropped, both because runnability is a predicate evaluated before the
  call;
- ~~pandas-backed operators~~ — filter, project, sort, the aggregate sequence with its
  partial/final decomposition, the accumulators, the hash scatter, the join capability
  matrix, and the T2 row-group partitioning policy, each written against the pandas/cuDF
  intersection with the divergences named;
- ~~every query checked against a single-shot oracle at five partitioning configs~~, the
  prototype's version of two-engine correctness;
- ~~both limit lowerings~~, as the limit rule now states them. One finding survives here:
  the tests must assert on the *calls*, since only those distinguish a limit from a filter
  applied after the transfer;
- ~~the stress surface~~ — a plan rewriter (`operators/injection.py`) rather than
  hand-written variants: one plan re-run at every partitioning, batch size, empty-lane and
  hash-placement preset, with `GpuCoalesceBatches[target]` injected above every source
  (#139's node, proving the drivers tolerate it anywhere) and sources emitting zero-row
  batches at a set probability. It carries one rule the planner's tests should quote: a
  join may be re-partitioned only when both sides are hash-partitioned on the join keys,
  since otherwise its lane count is load-bearing and splitting it joins matching slices;
- ~~empty partitions, empty batches, skewed hashes, the flow-and-backpressure surface,
  determinism (two runs, identical batch traces)~~;
- ~~validation scope~~ — partitioning and `SingleBatch` constraints in scope, schema checks
  not.

- ~~the hand-built plan corpus~~ — 22 TPC-H and 71 TPC-DS query texts rather than the
  3–4 and ~10 the plan asked for, each at three layouts and on both join backends. It was
  the piece most likely to find something and it did: [what the corpus rollout
  measured](#what-the-corpus-rollout-measured) is the section it produced, and every item
  there is a property of the design rather than of the prototype.

Closed without the **estimator** (`estimated_max_resident_size`, `target_batch_bytes`).
The prototype models scratch per executor and never derived batch sizes from a budget,
and T6 derives both in Rust directly — a prototype estimator would be a second model to
keep true against the one that ships. The corpus is what T6 will calibrate against.

~~**T1 — flatbuffer operation-name refactor**~~ (done). Nine of the fifteen legacy node-kind names
(`GpuFilter`, `GpuProject`, `GpuSort`, `GpuAggregate`, `GpuCrossJoin`,
`GpuNestedLoopJoin`, `GpuUnion`, `GpuLimit`, `GpuCoalesceBatches`) collide with the new
mode's node names. Rename the fbs tables and `PlanNodeKind` variants to a `Cudf` prefix
(`CudfScan`, `CudfFilter`, …) so the two vocabularies are visually distinct everywhere —
schema, generated code, the C++ `node_type()` switches and serializer identifiers on the
Rust side. A pure rename: FlatBuffers wire bytes carry no table names and enum ordinals
do not move, so the proof is `plan_bytes.sha256` staying byte-identical with no
regeneration, plus green legacy subsets. The same commit sweeps the llm-wiki references
(architecture.md's fb names, affected tickets, and the recipe-plan table in this spec).

Landed on master as PR #120, with `plan_bytes.sha256` byte-identical and no golden
regenerated, which is the proof the rename asked for.

~~**T2 — ParquetBatchPartitioner.**~~ The pure policy class and its unit tests: fewer
survivors than N; N=3; single row group over target; batching off ⇒ one batch per chunk;
empty survivors (explicit error — the fbs "empty map means legacy single partition"
convention must not leak in); the balance bound on uniform row groups (max−min partition
rows ≤ one row group);
fixed-output determinism case. No planner integration yet.

~~**T3 — node and trait skeleton.**~~ `GpuNode`, `PartitionLayout` (with the two-valued
`SortOrder`), `Schema` with semantics annotations, `Batch`/`CpuBatch`/`GpuBatch` shells
with the move/`!Clone`/`Drop` rules, executor trait definitions with `CallStats`,
`Backend`. Traits in their own files per coding-style. Compiles under rust-only
with the GPU side gated. Unit tests: `SortOrder` canonicalization, layout equality.
**First, before anything else in this task**, compile a skeleton: the `Backend` trait with
all seven associated types, two impls whose `Batch` types differ, `NodeExecutors<B>`, and a
generic function driving one build→probe→finish transition and one source step. It
compiles with no `dyn` anywhere (verified), and it is what pins the static-dispatch
property the GPU path depends on — the mock backend the driver tests need is then a third
impl, not a special case.

~~**T4 — translation layer, single-partition shapes.**~~ DataFusion physical plan (tp1) →
`GpuNode` tree for chains: load, filter, project, sort (+fetch), limit (root-adjacent ⇒
no node, `skip`/`fetch` set on `GpuUnload`; otherwise a `GpuLimit` node over a
planner-inserted `GpuMergePartitions` — never a coalesce), coalesce-all,
single/final aggregates, cross/nested-loop joins. Per-node-kind conscious mapping;
unrecognized node ⇒ plan-time error naming it; window ⇒ the #143 refusal. Unit tests
assert emitted constructs for simple queries.

~~**T5 — translation layer, partitioned shapes.**~~ tp4: shuffle points → Merge+Emit, the
aggregate sequence with its shortcuts and the gid rule, join side normalization (type
remap + column-order-restoring project) and build-side coalesce insertion per the
capability matrix, union/interleave with explicit branch-cast projects. The
`hashKeys ⊆ group columns` structure is produced here (validated in T8). Unit tests per
construct in tp1 and tp4, including side-swap cases.

~~**T6 — estimator pass and plan goldens.**~~ `estimated_max_resident_size` per node
(rows × width vocabulary, N-lane charging), `target_batch_bytes` derivation feeding T2's
partitioner, integration as `plan_batch_partitioned()`. Canonize all four
`<mode>.plans.txt`, memory sections included, for TPC-H and TPC-DS (minus #23's four and
window queries, which appear as refusals).

~~**T7 — schema registry.**~~ (done). The `Schema` carried in `NodeKind` populated on all
nodes, with column semantics annotations. Unit tests: hand-crafted plans produce expected
types and annotations; decimal precision/scale fidelity through project/aggregate/union-cast
paths.

The type and the annotations landed with T3-T6; the tests are PR #126. They assert on the tree
rather than on rendered text, since both engines derive their per-node bytes from the same
declared schema and a wrong type moves no golden byte — `agg_state` at the init, the per-lane
merge and the finalizing merge, and `avg`'s state columns typed by what they hold rather than
by position, which is the case the task existed for.

~~**T8 — validation.**~~ (done). `validate_schemas_and_partitions()` on every node type: partition
topology, key-distribution subset rule, sortedness requirements (merge requires
`BatchSorted`; a limit after a sort requires its input to be `is_stream_sorted()`, checked
on whichever node carries the interval — the `GpuLimit` mid-plan, the `GpuUnload`
root-adjacent), `SingleBatch`
expectations (join build, cross/nlj inputs), captured-index checks. Unit tests: manually
constructed wrong combinations error, right ones pass; then run validation over every
canonized corpus plan from T6.

Node-local validation landed with T4/T5 and is called from `plan_batch_partitioned`; the
generic pass is `batch_partitioned/validate.rs`, PR #126. It runs over every canonized plan
because the planner calls it, so a rejection renders as a `refused:` section and fails both the
golden compare and the registry cross-check. Three planner defects were found this way and
fixed there rather than ticketed: `NestedLoopJoinExec`'s dropped projection, `GpuJoin` minting
a key distribution instead of carrying one, and a non-exhaustive match that dropped the claim a
mark join earns.

~~**T13 — drivers and enforcer.**~~ (done). Both drivers over a mock `Backend` impl — the
third instantiation, alongside CPU and GPU — with the schedule and the accountant as units of
their own, and the accounting formula with its pre/post checks. What the task settled is in
[Drivers](#drivers) and [Memory accounting](#memory-accounting); what it left for T14 is every
real executor, since nothing here computes a row.

~~**T9 — additive ABI.**~~ (done). The three approved symbols in `gpu_executor.cpp` + `peacock_gpu.h`,
signatures as [GPU execution](#gpu-execution-through-the-frozen-ffi) gives them; any
*further* surface change goes through a proposal to the human, per the constraint section.
Rust bindings for all three; `GpuBatch` handle plumbing (session ref, `Drop` release,
`ManuallyDrop` consume boundary). Tests: a C++ gtest in the plan-executor suite reading
disjoint row-group subsets and asserting union == whole-scan; a gtest exporting ranges of
one handle and asserting the concatenation equals the whole, plus the empty range and the
past-the-end range; the same for slicing, plus that the input handle is released and
double-slicing it fails; Rust FFI smoke on shad-gpu. The range plumbing reaches
`UnloadExecutor::unload(batch, rows)`, so the trait's second argument lands here rather
than in T10.

Landed with two shapes worth knowing. The row range is one function, `clamp_row_range`, that
the export and the slice share, so the two cannot disagree about an overrun; and the row-group
override reaches `execute_scan` as a `cudf::host_span`, so the node's own vector and a caller's
array take one path. `RowRange` and `unload(batch, rows)` were already in from T13, so the
trait needed nothing. One test moved tier against the list above: two IPC streams do not
concatenate, so "the ranges are the whole" is asserted in `test_gpu_abi`, where arrow-rs decodes
them, and the gtest holds the contract edges instead.

~~**T14 — recipe-plan serialization.**~~ The `GpuNode` → fb-seq mapping implemented, canonized and
unit-tested. `attach_recipes()` runs after the plan is complete and hangs a recipe on each node
that drives the GPU ABI; a node that makes no ABI call gets none, which is a fact about the node
and so is worth reading off the plan. One function per node kind produces that node's recipe from
**that node alone** — no child, no parent, no tree walk. The mapping table is a per-node statement
and a function that can reach a child would let it stop being one, so the restriction is the
design rather than an economy.

A recipe is a sequence of ABI calls, each carrying the built FlatBuffers node it addresses — the
payload, not a reference to where the fields live. Two renderings, one function taking an enum:
without payloads it is a section in every `<mode>.plans.txt`, between the plan tree and
`--- memory ---`, keeping the tree shape and repeating nothing the tree already shows except the
lane count; with payloads it is a golden of its own, holding the recipes alone — no plan tree, no
memory — for a subset of queries chosen to reach every fb kind and every call shape longer than
one call. A digest of the serialized bytes rides beside the payload text, since text and bytes can
disagree and `plan_bytes.sha256` is the precedent for pinning the wire form rather than a
description of it. Unit tests cover the kinds whose recipe is more than one call, `GpuJoin` first:
the seq set and call pattern per join type, against the
[capability matrix](#join-capability-matrix).

Dense seqs are impossible, and that is a property of the fbs rather than a choice. Children are
nested (`input`, `left`/`right`, `inputs`) and `CudfScan` is the only leaf table, so a set of
addressed nodes whose arities exceed its own edge count has to be padded with stub scans — and
every stub takes a post-order slot, so it moves the seqs above it. Three rules follow: stubs
rather than a re-hung child, since a shared offset is a DAG and gets indexed twice; a call whose
input is a runtime handle hangs off the previous fb node of its own recipe, or a stub where there
is none; and a forwarder's unconsumed branch is gathered by a structural `CudfUnion`, because an
orphan is never indexed and its shift has no visible cause. The pass and the serializer are
therefore one walk: a seq is the post-order index of what was built, so it cannot be counted
before the building.

The `Expr` -> `fb::Expr` writer is its own file and is where the unit tests concentrate: every
expression variant and operator, nesting, and each scalar kind the corpus produces — decimals
with their precision and scale first, since a wrong write there is invisible in plan text and
wrong on a device. `plan_serializer.rs` serializes a DataFusion plan and keeps that one job;
`serialize_scalar_value` and `serialize_schema` are reused, `serialize_expr` cannot be, since it
downcasts `PhysicalExpr` and this IR is our own.

Post-order is the agreement to assert rather than assume: `begin_plan` indexes post-order, so the
emitted tree has to number exactly as the recipes say, and a plan simple enough to check by its
answer would answer correctly while addressing the wrong node.

Nothing executes here and nothing new runs on a GPU. `scripts/exec_model/operators/recipe.py` and
`recipe_join.py` are the starting point for the join sequences — a model, not a spec, and the fbs
and `cpp/src/operators/join.cpp` are what settle a disagreement.

The proving set is the new unit tests and the golden target, and no legacy subsets — the human
scoped it that way because the change is additive: a recipe is attached to a plan nothing reads
yet, so the only tests whose result can move are the ones that read it.

~~**T21 — a recipe plan on a live GPU, driven by hand.**~~ It needs T14 and nothing else: no driver,
no executors, no scheduling. A new test file on the shad-gpu tier plans a query over TPC-H sf1,
loads the recipe plan T14 already built, and makes exactly the calls the recipes name, threading
each call's output handle into the next one's input and exporting at the root. One helper does the
whole walk; one test per query calls it, so a failure names the query rather than a stage.

`begin_plan`'s `out_node_count` is asserted equal to the number of fb nodes the writer created —
not to the `GpuNode` count, which is a different number: stubs, structural unions and any node
with more than one call all separate the two, in both directions and in most plans. It is the
first thing this task can settle that nothing before it can: every seq indexes a post-order the
C++ builds in `index_post_order`, and until a device has parsed a buffer we wrote, our agreement
with that walk rests on two child-order functions having been read side by side. One assertion, at
the first call, in the first place both numbers exist at once.

Shapes, chosen so each call is unambiguous. Everything but the aggregates plans one partition and
one batch, which makes every recipe a single call per node and the walk a straight line: a bare
scan, a filter, a project over a filter, and the joins — inner, and one build-preserving type,
whose single probe batch takes the legacy one-call form. The aggregates plan one batch and **two**
partitions, because a merge is the operator this mode adds and one partition never performs one:
two lanes each merge their own state, the cross-lane merge folds them, and the finalize project
runs once. That is the first time `AggregateMode::Merge` and the finalize expression meet a
device.

`avg` is the case worth a test of its own. Its finalize divides a decimal by a count, and cuDF
derives a divide's result scale from its operands where arrow takes it from the declared output
type — so a wrong cast is invisible on a CPU host and wrong on a GPU, in a column whose type reads
correctly either way. Assert the digits, not the type.

The oracle is DataFusion on the same SQL — `data_fusion_exact`, the CPU tier's own vocabulary —
and deliberately neither a result golden nor our CPU executor. A golden records what the first run
produced, so a finalize whose scale is wrong from the start is pinned rather than caught; and our
CPU executor evaluates the same finalize expression the device is sent, so it agrees with a wrong
one. DataFusion computes `avg` without a Welford triple, a merge mode or cuDF's divide-scale rule,
which is what makes agreement with it evidence. Joins compare as sorted multisets, since a GPU
join's output order is not deterministic. What it deliberately leaves out is everything the
driver decides — batching, backpressure, arrival order — since every shape here is one batch;
those arrive with the executors, and the driven end-to-end over every layout is T17's.

What has a device behind it. Ten fb kinds have run on one; every other is refused by name in a
`match` over every `FbKind`, so a variant added later stops the file compiling rather than going
quietly unclassified. Still unproven on hardware: [#136](../tickets.md#t136)'s finish pass — probe
keys per batch, the concat at done, the finish join, the pad project — the whole Right family,
cross and nested-loop joins, both sort nodes, `slice_handle` and a ranged export.

~~**T10, T15 and T16 — the executors, as one task.**~~ All three land together on one branch, because
they are one question asked of three node families: what does an executor do when the recipe
already says which calls to make. Ordered inside the task as T10 then T15 then T16, since the
accumulators and the joins are the Exec executors' shapes with state added.

~~**T10 — Exec executors.**~~ Filter, project, per-batch sort, aggregate (partial/single), unload
(`GpuBatch → CpuBatch`, honouring the row range). The **GPU executor runs the recipe attached to
its node** — the calls, in order, with the handles threaded — and reuses no legacy operator code:
the recipe is the instruction set, and reaching into legacy operator internals would be a second
path to the same kernels. The **CPU executor relays to DataFusion**, where reuse with legacy is
expected rather than avoided, since both are asking DataFusion for the same operator.

~~**T15 — accumulators.**~~ `GpuCoalesceAllBatches`, `GpuAggregateBatches` (merge-only and finalizing),
`GpuAccumulateBatchesAndSort`, `GpuMergeSortedPartitions`, and the mid-plan `GpuLimit`. Edge cases:
zero batches, one batch, ties for the merge (partition-major stability), fetch interaction, large
batch counts, gid-carrying aggregate merges.

~~**T16 — partition ops and joins.**~~ `GpuEmitPartitions` (per-batch scatter at a small N and a
large one, empty outputs for skewed hashes, and the lane each key lands in, asserted on both
backends — co-partitioning is what every partitioned join rests on). `GpuMergePartitions` is not
here: its mapping is `Forwarder`'s, from T13, and its service order is the driver's. `GpuJoin` with
`set_build`/`probe_and_fetch`/`finish_and_fetch`, plus cross and nested-loop joins on the same
trait. The [capability matrix](#join-capability-matrix) is emulated as a test table — per
(type × layout): stream-vs-refuse, correctness against a hand-built oracle, the GPU finish pass via
key accumulation ([#136](../tickets.md#t136)), `null_equals_null` on the finish join. That finish
pass is the one shape this mode invented with no device behind it after T21, which is why the
matrix is emulated here rather than assumed.

**What a copy costs decides the matrix, and the copy does not exist yet.** Every handle is erased
by its reader, and the frozen surface has no copy symbol, so a shape whose recipe names
`BuildSideCopy` meets a second probe batch with a dead handle. The question T16 had to settle was
whether to keep claiming those shapes stream and refuse until [#145](../tickets.md#t145), or make
a single-batch probe the matrix's permanent rule. Over the 37 hash joins in the
partitioned-tp8-standard goldens a copy would cost 0.08 of the probe stream at the median and more
than it for 12, so a permanent single-batch rule would price every join at the worst one: the
claims stand and the device refuses, naming [#152](../tickets.md#t152), with a test on the
refusal.

Left and Full outer go further and have **no device path at all** until then, which is #152's
second row rather than its first: their key project and their per-call join read the same probe
batch, so no ordering of the two leaves both an input. The finish pass's pad is therefore proved
on the CPU alone, and the device test asserts the refusal.

**How everything here is tested.** Small synthetic data, never the corpus; plans hand-constructed
rather than planned, so a test names the shape it means instead of hoping a query produces it;
`attach_recipes()` is fair game, since the recipe is what a GPU executor consumes. The oracle is
hand-constructed too: an expected result written down, not derived by the code under test. CPU and
GPU tests in separate targets so CI hosts split them. A device test writes its own parquet, which
is the ABI's doing rather than an exception: the four entry points load a table only by reading
one, so a device test's input is a scan or nothing. What the rule excludes is tpch.minimal and the
generated sf1, whose values nobody chose.

**What this task does not do.** No driver: nothing here is hooked into the schedule, and every
assertion is about one executor answering one call. That defers the whole class of claims that
read as call counts and pull counts — a limit holding nothing whatever the offset, at most two
batches sliced per query, the scan stopping — to T17, which is where a driver exists to make them.
`PlaceholderRowExec` ([#158](../tickets.md#t158)) waits for the same reason: it is a source, and a
source proves itself by what the driver pulls from it. T17 then found it cannot be discharged at
all while the surface is frozen — see the sixth entry under
[What the frozen surface costs](#what-the-frozen-surface-costs-and-what-unfreezing-would-buy).

~~**T17 — the whole path, under injection.**~~ The first task in which SQL goes in and rows come out:
planning, the recipes, the executors and both drivers running together, rather than each proved
against a fixture of the last one's shape. Every test starts from a query's text and ends at its
results, so what is under test is the join between the pieces — which is the only part four tasks
of separate proofs cannot reach.

The oracle is DataFusion on the same SQL. Not the legacy CPU executor, as this entry said before
T21: a second engine of our own agrees with us wherever we are consistently wrong, and by the time
this task runs, the finalize expression it evaluates is the one we also send to the device. The
one independent implementation in reach is the one that decomposed the aggregate differently.

Queries chosen to be interesting rather than representative, over the sf1 corpus text, and
between them covering the [join capability matrix](#join-capability-matrix): every join type this
mode claims, crossed with the layouts that make each one stream or refuse. The matrix is emulated
on synthetic data in the executors task, where each type is one executor answering one call; here
it is planned from SQL and run through the drivers, which is the first time a type's claim is
tested as the thing a user gets rather than as the thing an operator returns.

Four shapes are not join cells, so no join cover reaches them, and each is named by the query that
carries it:

| Shape | Query | Why that one |
|---|---|---|
| union lowered to an interleave | tpcds q33, q56, q60 or q66 | the claim is output lane p from lane p of each branch, so it needs four lanes; q14 also interleaves and is the trap, since its is `lanes=1` |
| union that cannot interleave | tpcds q77 | its branches disagree on lane count — 4+1+4, and the golden says `lanes=9` — which is the case [Node set](#node-set) argues in prose and nothing executes |
| both row-interval lowerings | tpch nested-limits | the root-adjacent interval becomes `GpuUnload`'s skip/fetch and the mid-plan one a `GpuLimit` over the scan; the only `OFFSET`s in either corpus, and it has no `.cpu.txt`, so this is its first execution |
| a merge with state worth merging | tpch shuffle-stddev | `GpuAggregateBatches` rides in most of the join queries as a sum; this is the Welford init, both merges and the finalize project |

Nested-loop Left is the one matrix cell no corpus query reaches, and its shape — a single-batch
probe, since #136's finish trick accumulates keys and a predicate join has none — is reachable
from no other row, so this task writes the query. The other uncovered cells stand: an Inner with
`null_equals_null` (the flag rides an INTERSECT lowering, and every corpus INTERSECT lands as a
semi form) and the three plan-time refusals, which a corpus query cannot provoke by construction.

Each query is re-run under injection, several modes rather than one, with the same answer demanded
every time. The prototype's [`LayoutInjector`](../../scripts/exec_model/operators/injection.py) is
where to look for modes worth having — layouts re-planned rather than edited, a rebatcher above
every source, sources emitting zero-row batches at a set probability — and it is a model rather
than a specification, so a mode it lacks and this path needs is a mode to add. Rebuild rather than
edit, for the reason the prototype records: a node's partitioning is not a field, so a rewrite
re-runs the planner at a chosen `(target_partitions, batching, small_table_bytes)` and the shapes
come out consistent.

Two rules the injector carries and this one must too: a join may be re-partitioned only when both
sides are hash-partitioned on the join keys, since otherwise its lane count is load-bearing and
splitting it joins matching slices; and a degenerate hash — every key into one lane — is a legal
hash, because a shuffle's contract is co-location and nothing above it may depend on how evenly
the lanes were loaded.

It also inherits what the executors task could not assert without a driver: a limit holding
nothing whatever the offset, at most two batches sliced per query, the scan stopping — each a call
or pull count — and `PlaceholderRowExec` ([#158](../tickets.md#t158)), which is a source and so
proves itself by what a driver pulls from it.

`PlanIndex` gets the unit tests it has never had, and they belong here because this is the first
task whose failures would be read through it. Nothing tests it directly today: `PlanIndex::build`
has one caller, and the scheduler tests derive their own subtree ranges from a parents array rather
than taking the index's. Assert what the derivation decides rather than what a plan happens to
produce — pre-order numbering and the contiguous subtree range that every hold rests on, `parent`
and the snapshotted children, and the three counts a category changes: `ready_lanes` against
`lanes` for a cross-lane accumulator and an emitter, `input_lanes` for the `Done` events a
partition accumulator owes, and `slot_base` where it is lane-scoped against where it is not. Each
of those is wrong far from where it shows.

**Two numberings meet in the driver, and one walk should compute both.** `PlanIndex` is
pre-order — a subtree is a contiguous range, which is what every hold rests on — and a recipe is
keyed by post-order, because that is how the FFI addresses a node. So the index records each
node's post-order position beside its pre-order one, from the walk it already makes, and
`executors_for` takes it. What must not happen is a third derivation: `attach_recipes` numbers at
plan time and the index numbers at run time, so a test asserts the two agree over the corpus —
[#134](../tickets.md#t134) is the same pair one boundary over, and it is unchecked there.

A source executor is this task's to write, and so is the answer to a lane with no build batch: a
`set_build` that never happens because `GpuCoalesceAllBatches` emitted nothing is a driver
decision, not an executor one, and T16 left it here deliberately (the finish's own zero-key answer
is already settled).

The row range the driver hands an unload is asserted before the call. `clamp_row_range` absorbs
an offset past the end and a length past it, because a C ABI has to be total — but
`RowInterval::range_of` cannot produce either, so the tolerance can only be reached by a driver
whose `rows_seen` has drifted, and what that looks like is a `LIMIT` quietly returning short.
Assert non-empty and within the batch where the driver builds the range, so the arithmetic names
itself rather than being absorbed.

The mock backend gets a handful of its own for the same reason one level up. Every assertion in
`driver/tests/` is measured against it, so a mock that miscounts is 1255 lines of tests agreeing
with the wrong answer and staying green. Pin what a script says against what the mock does — the
scripted batch counts and sizes per source and lane, the skew pattern an emitter fills its lanes
by, and an accumulator emitting where the script says it emits. A few cases, not a suite: what is
being checked is that the instrument reads what it was set to.

It starts by wiring the executors to their traits. `Backend` names seven associated types, so no
earlier task can implement it — T16 finishes the last of them and none of them owns a source — and
`Executor`'s `resident_bytes`/`scratch_bytes` are the memory accounting this task adds. So the
executors arrive as inherent methods in the trait's shapes, and the first commit here is the one
that makes the compiler check that.

**Defects found here are fixed here.** Every task before this one proved its own layer against a
fixture; the first thing to run all of them at once will find things about their joins, and
parking those behind tickets would leave the path unproven in exactly the way this task exists to
end. T21 is the precedent: it was meant to be one test file and it found four defects, each a
rule held by a doc comment with nothing reading it, and one guard that could not go red.

~~**T17a — layout injection over corpus queries.**~~ (done). T17 runs seventeen queries at five modes and
calls it injection; it is not. Each of the five is a plan the planner would have chosen anyway —
`(target_partitions, sizing)` re-planned, with `small_table_bytes` and the budget constant. The
prototype's [`LayoutInjector`](../../scripts/exec_model/operators/injection.py) does a different
thing: it takes one plan and rewrites it into layouts no planner would emit — lanes deliberately
drained, a degenerate hash, a rebatcher above every source cutting against the grain, sources
emitting zero-row batches at a probability. Four dimensions, none of them reachable from SQL, and
none of them exercised by any real query today. They live in `driver/tests/stress.rs` over a mock
whose answer is a script.

**Why T17 could not do it, and what this task has to build.** `GpuNode` exposes `children()` and
nothing that reconstructs a node — no `with_children`, no `rebuild` — which is why T17 re-planned
instead. But the rewrite is writable without touching production: `as_node_ref` is a public
exhaustive `NodeRef` over all eighteen kinds, each node's steering fields are `pub`, and each kind
has a public constructor. So a test-side `rebuild(node, new_children)` is eighteen arms that read
the fields and call the constructor, and because the match is exhaustive a nineteenth kind fails to
compile rather than being silently un-rewritten — the same guard `node_kind()` and `driven` already
rest on.

**Prove the rewrite before using it, and a unit test is the right size.** A rebuild that drops a
field is a plan that differs from the one under test for a reason nobody chose, and every result
after it is then about a different query. The case is small: hand-built plans rebuilt with their own
children, identical in **debug output** — not in the rendered plan. The renderer is what a golden
reads and it does not print everything: a loader's survivors and `can_be_null`, and an aggregate's
intermediate schema, reach no plan line, so a rebuild that drops `can_be_null` renders identically
and passes. Those are exactly the fields a corpus plan never varies, which is what would have made
the rendering-only form a guard that cannot go red. Debug prints every field including the private
ones and is the identity. Nothing in this case reads a committed file: the comparison is a plan
against its own rebuild, so it holds without goldens and moves when neither of them does.

**Make the field cover exhaustive rather than claimed.** Fixtures that populate every optional
field are a hand-maintained list, and the failure it misses is the one that will happen: a field
added to a node next quarter, no fixture varying it, the arm free to drop it, the test green.
Three levels, and the third is the one worth building:

- *arms* — the match over `NodeRef` is exhaustive, so a nineteenth kind fails to compile;
- *kinds* — the fixture set is asserted to reach every variant, so a kind with no fixture is red;
- *fields* — derive them from the debug output rather than listing them. Every field name a node's
  debug prints must take **at least two distinct values across the fixture set**, because a field
  constant everywhere is a field whose loss no fixture can detect. It goes red naming the field,
  and it goes red the day the field is added rather than the day something depends on it.

That last one is what makes the case exhaustive without reflection and without a maintained list.
It is also a guard that can go red on its own, which on this chain has been the exception. Not through
`driver/plans.rs` — it is `#[cfg(test)] mod plans`, so an integration test cannot see it, and its
builders leave every optional field `None`, which is the half the property needs. The fixtures
live beside `rebuild` and give each node a value in every optional field: `Filter` with a
projection as well as without, `Limit` with a fetch, `Join` with a residual. A corpus plan covers
only the combinations its queries happen to produce, and a fixture set of all-`None` nodes would
let a dropped field through in silence.

Cover the arms the way `every_node_kind_builds_the_executor_its_category_names` covers its own:
assert the fixtures reach every `NodeRef` variant, so a kind added later is a compile error in the
match and a red count here. It goes red by dropping one field from one arm. Nothing is injected
until it passes, and no corpus query is needed to prove it.

**Two mechanisms, two dimensions each, because the dimensions are not the same kind of thing.** A rebatcher is a node —
it changes the tree, and it must be placeable anywhere a batch flows, not only above a source. The
other three are behaviour a node does not carry: this engine's hash is Spark-murmur3 fixed in the
emitter, and neither an empty lane nor a zero-row batch is a field of `GpuLoadParquet`.

| Dimension | Mechanism | Why not the other one |
|---|---|---|
| a rebatcher, at any edge that still validates | tree rewrite: insert a node above the chosen child. One direction only — nothing below the loader splits a batch ([#142](../tickets.md#t142)), so the node is `GpuCoalesceAllBatches` merging a lane to one, and the finer direction is the mode axis already. A coalesce clears the sort order by construction, so an edge under a `GpuMergeSortedPartitions` or a limit-after-sort is refused at validation: assert the refusals are exactly that class rather than skipping those edges quietly, since a skipped edge is a hole nothing reports | wrapping an executor cannot do it — an exec is one batch in, one out, and a merging rebatch must hold batches across calls, which is an accumulator's job. A wrapper that held them would lie to the accountant and break the queue bound the driver guarantees |
| a drained lane | tree rewrite: move that lane's row groups into another lane | **not** a wrap — a source producing nothing for a lane loses its rows, so every oracle comparison fails and the dimension is untestable. `partition_groups` is the mapping, so moving groups keeps the lane count and every row. Safe at every plan this mode produces: `co_partitioned()` requires one lane or a ByHash distribution on the join keys, so no join here rests on scan lane alignment — the shape the prototype needed `_is_shuffled_join` for and this mode does not have |
| zero-row batches at a probability | wrap the source executor: emit an empty batch instead of advancing | not a node field; the driver already carries empty batches, so this exercises a path the operators have and this one does not |
| a degenerate hash | wrap the emit executor: route every key to one lane. **Not compatible with a plan carrying Right, Full or RightAnti**: one lane holding every key leaves the others with an empty build side, and those three owe their probe side against one, which is [#175](../tickets.md#t175)'s refusal. Drop the dimension for those plans and pin the refusal with a case, so the drop is a rule a reader meets rather than an abort they rediscover | the emitter carries key *expressions*, not a hash function — ours is fixed |

Lane counts are deliberately absent: `target_partitions` already varies them through the planner,
and a lane count no planner would choose is the one dimension whose correctness rule the prototype
had to encode (`_is_shuffled_join`, `_LaneOrigin`). Buying that rule to vary something the mode
axis already varies is not worth it here.

**Three phases, and the middle one is the point.** Plans first, selection second, execution third,
because 10 queries × 5 modes × the injection crossing is more runs than a CI tier can hold, and
choosing *which* to run is a claim that has to be visible rather than a `take(30)`.

1. **Plan.** For each enabled query, plan at all five modes. Five plans, no injection yet, and a
   plan that fails to build is a failure rather than a skip.
2. **Select.** Derive the candidate set — each plan crossed with the injection settings — and
   choose at most **30 per query**. Representative means the selection covers each dimension at its
   boundaries and each mode at least once, not a sample: the rebatcher in both directions, the
   empty-batch probability at zero and at its high setting, a drained lane where the mode has more
   than one, the degenerate hash where the plan shuffles. The chosen set is **asserted, not
   trusted** — a test over the selector alone, with no queries, that a known candidate set yields
   a cover, and that dropping a dimension from the settings makes it go red. Deterministic and
   seeded: two runs choose the same 30, or a failure is not reproducible.
3. **Run.** Each selected plan through the driver, every answer against the same oracle.

**Render the oracle once per query.** Comparing renders both sides, so an oracle rendered per
variant makes a query's cost grow with the number of variants for no reason — at thirty variants
that is most of the tier. Render once, hold the text, compare every variant against it.

**The oracle does not change and must not.** It is DataFusion on the same SQL, planned and
collected once per query at `target_partitions=1`, compared against every variant. That is what
makes injection meaningful: the answer is fixed by something that never saw the layout, so a
layout that changes the answer is a defect rather than a disagreement between two of our own
shapes. One oracle per query, not per variant — it is the expensive half and it is invariant.

**Eleven queries, and the cost axis is not the obvious one.** Injection multiplies runs, so it goes
on the cheap end of the list — but *cheap* here is the size of the result, not of the scan. A run's
cost is dominated by the oracle comparison, so `anti-join`, a `SELECT *` over 1.2M rows, is the
most expensive member of a set picked by rows scanned. Scanned rows at sf1, which is the proxy the
set was first chosen by and is kept here so the correction is legible:

| Query | rows scanned | what it carries |
|---|---:|---|
| tpch `nested-loop-join` | 40,000 | nested-loop Inner |
| tpch `nested-loop-left-join` | 40,000 | nested-loop Left, single-batch probe |
| tpch `nested-limits` | 220,000 | both row-interval lowerings, cross join |
| tpcds q45 | 899,384 | LeftMark |
| tpch `anti-join` | 1,600,000 | RightAnti |
| tpcds q8 | 3,060,404 | Inner multi-key, LeftSemi with `null_equals_null` |
| tpcds q16 | 3,087,163 | LeftAnti, LeftSemi with a filter, a mid-plan limit |
| tpcds q93 | 3,187,918 | Right outer, multi-key |
| tpcds q97 | 4,361,952 | Full outer |
| tpcds q2 | 4,401,864 | the union that cannot interleave |
| tpcds q33 | 5,281,336 | the four-lane interleave — eleventh on cost, first on merit |

q33 is in on merit, and it happens to cost little: eleventh of the seventeen, six heavier ones
left out. It is the only four-lane interleave in the
list, and an interleave is the one operator whose correctness *is* a lane correspondence — output
lane p from lane p of each branch. Excluding the one shape a perturbed lane could break, to save
0.9M rows over the tenth, would be picking the cheap set over the point of the exercise.

The six left out are the heavy end — q38, q87, `shuffle-stddev`, q20, `left-join`, and q21 at
19.5M rows, three `lineitem` scans in one query. If the budget turns out to allow more, `left-join`
is the next one worth having: a Left outer's finish pass accumulates probe keys, so its residency
is a function of how the batches arrive.

**Shape of the change.** One new test library file carrying the wrappers, the settings, the
candidate derivation, `rebuild` and the selector — `peacockdb-core/tests/common/injection.rs`,
split into a second file only if it passes the 1000-line bar. The **tests** go in
`test_cpu_batch_partitioned.rs`, not beside the mechanism: a `#[test]` under `tests/common/`
compiles into every one of the 22 binaries that declare `mod common`, so it would run 22 times and
be counted 22 times. A target of its own would avoid that and costs a `pipeline.yml` step and a
`test_ci_coverage` entry, which is not worth it for two cases. The eleven
fixtures are declared as one list: `injected_queries!` expands it into both the `INJECTED` const
and the fixtures, so a query leaves the set only by leaving the list, and everything else is
unchanged. One thing in `peacockdb-core/src` changes, and only its visibility: `validate()` becomes `pub`.
The driver validates nothing — `check_canonical_form` is limit positions and no more — so an
injected tree would have run unchecked, and a rewrite that broke a node's requirements would have
answered rather than been refused. That is the failure this task must not have: an injector
quietly generating plans the planner would never emit. It is also what makes the rebatcher's
refusal a demonstration rather than a prediction, since `validate()` is what names the node and
the order it broke.

The cases split by dataset rather than by subject. Four need none — the identity case, the
selector's cover, the rebatcher refusal and the degenerate-hash emit — and go in
`tests/test_batch_partitioned_injection.rs` with its own `pipeline.yml` step and
`test_ci_coverage` entry, so they run in seconds on any host. The two that need a query — the dimension demonstration, which plans
`nested-loop-join` over sf1, and the injected corpus set — stay in the end-to-end file. A mechanism proof buried in a tier that takes minutes is one nobody runs
while iterating.

**Measure before capping, one run at a time.** T17's seventeen queries at five modes are 85 runs in
4m39s at four threads. Eleven queries at up to 30 is 330, on the cheap end of the corpus — a number
to measure, not to assume.

The measuring pass runs **serially**, `--test-threads=1`, and times each run on its own. Four
threads contending for the same host is what the correctness tier wants and the opposite of what a
timing wants: a number taken under contention cannot be compared with another taken under different
contention, so a table built from a parallel run would rank the wrong things. Time each
(query, mode, injection setting) individually and report every one — not a total, since a total
cannot say which row to cut.

The table is the deliverable of that pass and belongs in `llm-wiki/reports/`, since it is a
measurement of a host rather than a fact about the code: one row per query, the per-setting times
across it, its total, and the grand total, with the host and thread count at the top the way the
benchmark records carry `build_profile`. Then the cap is chosen against it.

**What may be trimmed, and what may not.** If it does not fit the tier, cut runs and not cover.
The selection rule already guarantees each dimension at its boundaries and each mode at least once,
so a smaller cap is still a cover — that is what the rule is for. What must survive any trim is one
carrier per injection dimension and the shapes that only one query has: q33's interleave, the two
nested-loop forms, `nested-limits`' two row-interval lowerings. Trimming to the cheapest eleven
minus q33 would be the obvious cut and the wrong one, for the reason q33 is in the list at all.
Where a query is dropped entirely, say which dimension lost a carrier and why the remaining ones
cover it.

~~**T18 — infra for running corpus queries.**~~ (done). T17 proved the path on seventeen queries and T17a
injected layouts into eleven; the corpus is 39 tpch and 99 tpcds, each at five modes on two
engines. What stands between the two is not more test cases but a way to declare one. This task
builds that. It was split out of T20, which carried both this and the new query shapes, once it
was clear the rollout needed the infra before it needed either.

**One declaration per query.** `corpus_query!(dataset, sf, query, cpu_modes, gpu_modes,
cpu_oracle, gpu_oracle)`, where the two mode arguments are a bitwise or over the five planning
modes the plan goldens already carry. A query's whole coverage is then one line that can be read
and diffed, rather than up to ten macro invocations that can disagree with each other. It expands
to one test case per (query, enabled mode) on each engine, so `cargo test <query>` names exactly
the runs that query has — which is what makes the filtered regeneration below possible at all.

The CPU and GPU cases go in **different binaries**, as the legacy pair does: the GPU targets are
staged to shad-gpu by the gpu-tests job, and `test_ci_coverage` verifies that job's array names
them. One macro writing into two binaries is the reason the mode arguments are separate — a query
can be enabled on the CPU at four modes and on a device at one.

**The oracle keywords already exist and are taken unchanged.** `cpu_oracle` is
`data_fusion_exact | data_fusion_approximate | data_fusion_subset`: one oracle, plain DataFusion
at `target_partitions = 1`, asked three ways — the whole answer, the whole answer at a 1e-12
relative tolerance where the sole divergence is float summation reassociation, and the count and
containment alone where the SQL does not determine which rows (see the unordered `LIMIT` below).
The third is new; the first two are legacy's, unchanged. `gpu_oracle` is
`golden_exact | golden_approx | golden_approx_std | live_cpu | skip`: the frozen result compared
exactly, at 1e-12, at 1e-11 where cuDF's variance diverges further than the convention allows,
against a live CPU run where the result is too large to commit, and not at all for a query whose
row order is undetermined. A second vocabulary for the same choice is how two families drift, so
these are the legacy sets or they are a rename of them, never a parallel set — and `live_cpu` is
that rename, applied to legacy's ten call sites in the same commit. Legacy spells it `oracle`,
which inside an argument named `gpu_oracle` says only that an oracle is an oracle, while the
choice it actually makes is between a frozen result and a live one.

There is no seventh argument for whether the run writes `.result.txt`. Legacy carries one
(`result_golden` / `no_result_golden`) because its producer and consumer are declared in
different files, and an orphan golden — written by a CPU case no GPU case reads — is silent
where a missing one is loud. Here both sides are in one declaration, so the predicate is
`gpu_oracle` naming a golden at any enabled mode, and the pairing cannot be stated wrongly.

**Goldens stop being one file per query.** Today each query carries its own `.cpu.txt`,
`.cost.txt` and `.result.txt` per mode, which is how `testdata/goldens/` reached 277 files for
tpch.sf1 and 625 for tpcds.sf1. This mode takes the shape its plan goldens already took: one
`.cpu.txt` and one `.cost.txt` per mode, each holding every query in `== <query>` sections, and
one `.result.txt` across all modes. The comparator is the plan goldens' own `section_differences`
— it names what moved, what is missing and what is out of order, and it has unit tests — so this
is a second caller rather than a second differ.

A query whose bit is clear for a mode still has a section in that mode's files, carrying a marker
that says it was skipped. An absent section and a skipped one are different facts and the file has
to hold both: a query that stopped planning is a regression, a query never enabled at that mode is
a decision, and a format that renders them alike loses the only artifact that could tell them
apart.

**`.result.txt` is one entry per query**, keyed by the query alone. The modes are supposed to
agree on results, so one of them authors it: the last mode the query declares in the fixed
sequence of five — `bp-tp4-sized` for most. Its authority comes from the declaration and not from
what happened to run, which is what keeps it well defined under a filtered regeneration. A run
that does not include the authoritative mode leaves the section untouched; it is the one golden
whose key carries no mode, so it is the one a partial regen could otherwise re-author from a mode
that is not the authority, with the body's own line moving to say so and nobody reading it. The
section still records which mode produced it: the key carries no mode because there is one entry,
and the body names one because where the modes disagree, that disagreement is what the file exists
to make visible. A result at or above `RESULT_GOLDEN_MAX_BYTES` (256 KB) keeps its section and
carries a marker saying so, rather than being deleted — and the cap is reached while rendering
rather than after it. Legacy renders the whole answer to one string and then measures it, so
`anti-join`'s 1.2 million rows are materialized in full to discover they are 240 MB and unwanted.
Here the rows are rendered one at a time against a running total and the whole set is dropped the
moment it passes the cap, so the peak is the cap and one row rather than the answer. Which also
settles the sort: a set that is never going to be written is never sorted — the legacy path deletes it, which reads as
"no golden" and as "golden not applicable" identically, and `build-test.md` states the old rule
and is corrected in the same commit.

**`live_cpu` is what the device uses when no frozen result can serve it, and there are two such
cases rather than one.** The first is legacy's: a result over the cap has no golden to compare
against, so the device is held to a live cpu run instead. The second falls out of this mode and
has no legacy counterpart — `.result.txt` holds one entry per query, authored by one mode, while
the device runs at every mode the query enables. Where the rows are the same at every mode that
is one golden serving five runs, which is the point of the single entry. Where they are not, it
is one golden that only one of them can match: `scan-limit` at `bp-tp1-single` returns different
rows from the `bp-tp4-sized` section, and `golden_exact` would fail on a correct device. So a
query whose `cpu_oracle` is `data_fusion_subset` writes `live_cpu` as its `gpu_oracle`, and is
compared against a cpu run at the *same* mode — where both walk one driver over one plan and the
rows are determined again.

**Written at the call site, never inferred from the argument beside it.** Every argument is
required and says what it means; a `gpu_oracle` deduced from a `cpu_oracle` is a behaviour
selected implicitly by an input, which is the shape `coding-style.md` names as an antipattern and
which this macro exists to avoid — seven arguments so a query's whole coverage reads off one
line. That both conditions are derivable is why a *check* can exist, never why a value would be
absent: a `golden_exact` where the cap or the subset oracle applies is asserted red, since it is a
test that fails on correct behaviour, and a `live_cpu` where neither applies is asserted red too,
since it spends a device-side live run on a comparison a committed file makes faster and harder.

**Every result comparison decides before it materializes, and today none of them do.** Three
places take the same wrong order and the fix is one idea applied three times. Containment collects
the unlimited answer and renders every row; the cap renders the whole result and then measures it;
and `assert_results_match`'s exact arm is `assert_eq!(render(actual), render(expected))`, whose
arguments Rust evaluates eagerly — so both sides are materialized in full to answer a yes-or-no
question, half a gigabyte of `String` for `anti-join`, and the rendering exists for the failure
message rather than for the comparison.

The exact arm becomes a digest: render a row, hash it, drop the string, sort the hashes, hash the
sequence. Memory goes from the answer to eight bytes a row, the verdict is unchanged, and nothing
is rendered on the green path at all. On a mismatch it re-streams and prints a bounded excerpt
around the first differing row — which is not a courtesy but the same lesson `section_differences`
carries, that a dump too large to read is a dump nobody reads, and a 240 MB `assert_eq!` diff is
that failure three orders of magnitude over. The tolerance arm keeps its map, because a digest
cannot express "within 1e-12"; it stops formatting the columns it does not key on.

No verdict changes, so no green test moves.

**A `live_cpu` comparison runs once per mode, beside the device run it checks.** It cannot reuse
another mode's answer: where the SQL does not fix the row set, a cpu run at `bp-tp4-sized` is no
more an authority on what the device returns at `bp-tp1-single` than the frozen section was, which
is the reason this value exists at all. Same rule for the over-cap case even though its result is
mode-invariant — the test is already per (query, mode), so same-mode costs nothing extra and one
rule beats two that differ by a condition a reader has to check.

What it costs is on the device leg, which is the leg that runs `--test-threads=1` on one host: a
`live_cpu` query executes twice per mode there, once on the device and once on the cpu backend
through the same driver. Legacy carries ten such queries and this mode adds `scan-limit`, so at
five modes it is on the order of fifty extra cpu runs on the gpu host. Named here rather than
discovered in a job duration, and it is part of the cost the corpus tier accepts rather than a
separate decision.

This is not the `result_golden` case one paragraph up. That argument is omitted because it has no
degrees of freedom left once the rest of the line is written — legacy needs the keyword only
because its producer and consumer live in different files. `gpu_oracle` has five values, and the
constraint pins it in two cases out of five.

A section can therefore turn from content into a marker, and back, as a result crosses the
threshold. Nothing guards that transition and nothing should: at a fixed scale factor a result's
size moves only when its answer moves, which is the thing every other check in the tier is
already watching. The marker is there so the file says why a result is absent, not to absorb a
size that oscillates.

**A run that stopped early says so, on its own line.** A satisfied limit ends the run with work
undone: lanes that were never pulled, row groups never read, batches produced and never consumed.
Every one of those shows up in the annotations as a smaller number, and a smaller number with no
stated cause is indistinguishable from a plan that produced less — which is the absent-versus-
skipped confusion this tier exists to prevent, one level down. So each section opens with
`early_exit=<node>@<ordinal>` naming the limit that was satisfied, or `early_exit=none`. Always
present, never inferred from absence: a query that ran to completion is a fact the file states.
It is stable, because [the determinism rules](#determinism-rules) pin the schedule — which
batches were consumed before the limit was met is fixed for a plan.

Beside it, `rows_skipped=N` on each node that has any: rows released without an unload call,
which the driver already counts per node precisely because the rows returned look identical
either way. It is the saving a limit buys, and the golden is where it becomes visible.

**A batch emitted and never consumed is still emitted.** It is counted where it enters a queue,
so it is in its producer's `batch_rows` and `batch_bytes` whether or not the parent ever took it;
what the parent took is its `in_rows`. The gap between the two is the work a limit threw away,
per child and per lane, and it is the quantity `early_exit` exists to explain — without the
marker it reads as a node that emitted more than its parent wanted, which is a defect
everywhere else.

The same rule decides the cost, and the answer is not the intuitive one: `.cost.txt` prices what
was produced, not what was used, because the device did that work and holding those bytes is what
the budget was spent on. A query that exits early is genuinely cheaper than one that does not,
and the batches it produced before exiting are genuinely not free.

Where the drop begins is bounded rather than recorded, and does not need a third list. Queues are
FIFO and arrival order is pinned, so what a parent took is a prefix of the child's lane — the
cumulative sum of `batch_rows[j]` reaching `in_rows`, ending mid-batch only where a limit sliced a
straddling one. So `in_rows[child][j]` lies between two adjacent prefix sums of that child's lane,
which is a tighter check than `<=` on the total and is the form to assert.

**Two checks change shape under it, and both get stronger rather than weaker.** The loader
identity — `batch_rows` having `partition_groups`' exact shape — is a plan-against-run
comparison, and early exit is exactly when the run does less than the plan. Per lane it becomes a
prefix, and per lane rather than over the flattened shape: `batch_rows[j]` is a prefix of
`partition_groups[j]` — its own lane's list, aligned from the start because a scan reads its
groups in order — with an empty lane allowed, and every lane equal on a run whose marker says
`none`. The scheduler stops where it is, so one run can have lane 0 complete, lane 1 short and
lane 2 empty. Compared flattened, that run either goes red or forces the check down to a total
length, which no longer says a batch lines up with the row groups that made it — the whole reason
the nesting is there.
The `in_rows` identity needs no marker at all, because `abandoned` is rendered beside it. That was
decided against a count rather than a hunch: keying the arithmetic on the marker would soften it
to `<=` at every node of an early-exiting section, and 84 of 99 tpcds queries carry a `LIMIT` — of
the 67 with a committed result golden, 35 return 100 rows or more, so the limit really fires. Half
the bench, not a handful. The marker is also not precise enough to carry arithmetic: it is
`any_satisfied()`, so a query returning exactly 100 rows sets it while consuming everything, and
the weakening would apply where nothing was skipped.

So the conservation law does the work — `consumed + abandoned == emitted`, one statement over the
file and over the report, rather than a law in the driver's tests and a weakening of it in the
golden. Same family as `in_flight_bytes` returning to zero and `holds == releases`, which the
report already carries: a batch that vanishes without being either consumed or abandoned is a
defect no inequality can see. Deciding it now costs one optional field in a format gaining five;
after T19 it costs regenerating eleven files per bench across a hundred queries.

The marker keeps the two jobs it is good at: telling a reader the run stopped early, and keying
the loader's prefix, which `abandoned` cannot close — a row group never read produced no batch to
abandon.

**Several test cases now write one file, and that is the task's one real hazard.** A whole-file
write is last-writer-wins, which would drop every other query's section and leave a green run. So
a regenerating write takes an advisory lock on the file, merges its section, and publishes by
writing a sibling and renaming onto the name — the rename because a crash mid-write must not
leave a truncated golden, which `point_canonical_root` already does for the same reason.

**The lock is on the file, not in the process, and the distinction is load-bearing.** Libtest runs
a binary's cases as threads in one process, so a `Mutex` would serialize them — but that is a
guarantee about one binary, and what makes it sufficient here is the separate fact that only the
CPU binary writes. An invariant, not a property of the language: the next writing binary breaks it
with nothing red, `cargo nextest` runs each case in its own process and would silently reduce a
mutex to no lock at all, and two shells regenerating at once are outside any of it. The
`canonical_root` comment records the same surprise one level down — two binaries reaching one path
at the same time, fixed with an atomic rename rather than a lock, because a lock in one process
could not see the other. `std::fs::File::lock` is stable on the toolchain in use, so this costs no
dependency.

**Partial regeneration follows from that.** A filtered run regenerates only the sections its cases
produced and leaves every other section as it is, under an environment variable of its own rather
than by widening `UPDATE_CANONICAL`, whose contract is a whole file. `PCK_TEST_FILTER` already
scopes which cases run, so the mechanism has a caller before it has a second one. What must
survive is the distinction above: a filtered regen that cannot tell "did not run" from "stopped
planning" deletes coverage silently, which is exactly what a golden exists to prevent.

**`.cost.txt` stays a pure function of its `.cpu.txt` and `cost_model.conf`**, so the derivation
is per section rather than per file, and `test_cost_model` re-derives every one.

**The GPU side reads what the CPU side wrote**, as in legacy: per-node plan shape and the
input/output statistics against that mode's `.cpu.txt` always, and `.result.txt` where the
`gpu_oracle` names a golden. It never writes either, and ignores the regeneration variable
rather than honouring it — a device that can author its own golden proves nothing against it.

One thing is easier here than in legacy, and worth not spending twice. Legacy's two engines run
separate executors, so the two rendering the same tree is a coincidence the golden exists to
check; here `batch_partitioned_driver` is generic over the backend, so both engines walk one
driver and report through one `RunReport`. The rendering is therefore written once, and what the
golden still checks is the answer the device gave, not the shape of the report it came in.

**Registry.** The five `bp_*` columns exist and mean plan enablement, declared by the golden's
section rather than by a macro — `test_batch_partitioned_plans` holds the two to each other in
both directions. Execution needs its own columns, five per engine, and those are macro-declared
through the existing link-time inventory, so the widget's three groups each have a source that
something checks. Adding ten columns to a seventeen-column csv is the point to decide whether the
row stays flat or the modes become a repeated group; the inventory tests are what must keep
working either way.

**Widget.** A batch-partitioned table per bench, repeating the legacy structure — peacockdb cost,
duckdb cost, ratio, features, tickets — with the four mode columns replaced by three: planning,
cpu execution, gpu execution. Each holds one cell per mode. A planning cell links to that query's
section in `<mode>.plans.txt`, which is also where its refusal is, so an enabled query and a
refused one link to the same file and differ only in what the reader lands on. A cpu cell links to
the query's section in that mode's `.cpu.txt`. Where not all modes are enabled, the cost columns
use the last mode in the sequence for which cpu execution is enabled, as the legacy rule does for
its own last mode.

**The cost-regression gate is a third rendering, and it degrades silently unless this task
changes it.** `--cost-diff` compares each `.cost.txt` against the same path at the PR's base and
upserts its own PR comment: improvements green, regressions red, and a non-zero exit that fails
the build. It is where a cost win becomes visible, so the new mode belongs in it. But it is
written for one file per query — `collect_cost_goldens` globs `*.cost.txt`, `read_total` takes
the *first* `peacockdb_cost=` line in a file, and `diff_label` takes the filename's first
dot-segment as the query name. Point that at `bp-tp4-sized-mini.cost.txt` and it picks the file
up, reads whichever query's section happens to be first, and labels the number with the mode: a
wrong row, rendered confidently, gating the build. The glob is what keeps it quiet — the files
are found, so nothing reports them missing.

So the differ becomes section-aware in the same way the comparator did: a row per (query, mode),
totals read per section, the label carrying both. `cost_diff` itself is unchanged, since it
already works over a map of label to number; what changes is what fills the map. Its unit tests
are the pattern to extend, and the red case is a two-section file whose second section moved —
the per-file reader reports no change, the section reader reports one.

**There are two widgets, and both get the table.** `cost-report` renders the same data twice —
`--md`, a markdown blob upserted as one PR comment keyed on a sentinel, and `--html`, the site
published to Pages from master. Close to identical rather than identical, and the differences
are the ones a format forces: markdown cannot set a row background, so a row over the ratio
threshold is flagged in the cell instead, and the mode cells have a markdown counterpart to
`mode_cells_html` rather than sharing it. A table added to one and not the other is the failure
to expect here, because the html is what a person looks at and the markdown is what the review
actually reads.

**No production change that alters what the engine computes, here or in T19.** A query that
does not run is disabled with a ticket, never fixed in passing: a fix made while enabling one
query is a fix nothing else in the branch proves, and the diff under review stops being the
infra. What this task does touch in `peacockdb-core/src` is the goldens' own surface and nothing
else — the node renderer per [Node display](#node-display) (`name@ordinal` references, layout in
place of the lane count, `fetch` and the aggregate lists wherever carried, schema in the plan
golden only), the per-batch line below, `cost_model.conf` entries for all eighteen node kinds at
once rather than as each is first seen — held there by a case red on a nineteenth, derived from
the exhaustive `as_node_ref` the way the writer's field cover is, since entered-at-once is a
moment and T19's "no `cost_model.conf`" rule needs a property — and
per-node emitted rows and bytes on `RunReport` — the one thing the engine does not already
report. Everything else the
comparison needs is there: `Batch` gives rows and bytes on both backends, the device's coming
through the frozen ABI's `PeacockNodeStats` priced against the declared schema, and
`trace`'s per-event `outputs` sums to `out_batches` per lane. What is absent is a per-node total
of what each node emitted; the driver reads both numbers already, for the accountant and the
limit interval, and keeps only aggregates. `rows_seen` is not the emitted total and must not be
reused as one — it counts rows arriving at a node for the limit rule, which is `in_rows` summed
over lanes rather than anything a node produced. Plus the
`pipeline.yml` steps `test_ci_coverage` requires, which are not engine code at all. Three more landed than that list, each approved on its own and each recorded here rather than
only in the exchange that approved it: `CpuBatch::byte_size` pricing from the plan's schema, which
is the one the headline forbids by name and which moved every budget decision in the engine; the
`gpu_backend` guard closing [#181](../tasks/bp-tickets.md#t181); and `driver/mod.rs`'s `mod index` going
`pub(crate)` so the renderer walks the driver's own index rather than a second pre-order.

Naming that surface is what gives the rule an edge, and the edge only holds against a list that
is true: T19 has none of these seven left to touch, and T20 is where the engine moves again.

**Every node carries what it consumed and the size of every batch it emitted**, as parallel
structures on one continuation line under the node:

    GpuLoadParquet: table=lineitem, partition_groups=[[[0,1],[2,3]],[[4],[5,6,7]]], lanes=2, …
      in_rows=[] batch_rows=[[1500304,1500303],[1500304,1500304]] batch_bytes=[[31881456,31881440],[31881472,31881455]]

Lanes outermost and batches within, which is `partition_groups`' own nesting — so on a loader,
element `i` of lane `j` in all three lines is one batch, and the row groups that produced it sit
at the same index as its size.

`in_rows` is what the node consumed, per lane, nested by child rather than flat — a filter reads
`in_rows=[[860160,737280,…]]` and a join reads both of its sides. No space after a comma, which
is `partition_groups`' committed convention — the two nestings are read index for index and the
tail case is a 96-batch lane. Legacy prints one `in_rows`
per partition and it is the first child's, so a join's build side appears nowhere on the join's
own line; here the two sides differ in kind and the capability matrix turns on which is which,
so a format that can only show one of them is one that hides the interesting half. A source has
no children and prints `[]`, which is not the same as a child that consumed nothing. It is the
one figure the batch lists cannot imply, and the reason legacy's sub-line is dropped rather than
merely thinned: rows in against rows out, on the node's own line, is what makes a selectivity or
a skew visible without walking the tree.

Rows and bytes are separate lists rather than pairs because each is then a column a reader scans
down and a diff reports as one moved number; the cost is that a batch's two figures are not
adjacent, which the shared indexing is what makes navigable. All on one line, so a node stays one
entry however many batches it has.

This is where the per-batch record lives, rather than in a `batch-info.cpu.txt` for a chosen ten:
a separate file for a subset was worth it only while the figures were too bulky for every node,
and at a median of one batch and a p90 of six they are not. The tail is what to watch — the
worst source in the corpus is tpcds `inventory` at `bp-tp1-rowgroup` with 96 batches in one
lane, about two kilobytes across the two lists — so the comparator reports a moved section by
name and never dumps the line.

**A failing query is disabled with a ticket in [`bp-tickets.md`](../tasks/bp-tickets.md)**, not in the
main list. Rollout tickets arrive in bulk when a sweep hits a wall and close in bulk when it is
cleared, which is not what a triage pass reads for. The ID space is shared, so a number is never
two things. `TicketIndex::load` reads two files and gains a third here, and it is
not cosmetic: `cost-report` already exits 1 when a ticket the registry names resolves in neither
file, so the first rollout ticket filed in `bp-tickets.md` fails the cost-report job until the
index reads it.

**What proves the infra, since every way it fails is quiet.** A dropped section, a filtered
regeneration that deletes coverage, a skipped marker a reader cannot tell from an absent one — none
of these turn a run red on their own, so each gets a case that goes red on demand, built from
strings rather than by editing a golden and undoing it:

- two writers interleaved at the merge keep both sections, and the same case with the lock
  removed loses one — a lock nothing can be shown to need is a lock the next refactor drops.
  Forced rather than raced: two threads that merely start together prove nothing on a fast
  machine and fail on a loaded one, so the overlap is imposed — a barrier between read and write,
  or the merge driven directly with a second writer's section already on disk. Every case here
  is deterministic or it is not a case: this tier's whole claim is that a golden means something,
  and a test that passes on the third run is a golden that means nothing;
- a filtered regeneration rewrites its own sections and leaves every other byte identical;
- a section absent because its case did not run and one absent because the query stopped planning
  are distinguished, and the run that confuses them fails;
- a skipped marker round-trips: a query whose bit is clear writes one, and clearing a bit that was
  set turns a real section into a marker rather than deleting it;
- both of `cost-report`'s renderings carry the new table, asserted per rendering rather than once;
- the section reader and the per-file reader disagree on a two-section file whose second section
  moved, which is the cost gate's red case.

**Three things have to agree, and the inventory test becomes the place they do.** Legacy holds
the macro invocations and `cost-registry.csv` to each other in both directions, per binary,
because `inventory` collects per linked binary. The corpus tier adds a third leg: the goldens.
Every `corpus_query!` registration means an enabled cell, every enabled cell means a
registration, and every enabled (query, mode) means a golden section carrying content rather than
a skipped marker — with the converse holding too, since a `disabled` cell whose section is full
is coverage nobody is reading. The plan columns stay declared by the goldens alone, as they are
today; what is new is the execution columns being declared by the macro and checked against both.
One consequence to state rather than discover: the gpu half of this runs only on the gpu host,
because that is where its registrations are linked, so a gpu-column drift does not go red on the
cpu leg.

**Soundness is checked against the file's own redundancy, not with regular expressions.** A
regexp says a line looks like a line. What is wanted is whether the numbers mean anything, and
the format carries the same quantity more than once by design, so the checks are arithmetic and
cheap:

- a node's `out_rows` is the sum of its `batch_rows` lanes, and `out_bytes` of its `batch_bytes`.
  A batch is counted where it enters a queue, so a scatter output dropped for being empty is not
  one — the sums are unaffected either way, and the batch count then means what flowed;
- the lane count of those lists is the `lanes=N` on the node line;
- a node's `in_rows` for child *k* equals that child's `out_rows`, **per lane and not only in
  total**: the index is the *child's* lane, so it lines up with that child's own `batch_rows`
  entry for entry. Indexing by the consuming node's lanes would make the identity checkable only
  in aggregate wherever the two counts differ, which is every emitter, merge and accumulator —
  the nodes it is most worth checking. An equality on every run, because the node renders
  `abandoned` after its own `batch_bytes` — rows `release_in_flight` dropped, per lane, omitted
  where zero and so absent from every node of every run that drained. It sits on the node that
  *emitted*, because that is whose output queue was dropped, so the law crosses parent to child
  exactly as `in_rows` already does. The law is
  `consumed + abandoned == the child's emitted`, with no marker in it and no `<=` anywhere. The
  two exceptions this once named were measured and are both wrong: dropping a probe batch is
  consuming it, and a satisfied limit falls short at whichever node the schedule stopped at rather
  than at the limit;
- on a loader, `batch_rows` has `partition_groups`' exact shape, which is the correspondence that
  nesting was chosen for;
- the root's `out_rows` is the row count in `.result.txt`, which the oracle checked.

That last one is the interesting one, and the rest are worth more than they look. The cost tree
has no external oracle — the golden is written by the run it will later check — but a file that
contradicts itself is a file a renderer got wrong, and a renderer is most of what could be wrong.
It is a weak oracle rather than none, and it is free: the redundancy is already committed.

**Reading it back needs one parser, and there are already two and a half.** What exists:
`ordered_sections` and `section_differences`, which split on `== <query>` and report what moved,
private to `test_batch_partitioned_plans.rs`; `parse_node_line` in `common/cost_model.rs`, which
finds `output_bytes=` and takes the leading identifier, private, and enough for the cost
derivation and nothing else; and `read_total_str` in `cost-report`, another crate reading a
`key=` off a line. None of them knows a tree, a lane, or a nested list, which is what the
arithmetic above asks for.

So: the comparator moves into `tests/common/`, where both corpus binaries can reach it — and that
move is owed anyway, since `test_batch_partitioned_plans.rs` is at 1418 lines against a
thousand-line cap. `parse_node_line` grows from `(type, bytes)` into the node's fields and its
indent depth, and `cost_text_from_cpu` becomes a caller, so the cost derivation and the soundness
checks read one structure rather than two that agree by luck. `cost-report` stays separate, since
it reads a total out of `.cost.txt` and never a node line; what must not drift between the crates
is the `== <query>` convention itself, which is why it is written here rather than inferred from
whichever file a reader opens first.

A fourth parser is the thing to refuse. Three readers of one format already disagree about what a
node line is, and the format is about to carry four more fields.

**Consolidating them saves no code, and that is not the reason to do it.** All the parsing in the
tree is about ninety lines: `ordered_sections` is fifteen, `parse_node_line` seventeen,
`cost-report`'s `field` and `read_total_str` thirteen between them, and the rest is
`section_differences` reporting. A parser that knows a tree, a lane and a nested list is larger
than the three it replaces, so the change adds lines. What it buys is one definition of the
format at the moment the format gains four fields — the divergence is the cost, never the
duplication.

**The crate boundary is where consolidation stops, on purpose.** `cost-report`'s `[dependencies]`
is empty, and deliberately: it builds in seconds in the cpu tier because it pulls in neither the
executor nor a device. Making it depend on `peacockdb-core` to share thirteen lines of `key=`
reading trades a stated property for nothing, and a new workspace crate for the same thirteen
lines is worse. It needs the convention, not the code — `== <query>` sections, one
`peacockdb_cost=` per section.

So pin the convention with a fixture rather than with shared code: one committed two-section
sample that `cost-report`'s unit tests and the test-side parser's unit tests both read, each
asserting the same extracted values. The two readers then diverge red without either crate
depending on the other, which is the property that was actually wanted.

**An unordered `LIMIT` runs at every mode, and drops the DataFusion oracle rather than the
mode.** Legacy canonizes `scan_limit` at tp1 because at tp>1 its rows and its per-node bytes vary
run to run. That is a property of legacy's executor, not of this one:
[the determinism rules](#determinism-rules) pin the schedule and require that one plan run twice
gives one answer byte for byte, so a section here is stable at every mode. What those rules
explicitly do not promise is agreement *across* plans — tp1 and tp4 may return different rows
where the SQL does not determine which, which is what an unordered `LIMIT` is.

So the mode is fine and the comparison is what narrows: DataFusion single-stream is not an
authority on *which* ten rows a four-lane plan returns. It is still an authority on the rest, and
an unordered limit determines more than it looks. The count is `max(0, min(n, |unlimited| - m))`
for `LIMIT n OFFSET m` — written with the offset from the start, because the zero-skip form is a
rule that goes wrong on the first query that carries one and T20 adds those deliberately. And the
rows are a sub-*multiset* of the unlimited result, compared as a multiset and not as a set: set
membership passes a run that returned one row twice where the oracle has it once, which is a
live failure mode for a limit over a join. Both are asked of the session that
already runs, neither needs the two plans to agree on which rows, and together they catch what a
frozen golden cannot: a limit dropped, an offset ignored, rows invented, the wrong table sliced.

That is the third `cpu_oracle` value, and it is named for what it checks rather than for what it
declines to — the count and the containment, against `data_fusion_exact`'s whole answer. So no
query's answer is frozen with no external check, `scan-limit` included. The device is held to the
cpu's section on top of that, which is a real check rather than a fallback: both engines walk one
driver over one plan with emission order pinned, so disagreeing means one of them broke the
schedule.

Two limits to state with it. An *ordered* `LIMIT` with ties at the boundary is not covered:
neither `sort_unstable_by` nor cuDF's `sorted_order` is stable, so which tied row survives is
decided by neither engine's contract and cpu-gpu agreement is not owed. And the scope is small —
the determinism section counts one corpus query with a bare `LIMIT` whose rows are undetermined,
`tpch-queries/scan-limit.sql`; the other four limit an already-single-row aggregate.

**A query carries every mode it is correct at, and a ticket for the ones it is not.** Not a
default to be tuned: all five where all five run and agree with the oracle, and where some do
not, the ones that do are enabled and the rest are `disabled` against a ticket naming which modes
and why. That is what makes the mode arguments a set rather than a switch, and it is the same
rule on both engines — a query can be correct at five modes on the cpu and at two on a device,
which is two tickets and not one.

**The cost of running all of it is accepted for now.** 119 queries at five modes is about 595 cpu
runs and as many on a device, against a corpus measurement nobody has: T17a's report covers
eleven queries chosen for being the cheapest. Measured serially, one row per query: 45.9 s for 87
cpu runs on one thread, 26.2 s on the two CI uses, and a 1.34 GB peak of which one query is
1.28 GB. It gates nothing. The one thing that would force this open
again is a job that stops finishing: pipeline.yml's dataset-matrix leg has already been lost once at
fifty-eight minutes on a tier a fraction of this size, and the device leg runs
`--test-threads=1` on one host.

**A first enablement freezes whatever the run produced, and only half of it has an oracle.** The
answer does: `assert_cpu_results_match_datafusion` builds a separate plain-DataFusion session at
`target_partitions = 1` and compares against it unconditionally, before and regardless of any
golden write, so a wrong answer cannot be frozen — and that comparison is the standing check on
every later run too, not a first-enablement rite. The `.cpu.txt` does not: under a regeneration
it is written with no comparison, and nothing independent says the plan should have that shape or
that an interior node emitted that many rows. The root is anchored — its `out_rows` is the
result's row count, which the oracle checked — and every node below it is taken on trust.

What later covers some of it is the device asserting against the same file read-only, and it is
worth being exact about which half of that assertion carries information. The tree carries none:
`batch_partitioned_driver` is generic over the backend, so both engines walk one driver over one
plan and produce the same shape by construction rather than by agreeing. That is the design and
not a weakness of it — the walk being identical is what makes the two engines comparable at all.
The evidence is the rows and the bytes: the same walk, and the device's numbers against the
CPU's. The tree assertion stays anyway, because it costs nothing and goes red on the day that
construction stops holding, which is the only day it could ever say anything.

So enabling a hundred queries is an ordinary act for the answers, and for the interior of the
cost tree it rests on two engines producing the same counts through one walk. The mitigation is
the one T19 already has: batches of five, where a tree that looks wrong is still attributable.

**`build-test.md` gains rows, not only counts**: one per new tier in the category table, and
three in the golden table — the per-mode `.cpu.txt` and `.cost.txt` and the one `.result.txt` —
each naming the partial-regeneration variable beside `UPDATE_CANONICAL`. That table's existing
`.result.txt` row says the golden is deleted above 256 KB, which is the rule this task replaces,
so the row is split rather than edited: legacy keeps its sentence and the new mode states its own.

**And enough queries to prove the infra**: about ten tpch and about ten tpcds at sf1, enabled on
the macro as it is built — **on both engines**, at every one of the five modes each is correct at,
under the same rule and with the same tickets. The device half is not deferred to the rollout. An
infra whose gpu path has never run is an infra T19 would debug at the same time as its first
query, which is the thing staging this task exists to prevent, and the `gpu_oracle` argument,
`live_cpu`, the read-only assert against the cpu's section and the staging array are all surface
that means nothing until a device has walked it. They are chosen by plan size off the committed goldens rather than by
taste, because the constraint is not what can plan — all 22 tpch benchmark queries already plan
and attach recipes at all five modes, as do 81 of the 99 tpcds — but what is small enough that a
failure is legible. The tpch ten are the smallest carrying no ticket: q6, q1, q14, q19, q12, q13,
q15, q17, q3, q10. The tpcds ten are q41, q42, q3, q43, q52, q55, q96, q15, q37, q82. A legacy
ticket on a row is not automatically a blocker here — [#97](../tickets.md#t97) is the real-8-way
join blocker and this is a different executor — so those rows are held back as unknown rather
than as known bad, and T19 is where that is settled query by query.

| bench | enabled here |
|---|---|
| tpch | `q6` `q14` `q19` `q12` `q13` `q15` `q3` `q10` |
| tpcds | `q41` `q42` `q3` `q43` `q52` `q55` `q96` (tp1 only) `q15` `q37` `q82` |

Seventeen queries at 88 (query, mode) cases, not twenty: enabling them is what found that three do
not run. `tpch/q1` and `tpch/q17` are out at every mode on [#163](../tickets.md#t163) — `avg`
declares its count state `UInt64` and the accumulator produces `Int64`, which the ticket records
from the device and which the cpu reaches too. `tpcds/q96` keeps both tp1 modes and loses the
three tp4 ones to [#180](../tasks/bp-tickets.md#t180), where a shuffle puts a state merge under a
`count(*)` declared non-nullable.

The set is not topped back up to twenty. It was chosen as the smallest queries carrying no ticket
and two of them turned out to carry one, which is a fact about the corpus rather than a hole to
fill — and q96 partially enabled is worth more than a replacement would be, since it is the only
query here exercising the mode-scoped disablement the whole `cpu_modes`/`gpu_modes` shape exists
for.

~~**T19 — rollout.**~~ (done). Query-by-query enablement across the rest of the corpus on T18's macro,
starting from the twenty it already carries. No production code changes, per T18: a query that
does not run is disabled with a ticket in [`bp-tickets.md`](../tasks/bp-tickets.md), which is the whole
output of this task besides the enabled rows. Not one line of `peacockdb-core/src` moves here:
T18 left the goldens' own surface finished, so anything this task would have to change is by
definition a query's blocker rather than the rollout's.

**Four paths are edited by hand, one kind of file moves without being written, and a commit
touching anything else has stopped being a rollout.** By hand:

- the shared `corpus_query!` list, one file that every binary includes and reads through its own
  arm, as `gpu_cases.inc` is read today — so a query is enabled once, not once per engine;
- `testdata/cost-registry.csv`, cells rather than rows: all 138 corpus queries already have one;
- [`bp-tickets.md`](../tasks/bp-tickets.md), for what a query is disabled on — a whole query, or the
  subset of modes it is wrong at, which is the commoner case and the one whose ticket has to name
  the modes or the next reader re-derives them.

Every cell this task turns off carries a ticket, which is an invariant the registry already
holds: no row in it today has a `disabled` cell and an empty ticket list, and the four queries
DataFusion cannot plan at all carry [#23](../tickets.md#t23) against an `na`. A rollout is the
one thing that could break it, since it is the only task that turns cells off in bulk.

**The invariant proves less than it sounds like**, and the gap is worth knowing before leaning on it:
the tickets column is a per-row bag with no mapping to cells in either direction. A row already
carrying a legacy ticket satisfies "no disabled cell with an empty list" the moment a bp cell goes
off, with nobody having filed anything. Checked by hand at the rollout's close and the substance
holds — all 33 numbers resolve, and each of the nine rows disabling a strict subset of cpu modes has
a ticket naming those modes — but on six of the nine that ticket does not name the query, so a reader
of `tpcds/q5` opens four tickets to find which one applies.

**Two fixes ride along, both because the rollout is what breaks them.** The second is
[#194](../tickets.md#t194): `base_total` (`cost-report/src/main.rs:1623`) reads a base-side cost two
ways, and the git-ref arm — the one every PR uses — drops the `section` it was asked for and returns
the first `peacockdb_cost=` in the file. Legacy goldens are one file per query, so the two arms agree
there; the batch-partitioned per-mode files hold ~60 sections each, and this task is what fills them.
So every bp row on the cost widget is baselined against whichever query sorts first — measured on
`tpcds.sf1/bp-tp1-single-mini.cost.txt`, deltas from -99.6% to +341.3% against `q2`, none of them a
cost change. The git arm takes the same `entry_total(text, section)` path as the directory arm, and
the test is that the two arms agree on a multi-section file. The goldens are untouched: only the
baseline was wrong.

**The one test this task adds** asserts that every result section's
`mode=` is its query's last declared cpu mode, and goes in `test_corpus_goldens.rs`. Closing its
over-cap blind spot takes both halves. `over_cap`
(`common/corpus.rs`) writes the `skipped: ` prefix and no `mode=`, so an over-cap section records
everything except who produced it — and an over-cap section whose authority moved after a cut is
exactly the stale case this guard exists to catch. Four sections are over cap today, all in
`tpch.sf1/bp-mini.result.txt`: `q16` authored at `bp-tp1-rowgroup`, and `anti-join`,
`filter-project` and `semi-join` at `bp-tp4-sized`. They rewrite when their authoring mode runs, so
the guard's commit carries a four-section golden diff that the ticket has to explain.

`over_cap` gains the mode, **as its second line, after the marker**. The prefix stays first because
two readers use `starts_with(SKIPPED)` to mean *this section holds no rows*:
`corpus_gpu.rs:120` decides `frozen` by it, and moving the mode ahead of it would let a
`golden_exact` declaration pass against a section with no rows to compare. `corpus_gpu.rs:164` reads
`mode=` as the first line, but only for a `golden_*` oracle, which `assert_oracle_suits_the_golden`
has already refused for a marker — so an over-cap body never reaches it.

The guard therefore discriminates on **a `mode=` line being present**, not on `SKIPPED` being absent:
that set is the real sections and the over-cap ones, and it excludes `not enabled` markers, which
carry no author because none exists. It reads `ordered_sections` directly rather than
`sections_with_content`, whose prefix filter is what hid the over-cap case — and whose three
existing callers are undisturbed, since none of them wants the over-cap sections either.

Then the **golden sections** fill in: a query enabled at a mode stops carrying that mode's
skipped marker and starts carrying its plan, its per-batch sizes and its costs. That is eleven
files for the bench it belongs to — a `.cpu.txt` and a `.cost.txt` per mode, plus the one
`.result.txt` across modes — and twenty-two for a commit spanning both. The number is the point:
it is modes × benches and does not grow with the corpus, so the hundredth query enabled moves
the same eleven files as the first, where legacy would have added three more per (query, label)
and did, 277 of them for tpch.sf1 alone. And **`build-test.md`'s counts** move with the case
count — edited by hand like the three above, in the commit that moves them rather than in a later
sweep, which is why they are the fourth and not a second kind of thing that moves by itself.

**What it enables, and what stays out.** 120 of the corpus's 138 queries plan at all five modes
today, which is the eligibility test — not legacy enablement, since a query legacy disabled may
have been fixed since and the goldens are what know. Twenty are T18's, so a hundred are here:

| bench | enabled here |
|---|---|
| tpch (29) | `aggregate-groupby` `anti-join` `cross-join` `filter-project` `hash-join` `join-int` `left-join` `mixed-join` `nested-limits` `nested-loop-join` `nested-loop-left-join` `q11` `q16` `q18` `q2` `q20` `q21` `q22` `q4` `q5` `q7` `q8` `q9` `rollup-over-join` `scan-limit` `semi-join` `shuffle-additive` `shuffle-additive-avg` `shuffle-stddev` |
| tpcds (71) | `q1` `q10` `q11` `q13` `q14` `q16` `q17` `q18` `q19` `q2` `q21` `q22` `q23` `q24` `q25` `q26` `q29` `q30` `q31` `q32` `q33` `q34` `q35` `q38` `q39` `q4` `q40` `q45` `q46` `q48` `q5` `q50` `q54` `q56` `q58` `q59` `q6` `q60` `q61` `q62` `q64` `q65` `q66` `q68` `q69` `q7` `q71` `q73` `q74` `q75` `q76` `q77` `q78` `q79` `q8` `q80` `q81` `q83` `q84` `q85` `q87` `q88` `q9` `q90` `q91` `q92` `q93` `q94` `q95` `q97` `q99` |

`tpch/mixed-join` is CPU-only and the one query whose two engines differ: it plans, validates and
runs, and its recipes do not attach — [#168](../tickets.md#t168)'s interval `ScalarValue`, so the
crossing to a device is what fails, not the plan. `cpu_modes` carries all five and `gpu_modes`
none, which is the case the two mode arguments exist for.

The eighteen that stay out, every one against a ticket that already exists:

| held back | why | ticket |
|---|---|---|
| tpcds `q12` `q20` `q36` `q44` `q47` `q49` `q51` `q53` `q57` `q63` `q67` `q89` `q98` | the planner refuses `WindowAggExec` and `BoundedWindowAggExec` — the one capability the retired modes had and this one does not | [#143](../tickets.md#t143) |
| tpcds `q27` `q70` `q72` `q86` | DataFusion 45 does not physical-plan them at all (`plan_status=fail`) | [#23](../tickets.md#t23) |
| tpcds `q28` | a `DISTINCT` inside `count(DISTINCT …)`, refused by name | [#62](../tickets.md#t62) |

Window functions stay disabled and are not this task's to fix — [#143](../tickets.md#t143)
carries them.

**Seventeen of these hundred are already run by T17's tier, and the overlap is deliberate.**
`test_cpu_batch_partitioned` runs six queries at the five modes and eleven more at the modes plus
the injected shapes; every one of the seventeen falls in this task's list and none in T18's —
tpch `left-join` `q20` `q21` `shuffle-stddev` `anti-join` `nested-limits` `nested-loop-join`
`nested-loop-left-join`, tpcds `q38` `q87` `q2` `q8` `q16` `q33` `q45` `q93` `q97`. They are not
excluded, and a reader who finds the duplication should leave it.

The two tiers ask different questions of the same query: the injected one asks whether the
drivers tolerate a layout no planner would emit, over one plan; the corpus one asks whether the
answer, the plan and the costs match a golden, across the whole bench. Neither answers for the
other. And a corpus list defined by subtracting another list is a set held in prose — which is
exactly the defect T17a's completeness pass found in its own fixture list and closed by
generating `INJECTED` from one declaration. Reintroducing it one level up, so that removing a
query from the injected tier would silently need a matching addition here, buys a duplicate mode
run and costs the property that made the smaller list trustworthy.

**A device cell's ticket names its FIRST failure, not its only one.** The causes are ordered by how
far a plan gets: [#183](../tasks/bp-tickets.md#t183) is the unload refusing an export, at the end of a plan
that ran, and [#152](../tickets.md#t152) is a join refusing its second probe batch, earlier. So a
mode whose join takes one probe batch reaches the unload and fails on the string, while a mode whose
join takes two never gets there — the same query reporting two different causes, chosen by the
partitioning rather than by its shape. `q18` is #152 at four modes and #183 at one.

Any count of cells per ticket is therefore a count of first failures. Fixing one moves cells to
another column rather than turning them green, and a cell's ticket is where a reader should start
rather than the whole of what stands in the way.

**The same shape appears on the cpu side, where the loser is not merely unmeasured but invisible.**
Three ordering questions have turned up in the rollout — #152 versus #183, #152 versus #175, and
#175 versus #189 on `tpcds/q77`, whose rollup made the hasher a candidate at exactly the three tp4
modes where the empty build side refuses first. A disabled cell runs nothing, so whether #189 would
also have rejected q77's grouping-set id cannot be read off the corpus at all; the question is not
open pending a measurement, it is closed to measurement until #175 is fixed. Any claim that a ticket
has been exhausted by the corpus should be read against that.

**The sixth batch showed the rule cleanly.** Four unrelated queries — a semi-join with a correlated
subquery, a five-way join, an eight-way join, an anti-join over a self-join — all fell the same way:
#183 at `bp-tp1-single` and #152 at the other four modes, without exception. `bp-tp1-single` is the
only mode whose joins happened to get a single probe batch, so it was the only one reaching the
unload — a correlation the eleventh batch retired, below.

The eighth batch narrowed that. #152 has two rows and only one is mode-sensitive: the build-side
copy is about a second probe batch erasing what the first consumed, so a single probe batch avoids
it, while the probe-side copy is refused at any batch count — one probe batch is still one copy.
Which half a query meets is a property of its JOIN TYPE, inner copying the build and outer the
probe: `tpcds/q97`'s outer join refuses at `bp-tp1-single` too and never reaches a second cause.

The eleventh batch replaced the mode shorthand with a batch-count rule — #152's build half fires
whenever a copying join's probe side arrives in more than one batch — and the fourteenth falsified
it. `tpcds/q11` at `bp-tp1-single` has eleven Inner joins, all copying, with probe batch counts
1,1,1,336,1,336,1,88,1,88,1 measured and recorded before the device ran; it reaches the unload and
fails on #183. A copying join took 336 probe batches at that mode and did not refuse.

So what decides it is **not known**. `tpcds/q71` and `tpcds/q2` refuse at `bp-tp1-single` and q11
does not, and all three have copying joins with streaming probe sides there — whatever separates
them is not something the goldens carry. The likeliest next question is what feeds the build side
rather than what feeds the probe, since a build that is another node's handle is consumed by its
first reader where one a scan re-materializes per call is not; that is readable in `cpp/src` and in
the recipe writer rather than in the corpus, and it belongs to #152 rather than to a rollout.

Three narrowings and one falsification, recorded because the alternative is a fourth version that
fits every batch so far. What survives is that #152's two halves differ by join type.

So the device column currently measures #152's reach rather than the engine's, and the two tickets
are ordered: fixing #152 moves most of those cells to #183 rather than turning them green, and #183
is one defect at one site. Those two, in that order, are what stand between this rollout and a
device column that is not empty — with the caveat the seventh batch added: where
`bp-tp1-single` gets past the join at all, it is not the mode that passes but the mode that gets far
enough to find out what else is wrong, and each query it reaches has its own answer. q7 fails there on a string, q2 on a decimal, q8 on an integer,
all at the unload. So #183 is not one defect at one site but one site with a family of type
mismatches under it, and #187 and #191 are two more of that family rather than separate work.

**A mode disabled after its file was last regenerated has no marker section**, and both golden
checks report it as an absence rather than as the disablement it is. The merge writes markers for
every declared section of a file, so regenerating any *enabled* query in that file fills the missing
ones in. Neither test's message says so, and a batch that only disables modes therefore looks
broken until an unrelated query is re-run.

**#163 is what decides where this rollout lands, and the number is knowable in advance.**
Seventeen of the fifty-six tpcds queries remaining after T19's ninth batch carry `avg`, whose count
state that ticket disables at every mode on both engines. So roughly a third of the tail produces no
cells at all: the ceiling is a hundred minus those seventeen, minus whatever else refuses whole,
which put it near **83** rather than near a hundred — with #163 being most of the gap, as
[#152](../tickets.md#t152) is most of the device column's. Neither is a rollout failure and neither
is fixable here.

**It closed at 68 fully enabled, 8 partially, 24 out entirely, 0 unreached** — 357 of 500 possible
cpu cells, and **zero** device cells beyond the six T18 left. So 76 rather than the 83 predicted, and
that miss is worth more than the number: the ceiling was computed twice, corrected from a wrong 88-to-90
down to 83, and was still high, because both computations counted only the blockers known at the
time. [#190](../tasks/bp-tickets.md#t190) and [#192](../tasks/bp-tickets.md#t192) arrived after the projection. **A
ceiling derived from known causes is a bound on optimism, not a prediction** — unknown blockers only
ever subtract.

Seventeen `avg` queries were projected against #163 after the eleventh batch and all seventeen were
RUN rather than declared. None failed to refuse and none refused for a different reason, which is
what makes the tail a measurement rather than an assumption.

**Roll out in batches of about five, not in one sweep.** A hundred queries enabled at once is a
regeneration whose diff nobody reads and a failure nobody can attribute: eleven golden files move
either way, so the batch size is the only thing that says which query moved which section. Five
is small enough that a red run names its cause without bisecting and large enough that the
regeneration cost is amortised. Each batch is its own commit — the five queries, their registry
cells, the sections they filled, and whatever went to `bp-tickets.md` — so the history reads as
the rollout it was, and a batch that goes wrong is reverted without taking the ninety-five with
it.

Nothing else, and the list is still short enough to read off a diff: no `cost_model.conf`,
because T18 enters all eighteen node kinds at once rather than as each is first seen — an
exhaustive set entered piecemeal is one whose next gap is a rollout's problem; no `pipeline.yml` or `test_ci_coverage`,
because the targets exist by then; no `registry.rs`, whose columns are T18's.

That sweep is where T17's `nested-loop-left-join` gets its GPU columns. They ship `na` because
T17 could not commit a device run, which is [#116](../tickets.md#t116)'s shape — a cell with no
coverage and no blocker — so the reason is recorded here rather than left for a reader to
reconstruct from an empty column.

**Four tasks were specced during T19 and none of them is T20's.** They came out of what the rollout
found, and each closes tickets the device column is actually blocked on rather than shapes the corpus
lacks: [`refcounted-tables.md`](../tasks/refcounted-tables.md) for [#145](../tickets.md#t145) and
[#152](../tickets.md#t152), [`casts.md`](../tasks/casts.md) for [#183](../tasks/bp-tickets.md#t183),
[`wire-schema.md`](../tasks/wire-schema.md) for [#187](../tasks/bp-tickets.md#t187), and
[`empty-answers.md`](../tasks/empty-answers.md) for [#173](../tickets.md#t173) and
[#175](../tickets.md#t175). Order matters twice: `casts.md` before `wire-schema.md`, which writes the
schema `empty-answers.md` then reads. Between them they cover the two causes that hold 134 of the 138
disabled device rows.

**T20 — corpus shapes the benchmarks do not have.** Not done, and not a task any more: it is
[#195](../tickets.md#t195), which carries the audit, the six shapes and the engine work each
one needs.

**T22 is obsolete as written (2026-09-08).** `peacock_gpu_benchmarks` and the record tree it
wrote were deleted with the legacy modes, so there is nothing to port: a per-node measurement
for this mode starts from the protocol below rather than from that harness. The design
question it names — a node runs once per batch per lane, so a per-node figure is a sum over
calls and the call count belongs beside it — is the part worth keeping.

**T22 — per-node benchmarks for the new mode.** Port `peacock_gpu_benchmarks` to the
batch-partitioned executor, keeping the protocol that makes its numbers comparable: one
discarded warm-up, ten measured runs, the **2nd-smallest by `total_us`** reported whole, and
the floor measured over 200 samples. The run counts stay compile-time constants
(`tests/common/mod.rs`) so every record in the tree was taken at the same ones.

**A node is called many times here, and that is the port's one real design question.** The
legacy record carries one `time_us` per node, or one per partition — a node runs once. In
this mode a node runs once per batch per lane, so a per-node figure is a *sum over calls*
and the call count belongs beside it; a node at 40 ms over 200 calls and one at 40 ms over
two are different findings, and a record that cannot tell them apart measures nothing useful.
`CallStats` is already returned per call and is where the per-call figures come from.

The record gains the mode and the tier — `<query>.<mode>.benchmark.txt` alongside the legacy
`<query>.<mode>-<tp>-<tier>` — and keeps `build_profile`, `sync_floor_us`,
`nodes_at_or_below_floor` and the rest, since they mean the same thing. It also carries which
allocator measured the run: the pool landed with
[#151](../archive/archived-tickets.md#t151), [#148](../tickets.md#t148) is still open, and a
number taken without one is not comparable with a number taken with one.

Case list as the correctness tiers use, so the measured set cannot drift from the verified
one, and `test_ci_coverage` exempts the target explicitly because it asserts nothing.

---

<!-- archived from llm-wiki/tasks/schema_and_validation.md -->

# schema registry and validation (T7/T8 remainder)

**Goal.** Finish the two tasks whose implementations landed early on
`ENS-bp-plan-skeleton` but whose test surfaces did not: prove the `Schema` carried on every
node is right, and make `validate_schemas_and_partitions` a check that can go red for the
reasons it claims rather than only for the ones a real plan happens to hit.

**What already landed, so nobody rebuilds it.** `Schema` exists with `group_keys` and
`agg_state`, every node carries a populated one, the plan goldens print the declared type per
column, and node-local validation is called from `plan_batch_partitioned` with all ten
goldens passing it node by node. That half was pulled forward by a review finding — ten
guards existed and none ran on the live path.

## T7 — what the schema tests must show

- A hand-built plan carrying a project, an aggregate and a union produces the expected types
  and the expected semantics annotations, asserted on the tree rather than on rendered text.
- The annotations survive the aggregate sequence: `agg_state` is right at the init, at the
  per-lane merge and at the finalizing merge, which are three schemas for one logical
  aggregate.
- **Decimal precision and scale through project, aggregate and union-cast.** This is the
  one that earns the task: `avg`'s state columns were once typed backwards and per-node
  bytes could not show it, because both engines derive them from the same plan schema — so
  CPU and GPU agreed on the same wrong number and only a real divide would have diverged.

## T8 — what validation still owes

- The generic structural pass, over and above the per-node checks that exist.
- Manually constructed wrong combinations: each rule turned red by an input built to break
  it, per the reviewer's anchor that a guard which cannot go red is not a guard.
- Validation run over every canonized corpus plan as a standing check, not as a one-off.
- Defects in the checks themselves, including any the reviewer reported against
  `ENS-bp-plan-skeleton` and I deferred here.

**Constraints.** Every committed golden plan passes the validation this task adds. A
rejection is a planner defect until shown otherwise: stop, report it, and fix planning —
never weaken the check to fit the plan, and never regenerate a golden to silence one. The
expectation is still that no plan moves, so a golden that does move is a deliberate decision
taken with the human rather than a side effect of the regen. A test that only passes is worth
less than one shown to fail on the defect it guards.

**Verification bar.** Every rule in `validate_schemas_and_partitions` has an input that turns
it red. Decimal fidelity is asserted at each step of project → aggregate → union-cast. Every
committed golden plan passes validation, and a golden moves only where a planner fix required
it. `test_ci_coverage` names whatever targets appear.


Merged 2026-08-18 as PR #121.


---

<!-- archived from llm-wiki/tasks/join_capability_recipe.md -->

# join capability through the recipe plan (T0 extension)

**Goal.** Establish, by execution rather than argument, that the frozen FlatBuffers schema
and C++ operators can run **every** join mode in the batch-partitioned model — with the
build side complete and the probe side arriving in batches — and write the per-mode
lowering down where the implementation tasks will read it.

**What was built** (all in `scripts/exec_model/`, coordinator-owned):

- `operators/join_types.py` — `JoinType` in the fbs vocabulary and the capability matrix as
  one function, so no backend can hold a different opinion of it.
- `operators/cudf_calls.py` — the cuDF calls `cpp/src/operators/join.cpp` makes, at their
  own signatures: joins that return gather maps, `gather` with its out-of-bounds policy,
  `scatter`, `apply_boolean_mask`, `cross_join`.
- `operators/recipe.py` — the fb node structs, a handle registry that consumes on read as
  `NodeSession` does, and the node implementations mirroring join.cpp branch for branch.
- `operators/recipe_join.py` — the second join backend: every call answered by emitting fb
  seqs and making `execute_node` calls.
- `operators/joins.py` — the pandas backend, widened from five join types to all nine plus
  cross and nested loop.

**Constraints.** The two backends share no join code; agreement between them is the
evidence. The recipe backend may not reach for python where the frozen surface has no
answer — it names the gap and counts what working around it costs (`copy_handle`).

**Verification bar.** `scripts/exec_model/tests/test_end_to_end.py`: every join mode
against a SQL oracle, on both backends, at five batching/partitioning configs and across
the layout injector's presets; the emitted seq sequence asserted per mode against the
spec's table; the per-batch copy counts asserted per family; every refused shape refused
loudly on both backends. Whole prototype suite green, ~90 s.

**Outcome.** Recorded in the spec's [join capability
matrix](../architecture.md#capability-matrix): every mode is expressible,
the streamed-probe copy cost is [#152](../tickets.md#t152) quantified per family, and one
shape turned out to be a defect in the shipping engine rather than a limit of the mode —
[#153](../tickets.md#t153).


---

Merged 2026-08-04 as PRs #112 / #115 / #114.


---

<!-- archived from llm-wiki/tasks/build-test-flags.md -->

# Task: build-test.sh flag surface, failure semantics, and the regen guard

On `ENS-test-exec-mode`. Touches `scripts/build-test.sh`, `scripts/build-test-shadgpu.sh`,
`peacockdb-core/tests/test_plan_bytes.rs`, `llm-wiki/build-test.md`. The rules this
implements are already in `coding-style.md` ("Bash: the flag set is an interface, and
failure is fatal", b05a631) — that bullet was written from this script's defects.

No golden may move. No test may be added, removed or re-tiered.

## A — flag surface

1. **Delete `--cpu`.** It sets the default and has no callers anywhere in the repo.

2. **`--gpu` and `--rust-only` are mutually exclusive** and must be *rejected*, not
   resolved by argument order. Today:
   - `--rust-only --gpu` → `MODE=gpu` with `RUST_ONLY=1` still live, so the run branch
     sets `LD_ENV=":"` and the GPU binaries never get `LD_LIBRARY_PATH` — they fail to
     resolve `libpeacock_gpu` and it reads as a product fault.
   - `--gpu --rust-only` → the reverse, GPU silently ignored.
   Same two flags, opposite outcomes, no warning. Error naming the contradiction.

3. **Replace `--push-testdata` / `--pull-testdata KIND[,KIND]`** with per-kind flags:

       --push-{parquet,queries,goldens,duckdb-profiles,duckdb-dynfilters}
       --pull-{parquet,queries,goldens,duckdb-profiles,duckdb-dynfilters}

   The point is structural, not cosmetic: the argument parser *becomes* the validator.
   No comma splitting, no kind lookup at the call site, and an unknown kind is just an
   unknown flag caught by the existing `*) usage` arm before any side effect. The
   multi-kind form is generality nobody uses — every documented invocation moves exactly
   one kind. `testdata_dirs_for_kind` stays as the kind→dirs map; only its use as a
   validator goes.

4. **`--fetch-goldens` → `--pull-goldens`.** A rename, not an alias. Today it appends
   `goldens` to `PULL_TESTDATA`, so passing it alongside `--pull-testdata goldens` pulls
   twice. Keep the current ordering property: the flag must be resolved *before* the
   `--host` requirement check, so `--pull-goldens` alone still demands `--host`.

5. **`--rsync` → `--push-binaries`, in BOTH scripts.** `--rsync` names the tool rather
   than the intent. `build-test.sh`'s own first line says it mirrors
   `build-test-shadgpu.sh`, so renaming one and not the other breaks the parallel that
   makes either readable after time away.
   - It **still pushes goldens.** They are part of the payload, not an optional kind:
     binaries shipped without the fixtures they assert against is the trap that produced
     110/110 "canonical file not found". Requiring `--push-goldens` alongside would
     replace a footgun with a guarded footgun.
   - `--push-goldens` remains useful and is not redundant with it — it is the subset
     operation, refreshing fixtures without rebuilding or reshipping binaries.

6. **Require an action.** `--host x` with no `--build`/`--push-binaries`/`--run`/push/pull
   currently does nothing and exits 0.

7. **Value-taking flags check their value exists.**

8. **`usage()` states what is deliberately absent**, so the next reader does not file it
   as a gap:
   - no `--pull-binaries` — binaries flow one way, built locally and shipped;
   - `embeddings-cache` is a per-host intermediate for the tpch vector datasets
     (`fetch_embeddings.sh`, ~1.8 GB, gitignored) and is deliberately not syncable.
   Also state the mode ladder: rust-only ⊂ cpu ⊂ gpu, and that a mode which builds more
   never runs less.

## B — failure semantics

1. **`set -euo pipefail`.** `pipefail` is load-bearing: `cargo test --no-run … | python3`
   currently takes python's status, so a cargo failure is caught only by the explicit
   emptiness check afterwards.

2. **An empty derived suite is an error.** Verified: `mapfile -t A < <(helper)` with a
   helper that outputs nothing yields a zero-length array, the `for` body never runs, and
   the script exits 0. A typo in the derivation would silently run no tests and report
   success.

3. **All validation before the first side effect** — a bad flag must fail before anything
   is built, shipped or deleted.

4. **The remote heredoc keeps its deliberate `set -e` omission.** Running every test
   binary and accumulating `rc` is correct: a failing C++ test must not skip the Rust
   ones. This is the stated exception `coding-style.md` allows; keep the comment that
   says why, and keep the non-zero exit at the end.

## C — deduplicate the sync layer

1. **One `sync_goldens()`.** "Push goldens" exists twice with different flags: the
   `--push-binaries` block uses `rsync -r --delete`, `--push-goldens` uses
   `rsync -a --delete`. Same intent, different metadata handling, no shared code.

2. **Push mirrors (`--delete`) uniformly; pull is additive uniformly** — and the
   asymmetry is deliberate, so document it rather than "fixing" it. The remote is a
   *partial* mirror: `testdata/goldens/` contains `tpch.sf40/` (16 CSVs) and sf40 lives
   on shad-gpu, so mirroring downward from verda would delete fixtures that host never
   had. The destination is a git working tree.

   Known consequence, accepted: a regen deletes a `.result.txt` when a result exceeds
   256 KB (`maybe_write_result_golden`), and an additive pull cannot propagate that. The
   deletion is already announced on stderr and reaches the operator through the ssh
   heredoc — that is the handling, not a `--delete` flag armed for one rare case.

3. **Drop the `*.txt` filter on the goldens pull** — after A/B and D, not before. Its real
   job is keeping `plan_bytes.sha256` out of the round trip, which becomes the self-guard's
   job in D; keeping both would be two mechanisms for one invariant with the weaker one in
   the wrong place. Removing it also stops silently dropping the 16 sf40 CSVs.

4. **Clear `cpp/install/rust-tests` before staging**, matching `build-test-shadgpu.sh`.
   `build-test.sh` does not, so orphaned binaries from a previous mode accumulate;
   today that is mitigated only by running binaries by explicit name.

## D — move the regen guard into the test

`--update-canonical` exports `UPDATE_CANONICAL=1` to every staged binary, so the run set
doubles as the regen set and `regen_excluded()` subtracts `test_plan_bytes` back out.
That protects one invocation path only: `UPDATE_CANONICAL=1 cargo test --features
rust-only -p peacockdb-core --test test_plan_bytes` — the exact command the golden's own
header prints — still rewrites `plan_bytes.sha256` silently.

Move the refusal into `test_plan_bytes.rs`: under `UPDATE_CANONICAL`, refuse unless a
dedicated override (`PEACOCK_REGEN_PLAN_BYTES=1`) is also set, panicking with the reason —
the digests are the wire-format guard, the C++ side reads those bytes, and regenerating
rewrites the evidence instead of failing. Then **delete `regen_excluded()`**; the script
stops carrying knowledge about a test's internals.

`test_cost_model` stays in the regen set. That inclusion is a fix, not a risk: `.cost.txt`
derives from `.cpu.txt`, which a regen rewrites, so the old six-target list left every
`.cost.txt` stale and `test_cost_model` went red immediately after a "successful" regen.

## E — comments and docs

- Header comment says "Two suites" and then lists three.
- The `PUSH_TESTDATA` comment lists four kinds; `usage()` lists five (`duckdb-dynfilters`).
- The goldens-push comment justifies itself entirely in verify terms and disposes of the
  regen case in a parenthetical, which reads as "this push is redundant when
  regenerating" — the opposite of what is true. State both reasons: for a verify run the
  binaries assert against these files; for a regen the push establishes the baseline, so
  the pulled-back set is local-committed ∪ regenerated rather than remote-leftovers ∪
  regenerated, and `--delete` is the mechanism.
- `build-test-shadgpu.sh:176` references "the goldens that build-test.sh's --rust-only
  mode used to skip" — check it still says something true.
- `llm-wiki/build-test.md`: the verda row, the shad-gpu row (`--rsync` → `--push-binaries`),
  the golden-regen bullet, and a line recording that `embeddings-cache` is not syncable.

## Sequencing

**A+B → D → C → E.** A/B is the interface and the failure model and touches everything
later; D must land before C3; C is mechanical once D is in; E last, so the docs describe
the final state rather than an intermediate one.

## Verification

- `bash -n` on both scripts.
- A flag matrix: for each mode, print the derived suite and assert it matches; for each
  rejected combination, assert it errors non-zero. Include `--gpu --rust-only` in both
  orders, `--host` with no action, and a value-taking flag with no value.
- Prove the empty-suite error fires (temporarily break the derivation, confirm non-zero,
  restore).
- One real `--rust-only --build` to prove staging still produces binaries.
- `git status --porcelain testdata/goldens` must be empty at the end.

## Out of scope

`maybe_write_result_golden`'s discarded `remove_file` result and its unconditional "no
golden" message — a ticket, not this task. The `--cpu`/`--rust-only` naming axis is
resolved by A1/A2 and needs no further rename.


---

<!-- archived from llm-wiki/tasks/widget-golden-links.md -->

# Task: link CPU ✓ cells to their goldens; small-font Query and Σout columns

Branch `ENS-widget-golden-links` (off `ENS-test-exec-mode`). Cost-report widget only —
`cost-report/src/main.rs`. No changes to the registry CSV, the test suite, or any golden.

## 1. The premise, verified

Every `enabled` cell in `ftc_tp1`, `ftc_tp8` and `partitioned_cpu` **has a committed
`.cpu.txt`**. `assert_cpu_cost_canonical` runs unconditionally on every CPU macro
invocation (`common/mod.rs:847`), before and independent of the `ResultGolden` keyword —
that keyword gates only `.result.txt`. So a `✓` in those three columns always has a
golden to point at.

This does *not* extend to the GPU columns: those read the CPU golden rather than owning
one, so leave `full_table_gpu` / `partitioned_gpu` cells alone.

## 2. Which golden each ✓ points at

The device label is **not** in the CSV, and it is not one-per-column:

| Column | Golden label |
|---|---|
| `ftc_tp8` | `full_table-tp8-mini` |
| `partitioned_cpu` | `partitioned-tp8-standard` |
| `ftc_tp1` | `full_table-tp1-standard` |

**Corrected 2026-08-04.** This section originally said `ftc_tp1` is tp1-standard "except
`scan_limit`, which is `full_table-tp1-mini`". That is wrong, and the developer caught it:
`scan_limit` has **both** goldens. It is registered twice — `tp1_mini` at
`test_cpu_full_table.rs:24` and `tp1_standard` at `:188` — and `column_for` keys on the tp
count, not the memory tier, so both land in the single `ftc_tp1` cell. It is not the
exception to the column's label; it is the one query whose cell aggregates two runs.

**Decision: one label per column, `full_table-tp1-standard`.** It is the label every tp1
row uses including `scan_limit`, so the link target is predictable from the column alone.
Drop the tp1-mini candidate rather than carrying config that nothing can reach — this repo
already treats unreachable config as a hazard in its own right (`GOLDEN_INVARIANT_EXEMPT`
and `INTENTIONALLY_NOT_IN_CI` both carry staleness assertions for exactly that).

What a cell aggregating two runs should render is a real question, but a hyperlink cannot
express it and the answer would change the cell shape. Out of scope here; raise it as its
own task if the tp1-mini run ever needs to be reachable from the widget.

The fail-loud check below is what makes dropping the candidate safe: a future query
registered ONLY at tp1-mini has no golden under the single label, so the widget fails
naming it instead of rendering a dead link. That is strictly better than a silent second
candidate, because it forces the decision rather than guessing.

Link target: the same `links.golden_url(canon_rel, stem, "<label>.cpu.txt")` helper the
Σout cell uses, so dry runs with no sha degrade to plain text exactly as they do today.

Only `✓` becomes a link. `~`, `✗` and `—` stay plain — there is no golden behind them.

## 3. Small font

- **Query column:** non-numeric queries only — `aggregate_groupby`, `scan_limit`,
  `shuffle_stddev`, `hash_join`, … The discriminator already exists: `Row::number` is
  `None` for exactly these (it is `Some(n)` for `q<N>`). Numbered queries keep today's
  size.
- **PeacockDB Σout and DuckDB Σout:** small font for **both the header and the values**.
  The Ratio column is not in scope.

Both renders. They use different mechanisms and the difference is load-bearing: the HTML
report can use a CSS class (`th.modeh` is the precedent), while the PR-comment table must
use `<sub>` because GitHub strips `class`/`style` — see the `mode_cells_md` doc comment.

## 4. Watch for

- `MODE_COLUMNS` and `registry.rs::COLUMNS` are the CSV header contract. This task is
  display-only: do not touch either, and do not change any cell value.
- `ftc_cell()` currently renders `tp1✓ tp8✓` as one string inside a single `<td>`. Both
  glyphs need to become independently linkable, so that function has to return markup
  rather than a plain label — check its callers in both renders before changing its shape.
- `CPU_DEVICE` is the const the Σout cells resolve through. The new per-column labels are
  related but not the same thing; do not fold them together in a way that makes a Σout
  change silently move a mode link.

## 5. Verification

- `cargo test -p cost-report` — the widget's own unit tests, including the
  `mode_cells_md` shape asserts.
- `scripts/cost-report-preview.sh` and eyeball the generated HTML: a linked `✓` per
  enabled CPU cell, `scan_limit`'s tp1 link resolving to `full_table-tp1-mini`, small-font
  micro-query names, small-font Σout headers and values.
- Confirm the PR-comment render still parses as HTML on GitHub (`<sub>`, no class/style).
- No golden, CSV or test-suite file appears in `git status`.


---

<!-- archived from llm-wiki/tasks/test-exec-mode.md -->

# Task: execution mode explicit in test macros and golden filenames

Branch `ENS-test-exec-mode` (off `ENS-llm-wiki`). Pure refactor: **no behavior change, no
golden regeneration, no coverage change**. Every test that runs today must still run, with
the same assertions, reading the same bytes from renamed files.

## Why

Two things are inferred today that should be stated:

1. `common/mod.rs::partition_mode(device)` maps the string `"tp8-standard"` to
   `PartitionMode::RealMultiPartition` and everything else to `SinglePartition`. A device
   label silently picks an executor. Adding a device (say `tp8-mini` as a real 8-way tier,
   which #91 wants) would route it to the wrong executor with no diff to the routing code.
2. Golden filenames carry only `tp<N>-<tier>`, so `q15.tp8-mini.cpu.txt` does not say which
   CPU executor produced it. The mode is real: the same plan at tp8 produces a different
   per-node cost tree under full-table vs partitioned execution.

`node13` is an obsolete task number used as an executor name. It goes.

## 1. Macro renames

| Old | New |
|---|---|
| `cpu_result_test!` | `cpu_full_table_result_test!` |
| `cpu_result_approx_test!` | `cpu_full_table_result_approx_test!` |
| `cpu_node13_result_test!` | `cpu_partitioned_result_test!` |
| `cpu_node13_result_approx_test!` | `cpu_partitioned_result_approx_test!` |
| `gpu_test!` | `gpu_full_table_test!` **and** `gpu_partitioned_test!` |

`cpu_result_error_test!` / `cpu_result_fits_test!` keep their names — decision, not
oversight: they take a raw budget rather than a device, read no mode-tagged golden, and the
resident-OOM enforcer they drive is full-table-only by construction (#91 tracks porting it).
Say that in a one-line comment where they are defined so the asymmetry doesn't read as a
miss.

Generated test-fn names include the mode, so two modes at the same device can never collide:
`cpu_full_table_tpch_sf1_q1_tp8_mini`, `cpu_partitioned_tpch_sf1_q6_tp8_standard`,
`gpu_full_table_tpch_sf1_q1_full_table_tp1_standard`. Grep `pipeline.yml` and the
`build-test*.sh` scripts for `--exact` / filter strings built from the old names and fix them.

## 2. Call-site shapes

CPU — mode from the macro name, device stays `tp<N>_<tier>`:

    cpu_full_table_result_test!(tpch, 1, q1, tp8_mini, no_result_golden);
    cpu_partitioned_result_test!(tpch, 1, q6, tp8_standard, result_golden);

GPU — mode from the macro name; the device argument is the **combined golden label**, which
is the golden filename component verbatim:

    gpu_full_table_test!(tpch, 1, q1, full_table_tp1_standard, golden_exact);
    gpu_partitioned_test!(tpch, 1, q3, partitioned_tp8_standard, golden_exact);

So `gpu_full_table_test!(tpch, 1, q1, full_table_tp1_standard, …)` reads
`goldens/tpch.sf1/q1.full_table-tp1-standard.{cpu,result}.txt` — reconstructible from the
invocation with no lookup. The GPU run config splits cleanly: `PartitionMode` comes from the
macro name, `tp` + budget are parsed out of the label. A crossed pair
(`gpu_full_table_test!` with a `partitioned_…` label) is visible at the call site; assert it
cannot pass silently — the label's mode prefix must equal the macro's mode.

## 3. Last parameter: bool → enum

The trailing `$gen:literal` bool becomes an ident backed by a real enum in `common/mod.rs`,
so the call site names the artifact it produces:

    pub enum ResultGolden { Write, Skip }   // `result_golden` | `no_result_golden`

Mirror `gpu_result_mode`'s shape (keyword → enum, unknown keyword panics with the accepted
set). Keep the existing invariant comment about when writing is correct — it is the useful
part of that parameter.

## 4. Delete the implicit routing

- Delete `partition_mode(device: &str)` outright.
- `assert_cpu_results_match_datafusion`: replace `use_node13: bool` with an explicit
  execution-mode parameter that carries both the executor choice and the `PartitionMode`.
- `assert_gpu_query` / `assert_gpu_nodes_match_golden`: take the `PartitionMode` from the
  caller (the macro), never from a label.
- `plan_is_node13_executable` → `plan_is_partitioned_executable`, unchanged behavior; it
  stays the safety assert on the partitioned path.
- `registry.rs::column_for`: kind `"node13"` → `"partitioned"`; kind `"gpu"` splits into
  `"gpu_full_table"` / `"gpu_partitioned"` mapping straight to their columns instead of
  sniffing `device.starts_with("tp1")`. The `ftc` arm keeps its tp1/tp8 split — that one
  reads a `tp` count out of a `tp` label, which is parsing, not routing.

### The plan tier keeps a label → mode lookup (amended 2026-08-04)

"Delete `partition_mode` outright" was wrong about the plan tier, and the developer caught
it. `test_plan_bytes.rs::corpus()` builds its entire corpus by reading `.plan.txt` filenames
off disk and splitting the device out of the stem — there is no call site to state a mode at,
and `plan_bytes.sha256` keys on `<query>.<device>`, which this task freezes. The mode is
load-bearing there: `shuffle-additive` @ tp8-standard is the one plan golden whose shape
depends on `RealMultiPartition`.

So: `partition_mode` is deleted from the **execution** path — CPU results, GPU query, GPU
nodes, and the oracle `CpuExecutor` all take the mode from the macro name. One narrowly
scoped `plan_partition_mode(device)` survives for the plan tier, documented as that tier's
label contract and made **exhaustive** — an unknown label panics instead of falling through
to `SinglePartition`. That catch-all is the actual hazard this task's "Why" names, and
because `corpus()` reads labels off disk, exhaustiveness is self-enforcing: a new plan golden
with an unlisted device fails loudly instead of silently planning single-partition.

Keep it to **one** mechanism. Do not also add an explicit mode argument to `query_plan_test!`
or `plan_for` — two sources for the same fact is how a plan golden gets generated under one
mode and its byte digest under the other. Concretely: `test_query_plan_misc.rs:27`
(`plan_for(…, "tp8-standard")`) is unchanged, while `test_cpu_executor_misc.rs:41` is an
execution-tier site and does pass `PartitionMode::RealMultiPartition` explicitly.

The `coding-style.md` entry names this as a deliberate exception, so the surviving function
reads as scoped rather than missed.

**CSV columns and `testdata/cost-registry.csv` do not change.** `COLUMNS` is the committed
fixture's header contract (`load_csv` asserts on it) and `MODE_COLUMNS` in cost-report keys
off the same names. Renaming `ftc_tp1` is a separate change; not this one.

## 5. Golden filenames

New: `<query>.<mode>-<tp>-<tier>.{cpu,cost,result}.txt`, e.g.
`q15.full_table-tp8-mini.cpu.txt`.

**Unchanged:** `.plan.txt` (plan shape is executor-independent, and
`goldens/plan_bytes.sha256` pins those names) and `.duckdb_cost.txt` (oracle, no peacock
executor involved).

`git mv` only — do not regenerate. The mapping is total and collision-free because no device
label is used by both modes today (verified: `tp8_standard` appears only under
`cpu_node13_*`; every other label only under `cpu_result_*`):

| Old | New | files |
|---|---|---|
| `*.tp8-mini.{cpu,cost}.txt` | `*.full_table-tp8-mini.…` | 129 + 129 |
| `*.tp1-mini.{cpu,cost}.txt` | `*.full_table-tp1-mini.…` | 1 + 1 |
| `*.tp1-standard.{cpu,cost,result}.txt` | `*.full_table-tp1-standard.…` | 110 + 110 + 104 |
| `*.tp8-standard.{cpu,cost,result}.txt` | `*.partitioned-tp8-standard.…` | 18 + 18 + 14 |

634 files total across `testdata/goldens/{tpch.sf1,tpcds.sf1}`. Counts are mine and worth
re-deriving — if yours differ, say so before renaming rather than after. Verify content is
untouched: the multiset of file *contents* must be identical before and after (e.g. compare
sorted `sha256sum` values of the renamed set).

## 6. File reorganization — by mode, not by memory tier

| Now | Becomes |
|---|---|
| `test_cpu_executor.rs` (tp8-mini + tp1-mini + tp8-standard) | `test_cpu_full_table.rs` (tp8-mini, tp1-mini, tp1-standard) |
| `test_cpu_h200.rs` (tp1-standard) | `test_cpu_partitioned.rs` (tp8-standard) |
| `test_gpu.rs` | `test_gpu_full_table.rs` + `test_gpu_partitioned.rs` |

`test_cpu_executor_misc.rs` / `test_gpu_executor_misc.rs` keep their names; they only need
their golden paths updated.

Registry ownership follows the files — update both the `registry_matches_csv_*` fns and their
doc comments:

- `test_cpu_full_table.rs` → `ftc_tp1` + `ftc_tp8`
- `test_cpu_partitioned.rs` → `partitioned_cpu`
- `test_gpu_full_table.rs` → `full_table_gpu`
- `test_gpu_partitioned.rs` → `partitioned_gpu`

The cross-binary caveat in `test_cpu_h200.rs`'s doc comment (scan_limit registered at
tp1-mini in one binary, tp1-standard in another) is moot once both live in
`test_cpu_full_table.rs` — delete it rather than carrying it forward. The note at the foot of
`test_gpu.rs` explaining why the cross-mode invariant lives in `test_query_plan.rs` is still
load-bearing; keep it on whichever GPU file you consider primary.

`test_gpu.rs`'s file-level doc explains the merged one-run-asserts-both design — that belongs
in both new GPU files or in the macro doc, not dropped.

## 7. Consumers that read the old names

- `cost-report/src/main.rs`: `CPU_DEVICE = "tp8-mini"` → `"full_table-tp8-mini"`. Check the
  surrounding comment about scan_limit being tp1-mini-only (~L345) — it still holds, but its
  wording names devices.
- `registry.rs::assert_cross_mode_golden_invariant`: the `("full_table_gpu", "tp1-standard")`
  / `("partitioned_gpu", "tp8-standard")` pairs become the new labels. This one is
  load-bearing — it is what catches an enabled GPU mode with no CPU golden.
- `test_ci_coverage.rs`: four new test-target names in, two out. This gate is the reason a
  renamed binary can't silently drop out of CI.
- `.github/workflows/pipeline.yml`: rust test target lists in the cpu-cpu tier and the
  GPU-remote job; `scripts/build-test.sh` and `scripts/build-test-shadgpu.sh` build/stage
  test binaries one `--test` at a time and name them.
- `test_cost_model.rs` globs `*.cpu.txt` and derives the sibling `.cost.txt` by string
  replace — should need no change; confirm rather than assume.
- `llm-wiki/build-test.md` and `architecture.md`: test-file names, device labels, golden
  naming. Same commit.

## 8. Wiki changes owed by this task

`coding-style.md` — new antipattern entry, in the voice of the existing thread-local one:
implicit routing from a label. `partition_mode(device)` turned the string `"tp8-standard"`
into `RealMultiPartition`; the executor a test ran was a side effect of how its golden was
named, and a new device label would have silently taken the wrong path with no diff to the
routing code. State the mode at the call site and pass it as a parameter.

`build-test.md` — one sentence: a refactor that must not change behavior is verified with a
representative subset (one query per mode/tier per binary) plus the full rust-only tier, not
a full CPU/GPU suite run; the goldens are the invariant.

## 9. Verification — subset only, explicitly

Do **not** run the full CPU or GPU suite.

1. The rust-only **golden/meta gates**, must be green — named by target, not by package:

       cargo test --features rust-only -p peacockdb-core \
         --test test_plan_bytes --test test_cost_model --test test_ci_coverage

   plus the registry tests, which live inside the execution targets and must be filtered to
   (`--test test_query_plan -- registry_`, and the same for `test_cpu_full_table` /
   `test_cpu_partitioned`). Together: the CSV contract, the cross-mode golden invariant, and
   the CI-coverage gate. Cheapest and highest-value gate for this change; seconds, not
   minutes.

   **Corrected 2026-08-04** — this item originally read `cargo test --features rust-only -p
   peacockdb-core` with no `--test`, which contradicted this section's own headline: nothing
   cfg's the CPU execution targets out of the rust-only build, so package-wide sweeps all 241
   + 18 of them. `--features rust-only` selects a *build*, not a tier; only `--test` selects
   the tier. `build-test.md`'s table lists the bare package command as the rust-only loop,
   which is what made this easy to mis-transcribe — the §8 sentence owed to `build-test.md`
   should make the distinction explicit rather than just saying "run a subset".
2. `cargo build --tests` clean, no new warnings.
3. CPU subset — a few per binary, chosen to cover each device label:
   `test_cpu_full_table` at tp8-mini, tp1-mini (scan_limit), tp1-standard;
   `test_cpu_partitioned` at tp8-standard (include one approx: tpcds q17).
4. GPU subset on shad-gpu — one per new binary is enough (e.g. tpch q1 full-table, tpch q6
   partitioned). This proves golden-path resolution on both binaries; it is not a
   correctness run.
5. **Test-count invariant**: capture `--list` output for the affected binaries before and
   after and show the sets correspond 1:1 under the intended renames. A refactor that
   silently drops a test is the failure mode here, and neither a green subset nor
   `test_ci_coverage` alone would catch a dropped `gpu_test!` line.

## Out of scope

CSV column renames; `ftc` as a kind name; porting the resident-OOM enforcer to the
partitioned driver (#91); regenerating any golden; re-tiering any test.

## 10. Follow-up (added 2026-08-04): fold the approx variants into an oracle argument

Human's instruction, on this same task rather than a new one.

The `_approx_` macros exist only to pass `Some(1e-12)` instead of `None` for `rel_tol`.
That is a property of how the result is compared, not a different kind of test, and
spelling it in the macro name means two names per mode where one plus an argument says
more. Delete `cpu_full_table_result_approx_test!` and `cpu_partitioned_result_approx_test!`
and add a **second-to-last** argument to the two surviving macros:

    cpu_full_table_result_test!(tpch, 1, q1, tp8_mini, data_fusion_exact, no_result_golden);
    cpu_partitioned_result_test!(tpcds, 1, q17, tp8_standard, data_fusion_approximate, result_golden);

Backed by a real enum in `common/exec_mode.rs`, keyword-mapped like `ResultGolden` and
`gpu_result_mode` (unknown keyword panics naming the accepted set):

    pub enum CpuOracle { DataFusionExact, DataFusionApproximate }

`DataFusionExact` → `rel_tol = None`, `DataFusionApproximate` → `Some(1e-12)`.

The name states what the oracle IS, which the old name did not: **both** variants compare
against a live plain-DataFusion run at `target_partitions = 1` (`build_session_state(1)`);
only the float tolerance differs. Nothing about the oracle changes — this is a rename of
an existing bool-in-disguise.

Move the 1e-12 rationale — float summation reassociates across partitions at tp>1, ~1 ULP,
while the `output_bytes` cost golden stays exact because a ULP does not change byte width —
onto the `DataFusionApproximate` variant, where `ResultGolden` and `GpuResultMode` keep
theirs. Do not leave it stranded on a deleted macro.

**The five call sites that become `data_fusion_approximate`** (every other CPU call site
takes `data_fusion_exact`):

| File | Query |
|---|---|
| `test_cpu_full_table.rs:65` | tpch `shuffle_stddev` @ tp8-mini |
| `test_cpu_full_table.rs:85` | tpcds `q14` @ tp8-mini |
| `test_cpu_full_table.rs:109` | tpcds `q39` @ tp8-mini |
| `test_cpu_full_table.rs:226` | tpch `shuffle_stddev` @ tp1-standard |
| `test_cpu_partitioned.rs:38` | tpcds `q17` @ tp8-standard |

Note `tpcds q14 @ tp1-standard` is **exact** today and stays exact — at tp1 there is no
reassociation. Converting it along with its tp8-mini sibling would silently loosen a check.

**Test-fn names do not change.** The approx macros already generated the same
`cpu_<mode>_<ds>_sf<sf>_<query>_<device>` pattern as the exact ones, so the 259/259
correspondence must still hold exactly. Re-run the `--list` comparison and say so; a
changed count here means something other than the intended edit happened.

Verification is §9 unchanged, and no golden may move: `rel_tol` affects only the result
compare, never `assert_cpu_cost_canonical`.
