//! Planning a query: DataFusion's physical plan in, a node tree and its memory model out.
//!
//! Two passes, because the halves define each other — a batch size needs the tree it flows
//! through, and the tree's mapping needs a batch size. The first assumes one number per
//! source and exists only to derive the real ones. The shape does not move between them.
//!
//! `translator` and `memory_estimation` are subcomponents, so only this file may reach them;
//! the four entry points below are what a test drives instead.
mod memory_estimation;
mod nulls;
mod pipeline;
mod translator;

#[cfg(test)]
mod tests;

use std::sync::Arc;

use datafusion::physical_plan::ExecutionPlan;

use crate::plan::{GpuNode, PlanError};

// Only the two `#[cfg(test)]` entry points below name these.
#[cfg(test)]
use crate::plan::{Batching, Expr};
#[cfg(test)]
use datafusion::arrow::datatypes::Schema as ArrowSchema;
#[cfg(test)]
use datafusion::physical_plan::PhysicalExpr;

/// A source reading less than this stops being worth splitting: it has nothing to gain
/// from lanes and would pay a shuffle for them.
///
/// From the sf1 measurement at full projection: the largest table that must stay on one
/// lane is tpcds date_dim at 4,006,445 bytes, the smallest that must not is tpcds
/// web_returns at 8,041,397, and tpch supplier at 1,532,237 sets the floor. 5 MiB sits in
/// that gap nearer the lower end, so date_dim would have to grow 31% to cross it and
/// web_returns shrink 35%. It reads the projected bytes of the surviving row groups, so a
/// narrow scan of a big table falls below it — the rule working, not a value to retune.
pub const SMALL_TABLE_BYTES: u64 = 5 * 1024 * 1024;

/// The planner inputs a mode fixes: how many lanes to aim for, whether a lane holds more
/// than one batch, the budget the estimator divides, and the byte count below which a
/// source stops being worth splitting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlanKnobs {
    pub target_partitions: usize,
    pub sizing: BatchSizing,
    /// Read only by [`BatchSizing::Budgeted`]; the other two forms reproduce a plan from
    /// the data alone.
    pub budget: u64,
    pub small_table_bytes: u64,
}

/// What a mode asks of the partitioner. The planner's half of [`Batching`]: `Budgeted` has
/// no number until the estimator solves for one, which is why it is a separate word.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BatchSizing {
    OneBatchPerLane,
    OneBatchPerRowGroup,
    Budgeted,
}

/// What a golden's `--- memory ---` section renders: a figure per node in canonical
/// post-order, and the batch size each source was given.
#[derive(Debug, Clone, PartialEq)]
pub struct MemoryModel {
    pub(crate) budget: u64,
    /// Σ over the accumulators — held whatever the batch size, so it is spent first.
    pub(crate) accumulator_bytes: u64,
    /// The part of it that cannot be an overestimate — a build side is its input's rows,
    /// where an aggregate's state rests on a cardinality estimate. Only this part can
    /// refuse a plan.
    pub(crate) certain_accumulator_bytes: u64,
    /// What each source may spend, before its own amplification and size narrow it.
    pub(crate) share_per_source: u64,
    /// `estimated_max_resident_size` per node, indexed by post-order sequence.
    pub(crate) resident: Vec<u64>,
    /// One per source, in post-order sequence — which is the order translation reaches
    /// them, so the second pass consumes them in this order.
    pub(crate) sources: Vec<SourceEstimate>,
}

/// What the walk from one source found, and what it was given for it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct SourceEstimate {
    pub(crate) seq: usize,
    /// The largest a batch from this source gets on its way to the accumulator that ends
    /// the walk, counting the lanes live at that point.
    pub(crate) amplification: f64,
    pub(crate) target_batch_bytes: u64,
}

/// A batch size below this is not worth deriving: the mapping is quantized to whole row
/// groups anyway, so the model would be pretending to a precision the plan cannot use.
pub(crate) const MIN_TARGET_BATCH_BYTES: u64 = 1 << 20;

pub fn plan(
    root: &Arc<dyn ExecutionPlan>,
    knobs: PlanKnobs,
) -> Result<(Box<dyn GpuNode>, MemoryModel), PlanError> {
    pipeline::plan(root, knobs)
}

pub(crate) fn estimate(root: &dyn GpuNode, budget: u64) -> Result<MemoryModel, PlanError> {
    memory_estimation::estimate(root, budget)
}

/// Refuses an anti or mark join whose NULLs can meet under SQL semantics. Everything else
/// plans: semi honours the flag, and `null_equals_null=true` is asking for the equality the
/// executor hardcodes.
pub(crate) fn refuse_null_unsafe_joins(root: &dyn GpuNode) -> Result<(), PlanError> {
    nulls::refuse_null_unsafe_joins(root)
}

/// A DataFusion physical plan as a node tree, without the pipeline around it: no
/// validation, no null analysis and no memory model.
///
/// `#[cfg(test)]` because only tests want a tree without the pipeline: `plan_text/tests.rs`
/// and `planner/memory_estimation/tests.rs`. They are the reason this exists rather than
/// `Translator` being reachable: a subcomponent is the parent's alone, and a test in
/// `plan_text` is not the parent.
#[cfg(test)]
pub(crate) fn translate(
    target_partitions: usize,
    batching: Batching,
    plan: &Arc<dyn ExecutionPlan>,
) -> Result<Box<dyn GpuNode>, PlanError> {
    translator::Translator::new(target_partitions, batching).translate(plan)
}

/// One DataFusion physical expression in this engine's own vocabulary. `#[cfg(test)]` for
/// the same reason as [`translate`]: its one caller is
/// `executor/cpu_backend/expr_physical/tests.rs`, a test in another component.
#[cfg(test)]
pub(crate) fn translate_expr(
    expr: &Arc<dyn PhysicalExpr>,
    input_schema: &ArrowSchema,
) -> Result<Expr, PlanError> {
    translator::translate_expr(expr, input_schema)
}
