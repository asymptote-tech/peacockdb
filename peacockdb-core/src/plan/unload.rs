//! `GpuUnload`: the boundary crossing, and the only node whose output is a CPU batch.

use super::GpuUnload;
use std::any::Any;

use super::NodeKind;
use super::PlanError;
use super::accumulators::check_ordered_prefix;
use super::input_layout;
use super::{GpuNode, RowInterval};

impl GpuNode for GpuUnload {
    fn kind(&self) -> &NodeKind {
        &self.kind
    }

    fn children(&self) -> Vec<&dyn GpuNode> {
        vec![self.input.as_ref()]
    }

    /// An unload takes any lane count and any batch layout — its interval is counted
    /// across lanes by the driver rather than requiring a merge below it — but an interval
    /// it absorbed names rows in the input's order, so the same prefix rule applies here
    /// as on a mid-plan limit.
    fn validate_schemas_and_partitions(&self) -> Result<(), PlanError> {
        if self.interval.is_none() {
            return Ok(());
        }
        check_ordered_prefix("GpuUnload", &input_layout(self.input.as_ref()))
    }

    fn row_interval(&self) -> Option<RowInterval> {
        self.interval
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}
