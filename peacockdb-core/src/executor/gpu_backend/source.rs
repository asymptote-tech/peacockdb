//! `GpuLoadParquet` on a device: one `execute_scan_rowgroups` per batch, over the row
//! groups the mapping gave this lane.
//!
//! The additive symbol is what makes the node a source at all — the frozen `execute_node`
//! reads every row group the scan node carries, in one call — so the row groups a batch
//! reads are an argument here rather than a field of the node the seq addresses.

use std::sync::Arc;

use datafusion::arrow::datatypes::Schema as ArrowSchema;

use peacockdb_ffi::raw::{PeacockNodeStats, peacock_executor_execute_scan_rowgroups};

use crate::executor::node_timing_on;

use super::GpuSource;
use super::{CallSite, last_error, produced};
use crate::executor::Batch;
use crate::executor::GpuBatch;
use crate::executor::{AbiCall, AbiCalls, AbiTarget, BackendError, CallStats};
use crate::plan::GpuLoadParquet;
use crate::plan::PlanError;
use crate::wire::{AbiSymbol, CallPattern, Input, Recipe};

impl GpuSource {
    pub(crate) fn new(
        site: CallSite,
        recipe: &Recipe,
        node: &GpuLoadParquet,
        schema: &ArrowSchema,
    ) -> Result<Self, PlanError> {
        let [call] = recipe.calls.as_slice() else {
            return Err(PlanError::Invalid(format!(
                "a source makes one call per batch, and this recipe is `{recipe}`"
            )));
        };
        if call.symbol != AbiSymbol::ExecuteScanRowGroups
            || call.when != CallPattern::PerBatch
            || call.inputs.as_slice() != [Input::RowGroups]
        {
            return Err(PlanError::Invalid(format!(
                "a source reads the row groups of one batch per call, and this one is {call:?}"
            )));
        }
        let (seq, kind) = call
            .target
            .ok_or_else(|| PlanError::Invalid(format!("{call:?} addresses no seq")))?;
        let lane = site.lane;
        let batches = node.partition_groups.get(lane).ok_or_else(|| {
            PlanError::Invalid(format!(
                "lane {lane} of a scan the partitioner mapped into {} lanes",
                node.partition_groups.len()
            ))
        })?;
        Ok(Self {
            site,
            seq,
            kind,
            batches: batches.iter().cloned().collect(),
            schema: Arc::new(schema.clone()),
        })
    }

    /// The next batch, or `None` where the mapping gave this lane nothing more.
    pub(crate) fn read_next(&mut self) -> Result<Option<(GpuBatch, CallStats)>, BackendError> {
        let Some(groups) = self.batches.pop_front() else {
            return Ok(None);
        };
        let mut handle = 0u64;
        let mut stats = PeacockNodeStats::default();
        let rc = unsafe {
            peacock_executor_execute_scan_rowgroups(
                self.site.executor,
                self.seq as u64,
                groups.as_ptr(),
                groups.len() as u64,
                &mut handle,
                &mut stats,
            )
        };
        if rc != 0 {
            return Err(BackendError::new(format!(
                "execute_scan_rowgroups(#{}, {groups:?}): {}",
                self.seq,
                last_error(self.site.executor)
            )));
        }
        let batch = produced(self.site.executor, self.seq, handle, stats, &self.schema);
        // A scan takes no batch, so its input is nothing rather than unknown — the same
        // zero the driver models it with.
        let mut calls = AbiCalls::armed(node_timing_on());
        calls.record(AbiCall {
            seq: self.seq,
            target: AbiTarget::Node(self.kind),
            call_index: 0,
            in_rows: 0,
            in_bytes: 0,
            out_rows: stats.rows,
            out_bytes: batch.byte_size() as u64,
        });
        Ok(Some((
            batch,
            CallStats {
                scratch_bytes: None,
                calls,
            },
        )))
    }
}
