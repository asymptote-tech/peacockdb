//! A recipe plan on a live GPU, driven by hand: the first time anything the engine plans
//! meets a device.
//!
//! `walk.rs` walks the tree making exactly the calls each recipe names, threading every
//! output handle into the next call's input and exporting at the root; one test per query
//! here, so a failure names the query rather than a stage, and `declared.rs` drives the
//! same walk for the schema catalog. No driver and no scheduling — every shape here plans
//! one batch per lane, so a recipe's own call order is the schedule. The oracle is
//! DataFusion on the same SQL: a golden would pin a wrong finalize on its first run, and
//! our CPU executor evaluates the very finalize the device is sent.

mod declared;
mod walk;

use datafusion::common::JoinType;

use self::walk::{context, walk};
use super::{FbKind, ProjectRole, Seq};
use crate::planner::{BatchSizing, PlanKnobs};

use crate::test_support::{GPU_BUDGET, assert_results_match, total_rows};

// The value the plan goldens are canonized at, so every shape below is one that tier
// already renders.
use crate::planner::SMALL_TABLE_BYTES;

/// Everything but the aggregates: one lane and one batch, which makes every recipe a
/// single call per node and the walk a straight line.
pub(crate) const ONE_LANE: PlanKnobs = PlanKnobs {
    target_partitions: 1,
    sizing: BatchSizing::OneBatchPerLane,
    budget: GPU_BUDGET as u64,
    small_table_bytes: SMALL_TABLE_BYTES,
};

/// The aggregates, at one batch and two lanes: a merge is the operator the engine adds and
/// one lane never performs one.
pub(crate) const TWO_LANES: PlanKnobs = PlanKnobs {
    target_partitions: 2,
    sizing: BatchSizing::OneBatchPerLane,
    budget: GPU_BUDGET as u64,
    small_table_bytes: SMALL_TABLE_BYTES,
};

/// The calls in the order they were made, `#14 CudfAggregate{Merge}` each. Three of the
/// four defects this file found were found by whoever had just written the code and the
/// fourth by a grep; neither is available to whoever runs it next, and a wrong table names
/// no call on its own.
fn trail(calls: &[(Seq, FbKind)]) -> String {
    calls
        .iter()
        .map(|(seq, kind)| format!("#{seq} {kind}"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// The walk's answer against DataFusion's on the same SQL, compared as sorted multisets
/// since a GPU join's output order is not deterministic. Returns the calls it made.
async fn assert_walk_matches_datafusion(sql: &str, knobs: PlanKnobs) -> Vec<(Seq, FbKind)> {
    let walked = walk(sql, knobs).await;
    // An exact compare of two empty results holds having compared nothing, so a query whose
    // predicate selected none would prove only that the walk did not crash.
    assert!(
        total_rows(&walked.batches) > 0,
        "the walk exported no rows for {sql}"
    );
    let expected = context(1)
        .await
        .sql(sql)
        .await
        .expect("the oracle plans it")
        .collect()
        .await
        .expect("the oracle runs it");
    assert_results_match(
        &expected,
        &walked.batches,
        None,
        &format!(
            "{sql}\n  the walk made: {}\n  and its",
            trail(&walked.calls)
        ),
    );
    walked.calls
}

fn times(calls: &[(Seq, FbKind)], kind: FbKind) -> usize {
    calls.iter().filter(|(_, made)| *made == kind).count()
}

/// Finalize projects whose call directly before them is `kind`. Which aggregate a finalize
/// belongs to is not in the counts — both branches emit one — so the adjacency is what
/// separates an init that finalized itself from a merge that did.
fn finalizing(calls: &[(Seq, FbKind)], kind: FbKind) -> usize {
    calls
        .windows(2)
        .filter(|pair| pair[0].1 == kind && pair[1].1 == FbKind::Project(ProjectRole::Finalize))
        .count()
}

/// The queries, named so the coverage read at the end of the file is over the very set the
/// tests above run rather than a second list that could drift from it.
const BARE_SCAN: &str = "SELECT * FROM nation";
const FILTER: &str = "SELECT o_orderkey, o_totalprice FROM orders WHERE o_totalprice > 500000";
const PROJECT_OVER_FILTER: &str =
    "SELECT c_custkey * 2 AS doubled FROM customer WHERE c_nationkey = 3";
const INNER_JOIN: &str =
    "SELECT n_name, r_name FROM nation JOIN region ON n_regionkey = r_regionkey";
const SEMI_JOIN: &str = "SELECT c_custkey FROM customer c WHERE c_nationkey = 3 AND EXISTS \
     (SELECT 1 FROM orders o WHERE o.o_custkey = c.c_custkey AND o.o_totalprice > c.c_acctbal)";
const SUM_BY_FLAG: &str =
    "SELECT l_returnflag, sum(l_quantity) FROM lineitem GROUP BY l_returnflag";
const AVG_BY_FLAG: &str =
    "SELECT l_returnflag, avg(l_quantity) FROM lineitem GROUP BY l_returnflag";
const MAX_OF_SUMS: &str = "SELECT max(per_flag) FROM \
     (SELECT l_returnflag, sum(l_quantity) AS per_flag FROM lineitem GROUP BY l_returnflag)";
const ROLLUP: &str = "SELECT l_returnflag, l_linestatus, sum(l_quantity) FROM lineitem \
     GROUP BY ROLLUP(l_returnflag, l_linestatus)";

#[tokio::test]
async fn a_bare_scan_comes_back_as_the_table() {
    assert_walk_matches_datafusion(BARE_SCAN, ONE_LANE).await;
}

#[tokio::test]
async fn a_filter_keeps_the_rows_its_predicate_names() {
    assert_walk_matches_datafusion(FILTER, ONE_LANE).await;
}

#[tokio::test]
async fn a_project_over_a_filter_evaluates_on_what_the_filter_left() {
    assert_walk_matches_datafusion(PROJECT_OVER_FILTER, ONE_LANE).await;
}

#[tokio::test]
async fn an_inner_join_matches_the_oracle_as_a_multiset() {
    assert_walk_matches_datafusion(INNER_JOIN, ONE_LANE).await;
}

/// A filtered semi join is the build-preserving type whose single probe batch takes the
/// legacy one-call form: the capability matrix makes the probe single-batch, so the one
/// call hands the build side over rather than needing a copy of it.
#[tokio::test]
async fn a_semi_join_that_keeps_its_build_side_runs_as_one_call() {
    let calls = assert_walk_matches_datafusion(SEMI_JOIN, ONE_LANE).await;
    assert_eq!(
        times(
            &calls,
            FbKind::HashJoin {
                join_type: JoinType::LeftSemi
            }
        ),
        1,
        "the join ran as something other than one LeftSemi call: {}",
        trail(&calls)
    );
}

/// Two lanes each merge their own state, the cross-lane merge folds them, and the finalize
/// runs once per lane at done — the first time `AggregateMode::Merge` meets a device.
#[tokio::test]
async fn each_lane_merges_its_own_state_before_the_cross_lane_merge_folds_them() {
    let calls = assert_walk_matches_datafusion(SUM_BY_FLAG, TWO_LANES).await;
    assert_eq!(
        times(&calls, FbKind::Aggregate { merge: true }),
        4,
        "two lanes below the shuffle and two above it merge state: {}",
        trail(&calls)
    );
    assert_eq!(
        times(&calls, FbKind::Project(ProjectRole::Finalize)),
        2,
        "the finalize runs once per lane, at done: {}",
        trail(&calls)
    );
}

/// `avg` finalizes by dividing a decimal by a count, and cuDF takes a divide's result
/// scale from its operands where arrow takes it from the declared output type — so a wrong
/// cast is invisible on a CPU host and wrong here, in a column whose type reads correctly
/// either way. The compare is on the rendered digits.
#[tokio::test]
async fn an_average_finalizes_to_the_digits_the_oracle_computes() {
    let calls = assert_walk_matches_datafusion(AVG_BY_FLAG, TWO_LANES).await;
    assert_eq!(
        times(&calls, FbKind::Project(ProjectRole::Finalize)),
        2,
        "the divide never reached the device: {}",
        trail(&calls)
    );
}

/// Two aggregates, one per branch of the rule. The inner one groups the whole table and
/// its lane holds many batches, so it takes the merge route and the merge carries the
/// finalize. The outer `max` reads that merge's one output batch, which is already the
/// whole of its single group, so the translation hands the finalize to the init itself and
/// no merge is built for it — the shape the goldens hold 39 of and no device had run.
///
/// The adjacency is the claim, not the counts: both branches emit one init and one
/// finalize, so a plan that lost the self-finalizing arm would still show two of each.
#[tokio::test]
async fn an_aggregate_whose_input_is_one_batch_finalizes_without_a_merge() {
    let calls = assert_walk_matches_datafusion(MAX_OF_SUMS, ONE_LANE).await;
    assert_eq!(
        finalizing(&calls, FbKind::Aggregate { merge: false }),
        1,
        "no init finalized itself, so the single-batch arm never ran: {}",
        trail(&calls)
    );
    assert_eq!(
        finalizing(&calls, FbKind::Aggregate { merge: true }),
        1,
        "the inner aggregate's merge stopped carrying its finalize: {}",
        trail(&calls)
    );
}

/// A ROLLUP is the one shape whose payload is not derivable from the plan line: the masks
/// and the per-position NULL placeholders decide how many groups come back. Dropping them
/// made this query refuse on the device rather than answer wrongly, because the state then
/// carried no `__grouping_id` and the merge read a state column as a key — a width
/// coincidence of this shape, not a property of the class. Where the widths line up, the
/// same omission answers with one grouping set out of three and looks right.
#[tokio::test]
async fn a_rollup_answers_with_every_grouping_set() {
    let calls = assert_walk_matches_datafusion(ROLLUP, ONE_LANE).await;
    assert_eq!(
        times(&calls, FbKind::Aggregate { merge: false }),
        1,
        "the sets are expanded once, by the init: {}",
        trail(&calls)
    );
}

/// Which fb kinds a device has now run, and which this walk refuses — the set T15 and T16
/// inherit as already proven, and the one they inherit as still open.
#[derive(Debug, PartialEq, Eq)]
enum Driven {
    Handled,
    /// Why the walk cannot make this call, in the words its panic uses.
    Refused(&'static str),
}

/// A match rather than a list: a kind added to the mapping stops this compiling, where a
/// list would go quietly short. The two facts a reader wants are here and nowhere else —
/// the spec's prose says the same thing and cannot go red.
fn driven(kind: FbKind) -> Driven {
    match kind {
        FbKind::Scan
        | FbKind::Filter
        | FbKind::PlainProject
        | FbKind::CoalescePartitions
        | FbKind::Repartition { .. }
        | FbKind::Aggregate { .. }
        | FbKind::Project(ProjectRole::Finalize) => Driven::Handled,
        FbKind::HashJoin { join_type } => match join_type {
            JoinType::Inner | JoinType::LeftSemi => Driven::Handled,
            _ => Driven::Refused("a join type no shape here plans"),
        },
        FbKind::Project(ProjectRole::ProbeKeys)
        | FbKind::Project(ProjectRole::NullPad { .. })
        | FbKind::Project(ProjectRole::Narrow) => {
            Driven::Refused("the finish pass accumulates probe keys across batches (#136)")
        }
        FbKind::Sort | FbKind::SortPreservingMerge => Driven::Refused("no shape here plans a sort"),
        FbKind::CrossJoin | FbKind::NestedLoopJoin => {
            Driven::Refused("both copy their build side, and the ABI has no copy (#152)")
        }
    }
}

/// The kinds the queries above put on a device. Checked against the walk both ways: a kind
/// here that no query produces is a claim on paper, and a kind produced that is not here is
/// an arm that quietly gained a shape.
const PROVEN: [FbKind; 10] = [
    FbKind::Scan,
    FbKind::Filter,
    FbKind::PlainProject,
    FbKind::CoalescePartitions,
    FbKind::Repartition { lanes: 2 },
    FbKind::Aggregate { merge: false },
    FbKind::Aggregate { merge: true },
    FbKind::Project(ProjectRole::Finalize),
    FbKind::HashJoin {
        join_type: JoinType::Inner,
    },
    FbKind::HashJoin {
        join_type: JoinType::LeftSemi,
    },
];

#[tokio::test]
async fn the_kinds_a_device_has_run_are_the_kinds_this_file_claims() {
    let mut made: Vec<FbKind> = Vec::new();
    for (sql, knobs) in [
        (BARE_SCAN, ONE_LANE),
        (FILTER, ONE_LANE),
        (PROJECT_OVER_FILTER, ONE_LANE),
        (INNER_JOIN, ONE_LANE),
        (SEMI_JOIN, ONE_LANE),
        (MAX_OF_SUMS, ONE_LANE),
        (ROLLUP, ONE_LANE),
        (SUM_BY_FLAG, TWO_LANES),
        (AVG_BY_FLAG, TWO_LANES),
    ] {
        for (_, kind) in walk(sql, knobs).await.calls {
            if !made.contains(&kind) {
                made.push(kind);
            }
        }
    }
    for kind in &made {
        assert_eq!(
            driven(*kind),
            Driven::Handled,
            "{kind} reached a device and is classified as refused"
        );
        assert!(
            PROVEN.contains(kind),
            "{kind} reached a device and is not in PROVEN"
        );
    }
    for kind in PROVEN {
        assert!(
            made.contains(&kind),
            "PROVEN claims {kind} and no query here produces it"
        );
    }
}
