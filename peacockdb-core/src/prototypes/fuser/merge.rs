//! The mergeability hint: identical subtrees above the shared sources, across queries, and the
//! time merging them would save by the baseline's per-node times. The model is `merge_hint`'s.

#[cfg(test)]
mod tests;

use std::collections::{BTreeMap, HashMap};

use crate::plan::{GpuNode, NodeRef, PlanDag, as_node_ref};
use crate::plan_text::node_line_of;

use super::{Baseline, Fused, MergeEntry, MergeHint};

pub(crate) fn merge_hint(fused: &Fused, baseline: &Baseline) -> MergeHint {
    let roots: Vec<&dyn GpuNode> = fused.roots.iter().map(|root| root.as_ref()).collect();
    let dag = PlanDag::of(&roots);
    let keys = keys_of(&dag);
    let own = |at: usize| match fused.origins[at].as_slice() {
        [origin] if !is_load(dag.nodes[at].node) => {
            baseline.per_query[origin.query].total_us[origin.node]
        }
        _ => 0,
    };
    let groups = groups_of(&dag, fused, &keys);
    let group_of: HashMap<usize, usize> = groups
        .iter()
        .enumerate()
        .flat_map(|(at, members)| members.iter().map(move |member| (*member, at)))
        .collect();
    let saved = |times: &[u64]| times.iter().sum::<u64>() - times.iter().max().unwrap_or(&0);
    let mut entries = Vec::new();
    let mut total = 0;
    for (at, members) in groups.iter().enumerate() {
        total += saved(&members.iter().map(|m| own(*m)).collect::<Vec<u64>>());
        let parents: Vec<Option<&usize>> = members
            .iter()
            .map(|m| parent_of(&dag, *m).and_then(|p| group_of.get(&p)))
            .collect();
        let contained = parents[0].is_some_and(|outer| {
            *outer != at
                && groups[*outer].len() == members.len()
                && parents.iter().all(|p| *p == Some(outer))
        });
        if contained {
            continue;
        }
        let times: Vec<u64> = members.iter().map(|m| subtree_us(&dag, *m, &own)).collect();
        entries.push(MergeEntry {
            node: node_line_of(dag.nodes[members[0]].node),
            members: members.iter().map(|m| fused.origins[*m][0]).collect(),
            saved_us: saved(&times),
        });
    }
    entries.sort_by(|a, b| b.saved_us.cmp(&a.saved_us));
    MergeHint {
        entries,
        saved_us: total,
    }
}

/// Per node, by `PlanDag` index: a key two nodes share exactly when they compute alike over
/// alike inputs, and whether the node's subtree reaches a shared source. A key is its node's
/// line and its children's keys, interned; a shared source's is its sharing key, and a buffer's
/// is its child's, since it computes nothing.
fn keys_of(dag: &PlanDag<'_>) -> Vec<(usize, bool)> {
    let mut interned: HashMap<(String, Vec<usize>), usize> = HashMap::new();
    let mut keys = vec![(0, false); dag.nodes.len()];
    for at in dag.in_post_order() {
        let (node, children) = (dag.nodes[at].node, &dag.nodes[at].children);
        let reaches = node.sharing_key().is_some() || children.iter().any(|c| keys[*c].1);
        let line = match node.sharing_key() {
            Some(key) => format!("shared {key}"),
            None if dag.is_buffer(at) => {
                keys[at] = keys[children[0]];
                continue;
            }
            None => node_line_of(node),
        };
        let next = interned.len();
        let child_keys = children.iter().map(|c| keys[*c].0).collect();
        keys[at] = (*interned.entry((line, child_keys)).or_insert(next), reaches);
    }
    keys
}

/// The groups: two or more nodes of one key above a shared source, from two or more queries,
/// each standing for a node of a query's own tree, its root aside. Ordered by first member,
/// members in index order.
fn groups_of(dag: &PlanDag<'_>, fused: &Fused, keys: &[(usize, bool)]) -> Vec<Vec<usize>> {
    let mut by_key: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for (at, (key, reaches)) in keys.iter().enumerate() {
        let node = dag.nodes[at].node;
        let candidate = *reaches
            && node.sharing_key().is_none()
            && !dag.is_buffer(at)
            && !fused.origins[at].is_empty()
            && !matches!(as_node_ref(node), NodeRef::Unload(_));
        if candidate {
            by_key.entry(*key).or_default().push(at);
        }
    }
    let across = |members: &Vec<usize>| {
        let query = |at: &usize| fused.origins[*at][0].query;
        members.iter().any(|m| query(m) != query(&members[0]))
    };
    let mut groups: Vec<Vec<usize>> = by_key.into_values().filter(across).collect();
    groups.sort_by_key(|members| members[0]);
    groups
}

/// The node reading `at`, through any buffer: a node above a shared source has one reader.
fn parent_of(dag: &PlanDag<'_>, at: usize) -> Option<usize> {
    let mut parent = dag.nodes[at].readers.first()?.parent;
    while dag.is_buffer(parent) {
        parent = dag.nodes[parent].readers.first()?.parent;
    }
    Some(parent)
}

/// Σ `own` over the subtree of `at`, each node once.
fn subtree_us(dag: &PlanDag<'_>, at: usize, own: &impl Fn(usize) -> u64) -> u64 {
    let mut seen = vec![false; dag.nodes.len()];
    let mut stack = vec![at];
    let mut sum = 0;
    while let Some(next) = stack.pop() {
        if std::mem::replace(&mut seen[next], true) {
            continue;
        }
        sum += own(next);
        stack.extend(&dag.nodes[next].children);
    }
    sum
}

fn is_load(node: &dyn GpuNode) -> bool {
    matches!(as_node_ref(node), NodeRef::LoadParquet(_))
}
