//! Plans built from the real node types, since the driver reads what a node declares —
//! lane counts, categories, intervals. Only the executors are mock.

use std::sync::Arc;

use datafusion::arrow::datatypes::{DataType, Field, Schema as ArrowSchema};
use datafusion::common::JoinType;

use crate::plan::ColumnOrder;
use crate::plan::RowGroupMeta;
use crate::plan::ScanMetadata;
use crate::plan::Schema;
use crate::plan::{Expr, NamedExpr};
use crate::plan::{
    GpuCoalesceAllBatches, GpuEmitPartitions, GpuFilter, GpuHashJoin, GpuInterleave, GpuLimit,
    GpuLoadParquet, GpuMergePartitions, GpuMergeSortedPartitions, GpuProject, GpuSort, GpuUnion,
    GpuUnload,
};
use crate::plan::{GpuNode, RowInterval};

pub(crate) fn schema() -> Schema {
    Schema::new(Arc::new(ArrowSchema::new(vec![Field::new(
        "k",
        DataType::Int64,
        true,
    )])))
}

/// A loader with `lanes` lanes. The mapping is what the driver reads the lane count off;
/// how many batches each lane emits is the mock's script.
pub(crate) fn source(table: &str, lanes: usize) -> Box<dyn GpuNode> {
    let groups: Vec<RowGroupMeta> = (0..lanes as u32)
        .map(|index| RowGroupMeta {
            index,
            rows: 100,
            bytes: 800,
        })
        .collect();
    let scan = ScanMetadata {
        file: format!("/{table}.parquet"),
        groups: groups.clone(),
        can_be_null: vec![false],
    };
    let partition_groups = (0..lanes as u32).map(|lane| vec![vec![lane]]).collect();
    Box::new(GpuLoadParquet::new(
        table.to_string(),
        vec![0],
        partition_groups,
        &scan,
        None,
        schema(),
    ))
}

pub(crate) fn filter(input: Box<dyn GpuNode>) -> Box<dyn GpuNode> {
    Box::new(GpuFilter::new(input, Expr::column(0, "k"), None, schema()))
}

pub(crate) fn project(input: Box<dyn GpuNode>) -> Box<dyn GpuNode> {
    Box::new(GpuProject::new(
        input,
        vec![NamedExpr::new(Expr::column(0, "k"), "k")],
        schema(),
    ))
}

pub(crate) fn sort(input: Box<dyn GpuNode>) -> Box<dyn GpuNode> {
    Box::new(GpuSort::new(input, vec![key()], None))
}

pub(crate) fn coalesce_all(input: Box<dyn GpuNode>) -> Box<dyn GpuNode> {
    Box::new(GpuCoalesceAllBatches::new(input))
}

pub(crate) fn limit(input: Box<dyn GpuNode>, skip: u64, fetch: Option<u64>) -> Box<dyn GpuNode> {
    Box::new(GpuLimit::new(input, RowInterval { skip, fetch }))
}

pub(crate) fn emit(input: Box<dyn GpuNode>, lanes: usize) -> Box<dyn GpuNode> {
    Box::new(GpuEmitPartitions::new(input, vec![0], lanes))
}

pub(crate) fn merge(input: Box<dyn GpuNode>) -> Box<dyn GpuNode> {
    Box::new(GpuMergePartitions::new(input))
}

pub(crate) fn merge_sorted(input: Box<dyn GpuNode>) -> Box<dyn GpuNode> {
    Box::new(GpuMergeSortedPartitions::new(input, vec![key()], None))
}

pub(crate) fn union(branches: Vec<Box<dyn GpuNode>>) -> Box<dyn GpuNode> {
    Box::new(GpuUnion::new(branches, schema()))
}

pub(crate) fn interleave(branches: Vec<Box<dyn GpuNode>>) -> Box<dyn GpuNode> {
    Box::new(GpuInterleave::new(branches, schema()))
}

/// The build side is always the left child, which is the orientation the schedule turns
/// into "the build subtree drains first".
pub(crate) fn join(build: Box<dyn GpuNode>, probe: Box<dyn GpuNode>) -> Box<dyn GpuNode> {
    join_of(JoinType::Inner, build, probe)
}

/// `join` with its type chosen: what a lane owes when its build side is empty is the
/// type's answer, so a test about that needs one that owes something.
pub(crate) fn join_of(
    join_type: JoinType,
    build: Box<dyn GpuNode>,
    probe: Box<dyn GpuNode>,
) -> Box<dyn GpuNode> {
    Box::new(GpuHashJoin::new(
        build,
        probe,
        join_type,
        vec![(0, 0)],
        None,
        Vec::new(),
        false,
        None,
        schema(),
    ))
}

pub(crate) fn unload(input: Box<dyn GpuNode>) -> Box<dyn GpuNode> {
    Box::new(GpuUnload::new(input, None))
}

pub(crate) fn unload_limited(
    input: Box<dyn GpuNode>,
    skip: u64,
    fetch: Option<u64>,
) -> Box<dyn GpuNode> {
    Box::new(GpuUnload::new(input, Some(RowInterval { skip, fetch })))
}

fn key() -> ColumnOrder {
    ColumnOrder {
        column: 0,
        ascending: true,
        nulls_first: false,
    }
}
