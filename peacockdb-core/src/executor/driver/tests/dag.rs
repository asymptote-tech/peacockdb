//! Several roots over shared sources: each root answers the rows it answers alone, a shared
//! batch is loaded, held and released once, and every reader takes every batch.

use std::sync::Arc;

use datafusion::arrow::datatypes::{DataType, Field, Schema as ArrowSchema};

use super::mock::{FailAt, Script, spec};
use super::*;
use crate::executor::CpuBatch;
use crate::plan::Expr;
use crate::plan::{GpuProject, NamedExpr, PlanError, Schema};
use crate::tests::plans::*;

/// Three batches of sizes of their own, so an order is visible in what a reader took.
fn three(table: &str) -> Script {
    Script::default().source(
        table,
        vec![vec![spec(10, 80), spec(20, 160), spec(30, 240)]],
    )
}

fn rows_of(batches: &[CpuBatch]) -> Vec<usize> {
    batches
        .iter()
        .map(|batch| batch.record_batch().num_rows())
        .collect()
}

/// `unload(filter(S))`, `unload(project(S))`: 0 unload, 1 filter, 2 S, 3 unload, 4 project.
fn two_readers() -> Vec<Box<dyn GpuNode>> {
    let s = shared("s", 1);
    vec![unload(filter(read(&s))), unload(project(read(&s)))]
}

const S: usize = 2;

#[test]
fn each_root_answers_what_it_answers_alone() {
    let script = three("s");
    let roots = two_readers();
    let fused = run_roots(&refs(&roots), &script);
    let alone = [
        run(unload(filter(source("s", 1))).as_ref(), &script),
        run(unload(project(source("s", 1))).as_ref(), &script),
    ];
    assert_eq!(fused.answers.len(), 2);
    for (root, alone) in alone.iter().enumerate() {
        assert_eq!(
            rows_of(&fused.answers[root]),
            rows_of(&alone.batches),
            "root {root}"
        );
    }
    assert!(fused.report.batches.is_empty(), "the answers carry them");
    assert_accounted(&fused.report);
    assert_conserved_over(&refs(&roots), &fused.report);
}

#[test]
fn a_shared_batch_is_loaded_once() {
    let script = three("s");
    let fused = run_roots(&refs(&two_readers()), &script);
    assert_eq!(fused.report.emitted[S][0].len(), 3);
    assert_eq!(count(&fused.report, CallKind::NextBatch), 3);
}

#[test]
fn a_shared_batch_is_held_once_and_released_once() {
    let script = three("s");
    let fused = run_roots(&refs(&two_readers()), &script);
    let alone = run(unload(filter(source("s", 1))).as_ref(), &script);
    assert_eq!(
        alone.holds, 9,
        "three loaded, three filtered, three unloaded"
    );
    // Each root alone holds nine; together the three loads are held once between them.
    assert_eq!(fused.report.holds, 9 + 9 - 3);
    assert_eq!(fused.report.releases, fused.report.holds);
    assert_eq!(fused.report.in_flight_bytes, 0);
}

#[test]
fn each_reader_consumes_every_batch_once() {
    let script = three("s");
    let fused = run_roots(&refs(&two_readers()), &script);
    for reader in [1, 4] {
        assert_eq!(fused.report.consumed[reader][0][0], 60, "node {reader}");
        let emitted: Vec<u64> = fused.report.emitted[reader][0]
            .iter()
            .map(|batch| batch.rows)
            .collect();
        assert_eq!(emitted, vec![10, 20, 30], "node {reader}, in order");
    }
}

#[test]
fn the_output_hook_sees_a_shared_batch_once() {
    let script = three("s");
    let roots = two_readers();
    let mut seen = vec![0usize; 5];
    let fused = Driver::<Mock>::for_roots(&refs(&roots), &script, None, None)
        .expect("the plan indexes")
        .with_hook(Box::new(|node, _, _| {
            seen[node] += 1;
            Ok(())
        }))
        .run_fused(10_000)
        .expect("the plan runs");
    assert_eq!(seen[S], 3);
    assert_eq!((seen[1], seen[4]), (3, 3));
    assert_accounted(&fused.report);
}

/// A = `join(coalesce(S1), buffer(S2))`, B = `join(coalesce(S2), buffer(S1))`: each build
/// reads the source the other's probe holds.
fn crossed_builds() -> Vec<Box<dyn GpuNode>> {
    let (s1, s2) = (shared("s1", 1), shared("s2", 1));
    vec![
        unload(join(coalesce_all(read(&s1)), buffer(read(&s2)))),
        unload(join(coalesce_all(read(&s2)), buffer(read(&s1)))),
    ]
}

fn crossed_script() -> Script {
    Script::default()
        .source("s1", vec![vec![spec(10, 80), spec(20, 160)]])
        .source("s2", vec![vec![spec(5, 40), spec(15, 120), spec(25, 200)]])
}

#[test]
fn two_roots_whose_builds_wait_on_each_others_probe_both_answer() {
    let script = crossed_script();
    let roots = crossed_builds();
    let fused = run_roots(&refs(&roots), &script);
    let alone = [
        run(
            unload(join(coalesce_all(source("s1", 1)), source("s2", 1))).as_ref(),
            &script,
        ),
        run(
            unload(join(coalesce_all(source("s2", 1)), source("s1", 1))).as_ref(),
            &script,
        ),
    ];
    for (root, alone) in alone.iter().enumerate() {
        assert_eq!(
            rows_of(&fused.answers[root]),
            rows_of(&alone.batches),
            "root {root}"
        );
    }
    assert_accounted(&fused.report);
    assert_conserved_over(&refs(&roots), &fused.report);
}

#[test]
fn a_join_over_one_source_on_both_sides_answers() {
    let script = three("s");
    let s = shared("s", 1);
    let roots = vec![unload(join(coalesce_all(read(&s)), buffer(read(&s))))];
    let fused = run_roots(&refs(&roots), &script);
    let alone = run(
        unload(join(coalesce_all(source("s", 1)), source("s", 1))).as_ref(),
        &script,
    );
    assert_eq!(rows_of(&fused.answers[0]), rows_of(&alone.batches));
    assert_eq!(
        fused.report.emitted[3][0].len(),
        3,
        "the source loads each batch once"
    );
    assert_accounted(&fused.report);
}

#[test]
fn an_unbuffered_reader_inside_a_hold_is_refused_by_run_roots() {
    let script = three("s").source("b", vec![vec![spec(4, 32)]]);
    let s = shared("s", 1);
    let roots = vec![
        unload(projected_join(
            coalesce_all(source("b", 1)),
            filter(read(&s)),
        )),
        unload(filter(read(&s))),
    ];
    match crate::executor::run_roots::<Mock>(&refs(&roots), &script, None, None) {
        Err(RunError::Backend(PlanError::UnbufferedHold {
            reader, producer, ..
        })) => {
            assert_eq!((reader, producer), (4, 5));
        }
        other => panic!("expected an unbuffered hold, got {other:?}"),
    }
    assert_eq!(
        script.built.load(std::sync::atomic::Ordering::Relaxed),
        0,
        "refused before any executor was built"
    );
}

#[test]
fn a_shared_source_feeding_a_held_buffer_queues_one_batch() {
    let script = crossed_script();
    let roots = crossed_builds();
    let refs = refs(&roots);
    let mut driver = driver_of_roots(&refs, &script);
    while driver.step().expect("no protocol violation") {}
    for node in 0..driver.node_count() {
        assert!(
            driver.queue_high_water(node, 0) <= 1,
            "node {node} queued {}; the buffers keep the rest",
            driver.queue_high_water(node, 0)
        );
    }
}

#[test]
fn a_four_lane_shared_source_reaches_every_reader_on_every_lane() {
    let lanes: Vec<Vec<_>> = (0..4)
        .map(|lane| vec![spec(10 + lane, 80), spec(20 + lane, 160)])
        .collect();
    let script = Script::default().source("s", lanes);
    let s = shared("s", 4);
    // 0 unload, 1 filter, 2 S, 3 unload, 4 merge.
    let roots = vec![unload(filter(read(&s))), unload(merge(read(&s)))];
    let fused = run_roots(&refs(&roots), &script);
    for lane in 0..4 {
        let loaded: Vec<u64> = fused.report.emitted[S][lane]
            .iter()
            .map(|b| b.rows)
            .collect();
        let filtered: Vec<u64> = fused.report.emitted[1][lane]
            .iter()
            .map(|b| b.rows)
            .collect();
        assert_eq!(filtered, loaded, "lane {lane}, in order");
        let total: u64 = loaded.iter().sum();
        assert_eq!(
            fused.report.consumed[4][0][lane], total,
            "the merge took lane {lane}"
        );
    }
    assert_eq!(
        rows_of(&fused.answers[1]).iter().sum::<usize>(),
        4 * 30 + 2 * 6
    );
    assert_accounted(&fused.report);
    assert_conserved_over(&refs(&roots), &fused.report);
}

#[test]
fn a_root_that_receives_nothing_answers_its_own_empty_batch() {
    let script = three("s");
    let declared = Schema::new(Arc::new(ArrowSchema::new(vec![Field::new(
        "v",
        DataType::Int64,
        true,
    )])));
    let nothing: Box<dyn GpuNode> = Box::new(GpuProject::new(
        source("t", 1),
        vec![NamedExpr::new(Expr::column(0, "k"), "v")],
        declared,
    ));
    let roots = vec![unload(filter(source("s", 1))), unload(nothing)];
    let fused = run_roots(&refs(&roots), &script);
    assert_eq!(rows_of(&fused.answers[0]), vec![10, 20, 30]);
    assert_eq!(rows_of(&fused.answers[1]), vec![0]);
    let answered = fused.answers[1][0].record_batch().schema();
    assert_eq!(
        answered.field(0).name(),
        "v",
        "its own sink's input, not root 0's"
    );
}

#[test]
fn a_failed_call_with_a_shared_batch_queued_still_reconciles() {
    // The filter is the lower reader, so it takes a share of S's first batch and fails with
    // the original still queued for the coalesce.
    let script = three("s").failing_at(FailAt::Exec);
    let s = shared("s", 1);
    let roots = vec![
        unload(coalesce_all(coalesce_all(read(&s)))),
        unload(filter(read(&s))),
    ];
    let refs = refs(&roots);
    let mut driver = driver_of_roots(&refs, &script);
    let error = loop {
        match driver.step() {
            Ok(true) => {}
            Ok(false) => panic!("the run ended without failing"),
            Err(error) => break error,
        }
    };
    assert!(matches!(error, StepError::Run(RunError::CallFailed(_))));
    assert_eq!(driver.queue_len(3, 0), 1, "the coalesce has yet to take it");
    driver.release_all().expect("what it held goes back");
    let (holds, releases) = driver.hops();
    assert_eq!(holds, releases, "{holds} held, {releases} released");
    assert_eq!(driver.in_flight(), 0);
}

#[test]
fn run_roots_of_one_root_matches_run() {
    let script = three("s");
    let root = unload(filter(source("s", 1)));
    let fused = crate::executor::run_roots::<Mock>(&[root.as_ref()], &script, None, None)
        .expect("the plan runs");
    let alone = crate::executor::run::<Mock>(root.as_ref(), &script, None).expect("the plan runs");
    assert_eq!(fused.report.trace, alone.trace);
    assert_eq!(rows_of(&fused.answers[0]), rows_of(&alone.batches));
}

#[test]
fn no_roots_is_refused() {
    match crate::executor::run_roots::<Mock>(&[], &three("s"), None, None) {
        Err(RunError::Protocol(said)) => assert_eq!(said, "a run needs at least one root"),
        other => panic!("expected a protocol error, got {other:?}"),
    }
}

#[test]
fn a_batch_is_dropped_after_its_last_reader() {
    let script = Script::default().source("s", vec![vec![spec(10, 80)]]);
    let roots = two_readers();
    let refs = refs(&roots);
    let mut driver = driver_of_roots(&refs, &script);
    let mut after: Vec<(usize, usize)> = Vec::new();
    while driver.step().expect("no protocol violation") {
        after.push((driver.last_node(), driver.in_flight()));
    }
    let in_flight_after = |node: usize| {
        after
            .iter()
            .find(|(ran, _)| *ran == node)
            .map(|(_, bytes)| *bytes)
            .expect("the node ran")
    };
    assert_eq!(in_flight_after(S), 80);
    assert_eq!(
        in_flight_after(1),
        80 + 80,
        "the filter's share leaves S's batch charged"
    );
    assert_eq!(
        in_flight_after(4),
        80,
        "the project was the last reader: S's batch is gone"
    );
}

#[test]
fn a_buffered_share_keeps_its_charge_after_the_last_reader_takes() {
    // 0 unload, 1 join, 2 coalesce, 3 project, 4 filter, 5 b, 6 buffer, 7 S, 8 unload,
    // 9 coalesce, 10 filter. The build path is the deeper, so S runs first: the buffer takes
    // a share of each batch, then B's filter takes the original and consumes it.
    let script = three("s").source("b", vec![vec![spec(4, 32)]]);
    let s = shared("s", 1);
    let roots = vec![
        unload(join(
            coalesce_all(project(filter(source("b", 1)))),
            buffer(read(&s)),
        )),
        unload(coalesce_all(filter(read(&s)))),
    ];
    let refs = refs(&roots);
    let mut driver = driver_of_roots(&refs, &script);
    let mut before_the_build = 0;
    while driver.step().expect("no protocol violation") {
        if driver.last_call() == Some(CallKind::SetBuild) {
            break;
        }
        before_the_build = driver.in_flight();
    }
    assert_eq!(driver.kept(6, 0), 3, "the buffer kept all three");
    assert!(
        before_the_build >= 80 + 160 + 240,
        "{before_the_build} in flight: the kept shares no longer carry S's bytes"
    );
    let fused = run_roots(&refs, &script);
    assert_accounted(&fused.report);
    assert_conserved_over(&refs, &fused.report);
}

#[test]
fn a_limit_retiring_a_reader_keeps_the_charge_a_buffer_holds() {
    // 0 unload, 1 join, 2 coalesce, 3 project, 4 filter, 5 b, 6 buffer, 7 S, 8 unload,
    // 9 filter. S runs first; B's filter takes a share of its first batch and B is satisfied
    // by it, before the buffer has taken that batch at all.
    let (buffer_node, s_node, b_root) = (6, 7, 8);
    let script = three("s").source("b", vec![vec![spec(4, 32)]]);
    let s = shared("s", 1);
    let roots = vec![
        unload(join(
            coalesce_all(project(filter(source("b", 1)))),
            buffer(read(&s)),
        )),
        unload_limited(filter(read(&s)), 0, Some(1)),
    ];
    let refs = refs(&roots);
    let mut driver = driver_of_roots(&refs, &script);
    while driver.step().expect("no protocol violation") {
        if driver.last_call() == Some(CallKind::SetBuild) {
            break;
        }
    }
    assert!(
        driver.is_satisfied(b_root),
        "B was satisfied during the build"
    );
    assert_eq!(driver.kept(buffer_node, 0), 3, "the buffer kept all three");
    assert!(
        driver.in_flight() >= 80 + 160 + 240,
        "{} in flight: B's retired reader took the buffered bytes with it",
        driver.in_flight()
    );
    assert!(
        driver.queue_high_water(s_node, 0) <= 1,
        "nothing waited for B's retired filter: {}",
        driver.queue_high_water(s_node, 0)
    );
    let fused = run_roots(&refs, &script);
    assert_eq!(rows_of(&fused.answers[1]).iter().sum::<usize>(), 1);
    assert_accounted(&fused.report);
    assert_conserved_over(&refs, &fused.report);
}

/// A fused root answers the rows it answers alone, not always in the same order. Root 1
/// unions a source of its own with one it shares, and the shared source's height and index
/// come from the whole DAG, not from root 1. So the order holds only where a root reads one
/// source per stream; here it is pinned, so a scheduler change shows.
#[test]
fn a_fused_root_answers_its_rows_alone_though_not_always_in_their_order() {
    let script = three("s").source("t", vec![vec![spec(1, 8), spec(2, 16), spec(3, 24)]]);
    let s = shared("s", 1);
    let roots = vec![
        unload(filter(read(&s))),
        unload(union(vec![source("t", 1), read(&s)])),
    ];
    let fused = run_roots(&refs(&roots), &script);
    let alone = run(
        unload(union(vec![source("t", 1), source("s", 1)])).as_ref(),
        &script,
    );
    let (fused_order, alone_order) = (rows_of(&fused.answers[1]), rows_of(&alone.batches));
    let sorted = |mut rows: Vec<usize>| {
        rows.sort();
        rows
    };
    assert_eq!(sorted(fused_order.clone()), sorted(alone_order.clone()));
    // Alone, T is the lower-indexed source and drains first; fused, S was numbered under
    // root 0, ahead of T, and drains first.
    assert_eq!(alone_order, vec![1, 2, 3, 10, 20, 30]);
    assert_eq!(fused_order, vec![10, 20, 30, 1, 2, 3]);
}
