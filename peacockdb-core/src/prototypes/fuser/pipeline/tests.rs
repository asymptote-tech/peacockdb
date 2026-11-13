//! The pipelining hint: the schedule's arithmetic on chosen numbers, and its inputs read off a
//! fused plan and its baseline.

use crate::prototypes::fuser::tests::re_read_every_hold;
use crate::prototypes::fuser::tests::{baseline, filter, fused_with, held_pair, lineitem, unload};
use crate::prototypes::fuser::{PipelineHint, RootNeeds, pipeline_hint, pipeline_inputs};

fn root(execute_us: u64, sources: &[usize]) -> RootNeeds {
    RootNeeds {
        execute_us,
        sources: sources.to_vec(),
    }
}

#[test]
fn the_cheapest_root_loads_first() {
    let hint = pipeline_hint(&[100, 10], &[root(50, &[0]), root(20, &[1])]);
    assert_eq!(
        hint,
        PipelineHint {
            load_order: vec![1, 0],
            serial_us: 180,
            makespan_us: 160,
            saved_us: 20,
        }
    );
}

#[test]
fn a_shared_source_is_loaded_once() {
    let hint = pipeline_hint(&[40], &[root(10, &[0]), root(10, &[0])]);
    assert_eq!(hint.load_order, [0]);
    assert_eq!(
        (hint.serial_us, hint.makespan_us, hint.saved_us),
        (60, 60, 0)
    );
}

#[test]
fn execution_waits_for_the_executor() {
    let hint = pipeline_hint(&[5, 5], &[root(100, &[0]), root(1, &[1])]);
    assert_eq!(hint.load_order, [0, 1]);
    assert_eq!(hint.makespan_us, 106);
}

#[test]
fn a_root_needing_two_sources_waits_for_both() {
    // Root 1 needs both, root 0 one: root 0's source comes first, then root 1's missing one.
    let hint = pipeline_hint(&[30, 10], &[root(5, &[0]), root(5, &[0, 1])]);
    assert_eq!(hint.load_order, [0, 1]);
    assert_eq!(hint.makespan_us, 45);
}

#[test]
fn inputs_take_the_largest_member_load() {
    let trees = vec![
        unload(filter(lineitem(&[0, 4], &[0]))),
        unload(filter(lineitem(&[0, 5], &[0]))),
    ];
    let fused = fused_with(&trees, &mut re_read_every_hold);
    let mut times = baseline(&trees, |query, name| match name {
        "GpuLoadParquet" => [100, 300][query],
        _ => 1,
    });
    times.per_query[0].execute_us = 11;
    times.per_query[1].execute_us = 22;
    let (loads, roots) = pipeline_inputs(&fused, &times);
    assert_eq!(loads, [300]);
    assert_eq!(roots, [root(11, &[0]), root(22, &[0])]);
}

#[test]
fn a_re_read_is_a_source_of_its_own() {
    let trees = held_pair(&[0, 4], &[0, 4]);
    let fused = fused_with(&trees, &mut re_read_every_hold);
    assert_eq!(fused.reads[1], ("lineitem".to_string(), 2));
    // q0: unload, join, coalesce, orders (3), filter, lineitem (5); q1: unload, filter, lineitem (2).
    let times = baseline(&trees, |query, name| match (query, name) {
        (0, "GpuLoadParquet") => 70,
        (1, "GpuLoadParquet") => 90,
        _ => 1,
    });
    let (loads, roots) = pipeline_inputs(&fused, &times);
    assert_eq!(
        loads.len(),
        3,
        "orders, q0's own lineitem and the shared one"
    );
    assert_eq!(roots[0].sources.len(), 2);
    assert_eq!(roots[1].sources.len(), 1);
    assert!(!roots[0].sources.contains(&roots[1].sources[0]));
    assert_eq!(
        loads[roots[1].sources[0]], 90,
        "the shared load is q1's alone now"
    );
}
