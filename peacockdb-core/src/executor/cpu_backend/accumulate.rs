//! The accumulators: the nodes that hold a lane's batches back, and two that are accumulators
//! by category alone — the mid-plan limit, which holds nothing, and an aggregate's init,
//! which holds a flag.
//!
//! One type with a variant per behaviour, because a backend names one type for the
//! category and what these five do differs completely. The variant is what a call site
//! reads: a coalesce that concatenates, a sort that orders the whole stream, a merge that
//! folds state on a threshold, a limit that forwards, slices or drops, and an aggregate's
//! init that runs per batch with rows and once more over nothing at done.

use std::sync::Arc;

use datafusion::arrow::array::RecordBatch;
use datafusion::arrow::compute::concat_batches;
use datafusion::arrow::datatypes::{Schema as ArrowSchema, SchemaRef};
use datafusion::execution::TaskContext;
use datafusion::physical_plan::ExecutionPlan;
use datafusion::physical_plan::projection::ProjectionExec;
use datafusion::physical_plan::sorts::sort::SortExec;

use super::expr_physical::physical_projection;
use super::{
    CpuAccumulator, CpuExec, CpuPartitionAccumulator, aggregate_exec, declared_as, lex_ordering,
    placeholder, run_node,
};
use crate::executor::CpuBatch;
use crate::executor::{BackendError, CallResult, CallStats, LaneEvent};
use crate::plan::PlanError;
use crate::plan::{
    GpuAccumulateBatchesAndSort, GpuAggregate, GpuAggregateBatches, GpuCoalesceAllBatches,
    GpuLimit, GpuMergeSortedPartitions,
};
use crate::plan::{GpuNode, RowInterval};
use crate::plan::{Phase, finalize_columns};

/// What a `CpuAccumulator` holds between calls, one variant per node.
pub(crate) enum State {
    Coalesce(Coalesce),
    Sorted(SortedRuns),
    Aggregate(AggregateBatches),
    Limit(LimitStream),
    Init(AggregateInit),
}

impl CpuAccumulator {
    fn of(state: State) -> Self {
        Self { state }
    }

    pub(crate) fn coalesce(_node: &GpuCoalesceAllBatches, input: &ArrowSchema) -> Self {
        Self::of(State::Coalesce(Coalesce {
            held: Vec::new(),
            schema: Arc::new(input.clone()),
        }))
    }

    pub(crate) fn sorted(
        node: &GpuAccumulateBatchesAndSort,
        input: &ArrowSchema,
        ctx: Arc<TaskContext>,
    ) -> Result<Self, PlanError> {
        let ordering = lex_ordering(&node.keys, input)?;
        Ok(Self::of(State::Sorted(SortedRuns {
            held: Vec::new(),
            sort: Arc::new(SortExec::new(ordering, placeholder(input))),
            fetch: node.fetch,
            schema: Arc::new(input.clone()),
            ctx,
        })))
    }

    /// `compact_bytes` is the held-state size at which a compaction runs. It has no
    /// default here: the number comes from the same budget rule that sizes loader batches,
    /// which is the driver's (T17), and a stand-in default would be a policy nobody chose.
    pub(crate) fn aggregate(
        node: &GpuAggregateBatches,
        input: &ArrowSchema,
        ctx: Arc<TaskContext>,
        compact_bytes: usize,
    ) -> Result<Self, PlanError> {
        let body = &node.body;
        let state = node.intermediate();
        let merge = aggregate_exec(body, Phase::Merge, input, state, ctx.as_ref())?
            .pop()
            .expect("a merge is one operator");
        let output = node.kind().schema().expect("an aggregate is not a sink");
        let finalize = match body.finalize {
            Some(_) => {
                let columns = finalize_columns(body, state, output)?;
                let exprs = physical_projection(&columns, &state.fields, ctx.as_ref())?;
                Some(Arc::new(
                    ProjectionExec::try_new(exprs, placeholder(&state.fields)).map_err(
                        |error| PlanError::Invalid(format!("the finalize project: {error}")),
                    )?,
                ) as Arc<dyn ExecutionPlan>)
            }
            None => None,
        };
        let ctx = super::always_aggregating(ctx);
        Ok(Self::of(State::Aggregate(AggregateBatches {
            merge,
            finalize,
            state: None,
            pending: Vec::new(),
            pending_bytes: 0,
            threshold: compact_bytes,
            compactions: 0,
            held: state.fields.clone(),
            output: output.fields.clone(),
            ctx,
        })))
    }

    pub(crate) fn limit(node: &GpuLimit) -> Self {
        Self::of(State::Limit(LimitStream {
            interval: node.interval,
            seen: 0,
        }))
    }

    /// An aggregate's init: the exec node's operators, run per batch with rows, and once over
    /// an empty batch of its input at done where nothing went out.
    pub(crate) fn aggregate_init(
        node: &GpuAggregate,
        input: &ArrowSchema,
        ctx: Arc<TaskContext>,
    ) -> Result<Self, PlanError> {
        Ok(Self::of(State::Init(AggregateInit {
            chain: CpuExec::aggregate(node, input, ctx)?,
            input: Arc::new(input.clone()),
            emitted: false,
        })))
    }

    pub(crate) fn accumulate_and_fetch(&mut self, batch: CpuBatch) -> CallResult<Vec<CpuBatch>> {
        match &mut self.state {
            State::Coalesce(state) => state.accumulate_and_fetch(batch),
            State::Sorted(state) => state.accumulate_and_fetch(batch),
            State::Aggregate(state) => state.accumulate_and_fetch(batch),
            State::Limit(state) => state.accumulate_and_fetch(batch),
            State::Init(state) => state.accumulate_and_fetch(batch),
        }
    }

    pub(crate) fn mark_done_and_fetch(self) -> CallResult<Vec<CpuBatch>> {
        match self.state {
            State::Coalesce(state) => state.mark_done_and_fetch(),
            State::Sorted(state) => state.mark_done_and_fetch(),
            State::Aggregate(state) => state.mark_done_and_fetch(),
            State::Limit(state) => state.mark_done_and_fetch(),
            State::Init(state) => state.mark_done_and_fetch(),
        }
    }
}

/// What an accumulator answers with at done: one batch of everything it held, and nothing
/// at all where nothing arrived.
///
/// Both backends emit nothing where no batch arrived, before any call is made, which is
/// what makes them one engine here; the device's collapse answers the same way and the C++
/// guard behind it is never reached. A lane whose one arrival was a zero-row batch holds a
/// batch and emits it, which is how a scatter's kept empty reaches a join that owes rows
/// (#175). A grouped merge over no arrivals owes no groups, so nothing is also its answer. A
/// keyless merge always meets its lanes' identity rows, since a keyless init answers one at
/// done.
fn coalesce_or_nothing(schema: &SchemaRef, held: &[RecordBatch]) -> CallResult<Vec<CpuBatch>> {
    if held.is_empty() {
        return Ok((Vec::new(), CallStats::default()));
    }
    let batch = concat_batches(schema, held.iter())
        .map_err(|error| BackendError::new(format!("joining the lane's batches: {error}")))?;
    Ok((vec![CpuBatch::new(batch)], CallStats::default()))
}

/// A lane's batches concatenated into one at done.
pub(crate) struct Coalesce {
    held: Vec<RecordBatch>,
    schema: SchemaRef,
}

impl Coalesce {
    pub(crate) fn held(&self) -> &[RecordBatch] {
        &self.held
    }

    fn accumulate_and_fetch(&mut self, batch: CpuBatch) -> CallResult<Vec<CpuBatch>> {
        self.held.push(batch.into_record_batch());
        Ok((Vec::new(), CallStats::default()))
    }

    fn mark_done_and_fetch(self) -> CallResult<Vec<CpuBatch>> {
        coalesce_or_nothing(&self.schema, &self.held)
    }
}

/// A lane's batches ordered into one at done.
///
/// The device sorts each batch and merges the runs, which is two calls; here it is one sort
/// over the concatenation, and the same answer — a stable sort of the runs in arrival order
/// puts tied rows exactly where merging them would.
///
/// The fetch is applied by slicing rather than by `SortExec::with_fetch`, and that is the
/// point: the top-N path keeps a bounded heap and does not preserve arrival order among
/// ties, so which of two tied rows a limit kept would depend on the heap. An oracle cannot
/// answer that differently from run to run.
pub(crate) struct SortedRuns {
    held: Vec<RecordBatch>,
    sort: Arc<dyn ExecutionPlan>,
    fetch: Option<usize>,
    schema: SchemaRef,
    ctx: Arc<TaskContext>,
}

impl SortedRuns {
    pub(crate) fn held(&self) -> &[RecordBatch] {
        &self.held
    }

    fn accumulate_and_fetch(&mut self, batch: CpuBatch) -> CallResult<Vec<CpuBatch>> {
        self.held.push(batch.into_record_batch());
        Ok((Vec::new(), CallStats::default()))
    }

    fn mark_done_and_fetch(self) -> CallResult<Vec<CpuBatch>> {
        sorted_and_cut(&self.sort, self.held, self.fetch, &self.schema, &self.ctx)
    }
}

impl CpuPartitionAccumulator {
    pub(crate) fn merge_sorted(
        node: &GpuMergeSortedPartitions,
        lanes: usize,
        input: &ArrowSchema,
        ctx: Arc<TaskContext>,
    ) -> Result<Self, PlanError> {
        let ordering = lex_ordering(&node.keys, input)?;
        Ok(Self {
            per_lane: vec![Vec::new(); lanes],
            live: lanes,
            sort: Arc::new(SortExec::new(ordering, placeholder(input))),
            fetch: node.fetch,
            schema: Arc::new(input.clone()),
            ctx,
        })
    }

    /// What each lane is holding, for the accounting the driver sums.
    pub(crate) fn per_lane(&self) -> impl Iterator<Item = &[RecordBatch]> {
        self.per_lane.iter().map(|lane| lane.as_slice())
    }

    pub(crate) fn accumulate_and_fetch(
        &mut self,
        partition: usize,
        event: LaneEvent<CpuBatch>,
    ) -> CallResult<Vec<CpuBatch>> {
        match event {
            LaneEvent::Batch(batch) => {
                self.per_lane[partition].push(batch.into_record_batch());
                return Ok((Vec::new(), CallStats::default()));
            }
            LaneEvent::Done => self.live -= 1,
        }
        if self.live > 0 {
            return Ok((Vec::new(), CallStats::default()));
        }
        let partition_major: Vec<RecordBatch> = std::mem::take(&mut self.per_lane)
            .into_iter()
            .flatten()
            .collect();
        sorted_and_cut(
            &self.sort,
            partition_major,
            self.fetch,
            &self.schema,
            &self.ctx,
        )
    }
}

/// The fetch as a slice of an ordered batch — see [`SortedRuns`] for why it is not
/// DataFusion's top-N.
fn first_rows(batch: CpuBatch, fetch: Option<usize>) -> CpuBatch {
    let batch = batch.into_record_batch();
    match fetch {
        Some(fetch) if fetch < batch.num_rows() => CpuBatch::new(batch.slice(0, fetch)),
        _ => CpuBatch::new(batch),
    }
}

/// Every held batch as one ordered stream, cut to the fetch and answered as one batch, and
/// nothing where nothing arrived.
///
/// Batches that arrived holding no row are concatenated, not sorted: DataFusion's sort
/// answers zero rows with no batch at all, which [`coalesce_or_nothing`] would read as a lane
/// that received nothing. The device answers one batch of zero rows there, as every other
/// accumulator does. A fetch has no rows to cut.
fn sorted_and_cut(
    sort: &Arc<dyn ExecutionPlan>,
    held: Vec<RecordBatch>,
    fetch: Option<usize>,
    schema: &SchemaRef,
    ctx: &Arc<TaskContext>,
) -> CallResult<Vec<CpuBatch>> {
    if held.iter().all(|batch| batch.num_rows() == 0) {
        return coalesce_or_nothing(schema, &held);
    }
    let sorted = run_node(sort, vec![held], ctx)?;
    let (ordered, _) = coalesce_or_nothing(schema, &sorted)?;
    Ok((
        ordered
            .into_iter()
            .map(|batch| first_rows(batch, fetch))
            .collect(),
        CallStats::default(),
    ))
}

/// Pre-aggregated state merged into pre-aggregated state, emitted at done.
///
/// Compaction runs on a threshold that doubles when it fails to pay. Compacting on every
/// arrival re-scans the whole state once per batch, which is quadratic where the groups
/// are disjoint and nothing merges; never compacting holds the whole input where the
/// cardinality is high. So arrivals are held until they cross the threshold, folded once,
/// and the threshold set to twice what that fold left behind.
pub(crate) struct AggregateBatches {
    merge: Arc<dyn ExecutionPlan>,
    /// The finalize project, where this node finishes the aggregate. Its presence is the
    /// only thing that distinguishes a merging node from a finalizing one.
    finalize: Option<Arc<dyn ExecutionPlan>>,
    state: Option<RecordBatch>,
    pending: Vec<RecordBatch>,
    pending_bytes: usize,
    threshold: usize,
    /// How many times the threshold was reached. Read by the tests, which are what pins
    /// the doubling: without it a disjoint-key aggregate compacts once per arrival.
    compactions: usize,
    held: SchemaRef,
    output: SchemaRef,
    ctx: Arc<TaskContext>,
}

impl AggregateBatches {
    pub(crate) fn held_bytes(&self) -> usize {
        self.state
            .as_ref()
            .map(RecordBatch::get_array_memory_size)
            .unwrap_or(0)
            + self.pending_bytes
    }

    fn accumulate_and_fetch(&mut self, batch: CpuBatch) -> CallResult<Vec<CpuBatch>> {
        let batch = batch.into_record_batch();
        self.pending_bytes += batch.get_array_memory_size();
        self.pending.push(batch);
        if self.held_bytes() >= self.threshold {
            self.compact()?;
        }
        Ok((Vec::new(), CallStats::default()))
    }

    /// Fold every pending arrival into the state, and raise the bar. A compaction that did
    /// not shrink will not shrink next time either — the keys are simply distinct — so the
    /// threshold moves to twice what is now held rather than re-scanning it per arrival.
    fn compact(&mut self) -> Result<(), BackendError> {
        let mut inputs = Vec::with_capacity(self.pending.len() + 1);
        inputs.extend(self.state.take());
        inputs.append(&mut self.pending);
        self.pending_bytes = 0;
        let merged = run_node(&self.merge, vec![inputs], &self.ctx)?;
        let merged = declared(merged, &self.held)?;
        let state = concat_batches(&self.held, merged.iter())
            .map_err(|error| BackendError::new(format!("joining the merged state: {error}")))?;
        self.threshold = self.threshold.max(2 * state.get_array_memory_size());
        self.state = Some(state);
        self.compactions += 1;
        Ok(())
    }

    /// Fold what is pending and emit the state, finalized where this node finishes the
    /// aggregate — nothing where nothing arrived. A keyless merge's lanes each carry their
    /// init's identity row at least, so nothing is never a keyless answer here.
    fn mark_done_and_fetch(mut self) -> CallResult<Vec<CpuBatch>> {
        if !self.pending.is_empty() {
            self.compact()?;
        }
        let Some(state) = self.state else {
            return coalesce_or_nothing(&self.output, &[]);
        };
        let Some(finalize) = self.finalize else {
            return coalesce_or_nothing(&self.held, &[state]);
        };
        let finalized = run_node(&finalize, vec![vec![state]], &self.ctx)?;
        coalesce_or_nothing(&self.output, &declared(finalized, &self.output)?)
    }
}

/// The mid-plan limit: it streams, holding nothing.
///
/// Per batch, from a running count of the rows that have gone past: one entirely outside
/// the interval is dropped, one entirely inside is forwarded untouched, and only the two
/// that straddle its ends are sliced. Its input is one lane — the node checks that — so
/// the count it keeps is the count of the stream.
pub(crate) struct LimitStream {
    interval: RowInterval,
    seen: u64,
}

impl LimitStream {
    fn accumulate_and_fetch(&mut self, batch: CpuBatch) -> CallResult<Vec<CpuBatch>> {
        let batch = batch.into_record_batch();
        let n_rows = batch.num_rows() as u64;
        let rows = self.interval.range_of(self.seen, n_rows);
        self.seen += n_rows;
        let Some(rows) = rows else {
            return Ok((Vec::new(), CallStats::default()));
        };
        let kept = if rows.covers(n_rows) {
            batch
        } else {
            batch.slice(rows.offset as usize, rows.length as usize)
        };
        Ok((vec![CpuBatch::new(kept)], CallStats::default()))
    }

    fn mark_done_and_fetch(self) -> CallResult<Vec<CpuBatch>> {
        Ok((Vec::new(), CallStats::default()))
    }
}

/// An aggregate's init. A zero-row batch is dropped uncalled, so adding one changes no row;
/// a batch with rows runs the init (and the shortcut's finalize); done runs them over an empty
/// batch where nothing went out — DataFusion's identity row keyless, no groups grouped.
pub(crate) struct AggregateInit {
    chain: CpuExec,
    input: SchemaRef,
    emitted: bool,
}

impl AggregateInit {
    fn accumulate_and_fetch(&mut self, batch: CpuBatch) -> CallResult<Vec<CpuBatch>> {
        if batch.record_batch().num_rows() == 0 {
            return Ok((Vec::new(), CallStats::default()));
        }
        let (out, stats) = self.chain.exec(batch)?;
        self.emitted |= out.record_batch().num_rows() > 0;
        Ok((vec![out], stats))
    }

    fn mark_done_and_fetch(mut self) -> CallResult<Vec<CpuBatch>> {
        if self.emitted {
            return Ok((Vec::new(), CallStats::default()));
        }
        let nothing = CpuBatch::new(RecordBatch::new_empty(self.input.clone()));
        let (out, stats) = self.chain.exec(nothing)?;
        match out.record_batch().num_rows() {
            0 => Ok((Vec::new(), stats)),
            _ => Ok((vec![out], stats)),
        }
    }
}

fn declared(
    batches: Vec<RecordBatch>,
    schema: &SchemaRef,
) -> Result<Vec<RecordBatch>, BackendError> {
    batches
        .into_iter()
        .map(|batch| declared_as(batch, schema))
        .collect()
}

#[cfg(test)]
mod tests;
