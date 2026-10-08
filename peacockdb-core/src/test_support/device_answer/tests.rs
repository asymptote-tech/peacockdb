//! The device's answer against a committed section, shown failing: no device, no file.

use std::sync::Arc;

use datafusion::arrow::array::{Float64Array, Int32Array, RecordBatch};
use datafusion::arrow::datatypes::{DataType, Field, Schema};

use super::device_answer_matches;

fn answer() -> Vec<RecordBatch> {
    let schema = Arc::new(Schema::new(vec![
        Field::new("k", DataType::Int32, false),
        Field::new("v", DataType::Float64, false),
    ]));
    vec![
        RecordBatch::try_new(
            schema,
            vec![
                Arc::new(Int32Array::from(vec![1, 2])),
                Arc::new(Float64Array::from(vec![0.5, 1.25])),
            ],
        )
        .expect("a batch"),
    ]
}

/// A section as `mini.result.txt` holds one: the mode that wrote it, then the table.
fn section(rows: &[&str]) -> String {
    let mut out = "mode=tp1-single\n+---+------+\n| k | v    |\n+---+------+\n".to_string();
    for row in rows {
        out.push_str(row);
        out.push('\n');
    }
    out + "+---+------+\n"
}

fn right() -> String {
    section(&["| 1 | 0.5  |", "| 2 | 1.25 |"])
}

#[test]
fn the_device_comparison_accepts_the_right_answer() {
    assert_eq!(
        device_answer_matches(&right(), "golden_exact", &answer()),
        Ok(())
    );
    assert_eq!(
        device_answer_matches(&right(), "golden_approx_std", &answer()),
        Ok(())
    );
}

#[test]
fn the_device_comparison_fails_on_a_wrong_row_under_either_golden_oracle() {
    let wrong = section(&["| 1 | 0.5  |", "| 3 | 1.25 |"]);
    assert!(device_answer_matches(&wrong, "golden_exact", &answer()).is_err());
    assert!(device_answer_matches(&wrong, "golden_approx_std", &answer()).is_err());
}

#[test]
fn the_device_comparison_fails_on_a_missing_row() {
    let short = section(&["| 1 | 0.5  |"]);
    assert!(device_answer_matches(&short, "golden_exact", &answer()).is_err());
    assert!(device_answer_matches(&short, "golden_approx_std", &answer()).is_err());
}

/// The tolerance is a tolerance and not a shrug: a digit inside it passes and the next one
/// out fails, so the two bounds are asserted rather than one of them.
#[test]
fn the_device_comparison_holds_a_digit_to_the_tolerance_under_golden_approx_std() {
    let near = section(&["| 1 | 0.5  |", "| 2 | 1.2500000000001 |"]);
    let far = section(&["| 1 | 0.5  |", "| 2 | 1.26 |"]);
    assert_eq!(
        device_answer_matches(&near, "golden_approx_std", &answer()),
        Ok(())
    );
    assert!(device_answer_matches(&far, "golden_approx_std", &answer()).is_err());
    assert!(device_answer_matches(&near, "golden_exact", &answer()).is_err());
}

/// `live_cpu` compares against a cpu run at the same mode, which this function has not got.
/// It says so rather than passing over a section it was never meant to read.
#[test]
fn live_cpu_is_not_a_section_comparison() {
    assert!(
        device_answer_matches(&right(), "live_cpu", &answer())
            .expect_err("live_cpu has no section to read")
            .contains("live_cpu")
    );
}

#[test]
#[should_panic(expected = "golden_exact|golden_approx_std|live_cpu")]
fn an_unknown_gpu_oracle_names_the_accepted_set() {
    let _ = device_answer_matches(&right(), "golden_approx", &answer());
}
