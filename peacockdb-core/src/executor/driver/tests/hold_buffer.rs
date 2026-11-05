//! A hold buffer under a join's hold: what runs while the join builds, what the buffer keeps,
//! in what order it lets go, and what the accountant sees of it. One root throughout; several
//! roots over a shared source need one queue per reader.

use super::mock::{FailAt, Script, spec};
use super::*;
use crate::executor::EmittedBatch;
use crate::tests::plans::*;

/// unload <- join <- [coalesce <- project <- filter <- b, buffer <- filter <- p]. The build
/// side is the deeper one, so the probe source is the lower height and runs first: only the
/// buffer stands between it and a hold.
///
/// 0 unload, 1 join, 2 coalesce, 3 project, 4 filter, 5 b, 6 buffer, 7 filter, 8 p.
fn buffered_join(lanes: usize) -> Box<dyn GpuNode> {
    unload(join(
        coalesce_all(project(filter(source("b", lanes)))),
        buffer(filter(source("p", lanes))),
    ))
}

const JOIN: u32 = 1;
const BUFFER: usize = 6;
const PROBE_FILTER: usize = 7;

/// Each lane's probe batches of a size of their own, so an order and a lane are both visible
/// in what the buffer emits.
fn buffered_script(lanes: usize) -> Script {
    let probe = (0..lanes)
        .map(|lane| {
            (1..=3)
                .map(|at| spec(10 * at + lane, 80 * at + lane))
                .collect()
        })
        .collect();
    Script::default()
        .source("b", vec![vec![spec(4, 32); 3]; lanes])
        .source("p", probe)
}

fn step_of_first(report: &RunReport, node: u32, call: CallKind) -> u32 {
    report
        .trace
        .iter()
        .find(|event| event.node == node && event.call == call)
        .map(|event| event.step)
        .unwrap_or_else(|| panic!("node {node} never made {call:?}"))
}

fn sizes(batches: &[EmittedBatch]) -> Vec<(u64, usize)> {
    batches
        .iter()
        .map(|batch| (batch.rows, batch.bytes))
        .collect()
}

#[test]
fn the_subtree_below_a_held_buffer_runs_during_the_build() {
    let plan = buffered_join(1);
    let report = run(plan.as_ref(), &buffered_script(1));
    let set_build = step_of_first(&report, JOIN, CallKind::SetBuild);
    let filter_steps: Vec<u32> = report
        .trace
        .iter()
        .filter(|event| event.node as usize == PROBE_FILTER)
        .map(|event| event.step)
        .collect();
    assert_eq!(filter_steps.len(), 4, "three batches and the end of input");
    assert!(
        filter_steps.iter().all(|step| *step < set_build),
        "the filter ran at {filter_steps:?}, the build was set at {set_build}"
    );
    let kept = report
        .trace
        .iter()
        .filter(|event| event.node as usize == BUFFER && event.step < set_build)
        .map(|event| event.call)
        .collect::<Vec<CallKind>>();
    assert_eq!(
        kept,
        vec![CallKind::Keep; 3],
        "held, it keeps and emits nothing"
    );
    assert_accounted(&report);
    assert_conserved(plan.as_ref(), &report);
}

#[test]
fn a_held_buffer_with_nothing_offered_is_not_ready() {
    // A held buffer that stepped with nothing to take would be picked again and again, and
    // the run would end at the step bound rather than here.
    let plan = buffered_join(1);
    let report = run(plan.as_ref(), &buffered_script(1));
    let set_build = step_of_first(&report, JOIN, CallKind::SetBuild);
    let held_steps = report
        .trace
        .iter()
        .filter(|event| event.node as usize == BUFFER && event.step < set_build)
        .count();
    assert_eq!(held_steps, report.emitted[PROBE_FILTER][0].len());
}

#[test]
fn a_buffer_emits_in_arrival_order_once_the_hold_lifts() {
    let plan = buffered_join(1);
    let report = run(plan.as_ref(), &buffered_script(1));
    let set_build = step_of_first(&report, JOIN, CallKind::SetBuild);
    assert!(step_of_first(&report, BUFFER as u32, CallKind::Forward) > set_build);
    assert_eq!(
        sizes(&report.emitted[BUFFER][0]),
        sizes(&report.emitted[PROBE_FILTER][0]),
        "what the join was handed is what the filter emitted, in the order it emitted it"
    );
    assert_eq!(count(&report, CallKind::Probe), 3);
    assert_accounted(&report);
}

#[test]
fn a_four_lane_buffer_keeps_one_queue_per_lane() {
    let plan = buffered_join(4);
    let report = run(plan.as_ref(), &buffered_script(4));
    for lane in 0..4 {
        let arrived = sizes(&report.emitted[PROBE_FILTER][lane]);
        assert_eq!(arrived.len(), 3, "lane {lane}");
        assert_eq!(sizes(&report.emitted[BUFFER][lane]), arrived, "lane {lane}");
    }
    assert_accounted(&report);
    assert_conserved(plan.as_ref(), &report);
}

#[test]
fn an_unheld_buffer_forwards_as_it_arrives() {
    let script = Script::default().source("p", vec![vec![spec(10, 80), spec(20, 160)]]);
    let bare = unload(filter(source("p", 1)));
    let with = unload(buffer(filter(source("p", 1))));
    let (bare_report, with_report) = (run(bare.as_ref(), &script), run(with.as_ref(), &script));
    let rows = |report: &RunReport| -> Vec<usize> {
        report
            .batches
            .iter()
            .map(|batch| batch.record_batch().num_rows())
            .collect()
    };
    assert_eq!(rows(&with_report), rows(&bare_report));
    // Node 1 is the buffer; every node below it sits one index lower without it.
    let others = |report: &RunReport, buffer: Option<u32>| -> Vec<(u32, u32, CallKind, u32)> {
        report
            .trace
            .iter()
            .filter(|event| Some(event.node) != buffer)
            .map(|event| {
                let node = match buffer {
                    Some(at) if event.node > at => event.node - 1,
                    _ => event.node,
                };
                (node, event.lane, event.call, event.outputs)
            })
            .collect()
    };
    assert_eq!(others(&with_report, Some(1)), others(&bare_report, None));
    assert!(
        with_report
            .trace
            .iter()
            .filter(|event| event.node == 1)
            .all(|event| event.call != CallKind::Keep),
        "nothing holds it, so it keeps nothing"
    );
    assert_accounted(&with_report);
}

#[test]
fn a_buffer_holds_its_batches_in_the_accountant() {
    let plan = buffered_join(1);
    let script = buffered_script(1);
    let kept_bytes: usize = script.sources[1].1[0].iter().map(|batch| batch.bytes).sum();
    let mut driver = driver(plan.as_ref(), &script);
    let mut most_kept = 0;
    while driver.step().expect("no protocol violation") {
        if driver.last_call() == Some(CallKind::SetBuild) {
            break;
        }
        most_kept = most_kept.max(driver.in_flight());
    }
    assert!(
        most_kept >= kept_bytes,
        "the buffer kept {kept_bytes} bytes and the accountant saw {most_kept} in flight"
    );
    let report = run(plan.as_ref(), &script);
    assert!(report.peak_bytes >= kept_bytes);
    assert_accounted(&report);
}

#[test]
fn a_failed_call_with_batches_buffered_still_reconciles() {
    let plan = buffered_join(1);
    let script = buffered_script(1).failing_at(FailAt::MarkDone);
    let mut driver = driver(plan.as_ref(), &script);
    let error = loop {
        match driver.step() {
            Ok(true) => {}
            Ok(false) => panic!("the run ended without failing"),
            Err(error) => break error,
        }
    };
    assert!(matches!(error, StepError::Run(RunError::CallFailed(_))));
    assert!(
        driver.kept(BUFFER, 0) >= 2,
        "the build failed with the buffer holding batches"
    );
    driver.release_all().expect("what it held goes back");
    let (holds, releases) = driver.hops();
    assert_eq!(holds, releases, "{holds} held, {releases} released");
    assert_eq!(driver.in_flight(), 0);
    assert!(matches!(
        run_with(plan.as_ref(), &script, None),
        Err(RunError::CallFailed(_))
    ));
}
