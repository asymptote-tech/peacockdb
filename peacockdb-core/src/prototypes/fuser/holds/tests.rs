//! The hold rule: where a buffer goes, what it costs, when a held reader is buffered and when
//! it reads its table again, and its inputs read off a fused plan, the baseline and
//! `loads.tsv`.

use crate::plan::GpuNode;
use crate::prototypes::fuser::tests::{baseline, coalesce, filter, fused_with, join, key_of};
use crate::prototypes::fuser::tests::{lineitem, load_times, re_read_every_hold, unload};
use crate::prototypes::fuser::{Baseline, Choice, EdgeCost, HoldDecision, HoldInputs, HoldLedger};
use crate::prototypes::fuser::{Headroom, LoadTimes, decide, hold_inputs, place_buffer};
use crate::test_support::parse_benchmark_section;

fn edge(at: usize, bytes: u64, allowed: bool) -> EdgeCost {
    EdgeCost {
        edge: (at, 0),
        bytes,
        allowed,
    }
}

fn inputs(edges: Vec<EdgeCost>) -> HoldInputs {
    HoldInputs {
        reread_us: 700,
        reread_bytes: 100,
        edges,
    }
}

#[test]
fn the_buffer_goes_on_the_smallest_recorded_edge() {
    let edges = [edge(1, 100, true), edge(2, 20, true), edge(3, 50, true)];
    assert_eq!(place_buffer(&edges).map(|e| e.edge), Some((2, 0)));
}

#[test]
fn a_tie_goes_to_the_lower_edge() {
    let edges = [edge(1, 20, true), edge(2, 20, true)];
    assert_eq!(place_buffer(&edges).map(|e| e.edge), Some((1, 0)));
}

#[test]
fn a_disallowed_edge_is_never_placed() {
    let edges = [edge(1, 100, true), edge(2, 20, false)];
    assert_eq!(place_buffer(&edges).map(|e| e.edge), Some((1, 0)));
    assert!(place_buffer(&[edge(1, 1, false)]).is_none());
}

#[test]
fn a_buffer_that_fits_is_chosen() {
    let decision = decide(&inputs(vec![edge(4, 70, true)]), Headroom::Left(100));
    assert_eq!(
        decision,
        HoldDecision {
            choice: Choice::Buffer { edge: (4, 0) },
            bytes: 70,
            reread_us: 700,
            why: "fits: 70 <= 100".to_string(),
        }
    );
}

#[test]
fn a_buffer_that_does_not_fit_means_a_re_read() {
    let decision = decide(&inputs(vec![edge(4, 70, true)]), Headroom::Left(60));
    assert_eq!(decision.choice, Choice::ReRead);
    assert_eq!((decision.bytes, decision.reread_us), (70, 700));
    assert_eq!(decision.why, "too large: 70 > 60");
}

#[test]
fn over_budget_a_buffer_smaller_than_its_re_read_is_chosen() {
    let decision = decide(&inputs(vec![edge(4, 70, true)]), Headroom::Over);
    assert_eq!(decision.choice, Choice::Buffer { edge: (4, 0) });
    assert_eq!((decision.bytes, decision.reread_us), (70, 700));
    assert_eq!(decision.why, "over budget: 70 < re-read 100");
}

#[test]
fn over_budget_a_buffer_no_smaller_than_its_re_read_means_a_re_read() {
    let mut equal = inputs(vec![edge(4, 70, true)]);
    equal.reread_bytes = 70;
    let decision = decide(&equal, Headroom::Over);
    assert_eq!(decision.choice, Choice::ReRead);
    assert_eq!(decision.why, "over budget: 70 >= re-read 70");
}

#[test]
fn over_budget_no_allowed_edge_means_a_re_read() {
    let decision = decide(&inputs(vec![edge(4, 5, false)]), Headroom::Over);
    assert_eq!(decision.choice, Choice::ReRead);
    assert!(decision.why.starts_with("no edge: "), "{}", decision.why);
}

#[test]
fn no_allowed_edge_means_a_re_read() {
    let decision = decide(&inputs(vec![edge(4, 70, false)]), Headroom::Left(1 << 40));
    assert_eq!(decision.choice, Choice::ReRead);
    assert_eq!(
        decision.why,
        "no edge: each lies above a join that grew the rows, or above state a waiting root \
         may not keep"
    );
    let direct = decide(&inputs(Vec::new()), Headroom::Left(1 << 40));
    assert_eq!(direct.choice, Choice::ReRead);
    assert_eq!(
        direct.why,
        "no edge: the reader takes the shared load directly"
    );
}

/// `unload(J(coalesce(orders), J2(coalesce(customer), filter(lineitem))))` beside
/// `unload(filter(lineitem))`. q0's pre-order: unload 0, J 1, coalesce 2, orders 3, J2 4,
/// coalesce 5, customer 6, filter 7, lineitem 8; q1's: unload 0, filter 1, lineitem 2.
fn nested_joins() -> Vec<Box<dyn GpuNode>> {
    vec![
        unload(join(
            coalesce(key_of("orders")),
            join(
                coalesce(key_of("customer")),
                filter(lineitem(&[0, 4], &[0])),
            ),
        )),
        unload(filter(lineitem(&[0, 4], &[0]))),
    ]
}

const LOADS: [(&str, u64, u64); 2] = [("held", 3, 700), ("free", 1, 900)];

fn names() -> Vec<String> {
    vec!["held".to_string(), "free".to_string()]
}

/// The loads' seqs: q0's lineitem 3 (its others 0), q1's lineitem 1.
fn seqs(times: &Baseline) -> Vec<Vec<Option<u64>>> {
    let mut seqs: Vec<Vec<Option<u64>>> = times
        .per_query
        .iter()
        .map(|query| {
            let loads = query.names.iter().map(|name| name == "GpuLoadParquet");
            loads.map(|is_load| is_load.then_some(0)).collect()
        })
        .collect();
    let last = seqs[0].len() - 1;
    seqs[0][last] = Some(3);
    let last = seqs[1].len() - 1;
    seqs[1][last] = Some(1);
    seqs
}

/// The inputs of the first held reader fusing `trees` names.
fn first_inputs(trees: &[Box<dyn GpuNode>], times: &Baseline, loads: &LoadTimes) -> HoldInputs {
    let (seqs, names) = (seqs(times), names());
    let mut first = None;
    fused_with(trees, &mut |fused, held| {
        first.get_or_insert_with(|| hold_inputs(fused, held, times, loads, &seqs, &names));
        re_read_every_hold(fused, held)
    });
    first.expect("a held reader")
}

/// q0's figures by pre-order: `(index, batch_rows, batch_bytes)`.
fn nested_times(trees: &[Box<dyn GpuNode>], figures: &[(usize, u64, u64)]) -> Baseline {
    let mut times = baseline(trees, |_, _| 1);
    for (at, rows, bytes) in figures {
        times.per_query[0].batch_rows[*at] = *rows;
        times.per_query[0].batch_bytes[*at] = *bytes;
    }
    times
}

#[test]
fn a_buffer_above_a_join_charges_its_build() {
    let trees = nested_joins();
    let times = nested_times(&trees, &[(7, 5, 50), (4, 5, 30), (5, 1, 40)]);
    let inputs = first_inputs(&trees, &times, &load_times(&LOADS));
    let costs: Vec<(u64, bool)> = inputs.edges.iter().map(|e| (e.bytes, e.allowed)).collect();
    assert_eq!(costs, [(50, true), (30 + 40, true)]);
    assert_eq!(inputs.reread_us, 700);
}

#[test]
fn a_buffer_stays_below_a_join_that_grew_the_rows() {
    let trees = nested_joins();
    let times = nested_times(&trees, &[(7, 5, 50), (4, 10, 20), (5, 1, 0)]);
    let inputs = first_inputs(&trees, &times, &load_times(&LOADS));
    let costs: Vec<(u64, bool)> = inputs.edges.iter().map(|e| (e.bytes, e.allowed)).collect();
    assert_eq!(costs, [(50, true), (20, false)]);
    let placed = place_buffer(&inputs.edges).expect("the edge below the join");
    assert_eq!(placed.bytes, 50);
}

/// q0 `unload(join(coalesce(orders), filter(lineitem)))` and q1 `unload(filter(lineitem))`,
/// as the sf40 benchmark tree records them.
const HELD: &str = "\
GpuUnload
  time_us=[[5]] total_us=5 batch_rows=[[1]] batch_bytes=[[17]]
  GpuHashJoin: on=x
    time_us=[[9]] total_us=9 batch_rows=[[30]] batch_bytes=[[240]]
    GpuCoalesceAllBatches
      time_us=[[2]] total_us=2 batch_rows=[[50]] batch_bytes=[[400]]
      GpuLoadParquet: table=orders
        time_us=[[60]] total_us=60 batch_rows=[[50]] batch_bytes=[[400]]
    GpuFilter: predicate=x
      time_us=[[10]] total_us=10 batch_rows=[[40]] batch_bytes=[[320]]
      GpuLoadParquet: table=lineitem
        time_us=[[100]] total_us=100 batch_rows=[[90]] batch_bytes=[[1440]]
--- run ---
load_us=160
execute_us=26
peak_bytes=2000
";

const FREE: &str = "\
GpuUnload
  time_us=[[5]] total_us=5 batch_rows=[[1]] batch_bytes=[[17]]
  GpuFilter: predicate=x
    time_us=[[10]] total_us=10 batch_rows=[[40]] batch_bytes=[[320]]
    GpuLoadParquet: table=lineitem
      time_us=[[100]] total_us=100 batch_rows=[[90]] batch_bytes=[[1440]]
--- run ---
load_us=100
execute_us=15
peak_bytes=1500
";

#[test]
fn hold_inputs_read_the_baseline_and_loads_tsv() {
    let sections = [HELD, FREE].map(|text| parse_benchmark_section(text).expect("timed"));
    let times = Baseline::of(&sections);
    let loads = load_times(&[("held", 3, 700), ("free", 1, 900)]);
    // Same columns: q0's filter reads the shared load directly, an edge nothing recorded.
    let same = crate::prototypes::fuser::tests::held_pair(&[0, 4], &[0, 4]);
    let direct = first_inputs(&same, &times, &loads);
    let costs: Vec<(u64, bool)> = direct.edges.iter().map(|e| (e.bytes, e.allowed)).collect();
    assert_eq!((costs, direct.reread_us), (vec![(320, true)], 700));
    assert_eq!(
        direct.reread_bytes, 1440,
        "a re-read adds q0's own lineitem load to its root"
    );
    // Other columns: the edge above q0's inserted project carries its own load's bytes.
    let other = crate::prototypes::fuser::tests::held_pair(&[0, 4], &[0, 5]);
    let selected = first_inputs(&other, &times, &loads);
    let costs: Vec<u64> = selected.edges.iter().map(|e| e.bytes).collect();
    assert_eq!(costs, [1440, 320]);
}

#[test]
fn the_ledger_spends_headroom_in_order() {
    let trees: Vec<Box<dyn GpuNode>> = vec![
        unload(join(
            coalesce(key_of("orders")),
            filter(lineitem(&[0, 4], &[0])),
        )),
        unload(join(
            coalesce(key_of("customer")),
            filter(lineitem(&[0, 4], &[0])),
        )),
        unload(filter(lineitem(&[0, 4], &[0]))),
    ];
    let mut times = baseline(&trees, |_, _| 1);
    for query in &mut times.per_query[..2] {
        // unload, join, coalesce, build load, filter, lineitem
        query.batch_bytes = vec![0, 0, 0, 0, 60, 0];
    }
    let names: Vec<String> = ["a", "b", "c"].map(String::from).to_vec();
    let loads = load_times(&[("a", 0, 70), ("b", 0, 80), ("c", 0, 90)]);
    let seqs: Vec<Vec<Option<u64>>> = times
        .per_query
        .iter()
        .map(|q| {
            q.names
                .iter()
                .map(|n| (n == "GpuLoadParquet").then_some(0))
                .collect()
        })
        .collect();
    // 111 less its 10% margin is 100, all of it headroom: nothing shared or peaking is charged.
    let mut ledger = HoldLedger::new(111, &times, &loads, &seqs, &names);
    assert_eq!(ledger.pool_bytes, 111);
    let fused = fused_with(&trees, &mut |f, h| ledger.choose(f, h));
    let made: Vec<(Choice, &str)> = fused
        .holds
        .iter()
        .map(|record| (record.decision.choice, record.decision.why.as_str()))
        .collect();
    assert!(matches!(
        made[0],
        (Choice::Buffer { .. }, "fits: 60 <= 100")
    ));
    assert_eq!(made[1], (Choice::ReRead, "too large: 60 > 40"));
    assert_eq!(fused.holds[1].decision.reread_us, 80);
}
