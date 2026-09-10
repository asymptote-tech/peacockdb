use super::*;
use crate::batch_partitioned::error::PlanError;
use crate::batch_partitioned::layout::NodeKind;
use datafusion::arrow::array::RecordBatch;
use datafusion::arrow::datatypes::Schema as ArrowSchema;
use std::sync::Arc;

/// A batch that is a handle and a row count, as `GpuBatch` is — the second backend
/// exists to differ from the first here, which is what the generic code must absorb.
struct HandleBatch {
    rows: usize,
}
impl Batch for HandleBatch {
    fn num_rows(&self) -> usize {
        self.rows
    }
    fn byte_size(&self) -> usize {
        self.rows * 8
    }
}

fn empty_cpu_batch() -> CpuBatch {
    CpuBatch::new(RecordBatch::new_empty(Arc::new(ArrowSchema::empty())))
}

// One backend per invocation: every category is the same stub type, since what is
// under test is that the seven associated types resolve and the generic driver
// monomorphizes — not what any operator computes.
macro_rules! stub_backend {
    ($backend:ident, $ops:ident, $probing:ident, $batch:ty, $make:expr) => {
        struct $backend;
        struct $ops;
        struct $probing;

        impl Executor for $ops {
            fn resident_bytes(&self) -> usize {
                0
            }
            fn scratch_bytes(&self, _n_rows: u64, n_bytes: usize) -> usize {
                n_bytes
            }
        }
        impl Executor for $probing {
            fn resident_bytes(&self) -> usize {
                1
            }
            fn scratch_bytes(&self, _n_rows: u64, _n_bytes: usize) -> usize {
                0
            }
        }

        impl SourceExecutor<$backend> for $ops {
            fn next_batch(self) -> Result<SourceStep<$backend>, BackendError> {
                Ok(SourceStep::Batch {
                    batch: $make,
                    stats: CallStats::default(),
                    source: self,
                })
            }
        }
        impl ExecExecutor<$backend> for $ops {
            fn exec(&mut self, batch: $batch) -> CallResult<$batch> {
                Ok((batch, CallStats::default()))
            }
        }
        impl BatchAccumulatorExecutor<$backend> for $ops {
            fn accumulate_and_fetch(&mut self, batch: $batch) -> CallResult<Vec<$batch>> {
                Ok((vec![batch], CallStats::default()))
            }
            fn mark_done_and_fetch(self) -> CallResult<Vec<$batch>> {
                Ok((Vec::new(), CallStats::default()))
            }
        }
        impl PartitionAccumulatorExecutor<$backend> for $ops {
            fn accumulate_and_fetch(
                &mut self,
                _partition: usize,
                event: LaneEvent<$batch>,
            ) -> CallResult<Vec<$batch>> {
                let out = match event {
                    LaneEvent::Batch(batch) => vec![batch],
                    LaneEvent::Done => Vec::new(),
                };
                Ok((out, CallStats::default()))
            }
        }
        impl PartitionEmitterExecutor<$backend> for $ops {
            fn emit(&mut self, batch: $batch) -> CallResult<Vec<$batch>> {
                Ok((vec![batch], CallStats::default()))
            }
        }
        impl JoinExecutor<$backend> for $ops {
            type Probing = $probing;
            fn set_build(self, _batch: $batch) -> CallResult<$probing> {
                Ok(($probing, CallStats::default()))
            }
            fn without_build(self) -> Result<(), BackendError> {
                Ok(())
            }
        }
        impl ProbingJoin<$backend> for $probing {
            fn probe_and_fetch(&mut self, batch: $batch) -> CallResult<Vec<$batch>> {
                Ok((vec![batch], CallStats::default()))
            }
            fn finish_and_fetch(self) -> CallResult<Vec<$batch>> {
                Ok((Vec::new(), CallStats::default()))
            }
        }
        impl UnloadExecutor<$backend> for $ops {
            fn unload(&mut self, _batch: $batch, _rows: RowRange) -> CallResult<CpuBatch> {
                Ok((empty_cpu_batch(), CallStats::default()))
            }
        }

        impl Backend for $backend {
            type Context = ();
            type Batch = $batch;
            type Source = $ops;
            type Exec = $ops;
            type BatchAcc = $ops;
            type PartAcc = $ops;
            type Emitter = $ops;
            type Join = $ops;
            type Unload = $ops;

            fn executors_for(
                _ctx: &(),
                node: &dyn GpuNode,
                _post_order: usize,
                _lane: usize,
            ) -> Result<NodeExecutors<Self>, PlanError> {
                match node.kind() {
                    NodeKind::Source { .. } => Ok(NodeExecutors::Source($ops)),
                    NodeKind::Intermediate { .. } => Ok(NodeExecutors::Exec($ops)),
                    NodeKind::Sink => Ok(NodeExecutors::Unload($ops)),
                }
            }
        }
    };
}

stub_backend!(
    FirstBackend,
    FirstOps,
    FirstProbing,
    CpuBatch,
    empty_cpu_batch()
);
stub_backend!(
    SecondBackend,
    SecondOps,
    SecondProbing,
    HandleBatch,
    HandleBatch { rows: 7 }
);

/// One build -> probe -> finish transition, written once for every backend.
fn drive_join<B: Backend>(join: B::Join, build: B::Batch, probe: B::Batch) -> usize {
    let (mut probing, _) = join.set_build(build).expect("the build side is set");
    let (probed, _) = probing.probe_and_fetch(probe).expect("probed");
    let held = probing.resident_bytes();
    let (finished, _) = probing.finish_and_fetch().expect("finished");
    held + probed
        .iter()
        .chain(finished.iter())
        .map(Batch::num_rows)
        .sum::<usize>()
}

/// One source step, written once for every backend.
fn drive_source<B: Backend>(source: B::Source) -> usize {
    match source.next_batch().expect("a step") {
        SourceStep::Batch { batch, .. } => batch.num_rows(),
        SourceStep::Exhausted => 0,
    }
}

#[test]
fn one_generic_driver_serves_two_backends_with_different_batch_types() {
    assert_eq!(
        drive_join::<FirstBackend>(FirstOps, empty_cpu_batch(), empty_cpu_batch()),
        1
    );
    assert_eq!(
        drive_join::<SecondBackend>(SecondOps, HandleBatch { rows: 3 }, HandleBatch { rows: 4 }),
        5
    );
    assert_eq!(drive_source::<FirstBackend>(FirstOps), 0);
    assert_eq!(drive_source::<SecondBackend>(SecondOps), 7);
}
