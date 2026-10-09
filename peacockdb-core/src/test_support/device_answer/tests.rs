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

/// An EMPTY `PCK_WRITE_GPU_RESULT` is not recording. `build-test-shadgpu.sh` exports the
/// variable on every cycle, so empty is what an ordinary `--run` sends; read as a version it
/// wrote `gpu-result-.txt` on every cycle, and `--pull-results` then found a file to bring
/// home whether or not the operator asked to record.
#[test]
fn an_empty_recording_variable_is_not_a_version() {
    use super::{GpuRecording, gpu_recording_asked};
    assert_eq!(gpu_recording_asked(None), GpuRecording::No);
    assert_eq!(gpu_recording_asked(Some("")), GpuRecording::No);
    assert_eq!(gpu_recording_asked(Some("1")), GpuRecording::Committed);
    assert_eq!(
        gpu_recording_asked(Some("26.02")),
        GpuRecording::Versioned("26.02".to_string())
    );
}

/// A NaN against a number is a difference, under the tolerant oracle too. `NaN > tol` is
/// false, so a bare magnitude test accepts a device cell that came back NaN where the golden
/// holds a value — the shape a failed reduction has.
#[test]
fn the_device_comparison_fails_on_a_nan_against_a_number() {
    let nan = section(&["| 1 | 0.5  |", "| 2 | NaN  |"]);
    assert!(device_answer_matches(&nan, "golden_approx_std", &answer()).is_err());
    assert!(device_answer_matches(&nan, "golden_exact", &answer()).is_err());
}

/// cuDF's own `version_config.hpp`, the text `build.rs` copies into the build. The real 25.02
/// header is the fixture, because the one digit that matters is the zero: cuDF's `MINOR` is `2`
/// and every path, gitignore line and spec sentence in the tree says `25.02`.
#[test]
fn the_cudf_version_is_read_from_its_header_with_the_minor_padded() {
    use super::cudf_version_of_config;
    let real = "#pragma once\n\n#define CUDF_VERSION_MAJOR 25\n#define CUDF_VERSION_MINOR 2\n\
                #define CUDF_VERSION_PATCH 2\n";
    assert_eq!(cudf_version_of_config(real).as_deref(), Some("25.02"));
    let next = "#define CUDF_VERSION_MAJOR 26\n#define CUDF_VERSION_MINOR 2\n";
    assert_eq!(cudf_version_of_config(next).as_deref(), Some("26.02"));
    let october = "#define CUDF_VERSION_MAJOR 25\n#define CUDF_VERSION_MINOR 10\n";
    assert_eq!(cudf_version_of_config(october).as_deref(), Some("25.10"));
    // A rust-only build has no cuDF, so `build.rs` writes an empty file rather than a value
    // nothing checked.
    assert_eq!(cudf_version_of_config(""), None);
    assert_eq!(
        cudf_version_of_config("#define CUDF_VERSION_MAJOR 25\n"),
        None
    );
}
