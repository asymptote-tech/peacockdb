//! Rendering a plan as text: one line per node, indentation as the tree.
//!
//! One rule runs through it — the ordinal is authoritative and the name comes from the
//! declared schema at that position, so every column reference reads `name@ordinal` and a
//! name disagreeing with its ordinal is visible rather than invisible. The rest is what a
//! reader of a golden needs and cannot derive: the layout a node declares, every `fetch`
//! that trims rows, the loader's mapping verbatim, and the declared schema, without which
//! an explicit cast's target means nothing.

mod expr_text;
mod memory;
mod node_text;
mod run_text;

#[cfg(test)]
mod tests;

use crate::executor::RunReport;
use crate::plan::Expr;
use crate::plan::GpuNode;
use crate::planner::MemoryModel;

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

/// The execution golden: the plan with what each node actually ran under it.
pub fn render_run(root: &dyn GpuNode, report: &RunReport) -> String {
    run_text::render_run(root, report)
}
