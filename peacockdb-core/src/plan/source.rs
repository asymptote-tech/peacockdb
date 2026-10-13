//! The two source nodes: `GpuLoadParquet`, and `GpuEmpty`, which reads nothing.

use super::GpuNode;
use super::PlanError;
use super::Schema;
use super::{BatchLayout, KeyDistribution, NodeKind, PartitionLayout, SortOrder};
use super::{GpuEmpty, GpuLoadParquet, ScanMetadata};
use std::any::Any;

impl GpuNode for GpuLoadParquet {
    fn kind(&self) -> &NodeKind {
        &self.kind
    }

    fn children(&self) -> Vec<&dyn GpuNode> {
        Vec::new()
    }

    /// A lane with no batches is not a defect — four lanes over two row groups leave two
    /// of them empty, and the mapping says so rather than inventing work.
    fn validate_schemas_and_partitions(&self) -> Result<(), PlanError> {
        if self.partition_groups.is_empty() {
            return Err(PlanError::Invalid(format!(
                "{}: the partitioner returned no lanes",
                self.table
            )));
        }
        for (lane, batches) in self.partition_groups.iter().enumerate() {
            for (batch, groups) in batches.iter().enumerate() {
                if groups.is_empty() {
                    return Err(PlanError::Invalid(format!(
                        "{}: lane {lane} batch {batch} reads no row group, so it is a read \
                         that returns nothing",
                        self.table
                    )));
                }
                for group in groups {
                    if !self.survivors.iter().any(|meta| meta.index == *group) {
                        return Err(PlanError::Invalid(format!(
                            "{}: lane {lane} reads row group {group}, which pruning left \
                             out — the mapping and the survivors come from one read of the \
                             metadata and have to address the same groups",
                            self.table
                        )));
                    }
                }
            }
        }
        if self.rows_only() && self.kind.schema() != Some(&Schema::rowmarker()) {
            return Err(PlanError::Invalid(format!(
                "{}: it reads no column, so it declares __rowmarker__ and nothing else — the \
                 rows it answers (#63)",
                self.table
            )));
        }
        Ok(())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

pub(crate) fn new_load_parquet(
    table: String,
    projection: Vec<u32>,
    partition_groups: Vec<Vec<Vec<u32>>>,
    scan: &ScanMetadata,
    limit: Option<usize>,
    schema: Schema,
) -> GpuLoadParquet {
    // Batching off still declares MultipleBatches: no downstream phase may assume a
    // lane is one batch, and only an accumulator may make that declaration.
    let layout = PartitionLayout {
        n: partition_groups.len(),
        key_distribution: KeyDistribution::NotSpecified,
        sort_order: SortOrder::NotSpecified,
        batch_layout: BatchLayout::MultipleBatches,
    };
    // The marker a scan of no column answers is a literal, never NULL.
    let can_be_null = match projection.is_empty() {
        true => vec![false],
        false => scan.can_be_null.clone(),
    };
    GpuLoadParquet {
        kind: NodeKind::Source { layout, schema },
        table,
        file: scan.file.clone(),
        projection,
        partition_groups,
        survivors: scan.groups.clone(),
        can_be_null,
        limit,
    }
}

pub(crate) fn largest_batch_bytes(node: &GpuLoadParquet) -> u64 {
    // Every entry in the mapping came from these survivors, so a miss would be a
    // mapping addressing a row group this scan does not read.
    let bytes_of = |index: &u32| {
        node.survivors
            .iter()
            .find(|group| group.index == *index)
            .expect("the mapping addresses a surviving row group")
            .bytes
    };
    node.partition_groups
        .iter()
        .flatten()
        .map(|batch| batch.iter().map(bytes_of).sum::<u64>())
        .max()
        .unwrap_or(0)
}

impl GpuNode for GpuEmpty {
    fn kind(&self) -> &NodeKind {
        &self.kind
    }

    fn children(&self) -> Vec<&dyn GpuNode> {
        Vec::new()
    }

    fn validate_schemas_and_partitions(&self) -> Result<(), PlanError> {
        Ok(())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// One lane, declared one batch: it emits none, so a node above that needs its input whole
/// already has it.
pub(crate) fn new_empty(schema: Schema) -> GpuEmpty {
    let layout = PartitionLayout {
        n: 1,
        key_distribution: KeyDistribution::NotSpecified,
        sort_order: SortOrder::NotSpecified,
        batch_layout: BatchLayout::SingleBatch,
    };
    GpuEmpty {
        kind: NodeKind::Source { layout, schema },
    }
}
