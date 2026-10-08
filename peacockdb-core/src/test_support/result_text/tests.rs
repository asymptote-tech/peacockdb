//! The whole-answer comparator over batches: no dataset, no run, no file.
//!
//! What `assert_answer`'s tolerance arm reaches under `data_fusion_approximate`, and what
//! the device's `live_cpu` compare reaches with no tolerance at all. Driven here as
//! [`results_match`] so a wrong answer can be shown failing without a DataFusion run behind
//! it — `assert_results_match` panics, and a panic is not a thing a case can assert on
//! cheaply.

use std::sync::Arc;

use datafusion::arrow::array::{Float64Array, Int64Array, RecordBatch};
use datafusion::arrow::datatypes::{DataType, Field, Schema};

use super::{assert_results_match, results_match};
use crate::test_support::corpus::CpuOracle;

/// `(k, v)` with `v` a float, so the tolerant arm keys on `k` and compares `v`.
fn answer(rows: &[(i64, f64)]) -> Vec<RecordBatch> {
    let schema = Arc::new(Schema::new(vec![
        Field::new("k", DataType::Int64, false),
        Field::new("v", DataType::Float64, false),
    ]));
    vec![
        RecordBatch::try_new(
            schema,
            vec![
                Arc::new(Int64Array::from(
                    rows.iter().map(|(k, _)| *k).collect::<Vec<i64>>(),
                )),
                Arc::new(Float64Array::from(
                    rows.iter().map(|(_, v)| *v).collect::<Vec<f64>>(),
                )),
            ],
        )
        .expect("a batch"),
    ]
}

/// The tolerance `data_fusion_approximate` declares, read off the oracle rather than
/// written again here: a line's oracle and what this comparator is held to are one number.
fn tolerant() -> Option<f64> {
    CpuOracle::DataFusionApproximate.rel_tol()
}

#[test]
fn the_tolerant_oracle_accepts_a_float_a_reassociated_sum_would_move() {
    let oracle = answer(&[(1, 1.0), (2, 2.0)]);
    let drifted = answer(&[(1, 1.0 + 1e-15), (2, 2.0)]);
    assert_eq!(results_match(&oracle, &drifted, tolerant()), Ok(()));
}

#[test]
fn the_tolerant_oracle_fails_past_its_tolerance() {
    let oracle = answer(&[(1, 1.0), (2, 2.0)]);
    let wrong = answer(&[(1, 1.000_001), (2, 2.0)]);
    let said = results_match(&oracle, &wrong, tolerant()).expect_err("1e-6 is past 1e-12");
    assert!(said.contains("rel diff"), "{said}");
}

/// A changed non-float cell moves the row's KEY, so the tolerance never gets to look at the
/// floats: the answer holds a row the oracle does not.
#[test]
fn the_tolerant_oracle_fails_on_a_row_the_oracle_does_not_have() {
    let oracle = answer(&[(1, 1.0), (2, 2.0)]);
    let wrong = answer(&[(1, 1.0), (3, 2.0)]);
    let said = results_match(&oracle, &wrong, tolerant()).expect_err("key 3 is not in the oracle");
    assert!(said.contains("absent"), "{said}");
}

#[test]
fn the_tolerant_oracle_fails_on_a_missing_row() {
    let oracle = answer(&[(1, 1.0), (2, 2.0)]);
    let short = answer(&[(1, 1.0)]);
    let said = results_match(&oracle, &short, tolerant()).expect_err("a row is gone");
    assert!(said.contains("row keys differ"), "{said}");
}

/// A row the answer returned twice where the oracle has it once: the keys agree and only the
/// multiplicity does not, which is the one thing a set compare would miss.
#[test]
fn the_tolerant_oracle_fails_on_a_duplicated_row() {
    let oracle = answer(&[(1, 1.0), (2, 2.0)]);
    let doubled = answer(&[(1, 1.0), (1, 1.0), (2, 2.0)]);
    let said = results_match(&oracle, &doubled, tolerant()).expect_err("one row came back twice");
    assert!(said.contains("multiplicity"), "{said}");
}

/// No tolerance is what the device's `live_cpu` compare passes, and there a digit is a
/// different answer — the drift the tolerant arm accepts fails here.
#[test]
fn the_exact_oracle_fails_on_the_drift_the_tolerant_one_accepts() {
    let oracle = answer(&[(1, 1.0), (2, 2.0)]);
    let drifted = answer(&[(1, 1.0 + 1e-15), (2, 2.0)]);
    assert_eq!(results_match(&oracle, &oracle, None), Ok(()));
    let said = results_match(&oracle, &drifted, None).expect_err("exact means the same bytes");
    assert!(said.contains("exact compare"), "{said}");
}

/// The panicking wrapper still names the query it was comparing. Its callers hand it a
/// `dataset/query at mode` string and read it out of the failure, so the split must not have
/// dropped it.
#[test]
fn the_panicking_wrapper_names_the_query() {
    let oracle = answer(&[(1, 1.0)]);
    let wrong = answer(&[(1, 2.0)]);
    let fell = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        assert_results_match(&oracle, &wrong, None, "tpch/q1 at tp1-single")
    }))
    .expect_err("a wrong answer panics");
    let said = fell
        .downcast_ref::<String>()
        .expect("the panic carries a message");
    assert!(said.starts_with("tpch/q1 at tp1-single: "), "{said}");
}
