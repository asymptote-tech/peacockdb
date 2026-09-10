//! The nodes that hold a lane's batches back: coalesce-all, accumulate-and-sort, and the
//! mid-plan limit — which is an accumulator by category and holds nothing at all.

use super::{GpuAccumulateBatchesAndSort, GpuCoalesceAllBatches, GpuLimit};
use std::any::Any;

use super::PlanError;
use super::{BatchLayout, ColumnOrder, NodeKind, PartitionLayout, SortOrder};
use super::{GpuNode, RowInterval};
use super::{check_merge_keys, input_layout, input_schema};

impl GpuNode for GpuCoalesceAllBatches {
    fn kind(&self) -> &NodeKind {
        &self.kind
    }

    fn children(&self) -> Vec<&dyn GpuNode> {
        vec![self.input.as_ref()]
    }

    /// Nothing: concatenating a lane's batches requires nothing of them — any lane count,
    /// any batch count, any order — and it carries no parameter of its own to be wrong.
    fn validate_schemas_and_partitions(&self) -> Result<(), PlanError> {
        Ok(())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl GpuNode for GpuAccumulateBatchesAndSort {
    fn kind(&self) -> &NodeKind {
        &self.kind
    }

    fn children(&self) -> Vec<&dyn GpuNode> {
        vec![self.input.as_ref()]
    }

    fn validate_schemas_and_partitions(&self) -> Result<(), PlanError> {
        check_merge_keys(
            "GpuAccumulateBatchesAndSort",
            &self.keys,
            &input_layout(self.input.as_ref()),
        )
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// Which rows an interval names depends on the order they arrive in, so a limit under an
/// order is only the top n where the whole stream carries it. Batches each sorted and not
/// sorted against each other is the shape that reads as ordered and is not — it names n
/// rows from wherever the batch boundaries fell.
///
/// No plan reaches it today: every `SortExec` lowers to a per-batch sort under an
/// accumulator, so an ordered stream is the only ordered thing there is. It is a backstop
/// against a lowering that emits ranges before the accumulator has them all (#138).
pub(crate) fn check_ordered_prefix(node: &str, input: &PartitionLayout) -> Result<(), PlanError> {
    if input.sort_order.is_batch_sorted() && !input.is_stream_sorted() {
        return Err(PlanError::Invalid(format!(
            "{node}: its input's batches are each sorted and not sorted against each other, \
             so a prefix of it is not the top rows — the planner puts a \
             GpuAccumulateBatchesAndSort or a GpuMergeSortedPartitions below it"
        )));
    }
    Ok(())
}

impl GpuNode for GpuLimit {
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
                "GpuLimit: an interval over {} lanes names no rows — the planner inserts \
                 GpuMergePartitions below it",
                input.n
            )));
        }
        check_ordered_prefix("GpuLimit", &input)
    }

    fn row_interval(&self) -> Option<RowInterval> {
        Some(self.interval)
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

pub(crate) fn new_coalesce_all_batches(input: Box<dyn GpuNode>) -> GpuCoalesceAllBatches {
    let mut layout = input_layout(input.as_ref());
    layout.batch_layout = BatchLayout::SingleBatch;
    // Sorted batches concatenated are not a sorted batch; only a merge makes one.
    layout.sort_order = SortOrder::NotSpecified;
    let kind = NodeKind::Intermediate {
        layout,
        schema: input_schema(input.as_ref()),
    };
    GpuCoalesceAllBatches { kind, input }
}

pub(crate) fn new_accumulate_batches_and_sort(
    input: Box<dyn GpuNode>,
    keys: Vec<ColumnOrder>,
    fetch: Option<usize>,
) -> GpuAccumulateBatchesAndSort {
    let mut layout = input_layout(input.as_ref());
    layout.sort_order = SortOrder::batch_sorted(keys.clone());
    layout.batch_layout = BatchLayout::SingleBatch;
    let kind = NodeKind::Intermediate {
        layout,
        schema: input_schema(input.as_ref()),
    };
    GpuAccumulateBatchesAndSort {
        kind,
        keys,
        fetch,
        input,
    }
}

pub(crate) fn new_limit(input: Box<dyn GpuNode>, interval: RowInterval) -> GpuLimit {
    // A limit is a prefix of its stream: it neither increases the batch count nor
    // disturbs an order, so the layout is its input's.
    let kind = NodeKind::Intermediate {
        layout: input_layout(input.as_ref()),
        schema: input_schema(input.as_ref()),
    };
    GpuLimit {
        kind,
        interval,
        input,
    }
}
