//! Every shape the planner refuses, from the sql that reaches it, and the join shapes it once
//! refused, each now answering as DataFusion does.
//!
//! One test per refusal, named for the shape and for the reason, over the fixture in
//! `crate::tests::join_fixture`. Each asserts the ticket number is in the MESSAGE and not only
//! in a comment beside it: the message is what a user sees when a query does not plan.
//!
//! Three refusals are missing on purpose — a final aggregate with no partial below it, a
//! repartition by an unstated rule, an empty IN list. No query produces them, so
//! `join_capability.rs` hand-builds them, ticketless: consistency guards, not limitations.

use std::sync::Arc;

use datafusion::common::JoinType;
use datafusion::physical_plan::ExecutionPlan;
use datafusion::physical_plan::joins::NestedLoopJoinExec;

use crate::plan::PlanError;
use crate::tests::join_fixture::{Fixture, join_types_in};

/// Every nested-loop join in a plan, in tree order — `join_types_in` reads hash joins only.
fn nested_loop_types_in(plan: &Arc<dyn ExecutionPlan>) -> Vec<JoinType> {
    let mut found = Vec::new();
    if let Some(join) = plan.as_any().downcast_ref::<NestedLoopJoinExec>() {
        found.push(*join.join_type());
    }
    for child in plan.children() {
        found.extend(nested_loop_types_in(child));
    }
    found
}

#[tokio::test]
async fn an_outer_join_with_a_residual_filter_plans_and_answers_as_datafusion() {
    let fixture = Fixture::new("outer-residual").await;
    // `b.v = t.v` wherever the keys match, so the residual keeps the five odd keys and pads
    // the five even ones: an engine applying it after the outer gather drops the padded rows
    // (#153), and one ignoring it pads none.
    for (sql, join_type, rows) in [
        (
            "SELECT t.v, b.v FROM tiny t LEFT JOIN big b ON t.k = b.k AND b.v + t.k % 2 > t.v",
            JoinType::Left,
            10,
        ),
        // SWAP: big outside, so DataFusion builds tiny and the type becomes Right.
        (
            "SELECT b.v, t.v FROM big b LEFT JOIN tiny t ON b.k = t.k AND t.v < b.v + b.k % 2",
            JoinType::Right,
            1000,
        ),
        (
            "SELECT t.v, b.v FROM tiny t FULL OUTER JOIN big b \
             ON t.k = b.k AND b.v + t.k % 2 > t.v",
            JoinType::Full,
            1005,
        ),
    ] {
        assert_eq!(
            join_types_in(&fixture.plan(sql).await),
            vec![join_type],
            "{sql}"
        );
        assert_eq!(fixture.answers_as_datafusion(sql).await, rows, "{sql}");
    }
}

#[tokio::test]
async fn a_swapped_semi_or_anti_join_with_a_residual_filter_plans_and_answers_as_datafusion() {
    let fixture = Fixture::new("rightsemi-residual").await;
    // DataFusion swaps a FILTERED semi join, so this shape is a query's to produce and not
    // only a constructor's: big outside, tiny inside, a predicate on both (#159). The residual
    // keeps the five odd keys.
    for (sql, join_type, rows) in [
        (
            "SELECT b.v FROM big b WHERE EXISTS \
             (SELECT 1 FROM tiny t WHERE t.k = b.k AND t.v < b.v + b.k % 2)",
            JoinType::RightSemi,
            5,
        ),
        (
            "SELECT b.v FROM big b WHERE NOT EXISTS \
             (SELECT 1 FROM tiny t WHERE t.k = b.k AND t.v < b.v + b.k % 2)",
            JoinType::RightAnti,
            995,
        ),
    ] {
        assert_eq!(
            join_types_in(&fixture.plan(sql).await),
            vec![join_type],
            "{sql}"
        );
        assert_eq!(fixture.answers_as_datafusion(sql).await, rows, "{sql}");
    }
}

#[tokio::test]
async fn an_anti_join_over_null_keys_plans_with_not_exists_semantics() {
    let fixture = Fixture::new("anti-nulls").await;
    // Every non-NULL key finds itself; the three NULL-key rows find nothing, as `NOT EXISTS`
    // means — under set semantics they would find each other and be dropped (#59).
    let sql = "SELECT n.v FROM nulls n WHERE NOT EXISTS (SELECT 1 FROM nulls m WHERE m.k = n.k)";
    assert_eq!(fixture.answers_as_datafusion(sql).await, 3);
}

#[tokio::test]
async fn a_mark_join_over_null_keys_plans_with_in_semantics() {
    let fixture = Fixture::new("mark-nulls").await;
    // The seven non-NULL keys are members; of the three NULL-key rows only `v = 8` passes the
    // other arm. A NULL matched to a NULL would add the other two (#59).
    let sql = "SELECT n.v FROM nulls n WHERE n.k IN (SELECT k FROM nulls m) OR n.v > 5";
    assert_eq!(
        join_types_in(&fixture.plan(sql).await),
        vec![JoinType::LeftMark]
    );
    assert_eq!(fixture.answers_as_datafusion(sql).await, 8);
}

#[tokio::test]
async fn a_nested_loop_join_of_every_type_plans() {
    let fixture = Fixture::new("nlj-every-type").await;
    // The seven types beyond Inner and Left (#160), each from the query pbench's `nl-*` row
    // uses, with tiny for its `tiny` and big for its `fact`.
    for (sql, join_type) in [
        (
            "SELECT b.k, t.k FROM tiny t RIGHT JOIN big b ON b.v < t.v",
            JoinType::Right,
        ),
        (
            "SELECT b.k, t.k FROM big b FULL JOIN tiny t ON b.v < t.v",
            JoinType::Full,
        ),
        (
            "SELECT t.k FROM tiny t WHERE EXISTS (SELECT 1 FROM big b WHERE t.v > b.v)",
            JoinType::LeftSemi,
        ),
        (
            "SELECT t.k FROM tiny t WHERE NOT EXISTS (SELECT 1 FROM big b WHERE t.v > b.v)",
            JoinType::LeftAnti,
        ),
        (
            "SELECT t.k FROM tiny t WHERE t.k = 1 OR EXISTS (SELECT 1 FROM big b WHERE t.v > b.v)",
            JoinType::LeftMark,
        ),
        (
            "SELECT b.k FROM big b WHERE EXISTS (SELECT 1 FROM tiny t WHERE t.v > b.v)",
            JoinType::RightSemi,
        ),
        (
            "SELECT b.k FROM big b WHERE NOT EXISTS (SELECT 1 FROM tiny t WHERE t.v > b.v)",
            JoinType::RightAnti,
        ),
    ] {
        assert_eq!(
            nested_loop_types_in(&fixture.plan(sql).await),
            vec![join_type],
            "{sql}"
        );
        fixture.answers_as_datafusion(sql).await;
    }
}

#[tokio::test]
async fn a_window_function_is_refused_naming_143() {
    let fixture = Fixture::new("refuse-window").await;
    let err = fixture
        .refused("SELECT row_number() OVER (PARTITION BY k) FROM tiny")
        .await;
    assert!(
        matches!(&err, PlanError::Unsupported(what) if what.contains("#143")),
        "{err}"
    );
}

// #144: a second DISTINCT argument needs a gid-multiplying expand.
#[tokio::test]
async fn bug_two_distinct_arguments_are_refused() {
    let fixture = Fixture::new("refuse-two-distinct").await;
    let err = fixture
        .refused("SELECT count(DISTINCT k), count(DISTINCT v) FROM tiny")
        .await;
    assert!(
        matches!(&err, PlanError::Unsupported(what) if what.contains("#144")),
        "{err}"
    );
}

// #261: a Welford companion's state merges as one MERGE_M2 call, which an init cannot run.
#[tokio::test]
async fn bug_a_stddev_beside_a_distinct_is_refused() {
    let fixture = Fixture::new("refuse-welford-companion").await;
    let err = fixture
        .refused("SELECT stddev(v), count(DISTINCT k) FROM tiny")
        .await;
    assert!(
        matches!(&err, PlanError::Unsupported(what) if what.contains("#261")),
        "{err}"
    );
}

#[tokio::test]
async fn a_statistics_answered_count_is_refused_naming_158() {
    let fixture = Fixture::new("refuse-placeholder").await;
    // DataFusion answers it from the parquet metadata and leaves a PlaceholderRowExec, so
    // there is no aggregate to translate. #158 is what makes it executable at T10.
    let err = fixture.refused("SELECT count(*) FROM tiny").await;
    assert!(
        matches!(&err, PlanError::Unsupported(what)
            if what.contains("PlaceholderRowExec") && what.contains("#158")),
        "{err}"
    );
}

#[tokio::test]
async fn unsupported_expression_forms_are_refused_naming_the_form_and_162() {
    let fixture = Fixture::new("refuse-exprs").await;
    // Each of these survives DataFusion's simplifier, which is the part that makes them
    // reachable: `~ '^1'` becomes a LIKE and an Int64->Int32 TRY_CAST is proven away, so
    // the queries below are deliberately the forms that do not simplify.
    for (sql, named) in [
        (
            "SELECT v FROM tiny WHERE arrow_cast(k, 'Utf8') ~ '1|2'",
            "binary operator ~",
        ),
        (
            "SELECT v FROM tiny WHERE arrow_cast(k, 'Utf8') !~ '[0-9]+x'",
            "binary operator !~",
        ),
        (
            "SELECT v FROM tiny WHERE TRY_CAST(arrow_cast(k, 'Utf8') AS INT) > 1",
            "TRY_CAST",
        ),
    ] {
        let err = fixture.refused(sql).await;
        assert!(
            matches!(&err, PlanError::Unsupported(what)
                if what.contains(named) && what.contains("#162")),
            "{sql}: {err}"
        );
    }
}

#[tokio::test]
async fn an_aggregate_with_no_decomposition_is_refused_naming_161() {
    let fixture = Fixture::new("refuse-agg-shape").await;
    // The registry in aggregates.rs is what the engine can split into init, merge and
    // finish; a function outside it is refused by name rather than run in one lane.
    let err = fixture.refused("SELECT median(v) FROM tiny").await;
    assert!(
        matches!(&err, PlanError::Unsupported(what)
            if what.contains("median") && what.contains("#161")),
        "{err}"
    );
    // #161's other half, a FILTER clause, has no query: DataFusion 45's parser rejects
    // `sum(v) FILTER (WHERE v > 0)` outright, so the refusal beside it is reachable only
    // through a hand-built AggregateExec.
}
