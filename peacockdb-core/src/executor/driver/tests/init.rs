//! An aggregate's init as the driver drives it: a batch accumulator whose lane makes its done
//! call only where nothing went out. The mock journals each init call, so a case counts the
//! calls each of a lane's backend calls made.

use super::mock::{Script, spec};
use super::plans::*;
use super::*;

/// Pre-order: the unload, the init, the source.
const INIT: usize = 1;

/// The init calls each of `lane`'s backend calls made, in order.
fn init_calls(report: &RunReport, lane: usize) -> Vec<usize> {
    report.abi_calls[INIT][lane]
        .iter()
        .map(|calls| calls.recorded().expect("the init rule is armed").len())
        .collect()
}

fn rows_out(report: &RunReport, lane: usize) -> Vec<u64> {
    report.emitted[INIT][lane]
        .iter()
        .map(|batch| batch.rows)
        .collect()
}

#[test]
fn a_keyless_lane_that_saw_no_batch_makes_one_done_call_and_answers_one_row() {
    let script = Script::default().source("t", vec![vec![spec(10, 80)], vec![]]);
    let plan = unload(init(source("t", 2), true));
    let report = run(plan.as_ref(), &script);
    assert_eq!(init_calls(&report, 1), vec![1], "done, the lane's one call");
    assert_eq!(rows_out(&report, 1), vec![1]);
    assert_eq!(
        init_calls(&report, 0),
        vec![1, 0],
        "the other lane's batch, and no done call"
    );
    assert_accounted(&report);
}

#[test]
fn a_keyless_lane_of_zero_row_batches_makes_no_init_call_and_one_done_call() {
    let script = Script::default().source("t", vec![vec![spec(0, 8), spec(0, 8)]]);
    let plan = unload(init(source("t", 1), true));
    let report = run(plan.as_ref(), &script);
    assert_eq!(init_calls(&report, 0), vec![0, 0, 1]);
    assert_eq!(rows_out(&report, 0), vec![1]);
    assert_accounted(&report);
}

#[test]
fn a_keyless_lane_with_rows_makes_no_done_call() {
    let script = Script::default().source("t", vec![vec![spec(10, 80), spec(4, 32)]]);
    let plan = unload(init(source("t", 1), true));
    let report = run(plan.as_ref(), &script);
    assert_eq!(init_calls(&report, 0), vec![1, 1, 0]);
    assert_eq!(rows_out(&report, 0), vec![1, 1]);
}

#[test]
fn zero_row_batches_among_others_change_no_row() {
    let plain = Script::default().source("t", vec![vec![spec(10, 80), spec(4, 32)]]);
    let padded = Script::default().source(
        "t",
        vec![vec![
            spec(0, 8),
            spec(10, 80),
            spec(0, 8),
            spec(4, 32),
            spec(0, 8),
        ]],
    );
    for keyless in [true, false] {
        let plan = unload(init(source("t", 1), keyless));
        assert_eq!(
            rows_out(&run(plan.as_ref(), &padded), 0),
            rows_out(&run(plan.as_ref(), &plain), 0),
            "keyless: {keyless}"
        );
    }
}

#[test]
fn a_grouped_lane_that_saw_no_batch_makes_one_done_call_and_emits_nothing() {
    let script = Script::default().source("t", vec![vec![spec(10, 80)], vec![]]);
    let plan = unload(init(source("t", 2), false));
    let report = run(plan.as_ref(), &script);
    assert_eq!(init_calls(&report, 1), vec![1]);
    assert_eq!(rows_out(&report, 1), Vec::<u64>::new());
}

#[test]
fn a_grouped_lane_of_zero_row_batches_makes_one_done_call_and_emits_nothing() {
    let script = Script::default().source("t", vec![vec![spec(0, 8), spec(0, 8)]]);
    let plan = unload(init(source("t", 1), false));
    let report = run(plan.as_ref(), &script);
    assert_eq!(init_calls(&report, 0), vec![0, 0, 1]);
    assert_eq!(rows_out(&report, 0), Vec::<u64>::new());
}
