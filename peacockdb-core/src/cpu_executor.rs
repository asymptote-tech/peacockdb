use std::sync::Arc;

use datafusion::arrow::array::{BinaryArray, LargeBinaryArray, LargeStringArray, StringArray};
use datafusion::arrow::datatypes::DataType;
use datafusion::arrow::record_batch::RecordBatch;
use datafusion::error::Result;
use datafusion::execution::TaskContext;
use datafusion::physical_plan::memory::MemoryExec;
use datafusion::physical_plan::{collect, ExecutionPlan};

use crate::gpu_rule::{
    GpuAggregateExec, GpuCoalesceBatchesExec, GpuCoalescePartitionsExec, GpuFilterExec,
    GpuHashJoinExec, GpuProjectExec, GpuRepartitionExec, GpuScanExec, GpuSortExec,
    GpuSortPreservingMergeExec,
};

// ---------------------------------------------------------------------------
// GPU → CPU unwrapping
// ---------------------------------------------------------------------------

/// Strip a GPU wrapper from a node, returning the inner CPU `ExecutionPlan`.
///
/// `GpuScanExec` also carries a `batch_size` that must be forwarded to the
/// `TaskContext` so the Parquet reader produces correctly-sized batches.
/// Every other GPU node is a transparent shell — its inner CPU node already
/// carries the right configuration (e.g. `CoalesceBatchesExec.target_batch_size`
/// was patched by `GpuMemoryBudgetRule`).
///
/// Non-GPU nodes are returned unchanged with `None` for the batch size.
fn strip_gpu(node: Arc<dyn ExecutionPlan>) -> (Arc<dyn ExecutionPlan>, Option<usize>) {
    macro_rules! try_strip {
        ($ty:ty) => {
            if let Some(n) = node.as_any().downcast_ref::<$ty>() {
                return (n.inner().clone(), None);
            }
        };
    }

    // GpuScanExec is special: it carries the memory-budget batch size.
    if let Some(scan) = node.as_any().downcast_ref::<GpuScanExec>() {
        return (scan.inner().clone(), Some(scan.gpu_batch_size));
    }

    try_strip!(GpuFilterExec);
    try_strip!(GpuProjectExec);
    try_strip!(GpuAggregateExec);
    try_strip!(GpuHashJoinExec);
    try_strip!(GpuSortExec);
    try_strip!(GpuCoalesceBatchesExec);
    try_strip!(GpuCoalescePartitionsExec);
    try_strip!(GpuRepartitionExec);
    try_strip!(GpuSortPreservingMergeExec);

    // Plain CPU node — pass through unchanged.
    (node, None)
}

/// Apply a batch-size override to a `TaskContext`, returning the updated context.
/// The override comes from `GpuScanExec.gpu_batch_size`, which was computed by
/// `GpuMemoryBudgetRule` to keep GPU memory within budget.  We honour the same
/// limit on CPU so that peak working-set size stays within the same bound.
fn with_batch_size(ctx: Arc<TaskContext>, batch_size: usize) -> Arc<TaskContext> {
    let new_config = ctx.session_config().clone().with_batch_size(batch_size);
    Arc::new(TaskContext::new(
        ctx.task_id(),
        ctx.session_id(),
        new_config,
        ctx.scalar_functions().clone(),
        ctx.aggregate_functions().clone(),
        ctx.window_functions().clone(),
        ctx.runtime_env(),
    ))
}

// ---------------------------------------------------------------------------
// Node-by-node CPU execution
// ---------------------------------------------------------------------------

/// Per-node memory stats collected via the `on_node` callback.
pub struct NodeMemoryStats {
    /// Name of the CPU node that was executed (GPU wrapper already stripped).
    pub node_name: String,
    /// Sum of `get_array_memory_size()` across all output batches (allocated upper bound).
    pub allocated_bytes: usize,
    /// Sum of logical (exact) sizes across all output batches.
    pub logical_bytes: usize,
    /// Total number of output rows across all batches.
    pub row_count: usize,
    /// Largest single batch (in rows) produced by this node.
    /// Compare against `GpuScanExec.gpu_batch_size` to verify the memory contract.
    pub max_batch_rows: usize,
}

impl NodeMemoryStats {
    pub(crate) fn collect(node_name: &str, batches: &[RecordBatch]) -> Self {
        Self {
            node_name: node_name.to_string(),
            allocated_bytes: batches.iter().map(|b| batch_allocated_size(b)).sum(),
            logical_bytes: batches.iter().map(|b| batch_logical_size(b)).sum(),
            row_count: batches.iter().map(|b| b.num_rows()).sum(),
            max_batch_rows: batches.iter().map(|b| b.num_rows()).max().unwrap_or(0),
        }
    }
}

/// Execute a physical plan one node at a time, bottom-up, on CPU.
///
/// GPU wrapper nodes (`GpuFilterExec`, `GpuScanExec`, …) are stripped to their
/// inner DataFusion CPU nodes before execution.  The memory boundary encoded in
/// `GpuScanExec.gpu_batch_size` is preserved: the `TaskContext` batch size is
/// overridden to that value so the Parquet reader produces the same batch sizes
/// the GPU planner computed.
///
/// `on_node` is called after each node completes, in post-order (children before
/// parent), with the CPU node name and its output batches.  Pass `&mut |_, _| {}`
/// when no instrumentation is needed.
///
/// For each node the function:
/// 1. Strips the GPU wrapper (if any) → CPU node + optional batch_size.
/// 2. Applies the batch_size override to `TaskContext` if present.
/// 3. Recurses into the CPU node's children.
/// 4. Wraps each child's results in `MemoryExec` (DataFusion's in-memory source).
/// 5. Calls `collect()` on the isolated CPU node with its `MemoryExec` stubs.
/// 6. Calls `on_node(cpu_node_name, &batches)`.
pub async fn execute_node_by_node(
    root: Arc<dyn ExecutionPlan>,
    task_ctx: Arc<TaskContext>,
    on_node: &mut dyn FnMut(&str, &[RecordBatch]),
) -> Result<Vec<RecordBatch>> {
    let (cpu_node, batch_size_override) = strip_gpu(root);

    let task_ctx = match batch_size_override {
        Some(n) => with_batch_size(task_ctx, n),
        None => task_ctx,
    };

    let mut stub_children: Vec<Arc<dyn ExecutionPlan>> = vec![];
    for child in cpu_node.children() {
        let child_batches =
            Box::pin(execute_node_by_node(child.clone(), task_ctx.clone(), on_node)).await?;
        let mem_exec = MemoryExec::try_new(&[child_batches], child.schema(), None)?;
        stub_children.push(Arc::new(mem_exec));
    }

    let node_name = cpu_node.name().to_string();
    let node = cpu_node.with_new_children(stub_children)?;
    let batches = collect(node, task_ctx).await?;
    on_node(&node_name, &batches);
    Ok(batches)
}

/// Convenience wrapper: runs [`execute_node_by_node`] and collects
/// [`NodeMemoryStats`] per node in post-order.
pub async fn execute_node_by_node_instrumented(
    root: Arc<dyn ExecutionPlan>,
    task_ctx: Arc<TaskContext>,
    stats: &mut Vec<NodeMemoryStats>,
) -> Result<Vec<RecordBatch>> {
    execute_node_by_node(root, task_ctx, &mut |name, batches| {
        stats.push(NodeMemoryStats::collect(name, batches));
    })
    .await
}

// ---------------------------------------------------------------------------
// Memory size helpers
// ---------------------------------------------------------------------------

/// Sum of allocated buffer capacities across all columns.
///
/// Uses `get_array_memory_size()` which walks all buffers recursively
/// (validity bitmap + values + offsets + children). Safe upper bound —
/// may over-report for sliced batches or over-allocated builders.
pub fn batch_allocated_size(batch: &RecordBatch) -> usize {
    batch
        .columns()
        .iter()
        .map(|col| col.get_array_memory_size())
        .sum()
}

/// Exact logical byte size of a `RecordBatch`.
///
/// For fixed-width types this is derived from the schema and row count.
/// For variable-width types (`Utf8`, `Binary`, etc.) the offsets buffer is
/// read to get the exact data byte count.  Unknown / nested types contribute 0.
pub fn batch_logical_size(batch: &RecordBatch) -> usize {
    let rows = batch.num_rows();
    batch
        .schema()
        .fields()
        .iter()
        .zip(batch.columns().iter())
        .map(|(field, col)| {
            let bitmap_bytes = (rows + 7) / 8;
            let data_bytes = match field.data_type() {
                DataType::Boolean => (rows + 7) / 8,
                DataType::Int8 | DataType::UInt8 => rows,
                DataType::Int16 | DataType::UInt16 => rows * 2,
                DataType::Int32
                | DataType::UInt32
                | DataType::Float32
                | DataType::Date32 => rows * 4,
                DataType::Int64
                | DataType::UInt64
                | DataType::Float64
                | DataType::Date64 => rows * 8,
                DataType::Timestamp(_, _) => rows * 8,
                DataType::Utf8 => {
                    let offset_bytes = (rows + 1) * 4; // i32 offsets
                    let data = col
                        .as_any()
                        .downcast_ref::<StringArray>()
                        .map(|arr| {
                            let offsets = arr.value_offsets();
                            if offsets.is_empty() {
                                0usize
                            } else {
                                (offsets[rows] - offsets[0]) as usize
                            }
                        })
                        .unwrap_or(0);
                    offset_bytes + data
                }
                DataType::LargeUtf8 => {
                    let offset_bytes = (rows + 1) * 8; // i64 offsets
                    let data = col
                        .as_any()
                        .downcast_ref::<LargeStringArray>()
                        .map(|arr| {
                            let offsets = arr.value_offsets();
                            if offsets.is_empty() {
                                0usize
                            } else {
                                (offsets[rows] - offsets[0]) as usize
                            }
                        })
                        .unwrap_or(0);
                    offset_bytes + data
                }
                DataType::Binary => {
                    let offset_bytes = (rows + 1) * 4;
                    let data = col
                        .as_any()
                        .downcast_ref::<BinaryArray>()
                        .map(|arr| {
                            let offsets = arr.value_offsets();
                            if offsets.is_empty() {
                                0usize
                            } else {
                                (offsets[rows] - offsets[0]) as usize
                            }
                        })
                        .unwrap_or(0);
                    offset_bytes + data
                }
                DataType::LargeBinary => {
                    let offset_bytes = (rows + 1) * 8;
                    let data = col
                        .as_any()
                        .downcast_ref::<LargeBinaryArray>()
                        .map(|arr| {
                            let offsets = arr.value_offsets();
                            if offsets.is_empty() {
                                0usize
                            } else {
                                (offsets[rows] - offsets[0]) as usize
                            }
                        })
                        .unwrap_or(0);
                    offset_bytes + data
                }
                _ => 0,
            };
            bitmap_bytes + data_bytes
        })
        .sum()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::create_context_with_tables;
    use datafusion::arrow::array::{Int64Array, StringViewArray};
    use std::path::PathBuf;

    fn testdata_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../testdata/tpchsf1")
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
        use crate::gpu_rule::GpuScanExec;
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

        let reference: Vec<RecordBatch> =
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

        // ── Extract the batch_size ceiling from GpuScanExec ─────────────────
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

}
