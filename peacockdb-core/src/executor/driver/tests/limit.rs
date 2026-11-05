//! Both limit lowerings, asserted on the calls that were and were not made. A test on the
//! rows returned passes just as well when the whole input was read and trimmed at the end,
//! which is the thing this lowering exists to avoid.

use super::mock::{AccRule, Script, Spec, spec};
use super::*;
use crate::executor::CpuBatch;
use crate::tests::plans::*;

/// Six batches of ten rows on one lane, over a chain the interval sits on top of.
fn chain(skip: u64, fetch: Option<u64>) -> Box<dyn GpuNode> {
    unload_limited(filter(source("part", 1)), skip, fetch)
}

fn six_batches() -> Script {
    Script::default().source("part", vec![vec![spec(10, 80); 6]])
}

#[test]
fn a_root_adjacent_limit_stops_the_run_early() {
    let report = run(chain(0, Some(15)).as_ref(), &six_batches());
    assert!(!report.satisfied.is_empty());
    assert_eq!(
        count(&report, CallKind::NextBatch),
        2,
        "the scan stopped being scheduled once the interval was satisfied"
    );
    assert_eq!(rows_returned(&report), 15);
    assert_accounted(&report);
}

#[test]
fn without_the_limit_the_same_plan_drains_everything() {
    let report = run(unload(filter(source("part", 1))).as_ref(), &six_batches());
    assert!(report.satisfied.is_empty());
    assert_eq!(count(&report, CallKind::NextBatch), 6);
    assert_eq!(rows_returned(&report), 60);
}

#[test]
fn the_skip_prefix_is_never_unloaded() {
    let report = run(chain(25, Some(10)).as_ref(), &six_batches());
    assert_eq!(
        count(&report, CallKind::ReleaseUnwanted),
        2,
        "the two batches before row 25 were released where they stood"
    );
    assert_eq!(
        report.rows_skipped.iter().sum::<u64>(),
        20,
        "and their rows never crossed the boundary"
    );
    assert_eq!(rows_returned(&report), 10);
    assert_accounted(&report);
}

#[test]
fn only_the_straddling_batches_are_narrowed() {
    // Rows 25..35 of ten-row batches: batch 2 straddles the start, batch 3 the end.
    let report = run(chain(25, Some(10)).as_ref(), &six_batches());
    assert_eq!(count(&report, CallKind::UnloadRange), 2);
    assert_eq!(
        count(&report, CallKind::Unload),
        0,
        "no batch of this interval is wanted whole"
    );
}

#[test]
fn a_batch_wholly_inside_the_interval_is_unloaded_whole() {
    // Rows 10..40: batches 1, 2 and 3 are wanted entire, and no range rides the call.
    let report = run(chain(10, Some(30)).as_ref(), &six_batches());
    assert_eq!(count(&report, CallKind::Unload), 3);
    assert_eq!(count(&report, CallKind::UnloadRange), 0);
    assert_eq!(count(&report, CallKind::ReleaseUnwanted), 1);
    assert_eq!(rows_returned(&report), 30);
}

#[test]
fn the_count_is_across_lanes_and_not_per_lane() {
    // Three lanes of one four-row batch each under a fetch of five. Counted per lane every
    // lane would pass its own four rows and the query would return twelve.
    let plan = unload_limited(filter(source("part", 3)), 0, Some(5));
    let script = Script::default().source(
        "part",
        vec![vec![spec(4, 32)], vec![spec(4, 32)], vec![spec(4, 32)]],
    );
    let report = run(plan.as_ref(), &script);
    assert_eq!(rows_returned(&report), 5);
    assert_eq!(
        count(&report, CallKind::Unload),
        1,
        "lane 0 was wanted whole"
    );
    assert_eq!(
        count(&report, CallKind::UnloadRange),
        1,
        "lane 1 straddles the end"
    );
    assert_eq!(
        count(&report, CallKind::ReleaseUnwanted),
        1,
        "lane 2 is past it"
    );
    assert_accounted(&report);
}

#[test]
fn a_zero_fetch_unloads_nothing_at_all_and_the_plan_still_completes() {
    let report = run(chain(0, Some(0)).as_ref(), &six_batches());
    assert!(
        !report.satisfied.is_empty(),
        "satisfied before a single step"
    );
    assert_eq!(count(&report, CallKind::Unload), 0);
    assert_eq!(count(&report, CallKind::UnloadRange), 0);
    assert_eq!(rows_returned(&report), 0);
    assert_eq!(report.steps, 0, "nothing was ever runnable");
    assert_eq!(report.in_flight_bytes, 0);
}

/// A zero fetch is an answer that ran out of rows before anything ran, and it still has its
/// columns.
#[test]
fn a_zero_fetch_answers_one_zero_row_batch_under_the_sinks_input_columns() {
    let report = run(chain(0, Some(0)).as_ref(), &six_batches());
    assert_eq!(report.batches.len(), 1);
    assert_eq!(report.batches[0].record_batch().num_rows(), 0);
    assert_eq!(report.batches[0].record_batch().schema(), schema().fields);
}

#[test]
fn an_offset_with_no_fetch_never_exits_early() {
    let report = run(chain(25, None).as_ref(), &six_batches());
    assert!(
        report.satisfied.is_empty(),
        "no prefix determines a pure offset, so it can only drop and trim"
    );
    assert_eq!(
        count(&report, CallKind::NextBatch),
        6,
        "every batch was read"
    );
    assert_eq!(count(&report, CallKind::ReleaseUnwanted), 2);
    assert_eq!(rows_returned(&report), 35);
    assert_accounted(&report);
}

#[test]
fn a_skip_past_the_end_returns_nothing_and_unloads_nothing() {
    let report = run(chain(1000, Some(10)).as_ref(), &six_batches());
    assert_eq!(
        count(&report, CallKind::Unload) + count(&report, CallKind::UnloadRange),
        0
    );
    assert_eq!(count(&report, CallKind::ReleaseUnwanted), 6);
    assert_eq!(rows_returned(&report), 0);
    assert_eq!(report.in_flight_bytes, 0);
}

#[test]
fn the_early_exit_reaches_through_a_shuffle() {
    let plan = unload_limited(merge(emit(source("part", 1), 4)), 0, Some(6));
    let script = Script::default().source("part", vec![vec![spec(8, 64); 5]]);
    let report = run(plan.as_ref(), &script);
    assert!(!report.satisfied.is_empty());
    assert!(
        count(&report, CallKind::NextBatch) < 5,
        "the hold reached the scan through the merge and the emit"
    );
    assert_eq!(
        report.in_flight_bytes, 0,
        "every batch still queued when the run stopped was released"
    );
}

#[test]
fn a_mid_plan_limit_stops_its_own_subtree_and_holds_nothing() {
    // The mid-plan node is an accumulator by category and streams: the driver reads the
    // rows it emitted, and satisfaction stops the scan the same way it does at a sink.
    let plan = unload(filter(limit(merge(source("part", 1)), 0, Some(12))));
    let script = Script::default().source("part", vec![vec![spec(10, 80); 6]]);
    let report = run(plan.as_ref(), &script);
    assert!(!report.satisfied.is_empty());
    assert_eq!(
        count(&report, CallKind::NextBatch),
        2,
        "the scan stopped once twelve rows had left the limit"
    );
    let limit_node = 2;
    assert!(
        report.peak_queued[limit_node] <= 1,
        "a limit holds nothing: it forwards or releases each batch as it arrives"
    );
}

#[test]
fn a_satisfied_limit_reports_done_so_the_node_above_it_can_finish() {
    // `SELECT count(*) FROM (SELECT ... LIMIT n)`: an accumulator above the limit only
    // emits when it is told its input ended, and satisfaction is what ends it — no later
    // batch is coming. A limit that is held without being marked done leaves the parent
    // waiting for a lane that has in fact finished, and the query answers nothing.
    let plan = unload(coalesce_all(limit(merge(source("part", 1)), 0, Some(12))));
    let script = Script::default().source("part", vec![vec![spec(10, 80); 6]]);
    let report = run(plan.as_ref(), &script);
    assert!(!report.satisfied.is_empty());
    assert_eq!(
        count(&report, CallKind::MarkDone),
        1,
        "the accumulator above the limit was never told its input had ended"
    );
    // 20 rather than 12, and deliberately so: the mock's limit forwards whole batches where
    // the real engine's `LimitStream` trims to the interval. This test is about the driver's
    // done propagation, not the trimming (each backend's own limit tests cover that), so the
    // number here means "not nothing", not the engine's answer.
    assert_eq!(
        rows_returned(&report),
        20,
        "the count reached the root instead of the query answering nothing"
    );
}

/// A mid-plan limit over the six ten-row batches, its executor following `rule`.
fn mid_plan(skip: u64, fetch: Option<u64>, rule: AccRule) -> RunReport {
    let plan = unload(filter(limit(merge(source("part", 1)), skip, fetch)));
    run(plan.as_ref(), &six_batches().with_limit(rule))
}

#[test]
fn a_mid_plan_limit_is_satisfied_by_fetch_alone_and_not_by_skip_plus_fetch() {
    // The mock forwards each batch whole and implements no skip, so a real limit executor
    // would need three pulls here rather than one. The one pull is what tells the driver's
    // rules apart: by rows emitted a limit wanting five is done with the first batch, by its
    // input against skip + fetch it reads three.
    let report = mid_plan(25, Some(5), AccRule::Streaming);
    assert!(!report.satisfied.is_empty());
    assert_eq!(
        count(&report, CallKind::NextBatch),
        1,
        "the limit's own output reached its fetch with the first batch"
    );
    assert_accounted(&report);
}

#[test]
fn a_limit_emitting_fewer_rows_than_it_read_keeps_the_driver_pulling() {
    // Four rows out of every ten: twelve have left the limit after the third batch and not
    // before, where its input reached twelve after the second.
    let report = mid_plan(0, Some(12), AccRule::Trimming(4));
    assert!(!report.satisfied.is_empty());
    assert_eq!(
        count(&report, CallKind::NextBatch),
        3,
        "the driver stopped on rows the limit never let through"
    );
    assert_accounted(&report);
}

#[test]
fn a_mid_plan_limit_with_no_fetch_is_never_satisfied() {
    let report = mid_plan(5, None, AccRule::Streaming);
    assert!(
        report.satisfied.is_empty(),
        "no count of rows emitted determines a pure offset"
    );
    assert_eq!(
        count(&report, CallKind::NextBatch),
        6,
        "every batch was read"
    );
    assert_accounted(&report);
}

#[test]
fn a_mid_plan_limit_of_no_rows_is_satisfied_before_any_pull_whatever_its_skip() {
    // With a skip of 0 the old rule held at the seed too; the skip is what made it pull.
    let report = mid_plan(5, Some(0), AccRule::Streaming);
    assert!(!report.satisfied.is_empty(), "satisfied at the seed");
    assert_eq!(count(&report, CallKind::NextBatch), 0);
    assert_eq!(rows_returned(&report), 0);
    assert_eq!(report.in_flight_bytes, 0);
}

#[test]
fn a_limit_whose_only_consumer_is_the_sink_is_refused_before_the_run() {
    // The planner's canonical form, checked on the way in: a mock plan meets the same
    // refusal a planned one would, rather than the driver quietly running a tree the
    // planner does not emit.
    let plan = unload(limit(source("part", 1), 0, Some(5)));
    let script = Script::default().source("part", vec![vec![spec(10, 80)]]);
    match run_with(plan.as_ref(), &script, None) {
        Err(RunError::Backend(error)) => assert!(
            error.to_string().contains("a limit feeding only the sink"),
            "the refusal says the wrong thing: {error}"
        ),
        other => panic!("expected the planner's refusal, got {other:?}"),
    }
}

// -- several roots over a shared source ----------------------------------------------

fn batches_of_ten(count: usize) -> Vec<Spec> {
    vec![spec(10, 80); count]
}

fn total_rows(batches: &[CpuBatch]) -> usize {
    batches
        .iter()
        .map(|batch| batch.record_batch().num_rows())
        .sum()
}

#[test]
fn a_satisfied_root_does_not_stop_a_source_another_root_reads() {
    // 0 unload A, 1 S, 2 unload B.
    let script = Script::default().source("s", vec![batches_of_ten(5)]);
    let s = shared("s", 1);
    let roots = vec![unload_limited(read(&s), 0, Some(1)), unload(read(&s))];
    let fused = run_roots(&refs(&roots), &script);
    assert_eq!(fused.report.emitted[1][0].len(), 5, "B still read all of S");
    assert_eq!(total_rows(&fused.answers[0]), 1);
    assert_eq!(total_rows(&fused.answers[1]), 50);
    assert!(
        fused.report.peak_queued[1] <= 1,
        "A's reader retired, so B took each batch whole and nothing piled up: {}",
        fused.report.peak_queued[1]
    );
    assert_accounted(&fused.report);
    assert_conserved_over(&refs(&roots), &fused.report);
}

#[test]
fn the_source_stops_once_every_reader_is_satisfied() {
    let script = Script::default().source("s", vec![batches_of_ten(5)]);
    let s = shared("s", 1);
    let roots = vec![
        unload_limited(read(&s), 0, Some(1)),
        unload_limited(read(&s), 0, Some(1)),
    ];
    let fused = run_roots(&refs(&roots), &script);
    assert_eq!(count(&fused.report, CallKind::NextBatch), 1);
    assert_eq!(fused.report.satisfied, vec![0, 1, 2], "S among them");
    assert_eq!(total_rows(&fused.answers[0]), 1);
    assert_eq!(total_rows(&fused.answers[1]), 1);
    assert_accounted(&fused.report);
    assert_conserved_over(&refs(&roots), &fused.report);
}

/// A = `unload(filter(limit(merge(S))))`, B = `unload(S)`, S at two lanes: 0 unload, 1 filter,
/// 2 limit, 3 merge, 4 S, 5 unload. B takes a share of both lanes, the merge forwards lane
/// 0's, and the limit is satisfied by it with lane 1's still queued for the merge alone.
fn a_merge_left_behind() -> (Vec<Box<dyn GpuNode>>, Script) {
    let script = Script::default().source("s", vec![batches_of_ten(3), batches_of_ten(3)]);
    let s = shared("s", 2);
    let roots = vec![
        unload(filter(limit(merge(read(&s)), 0, Some(5)))),
        unload(read(&s)),
    ];
    (roots, script)
}

#[test]
fn a_retired_reader_releases_what_it_left_queued() {
    let (limit_node, s) = (2, 4);
    let (roots, script) = a_merge_left_behind();
    let refs = refs(&roots);
    let mut driver = driver_of_roots(&refs, &script);
    while driver.step().expect("no protocol violation") {
        if driver.is_satisfied(limit_node) {
            break;
        }
    }
    assert!(driver.is_satisfied(limit_node), "the limit was satisfied");
    assert_eq!(
        driver.queue_len(s, 1),
        0,
        "lane 1's batch was wanted by the retired merge alone, so it went back"
    );
    let fused = run_roots(&refs, &script);
    assert_eq!(total_rows(&fused.answers[1]), 60, "B read every row");
    assert_accounted(&fused.report);
    assert_conserved_over(&refs, &fused.report);
}

#[test]
fn a_limit_zero_root_retires_its_reader_before_any_step() {
    // 0 unload A, 1 S, 2 unload B.
    let script = Script::default().source("s", vec![batches_of_ten(3)]);
    let s = shared("s", 1);
    let roots = vec![unload_limited(read(&s), 0, Some(0)), unload(read(&s))];
    let fused = run_roots(&refs(&roots), &script);
    assert_eq!(total_rows(&fused.answers[0]), 0);
    assert_eq!(total_rows(&fused.answers[1]), 30, "B still gets every row");
    assert_eq!(fused.report.consumed[0][0][0], 0, "A took nothing");
    assert_eq!(
        fused.report.abandoned[1][0], 30,
        "every row of S is one A never took"
    );
    assert!(
        fused.report.peak_queued[1] <= 1,
        "no batch waited for A: {}",
        fused.report.peak_queued[1]
    );
    assert_accounted(&fused.report);
    assert_conserved_over(&refs(&roots), &fused.report);
}

#[test]
fn a_satisfied_limit_releases_what_a_buffer_below_it_keeps() {
    // 0 unload, 1 join, 2 coalesce, 3 project, 4 filter, 5 b, 6 buffer, 7 filter, 8 p. The
    // build path is the deeper, so p drains into the buffer during the build; the first
    // probe batch satisfies the limit with two still kept.
    let buffer_node = 6;
    let plan = unload_limited(
        join(
            coalesce_all(project(filter(source("b", 1)))),
            buffer(filter(source("p", 1))),
        ),
        0,
        Some(1),
    );
    let script = Script::default()
        .source("b", vec![vec![spec(4, 32)]])
        .source("p", vec![batches_of_ten(3)]);
    let mut driver = driver(plan.as_ref(), &script);
    while driver.step().expect("no protocol violation") {
        if driver.is_satisfied(0) {
            break;
        }
    }
    assert!(driver.is_satisfied(0), "the limit was satisfied");
    assert_eq!(
        driver.kept(buffer_node, 0),
        0,
        "the buffer gave back what it kept as the limit was satisfied"
    );
    let report = run(plan.as_ref(), &script);
    assert_eq!(
        report.emitted[buffer_node][0].len(),
        1,
        "released, not emitted"
    );
    assert_accounted(&report);
    assert_conserved(plan.as_ref(), &report);
}

/// A satisfied limit excuses what it left queued, not a root that never ended: the run is
/// over with root 1 never stepped, and the driver must say so rather than answer it empty.
#[test]
fn a_satisfied_limit_does_not_excuse_a_root_that_never_ended() {
    let script = Script::default()
        .source("a", vec![vec![spec(10, 80)]])
        .source("b", vec![vec![spec(10, 80)]]);
    let roots = vec![
        unload_limited(filter(source("a", 1)), 0, Some(0)),
        unload(filter(source("b", 1))),
    ];
    let mut driver = driver_of_roots(&refs(&roots), &script);
    match driver.finish() {
        Err(StepError::Run(RunError::Protocol(message))) => {
            assert!(message.contains("never ended"), "{message}")
        }
        other => panic!("a root that never ended was let through: {other:?}"),
    }
}
