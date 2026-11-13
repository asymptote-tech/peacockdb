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

mod bench_text;
mod expr_text;
mod memory;
mod node_text;
mod run_text;

#[cfg(test)]
mod tests;

use crate::executor::{Measurements, RunReport};
use crate::plan::GpuNode;
use crate::planner::MemoryModel;

/// The node the device loads a table with: its `total_us` is a benchmark run's load, as the
/// renderer adds it and as the tree's reader adds it back.
pub(crate) const LOAD_NODE: &str = "GpuLoadParquet";

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

/// The benchmark tree: the plan with the device microseconds each node's calls spent, and
/// the batches each node emitted in the same run.
pub(crate) fn render_timings(
    root: &dyn GpuNode,
    times: &Measurements,
    report: &RunReport,
) -> String {
    bench_text::render_timings(root, times, report)
}

/// The tree's `total_us` added under the renderer's rule — what the trailer reports as
/// `device_us`, so the two cannot disagree by a region the clock rounded to zero.
pub(crate) fn timed_device_us(times: &Measurements) -> u64 {
    bench_text::timed_device_us(times)
}

/// The tree's `GpuLoadParquet` totals added — the trailer's `load_us`, a part of
/// [`timed_device_us`] under the same rule, so `execute_us` never goes below zero.
pub(crate) fn timed_load_us(root: &dyn GpuNode, times: &Measurements) -> u64 {
    bench_text::timed_load_us(root, times)
}
