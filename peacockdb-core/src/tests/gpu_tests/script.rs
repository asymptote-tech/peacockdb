//! One call sequence, two backends. `Script` says which calls in `RecordBatch` terms;
//! `drive` makes them on any `Backend` through `executors_for`, so what runs is the trait
//! and not a constructor; `run_both` does it on each and hands back both results.

use datafusion::arrow::record_batch::RecordBatch;
use datafusion::execution::context::SessionContext;

use super::device::Device;
use crate::executor::{
    Backend, BackendError, BatchAccumulatorExecutor, CpuBackend, CpuBatch, ExecExecutor,
    GpuBackend, JoinExecutor, LaneEvent, NodeExecutors, PartitionAccumulatorExecutor,
    PartitionEmitterExecutor, ProbingJoin, RowRange, SourceExecutor, SourceStep, UnloadExecutor,
};
use crate::plan::{ExecutorCategory, GpuNode, category_of};
use crate::tests::compare::{Order, Slot, same};

/// One variant per executor category, so `drive` is the whole call protocol in one match.
pub(crate) enum Script {
    /// One `exec` per batch; one slot each.
    Exec(Vec<RecordBatch>),
    /// `accumulate_and_fetch` per batch, then `mark_done_and_fetch`; one slot per call.
    Accumulate(Vec<RecordBatch>),
    /// Per lane, in lane order: `Batch` per batch then `Done`; one slot per call.
    Lanes(Vec<Vec<RecordBatch>>),
    /// One `emit` per batch; one slot per output lane per call.
    Emit(Vec<RecordBatch>),
    /// `set_build` then `probe_and_fetch` per probe batch then `finish_and_fetch`, one slot
    /// per call; with no build, `without_build` and no slots at all.
    Join {
        build: Option<RecordBatch>,
        probe: Vec<RecordBatch>,
    },
    /// One `unload`; one slot.
    Unload { batch: RecordBatch, rows: RowRange },
    /// `next_batch` until exhausted, on `lane`; one slot per batch.
    Source { lane: usize },
}

impl Script {
    fn category(&self) -> ExecutorCategory {
        match self {
            Script::Exec(_) => ExecutorCategory::Exec,
            Script::Accumulate(_) => ExecutorCategory::BatchAccumulator,
            Script::Lanes(_) => ExecutorCategory::PartitionAccumulator,
            Script::Emit(_) => ExecutorCategory::PartitionEmitter,
            Script::Join { .. } => ExecutorCategory::Join,
            Script::Unload { .. } => ExecutorCategory::Unload,
            Script::Source { .. } => ExecutorCategory::Source,
        }
    }

    fn lane(&self) -> usize {
        match self {
            Script::Source { lane } => *lane,
            _ => 0,
        }
    }
}

pub(crate) struct Outcome {
    pub(crate) cpu: Result<Vec<Slot>, BackendError>,
    pub(crate) gpu: Result<Vec<Slot>, BackendError>,
}

impl Outcome {
    /// Both answered, and with the same thing.
    pub(crate) fn same(&self, order: Order) {
        let cpu = self
            .cpu
            .as_ref()
            .unwrap_or_else(|why| panic!("cpu refused: {}", why.message));
        let gpu = self
            .gpu
            .as_ref()
            .unwrap_or_else(|why| panic!("gpu refused: {}", why.message));
        if let Err(why) = same(cpu, gpu, order) {
            panic!("cpu and gpu differ: {why}");
        }
    }

    /// The device's refusal, for a `bug_` test to pin by message; a device that answered
    /// is the failure that says the ticket closed.
    pub(crate) fn gpu_refuses(&self) -> &str {
        assert!(self.cpu.is_ok(), "the cpu refused too: {:?}", self.cpu);
        &self
            .gpu
            .as_ref()
            .expect_err("the device was expected to refuse")
            .message
    }

    pub(crate) fn cpu_refuses(&self) -> &str {
        assert!(self.gpu.is_ok(), "the device refused too: {:?}", self.gpu);
        &self
            .cpu
            .as_ref()
            .expect_err("the cpu was expected to refuse")
            .message
    }
}

pub(crate) fn run_both(node: &dyn GpuNode, script: Script) -> Outcome {
    assert_eq!(
        category_of(node),
        script.category(),
        "the script's shape is not the node's category"
    );
    let cpu_ctx = SessionContext::new().task_ctx();
    let cpu = drive::<CpuBackend>(
        &cpu_ctx,
        node,
        &script,
        |batch| CpuBatch::new(batch.clone()),
        |batch| batch.into_record_batch(),
    );
    let device = Device::open(node);
    let gpu = drive::<GpuBackend>(
        device.ctx(),
        node,
        &script,
        |batch| device.upload(batch),
        |batch| {
            device
                .fetch(batch, RowRange::WHOLE)
                .expect("a whole export ships its schema")
        },
    );
    Outcome { cpu, gpu }
}

/// The script on one backend. `up` and `down` are that backend's two conversions, and the
/// only thing that differs between the two runs.
fn drive<B: Backend>(
    ctx: &B::Context,
    node: &dyn GpuNode,
    script: &Script,
    up: impl Fn(&RecordBatch) -> B::Batch,
    down: impl Fn(B::Batch) -> RecordBatch,
) -> Result<Vec<Slot>, BackendError> {
    // The root's post-order is the tree's size less one. Counted here rather than read off
    // `PlanIndex::build`, which asks every node's category and so refuses a `Given` leaf.
    fn size(node: &dyn GpuNode) -> usize {
        1 + node.children().into_iter().map(size).sum::<usize>()
    }
    let post_order = size(node) - 1;
    let executors = B::executors_for(ctx, node, post_order, script.lane())
        .map_err(|why| BackendError::new(format!("executors_for: {why}")))?;
    let lower = |batches: Vec<B::Batch>| batches.into_iter().map(&down).collect::<Slot>();
    let mut slots = Vec::new();
    match (executors, script) {
        (NodeExecutors::Exec(mut exec), Script::Exec(batches)) => {
            for batch in batches {
                let (out, _) = exec.exec(up(batch))?;
                slots.push(vec![down(out)]);
            }
        }
        (NodeExecutors::BatchAccumulator(mut acc), Script::Accumulate(batches)) => {
            for batch in batches {
                let (out, _) = acc.accumulate_and_fetch(up(batch))?;
                slots.push(lower(out));
            }
            let (out, _) = acc.mark_done_and_fetch()?;
            slots.push(lower(out));
        }
        (NodeExecutors::PartitionAccumulator(mut acc), Script::Lanes(lanes)) => {
            for (lane, batches) in lanes.iter().enumerate() {
                for batch in batches {
                    let (out, _) = acc.accumulate_and_fetch(lane, LaneEvent::Batch(up(batch)))?;
                    slots.push(lower(out));
                }
                let (out, _) = acc.accumulate_and_fetch(lane, LaneEvent::Done)?;
                slots.push(lower(out));
            }
        }
        (NodeExecutors::PartitionEmitter(mut emitter), Script::Emit(batches)) => {
            for batch in batches {
                let (lanes, _) = emitter.emit(up(batch))?;
                for out in lanes {
                    slots.push(vec![down(out)]);
                }
            }
        }
        (NodeExecutors::Join(join), Script::Join { build, probe }) => match build {
            Some(build) => {
                let (mut probing, _) = join.set_build(up(build))?;
                for batch in probe {
                    let (out, _) = probing.probe_and_fetch(up(batch))?;
                    slots.push(lower(out));
                }
                let (out, _) = probing.finish_and_fetch()?;
                slots.push(lower(out));
            }
            None => {
                assert!(
                    probe.is_empty(),
                    "a join with no build side is never probed"
                );
                join.without_build()?;
            }
        },
        (NodeExecutors::Unload(mut unload), Script::Unload { batch, rows }) => {
            let (out, _) = unload.unload(up(batch), *rows)?;
            slots.push(vec![out.into_record_batch()]);
        }
        (NodeExecutors::Source(mut source), Script::Source { .. }) => loop {
            match source.next_batch()? {
                SourceStep::Batch {
                    batch,
                    source: next,
                    ..
                } => {
                    slots.push(vec![down(batch)]);
                    source = next;
                }
                SourceStep::Exhausted => break,
            }
        },
        (executors, _) => panic!(
            "executors_for answered {:?} for a script of another shape",
            executors.category()
        ),
    }
    Ok(slots)
}

#[test]
fn a_one_sided_refusal_is_read_by_its_message() {
    let outcome = Outcome {
        cpu: Ok(Vec::new()),
        gpu: Err(BackendError::new("the device said no")),
    };
    assert_eq!(outcome.gpu_refuses(), "the device said no");
    let outcome = Outcome {
        cpu: Err(BackendError::new("the cpu said no")),
        gpu: Ok(Vec::new()),
    };
    assert_eq!(outcome.cpu_refuses(), "the cpu said no");
}

#[test]
#[should_panic(expected = "gpu refused: the device said no")]
fn same_names_the_side_that_refused() {
    Outcome {
        cpu: Ok(Vec::new()),
        gpu: Err(BackendError::new("the device said no")),
    }
    .same(Order::Any);
}
