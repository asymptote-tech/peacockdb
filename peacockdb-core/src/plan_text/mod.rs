//! Rendering a plan as text: one line per node, indentation as the tree.
//!
//! One rule runs through it — the ordinal is authoritative and the name comes from the
//! declared schema at that position, so every column reference reads `name@ordinal` and a
//! name disagreeing with its ordinal is visible rather than invisible. The rest is what a
//! reader of a golden needs and cannot derive: the layout a node declares, every `fetch`
//! that trims rows, the loader's mapping verbatim, and the declared schema, without which
//! an explicit cast's target means nothing.

mod expr_text;
mod fb_text;
mod memory;
mod node_text;
mod recipes;
mod run_text;

#[cfg(test)]
mod tests;

use crate::batch_partitioned::estimator::MemoryModel;
use crate::batch_partitioned::expr::Expr;
use crate::batch_partitioned::node::GpuNode;
use crate::batch_partitioned::recipe::RecipePlan;
use crate::executor::RunReport;

/// Whether the recipes section prints what each call passes the executor, or only which
/// kernel it addresses. One renderer either way: two would drift, and the ten mode goldens
/// and the payload golden would then disagree about a plan neither of them changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Payloads {
    Omitted,
    Shown,
}

/// The plan under `root`, one line per node. The plan golden carries the declared schema
/// per node; an execution golden does not, since what it records is what ran.
pub fn render_plan(root: &dyn GpuNode) -> String {
    node_text::render_plan(root)
}

/// One expression, with every column reference as `name@ordinal`.
pub fn expr_text(expr: &Expr) -> String {
    expr_text::expr_text(expr)
}

/// The `--- memory ---` section: what the estimator predicted, per node.
pub fn render_plan_memory(root: &dyn GpuNode, model: &MemoryModel) -> String {
    memory::render_plan_memory(root, model)
}

/// The `--- recipes ---` section: what each node asks of the device, under the same tree
/// the plan renders, so a line reads against the node above it.
pub fn render_plan_recipes(root: &dyn GpuNode, plan: &RecipePlan, payloads: Payloads) -> String {
    recipes::render_plan_recipes(root, plan, payloads)
}

/// The execution golden: the plan with what each node actually ran under it.
pub fn render_run(root: &dyn GpuNode, report: &RunReport) -> String {
    run_text::render_run(root, report)
}
