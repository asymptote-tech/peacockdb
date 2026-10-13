//! `GpuLoadParquet` through the harness: both backends read one parquet the test wrote
//! from a synthetic batch, one batch per row group, with and without a pushed-down limit.
//! The one helper this task adds is `write_parquet`, under the temp dir and removed after
//! each case. Every case is green or a `bug_` test with its ticket above it. `GpuEmpty`, the
//! source that reads nothing, closes the file: alone, and planned whole under a join.

use std::path::{Path, PathBuf};

use datafusion::arrow::datatypes::SchemaRef;
use datafusion::arrow::record_batch::RecordBatch;
use datafusion::parquet::arrow::ArrowWriter;
use datafusion::parquet::file::properties::WriterProperties;
use datafusion::parquet::file::reader::{FileReader, SerializedFileReader};

use super::script::{Outcome, Script, run_both};
use crate::plan::{GpuEmpty, GpuLoadParquet, RowGroupMeta, ScanMetadata, Schema};
use crate::tests::compare::{Order, assert_same};
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
pub(crate) fn scan(path: &Path, schema: SchemaRef, limit: Option<usize>) -> GpuLoadParquet {
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

/// `synthetic(64, 1)` as the four 16-row batches a per-row-group read of it emits.
fn four_sixteens() -> Vec<Vec<RecordBatch>> {
    let whole = synthetic(64, 1);
    (0..4).map(|i| vec![whole.slice(i * 16, 16)]).collect()
}

const GROUPS_AND_LIMIT: &str = "row_groups can't be set along with skip_rows and num_rows";

// A pushed-down limit: the wire carries it as `num_rows`, and every batch is a row-group
// read, so the two reach cuDF together — while the cpu never reads it. Both are #186.

// #186 — even one row group and a limit is a row-group list beside `num_rows`.
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

// #186 — the first batch's read is refused, so nothing of the four is answered.
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

// The file holds `Decimal128(18, 2)`; the cpu reads it so, and the device's export is told
// the same declaration from a bare scan with no operator between.
operator_case! {
    GpuLoadParquet,
    fn a_decimal_column_is_exported_at_its_declared_precision() {
        read_both("decimals", &decimals(64, 1), 16, None).same(Order::AsEmitted);
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

// #63 — a scan of no column reads rows alone: the cpu off the reader's row count, the device
// off the footer, each as one non-null Int8 zero per row.
operator_case! {
    GpuLoadParquet,
    fn a_rows_only_scan_agrees_on_both_backends() {
        let batch = synthetic(64, 1);
        let path = write_parquet("rows-only", &batch, 16);
        let columns = scan(&path, batch.schema(), None);
        let groups = ScanMetadata {
            file: columns.file.clone(),
            groups: columns.survivors.clone(),
            can_be_null: Vec::new(),
        };
        let rows_only = GpuLoadParquet::new(
            "t".to_string(),
            Vec::new(),
            columns.partition_groups.clone(),
            &groups,
            None,
            Schema::rowmarker(),
        );
        let outcome = run_both(&rows_only, Script::Source { lane: 0 });
        std::fs::remove_file(&path).expect("the file this case wrote");
        outcome.same(Order::AsEmitted);
        let rows: Vec<usize> = outcome
            .gpu
            .as_ref()
            .expect("the device answers")
            .iter()
            .flatten()
            .map(|batch| batch.num_rows())
            .collect();
        assert_eq!(rows, vec![16; 4]);
    }
}

// A relation DataFusion proved empty: one lane that ends before its first batch, and a plan
// of it alone is one stub on the wire, which the device loads and never calls.
operator_case! {
    GpuEmpty,
    fn an_empty_relation_emits_no_batch_on_either_engine() {
        let empty = GpuEmpty::new(Schema::new(synthetic(0, 1).schema()));
        let outcome = run_both(&empty, Script::Source { lane: 0 });
        outcome.same(Order::AsEmitted);
        assert!(outcome.gpu.as_ref().expect("the device answers").is_empty());
    }
}

/// pbench's `empty-side-left-join`, planned at every mode and run whole through the driver on
/// both engines: the empty relation is the build side of the Right join DataFusion swaps the
/// `LEFT JOIN` into, so no lane gets a build batch and every probe row is padded — DuckDB's
/// eight. The harness drives one node and never runs its children, so only a planned run
/// puts the empty relation's stub on the device's wire under a join.
#[tokio::test]
async fn a_left_join_over_an_empty_relation_pads_every_row_on_both_engines() {
    use crate::executor::{CpuBackend, GpuBackend, run};
    use crate::test_support::{MODES, batches_to_sorted_str, data_dir_for, queries_dir_for};
    let sql = std::fs::read_to_string(queries_dir_for("pbench").join("empty-side-left-join.sql"))
        .expect("the query text");
    for mode in &MODES {
        let what = format!("empty-side-left-join at {}", mode.name);
        let ctx = crate::register_tables_for(
            crate::build_session_state(mode.target_partitions),
            &data_dir_for("pbench", "1"),
        )
        .await
        .expect("register pbench");
        let plan = ctx
            .sql(&sql)
            .await
            .expect("the query plans")
            .create_physical_plan()
            .await
            .expect("a physical plan");
        let (tree, _) = crate::planner::plan(&plan, mode.knobs_for("pbench"))
            .unwrap_or_else(|why| panic!("{what}: {why}"));
        let rows = |batches: Vec<crate::executor::CpuBatch>| {
            let batches: Vec<RecordBatch> =
                batches.iter().map(|b| b.record_batch().clone()).collect();
            let padded: usize = batches.iter().map(|b| b.column(1).null_count()).sum();
            (batches_to_sorted_str(&batches), padded)
        };
        let cpu = run::<CpuBackend>(tree.as_ref(), &ctx.task_ctx(), None)
            .unwrap_or_else(|why| panic!("{what} on the cpu: {why}"));
        let device = super::device::Device::open(tree.as_ref());
        let gpu = run::<GpuBackend>(tree.as_ref(), device.ctx(), None)
            .unwrap_or_else(|why| panic!("{what} on the device: {why}"));
        let (cpu, padded) = rows(cpu.batches);
        assert_eq!(padded, 8, "{what}: every tiny row, padded, on the cpu");
        assert_eq!(rows(gpu.batches), (cpu, 8), "{what}");
    }
}
