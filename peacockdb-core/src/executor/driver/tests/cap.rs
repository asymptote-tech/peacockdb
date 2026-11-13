//! The concurrency cap: at most `cap` roots run at once, admitted in root order as earlier
//! ones finish, while a shared source runs on into the buffers of the roots still waiting.

use std::num::NonZeroUsize;

use super::mock::{Script, spec};
use super::*;
use crate::executor::CpuBatch;
use crate::plan::validate_dag;
use crate::tests::plans::*;

fn cap(k: usize) -> Option<NonZeroUsize> {
    NonZeroUsize::new(k)
}

fn three() -> Script {
    Script::default().source("s", vec![vec![spec(10, 80), spec(20, 160), spec(30, 240)]])
}

/// Root 0 `unload(filter(S))`; roots 1 and 2 `unload(buffer(filter(S)))`: 0 unload, 1 filter,
/// 2 S, 3 unload, 4 buffer, 5 filter, 6 unload, 7 buffer, 8 filter. Root 1's cap region is
/// {3, 4} and root 2's {6, 7}.
fn three_roots() -> Vec<Box<dyn GpuNode>> {
    let s = shared("s", 1);
    vec![
        unload(filter(read(&s))),
        unload(buffer(filter(read(&s)))),
        unload(buffer(filter(read(&s)))),
    ]
}

/// The first step at which a node of `region` did more than take a batch into its store.
fn first_run_step(report: &RunReport, region: &[u32]) -> u32 {
    report
        .trace
        .iter()
        .filter(|event| region.contains(&event.node) && event.call != CallKind::Keep)
        .map(|event| event.step)
        .min()
        .expect("the root ran")
}

fn last_step(report: &RunReport, node: u32) -> u32 {
    report
        .trace
        .iter()
        .filter(|event| event.node == node)
        .map(|event| event.step)
        .max()
        .expect("the node ran")
}

fn rows(answers: &[Vec<CpuBatch>]) -> Vec<Vec<usize>> {
    answers
        .iter()
        .map(|batches| {
            batches
                .iter()
                .map(|batch| batch.record_batch().num_rows())
                .collect()
        })
        .collect()
}

#[test]
fn a_cap_of_one_runs_roots_one_at_a_time() {
    let script = three();
    let roots = three_roots();
    let fused = run_capped(&refs(&roots), &script, cap(1));
    let report = &fused.report;
    assert!(
        first_run_step(report, &[3, 4]) > last_step(report, 0),
        "root 1 ran before root 0 had finished"
    );
    assert!(
        first_run_step(report, &[6, 7]) > last_step(report, 3),
        "root 2 ran before root 1 had finished"
    );
    assert_accounted(report);
    assert_conserved_over(&refs(&roots), report);
}

#[test]
fn a_capped_root_reads_a_shared_source_through_its_buffer() {
    let (s, buffer_of_root_2) = (2, 7);
    let script = three();
    let roots = three_roots();
    let fused = run_capped(&refs(&roots), &script, cap(1));
    let report = &fused.report;
    assert_eq!(report.emitted[s][0].len(), 3);
    assert!(
        last_step(report, s as u32) < first_run_step(report, &[6, 7]),
        "S finished while root 2 still waited"
    );
    assert_eq!(
        count_at(report, buffer_of_root_2, CallKind::Keep),
        3,
        "every batch waited in root 2's buffer"
    );
    assert!(
        report.peak_queued[s] <= 1,
        "and none in S's queue: {}",
        report.peak_queued[s]
    );
    assert_accounted(report);
}

fn count_at(report: &RunReport, node: usize, kind: CallKind) -> usize {
    calls(report, kind)
        .iter()
        .filter(|event| event.node as usize == node)
        .count()
}

#[test]
fn a_satisfied_root_frees_its_cap_slot() {
    let script = three();
    let s = shared("s", 1);
    let roots = vec![
        unload_limited(filter(read(&s)), 0, Some(1)),
        unload(buffer(filter(read(&s)))),
        unload(buffer(filter(read(&s)))),
    ];
    let fused = run_capped(&refs(&roots), &script, cap(1));
    assert_eq!(rows(&fused.answers)[0].iter().sum::<usize>(), 1);
    assert_eq!(
        rows(&fused.answers)[1],
        vec![10, 20, 30],
        "root 1 was admitted"
    );
    assert_eq!(
        rows(&fused.answers)[2],
        vec![10, 20, 30],
        "and root 2 after it"
    );
    assert_accounted(&fused.report);
    assert_conserved_over(&refs(&roots), &fused.report);
}

#[test]
fn without_a_cap_the_schedule_is_deterministic() {
    let script = three();
    let roots = three_roots();
    let first = run_capped(&refs(&roots), &script, None);
    let second = run_capped(&refs(&roots), &script, None);
    assert_eq!(first.report.trace, second.report.trace);
}

#[test]
fn every_cap_gives_the_same_answers() {
    let script = three();
    let roots = three_roots();
    let uncapped = rows(&run_capped(&refs(&roots), &script, None).answers);
    for k in 1..=3 {
        let capped = run_capped(&refs(&roots), &script, cap(k));
        assert_eq!(rows(&capped.answers), uncapped, "cap {k}");
        assert_accounted(&capped.report);
    }
}

#[test]
fn a_capped_root_with_a_join_is_accepted_and_runs() {
    let mut script = three();
    for at in 0..3 {
        script = script.source(&format!("b{at}"), vec![vec![spec(4 + at, 32)]]);
    }
    let s = shared("s", 1);
    let roots: Vec<Box<dyn GpuNode>> = (0..3)
        .map(|at| {
            unload(projected_join(
                coalesce_all(source(&format!("b{at}"), 1)),
                buffer(filter(read(&s))),
            ))
        })
        .collect();
    assert_eq!(validate_dag(&refs(&roots), cap(1)), Ok(()));
    let fused = crate::executor::run_roots::<Mock>(&refs(&roots), &script, None, cap(1))
        .expect("the plan runs");
    for (at, answer) in rows(&fused.answers).iter().enumerate() {
        let alone = run(
            unload(projected_join(
                coalesce_all(source(&format!("b{at}"), 1)),
                filter(source("s", 1)),
            ))
            .as_ref(),
            &script,
        );
        assert_eq!(*answer, rows(&[alone.batches])[0], "root {at}");
    }
    assert_accounted(&fused.report);
    assert_conserved_over(&refs(&roots), &fused.report);
}

/// Root 0's limit is satisfied before any step, below a filter. The filter learns its input
/// ended only if its readiness is read after the limit settles; else root 0 never ends, and
/// root 1, waiting on root 0 under a cap of one, is never admitted.
#[test]
fn a_limit_satisfied_at_seed_still_lets_the_next_root_in() {
    let script = Script::default()
        .source("a", vec![vec![spec(10, 80)]])
        .source("b", vec![vec![spec(10, 80), spec(20, 160)]]);
    let roots = vec![
        unload(filter(limit(merge(source("a", 1)), 0, Some(0)))),
        unload(filter(source("b", 1))),
    ];
    let fused = run_capped(&refs(&roots), &script, cap(1));
    assert_eq!(
        rows(&fused.answers)[1],
        vec![10, 20],
        "root 1 answered b's rows"
    );
    assert_accounted(&fused.report);
}
