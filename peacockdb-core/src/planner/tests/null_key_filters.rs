//! #137: a NULL key dropped before the shuffle it cannot match through.
//!
//! Over real datasets, because the footers decide the rule and not the declared types —
//! every corpus column is declared nullable. pbench's `f_k` holds 1,034 NULLs and `d_k` 50
//! where every tpch scan key holds none, which is the pair these cases read against;
//! `join_capability.rs` writes its own parquet and cannot ask this at all.
//!
//! The nine-way table of which side may drop is `plan::tests::joins`'s. Here: the three
//! answers a query shows, one lane showing that no shuffle means no filter, and two over the
//! hand-built fixture for what no corpus SQL reaches — the flag, and the filter's position.

use std::sync::Arc;

use datafusion::common::JoinType;
use datafusion::physical_expr::expressions::Column;
use datafusion::physical_plan::ExecutionPlan;
use datafusion::physical_plan::joins::{HashJoinExec, PartitionMode};

use crate::plan_text::render_plan;
use crate::test_support::{MODES, Mode, data_dir_for, queries_dir_for};
use crate::tests::join_fixture::{Fixture, planned};

/// The plan text of `sql` over a dataset at one of the modes, with that dataset's knobs —
/// pbench's put the small-table rule at zero, so `fact` and `dim` really shuffle.
async fn text_at(mode: &Mode, dataset: &str, sql: &str) -> String {
    let ctx = crate::register_tables_for(
        crate::build_session_state(mode.target_partitions),
        &data_dir_for(dataset, "1"),
    )
    .await
    .expect("register the tables");
    let physical = ctx
        .sql(sql)
        .await
        .expect("the query plans")
        .create_physical_plan()
        .await
        .expect("physical plan");
    let (root, _) =
        crate::planner::plan(&physical, mode.knobs_for(dataset)).expect("this mode plans it");
    render_plan(root.as_ref())
}

#[tokio::test]
async fn an_inner_join_drops_null_keys_below_both_shuffles_where_the_data_holds_them() {
    let plan = text_at(
        &MODES[2],
        "pbench",
        "SELECT f_id, d_id FROM fact JOIN dim ON f_k = d_k",
    )
    .await;
    assert_eq!(plan.matches("IS NOT NULL").count(), 2, "{plan}");
    assert!(plan.contains("f_k@") && plan.contains("d_k@"), "{plan}");
}

#[tokio::test]
async fn a_left_join_drops_null_keys_on_its_probe_side_only() {
    // Left preserves the build (dim): its NULL-key rows are owed padded, so only fact's go.
    let plan = text_at(
        &MODES[2],
        "pbench",
        "SELECT d_id, f_id FROM dim LEFT JOIN fact ON d_k = f_k",
    )
    .await;
    let filters: Vec<&str> = plan.lines().filter(|l| l.contains("IS NOT NULL")).collect();
    assert_eq!(filters.len(), 1, "{plan}");
    assert!(filters[0].contains("f_k@"), "{plan}");
}

#[tokio::test]
async fn a_full_join_drops_no_null_key() {
    let plan = text_at(
        &MODES[2],
        "pbench",
        "SELECT d_id, f_id FROM dim FULL JOIN fact ON d_k = f_k",
    )
    .await;
    assert!(!plan.contains("IS NOT NULL"), "{plan}");
}

#[tokio::test]
async fn a_tpch_join_gains_no_filter() {
    // o_custkey and c_custkey hold no NULL in any row group, so q3's shuffles stay as they
    // are — which is what makes the footer reader load-bearing rather than decorative.
    let sql = std::fs::read_to_string(queries_dir_for("tpch").join("q3.sql")).expect("q3");
    let plan = text_at(&MODES[2], "tpch", &sql).await;
    assert!(!plan.contains("IS NOT NULL"), "{plan}");
}

#[tokio::test]
async fn one_lane_shuffles_nothing_and_so_drops_nothing() {
    // The rule reads the TRANSLATED side: with one lane there is no `GpuEmitPartitions`, so
    // there is no shuffle to skew and nothing to drop, however many NULLs the key holds.
    let plan = text_at(
        &MODES[0],
        "pbench",
        "SELECT f_id, d_id FROM fact JOIN dim ON f_k = d_k",
    )
    .await;
    assert!(!plan.contains("IS NOT NULL"), "{plan}");
}

/// An Inner join of two hash-scattered sides whose key column holds NULLs, built by hand
/// because `null_equals_null` is a flag no SQL in the corpus sets.
async fn scattered_inner_join(null_equals_null: bool) -> String {
    let fixture = Fixture::new(&format!("null-keys-{null_equals_null}")).await;
    let join: Arc<dyn ExecutionPlan> = Arc::new(
        HashJoinExec::try_new(
            fixture.scattered("nulls").await,
            fixture.scattered("nulls").await,
            vec![(Arc::new(Column::new("k", 0)), Arc::new(Column::new("k", 0)))],
            None,
            &JoinType::Inner,
            None,
            PartitionMode::Partitioned,
            null_equals_null,
        )
        .expect("a hash join"),
    );
    render_plan(planned(&join).expect("the join plans").as_ref())
}

#[tokio::test]
async fn set_semantics_keeps_the_null_keys_a_shuffle_would_skew() {
    // `null_equals_null = true` asks for NULL to match NULL, so a NULL-key row is a row the
    // answer needs and #137 must leave it alone. No corpus query sets the flag on a
    // nullable key, so nothing but this case can hold the guard.
    let plan = scattered_inner_join(true).await;
    assert!(!plan.contains("IS NOT NULL"), "{plan}");
}

#[tokio::test]
async fn the_filter_goes_below_the_shuffle_and_not_above_it() {
    // Filtering after the scatter would leave every NULL already piled into one lane, which
    // is the skew the rule exists to avoid. Only the position says which of the two it did.
    let plan = scattered_inner_join(false).await;
    let lines: Vec<&str> = plan.lines().collect();
    let emit = lines
        .iter()
        .position(|line| line.contains("GpuEmitPartitions"))
        .expect("a shuffled side");
    assert!(
        lines[emit + 1].contains("GpuFilter: predicate=k@0 IS NOT NULL"),
        "{plan}"
    );
}
