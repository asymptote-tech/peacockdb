//! The output hook: called once for every batch a node queues as its own, and a refusal
//! ends the query at that node and lane with the hook's own words.
//!
//! The mock's batch is rows and bytes with no schema, so the hook under test is a closure
//! rather than the validator; what is proved here is the wiring — where the driver calls
//! it, what a refusal becomes, and that `None` leaves the run exactly as `run` makes it.

use super::mock::{AccRule, Mock, Script, spec};
use super::plans::*;
use super::*;
use crate::executor::{Batch, OutputHook};
use crate::plan::ExecutorCategory;

/// Pre-order: the unload is 0, then down the chain.
const PROJECT: usize = 4;

/// Every category that emits: a source and an exec on the lane path, an emitter, a
/// partition accumulator, and a forwarder — the one that does not.
fn plan() -> Box<dyn GpuNode> {
    unload(merge_sorted(merge(emit(project(source("part", 1)), 4))))
}

fn three_batches() -> Script {
    Script::default().source("part", vec![vec![spec(10, 80); 3]])
}

fn refused(result: Result<RunReport, RunError>) -> String {
    match result {
        Err(RunError::CallFailed(said)) => said,
        other => panic!("expected the hook's refusal to fail the run, got {other:?}"),
    }
}

#[test]
fn a_hook_that_refuses_fails_the_run_at_that_node_and_lane() {
    let plan = plan();
    let hook: OutputHook<'_, Mock> = Box::new(|node, lane, _| match (node, lane) {
        (PROJECT, 0) => Err("column 0 k: Int64 vs INT32".to_string()),
        _ => Ok(()),
    });
    let said = refused(crate::executor::run_with_hook::<Mock>(
        plan.as_ref(),
        &three_batches(),
        None,
        Some(hook),
    ));
    assert!(
        said.contains("GpuProject"),
        "the message names no node: {said}"
    );
    assert!(said.contains("lane 0"), "the message names no lane: {said}");
    assert!(
        said.contains("Int64 vs INT32"),
        "the hook's own words are lost: {said}"
    );
}

#[test]
fn a_hook_is_called_once_per_emitted_batch() {
    let plan = plan();
    let mut seen = Vec::new();
    let hook: OutputHook<'_, Mock> = Box::new(|node, lane, batch| {
        seen.push((node, lane, batch.num_rows() as u64));
        Ok(())
    });
    let report =
        crate::executor::run_with_hook::<Mock>(plan.as_ref(), &three_batches(), None, Some(hook))
            .expect("an accepting hook passes the run");
    // What the report records as emitted, less the two records that are not a node's own
    // device output: the unload's host batches and a forwarder's moves.
    let index = PlanIndex::build(plan.as_ref()).expect("the plan indexes");
    let mut expected = Vec::new();
    for (node, indexed) in index.nodes.iter().enumerate() {
        if matches!(
            indexed.category,
            ExecutorCategory::Unload | ExecutorCategory::BatchForwarder
        ) {
            continue;
        }
        for (lane, batches) in report.emitted[node].iter().enumerate() {
            expected.extend(batches.iter().map(|batch| (node, lane, batch.rows)));
        }
    }
    seen.sort_unstable();
    expected.sort_unstable();
    assert_eq!(seen, expected);
    assert_eq!(
        seen.len(),
        19,
        "three in, three projected, each scattered over four lanes, one merged"
    );
    assert_accounted(&report);
}

#[test]
fn no_hook_is_the_run_as_it_was() {
    let plan = plan();
    let script = three_batches();
    let plain = crate::executor::run::<Mock>(plan.as_ref(), &script, None).expect("the plan runs");
    let hooked = crate::executor::run_with_hook::<Mock>(plan.as_ref(), &script, None, None)
        .expect("the plan runs");
    assert_eq!(format!("{plain:?}"), format!("{hooked:?}"));
}

#[test]
fn a_hook_refusing_the_third_batch_names_the_third_batchs_node_and_lane() {
    let plan = plan();
    let mut seen = Vec::new();
    let hook: OutputHook<'_, Mock> = Box::new(|node, lane, _| {
        seen.push((node, lane));
        match seen.len() {
            3 => Err("the third".to_string()),
            _ => Ok(()),
        }
    });
    let said = refused(crate::executor::run_with_hook::<Mock>(
        plan.as_ref(),
        &three_batches(),
        None,
        Some(hook),
    ));
    let index = PlanIndex::build(plan.as_ref()).expect("the plan indexes");
    let (node, lane) = seen[2];
    assert_eq!(seen.len(), 3, "the run went on past the refusal");
    assert!(
        said.contains(index.nodes[node].node.name()) && said.contains(&format!("lane {lane}")),
        "the message does not name {} lane {lane}: {said}",
        index.nodes[node].node.name()
    );
    // The third call is not at the first call's node, so the name above is not the one any
    // failure would carry.
    assert_ne!(seen[0].0, node);
}

#[test]
fn a_refused_query_gives_back_everything_it_held() {
    // The refused batch is held and not yet queued, so no queue can give it back: the
    // refusal site has to. Four sites, because the lane arm holds a vector and the two
    // cross-lane loops hold one at a time; the coalesce emits two at done so the batch
    // behind the refused one is also on the vector.
    const SOURCE: usize = 5;
    const COALESCE: usize = 4;
    const EMIT: usize = 3;
    const MERGE_SORTED: usize = 1;
    for at in [SOURCE, COALESCE, EMIT, MERGE_SORTED] {
        let plan = unload(merge_sorted(merge(emit(
            coalesce_all(source("part", 1)),
            4,
        ))));
        let script = three_batches().with_accumulator(AccRule::EmitAtDone(2));
        let hook: OutputHook<'_, Mock> = Box::new(move |node, _, _| match node == at {
            true => Err("refused".to_string()),
            false => Ok(()),
        });
        let mut driver = driver(plan.as_ref(), &script).with_hook(hook);
        let error = loop {
            match driver.step() {
                Ok(true) => continue,
                Ok(false) => panic!("node {at}: the run ended without failing"),
                Err(error) => break error,
            }
        };
        assert!(matches!(error, StepError::Run(RunError::CallFailed(_))));
        driver.release_all().expect("what it held goes back");
        let (holds, releases) = driver.hops();
        assert!(holds > 0, "a query that held nothing proves nothing");
        assert_eq!(
            holds, releases,
            "node {at}: {holds} held, {releases} released"
        );
    }
}
