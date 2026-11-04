//! Turning several planned trees into one DAG by hand, the way a fuser would: every load that
//! reads the same thing becomes one shared source. The planner builds no DAG, so tests do.

use std::collections::HashMap;

use crate::plan::{GpuLoadParquet, GpuNode, NodeRef, Schema, SharedSource, as_node_ref};
use crate::tests::rebuild::{rebuild, schema_of};

/// What two loads must agree on to be read once: the table, the columns, the mapping and the
/// schema they declare.
type LoadKey = (String, Vec<u32>, Vec<Vec<Vec<u32>>>, Schema);

/// `roots` rebuilt, each `GpuLoadParquet` that equals another anywhere in them replaced by a
/// clone of one `SharedSource`; and how many sources ended up shared. A load nothing else
/// equals is rebuilt as it was.
pub(crate) fn share_sources(roots: Vec<Box<dyn GpuNode>>) -> (Vec<Box<dyn GpuNode>>, usize) {
    let mut counts: Vec<(LoadKey, usize)> = Vec::new();
    for root in &roots {
        count_loads(root.as_ref(), &mut counts);
    }
    let mut shared: HashMap<usize, SharedSource> = HashMap::new();
    let rebuilt = roots
        .iter()
        .map(|root| share(root.as_ref(), &counts, &mut shared))
        .collect();
    (rebuilt, shared.len())
}

fn key_of(node: &dyn GpuNode) -> Option<LoadKey> {
    match as_node_ref(node) {
        NodeRef::LoadParquet(load) => Some((
            load.table.clone(),
            load.projection.clone(),
            load.partition_groups.clone(),
            schema_of(node),
        )),
        _ => None,
    }
}

fn count_loads(node: &dyn GpuNode, counts: &mut Vec<(LoadKey, usize)>) {
    if let Some(key) = key_of(node) {
        match counts.iter_mut().find(|(seen, _)| *seen == key) {
            Some((_, count)) => *count += 1,
            None => counts.push((key, 1)),
        }
    }
    for child in node.children() {
        count_loads(child, counts);
    }
}

/// `shared` is keyed by the load's position in `counts`, since a `Schema` does not hash.
fn share(
    node: &dyn GpuNode,
    counts: &[(LoadKey, usize)],
    shared: &mut HashMap<usize, SharedSource>,
) -> Box<dyn GpuNode> {
    let children = node
        .children()
        .into_iter()
        .map(|child| share(child, counts, shared))
        .collect();
    let rebuilt = rebuild(node, children);
    let Some(key) = key_of(node) else {
        return rebuilt;
    };
    let at = counts
        .iter()
        .position(|(seen, _)| *seen == key)
        .expect("every load was counted");
    if counts[at].1 < 2 {
        return rebuilt;
    }
    let source = shared.entry(at).or_insert_with(|| {
        let load = rebuilt
            .into_any_box()
            .downcast::<GpuLoadParquet>()
            .expect("a load rebuilds as a load");
        SharedSource::new(*load)
    });
    Box::new(source.clone())
}
