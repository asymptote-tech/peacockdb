//! A device session over one plan, shared by the tiers that open one, and the runs over one
//! tree alone and over several roots fused that the fused tests compare.

use std::path::PathBuf;

use datafusion::arrow::array::RecordBatch;

use crate::executor::{
    CpuBatch, GpuBackend, GpuContext, PrefillError, PrefillStats, Region, RunReport,
    collect_regions, prefill_on, run, run_roots,
};
use crate::plan::GpuNode;
use crate::wire::{RecipePlan, attach_recipes_for_roots};
use peacockdb_ffi::raw::{
    PeacockExecutor, peacock_executor_begin_plan, peacock_executor_create,
    peacock_executor_destroy, peacock_executor_end_plan, peacock_last_error,
};

use super::BUDGET;

/// The recipes attached and the buffer handed across, which is the whole of what a
/// `GpuContext` needs. Owns the executor and ends the plan on drop, so a failing case
/// releases the device rather than leaving it holding a plan.
pub(crate) struct Session {
    executor: *mut PeacockExecutor,
    recipes: Option<RecipePlan>,
}

impl Session {
    /// `join_scratch` is the mode's `join_scratch_bytes`, which every join's session chunks by.
    pub(crate) fn open(tree: &dyn GpuNode, join_scratch: u64, what: &str) -> Self {
        Self::open_roots(&[tree], join_scratch, what)
    }

    /// [`Session::open`] over several roots, crossing as one buffer and one seq space.
    pub(crate) fn open_roots(roots: &[&dyn GpuNode], join_scratch: u64, what: &str) -> Self {
        let recipes = attach_recipes_for_roots(roots, join_scratch)
            .unwrap_or_else(|e| panic!("{what}: no recipes: {e}"));
        let mut executor: *mut PeacockExecutor = std::ptr::null_mut();
        assert_eq!(
            unsafe { peacock_executor_create(BUDGET, &mut executor) },
            0,
            "{what}: peacock_executor_create failed"
        );
        let bytes = recipes.bytes();
        let mut nodes = 0u64;
        let rc = unsafe {
            peacock_executor_begin_plan(executor, bytes.as_ptr(), bytes.len() as u64, &mut nodes)
        };
        assert_eq!(rc, 0, "{what}: begin_plan failed: {}", error_of(executor));
        assert_eq!(nodes as usize, recipes.wire_nodes());
        Self {
            executor,
            recipes: Some(recipes),
        }
    }

    /// What the device recorded, drained while the session is still open: `end_plan`
    /// destroys the events, so a caller reading after the drop reads nothing.
    pub(crate) fn regions(&self, what: &str) -> Vec<Region> {
        collect_regions(self.executor).unwrap_or_else(|e| panic!("{what}: {e}"))
    }

    /// Make `files` resident in the process's pinned host cache. The cache is the process's,
    /// not this session's; the executor only carries the C side's error text.
    pub(crate) fn prefill_pinned_cache(
        &self,
        files: &[PathBuf],
    ) -> Result<PrefillStats, PrefillError> {
        prefill_on(self.executor, files)
    }

    pub(crate) fn context(&mut self) -> GpuContext {
        GpuContext {
            executor: self.executor,
            recipes: self.recipes.take().expect("the recipes are taken once"),
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        unsafe {
            peacock_executor_end_plan(self.executor);
            peacock_executor_destroy(self.executor);
        }
    }
}

/// `tree` run alone in a session over it, its accounting closed: its answer.
pub(crate) fn answer_alone(tree: &dyn GpuNode, join_scratch: u64, what: &str) -> Vec<RecordBatch> {
    let mut session = Session::open(tree, join_scratch, what);
    let report = run::<GpuBackend>(tree, &session.context(), None)
        .unwrap_or_else(|e| panic!("{what}, alone: {e}"));
    assert_closed(&report, what);
    record_batches(&report.batches)
}

/// `roots` run as one plan in one session over all of them, its accounting closed: each
/// root's answer, in root order.
pub(crate) fn answers_of_roots(
    roots: &[&dyn GpuNode],
    join_scratch: u64,
    what: &str,
) -> Vec<Vec<RecordBatch>> {
    let mut session = Session::open_roots(roots, join_scratch, what);
    let fused = run_roots::<GpuBackend>(roots, &session.context(), None, None)
        .unwrap_or_else(|e| panic!("{what}, fused: {e}"));
    assert_closed(&fused.report, what);
    fused
        .answers
        .iter()
        .map(|answer| record_batches(answer))
        .collect()
}

fn assert_closed(report: &RunReport, what: &str) {
    assert_eq!(report.in_flight_bytes, 0, "{what} ended holding batches");
    assert_eq!(
        report.holds, report.releases,
        "{what} held {} batches and released {}",
        report.holds, report.releases
    );
}

fn record_batches(batches: &[CpuBatch]) -> Vec<RecordBatch> {
    batches
        .iter()
        .map(|batch| batch.record_batch().clone())
        .collect()
}

fn error_of(executor: *mut PeacockExecutor) -> String {
    let message = unsafe { peacock_last_error(executor) };
    match message.is_null() {
        true => "(no message)".to_string(),
        false => unsafe { std::ffi::CStr::from_ptr(message) }
            .to_string_lossy()
            .into_owned(),
    }
}
