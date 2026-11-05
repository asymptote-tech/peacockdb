//! A satisfied interval: the node is held and done, and so is everything only it reads. A
//! reader crossing out of its region into a shared source retires, giving back what only it
//! still wanted; a source with no reader left stops; a buffer inside the region gives back
//! its store rather than emit it.
//!
//! A child of `partitioned` so it reads the driver's own fields; apart from it only to keep
//! that file under its length limit.

use super::Driver;
use crate::executor::driver::StepError;
use crate::executor::{Backend, RunError};
use crate::plan::ExecutorCategory;

impl<B: Backend> Driver<'_, B> {
    /// Enough rows have passed this node that no later one can change its answer: an
    /// unload's input has reached `skip + fetch`, or a mid-plan limit has emitted `fetch`.
    /// It is marked done as it is held, or the hold would stop it reporting and strand its
    /// parent — `LIMIT 0` is the case that forces it.
    pub(crate) fn settle_limit(&mut self, node: usize) -> Result<(), StepError> {
        let indexed = &self.index.nodes[node];
        let Some(interval) = indexed.interval else {
            return Ok(());
        };
        let (category, name) = (indexed.category, indexed.node.name());
        let satisfied = match category {
            ExecutorCategory::Unload => interval.satisfied_by(self.rows_seen[node]),
            ExecutorCategory::BatchAccumulator => {
                interval.satisfied_by_emitted(self.rows_emitted[node])
            }
            // The two carriers are judged by different counts, so a third would be judged
            // by whichever rule it fell into rather than by one written for it.
            _ => {
                return Err(StepError::Run(RunError::Protocol(format!(
                    "{name}: a row interval on neither an unload nor a limit"
                ))));
            }
        };
        if !satisfied {
            return Ok(());
        }
        let retired = self.scheduler.satisfy(node);
        let mut stopped = vec![node];
        for (producer, reader) in retired {
            for lane in 0..self.states[producer].out_queues.len() {
                for held in self.states[producer].out_queues[lane].retire(reader) {
                    self.acct.release(held.charge)?;
                }
            }
            if self.scheduler.is_satisfied(producer) && !stopped.contains(&producer) {
                stopped.push(producer);
            }
        }
        for node in stopped {
            self.states[node].out_done.fill(true);
            self.release_buffers_in(node)?;
        }
        Ok(())
    }

    /// The hold on a satisfied region never lifts, so a buffer in it would keep its store to
    /// the run's end. It was counted as consumed when it took each batch, so what it gives
    /// back here is no reader's abandoned rows.
    fn release_buffers_in(&mut self, node: usize) -> Result<(), StepError> {
        for at in 0..self.index.shape.regions[node].len() {
            let member = self.index.shape.regions[node][at];
            if !self.index.shape.buffers[member] {
                continue;
            }
            for lane in 0..self.states[member].kept.len() {
                while let Some(held) = self.states[member].kept[lane].pop_front() {
                    self.acct.release(held.charge)?;
                }
            }
            self.states[member].out_done.fill(true);
        }
        Ok(())
    }
}
