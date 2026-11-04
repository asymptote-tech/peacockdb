//! A hold buffer's arm: one store per lane. Under a hold it takes each batch its input lane
//! offers and emits nothing; once the hold lifts it emits the store in arrival order, then
//! forwards as batches arrive. One batch per lane per step either way, so the queue above it
//! stays at one. Every batch moves with its accounting, so a kept batch stays held until the
//! buffer emits it or the run releases it.

use super::Driver;
use crate::executor::driver::StepError;
use crate::executor::{Backend, CallKind};

impl<B: Backend> Driver<'_, B> {
    pub(crate) fn run_hold_buffer(&mut self, node: usize) {
        let child = self.index.nodes[node].children[0];
        let held = self.scheduler.is_held(node);
        for lane in 0..self.index.nodes[node].lanes {
            if !self.hold_buffer_lane_ready(node, lane) {
                continue;
            }
            if held {
                let batch = self.states[child].out_queues[lane]
                    .pop_front()
                    .expect("a held lane is ready only when it is offered a batch");
                self.record_consumed(node, 0, lane, batch.rows());
                self.states[node].kept[lane].push_back(batch);
                self.record(node, lane, CallKind::Keep, 0);
                continue;
            }
            let next = match self.states[node].kept[lane].pop_front() {
                Some(kept) => Some(kept),
                None => {
                    let arrived = self.states[child].out_queues[lane].pop_front();
                    if let Some(batch) = &arrived {
                        self.record_consumed(node, 0, lane, batch.rows());
                    }
                    arrived
                }
            };
            match next {
                Some(batch) => {
                    self.record_emitted(node, lane, &batch);
                    self.states[node].out_queues[lane].push_back(batch);
                    self.record(node, lane, CallKind::Forward, 1);
                }
                None => {
                    self.states[node].out_done[lane] = true;
                    self.record(node, lane, CallKind::ForwardDone, 0);
                }
            }
        }
    }

    /// Held, a lane is ready only when offered a batch: one that stepped with nothing to take
    /// would be picked again at once. Unheld, also while it keeps something, and once its
    /// input has ended, to end too.
    pub(crate) fn hold_buffer_lane_ready(&self, node: usize, lane: usize) -> bool {
        if self.states[node].out_done[lane] {
            return false;
        }
        let child = self.index.nodes[node].children[0];
        let offered = !self.states[child].out_queues[lane].is_empty();
        if self.scheduler.is_held(node) {
            return offered;
        }
        offered || !self.states[node].kept[lane].is_empty() || self.states[child].out_done[lane]
    }

    /// A join's hold has lifted. A buffer's readiness reads the hold, and nothing else
    /// re-checks a node no step touched, so each buffer the hold covered is re-checked here.
    pub(crate) fn hold_lifted(&mut self, join: usize) {
        let probe = self
            .index
            .shape
            .joins
            .iter()
            .find(|shape| shape.node == join)
            .expect("a join reports its own node")
            .probe
            .clone();
        for node in probe {
            if self.index.shape.buffers[node] {
                self.refresh(node);
            }
        }
    }

    pub(crate) fn release_kept(&mut self) -> Result<(), StepError> {
        for node in 0..self.index.len() {
            for lane in 0..self.states[node].kept.len() {
                while let Some(batch) = self.states[node].kept[lane].pop_front() {
                    self.acct.release(batch.bytes)?;
                }
            }
        }
        Ok(())
    }

    /// What a buffer still keeps at a run's end, as `assert_drained` names a stranded queue.
    pub(crate) fn kept_stranded(&self) -> Vec<String> {
        (0..self.index.len())
            .flat_map(|node| {
                self.states[node]
                    .kept
                    .iter()
                    .enumerate()
                    .filter(|(_, store)| !store.is_empty())
                    .map(move |(lane, store)| {
                        format!(
                            "{} lane {lane} keeps {}",
                            self.index.nodes[node].node.name(),
                            store.len()
                        )
                    })
            })
            .collect()
    }
}
