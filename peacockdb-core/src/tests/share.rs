//! Turning several planned trees into one DAG by hand, the way a fuser would: every load that
//! reads the same thing becomes one shared source, and a hold buffer goes on every edge that
//! needs one. The planner builds no DAG, so tests do.

use std::collections::HashMap;
use std::num::NonZeroUsize;

use datafusion::execution::context::SessionContext;

use crate::plan::{GpuLoadParquet, GpuNode, NodeRef, Schema, SharedSource, as_node_ref};
use crate::plan::{PlanDag, PlanError, new_hold_buffer, validate_dag};
use crate::planner;
use crate::test_support::{Mode, data_dir_for};
use crate::tests::rebuild::{rebuild, schema_of};

/// Four tpch queries to fuse. Wherever two read one table they read the same columns, so
/// their loads match and share: lineitem as `{l_orderkey, l_quantity}`, orders as
/// `{o_orderkey, o_orderpriority}`.
pub(crate) const LINEITEM_TOTALS: &str = "SELECT sum(l_quantity), count(l_orderkey) FROM lineitem";
/// Grouped on a bare column: the device refuses a computed group key
/// ([#264](../../../llm-wiki/tickets/complete-coverage.md#t264)).
pub(crate) const LINEITEM_BY_QUANTITY: &str =
    "SELECT l_quantity, count(l_orderkey) FROM lineitem GROUP BY l_quantity";
pub(crate) const QUANTITY_BY_PRIORITY: &str = "SELECT o_orderpriority, sum(l_quantity) \
     FROM orders JOIN lineitem ON l_orderkey = o_orderkey GROUP BY o_orderpriority";
pub(crate) const ORDERS_BY_PRIORITY: &str =
    "SELECT o_orderpriority, count(o_orderkey) FROM orders GROUP BY o_orderpriority";

/// Each of `queries` over `dataset` at sf1, planned at `mode` in one session, which comes back
/// with the trees: the cpu backend runs against its task context.
pub(crate) async fn planned(
    dataset: &str,
    queries: &[&str],
    mode: &Mode,
) -> (SessionContext, Vec<Box<dyn GpuNode>>) {
    let ctx = crate::register_tables_for(
        crate::build_session_state(mode.target_partitions),
        &data_dir_for(dataset, "1"),
    )
    .await
    .expect("register the tables");
    let mut trees = Vec::new();
    for sql in queries {
        let what = format!("{sql} at {}", mode.name);
        let plan = ctx
            .sql(sql)
            .await
            .unwrap_or_else(|why| panic!("{what}: {why}"))
            .create_physical_plan()
            .await
            .unwrap_or_else(|why| panic!("{what}: {why}"));
        let (tree, _memory) = planner::plan(&plan, mode.knobs_for(dataset))
            .unwrap_or_else(|why| panic!("{what}: {why}"));
        trees.push(tree);
    }
    (ctx, trees)
}

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

/// `roots` with a `GpuHoldBuffer` on every edge `validate_dag` refuses as an unbuffered hold,
/// one refusal at a time, and how many it inserted. Any other refusal is a panic naming it:
/// the plan was wrong before any buffer could help.
pub(crate) fn buffer_unbuffered_holds(
    mut roots: Vec<Box<dyn GpuNode>>,
    cap: Option<NonZeroUsize>,
) -> (Vec<Box<dyn GpuNode>>, usize) {
    let mut inserted = 0;
    loop {
        let refs: Vec<&dyn GpuNode> = roots.iter().map(|root| root.as_ref()).collect();
        match validate_dag(&refs, cap) {
            Ok(()) => return (roots, inserted),
            Err(PlanError::UnbufferedHold { reader, slot, .. }) => {
                roots = with_buffer_on(&refs, (reader, slot));
                inserted += 1;
            }
            Err(other) => panic!("refused for a reason no buffer answers: {other}"),
        }
    }
}

/// The DAG rebuilt with a buffer on the edge `(reader, slot)`, indexed as `PlanDag::of(roots)`
/// numbers it. Each shared source is rebuilt once and shared again by every reader.
fn with_buffer_on(roots: &[&dyn GpuNode], edge: (usize, usize)) -> Vec<Box<dyn GpuNode>> {
    let dag = PlanDag::of(roots);
    let mut shared: HashMap<usize, SharedSource> = HashMap::new();
    dag.roots
        .iter()
        .map(|root| rebuilt_at(&dag, *root, edge, &mut shared))
        .collect()
}

fn rebuilt_at(
    dag: &PlanDag<'_>,
    at: usize,
    edge: (usize, usize),
    shared: &mut HashMap<usize, SharedSource>,
) -> Box<dyn GpuNode> {
    let node = dag.nodes[at].node;
    let children = dag.nodes[at]
        .children
        .iter()
        .enumerate()
        .map(|(slot, child)| {
            let child = rebuilt_at(dag, *child, edge, shared);
            match (at, slot) == edge {
                true => Box::new(new_hold_buffer(child)) as Box<dyn GpuNode>,
                false => child,
            }
        })
        .collect();
    let rebuilt = rebuild(node, children);
    let Some(key) = node.sharing_key() else {
        return rebuilt;
    };
    let source = shared.entry(key).or_insert_with(|| {
        let load = rebuilt
            .into_any_box()
            .downcast::<GpuLoadParquet>()
            .expect("only a load is shared");
        SharedSource::new(*load)
    });
    Box::new(source.clone())
}
