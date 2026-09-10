//! The two drivers, the schedule they follow, and the accounting they fail a query on.
//!
//! [`partitioned`] owns the tree, the queues, the schedule and the three cross-lane
//! categories; [`single_partition`] is one lane of one lane-scoped node, deciding which
//! call that lane's input state calls for and making exactly one. Both are generic over
//! [`Backend`](crate::executor::Backend), so each backend monomorphizes and nothing on the
//! per-batch path is boxed. The reasons are `llm-wiki/architecture.md`, under Execution.

mod accounting;
mod index;
mod partitioned;
mod scheduler;
mod single_partition;

#[cfg(test)]
mod mock;
#[cfg(test)]
mod plans;
#[cfg(test)]
mod tests;

use crate::batch_partitioned::error::PlanError;
use crate::batch_partitioned::node::GpuNode;
use crate::executor::{Backend, PlanIndex, RunError, RunReport};
use accounting::Trip;

/// Run `root` to completion on `B`. `budget` of `None` accounts without ever tripping.
pub(crate) fn run<B: Backend>(
    root: &dyn GpuNode,
    ctx: &B::Context,
    budget: Option<usize>,
) -> Result<RunReport, RunError> {
    partitioned::run::<B>(root, ctx, budget)
}

/// The tree indexed once — heights, pre-order numbering, lane counts and slot ranges.
pub(crate) fn build_index<'a>(root: &'a dyn GpuNode) -> Result<PlanIndex<'a>, PlanError> {
    index::build(root)
}

/// Where a node's lane keeps its accounting slot.
pub(crate) fn slot_of(index: &PlanIndex<'_>, node: usize, lane: usize) -> usize {
    index::slot(index, node, lane)
}

/// Every node's post-order address, indexed by its pre-order one.
pub(crate) fn post_order_of_every_node(root: &dyn GpuNode) -> Result<Vec<usize>, PlanError> {
    index::post_order_of_every_node(root)
}

/// A trip carries no name, and the driver is what can supply one — so every step returns
/// this and the driver renders it at the one place a query ends.
#[derive(Debug)]
pub(crate) enum StepError {
    Run(RunError),
    Trip(Trip),
}

impl From<RunError> for StepError {
    fn from(error: RunError) -> Self {
        Self::Run(error)
    }
}

impl From<Trip> for StepError {
    fn from(trip: Trip) -> Self {
        Self::Trip(trip)
    }
}
