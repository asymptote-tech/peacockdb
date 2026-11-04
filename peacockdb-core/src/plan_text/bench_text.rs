//! The benchmark record's text: the plan tree, and what each node's calls cost.
//!
//! A sibling of [`run_text`](super::run_text), not a mode of it: the two answer different
//! questions of one tree and are written by different runs into different files. They
//! share the node line by calling one function, not by a renderer growing a flag.
//!
//! Beside the times go the chosen run's `batch_rows` and `batch_bytes`, written by the
//! execution golden's own list renderer: query-fuser sizes how many queries run at once from
//! the bytes a query's batches hold, and only the timed run says what they were at sf40.
//!
//! Its tests are the driver's: rendering takes a run, and the mock backend is there.

use std::fmt::Write as _;

use super::node_text::{join_parts, lanes_of, node_line_parts};
use crate::executor::{Measured, Measurements, RunReport};
use crate::executor::{PlanIndex, ROOT};
use crate::plan::GpuNode;

/// One call's device microseconds, as this file reports them.
///
/// `0` where no region answered for it: an entry nothing measured, or one the backend
/// served without crossing the ABI. (A journalled call with no region is not among them —
/// `join_regions` refuses that.) `1` where a region exists but its microseconds truncated
/// to zero: something ran on the device, and writing 0 would claim more than the clock can
/// say.
///
/// The one place the rule lives, so a node's total is the sum of the entries beside it by
/// construction rather than by two functions agreeing.
fn call_us(call: &Option<Measured>) -> u64 {
    match call {
        Some(time) if time.regions > 0 => time.device_us.max(1),
        _ => 0,
    }
}

/// The node the device loads a table with: its `total_us` is the run's load.
pub(crate) const LOAD_NODE: &str = "GpuLoadParquet";

/// `root` as it was timed: one node per line with the microseconds its calls spent on the
/// device beneath it, and the batches `report`, the same run, says it emitted.
pub(crate) fn render_timings(
    root: &dyn GpuNode,
    times: &Measurements,
    report: &RunReport,
) -> String {
    let index = PlanIndex::build(root).expect("the tree the run was made over indexes");
    let mut text = String::new();
    render_node(&index, times, report, ROOT, 0, &mut text);
    text
}

fn render_node(
    index: &PlanIndex<'_>,
    times: &Measurements,
    report: &RunReport,
    node: usize,
    depth: usize,
    text: &mut String,
) {
    let indent = "  ".repeat(depth);
    let _ = writeln!(
        text,
        "{indent}{}",
        join_parts(node_line_parts(index.nodes[node].node))
    );
    // Lanes outermost and calls within, the nesting `batch_rows` and `partition_groups`
    // use. `time_us` is by driving lane and call, the batches by output lane and batch: an
    // accumulator makes many calls and emits one batch.
    let batches = &report.emitted[node];
    let _ = writeln!(
        text,
        "{indent}  time_us={} total_us={} batch_rows={} batch_bytes={}",
        time_lanes(times, node),
        total_of(times, node),
        lanes_of(batches, |batch| batch.rows),
        lanes_of(batches, |batch| batch.bytes as u64),
    );
    for child in &index.nodes[node].children {
        render_node(index, times, report, *child, depth + 1, text);
    }
}

/// `[[22,37,40],[55,1,30]]` — one list per driving lane, one entry per call that lane made.
///
/// A lane the node was never driven on renders `[]` rather than being left out: a routing
/// node the driver answers itself has as many empty lanes as it has, and dropping them
/// would make its four lanes read as none.
fn time_lanes(times: &Measurements, node: usize) -> String {
    let rendered: Vec<String> = times
        .lanes(node)
        .iter()
        .map(|lane| {
            let calls: Vec<String> = lane.iter().map(|call| call_us(call).to_string()).collect();
            format!("[{}]", calls.join(","))
        })
        .collect();
    format!("[{}]", rendered.join(","))
}

/// Every node's `total_us` added — the trailer's `device_us`, under the one rule above, so
/// the file's own arithmetic (Σ `total_us` == `device_us`) holds by construction.
pub(crate) fn timed_device_us(times: &Measurements) -> u64 {
    (0..times.nodes()).map(|node| total_of(times, node)).sum()
}

/// Every load node's `total_us` added, under the same rule as `device_us`: the trailer's
/// `load_us`. A plan with no loader (`GpuEmpty` answers from no file) loads for 0.
pub(crate) fn timed_load_us(root: &dyn GpuNode, times: &Measurements) -> u64 {
    let index = PlanIndex::build(root).expect("the tree the run was made over indexes");
    (0..index.nodes.len())
        .filter(|node| index.nodes[*node].node.name() == LOAD_NODE)
        .map(|node| total_of(times, node))
        .sum()
}

/// The node's whole device time: the entries above, added. A node that made no call at all
/// sums nothing and reports 0.
fn total_of(times: &Measurements, node: usize) -> u64 {
    times
        .lanes(node)
        .iter()
        .flat_map(|lane| lane.iter())
        .map(call_us)
        .sum()
}
