//! A stub leaf for a case about one operator: it declares a schema and a layout, and an
//! executor is handed its node and its input batches, so nothing under test is a scan.

use std::any::Any;
use std::sync::Arc;

use datafusion::arrow::datatypes::{DataType, Field, Schema as ArrowSchema};

use crate::plan::{BatchLayout, GpuNode, NodeKind, PartitionLayout, PlanError, Schema};

/// A child that declares a schema and a layout and nothing else. Outside the node registry
/// on purpose: `attach_recipes` emits no seq for it, so an operator over `Given` leaves gets
/// its own recipe and the wire carries only the operator.
#[derive(Debug)]
pub(crate) struct Given {
    kind: NodeKind,
}

impl Given {
    /// One lane, the batch layout given.
    pub(crate) fn of(schema: Schema, batches: BatchLayout) -> Box<dyn GpuNode> {
        Self::with_layout(
            schema,
            PartitionLayout {
                batch_layout: batches,
                ..PartitionLayout::new(1)
            },
        )
    }

    /// One lane, several batches — the shape most cases start from.
    pub(crate) fn of_columns(fields: &[(&str, DataType)]) -> Box<dyn GpuNode> {
        Self::of(columns(fields), BatchLayout::MultipleBatches)
    }

    /// Whatever the operator above needs its child to have said: N lanes for a partition
    /// accumulator, a sort order for a sorted merge, a hash for a co-partitioned join.
    pub(crate) fn with_layout(schema: Schema, layout: PartitionLayout) -> Box<dyn GpuNode> {
        Box::new(Self {
            kind: NodeKind::Intermediate { layout, schema },
        })
    }
}

impl GpuNode for Given {
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

/// A plain schema over nullable fields, with no keys and no aggregate state annotated.
pub(crate) fn columns(fields: &[(&str, DataType)]) -> Schema {
    Schema::new(Arc::new(ArrowSchema::new(
        fields
            .iter()
            .map(|(name, kind)| Field::new(*name, kind.clone(), true))
            .collect::<Vec<_>>(),
    )))
}
