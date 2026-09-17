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
use crate::test_support::device_divergence;
use crate::tests::compare::{Order, Slot, assert_same, same};

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

    /// The device's refusal, read by its message — a `bug_` pin, where a device that answered
    /// is the failure that says the ticket closed, or a refusal that is the right answer.
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

    /// Both refusals, cpu first — a `bug_` pin where each side is wrong in its own way.
    pub(crate) fn both_refuse(&self) -> (&str, &str) {
        let cpu = self
            .cpu
            .as_ref()
            .expect_err("the cpu was expected to refuse");
        let gpu = self
            .gpu
            .as_ref()
            .expect_err("the device was expected to refuse");
        (&cpu.message, &gpu.message)
    }
}

/// A `bug_` test's assertion: both answered, each with exactly these slots.
pub(crate) fn each_answers(outcome: &Outcome, cpu: &[Vec<RecordBatch>], gpu: &[Vec<RecordBatch>]) {
    assert_same(
        cpu,
        outcome.cpu.as_ref().expect("the cpu answers"),
        Order::AsEmitted,
    );
    assert_same(
        gpu,
        outcome.gpu.as_ref().expect("the device answers"),
        Order::AsEmitted,
    );
}

fn shaped_for(node: &dyn GpuNode, script: &Script) {
    assert_eq!(
        category_of(node),
        script.category(),
        "the script's shape is not the node's category"
    );
}

pub(crate) fn run_both(node: &dyn GpuNode, script: Script) -> Outcome {
    shaped_for(node, &script);
    let cpu_ctx = SessionContext::new().task_ctx();
    let cpu = drive::<CpuBackend, _>(
        &cpu_ctx,
        node,
        &script,
        |batch| CpuBatch::new(batch.clone()),
        |batch| Ok(batch.into_record_batch()),
        CpuBatch::into_record_batch,
    );
    let device = Device::open(node);
    // What the node declares its output to be is what the sink would tell the export.
    let declared = node.kind().schema().map(|schema| schema.fields.as_ref());
    let gpu = drive::<GpuBackend, _>(
        device.ctx(),
        node,
        &script,
        |batch| device.upload(batch),
        |batch| {
            device
                .fetch(batch, RowRange::WHOLE, declared)
                .map(|back| back.expect("a whole export ships its schema"))
        },
        CpuBatch::into_record_batch,
    );
    Outcome { cpu, gpu }
}

/// The script on the device alone, every output handle read where it sits rather than
/// exported: what each diverges from the node's declaration by, in slot order, `None` where
/// it holds it. A device that refuses ends the case naming why, as `run_both` would.
pub(crate) fn divergences_on_device(node: &dyn GpuNode, script: Script) -> Vec<Option<String>> {
    shaped_for(node, &script);
    let device = Device::open(node);
    let declared = &node
        .kind()
        .schema()
        .expect("a sink declares no schema and holds no handle")
        .fields;
    let slots = drive::<GpuBackend, _>(
        device.ctx(),
        node,
        &script,
        |batch| device.upload(batch),
        |batch| Ok(device_divergence(declared, &device.schema_of(&batch))),
        |_| unreachable!("an unload answers host rows, not a handle"),
    )
    .unwrap_or_else(|why| panic!("the device refused: {}", why.message));
    slots.into_iter().flatten().collect()
}

/// A schema case's assertion: at least one handle was read, and none diverged.
pub(crate) fn assert_holds_as_declared(node: &dyn GpuNode, script: Script) {
    assert_none_diverge(divergences_on_device(node, script));
}

/// The same over divergences already read, for a case with a file to remove first.
pub(crate) fn assert_none_diverge(found: Vec<Option<String>>) {
    assert!(!found.is_empty(), "the script produced no handle to read");
    let diverging: Vec<&String> = found.iter().flatten().collect();
    assert!(
        diverging.is_empty(),
        "the device holds something other than the declaration: {diverging:?}"
    );
}

/// The script on one backend. `up` and `down` are that backend's two conversions, and the
/// only thing that differs between the two runs; a `down` that fails is the device's export
/// refusing, which ends the run as it would end a query. `unloaded` lowers the sink's host
/// rows, the one output that is never a batch of the backend's.
fn drive<B: Backend, T>(
    ctx: &B::Context,
    node: &dyn GpuNode,
    script: &Script,
    up: impl Fn(&RecordBatch) -> B::Batch,
    down: impl Fn(B::Batch) -> Result<T, BackendError>,
    unloaded: impl Fn(CpuBatch) -> T,
) -> Result<Vec<Vec<T>>, BackendError> {
    // The root's post-order is the tree's size less one. Counted here rather than read off
    // `PlanIndex::build`, which asks every node's category and so refuses a `Given` leaf.
    fn size(node: &dyn GpuNode) -> usize {
        1 + node.children().into_iter().map(size).sum::<usize>()
    }
    let post_order = size(node) - 1;
    let executors = B::executors_for(ctx, node, post_order, script.lane())
        .map_err(|why| BackendError::new(format!("executors_for: {why}")))?;
    let lower = |batches: Vec<B::Batch>| {
        batches
            .into_iter()
            .map(&down)
            .collect::<Result<Vec<T>, BackendError>>()
    };
    let mut slots = Vec::new();
    match (executors, script) {
        (NodeExecutors::Exec(mut exec), Script::Exec(batches)) => {
            for batch in batches {
                let (out, _) = exec.exec(up(batch))?;
                slots.push(vec![down(out)?]);
            }
        }
        (NodeExecutors::BatchAccumulator(mut acc), Script::Accumulate(batches)) => {
            for batch in batches {
                let (out, _) = acc.accumulate_and_fetch(up(batch))?;
                slots.push(lower(out)?);
            }
            let (out, _) = acc.mark_done_and_fetch()?;
            slots.push(lower(out)?);
        }
        (NodeExecutors::PartitionAccumulator(mut acc), Script::Lanes(lanes)) => {
            for (lane, batches) in lanes.iter().enumerate() {
                for batch in batches {
                    let (out, _) = acc.accumulate_and_fetch(lane, LaneEvent::Batch(up(batch)))?;
                    slots.push(lower(out)?);
                }
                let (out, _) = acc.accumulate_and_fetch(lane, LaneEvent::Done)?;
                slots.push(lower(out)?);
            }
        }
        (NodeExecutors::PartitionEmitter(mut emitter), Script::Emit(batches)) => {
            for batch in batches {
                let (lanes, _) = emitter.emit(up(batch))?;
                for out in lanes {
                    slots.push(vec![down(out)?]);
                }
            }
        }
        (NodeExecutors::Join(join), Script::Join { build, probe }) => match build {
            Some(build) => {
                let (mut probing, _) = join.set_build(up(build))?;
                for batch in probe {
                    let (out, _) = probing.probe_and_fetch(up(batch))?;
                    slots.push(lower(out)?);
                }
                let (out, _) = probing.finish_and_fetch()?;
                slots.push(lower(out)?);
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
            slots.push(vec![unloaded(out)]);
        }
        (NodeExecutors::Source(mut source), Script::Source { .. }) => loop {
            match source.next_batch()? {
                SourceStep::Batch {
                    batch,
                    source: next,
                    ..
                } => {
                    slots.push(vec![down(batch)?]);
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
fn a_two_sided_refusal_is_read_by_both_messages() {
    let outcome = Outcome {
        cpu: Err(BackendError::new("the cpu said no")),
        gpu: Err(BackendError::new("the device said no")),
    };
    assert_eq!(
        outcome.both_refuse(),
        ("the cpu said no", "the device said no")
    );
}

#[test]
#[should_panic(expected = "the device was expected to refuse")]
fn both_refuse_names_the_side_that_answered() {
    Outcome {
        cpu: Err(BackendError::new("the cpu said no")),
        gpu: Ok(Vec::new()),
    }
    .both_refuse();
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
