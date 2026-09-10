//! The nodes that change which lane a batch is in. `GpuEmitPartitions` arrives with the
//! shuffle shapes; what a one-lane plan needs is the merge.

use super::{GpuEmitPartitions, GpuMergePartitions, GpuMergeSortedPartitions};
use std::any::Any;

use super::GpuNode;
use super::PlanError;
use super::{BatchLayout, ColumnOrder, KeyDistribution, NodeKind, SortOrder};
use super::{check_merge_keys, input_layout, input_schema};

impl GpuNode for GpuMergePartitions {
    fn kind(&self) -> &NodeKind {
        &self.kind
    }

    fn children(&self) -> Vec<&dyn GpuNode> {
        vec![self.input.as_ref()]
    }

    /// Nothing: it forwards each batch it is handed, so every lane count and batch layout
    /// is one it can take — and what it declares of its own output, the claims it drops,
    /// is checked by the structural pass.
    fn validate_schemas_and_partitions(&self) -> Result<(), PlanError> {
        Ok(())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl GpuNode for GpuEmitPartitions {
    fn kind(&self) -> &NodeKind {
        &self.kind
    }

    fn children(&self) -> Vec<&dyn GpuNode> {
        vec![self.input.as_ref()]
    }

    fn validate_schemas_and_partitions(&self) -> Result<(), PlanError> {
        let input = input_layout(self.input.as_ref());
        if input.n != 1 {
            return Err(PlanError::Invalid(format!(
                "GpuEmitPartitions: it scatters one lane, not {} — the planner inserts \
                 GpuMergePartitions below it",
                input.n
            )));
        }
        let columns = input_schema(self.input.as_ref()).fields.fields().len();
        for key in &self.hash_keys {
            if *key as usize >= columns {
                return Err(PlanError::Invalid(format!(
                    "GpuEmitPartitions: hash key @{key} is past the {columns} columns its \
                     input has"
                )));
            }
        }
        Ok(())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl GpuNode for GpuMergeSortedPartitions {
    fn kind(&self) -> &NodeKind {
        &self.kind
    }

    fn children(&self) -> Vec<&dyn GpuNode> {
        vec![self.input.as_ref()]
    }

    fn validate_schemas_and_partitions(&self) -> Result<(), PlanError> {
        check_merge_keys(
            "GpuMergeSortedPartitions",
            &self.keys,
            &input_layout(self.input.as_ref()),
        )
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

pub(crate) fn new_merge_partitions(input: Box<dyn GpuNode>) -> GpuMergePartitions {
    let mut layout = input_layout(input.as_ref());
    if layout.n > 1 {
        // N lanes of one batch each leave one lane holding N of them.
        layout.batch_layout = BatchLayout::MultipleBatches;
    }
    layout.n = 1;
    // Interleaving lanes keeps neither the hash that separated them nor an order
    // that held within one of them.
    layout.key_distribution = KeyDistribution::NotSpecified;
    layout.sort_order = SortOrder::NotSpecified;
    let kind = NodeKind::Intermediate {
        layout,
        schema: input_schema(input.as_ref()),
    };
    GpuMergePartitions { kind, input }
}

pub(crate) fn new_emit_partitions(
    input: Box<dyn GpuNode>,
    hash_keys: Vec<u32>,
    n: usize,
) -> GpuEmitPartitions {
    let mut layout = input_layout(input.as_ref());
    layout.n = n;
    layout.key_distribution = KeyDistribution::ByHash {
        hash_keys: hash_keys.clone(),
    };
    // Scattering cuts every batch into N, so neither an order within a batch nor a
    // one-batch lane survives.
    layout.sort_order = SortOrder::NotSpecified;
    layout.batch_layout = BatchLayout::MultipleBatches;
    let kind = NodeKind::Intermediate {
        layout,
        schema: input_schema(input.as_ref()),
    };
    GpuEmitPartitions {
        kind,
        hash_keys,
        input,
    }
}

pub(crate) fn new_merge_sorted_partitions(
    input: Box<dyn GpuNode>,
    keys: Vec<ColumnOrder>,
    fetch: Option<usize>,
) -> GpuMergeSortedPartitions {
    let mut layout = input_layout(input.as_ref());
    layout.n = 1;
    layout.key_distribution = KeyDistribution::NotSpecified;
    layout.sort_order = SortOrder::batch_sorted(keys.clone());
    layout.batch_layout = BatchLayout::SingleBatch;
    let kind = NodeKind::Intermediate {
        layout,
        schema: input_schema(input.as_ref()),
    };
    GpuMergeSortedPartitions {
        kind,
        keys,
        fetch,
        input,
    }
}
