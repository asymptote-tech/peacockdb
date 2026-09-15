//! The process-global instrumentation switches, and the pooled allocator a measured run
//! must be taken under.
//!
//! Thin wrappers over `peacockdb_ffi::raw`, here because this is the only path that calls
//! them: the node-at-a-time executors that used to share them are gone. Nothing in a
//! shipping query reaches this file — a run that never turns the switches on pays one
//! relaxed load per call and nothing else.

use std::sync::atomic::{AtomicBool, Ordering};

use crate::executor::{NodeTiming, NvtxRange, RmmPool};

use peacockdb_ffi::raw::{
    peacock_install_rmm_pool, peacock_nvtx_pop_range, peacock_nvtx_push_range,
    peacock_set_node_timing, peacock_set_nvtx_ranges, PeacockRmmPoolInfo,
    PEACOCK_NODE_TIMING_EVENTS, PEACOCK_NODE_TIMING_OFF, PEACOCK_RMM_POOL_INSTALLED,
};

/// Install the pooled device allocator and report what happened.
///
/// Idempotent, and the guard lives in C++ rather than behind a `OnceLock` here, so the
/// process has one no matter which side calls first. A second call rebuilding the pool
/// would drop a resource live allocations still point into.
///
/// Must run before any GPU work; cheap afterwards, since it returns the first call's
/// outcome. The engine does not install this for itself — a shipping query still allocates
/// the expensive way, which is #148, a decision about the product and not about
/// measurement.
pub(crate) fn install_rmm_pool() -> RmmPool {
    let mut info = PeacockRmmPoolInfo::default();
    // Non-zero only for a null pointer, which cannot happen here.
    let _ = unsafe { peacock_install_rmm_pool(&mut info) };
    match info.state {
        PEACOCK_RMM_POOL_INSTALLED => RmmPool::Pool {
            integrated: info.integrated != 0,
            free_bytes: info.free_bytes,
            initial_bytes: info.initial_bytes,
            maximum_bytes: info.maximum_bytes,
        },
        // Includes any state a newer C++ side might add: an unrecognised outcome is not
        // an installed pool, and treating it as one is the mistake that matters.
        _ => RmmPool::Unavailable,
    }
}

/// Select the per-node GPU timing mode (process-global, [`NodeTiming::Off`] by default).
///
/// Why it is opt-in, and why the split into host setup / host submit / device exists, is
/// argued once on `set_node_timing` and `mark_device_start` in
/// `cpp/src/plan_executor.h`. The GPU suite runs `--test-threads=1` (cuDF/RMM share one
/// process-wide pool), so the global needs no cross-test guard.
pub(crate) fn set_node_timing(mode: NodeTiming) {
    let raw = match mode {
        NodeTiming::Off => PEACOCK_NODE_TIMING_OFF,
        NodeTiming::Events => PEACOCK_NODE_TIMING_EVENTS,
    };
    MEASURING.store(mode != NodeTiming::Off, Ordering::Relaxed);
    unsafe { peacock_set_node_timing(raw) };
}

/// The mode the setter last selected, so this side can ask without crossing the FFI on
/// every call. A mirror rather than a getter: the setter above is the only way the C++
/// global moves, and the two cannot disagree without going around it.
static MEASURING: AtomicBool = AtomicBool::new(false);

/// Whether the run is measured. What arms the per-call bookkeeping that only a measured
/// run has any use for.
pub(crate) fn node_timing_on() -> bool {
    MEASURING.load(Ordering::Relaxed)
}

/// Emit NVTX ranges around plan nodes and their output partitions (process-global, off
/// by default).
///
/// Why this is not folded into [`set_node_timing`] is argued on `set_nvtx_ranges` in
/// `cpp/src/plan_executor.h`. Nothing reads the ranges unless a profiler is attached,
/// so this is for capture runs, not for the benchmark tree.
pub(crate) fn set_nvtx_ranges(on: bool) {
    unsafe { peacock_set_nvtx_ranges(i32::from(on)) };
}

/// A named NVTX range around whatever the caller is about to do, closed when the returned
/// value drops.
///
/// For a benchmark harness naming the case it runs: node ranges carry `<seq>.<call_index>`
/// and seq numbering restarts per plan, so only containment says which query a call was in.
///
/// RAII rather than a pop the caller must remember — a case that panics mid-run would
/// leave the range open and swallow every later case into the wrong query. A no-op while
/// ranges are off, and nothing in the engine calls it.
#[must_use = "the range closes when this is dropped, so dropping it at once ranges nothing"]
pub(crate) fn nvtx_range(name: &str) -> NvtxRange {
    // Interior NUL is not an error worth a Result: the name is built by the harness from
    // its own case identifiers, and a NUL in one of those is a bug in the harness. The
    // range is simply not opened, which shows up as a capture missing a level.
    if let Ok(owned) = std::ffi::CString::new(name) {
        unsafe { peacock_nvtx_push_range(owned.as_ptr()) };
    }
    NvtxRange(())
}

/// Closes the range [`nvtx_range`] opened; `NvtxRange`'s `Drop` is the only caller.
pub(crate) fn nvtx_pop() {
    unsafe { peacock_nvtx_pop_range() };
}
