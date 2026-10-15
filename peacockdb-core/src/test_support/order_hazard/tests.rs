//! The order hazard over two four-row tables: which selections a device join's emission order
//! can move, read off DataFusion's plan and, for a sorted cut, off the rows either side of it.

use std::sync::Arc;

use datafusion::arrow::array::{ArrayRef, Int32Array, RecordBatch, StringArray};
use datafusion::execution::context::{SessionConfig, SessionContext};

use super::hazard_in;

/// `a(k, v)` and `b(k, w)`, joined one to one on `k`. `w` ties on its first two rows, so
/// `ORDER BY w` cuts between two rows whose `v` differ after one row, and not after two.
fn tables() -> SessionContext {
    tables_of(["p", "q", "r", "s"], [10, 10, 20, 30])
}

fn tables_of(v: [&str; 4], w: [i32; 4]) -> SessionContext {
    let ctx = SessionContext::new_with_config(SessionConfig::new().with_target_partitions(1));
    let int = |values: [i32; 4]| Arc::new(Int32Array::from(values.to_vec())) as ArrayRef;
    let a = RecordBatch::try_from_iter([
        ("k", int([1, 2, 3, 4])),
        ("v", Arc::new(StringArray::from(v.to_vec())) as ArrayRef),
    ])
    .expect("two columns of one length");
    let b = RecordBatch::try_from_iter([("k", int([1, 2, 3, 4])), ("w", int(w))])
        .expect("two columns of one length");
    ctx.register_batch("a", a).expect("register a");
    ctx.register_batch("b", b).expect("register b");
    ctx
}

async fn hazard(sql: &str) -> Option<String> {
    hazard_in(&tables(), sql).await
}

#[tokio::test]
async fn a_limit_with_no_sort_over_a_hash_join_is_a_hazard() {
    let why = hazard("SELECT a.v FROM a JOIN b ON a.k = b.k LIMIT 2").await;
    assert!(why.is_some_and(|why| why.contains("no sort")));
}

#[tokio::test]
async fn a_limit_over_an_aggregate_over_a_join_is_a_hazard() {
    let sql = "SELECT b.w, count(*) FROM a JOIN b ON a.k = b.k GROUP BY b.w LIMIT 1";
    assert!(hazard(sql).await.is_some());
}

/// A cross join keeps its inputs' order, so one fed by a hash join is as unordered as it is.
#[tokio::test]
async fn a_cross_join_carries_a_hash_joins_order_up_to_the_limit() {
    let sql = "SELECT x.v FROM (SELECT a.v FROM a JOIN b ON a.k = b.k) x CROSS JOIN b LIMIT 2";
    assert!(hazard(sql).await.is_some());
}

/// `tpch/nested-limits`' shape: both engines emit a cross join build-row-major, so the rows a
/// limit keeps are fixed.
#[tokio::test]
async fn a_limit_over_a_cross_join_of_scans_is_not_a_hazard() {
    assert_eq!(hazard("SELECT a.v FROM a CROSS JOIN b LIMIT 2").await, None);
}

#[tokio::test]
async fn a_limit_over_no_join_is_not_this_hazard() {
    assert_eq!(hazard("SELECT v FROM a LIMIT 2").await, None);
}

#[tokio::test]
async fn a_sorted_cut_between_tied_rows_that_differ_is_a_hazard() {
    let sql = "SELECT a.v, b.w FROM a JOIN b ON a.k = b.k ORDER BY b.w LIMIT 1";
    let why = hazard(sql).await;
    assert!(why.is_some_and(|why| why.contains("tie")));
}

#[tokio::test]
async fn a_sorted_cut_between_rows_that_do_not_tie_is_not_a_hazard() {
    let sql = "SELECT a.v, b.w FROM a JOIN b ON a.k = b.k ORDER BY b.w LIMIT 2";
    assert_eq!(hazard(sql).await, None);
}

/// Tied rows that are the same row: whichever the cut keeps, the answer is the same.
#[tokio::test]
async fn a_sorted_cut_between_identical_rows_is_not_a_hazard() {
    let sql = "SELECT b.w FROM a JOIN b ON a.k = b.k ORDER BY b.w LIMIT 1";
    assert_eq!(hazard(sql).await, None);
}

/// The offset is a cut too: `OFFSET 1` skips one of two tied rows.
#[tokio::test]
async fn an_offset_between_tied_rows_that_differ_is_a_hazard() {
    let sql = "SELECT a.v, b.w FROM a JOIN b ON a.k = b.k ORDER BY b.w LIMIT 2 OFFSET 1";
    assert!(hazard(sql).await.is_some());
}

#[tokio::test]
async fn a_sorted_cut_over_no_join_is_not_this_hazard() {
    assert_eq!(hazard("SELECT k, w FROM b ORDER BY w LIMIT 1").await, None);
}

/// tpcds q16's shape: a keyless aggregate answers one row whatever order its input arrives
/// in, so the limit above it has nothing to choose between.
#[tokio::test]
async fn a_limit_over_a_global_aggregate_over_a_join_is_not_a_hazard() {
    let sql = "SELECT count(*) FROM a JOIN b ON a.k = b.k LIMIT 1";
    assert_eq!(hazard(sql).await, None);
}

/// Three rows tie at the cut and two are the same. DataFusion's top-k orders this input `p, q, q`,
/// so the rows either side of the cut are the two `(q, 10)`, and the `(p, 10)` a device may drop
/// is further up the tie.
#[tokio::test]
async fn a_sorted_cut_inside_a_tie_of_three_rows_is_a_hazard() {
    let ctx = tables_of(["q", "p", "q", "s"], [10, 10, 10, 30]);
    let sql = "SELECT a.v, b.w FROM a JOIN b ON a.k = b.k ORDER BY b.w LIMIT 2";
    let why = hazard_in(&ctx, sql).await;
    assert!(why.is_some_and(|why| why.contains("tie")));
}

/// Four rows tie, and the top three DataFusion keeps are the same: the row that differs is
/// found only by reading past the first row after the cut.
#[tokio::test]
async fn a_tie_that_runs_past_the_row_after_the_cut_is_read_to_its_end() {
    let ctx = tables_of(["q", "q", "q", "p"], [10, 10, 10, 10]);
    let sql = "SELECT a.v, b.w FROM a JOIN b ON a.k = b.k ORDER BY b.w LIMIT 2";
    assert!(hazard_in(&ctx, sql).await.is_some());
}

#[tokio::test]
async fn a_sorted_cut_inside_a_tie_of_identical_rows_is_not_a_hazard() {
    let ctx = tables_of(["q", "q", "q", "s"], [10, 10, 10, 30]);
    let sql = "SELECT a.v, b.w FROM a JOIN b ON a.k = b.k ORDER BY b.w LIMIT 2";
    assert_eq!(hazard_in(&ctx, sql).await, None);
}

/// An offset with no limit leaves the sort without a fetch, and a projection sits between the
/// sort and the `GlobalLimitExec` that skips: the cut is the offset's alone.
#[tokio::test]
async fn an_offset_through_a_projection_inside_a_tie_is_a_hazard() {
    let ctx = tables_of(["p", "q", "q", "s"], [10, 10, 10, 30]);
    let sql = "SELECT a.v FROM a JOIN b ON a.k = b.k ORDER BY b.w OFFSET 2";
    let why = hazard_in(&ctx, sql).await;
    assert!(why.is_some_and(|why| why.contains("tie")));
}
