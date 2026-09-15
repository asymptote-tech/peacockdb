//! The accumulators on a device: what each one holds between calls, and which of its
//! recipe's calls it makes when.
//!
//! Holding is the whole difference from an exec node. A handle an FFI call consumes is
//! gone by move, and one this executor is sitting on is a `GpuBatch` — so a lane the
//! driver abandons releases what it held rather than leaking it.
//!
//! One type with a variant per node, since a backend names one type for the category.

use std::sync::Arc;

use datafusion::arrow::datatypes::{Schema as ArrowSchema, SchemaRef};

use peacockdb_ffi::raw::peacock_executor_slice_handle;

use crate::common::logical_size_from_schema;

use super::{
    CallSite, Consumed, execute_node, hand_over, last_error, no_abi_calls, one_call, priced,
};
use crate::executor::Batch;
use crate::executor::GpuBatch;
use crate::executor::node_timing_on;
use crate::executor::{
    AbiCall, AbiCalls, AbiTarget, BackendError, CallResult, CallStats, LaneEvent,
};
use crate::plan::PlanError;
use crate::plan::RowInterval;
use crate::wire::{AbiSymbol, Call, CallPattern, FbKind, Input, Recipe, Seq};

/// A `BatchAccumulator` node's executor, one variant per node.
pub enum GpuAccumulator {
    Coalesce(Collapse),
    Sorted(SortedRuns),
    Aggregate(AggregateBatches),
    Limit(LimitStream),
}

impl GpuAccumulator {
    /// One call at done, over everything the lane accumulated.
    pub fn coalesce(site: CallSite, recipe: &Recipe, schema: &ArrowSchema) -> Result<Self, PlanError> {
        let [call] = recipe.calls.as_slice() else {
            return Err(shape("a coalesce makes one call, at done", recipe));
        };
        Ok(Self::Coalesce(Collapse {
            site,
            collapse: addressed(call, CallPattern::AtDone, Input::LaneBatches)?,
            held: Vec::new(),
            schema: Arc::new(schema.clone()),
        }))
    }

    /// A sort per batch, then one merge over the runs at done.
    pub fn sorted(site: CallSite, recipe: &Recipe, schema: &ArrowSchema) -> Result<Self, PlanError> {
        let [per_batch, at_done] = recipe.calls.as_slice() else {
            return Err(shape(
                "an accumulating sort makes two calls: a sort per batch and a merge at done",
                recipe,
            ));
        };
        Ok(Self::Sorted(SortedRuns {
            site,
            sort: addressed(per_batch, CallPattern::PerBatch, Input::Batch)?,
            merge: addressed(at_done, CallPattern::AtDone, Input::LaneBatches)?,
            held: Vec::new(),
            schema: Arc::new(schema.clone()),
        }))
    }

    /// A concat and a merge per compaction and again at done, plus the finalize project
    /// where this node finishes the aggregate. `compact_bytes` is the held size that
    /// triggers a compaction; it comes from the budget rule, which is the driver's.
    pub fn aggregate(
        site: CallSite,
        recipe: &Recipe,
        state: &ArrowSchema,
        output: &ArrowSchema,
        compact_bytes: usize,
    ) -> Result<Self, PlanError> {
        let (concat, merge, finalize) = match recipe.calls.as_slice() {
            [concat, merge] => (concat, merge, None),
            [concat, merge, finalize] => (
                concat,
                merge,
                Some(addressed(
                    finalize,
                    CallPattern::AtDone,
                    Input::PriorOutput,
                )?),
            ),
            _ => {
                return Err(shape(
                    "a batch aggregate makes a concat and a merge, and a finalize project \
                     where it finishes the aggregate",
                    recipe,
                ));
            }
        };
        Ok(Self::Aggregate(AggregateBatches {
            site,
            concat: addressed(concat, CallPattern::PerCompaction, Input::LaneBatches)?,
            merge: addressed(merge, CallPattern::PerCompaction, Input::PriorOutput)?,
            finalize,
            state: None,
            pending: Vec::new(),
            threshold: compact_bytes,
            compactions: 0,
            held: Arc::new(state.clone()),
            output: Arc::new(output.clone()),
        }))
    }

    /// No seq at all: a limit's bounds are runtime values, so it addresses the slice
    /// symbol rather than a node.
    pub fn limit(
        site: CallSite,
        recipe: &Recipe,
        interval: RowInterval,
        schema: &ArrowSchema,
    ) -> Result<Self, PlanError> {
        let [call] = recipe.calls.as_slice() else {
            return Err(shape(
                "a limit makes one call, per straddling batch",
                recipe,
            ));
        };
        if call.symbol != AbiSymbol::SliceHandle || call.when != CallPattern::PerStraddlingBatch {
            return Err(shape(
                "a limit slices the batches that straddle its ends and calls nothing else",
                recipe,
            ));
        }
        Ok(Self::Limit(LimitStream {
            site,
            interval,
            seen: 0,
            schema: Arc::new(schema.clone()),
        }))
    }

    pub fn accumulate_and_fetch(&mut self, batch: GpuBatch) -> CallResult<Vec<GpuBatch>> {
        match self {
            Self::Coalesce(state) => state.accumulate_and_fetch(batch),
            Self::Sorted(state) => state.accumulate_and_fetch(batch),
            Self::Aggregate(state) => state.accumulate_and_fetch(batch),
            Self::Limit(state) => state.accumulate_and_fetch(batch),
        }
    }

    pub fn mark_done_and_fetch(self) -> CallResult<Vec<GpuBatch>> {
        match self {
            Self::Coalesce(state) => state.mark_done_and_fetch(),
            Self::Sorted(state) => state.mark_done_and_fetch(),
            Self::Aggregate(state) => state.mark_done_and_fetch(),
            Self::Limit(state) => state.mark_done_and_fetch(),
        }
    }
}

/// The seq a call addresses, once the call is what this node's recipe should hold. A
/// recipe that names a different pattern or a different input is one this executor would
/// drive at the wrong moment, which is a wrong answer rather than a failed call.
fn addressed(call: &Call, when: CallPattern, input: Input) -> Result<(Seq, FbKind), PlanError> {
    if call.when != when || call.inputs.as_slice() != [input] {
        return Err(PlanError::Invalid(format!(
            "this node's call is {call:?} where the executor drives {when:?} over {input:?}"
        )));
    }
    call.target
        .ok_or_else(|| PlanError::Invalid(format!("{call:?} addresses no seq")))
}

fn shape(expected: &str, recipe: &Recipe) -> PlanError {
    PlanError::Invalid(format!("{expected}, and this one is `{recipe}`"))
}

/// A lane's batches concatenated into one at done, and nothing at all where none arrived.
///
/// The collapse arm does answer a call with no handles — but with a table of no columns,
/// which is not the batch of the node's schema a SingleBatch output owes downstream. So
/// this backend emits nothing and the empty batch is the driver's to supply, which is the
/// one place that knows the schema without asking the device.
pub struct Collapse {
    site: CallSite,
    collapse: (Seq, FbKind),
    held: Vec<GpuBatch>,
    schema: SchemaRef,
}

impl Collapse {
    pub fn held(&self) -> &[GpuBatch] {
        &self.held
    }

    fn accumulate_and_fetch(&mut self, batch: GpuBatch) -> CallResult<Vec<GpuBatch>> {
        self.held.push(batch);
        Ok((Vec::new(), no_abi_calls()))
    }

    fn mark_done_and_fetch(self) -> CallResult<Vec<GpuBatch>> {
        if self.held.is_empty() {
            return Ok((Vec::new(), no_abi_calls()));
        }
        let mut calls = AbiCalls::armed(node_timing_on());
        let out = one_call(
            self.site,
            self.collapse,
            self.held,
            &self.schema,
            &mut calls,
        )?;
        Ok((
            vec![out],
            CallStats {
                scratch_bytes: None,
                calls,
            },
        ))
    }
}

/// Each batch sorted as it arrives, the runs merged into one at done — and nothing at all
/// where none arrived, since a merge of no runs is the collapse of nothing by another name
/// and the device refuses that (#173).
pub struct SortedRuns {
    site: CallSite,
    sort: (Seq, FbKind),
    merge: (Seq, FbKind),
    held: Vec<GpuBatch>,
    schema: SchemaRef,
}

impl SortedRuns {
    pub fn held(&self) -> &[GpuBatch] {
        &self.held
    }

    fn accumulate_and_fetch(&mut self, batch: GpuBatch) -> CallResult<Vec<GpuBatch>> {
        let mut calls = AbiCalls::armed(node_timing_on());
        let run = one_call(self.site, self.sort, vec![batch], &self.schema, &mut calls)?;
        self.held.push(run);
        Ok((
            Vec::new(),
            CallStats {
                scratch_bytes: None,
                calls,
            },
        ))
    }

    fn mark_done_and_fetch(self) -> CallResult<Vec<GpuBatch>> {
        if self.held.is_empty() {
            return Ok((Vec::new(), no_abi_calls()));
        }
        let mut calls = AbiCalls::armed(node_timing_on());
        let out = one_call(self.site, self.merge, self.held, &self.schema, &mut calls)?;
        Ok((
            vec![out],
            CallStats {
                scratch_bytes: None,
                calls,
            },
        ))
    }
}

/// Pre-aggregated state merged into pre-aggregated state, emitted at done.
///
/// The threshold doubles when a compaction fails to shrink what it folded — see the CPU
/// backend's copy of this rule, which is the same rule and the same reason.
pub struct AggregateBatches {
    site: CallSite,
    concat: (Seq, FbKind),
    merge: (Seq, FbKind),
    finalize: Option<(Seq, FbKind)>,
    state: Option<GpuBatch>,
    pending: Vec<GpuBatch>,
    threshold: usize,
    compactions: usize,
    held: SchemaRef,
    output: SchemaRef,
}

impl AggregateBatches {
    pub(crate) fn held_bytes(&self) -> usize {
        self.state.as_ref().map(Batch::byte_size).unwrap_or(0)
            + self.pending.iter().map(Batch::byte_size).sum::<usize>()
    }

    fn accumulate_and_fetch(&mut self, batch: GpuBatch) -> CallResult<Vec<GpuBatch>> {
        self.pending.push(batch);
        // The one node whose calls are conditional: a batch that does not tip the threshold
        // makes none at all. That is why the log is threaded rather than derived — nothing
        // outside this method can tell which batch compacted.
        let mut calls = AbiCalls::armed(node_timing_on());
        if self.held_bytes() >= self.threshold {
            self.compact(&mut calls)?;
        }
        Ok((
            Vec::new(),
            CallStats {
                scratch_bytes: None,
                calls,
            },
        ))
    }

    fn compact(&mut self, calls: &mut AbiCalls) -> Result<(), BackendError> {
        let mut folding = Vec::with_capacity(self.pending.len() + 1);
        folding.extend(self.state.take());
        folding.append(&mut self.pending);
        let (seq, kind) = self.concat;
        let (handles, taken) = hand_over(folding);
        let (concatenated, concat_stats) = execute_node(self.site, seq, kind, &[handles])?;
        // The concat's output is state, not the node's row — nothing here builds a batch
        // from it, so the state schema is the only thing that can price it.
        let concat_bytes = priced(concat_stats, &self.held);
        calls.record(AbiCall {
            seq,
            target: AbiTarget::Node(kind),
            call_index: 0,
            in_rows: taken.rows,
            in_bytes: taken.bytes,
            out_rows: concat_stats.rows,
            out_bytes: concat_bytes,
        });
        let (seq, kind) = self.merge;
        let (handle, stats) = execute_node(self.site, seq, kind, &[vec![concatenated]])?;
        let state = super::produced(self.site.executor, seq, handle, stats, &self.held);
        calls.record(AbiCall {
            seq,
            target: AbiTarget::Node(kind),
            call_index: 0,
            in_rows: concat_stats.rows,
            in_bytes: concat_bytes,
            out_rows: stats.rows,
            out_bytes: state.byte_size() as u64,
        });
        self.threshold = self.threshold.max(2 * state.byte_size());
        self.state = Some(state);
        self.compactions += 1;
        Ok(())
    }

    fn mark_done_and_fetch(mut self) -> CallResult<Vec<GpuBatch>> {
        let mut calls = AbiCalls::armed(node_timing_on());
        if !self.pending.is_empty() {
            self.compact(&mut calls)?;
        }
        let stats_of = |calls| CallStats {
            scratch_bytes: None,
            calls,
        };
        let Some(state) = self.state.take() else {
            return Ok((Vec::new(), stats_of(calls)));
        };
        let Some((seq, kind)) = self.finalize else {
            return Ok((vec![state], stats_of(calls)));
        };
        let out = one_call(
            self.site,
            (seq, kind),
            vec![state],
            &self.output,
            &mut calls,
        )?;
        Ok((vec![out], stats_of(calls)))
    }
}

/// The one node of the partition-accumulator category: every lane's sorted run merged into
/// one at the last lane's done, and nothing where no lane sent anything — a merge of no
/// runs is the collapse of nothing under another name, and the device refuses that (#173).
///
/// One call per lane event, since that is what round-robin driving produces, and the call
/// carrying the last `Done` is the emitting one. The handles go into the merge in lane
/// order, which is what makes a tie partition-major rather than arrival-ordered.
pub struct GpuPartitionAccumulator {
    site: CallSite,
    merge: (Seq, FbKind),
    per_lane: Vec<Vec<GpuBatch>>,
    live: usize,
    schema: SchemaRef,
}

impl GpuPartitionAccumulator {
    /// What each lane is holding, for the accounting the driver sums.
    pub fn per_lane(&self) -> impl Iterator<Item = &[GpuBatch]> {
        self.per_lane.iter().map(|lane| lane.as_slice())
    }

    pub fn merge_sorted(
        site: CallSite,
        recipe: &Recipe,
        lanes: usize,
        schema: &ArrowSchema,
    ) -> Result<Self, PlanError> {
        let [call] = recipe.calls.as_slice() else {
            return Err(shape(
                "a merge of sorted partitions makes one call, at done",
                recipe,
            ));
        };
        Ok(Self {
            site,
            merge: addressed(call, CallPattern::AtDone, Input::AllLanes)?,
            per_lane: (0..lanes).map(|_| Vec::new()).collect(),
            live: lanes,
            schema: Arc::new(schema.clone()),
        })
    }

    pub fn accumulate_and_fetch(
        &mut self,
        partition: usize,
        event: LaneEvent<GpuBatch>,
    ) -> CallResult<Vec<GpuBatch>> {
        match event {
            LaneEvent::Batch(batch) => {
                self.per_lane[partition].push(batch);
                return Ok((Vec::new(), no_abi_calls()));
            }
            LaneEvent::Done => self.live -= 1,
        }
        if self.live > 0 {
            return Ok((Vec::new(), no_abi_calls()));
        }
        let held: Vec<GpuBatch> = std::mem::take(&mut self.per_lane)
            .into_iter()
            .flatten()
            .collect();
        if held.is_empty() {
            return Ok((Vec::new(), no_abi_calls()));
        }
        let mut calls = AbiCalls::armed(node_timing_on());
        let out = one_call(self.site, self.merge, held, &self.schema, &mut calls)?;
        Ok((
            vec![out],
            CallStats {
                scratch_bytes: None,
                calls,
            },
        ))
    }
}

/// The mid-plan limit: it streams, holding nothing.
///
/// A batch entirely outside the interval is released where it stands, one entirely inside
/// is forwarded untouched, and only the two that straddle its ends are sliced. Its input
/// is one lane — the node checks that — so the count it keeps is the stream's.
pub struct LimitStream {
    site: CallSite,
    interval: RowInterval,
    seen: u64,
    schema: SchemaRef,
}

impl LimitStream {
    pub fn seen(&self) -> u64 {
        self.seen
    }

    fn accumulate_and_fetch(&mut self, batch: GpuBatch) -> CallResult<Vec<GpuBatch>> {
        let n_rows = batch.num_rows() as u64;
        let rows = self.interval.range_of(self.seen, n_rows);
        self.seen += n_rows;
        let Some(rows) = rows else {
            // Released here rather than shipped and trimmed, which is the whole saving on
            // an offset: the prefix a skip drops is unbounded.
            return Ok((Vec::new(), no_abi_calls()));
        };
        if rows.covers(n_rows) {
            return Ok((vec![batch], no_abi_calls()));
        }
        // The session is this executor's, as everywhere on this path: a batch carries the
        // pointer only so that dropping it can release its handle.
        let taken = Consumed::of(&batch);
        // A limit carries no seq of its own, so the region C++ opens for the slice is
        // charged to the node that produced the handle — and the trimmed rows are still
        // that node's output, so the batch below keeps the same producer.
        let seq = batch.producer();
        let (_, handle) = batch.consume();
        let mut sliced = 0u64;
        let rc = unsafe {
            peacock_executor_slice_handle(
                self.site.executor,
                handle,
                rows.offset,
                rows.length,
                &mut sliced,
            )
        };
        if rc != 0 {
            return Err(BackendError::new(format!(
                "slice_handle({handle}, {}..+{}): {}",
                rows.offset,
                rows.length,
                last_error(self.site.executor)
            )));
        }
        // The slice reports no stats, so the batch is priced from the rows asked for: a
        // range that clamps is one this node computed against the batch it holds.
        let kept = GpuBatch::new(
            self.site.executor,
            sliced,
            seq,
            rows.length as usize,
            logical_size_from_schema(&self.schema, rows.length as usize, 0),
        );
        let mut calls = AbiCalls::armed(node_timing_on());
        calls.record(AbiCall {
            seq,
            target: AbiTarget::Bare(AbiSymbol::SliceHandle),
            call_index: 0,
            in_rows: taken.rows,
            in_bytes: taken.bytes,
            out_rows: kept.num_rows() as u64,
            out_bytes: kept.byte_size() as u64,
        });
        Ok((
            vec![kept],
            CallStats {
                scratch_bytes: None,
                calls,
            },
        ))
    }

    fn mark_done_and_fetch(self) -> CallResult<Vec<GpuBatch>> {
        Ok((Vec::new(), no_abi_calls()))
    }
}
