//! Rendering a plan as text: one line per node, indentation as the tree.
//!
//! One rule runs through it — the ordinal is authoritative and the name comes from the
//! declared schema at that position, so every column reference reads `name@ordinal` and a
//! name disagreeing with its ordinal is visible rather than invisible. The rest is what a
//! reader of a golden needs and cannot derive: the layout a node declares, every `fetch`
//! that trims rows, the loader's mapping verbatim, and the declared schema, without which
//! an explicit cast's target means nothing.

// Nothing in a production build renders a plan yet: the CLI prints results
// (`peacockdb/src/main.rs`), the corpus harness renders the execution golden, and the plan
// and memory goldens are the tests'. So the lint is read where the callers are, and only
// there; the code stays in every build. The attribute leaves with the first CLI renderer.
#![cfg_attr(not(test), allow(dead_code))]

mod declared;
mod expr_text;
mod memory;
mod node_text;
mod run_text;

#[cfg(test)]
mod tests;

use crate::executor::RunReport;
use crate::plan::GpuNode;
use crate::planner::MemoryModel;
use crate::wire::RecipePlan;

/// The plan under `root`, one line per node. The plan golden carries the declared schema
/// per node; an execution golden does not, since what it records is what ran.
pub(crate) fn render_plan(root: &dyn GpuNode) -> String {
    node_text::render_plan(root)
}

/// The `--- memory ---` section: what the estimator predicted, per node.
pub(crate) fn render_plan_memory(root: &dyn GpuNode, model: &MemoryModel) -> String {
    memory::render_plan_memory(root, model)
}

/// The execution golden: the plan with what each node actually ran under it.
pub(crate) fn render_run(root: &dyn GpuNode, report: &RunReport) -> String {
    run_text::render_run(root, report)
}

/// The payload golden's second section: the schema each call declares, read off the
/// `Call` data before `Writer` serializes anything. A different source from the payload
/// section, which is read back out of the finished buffer — which is the point: when a
/// later task puts schemas on the wire, the two sections become each other's check.
pub(crate) fn render_declared_schemas(root: &dyn GpuNode, plan: &RecipePlan) -> String {
    declared::render_declared_schemas(root, plan)
}
