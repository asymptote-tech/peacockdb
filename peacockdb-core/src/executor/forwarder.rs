//! Batch forwarding: the nodes that renumber lanes without touching rows.
//!
//! `GpuMergePartitions`, `GpuUnion` and `GpuInterleave` are one driver arm over three
//! mappings. A visit to output lane p cycles `sources_of(p)` in listed order, forwarding
//! one batch per visit, skipping sources with nothing queued and retiring those whose
//! producer has finished — the merge's round-robin and the interleave's per-lane child
//! rotation are that same rule.

use super::{BatchForwarder, Forwarder};
use crate::plan::GpuNode;
use crate::plan::{NodeRef, as_node_ref};

impl BatchForwarder for Forwarder {
    fn sources_of(&self, out_lane: usize) -> Vec<(usize, usize)> {
        match self {
            Self::MergePartitions { n } => {
                assert_eq!(out_lane, 0, "GpuMergePartitions has one output lane");
                (0..*n).map(|lane| (0, lane)).collect()
            }
            Self::Union { lanes } => vec![lanes[out_lane]],
            Self::Interleave { children, n } => {
                assert!(
                    out_lane < *n,
                    "output lane {out_lane} is outside the interleave"
                );
                (0..*children).map(|child| (child, out_lane)).collect()
            }
        }
    }
}

/// The routing a node declares, which is a property of the node rather than of a backend —
/// every backend would compute the same thing, so it is read off the node by whoever routes.
pub(crate) fn forwarder_for(node: &dyn GpuNode) -> Forwarder {
    let lanes = |plan: &dyn GpuNode| plan.kind().layout().map_or(1, |layout| layout.n);
    match as_node_ref(node) {
        NodeRef::MergePartitions(_) => Forwarder::MergePartitions {
            n: lanes(node.children()[0]),
        },
        NodeRef::Union(_) => Forwarder::Union {
            lanes: node
                .children()
                .iter()
                .enumerate()
                .flat_map(|(child, branch)| (0..lanes(*branch)).map(move |lane| (child, lane)))
                .collect(),
        },
        NodeRef::Interleave(_) => Forwarder::Interleave {
            children: node.children().len(),
            n: lanes(node),
        },
        _ => unreachable!("only the three routing nodes are forwarders"),
    }
}

#[cfg(test)]
mod tests;
