//! The concurrency cap: the rule's arithmetic on chosen numbers, its inputs read off a fused
//! plan, and settling a batch so its waiting roots are buffered or re-read.

use std::num::NonZeroUsize;

use crate::plan::{GpuNode, NodeRef, PlanDag, as_node_ref, validate_dag};
use crate::prototypes::fuser::tests::unload;
use crate::prototypes::fuser::tests::{baseline, coalesce, filter, fused_with, held_pair, join};
use crate::prototypes::fuser::tests::{key_of, lineitem, load_times, re_read_every_hold, refs};
use crate::prototypes::fuser::{Baseline, CapDecision, CapInputs, Choice, HoldLedger};
use crate::prototypes::fuser::{cap_for, cap_inputs, scaled_bytes, settle};

fn inputs(pool: u64, shared: &[u64], buffers: &[u64], roots: &[u64]) -> CapInputs {
    CapInputs {
        pool_bytes: pool,
        shared_bytes: shared.to_vec(),
        buffer_bytes: buffers.to_vec(),
        root_bytes: roots.to_vec(),
    }
}

fn cap(n: usize) -> NonZeroUsize {
    NonZeroUsize::new(n).expect("a cap")
}

#[test]
fn the_cap_admits_the_largest_roots_that_fit() {
    assert_eq!(
        cap_for(&inputs(100, &[30], &[], &[40, 20, 10])),
        CapDecision {
            cap: cap(2),
            budget_bytes: 90,
            estimate_bytes: 90,
            over_budget: false,
        }
    );
}

#[test]
fn every_root_fits() {
    assert_eq!(cap_for(&inputs(100, &[0], &[], &[1, 1, 1])).cap, cap(3));
}

#[test]
fn shared_bytes_over_budget_give_cap_one_and_say_so() {
    let decision = cap_for(&inputs(100, &[95], &[], &[1]));
    assert_eq!((decision.cap, decision.over_budget), (cap(1), true));
}

#[test]
fn shared_bytes_count_whatever_the_cap() {
    let decision = cap_for(&inputs(100, &[60], &[], &[20, 20]));
    assert_eq!((decision.cap, decision.estimate_bytes), (cap(1), 80));
}

#[test]
fn buffer_bytes_count_whatever_the_cap() {
    let decision = cap_for(&inputs(100, &[30], &[25], &[30, 10]));
    assert_eq!(
        (decision.cap, decision.estimate_bytes, decision.over_budget),
        (cap(1), 85, false)
    );
}

#[test]
fn scaling_is_by_planned_bytes() {
    assert_eq!(scaled_bytes(100, 50, 75), 150);
    assert_eq!(scaled_bytes(u64::MAX / 2, 4, 2), u64::MAX / 4);
    assert_eq!(scaled_bytes(7, 0, 10), 7);
}

/// What a load node, fused or a query's own, plans to read.
fn planned_bytes(node: &dyn GpuNode) -> u64 {
    match as_node_ref(node) {
        NodeRef::LoadParquet(load) => load.bytes(),
        _ => panic!("{} is not a load", node.name()),
    }
}

#[test]
fn a_root_is_charged_its_peak_less_its_shared_loads() {
    let trees = vec![
        unload(filter(lineitem(&[0, 4], &[0]))),
        unload(filter(lineitem(&[0, 5], &[0, 1]))),
    ];
    let fused = fused_with(&trees, &mut re_read_every_hold);
    let mut times = baseline(&trees, |_, _| 1);
    for (query, (load, peak)) in [(300, 1000), (200, 500)].into_iter().enumerate() {
        times.per_query[query].batch_bytes[2] = load;
        times.per_query[query].peak_bytes = peak;
    }
    let charged = cap_inputs(&fused, &times, 1 << 30);
    assert_eq!(charged.pool_bytes, 1 << 30);
    assert_eq!(charged.root_bytes, [700, 300]);
    assert!(charged.buffer_bytes.is_empty());
    let roots = refs(&fused.roots);
    let dag = PlanDag::of(&roots);
    let union = (0..dag.nodes.len())
        .find(|at| dag.nodes[*at].node.sharing_key().is_some())
        .expect("one shared load");
    let own = trees[0].children()[0].children()[0];
    let expected = scaled_bytes(
        300,
        planned_bytes(own),
        planned_bytes(dag.nodes[union].node),
    );
    assert!(expected > 300, "the union reads more than q0 alone");
    assert_eq!(charged.shared_bytes, [expected]);
}

#[test]
fn a_re_read_load_is_charged_in_full() {
    let trees = held_pair(&[0, 4], &[0, 4]);
    let fused = fused_with(&trees, &mut re_read_every_hold);
    let mut times = baseline(&trees, |_, _| 1);
    times.per_query[0].batch_bytes[5] = 300;
    times.per_query[0].peak_bytes = 1000;
    times.per_query[1].batch_bytes[2] = 200;
    times.per_query[1].peak_bytes = 500;
    assert_eq!(cap_inputs(&fused, &times, 1 << 30).root_bytes, [1000, 300]);
}

/// A ledger over `times`, every load 70 µs to re-read, the trees' loads all at seq 0.
fn ledger<'b>(
    trees: &[Box<dyn GpuNode>],
    times: &'b Baseline,
    loads: &'b crate::prototypes::fuser::LoadTimes,
    seqs: &'b [Vec<Option<u64>>],
    names: &'b [String],
    pool: u64,
) -> HoldLedger<'b> {
    assert_eq!(trees.len(), names.len());
    HoldLedger::new(pool, times, loads, seqs, names)
}

fn seqs_of(times: &Baseline) -> Vec<Vec<Option<u64>>> {
    times
        .per_query
        .iter()
        .map(|query| {
            let loads = query.names.iter().map(|name| name == "GpuLoadParquet");
            loads.map(|is_load| is_load.then_some(0)).collect()
        })
        .collect()
}

#[test]
fn settling_buffers_the_roots_that_wait() {
    let trees: Vec<Box<dyn GpuNode>> = (0..3)
        .map(|_| unload(filter(lineitem(&[0, 4], &[0]))))
        .collect();
    let mut times = baseline(&trees, |_, _| 1);
    for (query, peak) in [40, 30, 30].into_iter().enumerate() {
        times.per_query[query].peak_bytes = peak;
        times.per_query[query].batch_bytes = vec![1, 5, 0];
    }
    let names: Vec<String> = ["a", "b", "c"].map(String::from).to_vec();
    let loads = load_times(&[("a", 0, 70), ("b", 0, 70), ("c", 0, 70)]);
    let seqs = seqs_of(&times);
    let mut ledger = ledger(&trees, &times, &loads, &seqs, &names, 100);
    let fused = fused_with(&trees, &mut |f, h| ledger.choose(f, h));
    assert!(fused.holds.is_empty(), "no join holds a reader");
    let (settled, decision) = settle(fused, &times, &mut ledger).expect("the batch settles");
    assert_eq!(decision.cap, cap(2));
    assert_eq!(
        settled.holds.len(),
        1,
        "root 2 waits, and its reader is decided"
    );
    assert_eq!(settled.holds[0].query, 2);
    assert!(matches!(
        settled.holds[0].decision.choice,
        Choice::Buffer { .. }
    ));
    assert_eq!(decision.estimate_bytes, 5 + 40 + 30);
    validate_dag(&refs(&settled.roots), Some(decision.cap)).expect("it validates under its cap");
}

#[test]
fn settling_a_batch_with_joins_under_a_cap_validates() {
    let trees: Vec<Box<dyn GpuNode>> = ["orders", "customer", "part"]
        .iter()
        .map(|table| {
            unload(join(
                coalesce(key_of(table)),
                filter(lineitem(&[0, 4], &[0])),
            ))
        })
        .collect();
    let mut times = baseline(&trees, |_, _| 1);
    for query in &mut times.per_query {
        query.peak_bytes = 80;
        // unload, join, coalesce, build load, filter, lineitem
        query.batch_bytes = vec![1, 1, 1, 1, 5, 0];
    }
    let names: Vec<String> = ["a", "b", "c"].map(String::from).to_vec();
    let loads = load_times(&[("a", 0, 70), ("b", 0, 70), ("c", 0, 70)]);
    let seqs = seqs_of(&times);
    let mut ledger = ledger(&trees, &times, &loads, &seqs, &names, 100);
    let fused = fused_with(&trees, &mut |f, h| ledger.choose(f, h));
    let (settled, decision) = settle(fused, &times, &mut ledger).expect("never a HoldCycle");
    assert_eq!(decision.cap, cap(1));
    validate_dag(&refs(&settled.roots), Some(cap(1))).expect("it validates under a cap of one");
}

#[test]
fn a_waiting_root_is_never_buffered_above_its_state() {
    // The cheapest edge is above the accumulator; a buffer there would keep its state outside
    // the cap, which validation refuses.
    let trees: Vec<Box<dyn GpuNode>> = (0..2)
        .map(|_| unload(coalesce(filter(lineitem(&[0, 4], &[0])))))
        .collect();
    let mut times = baseline(&trees, |_, _| 1);
    for query in &mut times.per_query {
        query.peak_bytes = 80;
        // unload, coalesce, filter, lineitem
        query.batch_bytes = vec![1, 5, 50, 0];
    }
    let names: Vec<String> = ["a", "b"].map(String::from).to_vec();
    let loads = load_times(&[("a", 0, 70), ("b", 0, 70)]);
    let seqs = seqs_of(&times);
    // A budget of 135: one root of 80 at a time, and 55 left for a buffer.
    let mut ledger = ledger(&trees, &times, &loads, &seqs, &names, 150);
    let fused = fused_with(&trees, &mut |f, h| ledger.choose(f, h));
    let (settled, decision) = settle(fused, &times, &mut ledger).expect("the batch settles");
    assert_eq!(decision.cap, cap(1));
    assert_eq!(settled.holds.len(), 1);
    assert!(matches!(
        settled.holds[0].decision.choice,
        Choice::Buffer { .. }
    ));
    assert_eq!(settled.holds[0].decision.bytes, 50, "below the accumulator");
    validate_dag(&refs(&settled.roots), Some(cap(1))).expect("it validates under a cap of one");
}

#[test]
fn a_join_hold_in_a_root_that_waits_is_never_buffered_above_its_state() {
    // Each lineitem reader sits in its join's probe hold, and the cheapest edge is above the
    // accumulator. A join's hold allows a buffer there, but once the cap makes the second root
    // wait, the accumulator under that buffer would keep its state outside the cap.
    let trees: Vec<Box<dyn GpuNode>> = ["orders", "customer"]
        .iter()
        .map(|table| {
            unload(join(
                coalesce(key_of(table)),
                coalesce(filter(lineitem(&[0, 4], &[0]))),
            ))
        })
        .collect();
    let mut times = baseline(&trees, |_, _| 1);
    for query in &mut times.per_query {
        query.peak_bytes = 80;
        // unload, join, coalesce, build load, coalesce, filter, lineitem
        query.batch_bytes = vec![1, 1, 1, 1, 5, 50, 0];
    }
    let names: Vec<String> = ["a", "b"].map(String::from).to_vec();
    let loads = load_times(&[("a", 0, 70), ("b", 0, 70)]);
    let seqs = seqs_of(&times);
    // A budget of 135: one root of 80 at a time.
    let mut ledger = ledger(&trees, &times, &loads, &seqs, &names, 150);
    let fused = fused_with(&trees, &mut |f, h| ledger.choose(f, h));
    assert!(
        fused
            .holds
            .iter()
            .all(|hold| hold.decision.bytes == 5 && hold.hold == "GpuHashJoin"),
        "with no cap yet, each join's buffer sits above the accumulator"
    );
    let (settled, decision) = settle(fused, &times, &mut ledger).expect("the batch settles");
    assert_eq!(decision.cap, cap(1));
    let waiting: Vec<_> = settled.holds.iter().filter(|h| h.query == 1).collect();
    assert_eq!(
        waiting.len(),
        1,
        "the waiting root's reader is decided once"
    );
    assert!(matches!(waiting[0].decision.choice, Choice::Buffer { .. }));
    assert_eq!(waiting[0].decision.bytes, 50, "below the accumulator");
    assert_eq!(decision.estimate_bytes, 5 + 50 + 80);
    validate_dag(&refs(&settled.roots), Some(cap(1))).expect("it validates under a cap of one");
}
