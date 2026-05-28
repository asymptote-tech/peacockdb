use std::any::Any;
use std::fmt;
use std::sync::Arc;

use datafusion::arrow::datatypes::SchemaRef;
use datafusion::common::config::ConfigOptions;
use datafusion::common::tree_node::{Transformed, TreeNode};
use datafusion::common::Result;
use datafusion::datasource::physical_plan::ParquetExec;
use datafusion::execution::TaskContext;
use datafusion::physical_optimizer::PhysicalOptimizerRule;
use datafusion::physical_plan::coalesce_batches::CoalesceBatchesExec;
use datafusion::physical_plan::coalesce_partitions::CoalescePartitionsExec;
use datafusion::physical_plan::aggregates::AggregateExec;
use datafusion::physical_plan::filter::FilterExec;
use datafusion::physical_plan::joins::HashJoinExec;
use datafusion::physical_plan::projection::ProjectionExec;
use datafusion::physical_plan::repartition::RepartitionExec;
use datafusion::physical_plan::sorts::sort::SortExec;
use datafusion::physical_plan::sorts::sort_preserving_merge::SortPreservingMergeExec;
use datafusion::physical_plan::{
    DisplayAs, DisplayFormatType, ExecutionPlan, PlanProperties, SendableRecordBatchStream,
};

// ---------------------------------------------------------------------------
// GPU exec node stubs (delegate to inner CPU node)
// ---------------------------------------------------------------------------

/// Optional extra display info appended after the node name in plan output.
/// Implement with a non-empty string to annotate a specific GPU node type.
trait GpuExtraDisplay {
    fn extra_display_info(&self) -> String {
        String::new()
    }
}

macro_rules! gpu_exec_node {
    ($name:ident) => {
        #[derive(Debug)]
        pub struct $name {
            inner: Arc<dyn ExecutionPlan>,
        }

        impl $name {
            pub fn new(inner: Arc<dyn ExecutionPlan>) -> Self {
                Self { inner }
            }
            pub fn inner(&self) -> &Arc<dyn ExecutionPlan> {
                &self.inner
            }
        }

        impl DisplayAs for $name {
            fn fmt_as(&self, _t: DisplayFormatType, f: &mut fmt::Formatter) -> fmt::Result {
                let extra = self.extra_display_info();
                if extra.is_empty() {
                    write!(f, "{}", stringify!($name))
                } else {
                    write!(f, "{}: {}", stringify!($name), extra)
                }
            }
        }

        impl ExecutionPlan for $name {
            fn as_any(&self) -> &dyn Any {
                self
            }
            fn schema(&self) -> SchemaRef {
                self.inner.schema()
            }
            fn properties(&self) -> &PlanProperties {
                self.inner.properties()
            }
            fn children(&self) -> Vec<&Arc<dyn ExecutionPlan>> {
                self.inner.children()
            }
            fn with_new_children(
                self: Arc<Self>,
                children: Vec<Arc<dyn ExecutionPlan>>,
            ) -> Result<Arc<dyn ExecutionPlan>> {
                let new_inner = self.inner.clone().with_new_children(children)?;
                Ok(Arc::new(Self::new(new_inner)))
            }
            fn name(&self) -> &str {
                stringify!($name)
            }
            fn execute(
                &self,
                partition: usize,
                context: Arc<TaskContext>,
            ) -> Result<SendableRecordBatchStream> {
                self.inner.execute(partition, context)
            }
        }
    };
}

gpu_exec_node!(GpuFilterExec);
impl GpuExtraDisplay for GpuFilterExec {}

gpu_exec_node!(GpuProjectExec);
impl GpuExtraDisplay for GpuProjectExec {}

gpu_exec_node!(GpuAggregateExec);
impl GpuExtraDisplay for GpuAggregateExec {
    fn extra_display_info(&self) -> String {
        let agg = self.inner.as_any().downcast_ref::<AggregateExec>().unwrap();
        let groups: Vec<&str> = agg.group_expr().expr().iter()
            .map(|(_, name): &(_, String)| name.as_str())
            .collect();
        let aggrs: Vec<&str> = agg.aggr_expr().iter()
            .map(|e| e.name())
            .collect();
        format!("group_by=[{}], aggr=[{}]", groups.join(", "), aggrs.join(", "))
    }
}

gpu_exec_node!(GpuHashJoinExec);
impl GpuExtraDisplay for GpuHashJoinExec {}

gpu_exec_node!(GpuSortExec);
impl GpuExtraDisplay for GpuSortExec {}

gpu_exec_node!(GpuCoalesceBatchesExec);
impl GpuExtraDisplay for GpuCoalesceBatchesExec {
    fn extra_display_info(&self) -> String {
        let cb = self.inner.as_any().downcast_ref::<CoalesceBatchesExec>().unwrap();
        format!("target_batch_size={}", cb.target_batch_size())
    }
}

gpu_exec_node!(GpuCoalescePartitionsExec);
impl GpuExtraDisplay for GpuCoalescePartitionsExec {}

gpu_exec_node!(GpuRepartitionExec);
impl GpuExtraDisplay for GpuRepartitionExec {
    fn extra_display_info(&self) -> String {
        let rp = self.inner.as_any().downcast_ref::<RepartitionExec>().unwrap();
        let partitioning = rp.partitioning();
        let input_partitions = rp.input().properties().output_partitioning().partition_count();
        format!("partitioning={partitioning}, input_partitions={input_partitions}")
    }
}

gpu_exec_node!(GpuSortPreservingMergeExec);
impl GpuExtraDisplay for GpuSortPreservingMergeExec {
    fn extra_display_info(&self) -> String {
        let spm = self.inner.as_any().downcast_ref::<SortPreservingMergeExec>().unwrap();
        format!("[{}]", spm.expr())
    }
}

// ---------------------------------------------------------------------------
// GpuScanExec — wraps ParquetExec to override batch_size at execution time
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub struct GpuScanExec {
    inner: Arc<dyn ExecutionPlan>,
    pub gpu_batch_size: usize,
}

impl GpuScanExec {
    pub fn new(inner: Arc<dyn ExecutionPlan>, gpu_batch_size: usize) -> Self {
        Self {
            inner,
            gpu_batch_size,
        }
    }
    pub fn inner(&self) -> &Arc<dyn ExecutionPlan> {
        &self.inner
    }
}

impl DisplayAs for GpuScanExec {
    fn fmt_as(&self, _t: DisplayFormatType, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "GpuScanExec: batch_size={}", self.gpu_batch_size)
    }
}

impl ExecutionPlan for GpuScanExec {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn schema(&self) -> SchemaRef {
        self.inner.schema()
    }
    fn properties(&self) -> &PlanProperties {
        self.inner.properties()
    }
    fn children(&self) -> Vec<&Arc<dyn ExecutionPlan>> {
        self.inner.children()
    }
    fn with_new_children(
        self: Arc<Self>,
        children: Vec<Arc<dyn ExecutionPlan>>,
    ) -> Result<Arc<dyn ExecutionPlan>> {
        let new_inner = self.inner.clone().with_new_children(children)?;
        Ok(Arc::new(Self::new(new_inner, self.gpu_batch_size)))
    }
    fn name(&self) -> &str {
        "GpuScanExec"
    }
    fn execute(
        &self,
        partition: usize,
        context: Arc<TaskContext>,
    ) -> Result<SendableRecordBatchStream> {
        let new_config = context
            .session_config()
            .clone()
            .with_batch_size(self.gpu_batch_size);
        let new_ctx = Arc::new(TaskContext::new(
            context.task_id(),
            context.session_id(),
            new_config,
            context.scalar_functions().clone(),
            context.aggregate_functions().clone(),
            context.window_functions().clone(),
            context.runtime_env(),
        ));
        self.inner.execute(partition, new_ctx)
    }
}

// ---------------------------------------------------------------------------
// GpuExecutionRule — replace CPU nodes with GPU wrappers
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub struct GpuExecutionRule;

impl PhysicalOptimizerRule for GpuExecutionRule {
    fn optimize(
        &self,
        plan: Arc<dyn ExecutionPlan>,
        _config: &ConfigOptions,
    ) -> Result<Arc<dyn ExecutionPlan>> {
        let result = plan.transform_up(|node: Arc<dyn ExecutionPlan>| {
            let new_node: Arc<dyn ExecutionPlan> = if node.as_any().is::<FilterExec>() {
                Arc::new(GpuFilterExec::new(node))
            } else if node.as_any().is::<ProjectionExec>() {
                Arc::new(GpuProjectExec::new(node))
            } else if node.as_any().is::<AggregateExec>() {
                Arc::new(GpuAggregateExec::new(node))
            } else if node.as_any().is::<HashJoinExec>() {
                Arc::new(GpuHashJoinExec::new(node))
            } else if node.as_any().is::<SortExec>() {
                Arc::new(GpuSortExec::new(node))
            } else if node.as_any().is::<CoalesceBatchesExec>() {
                Arc::new(GpuCoalesceBatchesExec::new(node))
            } else if node.as_any().is::<CoalescePartitionsExec>() {
                Arc::new(GpuCoalescePartitionsExec::new(node))
            } else if node.as_any().is::<RepartitionExec>() {
                Arc::new(GpuRepartitionExec::new(node))
            } else if node.as_any().is::<SortPreservingMergeExec>() {
                Arc::new(GpuSortPreservingMergeExec::new(node))
            } else {
                return Ok(Transformed::no(node));
            };
            Ok(Transformed::yes(new_node))
        })?;
        Ok(result.data)
    }

    fn name(&self) -> &str {
        "gpu_execution"
    }

    fn schema_check(&self) -> bool {
        true
    }
}

// ---------------------------------------------------------------------------
// Memory analysis
// ---------------------------------------------------------------------------

/// Estimated byte width of a single row for the given schema.
/// Uses `DataType::primitive_width()` for fixed-size types,
/// falls back to 32 bytes for variable-length types (Utf8, Binary, etc.).
pub fn row_width(schema: &SchemaRef) -> usize {
    schema
        .fields()
        .iter()
        .map(|f| f.data_type().primitive_width().unwrap_or(32))
        .sum::<usize>()
        .max(1) // at least 1 byte per row
}

// ---------------------------------------------------------------------------
// Estimator traits
// ---------------------------------------------------------------------------

/// Estimates the selectivity of a filter operator: the fraction of input rows
/// that pass the predicate (0.0 = nothing passes, 1.0 = everything passes).
///
/// TODO: Implement a statistics-based estimator that uses DataFusion's
/// `ExecutionPlan::statistics()` — e.g., histogram-based estimation for range
/// filters, NDV-based estimation for equality filters.
pub(crate) trait SelectivityEstimator {
    fn estimate(&self, plan: &Arc<dyn ExecutionPlan>) -> f64;
}

/// Estimates the cardinality ratio of a join: output_rows / max(left_rows, right_rows).
/// A ratio of 1.0 means 1:1, >1.0 means fan-out, <1.0 means a semi-join or filtering join.
///
/// TODO: Implement a statistics-based estimator that uses DataFusion's
/// `ExecutionPlan::statistics()` — e.g., foreign-key detection for 1:1 joins,
/// NDV-based join selectivity for many-to-many.
pub(crate) trait CardinalityEstimator {
    fn estimate(&self, plan: &Arc<dyn ExecutionPlan>) -> f64;
}

/// Assumes all filters pass 100% of rows.
pub(crate) struct TrivialSelectivityEstimator;

impl SelectivityEstimator for TrivialSelectivityEstimator {
    fn estimate(&self, _plan: &Arc<dyn ExecutionPlan>) -> f64 {
        1.0
    }
}

/// Assumes all joins are 1:1 (output rows = input rows).
pub(crate) struct TrivialCardinalityEstimator;

impl CardinalityEstimator for TrivialCardinalityEstimator {
    fn estimate(&self, _plan: &Arc<dyn ExecutionPlan>) -> f64 {
        1.0
    }
}

// ---------------------------------------------------------------------------
// Subtree memory model
// ---------------------------------------------------------------------------

/// Result of analyzing a plan subtree's memory usage.
///
/// Memory is modeled as a linear function of the scan batch size N.
/// `output_row_ratio` tracks the cumulative row multiplier: if a filter has
/// 50% selectivity, downstream operators see 0.5 × N rows instead of N.
#[derive(Clone, Copy)]
pub struct SubtreeMemory {
    /// Peak GPU memory as bytes per scan-batch-row N:
    /// `peak_bytes = subtree_max_row_bytes * N`.
    pub subtree_max_row_bytes: usize,
    /// Output row width in bytes (per output row).
    pub output_width: usize,
    /// Ratio of output rows to original batch size N.
    /// 1.0 means row count is preserved; <1.0 after filters; >1.0 after fan-out joins.
    pub output_row_ratio: f64,
    /// Estimated input bytes flowing into this node per scan-batch-row N.
    pub input_row_bytes: usize,
    /// Estimated output bytes produced by this node per scan-batch-row N.
    pub output_row_bytes: usize,
}

/// Walk the plan tree and compute peak memory as a linear function of batch size N.
///
/// Per-operator memory = input batch + output batch, where the row counts are
/// adjusted by selectivity (filters) and cardinality (joins) estimators.
pub fn analyze_memory(plan: &Arc<dyn ExecutionPlan>) -> SubtreeMemory {
    analyze_memory_with(
        plan,
        &TrivialSelectivityEstimator,
        &TrivialCardinalityEstimator,
    )
}

/// Compute a node's `SubtreeMemory` given already-computed child results.
/// Does not recurse — callers are responsible for walking children first.
pub(crate) fn node_memory_with(
    plan: &Arc<dyn ExecutionPlan>,
    child_mems: &[SubtreeMemory],
    selectivity: &dyn SelectivityEstimator,
    cardinality: &dyn CardinalityEstimator,
) -> SubtreeMemory {
    let output_width = row_width(&plan.schema());

    if child_mems.is_empty() {
        return SubtreeMemory {
            subtree_max_row_bytes: output_width,
            output_width,
            output_row_ratio: 1.0,
            input_row_bytes: 0,
            output_row_bytes: output_width,
        };
    }

    match plan.name() {
        "GpuFilterExec" => {
            let child = child_mems[0];
            let sel = selectivity.estimate(plan);
            let input_rows_bytes = (child.output_row_ratio * child.output_width as f64) as usize;
            let output_rows_bytes = (sel * output_width as f64) as usize;
            SubtreeMemory {
                subtree_max_row_bytes: child
                    .subtree_max_row_bytes
                    .max(input_rows_bytes + output_rows_bytes),
                output_width,
                output_row_ratio: child.output_row_ratio * sel,
                input_row_bytes: input_rows_bytes,
                output_row_bytes: output_rows_bytes,
            }
        }
        "GpuProjectExec" | "GpuAggregateExec" => {
            let child = child_mems[0];
            let input_rows_bytes = (child.output_row_ratio * child.output_width as f64) as usize;
            let output_rows_bytes = (child.output_row_ratio * output_width as f64) as usize;
            SubtreeMemory {
                subtree_max_row_bytes: child
                    .subtree_max_row_bytes
                    .max(input_rows_bytes + output_rows_bytes),
                output_width,
                output_row_ratio: child.output_row_ratio,
                input_row_bytes: input_rows_bytes,
                output_row_bytes: output_rows_bytes,
            }
        }
        "GpuHashJoinExec" => {
            let (build, probe) = (child_mems[0], child_mems[1]);
            let card = cardinality.estimate(plan);
            let build_bytes = (build.output_row_ratio * build.output_width as f64) as usize;
            let probe_bytes = (probe.output_row_ratio * probe.output_width as f64) as usize;
            let output_ratio = build.output_row_ratio.max(probe.output_row_ratio) * card;
            let output_bytes = (output_ratio * output_width as f64) as usize;
            let own = build_bytes + probe_bytes + output_bytes;
            SubtreeMemory {
                subtree_max_row_bytes: build
                    .subtree_max_row_bytes
                    .max(probe.subtree_max_row_bytes)
                    .max(own),
                output_width,
                output_row_ratio: output_ratio,
                input_row_bytes: build_bytes + probe_bytes,
                output_row_bytes: output_bytes,
            }
        }
        "CrossJoinExec" | "NestedLoopJoinExec" => {
            let (left, right) = (child_mems[0], child_mems[1]);
            let card = cardinality.estimate(plan);
            let left_bytes = (left.output_row_ratio * left.output_width as f64) as usize;
            let right_bytes = (right.output_row_ratio * right.output_width as f64) as usize;
            let output_ratio = left.output_row_ratio * right.output_row_ratio * card;
            let output_bytes = (output_ratio * output_width as f64) as usize;
            let own = left_bytes + right_bytes + output_bytes;
            SubtreeMemory {
                subtree_max_row_bytes: left
                    .subtree_max_row_bytes
                    .max(right.subtree_max_row_bytes)
                    .max(own),
                output_width,
                output_row_ratio: output_ratio,
                input_row_bytes: left_bytes + right_bytes,
                output_row_bytes: output_bytes,
            }
        }
        "GpuSortExec" => {
            let child = child_mems[0];
            let input_bytes = (child.output_row_ratio * child.output_width as f64) as usize;
            SubtreeMemory {
                subtree_max_row_bytes: child.subtree_max_row_bytes.max(2 * input_bytes),
                output_width,
                output_row_ratio: child.output_row_ratio,
                input_row_bytes: input_bytes,
                output_row_bytes: input_bytes,
            }
        }
        _ => {
            let max_peak = child_mems
                .iter()
                .map(|c| c.subtree_max_row_bytes)
                .max()
                .unwrap_or(output_width);
            let max_ratio = child_mems
                .iter()
                .map(|c| c.output_row_ratio)
                .fold(1.0_f64, f64::max);
            let max_child_output = child_mems
                .iter()
                .map(|c| c.output_row_bytes)
                .max()
                .unwrap_or(output_width);
            SubtreeMemory {
                subtree_max_row_bytes: max_peak,
                output_width,
                output_row_ratio: max_ratio,
                input_row_bytes: max_child_output,
                output_row_bytes: max_child_output,
            }
        }
    }
}

pub(crate) fn analyze_memory_with(
    plan: &Arc<dyn ExecutionPlan>,
    selectivity: &dyn SelectivityEstimator,
    cardinality: &dyn CardinalityEstimator,
) -> SubtreeMemory {
    let child_mems: Vec<SubtreeMemory> = plan
        .children()
        .iter()
        .map(|c| analyze_memory_with(c, selectivity, cardinality))
        .collect();
    node_memory_with(plan, &child_mems, selectivity, cardinality)
}

/// Walk the plan tree once and return per-node memory info in pre-order.
/// Each entry is `(name, depth, SubtreeMemory)`. O(n) — each node is visited once.
pub fn analyze_memory_nodes(plan: &Arc<dyn ExecutionPlan>) -> Vec<(String, usize, SubtreeMemory)> {
    fn walk(
        plan: &Arc<dyn ExecutionPlan>,
        depth: usize,
        result: &mut Vec<(String, usize, SubtreeMemory)>,
    ) -> SubtreeMemory {
        let my_idx = result.len();
        result.push((plan.name().to_string(), depth, SubtreeMemory {
            subtree_max_row_bytes: 0,
            output_width: 0,
            output_row_ratio: 0.0,
            input_row_bytes: 0,
            output_row_bytes: 0,
        }));
        let child_mems: Vec<SubtreeMemory> = plan
            .children()
            .iter()
            .map(|c| walk(c, depth + 1, result))
            .collect();
        let mem = node_memory_with(
            plan,
            &child_mems,
            &TrivialSelectivityEstimator,
            &TrivialCardinalityEstimator,
        );
        result[my_idx].2 = mem;
        mem
    }
    let mut result = Vec::new();
    walk(plan, 0, &mut result);
    result
}

// ---------------------------------------------------------------------------
// GpuMemoryBudgetRule — compute batch size from memory budget, wrap scans
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub struct GpuMemoryBudgetRule {
    gpu_memory_budget: usize,
}

impl GpuMemoryBudgetRule {
    pub fn new(gpu_memory_budget: usize) -> Self {
        Self { gpu_memory_budget }
    }
}

impl PhysicalOptimizerRule for GpuMemoryBudgetRule {
    fn optimize(
        &self,
        plan: Arc<dyn ExecutionPlan>,
        config: &ConfigOptions,
    ) -> Result<Arc<dyn ExecutionPlan>> {
        let mem = analyze_memory(&plan);
        let max_n = if mem.subtree_max_row_bytes > 0 {
            self.gpu_memory_budget / mem.subtree_max_row_bytes
        } else {
            config.execution.batch_size
        };
        let batch_size = max_n.max(1);

        let result = plan.transform_up(|node: Arc<dyn ExecutionPlan>| {
            if node.as_any().is::<ParquetExec>() {
                Ok(Transformed::yes(
                    Arc::new(GpuScanExec::new(node, batch_size)) as Arc<dyn ExecutionPlan>,
                ))
            } else if node.as_any().is::<GpuCoalesceBatchesExec>() {
                let gpu_cb = node.as_any().downcast_ref::<GpuCoalesceBatchesExec>().unwrap();
                let coalesce = gpu_cb.inner().as_any().downcast_ref::<CoalesceBatchesExec>().unwrap();
                let input = coalesce.input().clone();
                let new_inner: Arc<dyn ExecutionPlan> = Arc::new(CoalesceBatchesExec::new(input, batch_size));
                Ok(Transformed::yes(
                    Arc::new(GpuCoalesceBatchesExec::new(new_inner)) as Arc<dyn ExecutionPlan>,
                ))
            } else {
                Ok(Transformed::no(node))
            }
        })?;
        Ok(result.data)
    }

    fn name(&self) -> &str {
        "gpu_memory_budget"
    }

    fn schema_check(&self) -> bool {
        true
    }
}
