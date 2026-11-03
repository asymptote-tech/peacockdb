# dag-driver implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** the driver runs a plan with several roots in which a `GpuLoadParquet` feeds several
consumers, loading each batch once, with per-consumer limits, no deadlock across roots, and an
optional cap on how many roots run at once; a one-root tree runs exactly as today.

**Architecture:** sharing is a type, not a convention: `plan::SharedSource` wraps an
`Arc<GpuLoadParquet>` and is the only node that two parents may hold, so "only a source may be
shared" needs no run-time check. Every walk that assumed a tree goes through one new
abstraction, `plan::PlanDag` (first-visit pre-order across the roots, post-order, longest-path
heights, readers per node, and *regions*: the nodes all of whose paths to a root pass through a
node or an edge). On a tree a region is today's contiguous subtree, so every number the goldens
read is unchanged. Queues get one cursor per reader; a non-last reader takes a `Batch::share`
(a new `peacock_handle_duplicate` on the device, an Arc clone on the CPU) and the last reader the
original. The entry's accounting charge is one, refcounted over its holders (the queue cursors
still to take it, a buffer store, a consumed input), and released when the last holder drops it,
not when the last reader takes it. Deadlock across roots is answered by
a Rust-only node, `GpuHoldBuffer`: a hold that lifts (a join's, the cap's) stops at a buffer,
which keeps what its child emits while held; validation refuses any edge into such a hold from
a producer outside it that does not end at a buffer, so no ordinary queue ever grows past one
batch per lane.

**Tech stack:** Rust (peacockdb-core, peacockdb-ffi), C++17/cuDF (`cpp/`), flatbuffers
(`flatbuffers/gpu_plan.fbs`), gtest.

**Spec:** [`dag-driver.md`](dag-driver.md). Read with it: `architecture.md` "Execution" (Traits,
The scheduling rule, Early exit at a limit, Memory accounting) and "The wire format".

## Global Constraints


- **VM time is minimal** (the human's rule, 2026-11-03). A VM starts right before its first
  remote command and stops right after its last `pull`. It is never left up across a wait: a local
  build, CI, a review, the human. Binaries are built and staged first, and the H100 starts only
  once they are ready to ship. No step leaves a VM running for a later task. Each start and stop
  goes into the detail file's `## VM time` table, in the host's local time, with the hours per VM
  for the task. The watchers (1 h idle, 4 h per process, the hourly GitHub check) are the backstop,
  not the plan.
- Code base: master's engine. containers-rebase does not touch the driver; nothing here reads
  `origin/ENS-containers`. Chain order: containers-rebase, new-hosts, pinned-loads, bench-rows,
  sf40-baseline, dag-driver, query-fuser, batching-report, tp4-sized — bench-rows lands first, so
  `bench_text.rs` already has its `render_timings(root, times, report)` and `timed_load_us`.
- Developers never commit, push, or dispatch a workflow (prompts.md). A step that says
  *Commit `<msg>`* means: report a checkpoint `<msg>` and stop; the coordinator stages, commits and pushes, then
  dispatches the next step. Only the plan's last step is green. A step marked *(coordinator)* is the coordinator's; the developer stops before it
  and says what it needs.
- Device steps run on nebius-gpu, the L40S, which is for correctness (build-test.md, Nebius host,
  host roles); this task runs no sf40 and no benchmark.
- Every commit keeps every existing golden byte-identical: plan, `.cpu.txt`, `.cost.txt`,
  `recipe-payloads.txt`, device tier. No single-query plan contains a `GpuHoldBuffer`, so every
  single-query plan schedules exactly as today. Each task's last step runs the golden check below.
- The planner is unchanged. Nothing builds a DAG or a buffer from SQL; tests build them by hand.
- Additive only at the boundaries: one new C ABI symbol (`peacock_handle_duplicate`); one new
  fbs field (`GpuPlan.roots`), unset for a one-root plan. `GpuHoldBuffer` changes no C++, ABI or
  fbs: it makes no device call.
- **The only unbounded storage is a node you can see** (spec 3b). A queue between two ordinary
  nodes stays at one batch per lane; nothing in this plan lets a reader's queue grow to absorb
  a hold.
- Driver rules hold for every mode; tests cover tp1_single and at least one multi-lane mode
  (`tp4-sized` on the CPU, 4 lanes in the mock).
- coding-style.md: files < 1000 lines (split `partitioned.rs`, now 998, before it grows — Task
  4); a component's API is declared in its `mod.rs` with one-expression delegates; tests in
  `foo/tests.rs`; no re-exports; rustfmt on touched leaf files only; `git clang-format` on
  changed C++ lines.
- `architecture.md` and `build-test.md` change in the commit whose code they describe (case
  counts in build-test.md's tier headers and rows included).

**Golden check** (run at the end of every task):

```
cargo test --features rust-only -p peacockdb-core --lib                       # all pass
cargo test --features rust-only -p peacockdb-core --test test_corpus_goldens  # the count recorded at Task 1 Step 1, plus this task's additions
cargo test --features rust-only -p peacockdb-core --lib -- planner::tests::plan_goldens
git status --short testdata/                                                    # empty
```

## Review Focus

1. A failed call or a budget trip while shared batches are queued for several readers, or kept
   by a held buffer: each entry is released once, `holds == releases`, `in_flight_bytes == 0`.
   → Task 4, `a_failed_call_with_batches_buffered_still_reconciles`; Task 5,
   `a_failed_call_with_a_shared_batch_queued_still_reconciles`.
2. A root that receives nothing in a fused run answers one zero-row batch under *its own*
   sink's input schema, not root 0's. → Task 5, `a_root_that_receives_nothing_answers_its_own_empty_batch`.
3. `LIMIT 0` on one root over a shared source: its reader is retired before the first step,
   and the other root still gets every row. → Task 6, `a_limit_zero_root_retires_its_reader_before_any_step`.
4. A limit satisfied above a held buffer: the buffer's kept batches are released, not emitted,
   and its subtree stops. → Task 6, `a_satisfied_limit_releases_what_a_buffer_below_it_keeps`.
5. A shared source at 4 lanes read by a lane-scoped node and by an emitter: every reader gets
   every batch of every lane, in lane order. → Task 5, `a_four_lane_shared_source_reaches_every_reader_on_every_lane`.

---

### Task 1: `peacock_handle_duplicate` and `Batch::share`

**Files:**
- Modify: `cpp/include/peacock_gpu.h:299-300` (after `peacock_handle_release`), `cpp/src/plan_executor.h:165` (`NodeSession`), `cpp/src/node_session.cpp` (beside `slice_handle`, :743), `cpp/src/gpu_executor.cpp:301-320` (pattern), `peacockdb-ffi/src/lib.rs:270`
- Modify: `peacockdb-core/src/executor/mod.rs:48-61` (`Batch`, its doc, `CpuBatch`'s doc), `executor/cpu_batch.rs`, `executor/gpu_batch.rs`, `executor/tests.rs:13` (`HandleBatch`), `executor/driver/tests/mock.rs:47` (`MockBatch`)
- Test: `cpp/tests/gpu/test_plan_executor.cpp`, `cpp/tests/cpu/test_executor.cpp`, `peacockdb-core/src/executor/gpu_backend/gpu_tests/abi.rs`, `peacockdb-core/src/executor/tests.rs`
- Docs: `architecture.md` Traits ("neither batch type is `Clone`…"), "Three additive ABI symbols" → four (`:1027`), and the ABI's symbol count (`:1116`, "twenty-five symbols in six groups" on master, one more after containers-rebase's `device_info`) gains `peacock_handle_duplicate`, in words, in the group it joins.

**Interfaces:**
- Produces (C): `int peacock_handle_duplicate(peacock_executor_t* executor, uint64_t handle, uint64_t* out_handle);` — a new handle over a copy of the `TableResult` (owners' `shared_ptr`s copied, no device allocation), NOT consuming `handle`; 0 on success; non-zero on null args, no plan, or unknown handle (unknown handle ends the query, as `slice_handle` does). Under timing the new handle's producer is the original's.
- Produces (C++): `uint64_t NodeSession::duplicate_handle(uint64_t handle);`
- Produces (Rust): `pub trait Batch { …; fn share(&self) -> Result<Self, BackendError> where Self: Sized; }`. `CpuBatch::share` = `RecordBatch::clone` (Arc arrays); `GpuBatch::share` calls the symbol and keeps `producer`, `num_rows`, `byte_size`; `MockBatch`/`HandleBatch` copy their fields.

- [ ] **Step 1: failing tests.** First run the golden check once on the parent branch and record
  its `test_corpus_goldens` pass count in the detail file (master's 28 plus the chain's
  additions); later golden checks compare with it. C++ GPU: `HandleDuplicate.SharesOwnersAndAllocatesNothing` — upload a 1000-row int64+string table, duplicate under the RMM statistics adaptor: bytes allocated during the call == 0; release the original; `result_from_handle` of the duplicate == the uploaded rows. `HandleDuplicate.EachHandleIsReleasedOnItsOwn` — release the duplicate, the original still exports. `HandleDuplicate.IsChargedToTheOriginalsProducer` — timing on, produce a handle at seq s, duplicate, export the duplicate: the export region carries seq s. `HandleDuplicate.AnUnknownHandleIsRefused` — rc != 0, `last_error` names the handle. C++ CPU: `Lifecycle.DuplicateWithNoPlanIsRefused` — rc 1, `"no plan loaded (call peacock_executor_begin_plan first)"`. Rust gpu: `abi.rs::a_duplicate_reads_the_same_rows_after_the_original_is_released`. Rust cpu (`executor/tests.rs`): `a_shared_cpu_batch_shares_its_buffers` — every column's `to_data().buffers()[i].as_ptr()` equal between the batch and its share.
- [ ] **Step 2:** `cargo test --features rust-only -p peacockdb-core --lib -- executor::tests::a_shared_cpu_batch` → FAIL (no `share`).
- [ ] **Step 3:** implement. `duplicate_handle` copies the registry entry and calls `register_handle(copy, sink, producer_of(handle))`.
- [ ] **Step 4:** CPU: the Rust test passes; `ctest -L cpu` in `cpp/build` passes (+1). Device, **nebius-gpu** (default of `nebius.sh`): the "GPU step, end to end" of build-test.md "The scripts" (`start`, `nebius-sync.sh`, `build-test-shadgpu.sh --build`, `nebius-test.sh run`, `status` until `ALL rc=0`, `stop`). Pass = `ALL rc=0` and the five new cases listed in `.gpu-test/` logs as OK.
- [ ] **Step 5:** golden check; commit `executor: peacock_handle_duplicate and Batch::share`.

### Task 2: `SharedSource`, `PlanDag`, and validation over roots

**Files:**
- Modify: `peacockdb-core/src/plan/mod.rs:1107-1136` (`GpuNode` gains `sharing_key`), plus declarations of `SharedSource`, `PlanDag`, `DagNode`, `Reader` and their one-expression delegates
- Create: `plan/shared.rs` (`impl GpuNode for SharedSource`), `plan/dag.rs` (the walk and regions), `plan/dag/tests.rs`
- Modify: `plan/validate.rs:24-73` (walks via `PlanDag`), `plan/validate/tests.rs`
- Create: `peacockdb-core/src/tests/share.rs` (fixture), declare in `src/tests/mod.rs`

**Interfaces:**
- Produces:
  - `pub(crate) struct SharedSource { load: Arc<GpuLoadParquet> }` with `#[derive(Debug, Clone)]`, `pub(crate) fn new(load: GpuLoadParquet) -> Self`. Its `GpuNode` impl delegates `kind`, `children` (empty), `validate_schemas_and_partitions`, `row_interval`, `as_any` (→ the inner `GpuLoadParquet`, so `name()`, `as_node_ref` and every backend match see a load). A parent holds `Box::new(shared.clone())`.
  - `GpuNode::sharing_key(&self) -> Option<usize>` — default `None`; `SharedSource` returns `Arc::as_ptr(&self.load) as usize`. Identity for every walk: the key if `Some`, else the node's address.
  - `pub(crate) struct Reader { pub(crate) parent: usize, pub(crate) slot: usize }`
  - `pub(crate) struct DagNode<'a> { pub(crate) node: &'a dyn GpuNode, pub(crate) children: Vec<usize>, pub(crate) readers: Vec<Reader>, pub(crate) post_order: usize, pub(crate) height: u32 }` — `children` may repeat an index (one join reading one source on both sides); `readers` in first-seen order.
  - `pub(crate) struct PlanDag<'a> { pub(crate) nodes: Vec<DagNode<'a>>, pub(crate) roots: Vec<usize> }`, indexed in first-visit pre-order, roots walked in the order given; `post_order` counted at the end of a first visit (on a tree: today's numbering).
  - `PlanDag::of(roots: &[&'a dyn GpuNode]) -> PlanDag<'a>`; `PlanDag::in_post_order(&self) -> Vec<usize>`; `PlanDag::region_of_node(&self, node: usize) -> Vec<usize>`; `PlanDag::region_of_edge(&self, parent: usize, slot: usize) -> Vec<usize>` — ascending. (Task 4 makes `region_of_edge` stop at a buffer.)
  - `pub(crate) fn validate_roots(roots: &[&dyn GpuNode]) -> Result<(), PlanError>` and `pub(crate) fn check_canonical_form_of_roots(roots: &[&dyn GpuNode]) -> Result<(), PlanError>`; `validate(root)` and `check_canonical_form(root)` delegate with `&[root]`. Every root must be a `Sink`, and a `Sink` anywhere else is refused: `"{name}: a sink feeds another node — every GpuUnload is a root"`.
  - test fixture `src/tests/share.rs`: `pub(crate) fn share_sources(roots: Vec<Box<dyn GpuNode>>) -> (Vec<Box<dyn GpuNode>>, usize)` — rebuilds each tree with `tests::rebuild::rebuild`, replacing every `GpuLoadParquet` whose `(table, projection, partition_groups, schema)` equals one already seen by a clone of that one's `SharedSource`; returns the trees and how many sources ended up shared.

Heights are the **longest** path to a root, so every reader of a node is strictly lower than it
(the push rule: all readers drain before a producer runs again). A region is computed by
deciding descendants in ascending height: a node joins when it has a reader and every reader's
parent (for an edge region, the edge itself at the start) is already in. Regions are not
contiguous on a DAG; on a tree `region_of_node(n) == (n..end)` and `region_of_edge(J, 1)` is the
probe subtree.

- [ ] **Step 1: failing tests** (`plan/dag/tests.rs`, plans from `executor/driver/tests/plans.rs`): `a_tree_numbers_as_the_walk_always_did` — over every fixture of `tests::rebuild::every_kind()`: pre-order, post-order, heights == depth, `region_of_node(n)` is the contiguous `n..end`. `a_shared_source_is_one_node_with_two_readers` — `unload(filter(S))`, `unload(project(S))`: 5 nodes, S's readers `[(1,0),(3,0)]`, S's post-order 0. `two_roots_number_in_root_order` — roots `[0, 3]`. `a_shared_node_sits_below_its_deepest_consumer` — S under depth 1 and depth 3: height 4. `a_region_stops_at_a_node_read_from_outside_it` — `region_of_node(root1)` excludes S and is not a contiguous range. `a_join_over_one_source_on_both_sides_holds_nothing_below_its_probe_edge` — `region_of_edge(J, 1) == []`. `validate/tests.rs::the_validation_walk_visits_a_shared_node_once` — the private walk-order helper over two roots names `GpuLoadParquet` once; `a_sink_below_a_root_is_refused`.
- [ ] **Step 2:** `cargo test --features rust-only -p peacockdb-core --lib -- plan::` → FAIL to compile.
- [ ] **Step 3:** implement as above.
- [ ] **Step 4:** the new tests pass.
- [ ] **Step 5:** golden check; commit `plan: SharedSource, PlanDag, validation over roots`.

### Task 3: the index and the scheduler over a DAG

**Files:**
- Modify: `executor/mod.rs:929-997` (`JoinShape`, `PlanShape`, `IndexedNode`, `PlanIndex`), `:999` (`ROOT` stays, for the one-root renderers until Task 9)
- Modify: `executor/driver/index.rs` (rebuilt on `PlanDag`; `subtree_ranges` goes), `executor/driver/mod.rs:48-51`, `executor/driver/scheduler.rs`, `executor/driver/partitioned.rs:233-245` (`step` re-checks every reader)
- Test: `executor/driver/index/tests.rs`, `executor/driver/scheduler/tests.rs`

**Interfaces:**
- Consumes: `PlanDag`, `Reader` (Task 2).
- Produces:
  - `PlanShape { heights: Vec<u32>, lanes: Vec<usize>, regions: Vec<Vec<usize>>, readers: Vec<Vec<Reader>>, joins: Vec<JoinShape>, roots: Vec<usize> }` — `regions[n] = region_of_node(n)` replaces `subtree`.
  - `JoinShape { node: usize, probe: Vec<usize>, lanes: usize }` — `probe = region_of_edge(node, PROBE_CHILD)`.
  - `IndexedNode` loses `parent: Option<usize>`, gains `readers: Vec<Reader>` and `child_readers: Vec<usize>` (per child slot, this edge's reader ordinal at that child).
  - `PlanIndex::build_roots(roots: &[&'a dyn GpuNode]) -> Result<PlanIndex<'a>, PlanError>`; `build(root)` delegates.
  - `Scheduler::new(shape: &PlanShape)` unchanged in signature; holds over `regions`/`probe` lists.

- [ ] **Step 1: failing tests.** index: `a_shared_source_is_indexed_once_with_two_readers` (`child_readers` of the two consumers are 0 and 1); `heights_are_the_longest_path_to_a_root`; `every_root_is_indexed_in_order`; `a_tree_indexes_exactly_as_before` (over `every_kind()`: regions are the old contiguous ranges, `post_order` and `slot_base` as the old walk gave). scheduler: the `shape()` helper takes reader lists; `a_hold_covers_only_what_reaches_a_root_through_the_held_edge` (a probe child also read by another root is not held, the exclusive filter above it is); `the_incremental_schedule_picks_what_a_full_rescan_would` extended — `random_shape` adds random extra reader edges onto leaves, `assert_contiguous` becomes `assert_regions_are_dominated` (every member's readers are inside the region or are the edge).
- [ ] **Step 2:** run `-- executor::driver::index:: executor::driver::scheduler::` → FAIL.
- [ ] **Step 3:** implement; `step` refreshes the node and the parent of each of its readers.
- [ ] **Step 4:** pass; the full `executor::driver::` suite still passes unchanged.
- [ ] **Step 5:** golden check; `architecture.md` "The scheduling rule": height is the longest path, a step re-checks every reader, holds are regions. Commit `driver: index and schedule a DAG`.

### Task 4: `GpuHoldBuffer`, a hold that stops at it, and the rule that an unbuffered edge into a hold is refused

On a tree, first: every test here but the validation ones runs one root.

**Files:**
- Create: `plan/hold_buffer.rs` (`GpuHoldBuffer`, its `GpuNode` impl, `new_hold_buffer`), declared in `plan/mod.rs`; `plan/mod.rs:1141-1168` (`NodeRef::HoldBuffer`), `:1215` (`node_ref_of`), `:1232-1241` (`ExecutorCategory::HoldBuffer`, not lane-scoped), `:1270` (`category_of`), `:1295` (name `"GpuHoldBuffer"`), `:49-59` (`PlanError::UnbufferedHold`), `plan/error.rs` (its `Display`)
- Modify: `plan/dag.rs` (`region_of_edge` stops at a buffer; `held_region_of_node`), `plan/validate.rs` (the rule below; the `:322` arm)
- Modify: `executor/mod.rs:444-454` (`NodeExecutors::HoldBuffer`, no backend), `executor/cpu_backend/backend.rs:123`, `executor/gpu_backend/backend.rs:56,168` (return it before any recipe lookup, as the routers do), `executor/driver/index.rs` (`PlanShape.buffers`), `executor/driver/scheduler.rs` (a buffer is not masked by a hold; `is_held`)
- Create: `executor/driver/partitioned/answers.rs` — a child module (`mod answers;` in `partitioned.rs`, so it sees `Driver`'s private fields) holding `answer`/`report` (`partitioned.rs:949-986`), moved verbatim, so `partitioned.rs` stays < 1000 lines; `executor/driver/partitioned/hold_buffer.rs` — the buffer arm (`run_hold_buffer`, `hold_buffer_lane_ready`, the per-lane stores)
- Modify: `executor/driver/partitioned.rs` (dispatch on `ExecutorCategory::HoldBuffer`, `release_in_flight` and `assert_drained` cover the stores)
- Modify: `planner/memory_estimation.rs:217` (`held_by_accumulator` arm), `planner/nullability.rs:186` (passthrough arm), `plan_text/node_text.rs:173` (its line), `wire/attach.rs:112` (the routers' `Ok(None)` arm)
- Test: `plan/dag/tests.rs`, `plan/validate/tests.rs`, `executor/driver/scheduler/tests.rs`, `executor/driver/tests/hold_buffer.rs` (new, declared in `tests/mod.rs`), `planner/memory_estimation/tests.rs`, `wire/tests.rs`, `plan_text/tests.rs`

**Interfaces:**
- Consumes: `PlanDag`, `validate_roots` (Task 2); `PlanShape`, `Scheduler` (Task 3).
- Produces:
  - `pub(crate) struct GpuHoldBuffer { input: Box<dyn GpuNode>, kind: NodeKind }` — `NodeKind::Intermediate` with its input's layout and schema; one child; `validate_schemas_and_partitions` accepts any non-sink child.
  - **The constructor query-fuser calls:** `pub(crate) fn new_hold_buffer(input: Box<dyn GpuNode>) -> GpuHoldBuffer` (as `new_merge_partitions` is, `plan/partition_ops.rs:90`).
  - `PlanDag::region_of_edge(&self, parent, slot)` now **stops at a buffer**: a `GpuHoldBuffer` joins the region by the usual rule, its descendants are not decided through it. `PlanDag::held_region_of_node(&self, node: usize) -> Vec<usize>` — `region_of_node` with the same stop (the cap, Task 7). `region_of_node` itself does not stop (a limit's hold never lifts, Task 6). `PlanDag::is_buffer(&self, node: usize) -> bool`.
  - `PlanError::UnbufferedHold { hold: usize, reader: usize, slot: usize, producer: usize, what: String }` — indices in `PlanDag::of(roots)` pre-order; `hold` is the join (or, Task 7, the root) whose hold the edge enters; the edge is `(reader, slot)` → `producer`. Display: `"invalid plan: {what}"`, with `what` = `"{reader name} (node {reader}) reads {producer name} (node {producer}) from outside the hold of {hold name} (node {hold}); a GpuHoldBuffer must sit on that edge"`.
  - The rule, in `validate_roots`, per join J (pre-order), with `R = region_of_edge(J, PROBE_CHILD)`: every edge `(p, slot) → c` with `c ∉ R` and either `p ∈ R` or `(p, slot) == (J, PROBE_CHILD)` is refused with `UnbufferedHold` unless `p` is a `GpuHoldBuffer`. The first refusal in (J, p, slot) pre-order is returned. On a tree no edge enters a region from outside but a buffer's own, so every single-query plan passes.
  - `pub(crate) fn unbuffered_entry(dag: &PlanDag<'_>, region: &[usize], held_edge: Option<(usize, usize)>) -> Option<(usize, usize, usize)>` — the first `(reader, slot, producer)` as above; Task 7 reuses it for the cap.
  - Test fixture, `src/tests/share.rs` (created in Task 2): `pub(crate) fn buffer_unbuffered_holds(roots: Vec<Box<dyn GpuNode>>, cap: Option<NonZeroUsize>) -> (Vec<Box<dyn GpuNode>>, usize)` — loops `validate_dag(&roots, cap)`; at each `UnbufferedHold { reader, slot, .. }` rebuilds with a `new_hold_buffer` on edge `(reader, slot)`; returns the roots once no `UnbufferedHold` is left, and how many buffers it inserted. Any other refusal is returned to the caller's test as a panic naming it. The property test and Task 11 use it.
  - `pub(crate) fn validate_dag(roots: &[&dyn GpuNode], cap: Option<NonZeroUsize>) -> Result<(), PlanError>` in `plan/validate.rs` — the one entry point (spec 3c): `validate_roots` (structure), the 3b rule above per join, `validate_cap` (Task 7; until Task 7 lands, `cap` must be `None`), then the wait-for graph: one vertex per hold, an edge H → H′ when a node H waits on is in `held_region` of H′ (holds stop at buffers). A join J waits on its build region. A capped root r ≥ k (Task 7) waits on its admission, that is, on the roots of ordinal < r finishing: what it waits on is the nodes of those roots, **never its own region**, so it has no edge to itself. A join inside root r never gets an edge to r's cap: the root's admission precedes every step of its joins. A cycle → `PlanError::HoldCycle { holds: Vec<usize> }` (pre-order indices, cycle order, Display `"invalid plan: holds wait on each other: {names, → joined}"`). `run_roots` calls `validate_dag` and nothing else; `validate_roots`/`validate_cap` stay as its parts.
  - `PlanShape.buffers: Vec<bool>`; `Scheduler::is_held(&self, node: usize) -> bool`. A buffer counts its holds as any node does, but a hold never masks it out of picks; the driver's `hold_buffer_lane_ready` reads `is_held`.
  - Driver behaviour (`partitioned/hold_buffer.rs`), per lane `l` of a buffer B, store `VecDeque<Held<B::Batch>>`:
    - held: a lane is ready when its child lane has a batch for it; the step moves that `Held` (charge and all) to the back of the store and emits nothing.
    - not held: a lane is ready when its store is non-empty, its child lane has a batch, or its child lane is done and the store is empty (to mark the lane done). The step emits the store's front if any, else forwards the child's batch; one batch per lane per step, so its parent's queue stays one per lane.
    - `release_in_flight` releases every stored entry once; `assert_drained` requires every store empty.
  - `NodeExecutors::HoldBuffer` — no backend state; both backends return it for `NodeRef::HoldBuffer` before reading a recipe.
  - Estimator: `held_by_accumulator(buffer) = Some(bytes(child))` — everything its child emits, counted in `accumulator_bytes` as an accumulator is.
  - Display (`node_text`): `GpuHoldBuffer: lanes=<n>`, then the usual run figures.
  - Wire: **no wire node.** The arm joins the routers' `Ok(None)` (`wire/attach.rs:112`); its recipe is `None`; the child's offset passes up through `writer.reduce(mark)` as it does through a `GpuUnion`. The spec's "wire shows it as its own node" is met where the wire format shows nodes that make no call: its own row in architecture.md "From node to seqs" (`none` / driver-owned per-lane store, zero FFI calls) and its own `None` entry in `RecipePlan.recipes`. A wire node would be a `Cudf*` kind the C++ dispatches on with nothing to do, an fbs and C++ change the spec rules out ("No C++ or ABI change").

- [ ] **Step 1: failing tests.**
  - dag: `a_hold_stops_at_a_buffer` — `unload(join(b, buffer(filter(P))))`: `region_of_edge(J, 1) == [buffer]`; `region_of_node(J)` still holds the filter and P; `held_region_of_node(root)` stops at the buffer.
  - validate_dag: `a_hold_cycle_is_refused_by_name` (the wait-for graph's cycle finder over a hand-built vertex/edge list: `HoldCycle` in cycle order; an acyclic list → `Ok`); `validate_dag_refuses_what_its_parts_refuse` (each of the shapes below through `validate_dag` gives the same error as its part); `every_dag_validate_dag_accepts_finishes` — property test, 500 seeded random DAGs (2–4 roots, 1–3 shared sources, 0–3 joins, buffers on random edges, cap none until Task 7) on the mock backend with a step bound of 10 000, the simulation's verdict (finished, or stuck at the bound) compared with `validate_dag`'s both ways: accepted ⇒ every root done and `holds == releases`; stuck ⇒ `validate_dag` had refused it. `no_valid_dag_is_refused` — the other direction, over the same generator: each DAG first goes through `buffer_unbuffered_holds` (below), so it passes the structure and rule 3b; then `validate_dag` must return `Ok` (never `HoldCycle`) and the simulation must finish. A refusal names the seed, the vertices and the cycle. Seeds printed on every failure.
  - validate: `an_unbuffered_reader_of_a_shared_source_inside_a_hold_is_refused` — A = `unload(join(b, filter(S)))`, B = `unload(project(S))`: `Err(PlanError::UnbufferedHold { hold: J, reader: filter, slot: 0, producer: S, .. })`, and the exact Display text. `a_join_reading_one_source_on_both_sides_without_a_buffer_is_refused` — `unload(join(S, S))`: the refused edge is `(J, 1)`. `a_buffer_on_that_edge_passes` — `unload(join(b, filter(buffer(S))))` + B, and `unload(join(S, buffer(S)))`: `Ok`. `two_roots_whose_builds_wait_on_each_others_probe_are_refused_without_buffers` — A = `join(S1, S2)`, B = `join(S2, S1)`: refused at A's probe edge. `a_tree_never_needs_a_buffer` — every `every_kind()` fixture validates as before.
  - scheduler: `a_held_buffer_stays_runnable_for_intake` — with J building, `is_held(buffer)` and the buffer is picked when its child lane has a batch; `a_held_buffer_with_nothing_offered_is_not_ready`.
  - driver (`tests/hold_buffer.rs`, mock): `the_subtree_below_a_held_buffer_runs_during_the_build` (`unload(join(B 3 batches, buffer(filter(P 3 batches))))`: every filter step precedes the join's last build step in the trace); `a_buffer_emits_in_arrival_order_once_the_hold_lifts` (the join's probe inputs equal the filter's emissions, in order); `a_four_lane_buffer_keeps_one_queue_per_lane` (P and filter at 4 lanes: each lane's emissions equal that lane's arrivals, no lane's batch on another); `an_unheld_buffer_forwards_as_it_arrives` (a buffer under no hold: answers and the non-buffer trace steps equal the same plan without it); `a_buffer_holds_its_batches_in_the_accountant` (peak `in_flight_bytes` ≥ Σ the filter's batch bytes; at the end `holds == releases`, `in_flight_bytes == 0`); `a_failed_call_with_batches_buffered_still_reconciles` (`failing_at` the join's build after the buffer took 2 batches: `holds == releases`).
  - estimator: `a_buffer_is_charged_everything_its_child_emits` — `resident[buffer] == bytes(filter)`, and `accumulator_bytes` includes it.
  - wire: `a_hold_buffer_crosses_as_nothing` — `attach_recipes` of the plan with the buffer has the same `bytes` and `wire_nodes` as without; `recipes[buffer's post-order]` is `None`.
  - plan_text: `a_buffer_prints_its_own_line` — `GpuHoldBuffer: lanes=1` between the join's and the filter's lines.
- [ ] **Step 2:** `cargo test --features rust-only -p peacockdb-core --lib -- plan:: executor::driver:: planner::memory_estimation wire:: plan_text::` → FAIL to compile.
- [ ] **Step 3:** implement as above. `partitioned.rs` moves `answer`/`report` first, then grows the dispatch arm.
- [ ] **Step 4:** pass; every old `executor::driver::` test passes unchanged.
- [ ] **Step 5:** golden check. `architecture.md`:
  - "The scheduling rule": a hold that lifts stops at a `GpuHoldBuffer` — the buffer is held, nothing below it is, so its subtree runs during the build and what waits is the buffer's input, after the filters and projections below it; a held buffer takes what it is offered and emits nothing; unheld, it emits its store in arrival order, one batch per lane per step. Then this paragraph, verbatim, as a rule every later driver change keeps:

    > **The only unbounded storage is a node you can see.** A queue between two ordinary nodes
    > stays self-bounding at one batch per lane, as "Queues need no cap" says. Validation
    > refuses any plan where an edge into a held region carries batches from a producer outside
    > it and nothing on that edge or below it, inside the region, is a hold buffer. That
    > includes a shared source read from inside a hold. Unbounded storage exists only in
    > accumulators and hold buffers, which the plan shows and the estimator charges.

  - "From node to seqs": the `GpuHoldBuffer` row (`none`; driver-owned per-lane store, zero FFI calls).
  - "Memory accounting": a buffered batch stays held until the buffer emits it or the run releases it.

  Commit `driver: GpuHoldBuffer, and a hold stops at it`.

### Task 5: reader queues, one charge per batch, and several roots

**Files:**
- Create: `executor/driver/queue.rs` (`OutQueue<T>`), `executor/driver/queue/tests.rs`
- Modify: `executor/driver/partitioned/answers.rs` (Task 4) — the per-root answer bookkeeping
- Modify: `executor/driver/accounting.rs:33-51,118-135` (`Charge`), every `hold`/`release` site in `partitioned.rs`, `partitioned/hold_buffer.rs` and `single_partition.rs` (265-523), `partitioned.rs` (Driver fields, `new`, `avail`, `peek_rows`, `take_input`, emitter, accumulator, forwarder, `release_in_flight`, `assert_drained`, unload results)
- Modify: `executor/mod.rs:633-688` (`RunReport` docs), `:1001-1027` (entry points), `executor/driver/mod.rs:29-46`
- Test: `executor/driver/tests/dag.rs` (new, declared in `tests/mod.rs` with a `driver_of_roots` helper), `executor/driver/partitioned/tests.rs`, `executor/driver/accounting/tests.rs`

**Interfaces:**
- Consumes: `Batch::share` (Task 1), `PlanIndex::build_roots` (Task 3), `new_hold_buffer`, `PlanError::UnbufferedHold` (Task 4).
- Produces:
  - `#[derive(Debug, Clone)] pub(crate) struct Charge { pub bytes: usize, holders: Rc<()> }` — one per queue entry, refcounted over its holders; `Charge::new(bytes)` has one holder, `clone()` adds one. `Held<T> { pub batch: T, pub charge: Charge }`, `Held::of` makes the first holder. `ResidentAccountant::hold(&mut self, charge: &Charge)` once, when the entry is pushed; `release(&mut self, charge: Charge) -> Result<(), RunError>` drops one holder and changes the totals and the release counter only when it was the last (`Rc::strong_count == 1`). Holders are each queue cursor that has not yet taken the entry, a buffer store that keeps it, and the consumed input of the step that read it; a share taken by a non-last reader holds a clone of the charge, so whichever holder drops last releases the bytes, whatever the order of takes. `begin_call` still prices by `charge.bytes`.
  - `pub(crate) struct OutQueue<T>`: `new(readers: usize)`, `push(&mut self, held: Held<T>)`, `has(&self, reader: usize) -> bool`, `front_rows(&self, reader: usize) -> Option<u64>`, `take(&mut self, reader: usize) -> Result<Option<Held<T>>, BackendError>` (a non-last live reader gets `share()` with a clone of the entry's charge; the last gets the original and the queue's own holder), `retire(&mut self, reader: usize) -> Vec<Held<T>>` (that reader's holders of each entry it never took; the bytes leave only when no other holder is left), `drain(&mut self) -> Vec<(Held<T>, usize)>` (each entry once, with how many readers never took it), `len(&self) -> usize`, `high_water(&self) -> usize` (most entries ever queued at once).
  - **The entry point query-fuser calls** (`executor/mod.rs`):

    ```rust
    pub(crate) struct FusedRun {
        /// One answer per root, in the order the roots were given; each is what that root
        /// alone answers, in the order it alone answers it.
        pub(crate) answers: Vec<Vec<CpuBatch>>,
        /// The run's figures over the whole DAG, indexed by `PlanIndex::build_roots`'s
        /// pre-order. `batches` is empty here: `answers` carries them.
        pub(crate) report: RunReport,
    }
    pub(crate) fn run_roots<B: Backend>(
        roots: &[&dyn GpuNode],
        ctx: &B::Context,
        budget: Option<usize>,
        cap: Option<NonZeroUsize>,
    ) -> Result<FusedRun, RunError>;
    pub(crate) fn run_roots_with_hook<B: Backend>(
        roots: &[&dyn GpuNode],
        ctx: &B::Context,
        budget: Option<usize>,
        cap: Option<NonZeroUsize>,
        hook: Option<OutputHook<'_, B>>,
    ) -> Result<FusedRun, RunError>;
    ```

    `run`/`run_with_hook` delegate with `&[root]`, `None` and move `answers[0]` into
    `report.batches`. `roots` empty → `RunError::Protocol("a run needs at least one root")`.
    `run_roots` calls `validate_dag` before building the driver; a refusal comes back as
    `RunError::Backend(PlanError::UnbufferedHold { .. })` (or `HoldCycle`), the variant plan refusals already
    travel in. `cap` is accepted and ignored until Task 7 (its tests land there).
  - `RunReport.abandoned[node][lane]` becomes the sum over readers of rows that reader never took, so `Σ consumed over readers + abandoned == readers × emitted`; on a tree, unchanged.
  - Driver: `Driver::for_roots(roots: &[&'a dyn GpuNode], ctx, budget, cap: Option<NonZeroUsize>)`; `Driver::new(root, ctx, budget)` delegates.

- [ ] **Step 1: failing tests.** queue: `one_reader_is_a_fifo_with_no_share`; `a_non_last_reader_gets_a_share_and_the_last_the_original`; `retiring_the_last_live_reader_hands_back_the_originals`. accounting: `a_charge_is_released_by_its_last_holder` (two clones, release one: totals unchanged; release the other: bytes leave, `releases` +1). dag.rs (mock, S scripted 3 batches): `each_root_answers_what_it_answers_alone` (per-root unload emission rows equal each root run alone with `run`); `a_shared_batch_is_loaded_once` (S's `emitted` has 3 entries); `a_shared_batch_is_held_once_and_released_once` (`unload(filter(S))`, `unload(project(S))`: `holds == 9`, `releases == 9`, `in_flight_bytes == 0`); `each_reader_consumes_every_batch_once`; `the_output_hook_sees_a_shared_batch_once` (3 calls for S); `two_roots_whose_builds_wait_on_each_others_probe_both_answer` (A = `join(S1, buffer(S2))`, B = `join(S2, buffer(S1))`: both answer as alone); `a_join_over_one_source_on_both_sides_answers` (`join(S, buffer(S))`); `an_unbuffered_reader_inside_a_hold_is_refused_by_run_roots` (A = `join(b, filter(S))`, B = `filter(S)`: `RunError::Backend(PlanError::UnbufferedHold { .. })`, no step taken); `a_shared_source_feeding_a_held_buffer_queues_one_batch` (the two-roots shape: every out-queue's `high_water() <= 1` at 1 lane — the buffers kept the rest); `a_four_lane_shared_source_reaches_every_reader_on_every_lane` (S at 4 lanes read by `filter` and by `emit(…, 4)`); `a_root_that_receives_nothing_answers_its_own_empty_batch`; `a_failed_call_with_a_shared_batch_queued_still_reconciles` (`failing_at` the second root's filter: `holds == releases`); `run_roots_of_one_root_matches_run` (trace equal); `no_roots_is_refused`. partitioned/tests.rs: `a_batch_is_dropped_after_its_last_reader` (step by step: S's bytes stay in flight after the first reader's step, leave after the second's). dag.rs: `a_buffered_share_keeps_its_charge_after_the_last_reader_takes` — root A `unload(join(b, buffer(S)))`, root B `unload(agg(filter(S)))`, the buffer taking S's batch first and B's filter taking and consuming it after: after B's step, `in_flight_bytes` ≥ the buffered bytes, and stays so until the hold lifts and the buffer emits; at the end `holds == releases`, `in_flight_bytes == 0`. `a_limit_retiring_a_reader_keeps_the_charge_a_buffer_holds` — the same shape with B `unload_limited(filter(S), 0, Some(1))`: after B retires, the buffered bytes are still in flight.
- [ ] **Step 2:** `cargo test --features rust-only -p peacockdb-core --lib -- executor::driver::` → FAIL to compile.
- [ ] **Step 3:** implement. Unload results go to `answers[root ordinal of that unload]`; `finish` checks every root is done; `answer` per root reads that root's sink child schema. A buffer reading a shared source takes through its own reader cursor, as any reader does.
- [ ] **Step 4:** all `executor::driver::` tests pass, old and new.
- [ ] **Step 5:** golden check. `architecture.md`: Traits (`Batch::share`), "Queues need no cap" (one cursor per reader, still one batch per lane per reader; a hold is answered by a buffer, never by a growing queue), the no-deadlock argument (below), Memory accounting (one hold per batch, refcounted over its holders, released when the last holder drops it, which may be a buffer after every reader has taken). Commit `driver: shared sources and several roots`.

The argument `architecture.md` carries, replacing "plans are trees": a hold that lifts covers
only a region, the nodes all of whose paths to a root pass through the held edge, and it stops
at a hold buffer, which is held while its subtree is not. Validation guarantees that every edge
entering a held region from outside ends at a buffer, and a held buffer takes every batch it is
offered, so no producer outside a hold ever waits on it: it drains as if the hold were not
there. A join's build side is never in its own hold, and a shared node is held only when every
reader is. If join J′ holds a node of J's build side, J′'s probe edge lies on every path from
it, so J′ is either inside J's build side (it resolves first, by induction) or above J, holding
J itself. Holding is acyclic because the plan is, so the lowest join no other unfinished join
holds can always finish its build. No queue grows to make this true: what waits sits in a
buffer, which the plan shows and the estimator charges. The two shapes: two roots whose builds
wait on each other's probe source run because each probe edge ends at a buffer, and a join
reading one source on both sides runs because the source feeds the build directly and the
probe through a buffer.

### Task 6: a limit's early exit per consumer

**Files:**
- Modify: `executor/driver/scheduler.rs:133-156` (`satisfy`), `executor/driver/partitioned.rs:633-658` (`settle_limit`), `:707-717` (`release_in_flight`), `partitioned/hold_buffer.rs` (a buffer inside a satisfied region)
- Test: `executor/driver/scheduler/tests.rs`, `executor/driver/tests/limit.rs`

**Interfaces:**
- Produces: `Scheduler::satisfy(&mut self, node: usize) -> Vec<(usize, usize)>` — holds `regions[node]` (which does not stop at buffers: a limit's hold never lifts), and returns each `(producer, reader ordinal)` whose reader is inside that region and producer outside; a producer whose readers are now all retired is satisfied in turn (and its pairs returned too). `Scheduler::is_satisfied`, `any_satisfied` unchanged. The driver retires each returned reader (`OutQueue::retire`), releases the originals returned, adds their rows to `abandoned`, and marks a newly satisfied producer's lanes done. A buffer inside a satisfied region releases its store, adds those rows to `abandoned`, and is marked done.

- [ ] **Step 1: failing tests.** scheduler: `satisfying_a_consumer_retires_its_reader_and_a_source_with_none_left_is_satisfied`; `a_satisfied_tree_returns_no_reader` (on a tree, `satisfy` returns `[]`). limit.rs: `a_satisfied_root_does_not_stop_a_source_another_root_reads` (A = `unload_limited(S, 0, Some(1))`, B = `unload(S)`, S 5 batches: S emits 5, B answers all, A answers 1 row, `holds == releases`); `the_source_stops_once_every_reader_is_satisfied` (both limited to 1 row: S emits 1 batch, `satisfied` lists S); `a_retired_reader_releases_what_it_left_queued`; `a_limit_zero_root_retires_its_reader_before_any_step`; `a_satisfied_limit_releases_what_a_buffer_below_it_keeps` (`unload_limited(join(B, buffer(P)), 0, Some(1))`, B 3 batches: the buffer's kept batches are released, P stops, `holds == releases`, `in_flight_bytes == 0`).
- [ ] **Step 2:** run → FAIL.
- [ ] **Step 3:** implement.
- [ ] **Step 4:** pass; `executor::driver::` all pass.
- [ ] **Step 5:** golden check; `architecture.md` "Early exit at a limit": the hold is the satisfied node's region, through any buffer; a reader crossing out of it retires; a buffer inside it releases what it keeps; a source stops when every reader is satisfied or done. Commit `driver: per-consumer early exit`.

### Task 7: the concurrency cap

**Files:**
- Modify: `executor/driver/scheduler.rs` (cap holds), `executor/driver/partitioned.rs` (reports a finished root), `plan/validate.rs` (`validate_cap`), `executor/mod.rs` (`run_roots` calls it)
- Test: `executor/driver/scheduler/tests.rs`, `executor/driver/tests/cap.rs` (new), `plan/validate/tests.rs`

**Interfaces:**
- Consumes: `held_region_of_node`, `unbuffered_entry`, `new_hold_buffer`, `buffer_unbuffered_holds` (Task 4).
- Produces:
  - `PlanShape.cap_regions: Vec<Vec<usize>>` — per root, `held_region_of_node(root)` (stops at buffers: the cap lifts).
  - `Scheduler::with_cap(shape: &PlanShape, cap: Option<NonZeroUsize>) -> Self` (`new` delegates with `None`); `Scheduler::root_finished(&mut self, root: usize)`. A root is *admitted* in root order while fewer than `cap` admitted roots are unfinished; an unadmitted root's `cap_regions[root]` carries a cap hold (a third counter beside build and limit holds). Shared nodes are outside every root's region, so the cap never holds a source. The driver calls `root_finished` once, when a root's lanes are all done or it is satisfied.
  - `pub(crate) fn validate_cap(roots: &[&dyn GpuNode], cap: Option<NonZeroUsize>) -> Result<(), PlanError>` — with `Some(k)`, `k < roots.len()`, every root of ordinal `≥ k` (the ones that can wait) is a held region under rule 3b: `unbuffered_entry(dag, cap_region, None)` must be `None`, else `UnbufferedHold { hold: root, .. }`. Roots below `k` are admitted at once and never wait. `None`, or `k ≥ roots.len()`: `Ok`. `validate_dag` calls it after the 3b rule.
  - So a cap below the root count needs every shared source a waiting root reads to reach it through a buffer; the batches wait there, never in a queue.
  - The cap's vertex in `validate_dag`'s wait-for graph (Task 4): an unadmitted root r waits on the roots of ordinal < r finishing, so its edges go to the holds whose held regions contain a node of those roots. `cap_regions[r]` is what the cap holds, not what r waits on: r never waits on its own region, and a join inside r never waits on r's cap.

- [ ] **Step 1: failing tests.** scheduler: `a_cap_holds_the_regions_of_roots_past_it`; `finishing_a_root_admits_the_next`; `a_cap_hold_stops_at_a_buffer`. validate: `a_cap_over_an_unbuffered_shared_reader_is_refused` (three roots `unload(filter(S))`, cap 1: `UnbufferedHold { hold: root 1, .. }`); `a_cap_at_or_above_the_root_count_needs_no_buffer`. cap.rs (three roots over one S; roots 1 and 2 read it as `unload(buffer(filter(S)))`): `a_cap_of_one_runs_roots_one_at_a_time` (no trace step of root 1's cap region precedes root 0's last unload step); `a_capped_root_reads_a_shared_source_through_its_buffer` (S emits all its batches with root 2 unadmitted; they wait in root 2's buffer, and S's queue `high_water() <= 1`); `a_satisfied_root_frees_its_cap_slot`; `without_a_cap_the_schedule_is_deterministic` (two runs, equal traces); `every_cap_gives_the_same_answers` (cap 1, 2, 3, none); `a_capped_root_with_a_join_is_accepted_and_runs` (three roots, each `unload(join(b_i, buffer(filter(S))))` with its own build source `b_i`, cap 1: `validate_dag(roots, Some(1))` is `Ok`, not `HoldCycle`; the run finishes and each root answers as it does alone). validate: `a_capped_root_never_waits_on_its_own_region` (the wait-for graph of that plan under cap 1 has no edge from a root's cap vertex to itself or from a join to its own root's cap). The property tests of Task 4 now draw the cap from none or 1..roots: `every_dag_validate_dag_accepts_finishes` and `no_valid_dag_is_refused` both run with it, so a valid capped fused batch with a join is accepted and finishes.
- [ ] **Step 2:** run → FAIL.
- [ ] **Step 3:** implement; `run_roots` passes `cap` through.
- [ ] **Step 4:** pass.
- [ ] **Step 5:** golden check; `architecture.md` "The scheduling rule": the cap; that it bounds executor state; that its hold stops at buffers like a join's, so rule 3b applies to every root that can wait, and a shared source never waits on an unadmitted root; that an unadmitted root waits only on the roots before it finishing. Commit `driver: the concurrency cap`.

### Task 8: the estimator charges a shared batch once

**Files:**
- Modify: `planner/memory_estimation.rs:19-128` (`Tree::of` numbers by `PlanDag` post-order), `planner/mod.rs:107-109`
- Test: `planner/memory_estimation/tests.rs`

**Interfaces:**
- Produces: `pub(crate) fn estimate_roots(roots: &[&dyn GpuNode], budget: u64) -> Result<MemoryModel, PlanError>` in `planner/mod.rs`; `estimate(root, budget)` delegates. `MemoryModel.resident` indexed by `PlanDag` post-order. A shared source's resident is `batch_bytes × lanes`, charged once: a valid plan has no reader that can hold it back (rule 3b, Tasks 4 and 7), and what a held reader keeps is its buffer's, charged at the buffer (Task 4). A shared node is counted once in `accumulator_bytes` and in the per-source shares.

- [ ] **Step 1: failing tests.** `a_shared_source_is_charged_once`; `a_buffered_reader_leaves_the_source_charged_one_batch` (B = `join(build, buffer(filter(S)))`, A = `filter(S)`: `resident[S] == batch_bytes × lanes`, `resident[buffer] == bytes(filter(S))`); `two_roots_estimate_as_the_sum_of_their_accumulators`.
- [ ] **Step 2:** run `-- planner::memory_estimation::` → FAIL.
- [ ] **Step 3:** implement.
- [ ] **Step 4:** pass.
- [ ] **Step 5:** golden check (every `--- memory ---` section unchanged); `architecture.md` "Memory accounting", plan time. Commit `planner: estimate shared batches`.

### Task 9: display

**Files:**
- Create: `plan_text/dag_walk.rs` — one depth-first walk over roots that reports each visit as first or again
- Modify: `plan_text/mod.rs:30-53`, `node_text.rs:15-26`, `memory.rs:13-69`, `run_text.rs:23-100`, `bench_text.rs:16-41` (all walk through `dag_walk`)
- Test: `plan_text/tests.rs`

**Interfaces:**
- Produces: `render_plans(roots: &[&dyn GpuNode]) -> String`, `render_plans_memory(roots, model: &MemoryModel) -> String`, `render_runs(roots, report: &RunReport) -> String`, `render_timings_of_roots(roots, times: &Measurements, report: &RunReport) -> String` (bench-rows' `render_timings(root, times, report)` signature, which lands first); the one-root forms delegate with `&[root]`.
- Format (exact): with several roots, each tree is preceded by a line `root <i>:` (0-based) and indented by one level; with one root, no header. A node with two or more readers prints its normal line prefixed `@<k> ` on its first visit, `k` counting shared nodes from 1 in walk order; every later visit prints `<indent>@<k>` alone and does not descend. A `GpuHoldBuffer` prints its own line (Task 4) in every renderer.

- [ ] **Step 1: failing tests.** `one_root_prints_no_header` (equal to `render_plan`'s current output for a fixture tree); `a_shared_node_prints_once_and_is_referenced_after` (two roots over S: the exact expected text, `@1 GpuLoadParquet: …` once, `    @1` once); `a_run_renders_a_shared_node_once`; `a_buffered_reader_of_a_shared_node_prints_the_buffer_above_the_reference` (`GpuHoldBuffer: lanes=1` line, then `@1` under it).
- [ ] **Step 2:** run `-- plan_text::` → FAIL.
- [ ] **Step 3:** implement.
- [ ] **Step 4:** pass.
- [ ] **Step 5:** golden check; `architecture.md` "Node display": the two rules. Commit `plan_text: print a shared node once`.

### Task 10: the wire format — one seq space, a shared node numbered once, several roots

**Files:**
- Modify: `flatbuffers/gpu_plan.fbs:606-608` → `table GpuPlan { root: PlanNode; roots: [PlanNode]; }` (appended field)
- Modify: `wire/writer.rs:76-84,158-175` (`reuse`, `finish` over k roots), `wire/attach.rs:27-56` (first-visit walk), `wire/mod.rs:366-371`, `wire/read.rs:16-25,138-150` (roots, visit once), `wire/recipes.rs:58-100` (count each node once)
- Modify: `cpp/src/node_session.cpp:345-348,410-420` (`index_post_order` with a visited set; roots)
- Modify: `test_support/gpu_session.rs:22-43` (`Session::open_roots`)
- Test: `wire/tests.rs`, `cpp/tests/cpu/test_executor.cpp`, `wire/gpu_tests/mod.rs`

**Interfaces:**
- Produces:
  - `pub(crate) fn attach_recipes_for_roots(roots: &[&dyn GpuNode], join_scratch_bytes: u64) -> Result<RecipePlan, PlanError>`; `attach_recipes(root, …)` delegates. Recipes are indexed by `PlanDag` post-order, one per node (a buffer's is `None`, Task 4).
  - `Writer::reuse(&mut self, offset: WIPOffset<fb::PlanNode<'a>>)` — pushes an already-created node back into the pool for the next consumer to take; `Writer::finish(self, roots: usize) -> Result<(Vec<u8>, Seq), PlanError>` — the pool must hold exactly `roots` offsets; one root writes `root` only (bytes unchanged), more write `roots` only.
  - C++: the session indexes `roots()` in order when present, else `root()`; both set → `"GpuPlan sets both root and roots"`; neither → today's `"GpuPlan has no root node"`. A node already indexed is not indexed again, so the first visit's seq is its seq on both sides.
  - test_support: `Session::open_roots(roots: &[&dyn GpuNode], join_scratch: u64, what: &str) -> Session`; `open` delegates.

- [ ] **Step 1: failing tests.** wire: `a_shared_source_takes_one_seq` (two roots over S: S's `CudfScan` seq appears in both consumers' recipes, `wire_nodes` == the count with S once); `several_roots_share_one_seq_space`; `one_root_writes_no_roots_field` (`GpuPlan.roots()` is `None`); `the_reader_visits_a_shared_node_once` (`check_seq_kinds` passes, `depth` correct); `a_buffered_shared_reader_takes_the_sources_seq` (`join(S, buffer(S))`: the join's probe recipe names S's seq). C++ CPU: `NodeSession.IndexesASharedNodeOnce`, `NodeSession.IndexesSeveralRootsInOrder`, `NodeSession.RefusesBothRootAndRoots` — fbs built by hand as the existing CPU cases do (`test_executor.cpp:287`); node count via `begin_plan`'s `out_node_count` if the CPU build allows a session, else via `NodeSession::node_count()`.
- [ ] **Step 2:** run `-- wire::` and `ctest -L cpu` → FAIL.
- [ ] **Step 3:** implement.
- [ ] **Step 4:** pass; `the_payload_golden_carries_what_each_call_hands_the_executor` passes with `recipe-payloads.txt` untouched.
- [ ] **Step 5:** golden check; `architecture.md` "The wire format": a DAG crosses as one buffer whose shared node is one offset with several parents; `roots`; first-visit numbering on both sides; a hold buffer is not on the wire. Commit `wire: several roots and a shared node`.

### Task 11: fused answers end to end, on the CPU and the device, TPC-DS q95 included

**Files:**
- Create: `src/tests/end_to_end/dag.rs` (declare in `end_to_end.rs:30-33`), `src/tests/gpu_tests/dag_cases.rs` (declare in `tests/gpu_tests/mod.rs`)
- Modify: `build-test.md` (rows and counts for every tier these tasks touched), `architecture.md` "Execution" intro (one paragraph: what a fused plan is, pointing at the rules above)

**Interfaces:**
- Consumes: `share_sources` (Task 2), `new_hold_buffer`, `PlanError::UnbufferedHold`, `buffer_unbuffered_holds` (Task 4), `run_roots` (Task 5), `Session::open_roots` (Task 10).
- Uses (test fixture, Task 4): `buffer_unbuffered_holds(roots, None)`.
- The queries (tpch sf1, written in the test; each table read with the same columns so the scans match): R1 `SELECT sum(l_quantity), count(l_orderkey) FROM lineitem`; R2 `SELECT l_orderkey % 7 AS k, max(l_quantity) FROM lineitem GROUP BY k`; R3 `SELECT o_orderpriority, sum(l_quantity) FROM orders JOIN lineitem ON l_orderkey = o_orderkey GROUP BY o_orderpriority` (lineitem projected to `{l_orderkey, l_quantity}`, orders to `{o_orderkey, o_orderpriority}`); R4 `SELECT o_orderpriority, count(o_orderkey) FROM orders GROUP BY o_orderpriority`.
- q95: TPC-DS q95 planned at sf1 by `plan_at("tpcds", "1", "q95", &MODES[0])`. Its plan (`testdata/goldens/tpcds.sf1/tp1-single-mini.cpu.txt`, `== q95`) reads `web_sales` with `projections=[ws_warehouse_sk@15, ws_order_number@17]` four times, as both sides of two self-joins (`GpuHashJoin: join_type=Inner, on=[(ws_order_number@1, ws_order_number@1)], filter=ws_warehouse_sk@probe:0 != …`): build through a `GpuCoalesceAllBatches`, probe directly. `share_sources` makes those four one source. Then each self-join's probe edge enters its own hold from outside, and each coalesce's input edge enters the hold of the `LeftSemi` join above it, so `buffer_unbuffered_holds` inserts 4 buffers.

- [ ] **Step 1: failing tests.** CPU (`end_to_end/dag.rs`), at `tp1_single` and `tp4_sized`: `two_roots_over_one_shared_source_answer_as_alone` (R1, R2: `share_sources` reports 1, `buffer_unbuffered_holds` 0; each `answers[i]` sorted equals that query's `run` alone, byte for byte via `batches_to_sorted_str`); `three_roots_over_two_shared_sources_answer_as_alone` (R1, R3, R4: 2 shared; the buffers the fixture inserts — lineitem under R3's probe — are counted and recorded in the detail file); `a_fused_run_holds_each_shared_batch_once` (`holds == releases`, `in_flight_bytes == 0`, and `holds` == the sum of the roots' holds alone minus the shared sources' batch counts, plus 0 for buffers, which take over a charge rather than add one). q95, `tp1_single`: `q95_shared_without_buffers_is_refused` (after `share_sources`, reporting 1: `validate_roots` → `UnbufferedHold`); `q95_fused_with_buffers_answers_as_q95_alone` (`buffer_unbuffered_holds(roots, None)` reports 4; `run_roots` of the one root equals q95's `run` alone, sorted, byte for byte). Device (`gpu_tests/dag_cases.rs`), `tp1_single` and `tp4_sized`: `two_roots_over_one_shared_source_answer_as_alone_on_the_device`, `three_roots_over_two_shared_sources_answer_as_alone_on_the_device`; `tp1_single`: `q95_fused_with_buffers_answers_as_q95_alone_on_the_device` — each root's IPC answer byte-identical to the same query run alone through `Session::open`.
- [ ] **Step 2:** `cargo test --features rust-only -p peacockdb-core --lib -- tests::end_to_end::dag::` → FAIL (module absent), then PASS after Step 3.
- [ ] **Step 3:** write the tests (no production code expected; a failure here is a bug in Tasks 4–10, fixed there with its own regression test). If q95's planned shape differs from the one described (a planner change since), the buffer count is what the fixture reports, and the detail file names the edges.
- [ ] **Step 4: device, nebius-gpu:** the build-test.md "GPU step, end to end". Pass = `ALL rc=0`, the 5 new `gpu_tests::` cases OK, and the device tier's counts otherwise equal to the last green `GPU Tests (nebius-gpu)` run (record both in the detail file). Stop the VM.
- [ ] **Step 5:** golden check; `cargo test -p cost-report --test test_source_formatting` passes (layout, visibility, test-module gates); commit `tests: fused answers end to end`.

---

## Self-review notes

- Spec 1 → Tasks 1, 2, 5; 2 → indexing 3, re-check 3, queues 5, accounting 5, estimator 8, validation/hook 2/5, display 9, wire 10; 3 → Tasks 4 (buffer, refusal, argument), 5 (both shapes run with buffers); 3a → Task 4 (node, hold stops at it, store per lane, accountant, estimator, display; wire decided below), Task 9 (DAG display); 3b → Task 4 (validation, architecture.md verbatim), Task 7 (the cap is a hold too); 4 → Task 6; 5 → Task 7; 6 → Tasks 4–7, 11 (q95 from the real plan) and the golden check per task; 7 → architecture.md edits in 3–7, 10, 11.
- Order keeps goldens: Tasks 1–3 add capability no planned plan reaches; 4 adds a node no planned plan contains and a validation rule no tree can trip; 5 changes queues and charges but a one-reader queue never shares and a charge with one holder is today's; 8–10 change walks whose first-visit order on a tree is today's order.
- Spec 3c's wait-for rule as reworded: a capped root waits on its admission (the roots before it finishing), never on its own region — Task 4's graph, Task 7's vertex, `a_capped_root_with_a_join_is_accepted_and_runs`, and the no-false-refusal arm `no_valid_dag_is_refused`.
- Interfaces query-fuser consumes are unchanged in name and shape: `SharedSource`, `PlanDag`, `validate_roots`, `run_roots`, `FusedRun`, `estimate_roots`, `attach_recipes_for_roots`, `Session::open_roots`, `render_timings_of_roots(roots, times, report)`; new for it: `new_hold_buffer`, `PlanError::UnbufferedHold`, `PlanError::HoldCycle`, `validate_dag` (the fuser validates every plan it builds through it), `validate_cap`.
- Spec gap, decided: 3a says display and wire show the buffer "as its own node". It makes no device call, so it has no wire node (it crosses as the routers do, `Ok(None)`); it appears as its own row in "From node to seqs" and its own `None` recipe. Spec 3b's "held region" is read to include the cap's hold, since a waiting root reading a shared source is exactly the unbounded-queue case 3b forbids; a limit's hold never lifts and retires its readers instead, so it needs no buffer.
