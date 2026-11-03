# The driver runs a plan in which one source feeds several consumers and several roots answer

Kind: production

The driver runs trees. Each node owns its children as `Box<dyn GpuNode>`. `executor/driver/index.rs`
numbers nodes pre-order with a single `parent`. `run` takes one root and pushes every unloaded batch
into one result list. To fuse several queries' plans, so their table loads are shared, the driver
must run a DAG: a `GpuLoadParquet` source read by several consumers, with one root per query.
**Only a source may be shared.** Merging any other node is out of scope here. query-fuser reports
how much it would save, without doing it.

This is production code with unit tests, not part of the prototype. A single-query plan is the
one-root, unshared case, and must run exactly as today.

## Done when

1. **A plan may have several roots and shared sources.** Each root's batches are returned
   separately, in the order that root alone would produce them. A source with k consumers loads
   each batch once. Every consumer reads it without a device copy, through refcounted ownership
   of the same columns: an additive ABI call that duplicates a handle over shared owners, or an
   equivalent the builder argues for. `CpuBatch` gets the same, by an explicit cheap clone of
   its Arc-backed arrays.
2. **Every place that assumes a tree is made DAG-correct**, or proven unaffected, each with a
   test:
   - indexing: several parents, several roots, height, and subtree ranges that are no longer
     contiguous;
   - the scheduler's re-check after a step, which reaches every parent;
   - output queues: one read per consumer, and a batch dropped only after its last consumer;
   - memory accounting: one hold and one release per shared batch, released at the last
     consumer, so `holds == releases` and `in_flight_bytes == 0` still hold;
   - the plan-time memory estimator, which charges a shared batch until its slowest consumer;
   - validation and the output hook, which visit a shared node once;
   - display: a shared node printed once, and referenced from each further consumer;
   - the wire format: one seq space across the fused plan, a shared node numbered once on both
     the Rust writer and C++ `index_post_order`, and several roots.
3. **Join holds cannot deadlock across roots.** The scheduling rule holds a join's probe subtree
   until its build is ready, and its no-deadlock argument rests on "plans are trees"
   (`architecture.md` "The scheduling rule"). Two cases are covered:
   - two roots whose builds each wait on a source the other holds;
   - one join reading the same shared source on both sides.

   The answer is the **hold buffer** below. A plan with a buffer on the held reader's path runs
   both cases. A plan without one is refused at validation with a named error. `architecture.md`
   carries the new argument.
3a. **A hold buffer node, `GpuHoldBuffer`.** It sits on one edge. While a join's hold covers it,
   it takes every batch its child emits and emits nothing; when the hold lifts, it emits them in
   arrival order. **A hold stops at a buffer**: the buffer itself is held, and nothing below it
   is. So its subtree runs while the build runs, and what waits is the buffer's input, after
   whatever filters and projections lie below it.
   - It is a Rust-side node: it keeps the handles it was given and calls nothing on the device.
     No C++ or ABI change.
   - The memory accountant holds its batches like any other held batch, and the estimator charges
     the buffer for everything its child emits.
   - Display shows it as its own node. It has no wire node, since it makes no device call (as `GpuUnion`); "From node to seqs" gives it its own row.
   - On lanes: one queue per lane.
   - No single-query plan contains one, so every single-query plan schedules exactly as today.
3b. **The only unbounded storage is a node you can see.** A queue between two ordinary nodes stays
   self-bounding at one batch per lane, as `architecture.md` "Queues need no cap" says today.
   Validation refuses any plan where an edge into a held region carries batches from a producer
   outside it and nothing on that edge or below it, inside the region, is a hold buffer. That
   includes a shared source read from inside a hold. Unbounded storage exists only in
   accumulators and hold buffers, which the plan shows and the estimator charges. This rule goes
   into `architecture.md` "The scheduling rule", in those words, as a rule every later driver
   change keeps.
3c. **`plan::validate_dag(roots, cap)`, one entry point that proves the DAG deadlock-free** before
   anything runs. `run_roots` calls it first, and the fuser calls it on every plan it builds. It
   checks, in order, and refuses with a named `PlanError` that names the nodes:
   - the structure: every root a sink and no sink elsewhere; only a `GpuLoadParquet` shared;
     acyclic;
   - rule 3b, for every join's hold and, under a cap, for every root that may wait;
   - **the wait-for graph is acyclic.** A hold H (a join in its build phase, or a capped root)
     waits for hold H′ when a node H's release needs (its build region, or the root's own
     region) lies inside H′'s held region, holds stopping at buffers. A cycle is refused as
     `PlanError::HoldCycle` listing the holds in order. Under 3b the argument says no cycle can
     form, but the check is kept: it is the mechanical form of the argument, and what a later
     rule change breaks first.

   A property test ties the check to the driver. Random DAGs, shared sources and buffers placed
   at random, run on the CPU or mock backend. Every DAG `validate_dag` accepts runs to completion,
   with `holds == releases`. Every DAG on which the driver stops with work left, caught by a step
   bound, is one `validate_dag` refused.
4. **A limit's early exit is per consumer.** One root satisfying its limit does not stop a
   shared source another root still reads. The source stops when every consumer is satisfied or
   done.
5. **The driver takes an optional concurrency cap:** at most k roots' subtrees past their shared
   sources run at once, and the rest wait. query-fuser uses it to keep a large batch inside device
   memory. Without the cap, scheduling is the existing rule extended to several roots, and stays
   deterministic.
6. **Tests:**
   - CPU-backend unit tests for each item above;
   - GPU tests of a two-root, one-shared-source plan, and of three roots over two shared sources,
     each root's answer byte-identical to that query run alone;
   - a test that a shared batch is held once and released once;
   - hold buffers: the subtree below a held buffer runs during the build; the buffer emits in
     arrival order once the hold lifts; a plan with an unbuffered reader of a shared source inside
     a hold is refused by name; the two deadlock shapes of item 3 with buffers answer as alone;
   - `validate_dag`: each refusal by name, and the property test of item 3c;
   - TPC-DS q95's shape, one join reading `web_sales` on both sides, from the real plan, fused
     with a buffer, answering as q95 alone.

   Every existing golden (plan, `.cpu.txt`, `.cost.txt`, recipe payloads, device tier) is
   byte-identical, because no existing plan has a shared node.
7. `architecture.md` "Execution" and "The wire format" describe the DAG rules.

## Scope

| path | change |
|---|---|
| `peacockdb-core/src/plan/` | shared children: `Arc` or arena ids; `children()` |
| `peacockdb-core/src/executor/driver/`, `executor/mod.rs`, `executor/accounting.rs` | indexing, scheduling, queues, roots, limits, accounting, the cap |
| `peacockdb-core/src/planner/memory_estimation.rs` | shared-batch lifetime |
| `peacockdb-core/src/plan/validate.rs`, `plan_text/` | single visit, display |
| `peacockdb-core/src/wire/`, `flatbuffers/gpu_plan.fbs`, `cpp/src/node_session.cpp` | several roots; DAG numbering |
| `cpp/include/peacock_gpu.h`, `cpp/src/` | the handle duplicate, additive |
| `llm-wiki/architecture.md` | the DAG rules |

Component-level API:
- the driver's entry point accepts several roots and returns per-root results;
- one additive C ABI symbol (handle duplicate);
- the wire schema gains several roots, additively, so a one-root plan's bytes are unchanged.

## Constraints

- The planner is unchanged. Nothing in this task builds a DAG from SQL; the tests build them by
  hand.
- tp1_single is the mode query-fuser uses, but the driver rules hold for every mode, and the
  tests cover at least one multi-lane mode.
