use std::path::PathBuf;
use std::sync::Arc;

use datafusion::arrow::array::Int64Array;
use datafusion::arrow::record_batch::RecordBatch;
use datafusion::arrow::util::pretty::pretty_format_batches;
use datafusion::physical_plan::ExecutionPlan;

use peacockdb_core::cpu_executor::{
    execute_node_by_node, execute_node_by_node_instrumented, NodeMemoryStats,
};
use peacockdb_core::{create_context_with_tables, build_session_state, register_tables_for};

fn testdata_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../testdata/tpch.sf1")
}

fn queries_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../testdata/tpch-queries")
}

fn has_gpu_node(plan: &Arc<dyn ExecutionPlan>) -> bool {
    plan.name().starts_with("Gpu") || plan.children().iter().any(|c| has_gpu_node(c))
}

fn all_node_names(plan: &Arc<dyn ExecutionPlan>) -> Vec<String> {
    let mut names = vec![plan.name().to_string()];
    for child in plan.children() {
        names.extend(all_node_names(child));
    }
    names
}

fn scan_batch_sizes(plan: &Arc<dyn ExecutionPlan>) -> Vec<usize> {
    use peacockdb_core::gpu_rule::GpuScanExec;
    let mut sizes = vec![];
    if let Some(scan) = plan.as_any().downcast_ref::<GpuScanExec>() {
        sizes.push(scan.gpu_batch_size);
    }
    for child in plan.children() {
        sizes.extend(scan_batch_sizes(child));
    }
    sizes
}

fn fmt_plan(plan: &Arc<dyn ExecutionPlan>) -> String {
    use datafusion::physical_plan::display::DisplayableExecutionPlan;
    DisplayableExecutionPlan::new(plan.as_ref())
        .indent(true)
        .to_string()
}

async fn make_ctx(budget: usize) -> datafusion::execution::context::SessionContext {
    create_context_with_tables(&testdata_dir(), 1, budget)
        .await
        .unwrap()
}

const FULL_BUDGET: usize = 2 * 1024 * 1024 * 1024;
const TIGHT_BUDGET: usize = 10 * 1024;

// 
#[tokio::test]
async fn test_execution_strips_gpu_nodes() {
    let ctx = make_ctx(FULL_BUDGET).await;
    let plan = ctx
        .sql("SELECT count(*) FROM nation WHERE n_regionkey >= 0")
        .await
        .unwrap()
        .create_physical_plan()
        .await
        .unwrap();

    assert!(
        has_gpu_node(&plan),
        "expected GPU nodes in plan, got: {:?}",
        all_node_names(&plan)
    );

    let mut stats: Vec<NodeMemoryStats> = vec![];
    execute_node_by_node_instrumented(plan, ctx.task_ctx(), &mut stats)
        .await
        .unwrap();

    assert!(!stats.is_empty(), "no nodes were executed");
    let gpu_names: Vec<&str> = stats
        .iter()
        .filter(|s| s.node_name.starts_with("Gpu"))
        .map(|s| s.node_name.as_str())
        .collect();
    assert!(gpu_names.is_empty(), "GPU nodes not stripped: {gpu_names:?}");
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

/// Run `name.sql` through both plain DataFusion and the CPU executor, then
/// assert that the result sets are equal (order-independent).
async fn assert_cpu_results_match_datafusion(name: &str) {
    let data_dir = testdata_dir();

    let sql_path = queries_dir().join(format!("{name}.sql"));
    let sql = std::fs::read_to_string(&sql_path)
        .unwrap_or_else(|_| panic!("query file not found: {}", sql_path.display()));
    let mut df_ctx = build_session_state(1);
    df_ctx = register_tables_for(df_ctx, &data_dir).await.unwrap();
    // Ground truth: plain DataFusion without GPU rules.
    let expected = df_ctx.sql(&sql).await.unwrap().collect().await.unwrap();

    // CPU executor: GPU-annotated plan executed node-by-node on CPU.
    let cpu_ctx = make_ctx(FULL_BUDGET).await;
    let plan = cpu_ctx.sql(&sql).await.unwrap().create_physical_plan().await.unwrap();
    let actual = execute_node_by_node(plan, cpu_ctx.task_ctx(), &mut |_, _| {})
        .await
        .unwrap();

    assert_eq!(
        batches_to_sorted_str(&actual),
        batches_to_sorted_str(&expected),
        "CPU executor result for '{name}' differs from plain DataFusion"
    );
}

macro_rules! cpu_result_test {
    ($func_name:ident, $query_name:literal) => {
        #[tokio::test]
        async fn $func_name() {
            assert_cpu_results_match_datafusion($query_name).await;
        }
    };
}

cpu_result_test!(test_cpu_scan_limit, "scan-limit");
cpu_result_test!(test_cpu_filter_project, "filter-project");
cpu_result_test!(test_cpu_aggregate_groupby, "aggregate-groupby");
cpu_result_test!(test_cpu_hash_join, "hash-join");
cpu_result_test!(test_cpu_left_join, "left-join");
cpu_result_test!(test_cpu_semi_join, "semi-join");
cpu_result_test!(test_cpu_anti_join, "anti-join");
cpu_result_test!(test_cpu_nested_loop_join, "nested-loop-join");
cpu_result_test!(test_cpu_mixed_join, "mixed-join");
cpu_result_test!(test_cpu_cross_join, "cross-join");
cpu_result_test!(test_cpu_q1, "q1");
cpu_result_test!(test_cpu_q2, "q2");
cpu_result_test!(test_cpu_q3, "q3");
cpu_result_test!(test_cpu_q4, "q4");
cpu_result_test!(test_cpu_q5, "q5");
cpu_result_test!(test_cpu_q6, "q6");
cpu_result_test!(test_cpu_q7, "q7");
cpu_result_test!(test_cpu_q8, "q8");
cpu_result_test!(test_cpu_q9, "q9");
cpu_result_test!(test_cpu_q10, "q10");
cpu_result_test!(test_cpu_q11, "q11");
cpu_result_test!(test_cpu_q12, "q12");
cpu_result_test!(test_cpu_q13, "q13");
cpu_result_test!(test_cpu_q14, "q14");
// cpu_result_test!(test_cpu_q15, "q15");  // q15 uses a view; skip like test_queries.rs
cpu_result_test!(test_cpu_q16, "q16");
cpu_result_test!(test_cpu_q17, "q17");
cpu_result_test!(test_cpu_q18, "q18");
cpu_result_test!(test_cpu_q19, "q19");
cpu_result_test!(test_cpu_q20, "q20");
cpu_result_test!(test_cpu_q21, "q21");
cpu_result_test!(test_cpu_q22, "q22");

#[tokio::test]
async fn test_memory_boundary_preserved_tight_budget() {
    let query = "SELECT count(*) FROM customer WHERE c_custkey > 0";

    let ctx_full = make_ctx(FULL_BUDGET).await;
    let plan_full = ctx_full.sql(query).await.unwrap()
        .create_physical_plan().await.unwrap();

    let ctx_tight = make_ctx(TIGHT_BUDGET).await;
    let plan_tight = ctx_tight.sql(query).await.unwrap()
        .create_physical_plan().await.unwrap();

    eprintln!("\n=== FULL BUDGET ({} GiB) plan ===\n{}", FULL_BUDGET / (1024*1024*1024), fmt_plan(&plan_full));
    eprintln!("=== TIGHT BUDGET ({} KiB) plan ===\n{}", TIGHT_BUDGET / 1024, fmt_plan(&plan_tight));

    let tight_scan_sizes = scan_batch_sizes(&plan_tight);
    assert!(
        !tight_scan_sizes.is_empty(),
        "expected GpuScanExec in tight plan; node names: {:?}",
        all_node_names(&plan_tight)
    );
    let gpu_batch_size = *tight_scan_sizes.iter().max().unwrap();

    let full_scan_sizes = scan_batch_sizes(&plan_full);
    let full_batch_size = *full_scan_sizes.iter().max().unwrap();

    eprintln!(
        "GpuScanExec batch_size — full budget: {full_batch_size}, tight budget: {gpu_batch_size}"
    );
    assert!(
        gpu_batch_size < full_batch_size,
        "tight budget batch_size ({gpu_batch_size}) should be smaller than full budget ({full_batch_size})"
    );

    let mut stats: Vec<NodeMemoryStats> = vec![];
    let batches =
        execute_node_by_node_instrumented(plan_tight, ctx_tight.task_ctx(), &mut stats)
            .await
            .unwrap();

    let count = batches[0].column(0).as_any().downcast_ref::<Int64Array>().unwrap().value(0);
    assert_eq!(count, 150_000, "customer table must have 150 000 rows");

    let scan_stats: Vec<&NodeMemoryStats> = stats
        .iter()
        .filter(|s| s.node_name == "ParquetExec")
        .collect();
    assert!(!scan_stats.is_empty(), "expected ParquetExec in stats");

    eprintln!("Per-node stats (post-order):");
    for s in &stats {
        eprintln!(
            "  {}: rows={}, max_batch={}, alloc={}B, logical={}B",
            s.node_name, s.row_count, s.max_batch_rows, s.allocated_bytes, s.logical_bytes
        );
    }

    for s in &scan_stats {
        assert!(
            s.max_batch_rows <= gpu_batch_size,
            "ParquetExec batch {} rows exceeds gpu_batch_size={}",
            s.max_batch_rows, gpu_batch_size
        );
    }

    let gpu_names: Vec<&str> = stats
        .iter()
        .filter(|s| s.node_name.starts_with("Gpu"))
        .map(|s| s.node_name.as_str())
        .collect();
    assert!(gpu_names.is_empty(), "GPU nodes in stats: {gpu_names:?}");
}

#[tokio::test]
async fn test_instrumented_stats_are_populated() {
    let ctx = make_ctx(FULL_BUDGET).await;
    let plan = ctx
        .sql("SELECT n_name, n_regionkey FROM nation WHERE n_regionkey = 1")
        .await
        .unwrap()
        .create_physical_plan()
        .await
        .unwrap();

    let mut stats: Vec<NodeMemoryStats> = vec![];
    let batches =
        execute_node_by_node_instrumented(plan, ctx.task_ctx(), &mut stats).await.unwrap();

    let final_rows: usize = batches.iter().map(|b| b.num_rows()).sum();
    let root_stat = stats.last().unwrap();
    assert_eq!(
        root_stat.row_count, final_rows,
        "root node row_count in stats does not match actual output"
    );
    assert!(
        root_stat.allocated_bytes > 0,
        "root node allocated_bytes should be > 0"
    );
    assert!(
        root_stat.logical_bytes > 0,
        "root node logical_bytes should be > 0"
    );
    assert!(
        root_stat.allocated_bytes >= root_stat.logical_bytes,
        "allocated_bytes ({}) must be >= logical_bytes ({})",
        root_stat.allocated_bytes,
        root_stat.logical_bytes
    );
}
