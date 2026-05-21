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

// 
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
    /// Logical byte size of all batches fed into this node (sum across all children).
    pub input_bytes: usize,
    /// Logical byte size of all batches produced by this node.
    pub output_bytes: usize,
    /// `input_bytes + output_bytes`.
    pub cost: usize,
    /// Total number of output rows across all batches.
    pub row_count: usize,
    /// Largest single batch (in rows) produced by this node.
    /// Compare against `GpuScanExec.gpu_batch_size` to verify the memory contract.
    pub max_batch_rows: usize,
}

impl NodeMemoryStats {
    pub(crate) fn collect(node_name: &str, input: &[RecordBatch], output: &[RecordBatch]) -> Self {
        let input_bytes: usize = input.iter().map(|b| batch_logical_size(b)).sum();
        let output_bytes: usize = output.iter().map(|b| batch_logical_size(b)).sum();
        Self {
            node_name: node_name.to_string(),
            allocated_bytes: output.iter().map(|b| batch_allocated_size(b)).sum(),
            input_bytes,
            output_bytes,
            cost: input_bytes + output_bytes,
            row_count: output.iter().map(|b| b.num_rows()).sum(),
            max_batch_rows: output.iter().map(|b| b.num_rows()).max().unwrap_or(0),
        }
    }
}

/// Recursively strip all GPU wrapper nodes from a plan tree, returning a
/// structurally identical tree composed of plain DataFusion CPU nodes.
pub fn strip_gpu_tree(plan: Arc<dyn ExecutionPlan>) -> Result<Arc<dyn ExecutionPlan>> {
    let (cpu_node, _) = strip_gpu(plan);
    let stripped_children = cpu_node
        .children()
        .into_iter()
        .map(|c| strip_gpu_tree(c.clone()))
        .collect::<Result<Vec<_>>>()?;
    cpu_node.with_new_children(stripped_children)
}

/// Execute a physical plan one node at a time, bottom-up, on CPU.
///
/// GPU wrapper nodes (`GpuFilterExec`, `GpuScanExec`, …) are stripped to their
/// inner DataFusion CPU nodes before execution.  The memory boundary encoded in
/// `GpuScanExec.gpu_batch_size` is preserved: the `TaskContext` batch size is
/// overridden to that value so the Parquet reader produces the same batch sizes
/// the GPU planner computed.
///
/// For each node the function:
/// 1. Strips the GPU wrapper (if any) → CPU node + optional batch_size.
/// 2. Applies the batch_size override to `TaskContext` if present.
/// 3. Recurses into the CPU node's children.
/// 4. Wraps each child's results in `MemoryExec` (DataFusion's in-memory source).
/// 5. Calls `collect()` on the isolated CPU node with its `MemoryExec` stubs.
/// 6. Calls `on_node(cpu_node_name, &input_batches, &output_batches)`.
///
/// TODO: this implementation OOMs on wide joins (e.g. hash-join.sql at SF=1) because
/// it calls `collect()` on every child before the parent runs, holding both full inputs
/// of a join in memory simultaneously. Fix by making execution streaming:
///
/// 1. `StreamSourceExec` — a custom `ExecutionPlan` wrapping a `SendableRecordBatchStream`
///    that returns it from `execute(0, ctx)`. Lets a live stream be passed as a child to
///    any DataFusion operator without materializing it first.
///
/// 2. `InstrumentedStream` — a stream adaptor that accumulates `NodeMemoryStats` as
///    batches flow through (row counts, allocated/logical bytes, max batch size) and
///    fires the `on_node` callback when the stream is exhausted.
///
/// 3. Refactor `execute_node_by_node` to return `SendableRecordBatchStream`:
///    - For each child, recurse to get a child stream.
///    - Wrap each child stream in `InstrumentedStream` → `StreamSourceExec`.
///    - Call `cpu_node.with_new_children(stream_sources)?.execute(0, task_ctx)`.
///    - Return the resulting stream wrapped in its own `InstrumentedStream`.
///
/// 4. Update `execute_node_by_node_instrumented` to `collect()` only the root stream;
///    all intermediate stats are populated by `InstrumentedStream` as the root is consumed.
///
/// 5. Change `on_node` signature from `FnMut(&str, &[RecordBatch], &[RecordBatch])` to
///    `FnMut(&str, &NodeMemoryStats)` since intermediate batches are no longer available
///    all at once.
///
/// After the fix: hash join build side is still materialized by DataFusion internally
/// (unavoidable), but the probe side streams through `batch_size` rows at a time.
/// Peak memory ≈ build side size + 2 × batch_size × row_width.
pub async fn execute_node_by_node(
    root: Arc<dyn ExecutionPlan>,
    task_ctx: Arc<TaskContext>,
    on_node: &mut dyn FnMut(&str, &[RecordBatch], &[RecordBatch]),
) -> Result<Vec<RecordBatch>> {
    let (cpu_node, batch_size_override) = strip_gpu(root);

    let task_ctx = match batch_size_override {
        Some(n) => with_batch_size(task_ctx, n),
        None => task_ctx,
    };

    let mut stub_children: Vec<Arc<dyn ExecutionPlan>> = vec![];
    let mut input_batches: Vec<RecordBatch> = vec![];
    for child in cpu_node.children() {
        let child_batches =
            Box::pin(execute_node_by_node(child.clone(), task_ctx.clone(), on_node)).await?;
        input_batches.extend(child_batches.iter().cloned());
        let mem_exec = MemoryExec::try_new(&[child_batches], child.schema(), None)?;
        stub_children.push(Arc::new(mem_exec));
    }

    let node_name = cpu_node.name().to_string();
    let node = cpu_node.with_new_children(stub_children)?;
    let batches = collect(node, task_ctx).await?;
    on_node(&node_name, &input_batches, &batches);
    Ok(batches)
}

/// Convenience wrapper: runs [`execute_node_by_node`] and collects
/// [`NodeMemoryStats`] per node in post-order.
pub async fn execute_node_by_node_instrumented(
    root: Arc<dyn ExecutionPlan>,
    task_ctx: Arc<TaskContext>,
    stats: &mut Vec<NodeMemoryStats>,
) -> Result<Vec<RecordBatch>> {
    execute_node_by_node(root, task_ctx, &mut |name, input, output| {
        stats.push(NodeMemoryStats::collect(name, input, output));
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

// Tests live in tests/test_cpu_executor.rs
