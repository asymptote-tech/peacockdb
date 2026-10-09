//! Whether a column of a logical plan can hold a NULL, traced to the parquet it reads.
//!
//! The `NOT IN` rewrite and #137's key filters both ask this before the physical plan
//! exists, and every corpus column is declared nullable, so the declared schema answers
//! nothing: the trace has to reach a scan and read the footers. These cases are against
//! real files for that reason — the hand-built rules of the GpuNode analysis are
//! `null_analysis.rs`. One case is over tpcds, whose `ss_sold_date_sk` holds NULLs where
//! every tpch key holds none: without it a reader that always answered "no NULL" would
//! pass every other case here.

use datafusion::common::Column;
use datafusion::execution::context::SessionContext;
use datafusion::logical_expr::LogicalPlan;

use crate::planner::nullability::logical_can_be_null;
use crate::test_support::data_dir_for;

async fn ctx_for(dataset: &str) -> SessionContext {
    crate::register_tables_for(crate::build_session_state(4), &data_dir_for(dataset, "1"))
        .await
        .expect("register the tables")
}

async fn optimized(ctx: &SessionContext, sql: &str) -> LogicalPlan {
    ctx.sql(sql)
        .await
        .expect("plan the query")
        .into_optimized_plan()
        .expect("optimize the plan")
}

#[tokio::test]
async fn a_key_whose_row_groups_hold_no_null_is_not_nullable() {
    let ctx = ctx_for("tpch").await;
    let plan = optimized(&ctx, "SELECT o_custkey FROM orders").await;
    assert!(!logical_can_be_null(&plan, &Column::from_name("o_custkey")));
}

#[tokio::test]
async fn a_key_whose_row_groups_hold_a_null_is_nullable() {
    let ctx = ctx_for("tpcds").await;
    let plan = optimized(&ctx, "SELECT ss_sold_date_sk FROM store_sales").await;
    assert!(logical_can_be_null(
        &plan,
        &Column::from_name("ss_sold_date_sk")
    ));
}

#[tokio::test]
async fn a_column_through_an_aggregate_counts_as_nullable() {
    let ctx = ctx_for("tpch").await;
    let plan = optimized(&ctx, "SELECT max(o_custkey) AS m FROM orders").await;
    assert!(logical_can_be_null(&plan, &Column::from_name("m")));
}

#[tokio::test]
async fn a_key_read_through_a_table_alias_is_traced() {
    // `FROM orders o` puts a SubqueryAlias over the scan; it must not end the trace.
    let ctx = ctx_for("tpch").await;
    let plan = optimized(&ctx, "SELECT o.o_custkey FROM orders o").await;
    assert!(!logical_can_be_null(&plan, &Column::from_name("o_custkey")));
}

#[tokio::test]
async fn an_outer_join_pads_and_so_nullable() {
    let ctx = ctx_for("tpch").await;
    let plan = optimized(
        &ctx,
        "SELECT c_custkey, o_custkey FROM customer LEFT JOIN orders ON c_custkey = o_custkey",
    )
    .await;
    assert!(logical_can_be_null(&plan, &Column::from_name("o_custkey")));
    assert!(!logical_can_be_null(&plan, &Column::from_name("c_custkey")));
}
