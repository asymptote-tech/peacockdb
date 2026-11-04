//! The queues as each edge reads them: a cursor per reader into its child's queue, the batch
//! a reader takes, and what the run's end does with what is left.
//!
//! A child of `partitioned` so it reads the driver's own fields; apart from it only to keep
//! that file under its length limit.

use super::Driver;
use crate::executor::driver::StepError;
use crate::executor::driver::accounting::Held;
use crate::executor::driver::queue::OutQueue;
use crate::executor::driver::single_partition::{Avail, LaneCall};
use crate::executor::{Backend, RunError};

impl<B: Backend> Driver<'_, B> {
    pub(crate) fn avail(&self, node: usize, lane: usize) -> Avail {
        let mut avail = Avail::default();
        for (slot, child) in self.index.nodes[node].children.iter().enumerate().take(2) {
            avail.has[slot] = self.offered(node, slot, lane);
            avail.done[slot] = self.states[*child].out_done[lane] && !avail.has[slot];
        }
        avail
    }

    /// The child the edge `(node, slot)` reads, and its cursor in that child's queues.
    pub(crate) fn edge(&self, node: usize, slot: usize) -> (usize, usize) {
        let indexed = &self.index.nodes[node];
        (indexed.children[slot], indexed.child_readers[slot])
    }

    /// Whether lane `lane` of the child at `slot` holds a batch this edge has yet to take.
    pub(crate) fn offered(&self, node: usize, slot: usize, lane: usize) -> bool {
        let (child, reader) = self.edge(node, slot);
        self.states[child].out_queues[lane].has(reader)
    }

    /// This edge's next batch from lane `lane` of the child at `slot`: a share where another
    /// reader has yet to take it, which is a backend call that can fail.
    pub(crate) fn take_from(
        &mut self,
        node: usize,
        slot: usize,
        lane: usize,
    ) -> Result<Option<Held<B::Batch>>, StepError> {
        let (child, reader) = self.edge(node, slot);
        let taken = self.states[child].out_queues[lane].take(reader);
        let taken = taken.map_err(|error| self.call_failed(node, lane, error))?;
        if let Some(batch) = &taken {
            self.record_consumed(node, slot, lane, batch.rows());
        }
        Ok(taken)
    }

    pub(crate) fn peek_rows(&self, node: usize, lane: usize, call: LaneCall) -> u64 {
        let slot = call.consumes().expect("a consuming call");
        let (child, reader) = self.edge(node, slot);
        self.states[child].out_queues[lane]
            .front_rows(reader)
            .unwrap_or(0)
    }

    /// `None` only for a `SetBuild` whose build side finished without a batch — the one
    /// call that reads its slot if there is something in it and is made either way.
    pub(crate) fn take_input(
        &mut self,
        node: usize,
        lane: usize,
        call: LaneCall,
    ) -> Result<Option<Held<B::Batch>>, StepError> {
        let slot = call.consumes().expect("a consuming call");
        match self.take_from(node, slot, lane)? {
            Some(batch) => Ok(Some(batch)),
            None if !call.needs_its_batch() => Ok(None),
            None => Err(StepError::Run(RunError::Protocol(format!(
                "{}: the schedule offered a batch that is not there",
                self.index.nodes[node].node.name()
            )))),
        }
    }

    pub(crate) fn queued(&self, node: usize) -> usize {
        self.states[node].out_queues.iter().map(OutQueue::len).sum()
    }

    pub(crate) fn release_in_flight(&mut self) -> Result<(), StepError> {
        for node in 0..self.index.len() {
            for lane in 0..self.states[node].out_queues.len() {
                // Each entry once: its readers that never took it count its rows as
                // abandoned, and a share a reader took is its own holder's to release.
                for (batch, untaken) in self.states[node].out_queues[lane].drain() {
                    self.abandoned[node][lane] += batch.rows() * untaken as u64;
                    self.acct.release(batch.charge)?;
                }
            }
        }
        self.release_kept()
    }

    pub(crate) fn assert_drained(&self) -> Result<(), StepError> {
        // A root not being done is the same failure seen from the other end: a node that
        // never learned its input had ended never emitted, and no queue holds the evidence.
        for root in &self.index.shape.roots {
            if !self.states[*root].out_done.iter().all(|done| *done) {
                return Err(RunError::Protocol(format!(
                    "nothing is runnable and {} has lanes that never ended",
                    self.index.nodes[*root].node.name()
                ))
                .into());
            }
        }
        let mut stranded: Vec<String> = (0..self.index.len())
            .flat_map(|node| {
                self.states[node]
                    .out_queues
                    .iter()
                    .enumerate()
                    .filter(|(_, queue)| !queue.is_empty())
                    .map(move |(lane, queue)| {
                        format!(
                            "{} lane {lane} holds {}",
                            self.index.nodes[node].node.name(),
                            queue.len()
                        )
                    })
            })
            .collect();
        stranded.extend(self.kept_stranded());
        if stranded.is_empty() {
            return Ok(());
        }
        Err(RunError::Protocol(format!(
            "nothing is runnable and batches remain: {}",
            stranded.join("; ")
        ))
        .into())
    }
}
