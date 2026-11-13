//! The execution golden's text: the plan tree, and what the run produced at each node.
//!
//! The node line is the plan golden's minus the declared schema — a plan fact, recorded
//! once beside the plan — plus the two totals every run carries. Under it goes one
//! line holding what the node consumed and the size of every batch it emitted.
//!
//! Its tests are the driver's, in `driver/tests/render.rs`: rendering a report takes a run,
//! and the mock backend that makes one deterministic lives there.
//!
//! The tree is walked through `PlanDag`, the walk the driver's `PlanIndex` is numbered by,
//! so a node's index here is the one the report is addressed by.

use std::fmt::Write as _;

use super::dag_walk;
use super::node_text::{join_parts, lanes_of, node_line_parts, numbers};
use crate::executor::RunReport;
use crate::plan::{GpuNode, PlanDag};

/// The roots as they ran: the early-exit marker, then one node per line with its per-batch
/// record beneath it.
pub(crate) fn render_runs(roots: &[&dyn GpuNode], report: &RunReport) -> String {
    let dag = PlanDag::of(roots);
    let tree = dag_walk::render(&dag, &mut |at, indent, prefix, text| {
        render_node(&dag, report, at, indent, prefix, text);
    });
    format!("early_exit={}\n{tree}", early_exit(&dag, report))
}

/// Which limits were satisfied, by name and post-order address — the address because that
/// is what both engines number a node by, and the name because nobody reads an address.
/// `none` where the run drained, stated rather than left to an absence, since a smaller
/// number with no cause reads exactly like a plan that produced less.
fn early_exit(dag: &PlanDag<'_>, report: &RunReport) -> String {
    if report.satisfied.is_empty() {
        return "none".to_string();
    }
    let named: Vec<String> = report
        .satisfied
        .iter()
        .map(|node| {
            format!(
                "{}@{}",
                dag.nodes[*node].node.name(),
                dag.nodes[*node].post_order
            )
        })
        .collect();
    named.join(",")
}

fn render_node(
    dag: &PlanDag<'_>,
    report: &RunReport,
    node: usize,
    indent: &str,
    prefix: &str,
    text: &mut String,
) {
    let batches = &report.emitted[node];
    let mut parts = node_line_parts(dag.nodes[node].node);
    parts.push(format!(
        "output_rows={}",
        batches
            .iter()
            .flatten()
            .map(|batch| batch.rows)
            .sum::<u64>()
    ));
    parts.push(format!(
        "output_bytes={}",
        batches
            .iter()
            .flatten()
            .map(|batch| batch.bytes)
            .sum::<usize>()
    ));
    let _ = writeln!(text, "{indent}{prefix}{}", join_parts(parts));
    // Lanes outermost and batches within, which is `partition_groups`' own nesting: on a
    // source, element i of lane j is one batch in all three lists and in the mapping.
    let _ = write!(
        text,
        "{indent}  in_rows={} batch_rows={} batch_bytes={}",
        lanes_of(&report.consumed[node], |rows| *rows),
        lanes_of(batches, |batch| batch.rows),
        lanes_of(batches, |batch| batch.bytes as u64),
    );
    // Both are early-exit quantities, so both are absent from every node of every run that
    // drained rather than a zero on each of them.
    let abandoned = &report.abandoned[node];
    if abandoned.iter().any(|rows| *rows > 0) {
        let _ = write!(text, " abandoned={}", numbers(abandoned));
    }
    if report.rows_skipped[node] > 0 {
        let _ = write!(text, " rows_skipped={}", report.rows_skipped[node]);
    }
    text.push('\n');
}
