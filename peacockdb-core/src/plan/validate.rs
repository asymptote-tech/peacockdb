//! Validation of a whole plan: each node against its children, then against itself, once
//! however many parents read it.
//!
//! The two halves answer different questions. A node's own
//! [`validate_schemas_and_partitions`](GpuNode::validate_schemas_and_partitions) judges
//! what its children hand it and can name the node that fixes it. The structural rules
//! here judge what a node says about its *own* output — the ordinals in its layout and
//! its annotations — which no child can be blamed for and which no node-local check
//! looks at.

use std::num::NonZeroUsize;

use datafusion::arrow::datatypes::Schema as ArrowSchema;

use super::GpuNode;
use super::PlanDag;
use super::PlanError;
use super::Schema;
use super::emitted_columns;
use super::unholdable;
use super::{AggregateBody, GpuCrossJoin, GpuNestedLoopJoin, NodeRef, try_as_node_ref};
use super::{KeyDistribution, NodeKind, SortOrder};

/// The structure first, then the canonical form, then every node in post-order, so a
/// child's complaint comes before its parent's.
pub(crate) fn validate_roots(roots: &[&dyn GpuNode]) -> Result<(), PlanError> {
    let dag = PlanDag::of(roots);
    roots_are_distinct(&dag)?;
    for &root in &dag.roots {
        let root = dag.nodes[root].node;
        if !is_sink(root) {
            return Err(PlanError::Invalid(format!(
                "{}: a plan ends at the crossing back to the host — the planner roots it in \
                 GpuUnload",
                root.name()
            )));
        }
    }
    sinks_are_roots(&dag)?;
    limit_positions(&dag)?;
    walk(&dag)?;
    holds_are_buffered(&dag)
}

/// [`validate_roots`], then the cap's own buffer rule, then the wait-for graph between holds,
/// which must have no cycle. Under the buffer rule none can form; the check is that argument
/// made mechanical, and the first thing a later change to the rule breaks.
pub(crate) fn validate_dag(
    roots: &[&dyn GpuNode],
    cap: Option<NonZeroUsize>,
) -> Result<(), PlanError> {
    validate_roots(roots)?;
    let dag = PlanDag::of(roots);
    capped_roots_are_buffered(&dag, cap)?;
    capped_roots_keep_their_state(&dag, cap)?;
    match first_cycle(&wait_for_edges(&dag, cap)) {
        Some(holds) => Err(hold_cycle(&dag, holds)),
        None => Ok(()),
    }
}

pub(crate) fn validate_cap(
    roots: &[&dyn GpuNode],
    cap: Option<NonZeroUsize>,
) -> Result<(), PlanError> {
    let dag = PlanDag::of(roots);
    capped_roots_are_buffered(&dag, cap)?;
    capped_roots_keep_their_state(&dag, cap)
}

/// The roots of ordinal `cap` and above, the ones that can wait for admission, each with the
/// region the cap holds: the root's, stopping at buffers, since the cap lifts.
fn waiting_roots(dag: &PlanDag<'_>, cap: Option<NonZeroUsize>) -> Vec<(usize, Vec<usize>)> {
    let admitted = cap.map_or(dag.roots.len(), NonZeroUsize::get);
    dag.roots
        .iter()
        .skip(admitted)
        .map(|root| (*root, dag.held_region_of_node(*root)))
        .collect()
}

/// "The only unbounded storage is a node you can see" (`architecture.md`, "The scheduling
/// rule") over the cap's holds: a waiting root's region is entered from outside only through
/// a buffer, so a shared source it reads never waits on its admission.
fn capped_roots_are_buffered(
    dag: &PlanDag<'_>,
    cap: Option<NonZeroUsize>,
) -> Result<(), PlanError> {
    for (root, region) in waiting_roots(dag, cap) {
        if let Some((reader, slot, producer)) = unbuffered_entry(dag, &region, None) {
            let name = |at: usize| dag.nodes[at].node.name();
            return Err(PlanError::UnbufferedHold {
                hold: root,
                reader,
                slot,
                producer,
                what: format!(
                    "{} (node {reader}) reads {} (node {producer}) from outside the root {} \
                     (node {root}), which waits for the cap; a GpuHoldBuffer must sit on that \
                     edge",
                    name(reader),
                    name(producer),
                    name(root)
                ),
            });
        }
    }
    Ok(())
}

/// The cap holds a waiting root only down to its buffers, so whatever lies below one runs
/// while the root waits. A stateless node there is a batch in and a batch out, kept by the
/// buffer above it; an accumulator would hold its state outside the cap, which exists to
/// bound the state alive at once.
fn capped_roots_keep_their_state(
    dag: &PlanDag<'_>,
    cap: Option<NonZeroUsize>,
) -> Result<(), PlanError> {
    for (root, held) in waiting_roots(dag, cap) {
        let mut below_a_buffer = dag
            .region_of_node(root)
            .into_iter()
            .filter(|node| !held.contains(node));
        if let Some(node) = below_a_buffer.find(|node| holds_state(dag.nodes[*node].node)) {
            let name = |at: usize| dag.nodes[at].node.name();
            return Err(PlanError::UncappedAccumulator {
                root,
                node,
                what: format!(
                    "{} (node {node}) sits below a GpuHoldBuffer of the root {} (node {root}), \
                     which waits for the cap, so its state would live outside the cap; only \
                     stateless nodes may sit between a waiting root's buffer and its source",
                    name(node),
                    name(root)
                ),
            });
        }
    }
    Ok(())
}

/// The accumulators: the kinds whose state grows with what they are fed. These are the
/// memory estimator's (`held_by_accumulator`), the buffer aside, which is the cap's own answer.
pub(crate) fn holds_state(node: &dyn GpuNode) -> bool {
    matches!(
        try_as_node_ref(node),
        Some(
            NodeRef::CoalesceAllBatches(_)
                | NodeRef::MergeSortedPartitions(_)
                | NodeRef::AccumulateBatchesAndSort(_)
                | NodeRef::AggregateBatches(_)
                | NodeRef::Join(_)
                | NodeRef::NestedLoopJoin(_)
                | NodeRef::CrossJoin(_)
        )
    )
}

/// A join's probe is its second child, which is the side its hold covers.
const PROBE_CHILD: usize = 1;

fn joins<'d>(dag: &'d PlanDag<'_>) -> impl Iterator<Item = usize> + 'd {
    (0..dag.nodes.len()).filter(|at| {
        matches!(
            try_as_node_ref(dag.nodes[*at].node),
            Some(NodeRef::Join(_) | NodeRef::CrossJoin(_) | NodeRef::NestedLoopJoin(_))
        )
    })
}

/// No edge enters a join's held probe region from a producer outside it unless a
/// `GpuHoldBuffer` reads that edge. A held buffer takes every batch it is offered, so it is
/// the one reader whose producer never waits on the hold. On a tree nothing enters a region
/// but through a buffer, so no single-query plan is refused here.
fn holds_are_buffered(dag: &PlanDag<'_>) -> Result<(), PlanError> {
    for join in joins(dag) {
        let region = dag.region_of_edge(join, PROBE_CHILD);
        if let Some((reader, slot, producer)) =
            unbuffered_entry(dag, &region, Some((join, PROBE_CHILD)))
        {
            let name = |at: usize| dag.nodes[at].node.name();
            return Err(PlanError::UnbufferedHold {
                hold: join,
                reader,
                slot,
                producer,
                what: format!(
                    "{} (node {reader}) reads {} (node {producer}) from outside the hold of {} \
                     (node {join}); a GpuHoldBuffer must sit on that edge",
                    name(reader),
                    name(producer),
                    name(join)
                ),
            });
        }
    }
    Ok(())
}

/// The first edge, as `(reader, slot, producer)` in pre-order, that carries batches into
/// `region` from outside it with no buffer reading it: an edge out of a member, or the
/// `held_edge` itself, to a producer that is not a member.
fn unbuffered_entry(
    dag: &PlanDag<'_>,
    region: &[usize],
    held_edge: Option<(usize, usize)>,
) -> Option<(usize, usize, usize)> {
    let mut inside = vec![false; dag.nodes.len()];
    for member in region {
        inside[*member] = true;
    }
    for (reader, node) in dag.nodes.iter().enumerate() {
        for (slot, &producer) in node.children.iter().enumerate() {
            let enters = inside[reader] || Some((reader, slot)) == held_edge;
            if enters && !inside[producer] && !dag.is_buffer(reader) {
                return Some((reader, slot, producer));
            }
        }
    }
    None
}

/// `(waiting, held)`, each vertex a hold named by its node: a join's by the join, the cap's
/// by the root it keeps waiting. A join in its build phase waits on every node its build side
/// reads, and so on each hold whose region holds one of them — never on its own root's cap,
/// which lifted before any of its steps. A waiting root waits on the roots before it
/// finishing, so on each hold whose region holds a node of theirs. Holds stop at buffers, so
/// a buffer below a build is what keeps that build from waiting.
fn wait_for_edges(dag: &PlanDag<'_>, cap: Option<NonZeroUsize>) -> Vec<(usize, usize)> {
    let waiting = waiting_roots(dag, cap);
    let held: Vec<(usize, Vec<usize>)> = joins(dag)
        .map(|join| (join, dag.region_of_edge(join, PROBE_CHILD)))
        .chain(waiting.iter().cloned())
        .collect();
    let edges_from = |from: usize, waits_on: &[bool], own_cap: Option<usize>| {
        held.iter()
            .filter(|(hold, region)| {
                Some(*hold) != own_cap && region.iter().any(|member| waits_on[*member])
            })
            .map(|(hold, _)| (from, *hold))
            .collect::<Vec<_>>()
    };
    let mut edges = Vec::new();
    for join in joins(dag) {
        let waits_on = reached_from(dag, dag.nodes[join].children[0]);
        let own_cap = waiting
            .iter()
            .find(|(_, region)| region.contains(&join))
            .map(|(root, _)| *root);
        edges.extend(edges_from(join, &waits_on, own_cap));
    }
    for (root, _) in &waiting {
        let mut waits_on = vec![false; dag.nodes.len()];
        for before in dag.roots.iter().take_while(|earlier| *earlier != root) {
            for (at, reached) in reached_from(dag, *before).into_iter().enumerate() {
                waits_on[at] |= reached;
            }
        }
        edges.extend(edges_from(*root, &waits_on, Some(*root)));
    }
    edges
}

/// `node` and everything it reads, through buffers too.
fn reached_from(dag: &PlanDag<'_>, node: usize) -> Vec<bool> {
    let mut seen = vec![false; dag.nodes.len()];
    let mut stack = vec![node];
    while let Some(at) = stack.pop() {
        if !std::mem::replace(&mut seen[at], true) {
            stack.extend(&dag.nodes[at].children);
        }
    }
    seen
}

/// The first cycle a depth-first search finds, from the lowest vertex and along the lowest
/// edge first, in the order its vertices are visited.
fn first_cycle(edges: &[(usize, usize)]) -> Option<Vec<usize>> {
    let mut vertices: Vec<usize> = edges.iter().flat_map(|(from, to)| [*from, *to]).collect();
    vertices.sort_unstable();
    vertices.dedup();
    let mut sorted = edges.to_vec();
    sorted.sort_unstable();
    let mut done = Vec::new();
    for start in &vertices {
        if !done.contains(start) {
            let mut path = Vec::new();
            if let Some(cycle) = search(*start, &sorted, &mut path, &mut done) {
                return Some(cycle);
            }
        }
    }
    None
}

fn search(
    at: usize,
    edges: &[(usize, usize)],
    path: &mut Vec<usize>,
    done: &mut Vec<usize>,
) -> Option<Vec<usize>> {
    if let Some(start) = path.iter().position(|on| *on == at) {
        return Some(path[start..].to_vec());
    }
    if done.contains(&at) {
        return None;
    }
    path.push(at);
    for (_, next) in edges.iter().filter(|(from, _)| *from == at) {
        if let Some(cycle) = search(*next, edges, path, done) {
            return Some(cycle);
        }
    }
    path.pop();
    done.push(at);
    None
}

fn hold_cycle(dag: &PlanDag<'_>, holds: Vec<usize>) -> PlanError {
    let names: Vec<String> = holds
        .iter()
        .map(|at| format!("{} (node {at})", dag.nodes[*at].node.name()))
        .collect();
    PlanError::HoldCycle {
        what: format!("holds wait on each other: {}", names.join(" → ")),
        holds,
    }
}

/// A root answers once, so one given twice would answer the same rows under two ordinals.
fn roots_are_distinct(dag: &PlanDag<'_>) -> Result<(), PlanError> {
    for (ordinal, root) in dag.roots.iter().enumerate() {
        if let Some(first) = dag.roots[..ordinal].iter().position(|seen| seen == root) {
            return Err(PlanError::Invalid(format!(
                "{}: root {ordinal} is root {first} given again — each root is its own query",
                dag.nodes[*root].node.name()
            )));
        }
    }
    Ok(())
}

/// A sink's output has already crossed to the host, so nothing on the device can read it.
fn sinks_are_roots(dag: &PlanDag<'_>) -> Result<(), PlanError> {
    match dag
        .nodes
        .iter()
        .find(|node| !node.readers.is_empty() && is_sink(node.node))
    {
        Some(sink) => Err(PlanError::Invalid(format!(
            "{}: a sink feeds another node — every GpuUnload is a root",
            sink.node.name()
        ))),
        None => Ok(()),
    }
}

fn is_sink(node: &dyn GpuNode) -> bool {
    matches!(node.kind(), NodeKind::Sink)
}

/// Which lowering a limit got is a question about position, and a node cannot see what is
/// above it: root-adjacent means the interval belongs to `GpuUnload`, so a limit node whose
/// only consumer is the sink is a plan the planner does not emit. Pre-order, as the tree
/// walk was, so the highest such limit is the one named.
fn limit_positions(dag: &PlanDag<'_>) -> Result<(), PlanError> {
    for node in &dag.nodes {
        let only_a_sink_reads_it = !node.readers.is_empty()
            && node
                .readers
                .iter()
                .all(|reader| is_sink(dag.nodes[reader.parent].node));
        if node.node.row_interval().is_some() && only_a_sink_reads_it {
            return Err(PlanError::Invalid(format!(
                "{}: a limit feeding only the sink is not a node — the planner puts its \
                 skip/fetch on GpuUnload, so the driver can release the batches it does not \
                 want instead of unloading them and throwing the rows away",
                node.node.name()
            )));
        }
    }
    Ok(())
}

/// The canonical-form rules a driver needs to have been applied before it runs, so a mock
/// plan meets the same refusal a planned one would.
pub(crate) fn check_canonical_form_of_roots(roots: &[&dyn GpuNode]) -> Result<(), PlanError> {
    limit_positions(&PlanDag::of(roots))
}

/// The order `walk` checks nodes in: post-order, a shared node once.
fn walk_order<'a, 'd>(dag: &'d PlanDag<'a>) -> impl Iterator<Item = &'a dyn GpuNode> + 'd {
    dag.in_post_order().into_iter().map(|at| dag.nodes[at].node)
}

fn walk(dag: &PlanDag<'_>) -> Result<(), PlanError> {
    walk_order(dag).try_for_each(check_node)
}

fn check_node(node: &dyn GpuNode) -> Result<(), PlanError> {
    node.validate_schemas_and_partitions()?;
    declares_a_column(node)?;
    structural(node)?;
    declared_width(node)?;
    device_holdable_types(node)?;
    types_across_the_edge(node)?;
    earned_claims(node)
}

/// Where a declaration must have come from. A hash is made by exactly one node and an
/// order by three, so a claim with no such node beneath it was minted rather than carried
/// — the shape I4 was: a join deriving `ByHash` from its keys and its column names, over
/// lanes nothing had scattered.
///
/// Subtree presence, not a path: which nodes may carry a claim past themselves is a rule
/// each node states about its own output, and this is the cross-check that the chain
/// starts somewhere real.
fn earned_claims(node: &dyn GpuNode) -> Result<(), PlanError> {
    let Some(layout) = node.kind().layout() else {
        return Ok(());
    };
    let name = node.name();
    if matches!(layout.key_distribution, KeyDistribution::ByHash { .. })
        && !below(node, &|found| {
            matches!(try_as_node_ref(found), Some(NodeRef::EmitPartitions(_)))
        })
    {
        return Err(PlanError::Invalid(format!(
            "{name}: it declares rows placed by a hash, and nothing below it scattered them \
             — only GpuEmitPartitions does"
        )));
    }
    if layout.sort_order.is_batch_sorted()
        && !below(node, &|found| {
            matches!(
                try_as_node_ref(found),
                Some(
                    NodeRef::Sort(_)
                        | NodeRef::AccumulateBatchesAndSort(_)
                        | NodeRef::MergeSortedPartitions(_)
                )
            )
        })
    {
        return Err(PlanError::Invalid(format!(
            "{name}: it declares sorted batches, and nothing below it sorted any"
        )));
    }
    Ok(())
}

/// The node itself or anything under it — a sort declares its own order, and a scatter its
/// own hash.
fn below(node: &dyn GpuNode, accept: &dyn Fn(&dyn GpuNode) -> bool) -> bool {
    accept(node) || node.children().iter().any(|child| below(*child, accept))
}

/// A node's declared columns are ones the device can hold. The scan is where a view type
/// entered a plan, and the option that made it do so is off; a schema that still names one,
/// or a decimal of another width, was minted by something this layer does not know about. An
/// aggregate declares a second schema, its intermediate table, which its finalize reads and
/// no `kind()` exposes.
fn device_holdable_types(node: &dyn GpuNode) -> Result<(), PlanError> {
    let Some(schema) = node.kind().schema() else {
        return Ok(());
    };
    device_holdable_columns(node.name(), "column", schema)?;
    match try_as_node_ref(node) {
        Some(NodeRef::Aggregate(aggregate)) => {
            device_holdable_columns(node.name(), "intermediate column", aggregate.intermediate())
        }
        Some(NodeRef::AggregateBatches(aggregate)) => {
            device_holdable_columns(node.name(), "intermediate column", aggregate.intermediate())
        }
        _ => Ok(()),
    }
}

fn device_holdable_columns(node: &str, what: &str, schema: &Schema) -> Result<(), PlanError> {
    for (at, field) in schema.fields.fields().iter().enumerate() {
        if let Some(why) = unholdable(field.data_type()) {
            return Err(PlanError::Invalid(format!(
                "{node}: {what} {at} {}: {} is {why}",
                field.name(),
                field.data_type()
            )));
        }
    }
    Ok(())
}

/// What the plan promised its caller. The layer's contract is that the rows it hands back
/// are the ones DataFusion planned, and nothing else states it: every node below is
/// checked against its own children, so a whole tree can be internally consistent and
/// answer a different query.
pub(crate) fn check_output_schema(
    root: &dyn GpuNode,
    planned: &ArrowSchema,
) -> Result<(), PlanError> {
    let emitted = root
        .children()
        .first()
        .and_then(|input| input.kind().schema())
        .expect("a sink has an input");
    if emitted.fields.fields().len() != planned.fields().len() {
        return Err(PlanError::Invalid(format!(
            "the plan emits {} columns and the query asked for {}",
            emitted.fields.fields().len(),
            planned.fields().len()
        )));
    }
    let mismatch = emitted
        .fields
        .fields()
        .iter()
        .zip(planned.fields().iter())
        .find(|(ours, theirs)| {
            ours.name() != theirs.name() || ours.data_type() != theirs.data_type()
        });
    if let Some((ours, theirs)) = mismatch {
        return Err(PlanError::Invalid(format!(
            "the plan emits {} as {:?} where the query asked for {} as {:?}",
            ours.name(),
            ours.data_type(),
            theirs.name(),
            theirs.data_type()
        )));
    }
    Ok(())
}

/// A device table's row count is its first column's, so a table of no columns reads as no
/// rows whatever it held. Where DataFusion plans rows and no column, the translator declares
/// `__rowmarker__` instead (#63); a node declaring nothing was built some other way.
fn declares_a_column(node: &dyn GpuNode) -> Result<(), PlanError> {
    match node.kind().schema() {
        Some(schema) if schema.fields.fields().is_empty() => Err(PlanError::Invalid(format!(
            "{}: it declares no column, which a device table reads as no rows — the planner \
             declares __rowmarker__ where DataFusion has rows and no column (#63)",
            node.name()
        ))),
        _ => Ok(()),
    }
}

/// A node's declared column count against the parameters that produce those columns. The
/// two are written in different places — the schema comes from DataFusion, the parameters
/// from this layer's own rebasing — and a node emitting a different number of columns from
/// the one it declares shifts every ordinal above it.
fn declared_width(node: &dyn GpuNode) -> Result<(), PlanError> {
    let Some(schema) = node.kind().schema() else {
        return Ok(());
    };
    let declared = schema.fields.fields().len();
    let width_of = |input: &dyn GpuNode| {
        input
            .kind()
            .schema()
            .expect("a sink cannot be an input")
            .fields
            .fields()
            .len()
    };
    // A node the registry does not know is a hand-built one under test, and declares
    // none of the parameters this rule reads.
    let Some(kind) = try_as_node_ref(node) else {
        return Ok(());
    };
    let (emitted, from) = match kind {
        NodeRef::LoadParquet(load) if load.rows_only() => (1, "a read of rows alone"),
        NodeRef::LoadParquet(load) => (load.projection.len(), "the columns it reads"),
        NodeRef::Project(project) => (project.exprs.len(), "its expression list"),
        NodeRef::Filter(filter) => match &filter.projection {
            Some(columns) => (columns.len(), "its projection"),
            None => (width_of(node.children()[0]), "its input"),
        },
        NodeRef::Join(join) => match &join.projection {
            Some(columns) => (columns.len(), "its projection"),
            None => (
                emitted_columns(
                    join.join_type,
                    width_of(node.children()[0]),
                    width_of(node.children()[1]),
                ),
                "the sides its join type emits",
            ),
        },
        NodeRef::CrossJoin(GpuCrossJoin {
            projection: Some(columns),
            ..
        })
        | NodeRef::NestedLoopJoin(GpuNestedLoopJoin {
            projection: Some(columns),
            ..
        }) => (columns.len(), "its projection"),
        NodeRef::CrossJoin(_) => (
            width_of(node.children()[0]) + width_of(node.children()[1]),
            "its two sides",
        ),
        NodeRef::NestedLoopJoin(join) => (
            emitted_columns(
                join.join_type,
                width_of(node.children()[0]),
                width_of(node.children()[1]),
            ),
            "the sides its join type emits",
        ),
        NodeRef::Aggregate(aggregate) => aggregate_width(&aggregate.body),
        NodeRef::AggregateBatches(aggregate) => aggregate_width(&aggregate.body),
        // The rest emit their input's columns: the one-input kinds are checked by
        // `types_across_the_edge`, union and interleave by `union::check_branch_schemas`.
        _ => return Ok(()),
    };
    if declared != emitted {
        return Err(PlanError::Invalid(format!(
            "{}: it declares {declared} columns and {from} produces {emitted}",
            node.name()
        )));
    }
    Ok(())
}

/// Group keys, then what the node emits per aggregate: its state columns where it hands
/// state on, and one finalized column each where it finishes. A grouping-set expansion
/// emits `__grouping_id` beside the keys, which is a key everywhere above it.
fn aggregate_width(body: &AggregateBody) -> (usize, &'static str) {
    let keys = body.group_by.len() + usize::from(!body.grouping_sets.is_empty());
    match &body.finalize {
        Some(finalize) => (keys + finalize.len(), "its keys and its finalize list"),
        None => (
            keys + body
                .aggs
                .iter()
                .map(|call| call.outputs.len())
                .sum::<usize>(),
            "its keys and its state columns",
        ),
    }
}

/// A node that moves rows rather than changing them emits its input's columns, so the two
/// schemas must be the same fields: the count first, then the fields pairwise. Types are
/// the half no name check and no per-node byte count can see: both engines derive their
/// bytes from this same declaration, so a column that changed type across an edge costs
/// the same on either and surfaces only in the answer.
///
/// It covers the nodes that CARRY a column. An aggregate's state is derived from its
/// producer where the plan is built (`PlanAgg::state_type`); a project's expression is
/// compared against nothing.
fn types_across_the_edge(node: &dyn GpuNode) -> Result<(), PlanError> {
    let carried: &dyn GpuNode = match try_as_node_ref(node) {
        Some(
            NodeRef::Sort(_)
            | NodeRef::CoalesceAllBatches(_)
            | NodeRef::AccumulateBatchesAndSort(_)
            | NodeRef::Limit(_)
            | NodeRef::MergePartitions(_)
            | NodeRef::EmitPartitions(_)
            | NodeRef::MergeSortedPartitions(_)
            | NodeRef::HoldBuffer(_),
        ) => node.children()[0],
        // A filter that projects re-selects columns rather than carrying them; its
        // projection is checked by width above and by ordinal at the node.
        Some(NodeRef::Filter(filter)) if filter.projection.is_none() => node.children()[0],
        _ => return Ok(()),
    };
    let (ours, theirs) = (
        node.kind().schema().expect("not a sink"),
        carried.kind().schema().expect("a sink cannot be an input"),
    );
    // Before the zip, which stops at the shorter list and so says nothing about a node
    // that dropped or invented a column.
    let (declared, carried_width) = (ours.fields.fields().len(), theirs.fields.fields().len());
    if declared != carried_width {
        return Err(PlanError::Invalid(format!(
            "{}: it declares {declared} columns and its input produces {carried_width}",
            node.name()
        )));
    }
    if let Some((ours, theirs)) = ours
        .fields
        .fields()
        .iter()
        .zip(theirs.fields.fields().iter())
        .find(|(ours, theirs)| {
            ours.name() != theirs.name() || ours.data_type() != theirs.data_type()
        })
    {
        return Err(PlanError::Invalid(format!(
            "{}: it moves rows rather than changing them, and declares {} as {:?} where its \
             input holds {} as {:?}",
            node.name(),
            ours.name(),
            ours.data_type(),
            theirs.name(),
            theirs.data_type()
        )));
    }
    Ok(())
}

/// A sink below a root is refused before the walk, by `sinks_are_roots`.
fn structural(node: &dyn GpuNode) -> Result<(), PlanError> {
    let name = node.name();
    let children = node.children();
    match (node.kind(), children.is_empty()) {
        (NodeKind::Source { .. }, false) => {
            return Err(PlanError::Invalid(format!(
                "{name}: a source reads a table, so it takes no input"
            )));
        }
        (NodeKind::Intermediate { .. }, true) => {
            return Err(PlanError::Invalid(format!(
                "{name}: nothing produces the rows it declares — only a source has no input"
            )));
        }
        _ => {}
    }

    let (Some(layout), Some(schema)) = (node.kind().layout(), node.kind().schema()) else {
        return Ok(());
    };
    if layout.n == 0 {
        return Err(PlanError::Invalid(format!(
            "{name}: no lanes, so it declares rows nothing can read"
        )));
    }
    let columns = schema.fields.fields().len();
    let in_range = |ordinal: u32, what: &str| -> Result<(), PlanError> {
        if ordinal as usize >= columns {
            return Err(PlanError::Invalid(format!(
                "{name}: {what} @{ordinal} is past the {columns} columns it emits"
            )));
        }
        Ok(())
    };

    if let KeyDistribution::ByHash { hash_keys } = &layout.key_distribution {
        for key in hash_keys {
            in_range(*key, "hash key")?;
        }
    }
    if let SortOrder::BatchSorted { columns } = &layout.sort_order {
        for order in columns {
            in_range(order.column, "sort key")?;
        }
    }
    for key in &schema.group_keys {
        in_range(*key, "group key")?;
    }
    annotated_state(name, schema, &in_range)
}

/// An aggregate state annotation names the columns it decomposed into, so those columns
/// have to be there: a merge reads the positions rather than re-deriving them, and a
/// position past the output would read whatever the next node emits at that ordinal.
fn annotated_state(
    name: &str,
    schema: &Schema,
    in_range: &dyn Fn(u32, &str) -> Result<(), PlanError>,
) -> Result<(), PlanError> {
    for state in &schema.agg_state {
        if state.positions.is_empty() {
            return Err(PlanError::Invalid(format!(
                "{name}: {} declares aggregate state in no column",
                state.output
            )));
        }
        for position in &state.positions {
            in_range(*position, &format!("{} state", state.output))?;
        }
        if state
            .positions
            .iter()
            .any(|position| schema.group_keys.contains(position))
        {
            return Err(PlanError::Invalid(format!(
                "{name}: {} declares its state in a column that is also a group key",
                state.output
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
