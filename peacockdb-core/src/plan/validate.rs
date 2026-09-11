//! Validation of a whole tree: each node against its children, then against itself.
//!
//! The two halves answer different questions. A node's own
//! [`validate_schemas_and_partitions`](GpuNode::validate_schemas_and_partitions) judges
//! what its children hand it and can name the node that fixes it. The structural rules
//! here judge what a node says about its *own* output — the ordinals in its layout and
//! its annotations — which no child can be blamed for and which no node-local check
//! looks at.

use datafusion::arrow::datatypes::Schema as ArrowSchema;

use super::GpuNode;
use super::PlanError;
use super::Schema;
use super::emitted_columns;
use super::{AggregateBody, GpuCrossJoin, GpuNestedLoopJoin, NodeRef, try_as_node_ref};
use super::{KeyDistribution, NodeKind, SortOrder};

/// Post-order, so a child's complaint comes before its parent's.
///
/// Public because the planner is not the only thing that builds a tree: a test that
/// rewrites a planned one into a shape no planner emits needs the same check the planner
/// ran, and the driver does not make it — [`check_canonical_form`] is all it asks for.
pub(crate) fn validate(root: &dyn GpuNode) -> Result<(), PlanError> {
    if !matches!(root.kind(), NodeKind::Sink) {
        return Err(PlanError::Invalid(format!(
            "{}: a plan ends at the crossing back to the host — the planner roots it in \
             GpuUnload",
            root.name()
        )));
    }
    check_canonical_form(root)?;
    walk(root)
}

/// Which lowering a limit got is a question about position, and a node cannot see what is
/// above it: root-adjacent means the interval belongs to `GpuUnload`, so a limit node whose
/// only consumer is the sink is a tree the planner does not emit.
fn limit_positions(node: &dyn GpuNode, parent_is_sink: bool) -> Result<(), PlanError> {
    if node.row_interval().is_some() && parent_is_sink {
        return Err(PlanError::Invalid(format!(
            "{}: a limit feeding only the sink is not a node — the planner puts its \
             skip/fetch on GpuUnload, so the driver can release the batches it does not want \
             instead of unloading them and throwing the rows away",
            node.name()
        )));
    }
    let is_sink = matches!(node.kind(), NodeKind::Sink);
    for child in node.children() {
        limit_positions(child, is_sink)?;
    }
    Ok(())
}

/// The canonical-form rules a driver needs to have been applied before it runs, so a mock
/// plan meets the same refusal a planned one would.
pub(crate) fn check_canonical_form(root: &dyn GpuNode) -> Result<(), PlanError> {
    limit_positions(root, false)
}

fn walk(node: &dyn GpuNode) -> Result<(), PlanError> {
    for child in node.children() {
        walk(child)?;
    }
    node.validate_schemas_and_partitions()?;
    structural(node)?;
    declared_width(node)?;
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
        NodeRef::CrossJoin(_) | NodeRef::NestedLoopJoin(_) => (
            width_of(node.children()[0]) + width_of(node.children()[1]),
            "its two sides",
        ),
        NodeRef::Aggregate(aggregate) => aggregate_width(&aggregate.body),
        NodeRef::AggregateBatches(aggregate) => aggregate_width(&aggregate.body),
        // The rest emit their input's columns, which `types_across_the_edge` compares
        // field for field — a stronger statement than a count.
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
/// schemas must be the same fields and not merely the same count. Types are the half no
/// name check and no per-node byte count can see: both engines derive their bytes from
/// this same declaration, so a column that changed type across an edge costs the same on
/// either and surfaces only in the answer.
///
/// It covers the nodes that CARRY a column. Deriving the type a computed column would
/// actually have — a project's expression, an aggregate's state — and checking the
/// declaration against it is [#163](../../../llm-wiki/tickets.md).
fn types_across_the_edge(node: &dyn GpuNode) -> Result<(), PlanError> {
    let carried: &dyn GpuNode = match try_as_node_ref(node) {
        Some(
            NodeRef::Sort(_)
            | NodeRef::CoalesceAllBatches(_)
            | NodeRef::AccumulateBatchesAndSort(_)
            | NodeRef::Limit(_)
            | NodeRef::MergePartitions(_)
            | NodeRef::EmitPartitions(_)
            | NodeRef::MergeSortedPartitions(_),
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

fn structural(node: &dyn GpuNode) -> Result<(), PlanError> {
    let name = node.name();
    let children = node.children();
    for child in &children {
        if matches!(child.kind(), NodeKind::Sink) {
            return Err(PlanError::Invalid(format!(
                "{name}: its input is a {}, whose output has already crossed to the host",
                child.name()
            )));
        }
    }
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
