//! Running a plan: the contracts a backend implements, the batch that crosses them, and
//! the driver that walks the tree calling them.
//!
//! Every declaration the component offers is here. The drivers are generic over
//! [`Backend`], so each backend monomorphizes — no vtable, and `Drop` on a GPU handle is a
//! direct call. That is also what keeps the tier boundary structural: a rust-only target
//! instantiates the drivers only at the CPU backend, so the GPU types are never named.
//!
//! The API is not the same in every feature shape. [`GpuBatch`] and the GPU backend exist
//! only where cuDF is linked, so their declarations carry the cfg rather than only their
//! modules. The reasons behind each shape are in `llm-wiki/architecture.md`, under
//! Execution.

mod cpu_batch;
mod driver;
mod errors;
mod forwarder;
mod row_range;

// `pub mod`, which no other subcomponent in the crate is, and it is temporary. Two
// integration targets construct backend executors directly — `test_cpu_executors` and
// `test_gpu_executors` — and a separate crate cannot reach a private subcomponent. They are
// two of the eleven `test-layout.md` moves into `src/`, and this goes with them. The layout
// test lists both by name so the exception cannot spread quietly.
pub mod cpu_backend;

#[cfg(not(feature = "rust-only"))]
pub mod gpu_backend;
#[cfg(not(feature = "rust-only"))]
mod gpu_batch;

#[cfg(test)]
mod tests;

use datafusion::arrow::array::RecordBatch;

use crate::plan::ExecutorCategory;
use crate::plan::PlanError;
use crate::plan::{GpuNode, RowInterval};

#[cfg(not(feature = "rust-only"))]
use peacockdb_ffi::raw::PeacockExecutor;

/// One table's worth of rows, and nothing else about it.
///
/// Ownership is by move — every executor method takes a batch by value, so reuse after
/// consumption is a compile error rather than an unknown-handle throw from C++. Neither
/// implementation is `Clone`: a `GpuBatch` cannot be, and the free `RecordBatch` clone
/// is not worth the asymmetry (a future dual consumer writes an explicit copy, #140).
pub trait Batch {
    fn num_rows(&self) -> usize;
    fn byte_size(&self) -> usize;
}

/// The CPU backend's batch, and what leaves the device at the unload.
/// Not `Clone`, deliberately — symmetry with `GpuBatch`, whose handle cannot be.
#[derive(Debug)]
pub struct CpuBatch {
    batch: RecordBatch,
}

impl CpuBatch {
    pub fn new(batch: RecordBatch) -> Self {
        Self { batch }
    }

    pub fn record_batch(&self) -> &RecordBatch {
        &self.batch
    }

    pub fn into_record_batch(self) -> RecordBatch {
        self.batch
    }
}

/// A handle to a resident `cudf::table`, plus the session it belongs to.
///
/// The handle is the whole value — no box, no vtable — and `Drop` releases it, which is
/// what keeps a batch the driver abandons from leaking VRAM. A handle an FFI call
/// consumed must skip that drop: C++ erased it, and releasing it again is a use of a
/// dead handle. [`GpuBatch::consume`] is that boundary, and the only place the release
/// is skipped.
#[cfg(not(feature = "rust-only"))]
/// The executor pointer is BORROWED, as everywhere else on the GPU path: the session
/// outlives every batch drawn from it.
pub struct GpuBatch {
    executor: *mut PeacockExecutor,
    handle: u64,
    num_rows: usize,
    byte_size: usize,
}

#[cfg(not(feature = "rust-only"))]
impl GpuBatch {
    pub fn new(
        executor: *mut PeacockExecutor,
        handle: u64,
        num_rows: usize,
        byte_size: usize,
    ) -> Self {
        Self {
            executor,
            handle,
            num_rows,
            byte_size,
        }
    }

    pub fn handle(&self) -> u64 {
        self.handle
    }

    pub fn executor(&self) -> *mut PeacockExecutor {
        self.executor
    }

    /// Hand the handle to an FFI call that consumes it — a slice, or an executor call
    /// taking it as an input. The batch is gone by move, and its release is skipped
    /// because C++ has erased the registry entry: releasing again would be a use of a
    /// dead handle. Every other way out of a `GpuBatch` runs `Drop`.
    pub fn consume(self) -> (*mut PeacockExecutor, u64) {
        gpu_batch::consume(self)
    }
}

/// Why a call failed: a message and no kind, because there is one response to all of them.
/// The driver adds the node and the lane and fails the query — a retry with a smaller batch
/// is #142's adaptive future and not this design.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackendError {
    pub message: String,
}

impl BackendError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

/// What a call gives back. Every one can fail, and a failure ends the query: the C++ side
/// resets the session and every resident table with it, so there is nothing to resume from.
pub type CallResult<T> = Result<(T, CallStats), BackendError>;

/// `scratch_bytes` is the measured transient; `None` when the run is not instrumented.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CallStats {
    pub scratch_bytes: Option<usize>,
}

// The executor contracts, one per node category.
//
// Every state transition emits in the same call, so there is no wrong interleaving to
// construct and output timing is a pure function of the call sequence. Every method that
// ends a protocol consumes `self`, which makes four run-time guards the prototype needed
// into compile errors: probing before `set_build`, a second `set_build`, probing after
// `finish_and_fetch`, and accumulating after `mark_done_and_fetch`. The source's consuming
// step removes a fifth — the driver's own exhaustion flag.

pub trait Executor {
    /// State held between calls.
    fn resident_bytes(&self) -> usize;

    /// Pre-call model. May consult `self`, so an accumulator includes its state. Calls
    /// with no input batch — `mark_done_and_fetch`, `finish_and_fetch` — are modeled
    /// with `n_rows = 0, n_bytes = 0`.
    fn scratch_bytes(&self, n_rows: u64, n_bytes: usize) -> usize;
}

pub trait ExecExecutor<B: Backend>: Executor {
    fn exec(&mut self, batch: B::Batch) -> CallResult<B::Batch>;
}

pub trait BatchAccumulatorExecutor<B: Backend>: Executor {
    fn accumulate_and_fetch(&mut self, batch: B::Batch) -> CallResult<Vec<B::Batch>>;
    fn mark_done_and_fetch(self) -> CallResult<Vec<B::Batch>>;
}

/// Parameterized by the BATCH rather than by the backend: an event carries a batch, and
/// binding it to a whole backend would make the type unnameable until one exists.
pub enum LaneEvent<B: Batch> {
    Batch(B),
    Done,
}

pub trait PartitionAccumulatorExecutor<B: Backend>: Executor {
    /// One call per lane event — the shape round-robin driving actually produces. The
    /// call delivering the last lane's `Done` is the emitting call.
    fn accumulate_and_fetch(
        &mut self,
        partition: usize,
        event: LaneEvent<B::Batch>,
    ) -> CallResult<Vec<B::Batch>>;
}

pub trait PartitionEmitterExecutor<B: Backend>: Executor {
    /// Exactly N outputs, some of them empty; N is a plan value, so the count is checked
    /// once inside the returned type rather than at each call site.
    fn emit(&mut self, batch: B::Batch) -> CallResult<Vec<B::Batch>>;
}

/// A typestate: build -> probe -> done, each transition consuming the last state.
pub trait JoinExecutor<B: Backend>: Executor {
    type Probing: ProbingJoin<B>;
    fn set_build(self, batch: B::Batch) -> CallResult<Self::Probing>;

    /// The build side finished without a batch — this lane's scatter gave it no build
    /// rows, which a small table over many lanes produces routinely. `Ok` means the lane
    /// owes nothing and ends here; an `Err` names a type whose answer is its probe side,
    /// which needs a call over a build table that does not exist.
    ///
    /// The driver asks rather than deciding, because what a lane owes is a property of
    /// the join type and the executor is where that lives.
    fn without_build(self) -> Result<(), BackendError>;
}

pub trait ProbingJoin<B: Backend>: Executor {
    fn probe_and_fetch(&mut self, batch: B::Batch) -> CallResult<Vec<B::Batch>>;
    fn finish_and_fetch(self) -> CallResult<Vec<B::Batch>>;
}

/// Exhaustion consumes the source, so the driver's slot IS its liveness.
pub enum SourceStep<B: Backend> {
    Batch {
        batch: B::Batch,
        stats: CallStats,
        source: B::Source,
    },
    Exhausted,
}

pub trait SourceExecutor<B: Backend>: Executor {
    fn next_batch(self) -> Result<SourceStep<B>, BackendError>;
}

/// `length: u64::MAX` means to the end. Straight through to the fetch's row range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RowRange {
    pub offset: u64,
    pub length: u64,
}

impl RowRange {
    /// Every row, which is what a node with no interval above it asks for.
    pub const WHOLE: Self = Self {
        offset: 0,
        length: u64::MAX,
    };

    /// Whether this names the whole of a batch that size — in which case the call needs no
    /// range at all, and the trace should not read as a trimmed one.
    pub fn covers(&self, n_rows: u64) -> bool {
        self.offset == 0 && self.length >= n_rows
    }

    /// The rows of a batch this range actually names, as `(offset, length)`. The twin of
    /// C++'s `clamp_row_range` (`node_session.cpp`), which the export and the slice share
    /// so that the two cannot disagree — this is the same rule for the backend that never
    /// crosses the ABI, and the two answering differently would be a divergence no test
    /// of either one alone could see.
    pub(crate) fn clamp(&self, n_rows: u64) -> (u64, u64) {
        row_range::clamp(self, n_rows)
    }
}

/// Unload is its own category because it is the one operator whose output is not
/// `B::Batch`: this is where data leaves the device, and the type says so. The row range
/// is a call argument because the count a root-adjacent limit derives from is cross-lane,
/// and an unload instance is per lane — only the driver holds that count.
pub trait UnloadExecutor<B: Backend>: Executor {
    fn unload(&mut self, batch: B::Batch, rows: RowRange) -> CallResult<CpuBatch>;
}

/// One impl per backend, naming a concrete type for the batch and for every executor
/// category. Backend choice is a turbofish at the entry point, not a selector consulted
/// per node.
pub trait Backend: Sized {
    /// What an executor is built from besides its node: the GPU's open session and the
    /// recipe plan its seqs address, the CPU's `TaskContext`.
    type Context;
    type Batch: Batch;
    type Source: SourceExecutor<Self>;
    type Exec: ExecExecutor<Self>;
    type BatchAcc: BatchAccumulatorExecutor<Self>;
    type PartAcc: PartitionAccumulatorExecutor<Self>;
    type Emitter: PartitionEmitterExecutor<Self>;
    type Join: JoinExecutor<Self>;
    type Unload: UnloadExecutor<Self>;

    /// A fresh instance set per call, so the driver instantiates per lane; `lane` is
    /// needed because a loader's lane picks its own row groups out of the partitioner's
    /// mapping. Construction lives here rather than on the node so that a node describes
    /// what it computes and stops knowing that backends exist.
    ///
    /// `post_order` is the node's position in a children-first walk, which is the address
    /// `RecipePlan` indexes by. It rides in from `PlanIndex`, which computes it in the
    /// walk it already makes: the schedule needs pre-order and the recipes are addressed
    /// post-order, and a backend deriving the second from the first would be a third walk
    /// that agrees with the other two by coincidence.
    fn executors_for(
        ctx: &Self::Context,
        node: &dyn GpuNode,
        post_order: usize,
        lane: usize,
    ) -> Result<NodeExecutors<Self>, PlanError>;
}

/// Which trait drives a node, with the executor stored inline — the match compiles to a
/// jump. `ProbingJoin` is absent because it comes from `set_build`, not from the backend.
pub enum NodeExecutors<B: Backend> {
    Source(B::Source),
    Exec(B::Exec),
    BatchAccumulator(B::BatchAcc),
    PartitionAccumulator(B::PartAcc),
    PartitionEmitter(B::Emitter),
    Join(B::Join),
    Unload(B::Unload),
    /// GpuMergePartitions, GpuUnion, GpuInterleave — routing only, no backend.
    BatchForwarder(Forwarder),
}

impl<B: Backend> NodeExecutors<B> {
    /// What the returned set drives. The driver checks it against the node's own
    /// category, so a backend wiring a node to the wrong trait fails where it was built
    /// rather than at the first call that finds the wrong method.
    pub fn category(&self) -> ExecutorCategory {
        match self {
            Self::Source(_) => ExecutorCategory::Source,
            Self::Exec(_) => ExecutorCategory::Exec,
            Self::BatchAccumulator(_) => ExecutorCategory::BatchAccumulator,
            Self::PartitionAccumulator(_) => ExecutorCategory::PartitionAccumulator,
            Self::PartitionEmitter(_) => ExecutorCategory::PartitionEmitter,
            Self::Join(_) => ExecutorCategory::Join,
            Self::Unload(_) => ExecutorCategory::Unload,
            Self::BatchForwarder(_) => ExecutorCategory::BatchForwarder,
        }
    }
}

/// Routes whole batches into a new lane numbering; never touches rows, never buffers.
/// No backends and no `CallStats` — routing is driver work, and a batch's bytes are
/// already accounted as driver-held in flight.
pub trait BatchForwarder {
    /// The (child index, child lane) pairs feeding output lane p, in service order.
    fn sources_of(&self, out_lane: usize) -> Vec<(usize, usize)>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Forwarder {
    /// N -> 1: lane 0 takes every lane of the one child, round-robin.
    MergePartitions { n: usize },
    /// Lane counts sum: output lane k is served by exactly one (child, lane).
    Union { lanes: Vec<(usize, usize)> },
    /// Child-major: output lane p is lane p of each child, which is why the inputs must
    /// share a hash distribution.
    Interleave { children: usize, n: usize },
}

/// The routing a node declares, which is a property of the node rather than of a backend —
/// every backend would compute the same thing, so it is read off the node by whoever routes.
pub fn forwarder_for(node: &dyn GpuNode) -> Forwarder {
    forwarder::forwarder_for(node)
}

/// Which check refused the call. It crosses the boundary as data because a caller may one
/// day answer the two differently — a pre-call refusal is a call that never ran, where a
/// post-call one is work already done. Today both end the query.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum When {
    PreCall,
    PostCall,
}

impl When {
    /// The words the message uses. Kept beside the variant so the field and the sentence
    /// cannot drift: the field is for code and the sentence is for a person, and neither
    /// replaces the other.
    pub fn describe(&self) -> &'static str {
        match self {
            Self::PreCall => "before the call",
            Self::PostCall => "after it",
        }
    }
}

/// What can go wrong while a plan runs, as against [`PlanError`], which is what can go
/// wrong before it does. A trip is a clean query failure, and what it promises is narrow:
/// a check that sees an accounted total over budget fails the query. The peak is an
/// observation taken elsewhere and is not what is checked, so it can exceed a budget that
/// never tripped — see the Memory accounting section of the task spec.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunError {
    /// One kind of failure with a phase, rather than two kinds: every caller that does not
    /// care about the phase — the DataFusion conversion, `Display` — keeps one arm, and a
    /// pair of variants mapping to the same thing is a pair that drifts.
    BudgetExceeded { when: When, message: String },
    /// A protocol violation no type can reach: a build side that produced zero or two
    /// batches, an `emit` returning other than N, a step after finishing.
    Protocol(String),
    /// A call failed. The session is gone with it, so the query is over.
    CallFailed(String),
    /// The backend has no executor for this node, or the set it returned drives a
    /// different category than the node is.
    Backend(PlanError),
}

/// What an executor was asked to do. An enum rather than a label: this is written once per
/// call, and a per-call format is a cost the trace does not need to impose.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallKind {
    NextBatch,
    SourceExhausted,
    Exec,
    Unload,
    /// An unload of part of a batch — the interval straddles one of its ends.
    UnloadRange,
    /// A batch outside the interval, released where it stood. No call was made, which is
    /// the whole saving and the thing a test on the rows returned cannot see.
    ReleaseUnwanted,
    Accumulate,
    MarkDone,
    SetBuild,
    /// A join lane whose build side ended with no batch — its scatter gave it no build
    /// rows. No call was made and none will be: what the lane owed was nothing.
    NoBuild,
    Probe,
    Finish,
    EndOfInput,
    Emit,
    EmitDone,
    /// One lane's batch delivered to a cross-lane accumulator.
    LaneEvent,
    /// One lane's end delivered to a cross-lane accumulator.
    LaneDone,
    Forward,
    ForwardDone,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TraceEvent {
    pub step: u32,
    pub node: u32,
    pub lane: u32,
    pub call: CallKind,
    pub outputs: u32,
}

/// A call whose modelled scratch came in under what it measured. Expected — a join's model
/// rests on a cardinality estimate — so it is recorded with its magnitude rather than
/// asserted away.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Underestimate {
    pub node: u32,
    pub lane: u32,
    pub modelled: usize,
    pub measured: usize,
}

impl Underestimate {
    /// How far under: 2.0 means the call used twice what was modelled.
    pub fn ratio(&self) -> f64 {
        if self.modelled == 0 {
            f64::INFINITY
        } else {
            self.measured as f64 / self.modelled as f64
        }
    }
}

/// One batch a node emitted. The driver reads both figures already — the rows for a limit
/// interval, the bytes for the accountant — and kept only totals until the corpus goldens
/// needed the sizes themselves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EmittedBatch {
    pub rows: u64,
    pub bytes: usize,
}

#[derive(Debug)]
pub struct RunReport {
    pub batches: Vec<CpuBatch>,
    pub peak_bytes: usize,
    /// Zero at the end of any correct run: a batch was held and never released otherwise.
    pub in_flight_bytes: usize,
    pub steps: usize,
    pub calls: usize,
    /// Batches held and batches released. Equal at the end of every run, on both the
    /// drained path and the early-exit one.
    pub holds: usize,
    pub releases: usize,
    pub trace: Vec<TraceEvent>,
    pub underestimates: Vec<Underestimate>,
    /// How many calls reported a measured transient rather than `None`. What makes an
    /// empty `underestimates` mean the model held: a backend measuring nothing produces
    /// the same empty list, and the two are indistinguishable without this.
    pub measured_calls: usize,
    /// Per node, rows released without an unload call — the saving a limit buys, made
    /// visible, since the rows returned look the same either way.
    pub rows_skipped: Vec<u64>,
    /// Per node, the most batches its queues held at once.
    pub peak_queued: Vec<usize>,
    /// Per node, its output lane count — what `peak_queued` is bounded by.
    pub lanes_of: Vec<usize>,
    /// Per node, per output lane, the batches it emitted in order.
    pub emitted: Vec<Vec<Vec<EmittedBatch>>>,
    /// Per node, per output lane, rows it emitted that nobody consumed — the queues an early
    /// exit left standing. Zero everywhere on a run that drained, and what closes
    /// `consumed + abandoned == the child's emitted` into an equality on every run.
    pub abandoned: Vec<Vec<u64>>,
    /// Per node, per child, per that child's lane, the rows this node consumed from it.
    /// Indexed by the child's lane rather than the consumer's, so it lines up with that
    /// child's own [`emitted`](RunReport::emitted) where the two differ — an emitter
    /// redistributes, so nothing else would sum.
    pub consumed: Vec<Vec<Vec<u64>>>,
    /// The nodes whose row interval was satisfied, in index order — empty on a run that
    /// drained. What the golden's `early_exit=` marker names, and the reason a lane can be
    /// short of what its plan called for: not a bool, because a reader of a smaller number
    /// needs to know which limit produced it.
    pub satisfied: Vec<usize>,
}

/// A join, as the schedule sees one: the range of its probe subtree, and how many lanes
/// have yet to leave their build phase.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct JoinShape {
    pub node: usize,
    /// `[start, end)` of the probe child's subtree in pre-order.
    pub probe: (usize, usize),
    pub lanes: usize,
}

/// The tree as plain numbers. Node indices are pre-order, so a subtree is a contiguous
/// range and "order" — the leftmost tie-break — is the index itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PlanShape {
    pub heights: Vec<u32>,
    pub lanes: Vec<usize>,
    pub subtree: Vec<(usize, usize)>,
    pub joins: Vec<JoinShape>,
}

impl PlanShape {
    pub(crate) fn node_count(&self) -> usize {
        self.heights.len()
    }
}

pub(crate) struct IndexedNode<'a> {
    pub node: &'a dyn GpuNode,
    pub category: ExecutorCategory,
    pub children: Vec<usize>,
    pub parent: Option<usize>,
    pub lanes: usize,
    /// This node's position in a children-first walk, which is what `RecipePlan` indexes
    /// by and what a backend hands to `executors_for`. Pre-order is what the schedule
    /// needs and post-order is what the recipes are addressed by; the two are computed
    /// here together so nothing has to reconcile them later.
    pub post_order: usize,
    /// The child's lane count, which is how many `Done` events a partition accumulator
    /// owes. Zero for every other category.
    pub input_lanes: usize,
    pub interval: Option<RowInterval>,
    /// How many independently-ready units the schedule tracks for this node. Output lanes
    /// for most, but a cross-lane accumulator becomes ready one input lane at a time and
    /// an emitter reads a single one.
    pub ready_lanes: usize,
    /// Where this node's accounting slots start: one per lane when it is lane-scoped,
    /// one for the node otherwise.
    pub slot_base: usize,
}

pub(crate) struct PlanIndex<'a> {
    pub nodes: Vec<IndexedNode<'a>>,
    pub shape: PlanShape,
    pub slots: usize,
}

impl<'a> PlanIndex<'a> {
    pub(crate) fn build(root: &'a dyn GpuNode) -> Result<Self, PlanError> {
        driver::build_index(root)
    }

    pub(crate) fn len(&self) -> usize {
        self.nodes.len()
    }

    pub(crate) fn slot(&self, node: usize, lane: usize) -> usize {
        driver::slot_of(self, node, lane)
    }
}

pub(crate) const ROOT: usize = 0;

/// Run `root` to completion on `B`. `budget` of `None` accounts without ever tripping.
pub fn run<B: Backend>(
    root: &dyn GpuNode,
    ctx: &B::Context,
    budget: Option<usize>,
) -> Result<RunReport, RunError> {
    driver::run::<B>(root, ctx, budget)
}

/// Every node's post-order address, indexed by its pre-order one — the numbering the
/// recipes and the FFI share.
pub fn post_order_of_every_node(root: &dyn GpuNode) -> Result<Vec<usize>, PlanError> {
    driver::post_order_of_every_node(root)
}
