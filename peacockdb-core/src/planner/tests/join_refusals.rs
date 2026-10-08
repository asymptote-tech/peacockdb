//! Every shape the planner refuses, from the sql that reaches it.
//!
//! One test per refusal, named for the shape and for the reason, over the fixture in
//! `crate::tests::join_fixture`. Each asserts the ticket number is in the MESSAGE and not only
//! in a comment beside it: the message is what a user sees when a query does not plan.
//!
//! Three refusals are missing here on purpose — a final aggregate with no partial below
//! it, a repartition by an unstated rule, and an empty IN list. No query produces any of
//! them, so they are hand-built in `join_capability.rs` and carry no ticket:
//! they are internal-consistency guards rather than limitations.

use crate::plan::PlanError;
use crate::tests::join_fixture::Fixture;

#[tokio::test]
async fn an_outer_join_with_a_residual_filter_is_refused_naming_153() {
    let fixture = Fixture::new("refuse-outer-residual").await;
    // The executor applies the residual after the outer gather, so a padded row's NULLs
    // make the predicate NULL and the row is dropped: a LEFT JOIN answering as an inner.
    let err = fixture
        .refused("SELECT t.v FROM tiny t LEFT JOIN big b ON t.k = b.k AND b.v > t.v")
        .await;
    assert!(
        matches!(&err, PlanError::Unsupported(what) if what.contains("#153")),
        "{err}"
    );
}

#[tokio::test]
async fn a_swapped_semi_or_anti_join_with_a_residual_filter_is_refused_naming_159() {
    let fixture = Fixture::new("refuse-rightsemi-residual").await;
    // DataFusion does swap a FILTERED semi join, so this shape is a query's to produce and
    // not only a constructor's: big outside, tiny inside, and a predicate on both. The
    // anti form swaps the same way, and both land on a mixed_* family with no swapped
    // variant.
    for sql in [
        "SELECT b.v FROM big b WHERE EXISTS \
         (SELECT 1 FROM tiny t WHERE t.k = b.k AND t.v < b.v)",
        "SELECT b.v FROM big b WHERE NOT EXISTS \
         (SELECT 1 FROM tiny t WHERE t.k = b.k AND t.v < b.v)",
    ] {
        let err = fixture.refused(sql).await;
        assert!(
            matches!(&err, PlanError::Unsupported(what)
                if what.contains("mixed_*") && what.contains("#159")),
            "{sql}: {err}"
        );
    }
}

#[tokio::test]
async fn an_anti_join_whose_key_is_null_on_both_sides_is_refused_naming_59_and_80() {
    let fixture = Fixture::new("refuse-anti-nulls").await;
    let err = fixture
        .refused("SELECT n.v FROM nulls n WHERE NOT EXISTS (SELECT 1 FROM nulls m WHERE m.k = n.k)")
        .await;
    assert!(
        matches!(&err, PlanError::Unsupported(what) if what.contains("#59") && what.contains("#80")),
        "{err}"
    );
}

#[tokio::test]
async fn a_mark_join_whose_key_is_null_on_both_sides_is_refused_naming_59_and_80() {
    let fixture = Fixture::new("refuse-mark-nulls").await;
    let err = fixture
        .refused("SELECT n.v FROM nulls n WHERE n.k IN (SELECT k FROM nulls m) OR n.v > 5")
        .await;
    assert!(
        matches!(&err, PlanError::Unsupported(what) if what.contains("#59")),
        "{err}"
    );
}

#[tokio::test]
async fn a_nested_loop_join_beyond_inner_and_left_is_refused_naming_160() {
    let fixture = Fixture::new("refuse-nlj-full").await;
    // A non-equi predicate on a FULL OUTER join: the executor rejects anything but Inner
    // and Left outright, and DataFusion does not swap this one away.
    let err = fixture
        .refused("SELECT t.v FROM tiny t FULL OUTER JOIN big b ON t.v > b.v")
        .await;
    assert!(
        matches!(&err, PlanError::Unsupported(what)
            if what.contains("Inner and Left") && what.contains("#160")),
        "{err}"
    );
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
    // Both arguments are named — `name@ordinal`, DataFusion's own rendering — which is what
    // separates this from the shape below: here they really are two columns, and a message
    // that only said "#144" would pin either one.
    assert!(
        matches!(&err, PlanError::Unsupported(what)
            if what.contains("#144") && what.contains("k@") && what.contains("v@")),
        "{err}"
    );
}

// #144's refusal, reached by a shape that is not #144: ONE argument under a cast the
// lowering's frozen list does not admit. `avg`/`sum` coerce Float32 to Float64 with a real
// CastExpr, and `keeps_distinct` takes integers to Float64 and not Float32, so the two
// arguments strip to different expressions. The cast is injective, so widening the list would
// make this plan — that is the human's call, and this goes red when somebody makes it.
#[tokio::test]
async fn bug_one_float32_distinct_argument_under_two_coercions_is_refused() {
    let fixture = Fixture::new("refuse-f32-distinct").await;
    let err = fixture
        .refused(
            "SELECT count(DISTINCT arrow_cast(v, 'Float32')), \
             sum(DISTINCT arrow_cast(v, 'Float32')) FROM tiny",
        )
        .await;
    // The message must describe the shape hit — two arguments that DIFFER — and not claim a
    // second DISTINCT argument, which this query does not have.
    assert!(
        matches!(&err, PlanError::Unsupported(what)
            if what.contains("#144") && !what.contains("a second DISTINCT argument")),
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
