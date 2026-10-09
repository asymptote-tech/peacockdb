//! `NOT IN` answers as SQL: the rewrite's forms, the folds, and what stays refused.
//!
//! Every expected row here is DuckDB 1.5.4's over the same two tables, so the cases say what
//! SQL means rather than what DataFusion does — which is the point: DataFusion 45 answers
//! seven rows where SQL gives three (#80). The tables are in memory, so the nullability
//! trace cannot reach a parquet footer and every operand counts as possibly-NULL; that is
//! the conservative side, and it is what makes these the rewrite's cases. The tpch cases
//! are the other side: real footers, no NULL, no rewrite.

use datafusion::arrow::array::{Array, AsArray};
use datafusion::arrow::datatypes::Int32Type;
use datafusion::execution::context::SessionContext;
use datafusion::logical_expr::LogicalPlan;

use crate::test_support::{data_dir_for, queries_dir_for};

const SETUP: &[&str] = &[
    "create table o(w int, x int) as values (1,1),(1,2),(1,null),(2,1),(2,null),(3,5),(null,1),(4,null)",
    "create table s(z int, y int) as values (1,1),(1,3),(2,null),(2,7),(4,8)",
];

async fn fixture() -> SessionContext {
    let ctx = crate::build_session_state(1);
    for statement in SETUP {
        ctx.sql(statement)
            .await
            .expect("the fixture statement plans")
            .collect()
            .await
            .expect("the fixture statement runs");
    }
    ctx
}

/// The `(w, x)` rows of `sql` over the fixture, sorted so the comparison is order-free.
async fn rows(sql: &str) -> Vec<(Option<i32>, Option<i32>)> {
    let ctx = fixture().await;
    let batches = ctx
        .sql(sql)
        .await
        .expect("the query plans")
        .collect()
        .await
        .expect("the query runs");
    let mut out = Vec::new();
    for batch in &batches {
        let w = batch.column(0).as_primitive::<Int32Type>();
        let x = batch.column(1).as_primitive::<Int32Type>();
        for i in 0..batch.num_rows() {
            out.push((
                w.is_valid(i).then(|| w.value(i)),
                x.is_valid(i).then(|| x.value(i)),
            ));
        }
    }
    out.sort();
    out
}

/// The total row count of `sql`, for a shape whose columns these cases do not read.
async fn row_count(sql: &str) -> usize {
    let ctx = fixture().await;
    let batches = ctx
        .sql(sql)
        .await
        .expect("the query plans")
        .collect()
        .await
        .expect("the query runs");
    batches.iter().map(|batch| batch.num_rows()).sum()
}

/// Why `sql` was refused, from whichever stage refused it: SQL-to-logical in `sql`, the
/// optimizer and physical planning in `collect`.
async fn refusal(sql: &str) -> String {
    let ctx = fixture().await;
    match ctx.sql(sql).await {
        Err(e) => e.to_string(),
        Ok(frame) => frame
            .collect()
            .await
            .expect_err("refused while planning")
            .to_string(),
    }
}

/// A corpus query's optimized logical plan over tpch sf1 — real footers, so the trace
/// decides from the data.
async fn tpch_plan(query: &str) -> LogicalPlan {
    let ctx = crate::register_tables_for(crate::build_session_state(1), &data_dir_for("tpch", "1"))
        .await
        .expect("register the tpch tables");
    let sql = std::fs::read_to_string(queries_dir_for("tpch").join(format!("{query}.sql")))
        .expect("the query text");
    ctx.sql(&sql)
        .await
        .expect("the query plans")
        .into_optimized_plan()
        .expect("optimize the plan")
}

#[tokio::test]
async fn correlated_not_in_answers_as_sql() {
    assert_eq!(
        rows("select * from o where x not in (select y from s where s.z = o.w)").await,
        vec![(None, Some(1)), (Some(1), Some(2)), (Some(3), Some(5))]
    );
}

#[tokio::test]
async fn uncorrelated_not_in_answers_as_sql() {
    assert_eq!(
        rows("select * from o where x not in (select y from s where z <> 2)").await,
        vec![(Some(1), Some(2)), (Some(3), Some(5))]
    );
}

#[tokio::test]
async fn correlated_not_in_under_or_answers_as_sql() {
    // No row has w = 99, so the OR's other arm decides alone: the same three rows.
    assert_eq!(
        rows("select * from o where w = 99 or x not in (select y from s where s.z = o.w)").await,
        vec![(None, Some(1)), (Some(1), Some(2)), (Some(3), Some(5))]
    );
}

#[tokio::test]
async fn uncorrelated_not_in_under_or_answers_as_sql() {
    assert_eq!(
        rows("select * from o where w = 99 or x not in (select y from s where z <> 2)").await,
        vec![(Some(1), Some(2)), (Some(3), Some(5))]
    );
}

#[tokio::test]
async fn a_negated_positive_in_on_the_spine_answers_as_sql() {
    // NOT (x IN S) reaches DataFusion as Not(InSubquery{negated: false}); the rule normalizes it.
    assert_eq!(
        rows("select * from o where not (x in (select y from s where z <> 2))").await,
        vec![(Some(1), Some(2)), (Some(3), Some(5))]
    );
}

#[tokio::test]
async fn a_negated_positive_in_under_or_answers_as_sql() {
    assert_eq!(
        rows("select * from o where w = 0 or not (x in (select y from s where s.z = o.w))").await,
        vec![(None, Some(1)), (Some(1), Some(2)), (Some(3), Some(5))]
    );
}

#[tokio::test]
async fn a_not_in_under_not_folds_to_a_positive_in() {
    // NNF: NOT (x NOT IN S) is x IN S, a semi join that needs no rewrite.
    assert_eq!(
        rows("select * from o where not (x not in (select y from s where z <> 2))").await,
        vec![(None, Some(1)), (Some(1), Some(1)), (Some(2), Some(1))]
    );
}

#[tokio::test]
async fn de_morgan_reaches_a_not_in_under_a_negated_or() {
    // NOT (w = 1 OR x NOT IN S) is w <> 1 AND x IN S. S (z <> 2) = {1, 3, 8}. The row whose w
    // is NULL is dropped: NULL AND true is NULL.
    assert_eq!(
        rows("select * from o where not (w = 1 or x not in (select y from s where z <> 2))").await,
        vec![(Some(2), Some(1))]
    );
}

// #250 — a mark join never yields NULL, so `(x IN S) IS NULL` cannot be answered by one.
#[tokio::test]
async fn bug_an_in_whose_null_is_read_is_refused() {
    let why = refusal("select * from o where (x in (select y from s)) is null").await;
    assert!(why.contains("#250"), "{why}");
}

#[tokio::test]
async fn a_not_in_nested_in_an_exists_is_rewritten_too() {
    // The rule walks subqueries' plans, so the inner NOT IN is rewritten before DataFusion
    // decorrelates the EXISTS around it.
    assert_eq!(
        rows(
            "select * from o where exists (select 1 from s where s.z = o.w \
             and s.y not in (select o2.x from o o2 where o2.w = 3))"
        )
        .await,
        vec![
            (Some(1), None),
            (Some(1), Some(1)),
            (Some(1), Some(2)),
            (Some(2), None),
            (Some(2), Some(1)),
            (Some(4), None)
        ]
    );
}

// The plan's nested case answers the same before and after the rewrite, so it cannot show the
// walk reaching inside a subquery. This one can: the inner set holds a NULL, so SQL's
// `s.y NOT IN (...)` is never true and the EXISTS is empty, where a two-valued answer keeps six.
#[tokio::test]
async fn a_nested_not_in_over_a_set_holding_a_null_empties_its_exists() {
    assert_eq!(
        rows(
            "select * from o where exists (select 1 from s where s.z = o.w \
             and s.y not in (select o2.x from o o2 where o2.w = 1))"
        )
        .await,
        vec![]
    );
}

#[tokio::test]
async fn an_uncorrelated_rewrite_over_an_unfiltered_subquery_counts_rows_itself() {
    // The rewrite's `(SELECT count(..) FROM S)` must not become count(*): DataFusion's
    // AggregateStatistics answers an unfiltered count(*) from statistics with a
    // PlaceholderRowExec, which the translator refuses (#158).
    let ctx = fixture().await;
    let plan = ctx
        .sql("select * from o where x not in (select y from s)")
        .await
        .expect("the query plans")
        .create_physical_plan()
        .await
        .expect("physical plan");
    let text = format!(
        "{}",
        datafusion::physical_plan::displayable(plan.as_ref()).indent(true)
    );
    assert!(!text.contains("PlaceholderRowExec"), "{text}");
}

#[tokio::test]
async fn not_in_over_keys_that_hold_no_null_is_left_alone() {
    // tpch anti-join: o_custkey and c_custkey hold no NULL in any row group, so the rewrite's
    // counts and extra NOT EXISTS never appear.
    let plan = tpch_plan("anti-join").await;
    assert!(
        !format!("{}", plan.display_indent()).contains("count("),
        "{}",
        plan.display_indent()
    );
}

#[tokio::test]
async fn not_in_over_an_untraceable_operand_is_rewritten() {
    // `x + 0` is not a bare column, so the trace cannot follow it and the rule rewrites.
    assert_eq!(
        rows("select * from o where x + 0 not in (select y from s where z <> 2)").await,
        vec![(Some(1), Some(2)), (Some(3), Some(5))]
    );
}

// #247 — DataFusion 45's own limits, each refused while planning on both engines.
#[tokio::test]
async fn bug_datafusion_45_refuses_six_subquery_shapes() {
    for sql in [
        "select * from o where exists (select 1 from s)",
        "select x in (select y from s) from o",
        "select * from o where (w, x) not in (select z, y from s)",
        "select * from o where x <> all (select y from s)",
        "select * from o, lateral (select * from s where s.z = o.w) l",
        "select * from o where x in (select y from s where s.z = o.w limit 1)",
    ] {
        assert!(
            !refusal(sql).await.is_empty(),
            "{sql} planned: #247 may be fixed"
        );
    }
}

// #247's seventh shape — a scalar subquery of several rows answers instead of erroring.
#[tokio::test]
async fn bug_a_scalar_subquery_of_several_rows_is_not_refused() {
    assert!(row_count("select (select y from s) from o").await > 0);
}
