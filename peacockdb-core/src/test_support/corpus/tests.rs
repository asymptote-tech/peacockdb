//! The cpu corpus helpers over strings and batches: no dataset, no run, no file.

use std::sync::Arc;

use datafusion::arrow::array::{Float64Array, Int32Array, RecordBatch};
use datafusion::arrow::datatypes::{DataType, Field, Schema};

use super::{CpuOracle, cpu_oracle_mode, result_matches};
use crate::test_support::MODES;
use crate::test_support::device_answer::{GpuResultMode, gpu_result_mode};

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

fn section(rows: &[&str]) -> String {
    let mut out = "mode=tp1-single\n+---+------+\n| k | v    |\n+---+------+\n".to_string();
    for row in rows {
        out.push_str(row);
        out.push('\n');
    }
    out + "+---+------+\n"
}

#[test]
fn the_cpu_helper_accepts_the_right_answer() {
    let right = section(&["| 1 | 0.5  |", "| 2 | 1.25 |"]);
    assert_eq!(result_matches(&right, &MODES[0], &answer()), Ok(()));
}

#[test]
fn the_cpu_helper_fails_on_a_wrong_row() {
    let wrong = section(&["| 1 | 0.5  |", "| 3 | 1.25 |"]);
    assert!(result_matches(&wrong, &MODES[0], &answer()).is_err());
}

#[test]
fn the_cpu_helper_fails_on_a_missing_row() {
    let short = section(&["| 1 | 0.5  |"]);
    assert!(result_matches(&short, &MODES[0], &answer()).is_err());
}

/// The cpu's section is exact text, so a digit anywhere is a different answer — there is no
/// tolerance here to be inside of. The device's `golden_approx_std` is where one lives.
#[test]
fn the_cpu_helper_fails_on_a_digit() {
    let near = section(&["| 1 | 0.5  |", "| 2 | 1.2500000000001 |"]);
    assert!(result_matches(&near, &MODES[0], &answer()).is_err());
}

/// A section written at another mode is a section this mode cannot be held to, and the
/// author line is part of what is compared.
#[test]
fn the_cpu_helper_fails_on_a_section_another_mode_wrote() {
    let right = section(&["| 1 | 0.5  |", "| 2 | 1.25 |"]);
    assert!(result_matches(&right, &MODES[4], &answer()).is_err());
}

/// Every variant of both enums is named by some `corpus_query!` line, so one the corpus
/// does not use is deleted rather than carried. `every_duckdb_oracle_is_named_by_some_line`
/// (`test_cpu_corpus.rs`) is the third of the three, over `DuckdbOracle`.
#[test]
fn every_oracle_variant_is_named_by_some_line() {
    let lines = super::corpus_lines();
    assert!(!lines.is_empty(), "the include declares nothing");
    for variant in CpuOracle::ALL {
        assert!(
            lines
                .iter()
                .any(|line| cpu_oracle_mode(&line[6]) == variant),
            "{variant:?} is named by no corpus_query! line — delete it rather than keep it"
        );
    }
    for variant in GpuResultMode::ALL {
        assert!(
            lines
                .iter()
                .any(|line| gpu_result_mode(&line[7]) == variant),
            "{variant:?} is named by no corpus_query! line — delete it rather than keep it"
        );
    }
}
