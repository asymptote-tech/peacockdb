//! Running a plan: the contracts a backend implements, the batch that crosses them, and the
//! driver that walks the tree calling them.
//!
//! The drivers are generic over [`Backend`], so each backend monomorphizes — no vtable, and
//! `Drop` on a GPU handle is a direct call. That is also what keeps the tier boundary
//! structural: a rust-only target instantiates the drivers only at the CPU backend.
//!
//! The API is not the same in every feature shape. [`GpuBatch`] and the GPU backend exist
//! only where cuDF is linked, so the cfg sits on the declarations rather than only on their
//! modules. The reasons are in `llm-wiki/architecture.md`, under Execution.
mod cpu_batch;
mod driver;
mod errors;
mod forwarder;
#[cfg(not(feature = "rust-only"))]
mod instrument;
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

use std::collections::HashMap;

use datafusion::arrow::array::RecordBatch;

use crate::plan::ExecutorCategory;
use crate::plan::PlanError;
use crate::plan::{GpuNode, RowInterval};
use crate::wire::{AbiSymbol, FbKind, Seq};

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
/// is skipped. The executor pointer is BORROWED, as everywhere else on this path: the
/// session outlives every batch drawn from it.
#[cfg(not(feature = "rust-only"))]
pub struct GpuBatch {
    executor: *mut PeacockExecutor,
    handle: u64,
    producer: Seq,
    num_rows: usize,
    byte_size: usize,
}

#[cfg(not(feature = "rust-only"))]
impl GpuBatch {
    /// `producer` is the seq of the call this handle came out of — the same thing C++
    /// records at every registry insert, and the only way a slice or an export can name
    /// the node whose output it handled.
    pub fn new(
        executor: *mut PeacockExecutor,
        handle: u64,
        producer: Seq,
        num_rows: usize,
        byte_size: usize,
    ) -> Self {
        Self {
            executor,
            handle,
            producer,
            num_rows,
            byte_size,
        }
    }

    pub fn handle(&self) -> u64 {
        self.handle
    }

    /// The seq of the call that produced this handle. A slice propagates it: the trimmed
    /// rows are still that node's output, which is what C++ charges the region to.
    pub fn producer(&self) -> Seq {
        self.producer
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

/// What an ABI call addressed: a plan node by its kind, or one of the two symbols whose
/// arguments are runtime row counts and that name no node at all.
///
/// The same split [`Call::target`](crate::wire::Call) makes on the wire, and for the same
/// reason: a slice and an export are handed a handle, so the plan has no seq for them and
/// [`FbKind`] has no member either. A record still has to print something for them, and
/// the symbol is what they are.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AbiTarget {
    Node(FbKind),
    Bare(AbiSymbol),
}

impl std::fmt::Display for AbiTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Node(kind) => write!(f, "{kind}"),
            Self::Bare(symbol) => write!(f, "{}", symbol.name()),
        }
    }
}

/// One ABI call an executor made, as its CALLER saw it.
///
/// C++ measures what a call cost. What it was handed and what it produced are priced here:
/// the call consumes its handles, so by the time the far side could measure the input the
/// registry entries are gone (#152), and C++ prices nothing. The two halves meet on
/// `(seq, call_index)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AbiCall {
    /// The node whose output this call handled. Its own for the two symbols that name a
    /// seq; for a slice and an export, the node that produced the handle — which is what
    /// C++ charges the region to, and the two must agree or the join cannot close.
    pub seq: Seq,
    pub target: AbiTarget,
    /// Which call of this seq the session had reached — the number C++ answers with, and
    /// the other half of the key the two records meet on.
    ///
    /// Zero as the backend records it and stamped by the driver, which is the one place
    /// that sees every call in the order they were made. An executor sees only its own.
    pub call_index: u64,
    pub in_rows: u64,
    pub in_bytes: u64,
    pub out_rows: u64,
    pub out_bytes: u64,
}

/// The ABI calls one executor call made, collected only while measuring.
///
/// `None` is not "made no calls" — it is "nobody was measuring", and a reader that cannot
/// tell those apart reports a silent backend as a fast one. Boxed so an unmeasured run
/// carries one null pointer rather than a vector's three words, which is the shape the C++
/// side uses for the same reason.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AbiCalls(Option<Box<Vec<AbiCall>>>);

impl AbiCalls {
    /// Armed only for a measured run: unarmed, `record` is a branch and nothing else.
    pub fn armed(measuring: bool) -> Self {
        Self(measuring.then(Box::default))
    }

    pub fn record(&mut self, call: AbiCall) {
        if let Some(calls) = &mut self.0 {
            calls.push(call);
        }
    }

    /// `None` where the run was not measured, which is what keeps an unmeasured node from
    /// rendering as one that made no calls.
    pub fn recorded(&self) -> Option<&[AbiCall]> {
        self.0.as_deref().map(Vec::as_slice)
    }

    /// For the driver alone, to stamp `call_index` — see the field.
    pub fn recorded_mut(&mut self) -> Option<&mut [AbiCall]> {
        self.0.as_deref_mut().map(Vec::as_mut_slice)
    }
}

/// `scratch_bytes` is the measured transient; `None` when the run is not instrumented.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CallStats {
    pub scratch_bytes: Option<usize>,
    /// The calls this one made, for a measured run only. The driver holds the coordinates
    /// — which node, which lane, which batch — so a backend reports only what it alone
    /// knows: the seq it addressed and what it handed over.
    pub calls: AbiCalls,
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

/// The CPU backend: every call relays to a DataFusion operator.
pub struct CpuBackend;

/// The GPU backend: every call crosses the ABI.
#[cfg(not(feature = "rust-only"))]
pub struct GpuBackend;

/// What an executor on the GPU backend is built from: the open session, and the recipes whose
/// seqs address the plan that session was given.
///
/// The pointer is BORROWED, as everywhere on this path — the session outlives every
/// executor drawn from it, and the handles they hand each other.
#[cfg(not(feature = "rust-only"))]
pub struct GpuContext {
    pub executor: *mut PeacockExecutor,
    pub recipes: crate::wire::RecipePlan,
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

/// One of this engine's expressions back in DataFusion's own vocabulary, which is what the
/// CPU backend hands its operators.
///
/// `#[cfg(test)]` because its only caller outside `cpu_backend` is a test in `plan`, and a
/// test in another component cannot reach an implementation module. Without the cfg a plain
/// build reports it dead.
#[cfg(test)]
pub(crate) fn physical_expr(
    expr: &crate::plan::Expr,
    input: &datafusion::arrow::datatypes::Schema,
    registry: &dyn datafusion::execution::FunctionRegistry,
) -> Result<std::sync::Arc<dyn datafusion::physical_plan::PhysicalExpr>, PlanError> {
    cpu_backend::physical_expr(expr, input, registry)
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
    /// Per node, per driving lane, the ABI calls each of that lane's backend calls made,
    /// in order: one entry per call that reached an executor and none for a step the
    /// driver answered itself. A backend that reports nothing leaves every entry
    /// unmeasured rather than empty — the distinction `AbiCalls` carries.
    pub abi_calls: Vec<Vec<Vec<AbiCalls>>>,
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

/// One measured region, as C++ reports it: which call it belonged to and what that call
/// cost on the host and on the device. Its key is `(seq, call_index)`, which is what an
/// [`AbiCall`] carries — the two records are halves of one row and meet there.
///
/// Here rather than in `gpu_backend` for the reason [`AbiCalls`] lives in `executor`: this
/// is what a measurement IS, and the backend fills one in. Not the ABI struct, so a
/// `rust-only` build with no backend still has a driver that compiles.
///
/// One call can answer with several, one per output partition, and a call's cost is their
/// sum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Region {
    pub seq: Seq,
    pub partition: usize,
    pub call_index: u64,
    pub host_us: u64,
    pub device_us: u64,
}

/// What one call cost and produced: the device's half summed over the call's regions, the
/// price of its output taken from the [`AbiCall`] beside them.
///
/// Summed rather than picked because a call answering with several output partitions
/// spreads itself across them, so any single region is a fraction of the call.
///
/// Named for the measurement rather than for the time because it carries both: the output
/// of a call in the middle of a node's chain exists on this side nowhere else.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Measured {
    /// Steady clock across the whole call, this call's regions added.
    pub host_us: u64,
    /// Between each region's two CUDA events. Zero where a region recorded no complete
    /// pair — it touched no device.
    pub device_us: u64,
    /// Rows this answered with, as the caller priced them. The only place a middle call's
    /// output exists: a node driving several hands its caller the last one's and drops
    /// the rest.
    pub out_rows: u64,
    pub out_bytes: u64,
    /// Regions that answered. One on a [`Region`], where it is what makes the count fall
    /// out of the sum; zero on a call means the device recorded none, which is not the
    /// same as a call that cost nothing.
    pub regions: usize,
}

impl std::ops::AddAssign for Measured {
    /// Field for field, because every field of this is additive over calls — including
    /// `regions` itself. The one operation the summations in `driver::measurements` need,
    /// so that a field added above is summed everywhere without visiting them.
    fn add_assign(&mut self, other: Self) {
        self.host_us += other.host_us;
        self.device_us += other.device_us;
        self.out_rows += other.out_rows;
        self.out_bytes += other.out_bytes;
        self.regions += other.regions;
    }
}

/// Every call of a run, measured — at both granularities a reader needs.
///
/// The device answers per `(seq, call_index)`, so **the split between the seqs of one
/// driver call is measured**: a per-entry total alone would attribute the whole to each
/// seq. The two consumers want different units, and both are honest:
///
/// | | unit | why |
/// |---|---|---|
/// | `.benchmark.txt` | a driver call | its axis is lanes × batches, and a batch is a driver call |
/// | `records.tsv` | one cuDF call | its row is one call, and the device measured each |
#[derive(Debug)]
pub struct Measurements {
    /// Node → driving lane → the calls that lane made, each the sum over the seqs it
    /// addressed. `None` for a call the run did not measure, so an unmeasured backend
    /// reads as absent rather than as free.
    per_entry: Vec<Vec<Vec<Option<Measured>>>>,
    per_call: HashMap<(Seq, u64), Measured>,
}

impl Measurements {
    /// Lanes of a node, each with one entry per call it made. For walking the shape without
    /// knowing its lengths.
    pub fn lanes(&self, node: usize) -> &[Vec<Option<Measured>>] {
        &self.per_entry[node]
    }

    pub fn nodes(&self) -> usize {
        self.per_entry.len()
    }

    /// What one cuDF call cost, as the device reported it — the unit a record row is in.
    pub fn call(&self, seq: Seq, call_index: u64) -> Option<Measured> {
        self.per_call.get(&(seq, call_index)).copied()
    }
}

/// Cost every journalled call from the regions the device answered with, refusing a
/// mismatch in either direction — see `driver::measurements`.
pub fn join_regions(report: &RunReport, regions: &[Region]) -> Result<Measurements, String> {
    driver::join_regions(report, regions)
}

/// One node's whole cost, summed over every lane and every call it made; `None` where
/// the node was not measured at all.
pub fn node_measured(times: &Measurements, node: usize) -> Option<Measured> {
    driver::node_measured(times, node)
}

/// Each node as a record names it — its type and its post-order position — in the
/// driver's own pre-order, the order [`RunReport`] is indexed by.
pub fn nodes_as_recorded(root: &dyn GpuNode) -> Result<Vec<(&'static str, usize)>, PlanError> {
    driver::nodes_as_recorded(root)
}

/// Which device allocator a measurement was taken under — the outcome of
/// [`install_rmm_pool`], not the request.
///
/// cuDF routes every intermediate through rmm's current device resource, and the
/// difference between a pool and rmm's default (a `cudaMalloc`/`cudaFree` per
/// allocation) is far larger than run-to-run noise — worst on exactly the nodes with
/// the largest outputs. So `Unavailable` does not describe a slower run to be recorded
/// and compared; it describes a run whose times mean nothing, and the benchmark harness
/// refuses it.
#[cfg(not(feature = "rust-only"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RmmPool {
    /// A pooled resource is installed. Sizes are what it was actually built with.
    Pool { integrated: bool, free_bytes: u64, initial_bytes: u64, maximum_bytes: u64 },
    /// The pool could not be built — typically a neighbour holding the device when the
    /// reservation was computed — so rmm's default resource is in place and nobody
    /// chose that.
    Unavailable,
}

#[cfg(not(feature = "rust-only"))]
impl std::fmt::Display for RmmPool {
    /// The `allocator=` line of a benchmark record. One line, no spaces around `=`,
    /// sizes in GiB because that is the unit the sizing rule is written in.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        const GIB: f64 = 1073741824.0;
        match *self {
            RmmPool::Pool { integrated, free_bytes, initial_bytes, maximum_bytes } => write!(
                f,
                "rmm-pool initial={:.1}GiB max={:.1}GiB of {:.1}GiB free on {} device",
                initial_bytes as f64 / GIB,
                maximum_bytes as f64 / GIB,
                free_bytes as f64 / GIB,
                if integrated { "an integrated" } else { "a discrete" },
            ),
            RmmPool::Unavailable => {
                write!(f, "rmm-default (pool unavailable), cudaMalloc per allocation")
            }
        }
    }
}

/// How per-node GPU regions are measured. `Off` by default, because measuring is not
/// free.
#[cfg(not(feature = "rust-only"))]
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum NodeTiming {
    /// No measurement. Every timing field stays 0.
    #[default]
    Off,
    /// CUDA events around the device work, host clock around the host work, no sync
    /// inside the region. Device numbers arrive via `collect_regions` after the
    /// root materialize, into [`PartitionStat::device_us`].
    Events,
}

/// Closes the range [`nvtx_range`] opened.
#[cfg(not(feature = "rust-only"))]
pub struct NvtxRange(());

#[cfg(not(feature = "rust-only"))]
impl Drop for NvtxRange {
    fn drop(&mut self) {
        instrument::nvtx_pop()
    }
}

/// Install the pooled device allocator and report what happened. Idempotent; the guard
/// lives in C++ so the process has one pool whichever side asks first.
#[cfg(not(feature = "rust-only"))]
pub fn install_rmm_pool() -> RmmPool {
    instrument::install_rmm_pool()
}

/// Select the per-node timing mode (process-global, `Off` by default).
#[cfg(not(feature = "rust-only"))]
pub fn set_node_timing(mode: NodeTiming) {
    instrument::set_node_timing(mode)
}

/// Whether the run is measured — what arms the per-call journal.
#[cfg(not(feature = "rust-only"))]
pub fn node_timing_on() -> bool {
    instrument::node_timing_on()
}

/// Emit NVTX ranges around plan nodes and their output partitions (process-global, off
/// by default).
#[cfg(not(feature = "rust-only"))]
pub fn set_nvtx_ranges(on: bool) {
    instrument::set_nvtx_ranges(on)
}

/// A named NVTX range around whatever the caller is about to do, closed on drop.
#[cfg(not(feature = "rust-only"))]
#[must_use = "the range closes when this is dropped, so dropping it at once ranges nothing"]
pub fn nvtx_range(name: &str) -> NvtxRange {
    instrument::nvtx_range(name)
}

/// Drain what the session recorded, after the root export and before the plan ends — the
/// events die with the plan, and the device times do not exist until then.
#[cfg(not(feature = "rust-only"))]
pub fn collect_regions(executor: *mut PeacockExecutor) -> Result<Vec<Region>, BackendError> {
    gpu_backend::collect_regions(executor)
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
