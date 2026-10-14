//! `IS NOT DISTINCT FROM` as a join condition (review row 9). DataFusion 45 extracts only `=`
//! as a join key, so `ON a.k IS NOT DISTINCT FROM b.k` reaches the planner as a keyless nested
//! loop. The planner makes each such conjunct over one column of each side a hash key with
//! `null_equals_null`, and leaves every other conjunct as the residual.
//!
//! Over `crate::tests::join_fixture`: `nulls` holds keys 1, 3, 5, 6, 7, 8, 10 and three NULLs,
//! its `v` the row number 0..9, so `nulls` against itself matches seven keys once each and
//! the three NULL-key rows three ways each — sixteen pairs.

use datafusion::common::JoinType;

use crate::plan::{BinaryOp, Expr, GpuNode};
use crate::plan::{NodeRef, as_node_ref};
use crate::tests::join_fixture::{Fixture, planned};

/// Every join in the tree as `(kind, type, null_equals_null, residual)`, parents first. A
/// nested loop has no flag, so it reads `false`.
fn joins_in(node: &dyn GpuNode) -> Vec<(&'static str, JoinType, bool, Option<Expr>)> {
    let mut found = match as_node_ref(node) {
        NodeRef::Join(join) => vec![(
            "hash",
            join.join_type,
            join.null_equals_null,
            join.filter.clone(),
        )],
        NodeRef::NestedLoopJoin(join) => vec![(
            "nested loop",
            join.join_type,
            false,
            Some(join.filter.clone()),
        )],
        _ => Vec::new(),
    };
    for child in node.children() {
        found.extend(joins_in(child));
    }
    found
}

async fn joins_of(
    fixture: &Fixture,
    sql: &str,
) -> Vec<(&'static str, JoinType, bool, Option<Expr>)> {
    let tree = planned(&fixture.plan(sql).await).unwrap_or_else(|e| panic!("{sql}: {e}"));
    joins_in(tree.as_ref())
}

fn is_op(expr: &Option<Expr>, op: BinaryOp) -> bool {
    matches!(expr, Some(Expr::Binary { op: found, .. }) if *found == op)
}

#[tokio::test]
async fn is_not_distinct_from_alone_is_a_null_equal_hash_key() {
    let fixture = Fixture::new("indf-alone").await;
    for (sql, join_type, rows) in [
        (
            "SELECT a.v, b.v FROM nulls a FULL JOIN nulls b ON a.k IS NOT DISTINCT FROM b.k",
            JoinType::Full,
            16,
        ),
        (
            // The probe's column first: DataFusion keeps the operands in the order written.
            "SELECT a.v, b.v FROM nulls a JOIN nulls b ON b.k IS NOT DISTINCT FROM a.k",
            JoinType::Inner,
            16,
        ),
        // tiny holds no NULL, so the three NULL-key rows of `nulls` find nothing there.
        (
            "SELECT a.v FROM nulls a WHERE EXISTS \
             (SELECT 1 FROM tiny b WHERE a.k IS NOT DISTINCT FROM b.k)",
            JoinType::LeftSemi,
            7,
        ),
        (
            "SELECT a.v FROM nulls a WHERE NOT EXISTS \
             (SELECT 1 FROM tiny b WHERE a.k IS NOT DISTINCT FROM b.k)",
            JoinType::LeftAnti,
            3,
        ),
    ] {
        assert_eq!(
            joins_of(&fixture, sql).await,
            vec![("hash", join_type, true, None)],
            "{sql}"
        );
        assert_eq!(fixture.answers_as_datafusion(sql).await, rows, "{sql}");
    }
}

#[tokio::test]
async fn a_conjunct_beside_is_not_distinct_from_is_the_residual() {
    let fixture = Fixture::new("indf-residual").await;
    // Of the sixteen key pairs only the NULL group's rows (v = 1, 3, 8) pair with a larger v:
    // (1, 3), (1, 8), (3, 8). Full pads the eight rows of each side nothing matched. The
    // parentheses are sqlparser's: IS NOT DISTINCT FROM binds looser than AND.
    let sql = "SELECT a.v, b.v FROM nulls a FULL JOIN nulls b \
               ON (a.k IS NOT DISTINCT FROM b.k) AND a.v < b.v";
    let joins = joins_of(&fixture, sql).await;
    assert_eq!(joins.len(), 1, "{joins:?}");
    let (kind, join_type, null_equals_null, residual) = &joins[0];
    assert_eq!(
        (*kind, *join_type, *null_equals_null),
        ("hash", JoinType::Full, true)
    );
    assert!(is_op(residual, BinaryOp::Lt), "{residual:?}");
    assert_eq!(fixture.answers_as_datafusion(sql).await, 3 + 8 + 8);
}

#[tokio::test]
async fn an_equality_key_beside_is_not_distinct_from_keeps_the_sql_default() {
    let fixture = Fixture::new("indf-beside-eq").await;
    // DataFusion keys the join on `=` itself, so the flag stays false and IS NOT DISTINCT FROM
    // is the residual: the seven non-NULL keys, each against itself.
    let sql = "SELECT a.v, b.v FROM nulls a JOIN nulls b \
               ON a.k = b.k AND a.v IS NOT DISTINCT FROM b.v";
    let joins = joins_of(&fixture, sql).await;
    assert_eq!(joins.len(), 1, "{joins:?}");
    let (kind, join_type, null_equals_null, residual) = &joins[0];
    assert_eq!(
        (*kind, *join_type, *null_equals_null),
        ("hash", JoinType::Inner, false)
    );
    assert!(is_op(residual, BinaryOp::IsNotDistinctFrom), "{residual:?}");
    assert_eq!(fixture.answers_as_datafusion(sql).await, 7);
}

#[tokio::test]
async fn what_is_not_a_column_pair_stays_a_nested_loop() {
    let fixture = Fixture::new("indf-not-a-key").await;
    // IS DISTINCT FROM is no equality, and an expression is no key: both keep the loop.
    for (sql, op, rows) in [
        (
            "SELECT a.v, b.v FROM nulls a JOIN nulls b ON a.k IS DISTINCT FROM b.k",
            BinaryOp::IsDistinctFrom,
            100 - 16,
        ),
        (
            "SELECT a.v, b.v FROM nulls a JOIN nulls b ON a.k + 1 IS NOT DISTINCT FROM b.k",
            BinaryOp::IsNotDistinctFrom,
            // 5 + 1 = 6, 6 + 1 = 7 and 7 + 1 = 8, and the three NULLs three ways each.
            3 + 9,
        ),
    ] {
        let joins = joins_of(&fixture, sql).await;
        assert_eq!(joins.len(), 1, "{sql}: {joins:?}");
        let (kind, join_type, _, filter) = &joins[0];
        assert_eq!(
            (*kind, *join_type),
            ("nested loop", JoinType::Inner),
            "{sql}"
        );
        assert!(is_op(filter, op), "{sql}: {filter:?}");
        assert_eq!(fixture.answers_as_datafusion(sql).await, rows, "{sql}");
    }
}
