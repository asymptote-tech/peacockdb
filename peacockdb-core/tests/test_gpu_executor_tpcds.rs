//! GPU-execution tests for the TPC-DS suite, mirroring `test_gpu_executor.rs`
//! (TPC-H). Each query is run through the GPU executor and the result set is
//! compared against peacock's CPU executor (order-independent).
//!
//! Many TPC-DS queries do not yet execute on the GPU. Those are commented out
//! below and tracked in the TPC-DS GPU-execution ticket, grouped into buckets
//! by a surface-level read of the failure (not a deep diagnosis). As bugs and
//! missing functionality are addressed, the corresponding tests are re-enabled.

#[cfg(not(feature = "rust-only"))]
mod gpu_executor_tpcds_tests {
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
    testdata_root().join("tpcds.sf1")
}

fn queries_dir() -> PathBuf {
    testdata_root().join("tpcds-queries")
}

const GPU_BUDGET: usize = 2 * 1024 * 1024 * 1024;

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
/// executor, then assert that the result sets are equal (order-independent).
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

// =====================================================================
// Passing on the GPU (25). Compared against the peacock CPU executor on
// the H200 (SF-1). The remaining 70 are commented out below, grouped by
// failure bucket; the 4 that don't physical-plan are noted inline. See
// the TPC-DS GPU-execution tracking ticket (issue #29) for the analysis.
// =====================================================================

gpu_result_test!(test_gpu_tpcds_q1, "q1");
gpu_result_test!(test_gpu_tpcds_q3, "q3");
gpu_result_test!(test_gpu_tpcds_q19, "q19");
gpu_result_test!(test_gpu_tpcds_q25, "q25");
gpu_result_test!(test_gpu_tpcds_q29, "q29");
gpu_result_test!(test_gpu_tpcds_q30, "q30");
gpu_result_test!(test_gpu_tpcds_q31, "q31");
gpu_result_test!(test_gpu_tpcds_q34, "q34");
gpu_result_test!(test_gpu_tpcds_q37, "q37");
gpu_result_test!(test_gpu_tpcds_q42, "q42");
gpu_result_test!(test_gpu_tpcds_q43, "q43");
gpu_result_test!(test_gpu_tpcds_q46, "q46");
gpu_result_test!(test_gpu_tpcds_q48, "q48");
gpu_result_test!(test_gpu_tpcds_q52, "q52");
gpu_result_test!(test_gpu_tpcds_q55, "q55");
gpu_result_test!(test_gpu_tpcds_q58, "q58");
gpu_result_test!(test_gpu_tpcds_q59, "q59");
gpu_result_test!(test_gpu_tpcds_q65, "q65");
gpu_result_test!(test_gpu_tpcds_q68, "q68");
gpu_result_test!(test_gpu_tpcds_q69, "q69");
gpu_result_test!(test_gpu_tpcds_q73, "q73");
gpu_result_test!(test_gpu_tpcds_q82, "q82");
gpu_result_test!(test_gpu_tpcds_q83, "q83");
gpu_result_test!(test_gpu_tpcds_q85, "q85");
gpu_result_test!(test_gpu_tpcds_q91, "q91");
// Bucket A (UnionExec / InterleaveExec → GpuUnion):
gpu_result_test!(test_gpu_tpcds_q33, "q33");
gpu_result_test!(test_gpu_tpcds_q56, "q56");
gpu_result_test!(test_gpu_tpcds_q60, "q60");
gpu_result_test!(test_gpu_tpcds_q71, "q71");
// Bucket B (GlobalLimitExec → GpuLimit):
gpu_result_test!(test_gpu_tpcds_q16, "q16");
gpu_result_test!(test_gpu_tpcds_q32, "q32");
gpu_result_test!(test_gpu_tpcds_q92, "q92");
gpu_result_test!(test_gpu_tpcds_q94, "q94");
gpu_result_test!(test_gpu_tpcds_q95, "q95");

// =====================================================================
// Skipped — failing on the GPU. Buckets per issue #29 (surface analysis;
// a query may move buckets once its first blocker is fixed). Re-enable a
// line as its bucket is addressed.
// =====================================================================

// Buckets A (UnionExec/InterleaveExec → GpuUnion) and B (GlobalLimitExec →
// GpuLimit) are implemented; the passing queries moved up to the active list.
// The queries below carried a Union/Limit node but hit a *second* blocker once
// it was lowered — re-bucketed by that blocker below.

// --- Bucket A residual: GpuUnion concatenate type mismatch ---
// q5: union branches produce DECIMAL128 columns with drifting scales (each
// channel's SUM lands a different cuDF scale), so cudf::concatenate rejects
// them. Needs per-branch cast to the union's declared output scale.
// gpu_result_test!(test_gpu_tpcds_q5, "q5");

// --- Bucket C: window functions (BoundedWindowAggExec) ---
// gpu_result_test!(test_gpu_tpcds_q49, "q49");

// --- Bucket D: joins not yet supported ---
// CrossJoinExec (see #28):
// gpu_result_test!(test_gpu_tpcds_q23, "q23");
// gpu_result_test!(test_gpu_tpcds_q28, "q28");
// gpu_result_test!(test_gpu_tpcds_q77, "q77");
// NestedLoopJoinExec (see #28):
// gpu_result_test!(test_gpu_tpcds_q14, "q14");
// unsupported join type: 2 (Right join):
// gpu_result_test!(test_gpu_tpcds_q80, "q80");

// --- Bucket E: aggregate gaps ---
// q2: GpuAggregate binaryop "Unsupported operator for these types".
// gpu_result_test!(test_gpu_tpcds_q2, "q2");

// --- Bucket F: projection / scalar-expr gaps ---
// q38, q96: GpuProject: no expressions.
// q66, q76: GpuProject non-fixed-width type (column_factories).
// q75: scalar fn: coalesce. q97: unsupported UnaryOp: 2.
// gpu_result_test!(test_gpu_tpcds_q38, "q38");
// gpu_result_test!(test_gpu_tpcds_q66, "q66");
// gpu_result_test!(test_gpu_tpcds_q75, "q75");
// gpu_result_test!(test_gpu_tpcds_q76, "q76");
// gpu_result_test!(test_gpu_tpcds_q96, "q96");
// gpu_result_test!(test_gpu_tpcds_q97, "q97");

// --- Bucket C: window functions (BoundedWindowAggExec / PARTITION BY ordering) ---
// gpu_result_test!(test_gpu_tpcds_q12, "q12");
// gpu_result_test!(test_gpu_tpcds_q20, "q20");
// gpu_result_test!(test_gpu_tpcds_q36, "q36");
// gpu_result_test!(test_gpu_tpcds_q44, "q44");
// gpu_result_test!(test_gpu_tpcds_q47, "q47");
// gpu_result_test!(test_gpu_tpcds_q51, "q51");
// gpu_result_test!(test_gpu_tpcds_q53, "q53");
// gpu_result_test!(test_gpu_tpcds_q57, "q57");
// gpu_result_test!(test_gpu_tpcds_q63, "q63");
// gpu_result_test!(test_gpu_tpcds_q67, "q67");
// gpu_result_test!(test_gpu_tpcds_q89, "q89");
// gpu_result_test!(test_gpu_tpcds_q98, "q98");

// --- Bucket D: joins not yet supported ---
// LeftMark join:
// gpu_result_test!(test_gpu_tpcds_q10, "q10");
// gpu_result_test!(test_gpu_tpcds_q35, "q35");
// gpu_result_test!(test_gpu_tpcds_q45, "q45");
// NestedLoopJoinExec (see #28):
// gpu_result_test!(test_gpu_tpcds_q9, "q9");
// gpu_result_test!(test_gpu_tpcds_q24, "q24");
// gpu_result_test!(test_gpu_tpcds_q54, "q54");
// CrossJoinExec (see #28):
// gpu_result_test!(test_gpu_tpcds_q61, "q61");
// gpu_result_test!(test_gpu_tpcds_q88, "q88");
// gpu_result_test!(test_gpu_tpcds_q90, "q90");
// unsupported join type: 2 (Right join):
// gpu_result_test!(test_gpu_tpcds_q40, "q40");
// gpu_result_test!(test_gpu_tpcds_q78, "q78");
// gpu_result_test!(test_gpu_tpcds_q93, "q93");

// --- Bucket E: aggregate gaps ---
// stddev unmapped:
// gpu_result_test!(test_gpu_tpcds_q17, "q17");
// gpu_result_test!(test_gpu_tpcds_q39, "q39");
// GpuAggregate CUDF failure (compound reduction / cast / AST operator):
// gpu_result_test!(test_gpu_tpcds_q13, "q13");
// gpu_result_test!(test_gpu_tpcds_q18, "q18");
// gpu_result_test!(test_gpu_tpcds_q22, "q22");

// --- Bucket F: projection / scalar-expr gaps ---
// gpu_result_test!(test_gpu_tpcds_q41, "q41"); // unsupported literal type: 1
// gpu_result_test!(test_gpu_tpcds_q84, "q84"); // scalar fn: concat
// gpu_result_test!(test_gpu_tpcds_q99, "q99"); // scalar fn: lower
// gpu_result_test!(test_gpu_tpcds_q87, "q87"); // GpuProject: no expressions

// --- Bucket G: FlatBuffer verification failed (large plans) ---
// gpu_result_test!(test_gpu_tpcds_q8, "q8");
// gpu_result_test!(test_gpu_tpcds_q64, "q64");

// --- Bucket H: result divergence (executes, wrong result) ---
// gpu_result_test!(test_gpu_tpcds_q4, "q4");
// gpu_result_test!(test_gpu_tpcds_q6, "q6");
// gpu_result_test!(test_gpu_tpcds_q7, "q7");
// gpu_result_test!(test_gpu_tpcds_q11, "q11");
// gpu_result_test!(test_gpu_tpcds_q15, "q15");
// gpu_result_test!(test_gpu_tpcds_q21, "q21");
// gpu_result_test!(test_gpu_tpcds_q26, "q26");
// gpu_result_test!(test_gpu_tpcds_q50, "q50");
// gpu_result_test!(test_gpu_tpcds_q62, "q62");
// gpu_result_test!(test_gpu_tpcds_q74, "q74");
// gpu_result_test!(test_gpu_tpcds_q79, "q79");
// gpu_result_test!(test_gpu_tpcds_q81, "q81");

// --- Does not physical-plan (also skipped in the plan tests, not a GPU gap) ---
// q27: ROLLUP ordering rejected by SanityCheckPlan.
// q70, q86: GROUPING() aggregate not physical-planned.
// q72: Date32 + Int64 type coercion.
}
