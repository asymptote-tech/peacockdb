//! The `--- memory ---` section: the estimator's per-node figures under the same tree the
//! plan renders, so a line reads against the node above it.

use std::fmt::Write as _;

use super::dag_walk;
use crate::plan::{GpuNode, PlanDag};
use crate::plan::{NodeRef, as_node_ref};
use crate::planner::MemoryModel;

/// The sibling memory golden: the same tree, carrying what the estimator derived rather
/// than what the planner decided. Sources also print the batch size they were given, which
/// is the one number the partitioner reads back. The model is indexed by `PlanDag`'s
/// post-order, which the walk's nodes carry.
pub(crate) fn render_plans_memory(roots: &[&dyn GpuNode], model: &MemoryModel) -> String {
    let dag = PlanDag::of(roots);
    let tree = dag_walk::render(&dag, &mut |at, indent, prefix, text| {
        let (node, seq) = (dag.nodes[at].node, dag.nodes[at].post_order);
        let mut fields = vec![format!(
            "estimated_max_resident_size={}",
            model.resident.get(seq).copied().unwrap_or(0)
        )];
        if let Some(source) = model.sources.iter().find(|source| source.seq == seq) {
            // What the source holds is what makes the rest of the line legible: it is the
            // cap on the target, so a reader sees whether the target is the source or the
            // budget talking, and it is what the small-table threshold compared to decide
            // the lanes.
            if let NodeRef::LoadParquet(load) = as_node_ref(node) {
                fields.push(format!("source_bytes={}", load.bytes()));
            }
            fields.push(format!(
                "target_batch_bytes={}, amplification={:.1}",
                source.target_batch_bytes, source.amplification
            ));
        }
        let _ = writeln!(
            text,
            "{indent}{prefix}{}: {}",
            node.name(),
            fields.join(", ")
        );
    });
    format!(
        "budget={}, accumulators={}, certain={}\n{tree}",
        model.budget, model.accumulator_bytes, model.certain_accumulator_bytes
    )
}
