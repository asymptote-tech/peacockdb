//! [`run`](crate::executor::run) — the tree, the queues, the schedule, and everything
//! cross-partition.
//!
//! One step runs every lane of the node the schedule picked: smallest height, ties
//! leftmost. That single rule is what makes this a push model — a batch's parent is
//! strictly lower, so it is carried up before anything below produces again — and what
//! bounds every queue at one batch per lane without a cap. Lane-scoped work is delegated
//! to [`super::single_partition`]; the three cross-lane categories are here, along with
//! the hold on a satisfied limit — an unload's interval, whose count only the driver can
//! keep, and a mid-plan `GpuLimit`, judged by the rows it emitted.

use std::collections::{HashMap, VecDeque};

use super::StepError;
use super::accounting::{Held, ResidentAccountant, Slot, Trip};
use super::index::PROBE_CHILD;
use super::queue::OutQueue;
use super::scheduler::Scheduler;
use super::single_partition::{LaneCall, LaneDriver, LaneOutputs, LaneSite};
use crate::executor::{
    AbiCalls, Backend, BackendError, Batch, BatchForwarder, CallKind, CpuBatch, EmittedBatch,
    Forwarder, FusedRun, LaneEvent, NodeExecutors, OutputHook, PartitionAccumulatorExecutor,
    PartitionEmitterExecutor, PlanIndex, ROOT, RowRange, RunError, RunReport, TraceEvent,
};
use crate::plan::ExecutorCategory;
use crate::plan::GpuNode;
use crate::plan::PlanError;
use crate::plan::check_canonical_form_of_roots;
use crate::wire::Seq;

mod answers;
mod early_exit;
mod hold_buffer;
mod queues;
#[cfg(test)]
mod tests;

/// A step neither moves a batch nor finalizes a lane only if the schedule is wrong, so a
/// run that does not end is a bug here rather than a query that is merely large.
const DEFAULT_MAX_STEPS: usize = 1_000_000;

/// Run `root` to completion on `B`. `budget` of `None` accounts without ever tripping.
pub(crate) fn run<B: Backend>(
    root: &dyn GpuNode,
    ctx: &B::Context,
    budget: Option<usize>,
) -> Result<RunReport, RunError> {
    run_with_hook::<B>(root, ctx, budget, None)
}

/// [`run`] with an output hook — see [`OutputHook`].
pub(crate) fn run_with_hook<'a, B: Backend>(
    root: &'a dyn GpuNode,
    ctx: &'a B::Context,
    budget: Option<usize>,
    hook: Option<OutputHook<'a, B>>,
) -> Result<RunReport, RunError> {
    let mut driver = Driver::<B>::new(root, ctx, budget)?;
    driver.hook = hook;
    driver.run(DEFAULT_MAX_STEPS)
}

/// Several roots, one answer each. The caller has validated the plan as a DAG.
pub(crate) fn run_roots_with_hook<'a, B: Backend>(
    roots: &[&'a dyn GpuNode],
    ctx: &'a B::Context,
    budget: Option<usize>,
    hook: Option<OutputHook<'a, B>>,
) -> Result<FusedRun, RunError> {
    let mut driver = Driver::<B>::for_roots(roots, ctx, budget)?;
    driver.hook = hook;
    driver.run_fused(DEFAULT_MAX_STEPS)
}

pub(crate) struct Driver<'a, B: Backend> {
    index: PlanIndex<'a>,
    ctx: &'a B::Context,
    /// Called on every batch a node queues as its own device output, after it is recorded
    /// and before it is queued. The unload's host batches and a forwarder's moves are not
    /// emissions of a `B::Batch` and never reach it.
    hook: Option<OutputHook<'a, B>>,
    scheduler: Scheduler,
    acct: ResidentAccountant,
    states: Vec<NodeState<B>>,
    /// Per root, in the order the roots were given, what its unload handed back.
    results: Vec<Vec<CpuBatch>>,
    /// Per node, which root it is, if it is one: where an unload's batches go.
    root_of: Vec<Option<usize>>,
    trace: Vec<TraceEvent>,
    steps: usize,
    /// Per unload carrying an interval, rows of its input stream seen so far, summed over
    /// every lane; zero for every other node. Only the driver can hold this: an unload
    /// instance is one lane's and the count is not.
    rows_seen: Vec<u64>,
    /// Per mid-plan limit, rows it has emitted. Its executor keeps the only count of its
    /// one-lane input and makes every cut; this is what those cuts let through, read off
    /// its outputs rather than computed a second time (#234).
    rows_emitted: Vec<u64>,
    rows_skipped: Vec<u64>,
    peak_queued: Vec<usize>,
    emitted: Vec<Vec<Vec<EmittedBatch>>>,
    abi_calls: Vec<Vec<Vec<AbiCalls>>>,
    /// Calls made against each seq so far, which is what stamps `AbiCall::call_index`.
    /// Empty on an unmeasured run: nothing is recorded, so nothing is counted.
    calls_made: HashMap<Seq, u64>,
    consumed: Vec<Vec<Vec<u64>>>,
}

/// Output queues live on the producing node, which is what makes lane remapping free: an
/// emitter reads its child's one lane, a forwarder reads whichever pair its map names, a
/// join reads lane p of each side, and no producer knows who consumes it. Each queue has a
/// cursor per edge into the node, the ordinal `IndexedNode::child_readers` names.
struct NodeState<B: Backend> {
    out_queues: Vec<OutQueue<B::Batch>>,
    out_done: Vec<bool>,
    lanes: Vec<LaneDriver<B>>,
    cross: Option<CrossExecutor<B>>,
    /// `sources_of` materialized once per out lane: the predicate asks for it on every
    /// step, and the trait's `Vec` would be an allocation each time.
    forward_map: Vec<Vec<(usize, usize)>>,
    cursors: Vec<usize>,
    retired: Vec<Vec<bool>>,
    lane_done_sent: Vec<bool>,
    emitter_finished: bool,
    /// A hold buffer's store, per lane: what it took while held and has not yet emitted.
    kept: Vec<VecDeque<Held<B::Batch>>>,
}

enum CrossExecutor<B: Backend> {
    Accumulator(B::PartAcc),
    Emitter(B::Emitter),
}

impl<'a, B: Backend> Driver<'a, B> {
    pub(crate) fn new(
        root: &'a dyn GpuNode,
        ctx: &'a B::Context,
        budget: Option<usize>,
    ) -> Result<Self, RunError> {
        Self::for_roots(&[root], ctx, budget)
    }

    pub(crate) fn for_roots(
        roots: &[&'a dyn GpuNode],
        ctx: &'a B::Context,
        budget: Option<usize>,
    ) -> Result<Self, RunError> {
        // The rule the planner applies, applied to a tree that did not come from it: a
        // hand-built plan meets the same refusal a planned one would, and there is one
        // statement of it rather than two.
        check_canonical_form_of_roots(roots).map_err(RunError::Backend)?;
        let index = PlanIndex::build_roots(roots).map_err(RunError::Backend)?;
        let scheduler = Scheduler::new(&index.shape);
        let acct = ResidentAccountant::new(index.slots, budget);
        let states = index
            .nodes
            .iter()
            .map(|node| NodeState {
                out_queues: (0..node.lanes)
                    .map(|_| OutQueue::new(node.readers.len()))
                    .collect(),
                out_done: vec![false; node.lanes],
                lanes: (0..node.lanes).map(|_| LaneDriver::default()).collect(),
                cross: None,
                forward_map: Vec::new(),
                cursors: vec![0; node.lanes],
                retired: Vec::new(),
                lane_done_sent: vec![false; node.input_lanes],
                emitter_finished: false,
                kept: (0..node.lanes).map(|_| VecDeque::new()).collect(),
            })
            .collect();
        let nodes = index.len();
        let emitted = index
            .nodes
            .iter()
            .map(|node| vec![Vec::new(); node.lanes])
            .collect();
        // By the lanes that drive the node, not the ones it emits into: a cross-lane
        // accumulator is called once per input lane and answers on one.
        let abi_calls = index
            .nodes
            .iter()
            .map(|node| vec![Vec::new(); node.ready_lanes])
            .collect();
        let consumed = index
            .nodes
            .iter()
            .map(|node| {
                node.children
                    .iter()
                    .map(|child| vec![0u64; index.nodes[*child].lanes])
                    .collect()
            })
            .collect();
        let mut root_of = vec![None; nodes];
        for (ordinal, root) in index.shape.roots.iter().enumerate() {
            root_of[*root] = Some(ordinal);
        }
        let mut driver = Self {
            results: index.shape.roots.iter().map(|_| Vec::new()).collect(),
            root_of,
            index,
            ctx,
            hook: None,
            scheduler,
            acct,
            states,
            trace: Vec::new(),
            steps: 0,
            rows_seen: vec![0; nodes],
            rows_emitted: vec![0; nodes],
            rows_skipped: vec![0; nodes],
            peak_queued: vec![0; nodes],
            emitted,
            abi_calls,
            calls_made: HashMap::new(),
            consumed,
        };
        driver.wire_forwarders()?;
        Ok(driver)
    }

    /// [`Self::run_fused`] over one root, whose answer becomes the report's batches.
    pub(crate) fn run(self, max_steps: usize) -> Result<RunReport, RunError> {
        let FusedRun {
            mut answers,
            mut report,
        } = self.run_fused(max_steps)?;
        report.batches = answers.swap_remove(ROOT);
        Ok(report)
    }

    pub(crate) fn run_fused(mut self, max_steps: usize) -> Result<FusedRun, RunError> {
        let outcome = self.drive(max_steps);
        if outcome.is_err() {
            // A query that ended badly still gives back what it was holding, by the same
            // path the early exit uses: no handle is touched again, but the totals still
            // reconcile, so held and released stay equal on every path out of here.
            let _ = self.release_in_flight();
        }
        match outcome {
            Ok(()) => Ok(self.fused()),
            Err(StepError::Run(error)) => Err(error),
            Err(StepError::Trip(trip)) => Err(self.budget_error(trip)),
        }
    }

    fn drive(&mut self, max_steps: usize) -> Result<(), StepError> {
        self.seed()?;
        while self.step()? {
            if self.steps > max_steps {
                return Err(
                    RunError::Protocol(format!("no termination after {max_steps} steps")).into(),
                );
            }
        }
        self.finish()
    }

    /// Readiness for every node, and the limits settled: a zero-row interval is satisfied
    /// before anything runs, and the plan still has to complete rather than stall.
    pub(crate) fn seed(&mut self) -> Result<(), StepError> {
        for node in 0..self.index.len() {
            self.refresh(node);
            self.settle_limit(node)?;
        }
        Ok(())
    }

    /// Run one node — every lane of it. `false` when nothing is runnable, which is how a
    /// run ends.
    pub(crate) fn step(&mut self) -> Result<bool, StepError> {
        let Some(node) = self.scheduler.next() else {
            return Ok(false);
        };
        self.steps += 1;
        self.run_node(node)?;
        self.settle_limit(node)?;
        self.refresh(node);
        for at in 0..self.index.nodes[node].readers.len() {
            let parent = self.index.nodes[node].readers[at].parent;
            self.refresh(parent);
        }
        Ok(true)
    }

    /// A run that ended because a limit was satisfied legitimately leaves lanes not done
    /// and queues non-empty; any other one has to have drained.
    pub(crate) fn finish(&mut self) -> Result<(), StepError> {
        if self.scheduler.any_satisfied() {
            self.release_in_flight()
        } else {
            self.assert_drained()
        }
    }

    fn run_node(&mut self, node: usize) -> Result<(), StepError> {
        match self.index.nodes[node].category {
            category if category.is_lane_scoped() => self.run_lane_scoped(node)?,
            ExecutorCategory::PartitionEmitter => self.run_emitter(node)?,
            ExecutorCategory::PartitionAccumulator => self.run_partition_accumulator(node)?,
            ExecutorCategory::BatchForwarder => self.run_forwarder(node)?,
            ExecutorCategory::HoldBuffer => self.run_hold_buffer(node)?,
            other => {
                return Err(RunError::Protocol(format!("{other:?} has no driver arm")).into());
            }
        }
        self.peak_queued[node] = self.peak_queued[node].max(self.queued(node));
        Ok(())
    }

    // -- lane-scoped -------------------------------------------------------------

    fn run_lane_scoped(&mut self, node: usize) -> Result<(), StepError> {
        let indexed = &self.index.nodes[node];
        let (category, interval, gpu_node) = (indexed.category, indexed.interval, indexed.node);
        let unloading = category == ExecutorCategory::Unload;
        for lane in 0..self.index.nodes[node].lanes {
            let avail = self.avail(node, lane);
            if !self.states[node].lanes[lane].can_step(category, &avail) {
                continue;
            }
            // A protocol violation is about one lane of one node, and the lane driver
            // knows neither: without this the message names a shape and no site.
            let name = self.index.nodes[node].node.name();
            let call = self.states[node].lanes[lane]
                .select(category, &avail)
                .map_err(|error| match error {
                    RunError::Protocol(what) => {
                        RunError::Protocol(format!("{name} lane {lane}: {what}"))
                    }
                    other => other,
                })?;
            // Every branch below is about the batch this call consumes, and the calls that
            // end a lane consume none: an interval decides nothing about them.
            let consuming = call.consumes().is_some();
            let arriving = match (interval, unloading && consuming) {
                (Some(_), true) => self.peek_rows(node, lane, call),
                _ => 0,
            };
            // Only an unload's decision is made here, its range being an argument of the
            // driver's own call. A mid-plan limit makes the same three-way choice inside its
            // executor, and the driver reads what it emitted.
            let mut rows = RowRange::WHOLE;
            if let Some(interval) = interval.filter(|_| unloading && consuming) {
                match interval.range_of(self.rows_seen[node], arriving) {
                    // Not one row is wanted, so it never crosses the boundary: the batch
                    // is released where it stands, and the saving on the skip prefix is
                    // unbounded.
                    None => {
                        let unwanted = self
                            .take_input(node, lane, call)?
                            .expect("an unload consumes its batch");
                        self.rows_seen[node] += arriving;
                        self.rows_skipped[node] += arriving;
                        self.acct.release(unwanted.charge)?;
                        self.record(node, lane, CallKind::ReleaseUnwanted, 0);
                        continue;
                    }
                    Some(range) if !range.covers(arriving) => rows = range,
                    Some(_) => {}
                }
                // The ABI clamps an offset past the end and a length past it, because a C
                // ABI has to be total — and `range_of` can produce neither, so reaching
                // that tolerance means `rows_seen` has drifted. What that looks like from
                // the outside is a LIMIT quietly returning short, so the arithmetic is
                // checked here where it is done rather than absorbed where it is used.
                if rows != RowRange::WHOLE
                    && (rows.length == 0 || rows.offset + rows.length > arriving)
                {
                    return Err(RunError::Protocol(format!(
                        "{}: the rows wanted of this batch are {}..+{} of {arriving}",
                        self.index.nodes[node].node.name(),
                        rows.offset,
                        rows.length
                    ))
                    .into());
                }
            }
            let input = match consuming {
                true => self.take_input(node, lane, call)?,
                false => None,
            };
            let site = LaneSite::<B> {
                ctx: self.ctx,
                node: gpu_node,
                category,
                post_order: self.index.nodes[node].post_order,
                lane,
                slot: self.slot(node, lane),
            };
            let outcome =
                self.states[node].lanes[lane].run(&site, call, input, rows, &mut self.acct)?;
            self.rows_seen[node] += arriving;
            let produced = match outcome.outputs {
                LaneOutputs::Device(batches) => {
                    let produced = batches.len();
                    let mut batches = batches.into_iter();
                    while let Some(batch) = batches.next() {
                        self.record_emitted(node, lane, &batch);
                        // Device outputs carrying an interval are a mid-plan limit's: the
                        // unload's are host batches.
                        if interval.is_some() {
                            self.rows_emitted[node] += batch.rows();
                        }
                        // The lane held this call's whole output at once, so a refusal
                        // gives back the batches behind the refused one as well.
                        let batch = match self.offer(node, lane, batch) {
                            Ok(batch) => batch,
                            Err(refusal) => {
                                for unqueued in batches {
                                    let _ = self.acct.release(unqueued.charge);
                                }
                                return Err(refusal);
                            }
                        };
                        self.states[node].out_queues[lane].push(batch);
                    }
                    produced
                }
                // The unload is the crossing: these bytes are on the host now, so they
                // leave the accounted set as they are handed to the caller.
                LaneOutputs::Host(batches) => {
                    let produced = batches.len();
                    let root = self.root_of[node].expect("every unload is a root");
                    for batch in batches {
                        self.record_emitted(node, lane, &batch);
                        self.acct.release(batch.charge)?;
                        self.results[root].push(batch.batch);
                    }
                    produced
                }
            };
            // The call ends a lane's build phase whether a batch arrived or not, and the
            // hold lifts when every lane has: a lane that owed nothing still has to say
            // so, or the probe region is held by a lane that will never build.
            if call == LaneCall::SetBuild && self.scheduler.lane_left_build(node) {
                self.hold_lifted(node);
            }
            if outcome.finished {
                self.states[node].out_done[lane] = true;
            }
            self.record(node, lane, outcome.call, produced);
            self.record_calls(node, lane, outcome.calls);
        }
        Ok(())
    }

    // -- cross-lane --------------------------------------------------------------

    fn run_emitter(&mut self, node: usize) -> Result<(), StepError> {
        let lanes = self.index.nodes[node].lanes;
        let slot = self.slot(node, 0);
        if !self.offered(node, 0, 0) {
            self.states[node].emitter_finished = true;
            self.states[node].out_done = vec![true; lanes];
            self.acct.forget(slot);
            self.record(node, 0, CallKind::EmitDone, 0);
            return Ok(());
        }
        let batch = self.take_from(node, 0, 0)?.expect("a batch");
        self.build_cross(node)?;
        let Some(CrossExecutor::Emitter(emitter)) = &mut self.states[node].cross else {
            return Err(wrong_cross(self.index.nodes[node].node).into());
        };
        let modelled = self
            .acct
            .begin_call(slot, emitter, batch.rows(), batch.charge.bytes)?;
        let emitted = emitter.emit(batch.batch);
        // The batch went into the call either way, so its bytes leave here whether or not
        // the call came back.
        self.acct.release(batch.charge)?;
        let (outputs, stats) = emitted.map_err(|e| self.call_failed(node, 0, e))?;
        if outputs.len() != lanes {
            return Err(RunError::Protocol(format!(
                "{}: emit returned {} lanes against the {lanes} the plan declares",
                self.index.nodes[node].node.name(),
                outputs.len()
            ))
            .into());
        }
        let mut emitted = 0;
        for (lane, out) in outputs.into_iter().enumerate() {
            // Empty scatter outputs are dropped so nothing empty traverses a chain because
            // of hash skew. A join above answers a lane its build side never reached
            // through `set_build(None)`, whatever its type owes.
            if out.num_rows() == 0 {
                continue;
            }
            let held = Held::of(out);
            self.acct.hold(&held.charge);
            self.record_emitted(node, lane, &held);
            let held = self.offer(node, lane, held)?;
            self.states[node].out_queues[lane].push(held);
            emitted += 1;
        }
        let Some(CrossExecutor::Emitter(emitter)) = &self.states[node].cross else {
            return Err(wrong_cross(self.index.nodes[node].node).into());
        };
        self.acct.end_call(slot, emitter, &stats, modelled)?;
        self.record(node, 0, CallKind::Emit, emitted);
        self.record_calls(node, 0, Some(stats.calls));
        Ok(())
    }

    fn run_partition_accumulator(&mut self, node: usize) -> Result<(), StepError> {
        let child = self.index.nodes[node].children[0];
        let slot = self.slot(node, 0);
        for lane in 0..self.index.nodes[node].input_lanes {
            let has = self.offered(node, 0, lane);
            let done = self.states[child].out_done[lane] && !self.states[node].lane_done_sent[lane];
            if !has && !done {
                continue;
            }
            let batch = match has {
                true => self.take_from(node, 0, lane)?,
                false => None,
            };
            if batch.is_none() {
                self.states[node].lane_done_sent[lane] = true;
            }
            let (rows, bytes) = batch
                .as_ref()
                .map_or((0, 0), |held| (held.rows(), held.charge.bytes));
            self.build_cross(node)?;
            let Some(CrossExecutor::Accumulator(accumulator)) = &mut self.states[node].cross else {
                return Err(wrong_cross(self.index.nodes[node].node).into());
            };
            let modelled = self.acct.begin_call(slot, accumulator, rows, bytes)?;
            let (event, kind, charge) = match batch {
                Some(held) => (
                    LaneEvent::Batch(held.batch),
                    CallKind::LaneEvent,
                    Some(held.charge),
                ),
                None => (LaneEvent::Done, CallKind::LaneDone, None),
            };
            let accumulated = accumulator.accumulate_and_fetch(lane, event);
            // A lane's end carries no batch, so there is nothing to release: counting one
            // anyway made holds and releases disagree on every plan with a cross-lane
            // accumulator, which is what the report says they never do.
            if let Some(charge) = charge {
                self.acct.release(charge)?;
            }
            let (outputs, stats) = accumulated.map_err(|e| self.call_failed(node, lane, e))?;
            let produced = outputs.len();
            for out in outputs {
                let held = Held::of(out);
                self.acct.hold(&held.charge);
                self.record_emitted(node, 0, &held);
                let held = self.offer(node, 0, held)?;
                self.states[node].out_queues[0].push(held);
            }
            let Some(CrossExecutor::Accumulator(accumulator)) = &self.states[node].cross else {
                return Err(wrong_cross(self.index.nodes[node].node).into());
            };
            self.acct.end_call(slot, accumulator, &stats, modelled)?;
            self.record(node, lane, kind, produced);
            self.record_calls(node, lane, Some(stats.calls));
        }
        if self.states[node].lane_done_sent.iter().all(|sent| *sent) {
            self.states[node].out_done[0] = true;
            self.acct.forget(slot);
        }
        Ok(())
    }

    /// One batch per visit, cycling the lane's map in order from its cursor. The merge's
    /// round-robin and the interleave's child rotation are this one rule.
    fn run_forwarder(&mut self, node: usize) -> Result<(), StepError> {
        for lane in 0..self.index.nodes[node].lanes {
            if self.states[node].out_done[lane] {
                continue;
            }
            match self.forward_one(node, lane)? {
                true => self.record(node, lane, CallKind::Forward, 1),
                false if self.states[node].retired[lane].iter().all(|gone| *gone) => {
                    self.states[node].out_done[lane] = true;
                    self.record(node, lane, CallKind::ForwardDone, 0);
                }
                false => {}
            }
        }
        Ok(())
    }

    fn forward_one(&mut self, node: usize, lane: usize) -> Result<bool, StepError> {
        let sources = self.states[node].forward_map[lane].len();
        let start = self.states[node].cursors[lane];
        for offset in 0..sources {
            let index = (start + offset) % sources;
            if self.states[node].retired[lane][index] {
                continue;
            }
            let (child_index, child_lane) = self.states[node].forward_map[lane][index];
            let child = self.index.nodes[node].children[child_index];
            // A move between queues: the batch stays in flight, so nothing is accounted.
            if let Some(batch) = self.take_from(node, child_index, child_lane)? {
                self.record_emitted(node, lane, &batch);
                self.states[node].out_queues[lane].push(batch);
                self.states[node].cursors[lane] = (index + 1) % sources;
                return Ok(true);
            }
            if self.states[child].out_done[child_lane] {
                self.states[node].retired[lane][index] = true;
            }
        }
        Ok(false)
    }

    // -- schedule ----------------------------------------------------------------

    /// Recompute this node's ready lanes. Only the node just run and the parent of each of
    /// its readers can have changed: a node's readiness is a fact about its inputs, and nothing else moved.
    fn refresh(&mut self, node: usize) {
        for lane in 0..self.index.nodes[node].ready_lanes {
            let ready = self.lane_ready(node, lane);
            self.scheduler.set_lane_ready(node, lane, ready);
        }
    }

    fn lane_ready(&self, node: usize, lane: usize) -> bool {
        let indexed = &self.index.nodes[node];
        match indexed.category {
            category if category.is_lane_scoped() => {
                let avail = self.avail(node, lane);
                self.states[node].lanes[lane].can_step(category, &avail)
            }
            ExecutorCategory::PartitionEmitter => {
                let child = indexed.children[0];
                self.offered(node, 0, 0)
                    || (self.states[child].out_done[0] && !self.states[node].emitter_finished)
            }
            ExecutorCategory::PartitionAccumulator => {
                let child = indexed.children[0];
                self.offered(node, 0, lane)
                    || (self.states[child].out_done[lane]
                        && !self.states[node].lane_done_sent[lane])
            }
            ExecutorCategory::BatchForwarder => self.forwarder_lane_ready(node, lane),
            ExecutorCategory::HoldBuffer => self.hold_buffer_lane_ready(node, lane),
            _ => false,
        }
    }

    /// A forwarder lane can step when a source has a batch, or when the last live source
    /// has finished — the visit that retires it is what marks the lane done.
    fn forwarder_lane_ready(&self, node: usize, lane: usize) -> bool {
        if self.states[node].out_done[lane] {
            return false;
        }
        let mut live = 0;
        for (index, (child_index, child_lane)) in
            self.states[node].forward_map[lane].iter().enumerate()
        {
            if self.states[node].retired[lane][index] {
                continue;
            }
            let child = self.index.nodes[node].children[*child_index];
            if self.offered(node, *child_index, *child_lane) {
                return true;
            }
            if !self.states[child].out_done[*child_lane] {
                live += 1;
            }
        }
        live == 0
    }

    // -- wiring ------------------------------------------------------------------

    /// Forwarders are wired once: `sources_of` allocates, and the readiness predicate asks
    /// for it on every step. Checking the map against the tree here is also the only place
    /// a routing that names a child or a lane the plan does not have can be caught.
    fn wire_forwarders(&mut self) -> Result<(), RunError> {
        for node in 0..self.index.len() {
            if self.index.nodes[node].category != ExecutorCategory::BatchForwarder {
                continue;
            }
            let indexed = &self.index.nodes[node];
            let executors = B::executors_for(self.ctx, indexed.node, indexed.post_order, 0)
                .map_err(RunError::Backend)?;
            let NodeExecutors::BatchForwarder(forwarder) = executors else {
                return Err(RunError::Backend(PlanError::Invalid(format!(
                    "{}: the backend built a {:?} executor for a routing node",
                    indexed.node.name(),
                    executors.category()
                ))));
            };
            let map = self.forward_map_of(node, &forwarder)?;
            self.states[node].retired = map.iter().map(|lane| vec![false; lane.len()]).collect();
            self.states[node].forward_map = map;
        }
        Ok(())
    }

    fn forward_map_of(
        &self,
        node: usize,
        forwarder: &Forwarder,
    ) -> Result<Vec<Vec<(usize, usize)>>, RunError> {
        let indexed = &self.index.nodes[node];
        let invalid = |what: String| RunError::Backend(PlanError::Invalid(what));
        if let Forwarder::Union { lanes } = forwarder
            && lanes.len() != indexed.lanes
        {
            return Err(invalid(format!(
                "{}: the routing serves {} lanes and the layout declares {}",
                indexed.node.name(),
                lanes.len(),
                indexed.lanes
            )));
        }
        let mut map = Vec::with_capacity(indexed.lanes);
        for lane in 0..indexed.lanes {
            let sources = forwarder.sources_of(lane);
            if sources.is_empty() {
                return Err(invalid(format!(
                    "{}: lane {lane} is served by nothing, so it can never finish",
                    indexed.node.name()
                )));
            }
            for (child_index, child_lane) in &sources {
                let child = indexed.children.get(*child_index).ok_or_else(|| {
                    invalid(format!(
                        "{}: lane {lane} reads child {child_index} of {}",
                        indexed.node.name(),
                        indexed.children.len()
                    ))
                })?;
                if *child_lane >= self.index.nodes[*child].lanes {
                    return Err(invalid(format!(
                        "{}: lane {lane} reads lane {child_lane} of a {}-lane child",
                        indexed.node.name(),
                        self.index.nodes[*child].lanes
                    )));
                }
            }
            map.push(sources);
        }
        Ok(map)
    }

    fn build_cross(&mut self, node: usize) -> Result<(), RunError> {
        if self.states[node].cross.is_some() {
            return Ok(());
        }
        let indexed = &self.index.nodes[node];
        let executors = B::executors_for(self.ctx, indexed.node, indexed.post_order, 0)
            .map_err(RunError::Backend)?;
        if executors.category() != indexed.category {
            return Err(RunError::Backend(PlanError::Invalid(format!(
                "{}: the backend built a {:?} executor for a {:?} node",
                indexed.node.name(),
                executors.category(),
                indexed.category
            ))));
        }
        self.states[node].cross = Some(match executors {
            NodeExecutors::PartitionAccumulator(accumulator) => {
                CrossExecutor::Accumulator(accumulator)
            }
            NodeExecutors::PartitionEmitter(emitter) => CrossExecutor::Emitter(emitter),
            _ => return Err(wrong_cross(indexed.node)),
        });
        Ok(())
    }

    // -- reporting ---------------------------------------------------------------

    fn slot(&self, node: usize, lane: usize) -> Slot {
        Slot {
            index: self.index.slot(node, lane),
            node: node as u32,
            lane: lane as u32,
        }
    }

    fn record_emitted<T: crate::executor::Batch>(
        &mut self,
        node: usize,
        lane: usize,
        batch: &Held<T>,
    ) {
        self.emitted[node][lane].push(EmittedBatch {
            rows: batch.rows(),
            bytes: batch.charge.bytes,
        });
    }

    fn record_consumed(&mut self, node: usize, slot: usize, child_lane: usize, rows: u64) {
        self.consumed[node][slot][child_lane] += rows;
    }

    /// What one lane call made, filed under the lane that drove it. Separate from
    /// [`record`](Self::record) because only three of the call sites have any: the rest
    /// are the driver's own bookkeeping, which addresses no seq.
    fn record_calls(&mut self, node: usize, lane: usize, calls: Option<AbiCalls>) {
        let Some(mut calls) = calls else { return };
        // The number C++ counts to, counted here for the same seqs in the same order: the
        // driver is the one place that sees every call, and an executor sees only its own.
        // Nothing is touched on an unmeasured run — `recorded_mut` is `None` there.
        if let Some(made) = calls.recorded_mut() {
            for call in made {
                let next = self.calls_made.entry(call.seq).or_insert(0);
                call.call_index = *next;
                *next += 1;
            }
        }
        self.abi_calls[node][lane].push(calls);
    }

    fn record(&mut self, node: usize, lane: usize, call: CallKind, outputs: usize) {
        self.trace.push(TraceEvent {
            step: self.steps as u32,
            node: node as u32,
            lane: lane as u32,
            call,
            outputs: outputs as u32,
        });
    }

    /// A failed call ends the query: nothing further is scheduled and no handle is touched
    /// again. The node and the lane are what a message can name that a backend cannot.
    fn call_failed(&self, node: usize, lane: usize, error: BackendError) -> StepError {
        StepError::Run(RunError::CallFailed(format!(
            "{} lane {lane}: {error}",
            self.index.nodes[node].node.name()
        )))
    }

    /// The hook's look at a batch `node` is about to queue on `lane`, handed back to be
    /// queued; its refusal ends the query where a failed call would. The batch is held and
    /// not yet queued, so no queue will give it back: the refusal releases it here, as a
    /// failed lane call releases its input.
    fn offer(
        &mut self,
        node: usize,
        lane: usize,
        held: Held<B::Batch>,
    ) -> Result<Held<B::Batch>, StepError> {
        let Some(hook) = self.hook.as_mut() else {
            return Ok(held);
        };
        match hook(node, lane, &held.batch) {
            Ok(()) => Ok(held),
            Err(why) => {
                let _ = self.acct.release(held.charge);
                Err(StepError::Run(RunError::CallFailed(format!(
                    "{} lane {lane}: the output hook refused a batch: {why}",
                    self.index.nodes[node].node.name()
                ))))
            }
        }
    }

    fn budget_error(&self, trip: Trip) -> RunError {
        RunError::BudgetExceeded {
            when: trip.when,
            message: format!(
                "resident GPU memory budget exceeded at {} lane {}, {}: {} bytes > budget {} \
                 bytes",
                self.index.nodes[trip.slot.node as usize].node.name(),
                trip.slot.lane,
                trip.when.describe(),
                trip.bytes,
                trip.budget
            ),
        }
    }
}

fn wrong_cross(node: &dyn GpuNode) -> RunError {
    RunError::Backend(PlanError::Invalid(format!(
        "{}: its executor is not the cross-lane one its category names",
        node.name()
    )))
}

/// The probe side is always the right child, which is the orientation the schedule's
/// leftmost tie-break turns into "the build subtree drains first".
const _: () = assert!(PROBE_CHILD == 1 && ROOT == 0);
