//! `GpuLoadParquet` through the harness: both backends read one parquet the test wrote
//! from a synthetic batch, one batch per row group, with and without a pushed-down limit.
//! The one helper this task adds is `write_parquet`, under the temp dir and removed after
//! each case. Every case is green or a `bug_` test with its ticket above it.

use std::path::{Path, PathBuf};

use datafusion::arrow::compute::cast;
use datafusion::arrow::datatypes::{DataType, SchemaRef};
use datafusion::arrow::record_batch::RecordBatch;
use datafusion::parquet::arrow::ArrowWriter;
use datafusion::parquet::file::properties::WriterProperties;
use datafusion::parquet::file::reader::{FileReader, SerializedFileReader};

use super::script::{Outcome, Script, run_both};
use crate::plan::{GpuLoadParquet, RowGroupMeta, ScanMetadata, Schema};
use crate::tests::compare::{Order, assert_same};
use crate::tests::synthetic::{decimals, synthetic};

/// `batch` as a parquet file of `rows_per_group`-row row groups, under the temp dir and
/// named by the case, so two cases never share one. A zero-row batch writes no row group.
fn write_parquet(name: &str, batch: &RecordBatch, rows_per_group: usize) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "operator-cases-{name}-{}.parquet",
        std::process::id()
    ));
    let props = WriterProperties::builder()
        .set_max_row_group_size(rows_per_group)
        .build();
    let file = std::fs::File::create(&path).expect("a writable temp dir");
    let mut writer =
        ArrowWriter::try_new(file, batch.schema(), Some(props)).expect("the writer opens");
    writer.write(batch).expect("the rows are written");
    writer.close().expect("the footer is written");
    path
}

/// A one-lane scan of `path`, one batch per row group, every column projected, declaring
/// `schema` — the batch's own, as the planner declares the file's.
fn scan(path: &Path, schema: SchemaRef, limit: Option<usize>) -> GpuLoadParquet {
    let file = std::fs::File::open(path).expect("the file just written");
    let reader = SerializedFileReader::new(file).expect("a parquet file");
    let groups: Vec<RowGroupMeta> = (0..reader.metadata().num_row_groups())
        .map(|index| {
            let group = reader.metadata().row_group(index);
            RowGroupMeta {
                index: index as u32,
                rows: group.num_rows() as u64,
                bytes: group.total_byte_size() as u64,
            }
        })
        .collect();
    let per_group: Vec<Vec<u32>> = groups.iter().map(|g| vec![g.index]).collect();
    let width = schema.fields().len();
    let scan = ScanMetadata {
        file: path.to_string_lossy().into_owned(),
        groups,
        can_be_null: vec![true; width],
    };
    GpuLoadParquet::new(
        "t".to_string(),
        (0..width as u32).collect(),
        vec![per_group],
        &scan,
        limit,
        Schema::new(schema),
    )
}

/// Write, read on both, remove: the file is gone whether or not the comparison holds.
fn read_both(
    name: &str,
    batch: &RecordBatch,
    rows_per_group: usize,
    limit: Option<usize>,
) -> Outcome {
    let path = write_parquet(name, batch, rows_per_group);
    let outcome = run_both(
        &scan(&path, batch.schema(), limit),
        Script::Source { lane: 0 },
    );
    std::fs::remove_file(&path).expect("the file this case wrote");
    outcome
}

operator_case! {
    GpuLoadParquet,
    fn both_backends_read_the_same_batches_per_row_group() {
        read_both("per-group", &synthetic(64, 1), 16, None).same(Order::AsEmitted);
    }
}

// The `bug_` assertions: the device's refusal pinned by the stable part of its message,
// and each side pinned to hand-written slots.

fn gpu_refuses_with(outcome: &Outcome, message: &str) {
    let why = outcome.gpu_refuses();
    assert!(why.contains(message), "{why}");
}

fn cpu_answered(outcome: &Outcome, expected: &[Vec<RecordBatch>]) {
    assert_same(
        expected,
        outcome.cpu.as_ref().expect("the cpu answers"),
        Order::AsEmitted,
    );
}

fn gpu_answered(outcome: &Outcome, expected: &[Vec<RecordBatch>]) {
    assert_same(
        expected,
        outcome.gpu.as_ref().expect("the device answers"),
        Order::AsEmitted,
    );
}

/// `synthetic(64, 1)` as the four 16-row batches a per-row-group read of it emits.
fn four_sixteens() -> Vec<Vec<RecordBatch>> {
    let whole = synthetic(64, 1);
    (0..4).map(|i| vec![whole.slice(i * 16, 16)]).collect()
}

const GROUPS_AND_LIMIT: &str = "row_groups can't be set along with skip_rows and num_rows";

// A pushed-down limit: the wire carries it as `num_rows`, and every batch is a row-group
// read, so the two reach cuDF together (#188) — while the cpu never reads it (#186).

// #188 — even one row group and a limit is a row-group list beside `num_rows`.
operator_case! {
    GpuLoadParquet,
    fn bug_a_limit_over_one_row_group_is_refused_on_the_device() {
        let outcome = read_both("limit-gpu", &synthetic(64, 1), 64, Some(10));
        gpu_refuses_with(&outcome, GROUPS_AND_LIMIT);
    }
}

// #186 — `CpuSource::new` never reads `node.limit`: ten rows asked, sixty-four answered.
operator_case! {
    GpuLoadParquet,
    fn bug_a_limit_over_one_row_group_is_ignored_on_the_cpu() {
        let outcome = read_both("limit-cpu", &synthetic(64, 1), 64, Some(10));
        cpu_answered(&outcome, &[vec![synthetic(64, 1)]]);
    }
}

// #188 — the first batch's read is refused, so nothing of the four is answered.
operator_case! {
    GpuLoadParquet,
    fn bug_row_groups_and_a_limit_together_are_refused_on_the_device() {
        let outcome = read_both("groups-and-limit-gpu", &synthetic(64, 1), 16, Some(10));
        gpu_refuses_with(&outcome, GROUPS_AND_LIMIT);
    }
}

// #186 — four row groups of sixteen, all four answered whole under a limit of ten.
operator_case! {
    GpuLoadParquet,
    fn bug_row_groups_and_a_limit_together_are_read_whole_on_the_cpu() {
        let outcome = read_both("groups-and-limit-cpu", &synthetic(64, 1), 16, Some(10));
        cpu_answered(&outcome, &four_sixteens());
    }
}

// #187 — the file holds `Decimal128(18, 2)` and the cpu reads it so; the device exports
// the column at precision 38, from a bare scan with no operator between.
operator_case! {
    GpuLoadParquet,
    fn bug_a_decimal_column_is_exported_at_precision_38() {
        let outcome = read_both("decimals", &decimals(64, 1), 16, None);
        let dec = decimals(64, 1);
        cpu_answered(&outcome, &(0..4).map(|i| vec![dec.slice(i * 16, 16)]).collect::<Vec<_>>());
        let widened = RecordBatch::try_from_iter(vec![
            ("id", dec.column(0).clone()),
            ("dec", cast(dec.column(1), &DataType::Decimal128(38, 2)).expect("a widening")),
        ])
        .expect("two columns of one length");
        gpu_answered(&outcome, &(0..4).map(|i| vec![widened.slice(i * 16, 16)]).collect::<Vec<_>>());
    }
}

// A parquet of zero rows has no row group, so the mapping is one lane of no batches and
// both sources are exhausted at the first call.
operator_case! {
    GpuLoadParquet,
    fn a_parquet_of_zero_rows_reads_as_nothing_on_both() {
        read_both("zero-rows", &synthetic(0, 1), 16, None).same(Order::AsEmitted);
    }
}
