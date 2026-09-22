//! What `test_node_timing` measures: a planned query executed on the device under whatever
//! timing mode the caller set, read back as the wall clock, the calls the driver made, the
//! calls the journal holds and the regions the session recorded, summed.
//!
//! The binary compares those figures across modes; the engine types they are read off stay
//! on this side of the harness.

use std::time::Instant;

use crate::executor::{GpuBackend, NodeTiming, RunReport, run};

use super::corpus::plan_at;
use super::gpu_session::Session;
use super::{DevicePlan, TimedRun, mode_named};

/// Plan once, outside every measured run: `plan_at` reads every file's parquet metadata,
/// and repeating that would time the catalog rather than the query.
pub(crate) async fn device_plan(dataset: &str, sf: &str, query: &str, mode: &str) -> DevicePlan {
    let mode = mode_named(mode);
    let what = format!("{dataset}/{query} at {}", mode.name);
    let (ctx, tree) = plan_at(dataset, sf, query, mode).await;
    DevicePlan {
        _ctx: ctx,
        tree,
        what,
    }
}

/// The per-node timing switch by name. Exhaustive: an unlisted name panics naming the two
/// rather than measuring under whichever mode the process was left in.
pub(crate) fn set_node_timing(mode: &str) {
    crate::executor::set_node_timing(match mode {
        "off" => NodeTiming::Off,
        "events" => NodeTiming::Events,
        other => panic!("unknown timing mode '{other}' (expected one of off|events)"),
    })
}

/// One execution, timed end to end, with what the device recorded for it. The session
/// is per run: `attach_recipes` and `begin_plan` are what a query costs on this side of
/// the FFI, and holding one across runs would time the second differently from the first.
pub(crate) fn timed_run(plan: &DevicePlan) -> TimedRun {
    let what = plan.what.as_str();
    let mut session = Session::open(plan.tree.as_ref(), what);
    let ctx = session.context();
    let started = Instant::now();
    let report =
        run::<GpuBackend>(plan.tree.as_ref(), &ctx, None).unwrap_or_else(|e| panic!("{what}: {e}"));
    // Read after the run, not inside it: `GpuUnload` copies the root off the device, so
    // the driver has returned only once the device is actually finished. Under events
    // nothing else would guarantee that — the walk returns while the stream may still run.
    let total_us = started.elapsed().as_micros() as u64;
    let regions = session.regions(what);
    TimedRun {
        total_us,
        calls: executor_calls(&report),
        journalled: journalled_calls(&report),
        regions: regions.len(),
        host_us: regions.iter().map(|region| region.host_us).sum(),
        device_us: regions.iter().map(|region| region.device_us).sum(),
    }
}

/// Every call that reached a backend executor, measured or not: the journal keeps one
/// entry per such call in either mode, so this is the figure the two modes share.
fn executor_calls(report: &RunReport) -> usize {
    report.abi_calls.iter().flatten().map(Vec::len).sum()
}

/// Every ABI call the journal holds, over every node and lane. Zero while timing is off,
/// which is what `AbiCalls::recorded` returning `None` means.
fn journalled_calls(report: &RunReport) -> usize {
    report
        .abi_calls
        .iter()
        .flatten()
        .flatten()
        .filter_map(|made| made.recorded())
        .map(<[_]>::len)
        .sum()
}
