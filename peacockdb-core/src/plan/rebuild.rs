//! Rebuilding a plan node over new children: what an injected test rewrite and the fuser
//! both do to a planned tree.
//!
//! Rebuild rather than edit: a node's partitioning is not a field but a value its constructor
//! computed from the child it was handed, so a node with a new child has to be built again.
//! The exhaustive match makes a new kind a compile error here; `src/tests/rebuild.rs` holds the
//! fixtures that prove every arm keeps every field.

use super::GpuNode;
use super::ScanMetadata;
use super::Schema;
use super::{
    AggregateBody, GpuAccumulateBatchesAndSort, GpuAggregate, GpuAggregateBatches,
    GpuCoalesceAllBatches, GpuCrossJoin, GpuEmitPartitions, GpuEmpty, GpuFilter, GpuHashJoin,
    GpuInterleave, GpuLimit, GpuLoadParquet, GpuMergePartitions, GpuMergeSortedPartitions,
    GpuNestedLoopJoin, GpuProject, GpuSort, GpuUnion, GpuUnload, NodeRef, as_node_ref,
    new_hold_buffer,
};

/// `node` rebuilt over `children`, which are the rewritten children in the order
/// [`GpuNode::children`] reports them. Handed a node's own children back it is the
/// identity, which is the property everything else here rests on.
pub(crate) fn rebuild(node: &dyn GpuNode, children: Vec<Box<dyn GpuNode>>) -> Box<dyn GpuNode> {
    let name = node.name();
    let mut children = children.into_iter();
    let mut one = || {
        children
            .next()
            .unwrap_or_else(|| panic!("{name} was rebuilt with no child to take"))
    };
    match as_node_ref(node) {
        NodeRef::LoadParquet(load) => {
            let scan = scan_of(load);
            Box::new(GpuLoadParquet::new(
                load.table.clone(),
                load.projection.clone(),
                load.partition_groups.clone(),
                &scan,
                schema_of(node),
            ))
        }
        NodeRef::Empty(_) => Box::new(GpuEmpty::new(schema_of(node))),
        NodeRef::Filter(filter) => Box::new(GpuFilter::new(
            one(),
            filter.predicate.clone(),
            filter.projection.clone(),
            schema_of(node),
        )),
        NodeRef::Project(project) => Box::new(GpuProject::new(
            one(),
            project.exprs.clone(),
            schema_of(node),
        )),
        NodeRef::Sort(sort) => Box::new(GpuSort::new(one(), sort.keys.clone(), sort.fetch)),
        NodeRef::CoalesceAllBatches(_) => Box::new(GpuCoalesceAllBatches::new(one())),
        NodeRef::AccumulateBatchesAndSort(sort) => Box::new(GpuAccumulateBatchesAndSort::new(
            one(),
            sort.keys.clone(),
            sort.fetch,
        )),
        NodeRef::Limit(limit) => Box::new(GpuLimit::new(one(), limit.interval)),
        NodeRef::Aggregate(aggregate) => Box::new(GpuAggregate::new(
            one(),
            body_of(&aggregate.body),
            aggregate.intermediate().clone(),
            schema_of(node),
        )),
        NodeRef::AggregateBatches(merge) => Box::new(GpuAggregateBatches::new(
            one(),
            body_of(&merge.body),
            merge.intermediate().clone(),
            schema_of(node),
        )),
        NodeRef::Join(join) => {
            // Named rather than two calls in argument position: both sides are the same
            // type, so a swap is a wrong plan the compiler cannot see.
            let (build, probe) = (one(), one());
            Box::new(GpuHashJoin::new(
                build,
                probe,
                join.join_type,
                join.keys.clone(),
                join.filter.clone(),
                join.filter_columns.clone(),
                join.null_equals_null,
                join.projection.clone(),
                schema_of(node),
            ))
        }
        NodeRef::CrossJoin(join) => {
            let (build, probe) = (one(), one());
            Box::new(GpuCrossJoin::new(
                build,
                probe,
                join.projection.clone(),
                schema_of(node),
            ))
        }
        NodeRef::NestedLoopJoin(join) => {
            let (build, probe) = (one(), one());
            Box::new(GpuNestedLoopJoin::new(
                build,
                probe,
                join.join_type,
                join.filter.clone(),
                join.filter_columns.clone(),
                join.projection.clone(),
                schema_of(node),
            ))
        }
        NodeRef::MergePartitions(_) => Box::new(GpuMergePartitions::new(one())),
        NodeRef::EmitPartitions(emit) => Box::new(GpuEmitPartitions::new(
            one(),
            emit.hash_keys.clone(),
            lanes_of(node),
        )),
        NodeRef::MergeSortedPartitions(merge) => Box::new(GpuMergeSortedPartitions::new(
            one(),
            merge.keys.clone(),
            merge.fetch,
        )),
        NodeRef::Union(_) => Box::new(GpuUnion::new(children.collect(), schema_of(node))),
        NodeRef::Interleave(_) => Box::new(GpuInterleave::new(children.collect(), schema_of(node))),
        NodeRef::HoldBuffer(_) => Box::new(new_hold_buffer(one())),
        NodeRef::Unload(unload) => Box::new(GpuUnload::new(one(), unload.interval)),
    }
}

/// The metadata read a loader was built from, back out of the fields it kept — the file
/// the row-group indices are numbered in, the survivors and their statistics. One
/// statement of it, since the injector rebuilds loaders too.
pub(crate) fn scan_of(load: &GpuLoadParquet) -> ScanMetadata {
    ScanMetadata {
        file: load.file.clone(),
        groups: load.survivors.clone(),
        can_be_null: load.can_be_null.clone(),
    }
}

/// Field by field rather than a clone, so a field added to the body is a compile error
/// here — the same guard the exhaustive match gives the kinds.
fn body_of(body: &AggregateBody) -> AggregateBody {
    AggregateBody {
        group_by: body.group_by.clone(),
        grouping_sets: body.grouping_sets.clone(),
        null_exprs: body.null_exprs.clone(),
        aggs: body.aggs.clone(),
        finalize: body.finalize.clone(),
    }
}

fn schema_of(node: &dyn GpuNode) -> Schema {
    node.kind()
        .schema()
        .unwrap_or_else(|| panic!("{} declares a schema", node.name()))
        .clone()
}

/// An emitter's lane count is what it emits into, which is its own declared layout — its
/// input's is where the rows come from and is a different number.
fn lanes_of(node: &dyn GpuNode) -> usize {
    node.kind()
        .layout()
        .unwrap_or_else(|| panic!("{} declares a layout", node.name()))
        .n
}
