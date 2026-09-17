//! What a `GpuLoadParquet` hands the device up, read at the handle: one parquet per fixture
//! column type, written by the case and scanned on the device alone, every row group's
//! handle held to the declared schema. `write_parquet` and `scan` are `source_cases.rs`'s.
//! Every case is green or a `bug_` test with its ticket above it; nothing here repairs.

use datafusion::arrow::record_batch::RecordBatch;

use super::script::{Script, assert_none_diverge, divergences_on_device};
use super::source_cases::{scan, write_parquet};
use crate::tests::synthetic::{decimals, synthetic};

/// Write, scan on the device, remove, then assert: the file is gone whether or not the
/// handles held the declaration.
fn scan_holds_as_declared(case: &str, batch: &RecordBatch) {
    let path = write_parquet(case, batch, 16);
    let node = scan(&path, batch.schema(), None);
    let found = divergences_on_device(&node, Script::Source { lane: 0 });
    std::fs::remove_file(&path).expect("the file this case wrote");
    assert_none_diverge(found);
}

/// `batch`'s column `name` alone.
fn column_of(batch: &RecordBatch, name: &str) -> RecordBatch {
    let ordinal = batch.schema().index_of(name).expect("a fixture column");
    batch
        .project(&[ordinal])
        .expect("one column of the fixture")
}

operator_case! {
    GpuLoadParquet,
    fn a_scan_of_an_int64_column_declares_the_int64_the_device_holds() {
        scan_holds_as_declared("schema-int64", &column_of(&synthetic(64, 1), "id"));
    }
}

operator_case! {
    GpuLoadParquet,
    fn a_scan_of_an_int32_column_declares_the_int32_the_device_holds() {
        scan_holds_as_declared("schema-int32", &column_of(&synthetic(64, 1), "i32"));
    }
}

operator_case! {
    GpuLoadParquet,
    fn a_scan_of_a_float64_column_declares_the_float64_the_device_holds() {
        scan_holds_as_declared("schema-float64", &column_of(&synthetic(64, 1), "f64"));
    }
}

operator_case! {
    GpuLoadParquet,
    fn a_scan_of_a_utf8_column_declares_the_string_the_device_holds() {
        scan_holds_as_declared("schema-utf8", &column_of(&synthetic(64, 1), "s"));
    }
}

operator_case! {
    GpuLoadParquet,
    fn a_scan_of_a_date32_column_declares_the_timestamp_days_the_device_holds() {
        scan_holds_as_declared("schema-date32", &column_of(&synthetic(64, 1), "d"));
    }
}

operator_case! {
    GpuLoadParquet,
    fn a_scan_of_a_boolean_column_declares_the_bool8_the_device_holds() {
        scan_holds_as_declared("schema-boolean", &column_of(&synthetic(64, 1), "b"));
    }
}

operator_case! {
    GpuLoadParquet,
    fn a_scan_of_a_decimal_column_declares_the_scale_2_the_device_holds() {
        scan_holds_as_declared("schema-decimal", &column_of(&decimals(64, 1), "dec"));
    }
}

operator_case! {
    GpuLoadParquet,
    fn a_scan_of_every_synthetic_column_declares_what_the_device_holds() {
        scan_holds_as_declared("schema-every-column", &synthetic(64, 1));
    }
}
