use super::*;
use crate::plan::Batching;
use crate::planner::{BatchSizing, PlanKnobs, plan, translate};

const BUDGET: u64 = 2 * 1024 * 1024 * 1024;

async fn modelled(sql: &str, target_partitions: usize, budget: u64) -> MemoryModel {
    let data = crate::test_support::testdata_minimal_dir();
    let ctx = crate::register_tables_for(crate::build_session_state(target_partitions), &data)
        .await
        .expect("register the minimal tables");
    let plan = ctx
        .sql(sql)
        .await
        .expect("plan the query")
        .create_physical_plan()
        .await
        .expect("physical plan");
    let tree = translate(
        target_partitions,
        Batching::Sized {
            target_batch_bytes: 1 << 20,
        },
        &plan,
    )
    .expect("translate the plan");
    estimate(tree.as_ref(), budget).expect("estimate the plan")
}

/// Planned end to end at one of the three batching forms, which is what decides the
/// mapping the model then prices.
async fn modelled_as(sql: &str, target_partitions: usize, sizing: BatchSizing) -> MemoryModel {
    let data = crate::test_support::testdata_minimal_dir();
    let ctx = crate::register_tables_for(crate::build_session_state(target_partitions), &data)
        .await
        .expect("register the minimal tables");
    let physical = ctx
        .sql(sql)
        .await
        .expect("plan the query")
        .create_physical_plan()
        .await
        .expect("physical plan");
    plan(
        &physical,
        PlanKnobs {
            target_partitions,
            sizing,
            budget: BUDGET,
            small_table_bytes: 0,
        },
    )
    .expect("plan and estimate")
    .1
}

async fn refused(sql: &str, target_partitions: usize, budget: u64) -> PlanError {
    let data = crate::test_support::testdata_minimal_dir();
    let ctx = crate::register_tables_for(crate::build_session_state(target_partitions), &data)
        .await
        .expect("register the minimal tables");
    let plan = ctx
        .sql(sql)
        .await
        .unwrap()
        .create_physical_plan()
        .await
        .unwrap();
    let tree = translate(
        target_partitions,
        Batching::Sized {
            target_batch_bytes: 1 << 20,
        },
        &plan,
    )
    .expect("translate the plan");
    estimate(tree.as_ref(), budget).expect_err("this plan should not fit")
}

#[tokio::test]
async fn what_the_accumulators_hold_comes_off_the_budget_first() {
    let model = modelled(
        "SELECT c_nationkey, count(*) FROM customer GROUP BY c_nationkey",
        4,
        BUDGET,
    )
    .await;
    // The aggregate's state is held whatever the batch size, so it is spent before
    // anything is divided, and what is left is what the source may spend.
    assert!(model.accumulator_bytes > 0);
    assert_eq!(model.share_per_source, BUDGET - model.accumulator_bytes);
}

#[tokio::test]
async fn constants_that_cannot_be_less_and_exceed_the_budget_are_a_plan_time_error() {
    // A sort over a plain scan holds the whole table, and no estimate stands between
    // that number and the truth — which is the shape of a build side larger than vram.
    let err = refused("SELECT * FROM customer ORDER BY c_name", 1, 1 << 16).await;
    assert!(
        matches!(&err, PlanError::Invalid(what) if what.contains("no batch size")),
        "{err}"
    );
}

#[tokio::test]
async fn a_constant_that_rests_on_an_estimate_does_not_refuse_a_plan() {
    // An aggregate's state is one row per input row only because there is no
    // cardinality estimate (#19); tpch q1 groups six million rows into four. Refusing
    // on that would turn "we do not know" into "you cannot run this".
    let model = modelled(
        "SELECT c_nationkey, count(*) FROM customer GROUP BY c_nationkey",
        4,
        1 << 16,
    )
    .await;
    assert!(model.accumulator_bytes > model.budget);
    assert_eq!(model.certain_accumulator_bytes, 0);
    assert!(model.sources[0].target_batch_bytes > 0);
}

#[tokio::test]
async fn a_batch_is_charged_once_per_lane_in_force() {
    let one_lane = modelled("SELECT c_nationkey FROM customer", 1, BUDGET).await;
    let four_lanes = modelled("SELECT c_nationkey FROM customer", 4, BUDGET).await;
    // Four lanes hold four batches at once, so each may be a quarter the size.
    assert_eq!(
        four_lanes.sources[0].amplification,
        one_lane.sources[0].amplification * 4.0
    );
}

#[tokio::test]
async fn amplification_is_the_widest_point_on_the_path_not_either_end() {
    // The aggregate above the scan is where a batch is widest — its output row carries
    // a key and a count where the input row carried a key — and that is what sizes the
    // source, though it is neither end of the walk.
    let plain = modelled("SELECT c_nationkey FROM customer", 4, BUDGET).await;
    let widened = modelled(
        "SELECT c_nationkey, count(*) FROM customer GROUP BY c_nationkey",
        4,
        BUDGET,
    )
    .await;
    assert!(
        widened.sources[0].amplification > plain.sources[0].amplification,
        "the widening above the source did not size it: {} vs {}",
        widened.sources[0].amplification,
        plain.sources[0].amplification
    );
}

#[tokio::test]
async fn an_accumulator_ends_the_walk() {
    // Everything above the aggregate's merge is served by what the merge emits, so a
    // project up there cannot change what the source may spend.
    let bare = modelled(
        "SELECT c_nationkey, count(*) AS n FROM customer GROUP BY c_nationkey",
        4,
        BUDGET,
    )
    .await;
    let with_project = modelled(
        "SELECT c_nationkey, count(*) * 1000 AS n FROM customer GROUP BY c_nationkey",
        4,
        BUDGET,
    )
    .await;
    assert_eq!(bare.sources, with_project.sources);
}

#[tokio::test]
async fn two_sources_get_equal_shares_rather_than_proportional_ones() {
    let model = modelled(
        "SELECT c.c_name, s.s_name FROM customer c JOIN supplier s ON c.c_nationkey = s.s_nationkey",
        4,
        BUDGET,
    )
    .await;
    // customer is 150k rows and supplier 10k; a proportional split would hand the most
    // budget to the source already producing the most bytes. Both get the same share,
    // and what separates their targets afterwards is their own size.
    assert_eq!(model.sources.len(), 2);
    assert_eq!(
        model.share_per_source,
        (BUDGET - model.accumulator_bytes) / 2
    );
    for source in &model.sources {
        assert!(
            source.target_batch_bytes as f64 <= model.share_per_source as f64,
            "a source was given more than its share"
        );
    }
}

#[tokio::test]
async fn a_target_lands_on_the_coarse_grid() {
    // The share these sources can afford is far above what they hold, so each target
    // is the source's own size: a batch is never larger than what there is to read.
    let capped = modelled(
        "SELECT c.c_name, s.s_name FROM customer c JOIN supplier s ON c.c_nationkey = s.s_nationkey",
        4,
        BUDGET,
    )
    .await;
    for source in &capped.sources {
        assert!(source.target_batch_bytes < capped.share_per_source);
    }
    // Where the budget is what binds, the target lands on the grid — powers of two,
    // so an estimate that drifts slightly does not regenerate every golden.
    let bound = modelled("SELECT * FROM customer", 4, 64 * 1024 * 1024).await;
    let target = bound.sources[0].target_batch_bytes;
    assert!(target.is_power_of_two(), "{target} is not on the grid");
    assert!(target >= MIN_TARGET_BATCH_BYTES);
}

#[tokio::test]
async fn a_build_preserving_join_is_charged_for_the_keys_it_accumulates() {
    // The finish pass holds the key columns of every probe row it has seen (#136),
    // and the small table on the left is what keeps this a Left join — DataFusion
    // swaps the sides, and remaps the type, when the right one is smaller.
    let left = modelled(
        "SELECT s.s_name, c.c_name FROM supplier s LEFT JOIN customer c ON s.s_nationkey = c.c_nationkey",
        4,
        BUDGET,
    )
    .await;
    let inner = modelled(
        "SELECT s.s_name, c.c_name FROM supplier s JOIN customer c ON s.s_nationkey = c.c_nationkey",
        4,
        BUDGET,
    )
    .await;
    assert!(
        left.accumulator_bytes > inner.accumulator_bytes,
        "a streamed probe under a build-preserving join costs nothing: {} vs {}",
        left.accumulator_bytes,
        inner.accumulator_bytes
    );
}

#[tokio::test]
async fn a_loader_is_priced_by_the_batches_its_mapping_makes() {
    // Three batching forms, one budget: what a loader holds is the largest batch the
    // mapping produces, so per-row-group holds a row group where one-batch-per-lane
    // holds the lane. A model reading the budget's target instead would say the same
    // number for all three.
    let per_lane = modelled_as("SELECT * FROM customer", 1, BatchSizing::OneBatchPerLane).await;
    let per_group = modelled_as(
        "SELECT * FROM customer",
        1,
        BatchSizing::OneBatchPerRowGroup,
    )
    .await;
    assert!(
        per_group.resident[0] < per_lane.resident[0],
        "per-row-group holds {} where per-lane holds {}",
        per_group.resident[0],
        per_lane.resident[0]
    );
}

#[tokio::test]
async fn a_limit_bounds_what_the_nodes_above_it_hold() {
    let limited = modelled(
        "SELECT c_name FROM (SELECT * FROM customer WHERE c_nationkey > 1 LIMIT 10) t \
         ORDER BY c_name",
        1,
        BUDGET,
    )
    .await;
    let whole = modelled(
        "SELECT c_name FROM customer WHERE c_nationkey > 1 ORDER BY c_name",
        1,
        BUDGET,
    )
    .await;
    assert!(
        limited.accumulator_bytes < whole.accumulator_bytes,
        "the sort above a limit accumulates the whole table: {} vs {}",
        limited.accumulator_bytes,
        whole.accumulator_bytes
    );
}
