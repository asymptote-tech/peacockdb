//! What a run hands back: the sink's batches, and the report over every node.
//!
//! A child of `partitioned` so it reads the driver's own fields; apart from it only to keep
//! that file under its length limit.

use datafusion::arrow::array::RecordBatch;

use super::Driver;
use crate::executor::{Backend, CpuBatch, ROOT, RunReport};

impl<B: Backend> Driver<'_, B> {
    /// The sink's batches, or one of zero rows under its input's columns where none reached it.
    pub(crate) fn answer(&mut self) -> Vec<CpuBatch> {
        if !self.results.is_empty() {
            return std::mem::take(&mut self.results);
        }
        let declared = self.index.nodes[ROOT].node.children()[0]
            .kind()
            .schema()
            .expect("a sink's input declares its columns")
            .fields
            .clone();
        vec![CpuBatch::new(RecordBatch::new_empty(declared))]
    }

    pub(crate) fn report(mut self) -> RunReport {
        let batches = self.answer();
        RunReport {
            batches,
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
            abandoned: self.abandoned,
            consumed: self.consumed,
            satisfied: (0..self.index.len())
                .filter(|node| self.scheduler.is_satisfied(*node))
                .collect(),
        }
    }
}
