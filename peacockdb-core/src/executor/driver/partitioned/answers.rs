//! What a run hands back: each root's batches, and the report over every node.
//!
//! A child of `partitioned` so it reads the driver's own fields; apart from it only to keep
//! that file under its length limit.

use datafusion::arrow::array::RecordBatch;

use super::Driver;
use crate::executor::driver::queue::OutQueue;
use crate::executor::{Backend, CpuBatch, FusedRun, RunReport};

impl<B: Backend> Driver<'_, B> {
    /// Each root's unload batches, or one of zero rows under that sink's input columns where
    /// none reached it.
    fn answers(&mut self) -> Vec<Vec<CpuBatch>> {
        let roots = self.index.shape.roots.clone();
        roots
            .iter()
            .enumerate()
            .map(|(ordinal, root)| {
                let answered = std::mem::take(&mut self.results[ordinal]);
                if !answered.is_empty() {
                    return answered;
                }
                let declared = self.index.nodes[*root].node.children()[0]
                    .kind()
                    .schema()
                    .expect("a sink's input declares its columns")
                    .fields
                    .clone();
                vec![CpuBatch::new(RecordBatch::new_empty(declared))]
            })
            .collect()
    }

    pub(crate) fn fused(mut self) -> FusedRun {
        let answers = self.answers();
        let report = RunReport {
            batches: Vec::new(),
            peak_bytes: self.acct.peak(),
            in_flight_bytes: self.acct.in_flight(),
            steps: self.steps,
            holds: self.acct.hops().0,
            releases: self.acct.hops().1,
            underestimates: self.acct.underestimates().to_vec(),
            measured_calls: self.acct.measured_calls(),
            trace: self.trace,
            rows_skipped: self.rows_skipped,
            lanes_of: self.index.nodes.iter().map(|node| node.lanes).collect(),
            peak_queued: self.peak_queued,
            emitted: self.emitted,
            abi_calls: self.abi_calls,
            abandoned: self
                .states
                .iter()
                .map(|state| state.out_queues.iter().map(OutQueue::abandoned).collect())
                .collect(),
            consumed: self.consumed,
            satisfied: (0..self.index.len())
                .filter(|node| self.scheduler.is_satisfied(*node))
                .collect(),
        };
        FusedRun { answers, report }
    }
}
