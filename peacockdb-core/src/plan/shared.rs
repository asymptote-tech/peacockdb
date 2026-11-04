//! `SharedSource`: one `GpuLoadParquet` behind every parent that holds a clone of it.
//!
//! Everything but the identity is the load's, `as_any` included, so a plan line, a backend's
//! executor match and the serializer each see a load and need no arm of their own.

use std::any::Any;

use super::GpuNode;
use super::NodeKind;
use super::PlanError;
use super::RowInterval;
use super::SharedSource;

impl GpuNode for SharedSource {
    fn kind(&self) -> &NodeKind {
        self.load.kind()
    }

    fn children(&self) -> Vec<&dyn GpuNode> {
        Vec::new()
    }

    fn validate_schemas_and_partitions(&self) -> Result<(), PlanError> {
        self.load.validate_schemas_and_partitions()
    }

    fn row_interval(&self) -> Option<RowInterval> {
        self.load.row_interval()
    }

    fn sharing_key(&self) -> Option<usize> {
        Some(std::sync::Arc::as_ptr(&self.load) as usize)
    }

    fn as_any(&self) -> &dyn Any {
        self.load.as_any()
    }
}
