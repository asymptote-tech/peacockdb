//! The process-global instrumentation switches, and the pooled allocator a measured run
//! must be taken under.
//!
//! Thin wrappers over `peacockdb_ffi::raw`, here because this is the only path that calls
//! them: the node-at-a-time executors that used to share them are gone. Nothing in a
//! shipping query reaches this file — a run that never turns the switches on pays one
//! relaxed load per call and nothing else.

use std::sync::atomic::{AtomicBool, Ordering};

use peacockdb_ffi::raw::{
    peacock_install_rmm_pool, peacock_nvtx_pop_range, peacock_nvtx_push_range,
    peacock_set_node_timing, peacock_set_nvtx_ranges, PeacockRmmPoolInfo,
    PEACOCK_NODE_TIMING_EVENTS, PEACOCK_NODE_TIMING_OFF, PEACOCK_RMM_POOL_INSTALLED,
};

/// Which device allocator a measurement was taken under — the outcome of
/// [`install_rmm_pool`], not the request.
///
/// cuDF routes every intermediate through rmm's current device resource, and the
/// difference between a pool and rmm's default (a `cudaMalloc`/`cudaFree` per
/// allocation) is far larger than run-to-run noise — worst on exactly the nodes with
/// the largest outputs. So `Unavailable` does not describe a slower run to be recorded
/// and compared; it describes a run whose times mean nothing, and the benchmark harness
/// refuses it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RmmPool {
    /// A pooled resource is installed. Sizes are what it was actually built with.
    Pool { integrated: bool, free_bytes: u64, initial_bytes: u64, maximum_bytes: u64 },
    /// The pool could not be built — typically a neighbour holding the device when the
    /// reservation was computed — so rmm's default resource is in place and nobody
    /// chose that.
    Unavailable,
}

impl std::fmt::Display for RmmPool {
    /// The `allocator=` line of a benchmark record. One line, no spaces around `=`,
    /// sizes in GiB because that is the unit the sizing rule is written in.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        const GIB: f64 = 1073741824.0;
        match *self {
            RmmPool::Pool { integrated, free_bytes, initial_bytes, maximum_bytes } => write!(
                f,
                "rmm-pool initial={:.1}GiB max={:.1}GiB of {:.1}GiB free on {} device",
                initial_bytes as f64 / GIB,
                maximum_bytes as f64 / GIB,
                free_bytes as f64 / GIB,
                if integrated { "an integrated" } else { "a discrete" },
            ),
            RmmPool::Unavailable => {
                write!(f, "rmm-default (pool unavailable), cudaMalloc per allocation")
            }
        }
    }
}

/// Install the pooled device allocator and report what happened.
///
/// Idempotent, and the idempotency lives in C++ (`peacock::install_rmm_pool`) rather
/// than behind a `OnceLock` here, so the process has exactly one guard no matter which
/// side calls first — the gtest binaries call it from `main()`, this path calls it per
/// case. A second call rebuilding the pool would drop a resource that live allocations
/// still point into; the benchmark target, 127 `#[test]` functions sharing one process,
/// is precisely the shape that would find that.
///
/// Must run before any GPU work. Cheap to call again afterwards — it returns the first
/// call's outcome — which is why the caller can just ask for the label at write time.
///
/// The engine does not install this for itself: a shipping query still allocates the
/// expensive way, and `gpu_memory_limit` is still accepted and ignored. Changing that is
/// `llm-wiki/tickets.md` #148, and it is a decision about the product rather than about
/// measurement, which is why this entry point exists in the meantime.
pub fn install_rmm_pool() -> RmmPool {
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

/// How per-node GPU regions are measured. `Off` by default, because measuring is not
/// free.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum NodeTiming {
    /// No measurement. Every timing field stays 0.
    #[default]
    Off,
    /// CUDA events around the device work, host clock around the host work, no sync
    /// inside the region. Device numbers arrive via `collect_regions` after the
    /// root materialize, into [`PartitionStat::device_us`].
    Events,
}

/// Select the per-node GPU timing mode (process-global, [`NodeTiming::Off`] by default).
///
/// Why it is opt-in, and why the split into host setup / host submit / device exists, is
/// argued once on `set_node_timing` and `mark_device_start` in
/// `cpp/src/plan_executor.h`. The GPU suite runs `--test-threads=1` (cuDF/RMM share one
/// process-wide pool), so the global needs no cross-test guard.
pub fn set_node_timing(mode: NodeTiming) {
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
pub fn node_timing_on() -> bool {
    MEASURING.load(Ordering::Relaxed)
}

/// Emit NVTX ranges around plan nodes and their output partitions (process-global, off
/// by default).
///
/// Why this is not folded into [`set_node_timing`] is argued on `set_nvtx_ranges` in
/// `cpp/src/plan_executor.h`. Nothing reads the ranges unless a profiler is attached,
/// so this is for capture runs, not for the benchmark tree.
pub fn set_nvtx_ranges(on: bool) {
    unsafe { peacock_set_nvtx_ranges(i32::from(on)) };
}

/// A named NVTX range around whatever the caller is about to do, closed when the returned
/// value drops.
///
/// For a benchmark harness naming the case it runs. Node ranges carry `<seq>.<call_index>`
/// and seq numbering restarts with every plan, so a capture of several queries cannot say
/// from those names which query a call was in; this range answers it by containment, and
/// the reader stops needing to be told the query on its command line.
///
/// RAII rather than a pop the caller must remember: a case that panics mid-run would
/// otherwise leave the range open and swallow every case after it into the wrong query.
///
/// A no-op while ranges are off, and nothing in the engine calls it — a shipping query
/// pays nothing here because it never arrives.
#[must_use = "the range closes when this is dropped, so dropping it at once ranges nothing"]
pub fn nvtx_range(name: &str) -> NvtxRange {
    // Interior NUL is not an error worth a Result: the name is built by the harness from
    // its own case identifiers, and a NUL in one of those is a bug in the harness. The
    // range is simply not opened, which shows up as a capture missing a level.
    if let Ok(owned) = std::ffi::CString::new(name) {
        unsafe { peacock_nvtx_push_range(owned.as_ptr()) };
    }
    NvtxRange(())
}

/// Closes the range [`nvtx_range`] opened.
pub struct NvtxRange(());

impl Drop for NvtxRange {
    fn drop(&mut self) {
        unsafe { peacock_nvtx_pop_range() };
    }
}
