//! The one walk over a plan with several roots: each node visited once, wherever it is read.
//!
//! On a tree every number here is the one the recursive walk gave — pre-order indices,
//! post-order, depth as height, a node's region its contiguous subtree — which is what keeps
//! every golden unchanged. On a DAG a node's index and post-order come from its first visit,
//! its height from its deepest reader, and a region skips whatever is also read from outside.
//! A hold that lifts stops at a `GpuHoldBuffer`: its region decides nothing through one.

use std::collections::HashMap;

use super::{DagNode, GpuNode, NodeRef, PlanDag, Reader, try_as_node_ref};

pub(crate) fn of<'a>(roots: &[&'a dyn GpuNode]) -> PlanDag<'a> {
    let mut walk = Walk {
        nodes: Vec::new(),
        seen: HashMap::new(),
        next_post_order: 0,
    };
    let roots = roots.iter().map(|root| walk.visit(*root, None)).collect();
    let mut dag = PlanDag {
        nodes: walk.nodes,
        roots,
    };
    longest_paths(&mut dag);
    dag
}

pub(crate) fn in_post_order(dag: &PlanDag<'_>) -> Vec<usize> {
    let mut order = vec![0; dag.nodes.len()];
    for (at, node) in dag.nodes.iter().enumerate() {
        order[node.post_order] = at;
    }
    order
}

pub(crate) fn region_of_node(dag: &PlanDag<'_>, node: usize) -> Vec<usize> {
    node_region(dag, node, Through::Buffers)
}

pub(crate) fn held_region_of_node(dag: &PlanDag<'_>, node: usize) -> Vec<usize> {
    node_region(dag, node, Through::NoBuffer)
}

pub(crate) fn region_of_edge(dag: &PlanDag<'_>, parent: usize, slot: usize) -> Vec<usize> {
    let mut inside = vec![false; dag.nodes.len()];
    let child = dag.nodes[parent].children[slot];
    let edge = Some(Reader { parent, slot });
    decide(dag, &[child], &mut inside, edge, Through::NoBuffer);
    members(&inside)
}

pub(crate) fn is_buffer(dag: &PlanDag<'_>, node: usize) -> bool {
    matches!(
        try_as_node_ref(dag.nodes[node].node),
        Some(NodeRef::HoldBuffer(_))
    )
}

/// Whether a region is decided below a buffer. A limit's hold never lifts, so it covers what
/// a buffer would keep; a hold that lifts stops there, since the buffer takes what it is
/// offered and its subtree runs as if nothing were held.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Through {
    Buffers,
    NoBuffer,
}

fn node_region(dag: &PlanDag<'_>, node: usize, through: Through) -> Vec<usize> {
    let mut inside = vec![false; dag.nodes.len()];
    inside[node] = true;
    if through == Through::Buffers || !is_buffer(dag, node) {
        decide(dag, &dag.nodes[node].children, &mut inside, None, through);
    }
    members(&inside)
}

/// What makes two visits one node: a shared source's key, else the node's address.
fn identity(node: &dyn GpuNode) -> usize {
    node.sharing_key()
        .unwrap_or(node as *const dyn GpuNode as *const () as usize)
}

struct Walk<'a> {
    nodes: Vec<DagNode<'a>>,
    seen: HashMap<usize, usize>,
    next_post_order: usize,
}

impl<'a> Walk<'a> {
    fn visit(&mut self, node: &'a dyn GpuNode, reader: Option<Reader>) -> usize {
        let key = identity(node);
        if let Some(&at) = self.seen.get(&key) {
            self.nodes[at].readers.extend(reader);
            return at;
        }
        let at = self.nodes.len();
        self.seen.insert(key, at);
        self.nodes.push(DagNode {
            node,
            children: Vec::new(),
            readers: reader.into_iter().collect(),
            post_order: 0,
            height: 0,
        });
        for (slot, child) in node.children().into_iter().enumerate() {
            let child = self.visit(child, Some(Reader { parent: at, slot }));
            self.nodes[at].children.push(child);
        }
        self.nodes[at].post_order = self.next_post_order;
        self.next_post_order += 1;
        at
    }
}

/// A parent's post-order is above every child's, so the reverse of post-order reaches each
/// node after all its readers, and its height is final when its children are raised.
fn longest_paths(dag: &mut PlanDag<'_>) {
    for at in in_post_order(dag).into_iter().rev() {
        let height = dag.nodes[at].height + 1;
        for slot in 0..dag.nodes[at].children.len() {
            let child = dag.nodes[at].children[slot];
            dag.nodes[child].height = dag.nodes[child].height.max(height);
        }
    }
}

/// Every node below `starts` joins the region once each of its readers is the `edge` or a
/// member. A reader is strictly lower than what it reads, so deciding in ascending height
/// decides every reader first. A node reached only through a buffer is not decided, so it
/// stays out under `Through::NoBuffer`; one also reached around the buffer is decided.
fn decide(
    dag: &PlanDag<'_>,
    starts: &[usize],
    inside: &mut [bool],
    edge: Option<Reader>,
    through: Through,
) {
    let mut below = descendants(dag, starts, through);
    below.sort_by_key(|&at| (dag.nodes[at].height, at));
    for at in below {
        inside[at] = dag.nodes[at]
            .readers
            .iter()
            .all(|reader| Some(*reader) == edge || inside[reader.parent]);
    }
}

fn descendants(dag: &PlanDag<'_>, starts: &[usize], through: Through) -> Vec<usize> {
    let mut seen = vec![false; dag.nodes.len()];
    let mut stack = starts.to_vec();
    let mut found = Vec::new();
    while let Some(at) = stack.pop() {
        if !std::mem::replace(&mut seen[at], true) {
            found.push(at);
            if through == Through::Buffers || !is_buffer(dag, at) {
                stack.extend(&dag.nodes[at].children);
            }
        }
    }
    found
}

fn members(inside: &[bool]) -> Vec<usize> {
    (0..inside.len()).filter(|&at| inside[at]).collect()
}

#[cfg(test)]
mod tests;
