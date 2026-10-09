//! `GpuLoadParquet` through the harness: both backends read one parquet the test wrote
//! from a synthetic batch, one batch per row group, and a limit over the batches a scan
//! emitted, which is what a scan's limit is.
//! `write_parquet` puts the file under the temp dir and each case removes it. Every case
//! here is green: #186 took the four `bug_` pins a scan's own limit used to earn.

use std::path::{Path, PathBuf};

use datafusion::arrow::datatypes::SchemaRef;
use datafusion::arrow::record_batch::RecordBatch;
use datafusion::parquet::arrow::ArrowWriter;
use datafusion::parquet::file::properties::WriterProperties;
use datafusion::parquet::file::reader::{FileReader, SerializedFileReader};

use super::script::{Outcome, Script, each_answers, run_both};
use crate::plan::{
    BatchLayout, GpuLimit, GpuLoadParquet, RowGroupMeta, RowInterval, ScanMetadata, Schema,
};
use crate::tests::compare::Order;
use crate::tests::given::Given;
use crate::tests::synthetic::{decimals, synthetic};

/// `batch` as a parquet file of `rows_per_group`-row row groups, under the temp dir and
/// named by the case, so two cases never share one. A zero-row batch writes no row group.
pub(crate) fn write_parquet(name: &str, batch: &RecordBatch, rows_per_group: usize) -> PathBuf {
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
pub(crate) fn scan(path: &Path, schema: SchemaRef) -> GpuLoadParquet {
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
        Schema::new(schema),
    )
}

/// Write, read on both, remove: the file is gone whether or not the comparison holds.
fn read_both(name: &str, batch: &RecordBatch, rows_per_group: usize) -> Outcome {
    let path = write_parquet(name, batch, rows_per_group);
    let outcome = run_both(&scan(&path, batch.schema()), Script::Source { lane: 0 });
    std::fs::remove_file(&path).expect("the file this case wrote");
    outcome
}

operator_case! {
    GpuLoadParquet,
    fn both_backends_read_the_same_batches_per_row_group() {
        read_both("per-group", &synthetic(64, 1), 16).same(Order::AsEmitted);
    }
}

// A scan's limit is a `GpuLimit` over the batches the scan emits (#186): the reader reads
// its row groups whole, and the limit cuts. Each limit case's stream is a real scan's
// output — the file both backends just read, as the cpu emitted it — so the cut is over
// what a scan hands it, not batches sliced by hand as `harness_cases::limit_over`'s are.

/// `GpuLimit 0..+fetch` over the batches a per-row-group scan of `synthetic(64, 1)` in
/// groups of `rows_per_group` emitted, once both backends were seen to emit the same ones.
fn limit_over_scan(name: &str, fetch: u64, rows_per_group: usize) -> (GpuLimit, Vec<RecordBatch>) {
    let whole = synthetic(64, 1);
    let read = read_both(name, &whole, rows_per_group);
    read.same(Order::AsEmitted);
    let batches: Vec<RecordBatch> = read
        .cpu
        .expect("the cpu read the file")
        .into_iter()
        .flatten()
        .collect();
    assert_eq!(
        batches.len(),
        64 / rows_per_group,
        "one batch per row group"
    );
    let node = GpuLimit::new(
        Given::of(Schema::new(whole.schema()), BatchLayout::MultipleBatches),
        RowInterval {
            skip: 0,
            fetch: Some(fetch),
        },
    );
    (node, batches)
}

operator_case! {
    GpuLoadParquet,
    fn a_scan_of_one_row_group_reads_it_whole_on_both() {
        read_both("one-group", &synthetic(64, 1), 64).same(Order::AsEmitted);
    }
}

operator_case! {
    GpuLimit,
    fn a_limit_over_a_scan_of_one_row_group_keeps_its_first_rows_on_both() {
        let (node, batches) = limit_over_scan("limit-one-group", 10, 64);
        // One slot for the batch, one for the finish.
        let expected = [vec![batches[0].slice(0, 10)], vec![]];
        let outcome = run_both(&node, Script::Accumulate(batches));
        each_answers(&outcome, &expected, &expected);
    }
}

operator_case! {
    GpuLimit,
    fn a_limit_inside_the_first_of_four_scanned_row_groups_drops_the_rest_on_both() {
        let (node, batches) = limit_over_scan("limit-inside-first", 10, 16);
        let expected = [vec![batches[0].slice(0, 10)], vec![], vec![], vec![], vec![]];
        let outcome = run_both(&node, Script::Accumulate(batches));
        each_answers(&outcome, &expected, &expected);
    }
}

operator_case! {
    GpuLimit,
    fn a_limit_across_a_scanned_row_group_boundary_slices_the_second_group_on_both() {
        let (node, batches) = limit_over_scan("limit-across-groups", 20, 16);
        let expected = [
            vec![batches[0].clone()],
            vec![batches[1].slice(0, 4)],
            vec![],
            vec![],
            vec![],
        ];
        let outcome = run_both(&node, Script::Accumulate(batches));
        each_answers(&outcome, &expected, &expected);
    }
}

// The file holds `Decimal128(18, 2)`; the cpu reads it so, and the device's export is told
// the same declaration from a bare scan with no operator between.
operator_case! {
    GpuLoadParquet,
    fn a_decimal_column_is_exported_at_its_declared_precision() {
        read_both("decimals", &decimals(64, 1), 16).same(Order::AsEmitted);
    }
}

// A parquet of zero rows has no row group, so the mapping is one lane of no batches and
// both sources are exhausted at the first call.
operator_case! {
    GpuLoadParquet,
    fn a_parquet_of_zero_rows_reads_as_nothing_on_both() {
        read_both("zero-rows", &synthetic(0, 1), 16).same(Order::AsEmitted);
    }
}
