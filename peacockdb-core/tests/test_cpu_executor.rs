use std::path::PathBuf;
use std::sync::Arc;

use datafusion::arrow::array::{Int64Array, StringViewArray};
use datafusion::physical_plan::ExecutionPlan;

use peacockdb_core::cpu_executor::{
    execute_node_by_node, execute_node_by_node_instrumented, NodeMemoryStats,
};
use peacockdb_core::create_context_with_tables;

fn testdata_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../testdata/tpch.minimal")
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

#[tokio::test]
async fn test_cpu_results_match_direct_execution() {
    let ctx = make_ctx(FULL_BUDGET).await;
    let query = "SELECT n_name FROM nation WHERE n_regionkey >= 0 ORDER BY n_name";

    let reference: Vec<datafusion::arrow::record_batch::RecordBatch> =
        ctx.sql(query).await.unwrap().collect().await.unwrap();
    let ref_names: Vec<String> = reference
        .iter()
        .flat_map(|b| {
            b.column(0)
                .as_any()
                .downcast_ref::<StringViewArray>()
                .unwrap()
                .iter()
                .map(|v| v.unwrap().to_string())
        })
        .collect();

    let plan = ctx
        .sql(query)
        .await
        .unwrap()
        .create_physical_plan()
        .await
        .unwrap();
    let task_ctx = ctx.task_ctx();
    let cpu_batches = execute_node_by_node(plan, task_ctx, &mut |_, _| {}).await.unwrap();
    let cpu_names: Vec<String> = cpu_batches
        .iter()
        .flat_map(|b| {
            b.column(0)
                .as_any()
                .downcast_ref::<StringViewArray>()
                .unwrap()
                .iter()
                .map(|v| v.unwrap().to_string())
        })
        .collect();

    assert_eq!(
        cpu_names, ref_names,
        "CPU executor result differs from direct execution"
    );
    assert_eq!(cpu_names.len(), 25, "nation table must have 25 rows");
}

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
