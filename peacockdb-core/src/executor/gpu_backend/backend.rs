//! `GpuBackend`: the seven executor types this backend names, and the memory each reports.
//!
//! Every executor here runs the recipe its node published, so the whole of building one is
//! finding that recipe — which is what the context carries beside the session pointer, and
//! why `executors_for` takes a post-order rather than deriving one.

use datafusion::arrow::datatypes::Schema as ArrowSchema;

use super::accumulate::State;
use super::join::PairScratch;
use super::{
    GpuAccumulator, GpuEmitter, GpuExec, GpuExport, GpuJoin, GpuPartitionAccumulator,
    GpuProbingJoin, GpuSource,
};
use crate::executor::Batch;
use crate::executor::CpuBatch;
use crate::executor::GpuBatch;
use crate::executor::forwarder_for;
use crate::executor::{Backend, NodeExecutors};
use crate::executor::{
    BackendError, BatchAccumulatorExecutor, CallResult, ExecExecutor, Executor, JoinExecutor,
    LaneEvent, PartitionAccumulatorExecutor, PartitionEmitterExecutor, ProbingJoin, RowRange,
    SourceExecutor, SourceStep, UnloadExecutor,
};
use super::CallSite;
use crate::executor::{GpuBackend, GpuContext};
use crate::plan::GpuNode;
use crate::plan::PlanError;
use crate::plan::{NodeRef, as_node_ref};
use datafusion::common::JoinType;

/// The threshold a batch aggregate compacts at, until the driver derives one from the
/// budget the way the loader's batch size is derived (#142).
const COMPACT_BYTES: usize = 1 << 26;

impl Backend for GpuBackend {
    type Context = GpuContext;
    type Batch = GpuBatch;
    type Source = GpuSource;
    type Exec = GpuExec;
    type BatchAcc = GpuAccumulator;
    type PartAcc = GpuPartitionAccumulator;
    type Emitter = GpuEmitter;
    type Join = GpuJoin;
    type Unload = GpuExport;

    fn executors_for(
        ctx: &GpuContext,
        node: &dyn GpuNode,
        post_order: usize,
        lane: usize,
    ) -> Result<NodeExecutors<Self>, PlanError> {
        // Routing is the driver's, and an empty relation reads nothing, so neither has a
        // recipe to look for: its absence is what they mean rather than a gap to report.
        match as_node_ref(node) {
            NodeRef::MergePartitions(_) | NodeRef::Union(_) | NodeRef::Interleave(_) => {
                return Ok(NodeExecutors::BatchForwarder(forwarder_for(node)));
            }
            NodeRef::Empty(_) => return Ok(NodeExecutors::Source(GpuSource::empty())),
            _ => {}
        }
        let recipe = ctx.recipes.get(post_order).ok_or_else(|| {
            PlanError::Invalid(format!(
                "{} is at {post_order} in the tree, where the recipes hold no entry — a node \
                 that makes ABI calls has one",
                node.name()
            ))
        })?;
        // The lane belongs to the executor rather than to the call: C++ answers a call
        // with the output slot inside it, which is 0 for every lane at four lanes.
        let site = CallSite { executor: ctx.executor, node: post_order, lane };
        let out = |node: &dyn GpuNode| -> ArrowSchema {
            node.kind()
                .schema()
                .expect("a node that is not a sink declares its columns")
                .fields
                .as_ref()
                .clone()
        };
        let input = |ordinal: usize| out(node.children()[ordinal]);
        Ok(match as_node_ref(node) {
            NodeRef::LoadParquet(load) => {
                NodeExecutors::Source(GpuSource::new(site, recipe, load, &out(node))?)
            }
            NodeRef::Filter(_) | NodeRef::Project(_) | NodeRef::Sort(_) => {
                NodeExecutors::Exec(GpuExec::new(site, recipe, None, &out(node))?)
            }
            // The init: per batch with rows, and at done where nothing went out. Its first
            // call answers with the state its finalize reads, and nothing builds a batch from
            // that, so the state schema is the only thing that can price it.
            NodeRef::Aggregate(aggregate) => {
                NodeExecutors::BatchAccumulator(GpuAccumulator::aggregate_init(
                    site,
                    recipe,
                    &aggregate.intermediate().fields.as_ref().clone(),
                    &out(node),
                )?)
            }
            NodeRef::CoalesceAllBatches(_) => NodeExecutors::BatchAccumulator(
                GpuAccumulator::coalesce(site, recipe, &out(node))?,
            ),
            NodeRef::AccumulateBatchesAndSort(_) => NodeExecutors::BatchAccumulator(
                GpuAccumulator::sorted(site, recipe, &out(node))?,
            ),
            NodeRef::AggregateBatches(merge) => {
                NodeExecutors::BatchAccumulator(GpuAccumulator::aggregate(
                    site,
                    recipe,
                    &merge.intermediate().fields.as_ref().clone(),
                    &out(node),
                    COMPACT_BYTES,
                )?)
            }
            NodeRef::Limit(limit) => NodeExecutors::BatchAccumulator(GpuAccumulator::limit(
                site,
                recipe,
                limit.interval,
                &out(node),
            )?),
            NodeRef::MergeSortedPartitions(_) => {
                let lanes = node.children()[0]
                    .kind()
                    .layout()
                    .expect("a merge's input is not a sink")
                    .n;
                NodeExecutors::PartitionAccumulator(GpuPartitionAccumulator::merge_sorted(
                    site,
                    recipe,
                    lanes,
                    &out(node),
                )?)
            }
            NodeRef::EmitPartitions(_) => {
                NodeExecutors::PartitionEmitter(GpuEmitter::new(site, recipe, &out(node))?)
            }
            // One session executor for the three nodes, as the wire writes one `CudfJoin`.
            NodeRef::Join(_) | NodeRef::CrossJoin(_) | NodeRef::NestedLoopJoin(_) => {
                let (join_type, keyed, filter_columns, distinct_key_width) = match as_node_ref(node)
                {
                    NodeRef::Join(join) => (
                        join.join_type,
                        true,
                        &join.filter_columns[..],
                        join.distinct_key_width(),
                    ),
                    NodeRef::NestedLoopJoin(join) => {
                        (join.join_type, false, &join.filter_columns[..], 0)
                    }
                    _ => (JoinType::Inner, false, &[][..], 0),
                };
                let scratch = PairScratch::new(
                    keyed,
                    filter_columns,
                    &input(0),
                    &input(1),
                    ctx.recipes.join_scratch_bytes(),
                );
                NodeExecutors::Join(GpuJoin::new(
                    site,
                    recipe,
                    join_type,
                    scratch,
                    distinct_key_width,
                    &out(node),
                )?)
            }
            NodeRef::Unload(_) => NodeExecutors::Unload(GpuExport::new(site, &input(0))),
            NodeRef::MergePartitions(_)
            | NodeRef::Union(_)
            | NodeRef::Interleave(_)
            | NodeRef::Empty(_) => {
                unreachable!("routing nodes and the empty relation are answered above")
            }
        })
    }
}

/// A source holds nothing between calls: the handle a read produces is its output, and
/// the row groups it still owes are a list, not a table.
impl Executor for GpuSource {
    fn resident_bytes(&self) -> usize {
        0
    }
    fn scratch_bytes(&self, _n_rows: u64, _n_bytes: usize) -> usize {
        0
    }
}

impl SourceExecutor<GpuBackend> for GpuSource {
    fn next_batch(mut self) -> Result<SourceStep<GpuBackend>, BackendError> {
        match self.read_next()? {
            Some((batch, stats)) => Ok(SourceStep::Batch {
                batch,
                stats,
                source: self,
            }),
            None => Ok(SourceStep::Exhausted),
        }
    }
}

/// An exec node holds nothing between calls; its transient is the table the call is about
/// to build, which is bounded by the batch it reads.
impl Executor for GpuExec {
    fn resident_bytes(&self) -> usize {
        0
    }
    fn scratch_bytes(&self, _n_rows: u64, n_bytes: usize) -> usize {
        n_bytes
    }
}

impl ExecExecutor<GpuBackend> for GpuExec {
    fn exec(&mut self, batch: GpuBatch) -> CallResult<GpuBatch> {
        GpuExec::exec(self, batch)
    }
}

impl Executor for GpuAccumulator {
    fn resident_bytes(&self) -> usize {
        self.held_bytes()
    }
    /// What it holds plus what arrives: a compaction reads both at once, and the merge
    /// about to fold them is the transient the accountant has to have room for.
    fn scratch_bytes(&self, _n_rows: u64, n_bytes: usize) -> usize {
        self.held_bytes() + n_bytes
    }
}

impl BatchAccumulatorExecutor<GpuBackend> for GpuAccumulator {
    fn accumulate_and_fetch(&mut self, batch: GpuBatch) -> CallResult<Vec<GpuBatch>> {
        GpuAccumulator::accumulate_and_fetch(self, batch)
    }
    fn mark_done_and_fetch(self) -> CallResult<Vec<GpuBatch>> {
        GpuAccumulator::mark_done_and_fetch(self)
    }
}

impl Executor for GpuPartitionAccumulator {
    fn resident_bytes(&self) -> usize {
        self.held_bytes()
    }
    fn scratch_bytes(&self, _n_rows: u64, n_bytes: usize) -> usize {
        self.held_bytes() + n_bytes
    }
}

impl PartitionAccumulatorExecutor<GpuBackend> for GpuPartitionAccumulator {
    fn accumulate_and_fetch(
        &mut self,
        partition: usize,
        event: LaneEvent<GpuBatch>,
    ) -> CallResult<Vec<GpuBatch>> {
        GpuPartitionAccumulator::accumulate_and_fetch(self, partition, event)
    }
}

/// A scatter holds nothing: its N outputs are the batch it was handed, redistributed.
impl Executor for GpuEmitter {
    fn resident_bytes(&self) -> usize {
        0
    }
    fn scratch_bytes(&self, _n_rows: u64, n_bytes: usize) -> usize {
        n_bytes
    }
}

impl PartitionEmitterExecutor<GpuBackend> for GpuEmitter {
    fn emit(&mut self, batch: GpuBatch) -> CallResult<Vec<GpuBatch>> {
        GpuEmitter::emit(self, batch)
    }
}

/// Before its build side arrives a join holds nothing; `set_build` builds the hash table
/// and the `matched` column over the batch it consumes.
impl Executor for GpuJoin {
    fn resident_bytes(&self) -> usize {
        0
    }
    fn scratch_bytes(&self, n_rows: u64, _n_bytes: usize) -> usize {
        self.built_bytes(n_rows) as usize
    }
}

impl JoinExecutor<GpuBackend> for GpuJoin {
    type Probing = GpuProbingJoin;
    fn set_build(self, batch: Option<GpuBatch>) -> CallResult<GpuProbingJoin> {
        GpuJoin::set_build(self, batch)
    }
}

impl Executor for GpuProbingJoin {
    /// What the session holds until it is released.
    fn resident_bytes(&self) -> usize {
        self.held_bytes() as usize
    }
    /// The probe batch, an output as long, and the pairs the session bounds by
    /// `chunk_bytes`; at finish, the unmatched build rows.
    fn scratch_bytes(&self, n_rows: u64, n_bytes: usize) -> usize {
        self.call_bytes(n_rows, n_bytes as u64) as usize
    }
}

impl ProbingJoin<GpuBackend> for GpuProbingJoin {
    fn probe_and_fetch(&mut self, batch: GpuBatch) -> CallResult<Option<GpuBatch>> {
        GpuProbingJoin::probe_and_fetch(self, batch)
    }
    fn finish_and_fetch(self) -> CallResult<Option<GpuBatch>> {
        GpuProbingJoin::finish_and_fetch(self)
    }
    fn owes_nothing(&self) -> bool {
        GpuProbingJoin::owes_nothing(self)
    }
}

/// The export holds nothing and its transient is the rows crossing out — the IPC buffer
/// the range names, which is why a limit's range is worth having.
impl Executor for GpuExport {
    fn resident_bytes(&self) -> usize {
        0
    }
    fn scratch_bytes(&self, _n_rows: u64, n_bytes: usize) -> usize {
        n_bytes
    }
}

impl UnloadExecutor<GpuBackend> for GpuExport {
    fn unload(&mut self, batch: GpuBatch, rows: RowRange) -> CallResult<CpuBatch> {
        GpuExport::unload(self, batch, rows)
    }
}

/// Held bytes, which every accumulator answers off the handles it is keeping.
trait HeldBytes {
    fn held_bytes(&self) -> usize;
}

impl HeldBytes for GpuAccumulator {
    fn held_bytes(&self) -> usize {
        match &self.state {
            State::Coalesce(state) => bytes_of(state.held()),
            State::Sorted(state) => bytes_of(state.held()),
            State::Aggregate(state) => state.held_bytes(),
            // A limit holds nothing, which is the whole point of the slice symbol.
            State::Limit(_) => 0,
            // An init holds a flag.
            State::Init(_) => 0,
        }
    }
}

impl HeldBytes for GpuPartitionAccumulator {
    fn held_bytes(&self) -> usize {
        self.per_lane().map(bytes_of).sum()
    }
}

fn bytes_of(batches: &[GpuBatch]) -> usize {
    batches.iter().map(GpuBatch::byte_size).sum()
}
