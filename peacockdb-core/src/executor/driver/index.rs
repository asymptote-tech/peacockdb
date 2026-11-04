//! The plan indexed once: heights, the numbering, lane counts, and the regions every hold is
//! expressed over.
//!
//! Every number is `PlanDag`'s, so one walk decides them for every consumer: a node read by
//! several parents is indexed once, at its first visit, and "order" — the leftmost tie-break
//! — is the index itself. On a tree a region is a contiguous range, as it always was.
//! Children are snapshotted here because [`GpuNode::children`] builds a `Vec` per call, and
//! the schedule asks for them on every step.

use crate::executor::{IndexedNode, JoinShape, PlanIndex, PlanShape};
use crate::plan::PlanError;
use crate::plan::{ExecutorCategory, category_of};
use crate::plan::{GpuNode, PlanDag, Reader};

pub(crate) fn build<'a>(roots: &[&'a dyn GpuNode]) -> Result<PlanIndex<'a>, PlanError> {
    let dag = PlanDag::of(roots);
    let mut nodes: Vec<IndexedNode<'a>> = dag
        .nodes
        .iter()
        .enumerate()
        .map(|(at, node)| IndexedNode {
            node: node.node,
            category: category_of(node.node),
            children: node.children.clone(),
            readers: node.readers.clone(),
            child_readers: node
                .children
                .iter()
                .enumerate()
                .map(|(slot, child)| {
                    dag.nodes[*child]
                        .readers
                        .iter()
                        .position(|reader| *reader == Reader { parent: at, slot })
                        .expect("every edge is one of its child's readers")
                })
                .collect(),
            lanes: 0,
            post_order: node.post_order,
            input_lanes: 0,
            interval: node.node.row_interval(),
            ready_lanes: 0,
            slot_base: 0,
        })
        .collect();

    // Children first, so a sink, which declares no layout, can take its input's lanes.
    for at in dag.in_post_order() {
        let node = &nodes[at];
        let lanes = match node.node.kind().layout() {
            Some(layout) => layout.n,
            None => nodes[node.children[0]].lanes,
        };
        let input_lanes = if node.category == ExecutorCategory::PartitionAccumulator {
            nodes[node.children[0]].lanes
        } else {
            0
        };
        nodes[at].lanes = lanes;
        nodes[at].input_lanes = input_lanes;
    }

    for node in &mut nodes {
        node.ready_lanes = match node.category {
            ExecutorCategory::PartitionAccumulator => node.input_lanes,
            ExecutorCategory::PartitionEmitter => 1,
            _ => node.lanes,
        };
    }
    let joins = nodes
        .iter()
        .enumerate()
        .filter(|(_, node)| node.category == ExecutorCategory::Join)
        .map(|(at, node)| JoinShape {
            node: at,
            probe: dag.region_of_edge(at, PROBE_CHILD),
            lanes: node.lanes,
        })
        .collect();

    let mut slots = 0;
    for node in &mut nodes {
        node.slot_base = slots;
        slots += if node.category.is_lane_scoped() {
            node.lanes
        } else {
            1
        };
    }

    Ok(PlanIndex {
        shape: PlanShape {
            heights: dag.nodes.iter().map(|node| node.height).collect(),
            lanes: nodes.iter().map(|node| node.ready_lanes).collect(),
            regions: (0..nodes.len()).map(|at| dag.region_of_node(at)).collect(),
            readers: dag.nodes.iter().map(|node| node.readers.clone()).collect(),
            joins,
            roots: dag.roots.clone(),
            buffers: (0..nodes.len()).map(|at| dag.is_buffer(at)).collect(),
        },
        nodes,
        slots,
    })
}

pub(crate) fn slot(index: &PlanIndex<'_>, node: usize, lane: usize) -> usize {
    let indexed = &index.nodes[node];
    indexed.slot_base
        + if indexed.category.is_lane_scoped() {
            lane
        } else {
            0
        }
}

pub(crate) const PROBE_CHILD: usize = 1;

/// Each node as a record names it — its type and its children-first position — in the
/// driver's own pre-order, which is the order `RunReport` is indexed by. From here rather
/// than from a caller's own walk, so the two orders cannot disagree.
#[cfg_attr(feature = "rust-only", allow(dead_code))]
pub(crate) fn nodes_as_recorded(
    root: &dyn GpuNode,
) -> Result<Vec<(&'static str, usize)>, PlanError> {
    Ok(build(&[root])?
        .nodes
        .iter()
        .map(|node| (node.node.name(), node.post_order))
        .collect())
}

#[cfg(test)]
mod tests;
