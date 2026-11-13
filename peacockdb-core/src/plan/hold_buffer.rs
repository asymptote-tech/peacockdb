//! `GpuHoldBuffer`: what a hold stops at. It emits the batches it is handed, in the order it
//! was handed them, so its output is its input's in every respect a layout or schema states.

use std::any::Any;

use super::GpuHoldBuffer;
use super::GpuNode;
use super::NodeKind;
use super::PlanError;
use super::{input_layout, input_schema};

impl GpuNode for GpuHoldBuffer {
    fn kind(&self) -> &NodeKind {
        &self.kind
    }

    fn children(&self) -> Vec<&dyn GpuNode> {
        vec![self.input.as_ref()]
    }

    /// Nothing: it keeps whole batches per lane and emits them unchanged, so any input but a
    /// sink is one it can take, and a sink below a root is refused by the structural pass.
    fn validate_schemas_and_partitions(&self) -> Result<(), PlanError> {
        Ok(())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

pub(crate) fn over(input: Box<dyn GpuNode>) -> GpuHoldBuffer {
    let kind = NodeKind::Intermediate {
        layout: input_layout(input.as_ref()),
        schema: input_schema(input.as_ref()),
    };
    GpuHoldBuffer { kind, input }
}
