#[cfg(not(feature = "rust-only"))]
mod gpu_executor_tests {
use std::path::PathBuf;

use arrow::record_batch::RecordBatch;
use arrow::util::pretty::pretty_format_batches;
use peacockdb_core::gpu_executor::GpuExecutor;
use peacockdb_core::CpuExecutor;

// Root of the testdata tree. PEACOCK_TESTDATA_DIR overrides the
// compile-time path so the binary can be built on one machine and run
// on another (e.g. shad-gpu), where CARGO_MANIFEST_DIR doesn't exist.
fn testdata_root() -> PathBuf {
    if let Some(d) = std::env::var_os("PEACOCK_TESTDATA_DIR") {
        return PathBuf::from(d);
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../testdata")
}

fn testdata_dir() -> PathBuf {
    testdata_root().join("tpch.sf1")
}

fn queries_dir() -> PathBuf {
    testdata_root().join("tpch-queries")
}

const GPU_BUDGET: usize = 2 * 1024 * 1024 * 1024;

fn total_rows(batches: &[RecordBatch]) -> usize {
    batches.iter().map(|b| b.num_rows()).sum()
}

/// Render RecordBatches as a pretty table and sort the data rows so that
/// result comparison is order-independent (queries without ORDER BY may
/// return rows in any order depending on the executor path).
fn batches_to_sorted_str(batches: &[RecordBatch]) -> String {
    let formatted = pretty_format_batches(batches).unwrap().to_string();
    let lines: Vec<&str> = formatted.lines().collect();
    // Layout: border / header / border / ...data rows... / border
    if lines.len() > 4 {
        let mut data = lines[3..lines.len() - 1].to_vec();
        data.sort_unstable();
        let mut out = lines[..3].to_vec();
        out.extend(data);
        out.push(lines[lines.len() - 1]);
        out.join("\n")
    } else {
        formatted
    }
}

/// Run `name.sql` through both the GPU executor and the peacock CPU
/// executor (`execute_node_by_node` via `CpuExecutor`), then assert that
/// the result sets are equal (order-independent).
async fn assert_gpu_results_match_cpu(name: &str) {
    let data_dir = testdata_dir();

    let sql_path = queries_dir().join(format!("{name}.sql"));
    let sql = std::fs::read_to_string(&sql_path)
        .unwrap_or_else(|_| panic!("query file not found: {}", sql_path.display()));

    // Ground truth: peacock's CPU executor (GPU-annotated plan run on CPU
    // via execute_node_by_node).
    let cpu = CpuExecutor::new(&data_dir, 1, GPU_BUDGET).await.unwrap();
    let expected = cpu.execute(&sql).await.unwrap();

    // Subject: GPU executor.
    let gpu = GpuExecutor::new(&data_dir, 1, GPU_BUDGET).await.unwrap();
    let actual = gpu.execute(&sql).await.unwrap();

    assert_eq!(
        batches_to_sorted_str(&actual),
        batches_to_sorted_str(&expected),
        "GPU executor result for '{name}' differs from peacock CPU executor"
    );
}

macro_rules! gpu_result_test {
    ($func_name:ident, $query_name:literal) => {
        #[tokio::test]
        async fn $func_name() {
            assert_gpu_results_match_cpu($query_name).await;
        }
    };
}

gpu_result_test!(test_gpu_scan_limit, "scan-limit");
// Skipped (issue #12, bucket 6 — cuDF AST mixed int32/int64 coercion):
// gpu_result_test!(test_gpu_filter_project, "filter-project");
gpu_result_test!(test_gpu_aggregate_groupby, "aggregate-groupby");
gpu_result_test!(test_gpu_semi_join, "semi-join");
gpu_result_test!(test_gpu_anti_join, "anti-join");
// Skipped (issue #12, bucket 4 — needs GpuCrossJoinExec / GpuNestedLoopJoinExec):
// gpu_result_test!(test_gpu_nested_loop_join, "nested-loop-join");
// gpu_result_test!(test_gpu_cross_join, "cross-join");
// Skipped (issue #12, bucket 6 — cuDF AST type mismatch: Decimal × (Int − Decimal) coercion):
// gpu_result_test!(test_gpu_q1, "q1");
gpu_result_test!(test_gpu_q2, "q2");
// Skipped (issue #12, bucket 8 — cuDF groupby: Invalid type/aggregation combination
// on Decimal × Decimal sum):
// gpu_result_test!(test_gpu_q3, "q3");
gpu_result_test!(test_gpu_q4, "q4");
// Skipped (issue #12, bucket 7 — result divergence: revenue values differ):
// gpu_result_test!(test_gpu_q5, "q5");
// Skipped (issue #12, bucket 6 — cuDF AST type mismatch: Decimal column vs Float/Int literal):
// gpu_result_test!(test_gpu_q6, "q6");
// Skipped (issue #12, bucket 6 — cuDF AST type mismatch: Decimal × (Int − Decimal) coercion):
// gpu_result_test!(test_gpu_q7, "q7");
// Skipped (issue #12, bucket 6 — cuDF AST type mismatch: Decimal × (Int − Decimal) coercion):
// gpu_result_test!(test_gpu_q8, "q8");
// Skipped (issue #12, bucket 6 — cuDF AST type mismatch in GpuProject expr):
// gpu_result_test!(test_gpu_q9, "q9");
// Skipped (issue #12, bucket 7 — result divergence: revenue values differ):
// gpu_result_test!(test_gpu_q10, "q10");
// Skipped (issue #12, bucket 4 — TPC-H q11 has correlated HAVING subquery → NestedLoopJoinExec):
// gpu_result_test!(test_gpu_q11, "q11");
// Skipped (issue #12, bucket 8 — cuDF groupby: Invalid type/aggregation combination
// on sum of CASE WHEN ... THEN 1 ELSE 0 END):
// gpu_result_test!(test_gpu_q12, "q12");
gpu_result_test!(test_gpu_q13, "q13");
// Skipped (issue #12, bucket 6 — cuDF AST type mismatch: Decimal × (Int − Decimal) coercion):
// gpu_result_test!(test_gpu_q14, "q14");
// q15 uses a view; skip like test_cpu_executor.rs / test_queries.rs
gpu_result_test!(test_gpu_q16, "q16");
gpu_result_test!(test_gpu_q17, "q17");
// Skipped (issue #12, bucket 6 — cuDF AST type mismatch in join filter):
// gpu_result_test!(test_gpu_q18, "q18");
// Skipped (issue #12, bucket 6 — cuDF AST type mismatch in GpuFilter expr):
// gpu_result_test!(test_gpu_q19, "q19");
gpu_result_test!(test_gpu_q20, "q20");
// Skipped (issue #12, bucket 7 — result divergence: empty vs ~99 rows):
// gpu_result_test!(test_gpu_q21, "q21");
// Skipped (issue #12, bucket 4 — TPC-H q22 has correlated subquery → NestedLoopJoinExec):
// gpu_result_test!(test_gpu_q22, "q22");

#[tokio::test]
async fn test_gpu_scan_nation() {
    let exec = GpuExecutor::new(&testdata_dir(), 1, GPU_BUDGET).await.unwrap();
    let batches = exec.execute("SELECT * FROM nation").await.unwrap();
    println!("nation rows from GPU: {}", total_rows(&batches));
}

#[tokio::test]
async fn test_gpu_filter_nation() {
    let exec = GpuExecutor::new(&testdata_dir(), 1, GPU_BUDGET).await.unwrap();
    let batches = exec
        .execute("SELECT n_name FROM nation WHERE CAST(n_nationkey AS BIGINT) > 5")
        .await
        .unwrap();
    println!("filtered nation rows from GPU: {}", total_rows(&batches));
}

#[tokio::test]
async fn test_gpu_aggregate_count() {
    let exec = GpuExecutor::new(&testdata_dir(), 1, GPU_BUDGET).await.unwrap();
    // COUNT(*) alone triggers DataFusion's PlaceholderRowExec (metadata-only
    // row count, no scan). Use SUM to force a real GPU scan + aggregate.
    let batches = exec
        .execute("SELECT SUM(n_nationkey) FROM nation")
        .await
        .unwrap();
    println!("aggregate result rows from GPU: {}", total_rows(&batches));
}

#[tokio::test]
async fn test_gpu_join_nation_region() {
    let exec = GpuExecutor::new(&testdata_dir(), 1, GPU_BUDGET).await.unwrap();
    let batches = exec
        .execute(
            "SELECT n.n_name, r.r_name \
             FROM nation n JOIN region r ON n.n_regionkey = r.r_regionkey",
        )
        .await
        .unwrap();
    println!("join result rows from GPU: {}", total_rows(&batches));
}

#[tokio::test]
async fn test_gpu_sort_nation() {
    let exec = GpuExecutor::new(&testdata_dir(), 1, GPU_BUDGET).await.unwrap();
    let batches = exec
        .execute("SELECT n_name FROM nation ORDER BY n_name ASC")
        .await
        .unwrap();
    println!("sorted nation rows from GPU: {}", total_rows(&batches));
}

#[tokio::test]
async fn test_gpu_executor_create_destroy() {
    let exec = GpuExecutor::new(&testdata_dir(), 1, GPU_BUDGET).await.unwrap();
    drop(exec);
}
}
