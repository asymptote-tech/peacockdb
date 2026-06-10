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
use datafusion::physical_plan::joins::utils::JoinFilter;
use datafusion::physical_plan::joins::{CrossJoinExec, HashJoinExec, NestedLoopJoinExec};
use datafusion::physical_plan::projection::ProjectionExec;
use datafusion::physical_plan::limit::GlobalLimitExec;
use datafusion::physical_plan::repartition::RepartitionExec;
use datafusion::physical_plan::sorts::sort::SortExec;
use datafusion::physical_plan::sorts::sort_preserving_merge::SortPreservingMergeExec;
use datafusion::physical_plan::union::{InterleaveExec, UnionExec};
use datafusion::physical_plan::windows::{BoundedWindowAggExec, WindowAggExec};
use datafusion::physical_plan::PhysicalExpr;
use datafusion::physical_expr::expressions::{BinaryExpr, InListExpr, NotExpr};
use datafusion::logical_expr::Operator;
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
impl GpuExtraDisplay for GpuFilterExec {
    fn extra_display_info(&self) -> String {
        let fe = self.inner.as_any().downcast_ref::<FilterExec>().unwrap();
        let mut s = format!("predicate={}", fe.predicate());
        if let Some(proj) = fe.projection() {
            let cols: Vec<String> = proj.iter().map(|i| i.to_string()).collect();
            s.push_str(&format!(", projection=[{}]", cols.join(", ")));
        }
        s
    }
}

gpu_exec_node!(GpuProjectExec);
impl GpuExtraDisplay for GpuProjectExec {
    fn extra_display_info(&self) -> String {
        let pe = self.inner.as_any().downcast_ref::<ProjectionExec>().unwrap();
        let exprs: Vec<String> = pe
            .expr()
            .iter()
            .map(|(e, alias)| format!("{e} as {alias}"))
            .collect();
        format!("expr=[{}]", exprs.join(", "))
    }
}

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
impl GpuExtraDisplay for GpuHashJoinExec {
    fn extra_display_info(&self) -> String {
        let hj = self.inner.as_any().downcast_ref::<HashJoinExec>().unwrap();
        let on: Vec<String> = hj
            .on()
            .iter()
            .map(|(l, r)| format!("({l}, {r})"))
            .collect();
        let mut s = format!("join_type={:?}, on=[{}]", hj.join_type(), on.join(", "));
        if let Some(jf) = hj.filter() {
            s.push_str(&format!(", filter={}", jf.expression()));
        }
        if let Some(proj) = hj.projection.as_ref() {
            let cols: Vec<String> = proj.iter().map(|i| i.to_string()).collect();
            s.push_str(&format!(", projection=[{}]", cols.join(", ")));
        }
        s
    }
}

gpu_exec_node!(GpuCrossJoinExec);
impl GpuExtraDisplay for GpuCrossJoinExec {}

gpu_exec_node!(GpuNestedLoopJoinExec);
impl GpuExtraDisplay for GpuNestedLoopJoinExec {
    fn extra_display_info(&self) -> String {
        let nlj = self
            .inner
            .as_any()
            .downcast_ref::<NestedLoopJoinExec>()
            .unwrap();
        let mut s = format!("join_type={:?}", nlj.join_type());
        if let Some(jf) = nlj.filter() {
            s.push_str(&format!(", filter={}", jf.expression()));
        }
        if let Some(proj) = nlj.projection() {
            let cols: Vec<String> = proj.iter().map(|i| i.to_string()).collect();
            s.push_str(&format!(", projection=[{}]", cols.join(", ")));
        }
        s
    }
}

gpu_exec_node!(GpuSortExec);
impl GpuExtraDisplay for GpuSortExec {
    fn extra_display_info(&self) -> String {
        let se = self.inner.as_any().downcast_ref::<SortExec>().unwrap();
        let mut s = format!("expr=[{}]", se.expr());
        if let Some(f) = se.fetch() {
            s.push_str(&format!(", fetch={f}"));
        }
        s
    }
}

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

gpu_exec_node!(GpuUnionExec);
impl GpuExtraDisplay for GpuUnionExec {}

gpu_exec_node!(GpuInterleaveExec);
impl GpuExtraDisplay for GpuInterleaveExec {}

gpu_exec_node!(GpuGlobalLimitExec);
impl GpuExtraDisplay for GpuGlobalLimitExec {
    fn extra_display_info(&self) -> String {
        let gl = self.inner.as_any().downcast_ref::<GlobalLimitExec>().unwrap();
        match gl.fetch() {
            Some(f) => format!("skip={}, fetch={}", gl.skip(), f),
            None => format!("skip={}, fetch=None", gl.skip()),
        }
    }
}

gpu_exec_node!(GpuWindowExec);
impl GpuExtraDisplay for GpuWindowExec {
    fn extra_display_info(&self) -> String {
        // Window exprs live on either WindowAggExec or BoundedWindowAggExec.
        let names: Vec<String> = if let Some(w) =
            self.inner.as_any().downcast_ref::<WindowAggExec>()
        {
            w.window_expr().iter().map(|e| e.name().to_string()).collect()
        } else if let Some(w) = self.inner.as_any().downcast_ref::<BoundedWindowAggExec>() {
            w.window_expr().iter().map(|e| e.name().to_string()).collect()
        } else {
            vec![]
        };
        format!("wdw=[{}]", names.join(", "))
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

/// Expand `x IN (a, b, c)` into `((x = a) OR (x = b)) OR (x = c)` — or its
/// `NOT(...)` form for `NOT IN`. cuDF's AST has no IN opcode, so this lowering
/// must happen before execution; doing it here (in the plan) rather than inside
/// the serializer keeps serialization a verbatim encoding of the plan.
fn expand_in_list(in_list: &InListExpr) -> Result<Arc<dyn PhysicalExpr>> {
    let list = in_list.list();
    if list.is_empty() {
        return Err(datafusion::error::DataFusionError::NotImplemented(
            "IN with empty list".into(),
        ));
    }
    let target = in_list.expr();
    let eq = |item: &Arc<dyn PhysicalExpr>| -> Arc<dyn PhysicalExpr> {
        Arc::new(BinaryExpr::new(target.clone(), Operator::Eq, item.clone()))
    };
    let mut acc = eq(&list[0]);
    for item in &list[1..] {
        acc = Arc::new(BinaryExpr::new(acc, Operator::Or, eq(item)));
    }
    if in_list.negated() {
        acc = Arc::new(NotExpr::new(acc));
    }
    Ok(acc)
}

/// Recursively replace every `InListExpr` in `expr` with its OR-chain form.
fn lower_in_lists(expr: Arc<dyn PhysicalExpr>) -> Result<Transformed<Arc<dyn PhysicalExpr>>> {
    expr.transform_up(|e| {
        if let Some(in_list) = e.as_any().downcast_ref::<InListExpr>() {
            Ok(Transformed::yes(expand_in_list(in_list)?))
        } else {
            Ok(Transformed::no(e))
        }
    })
}

impl PhysicalOptimizerRule for GpuExecutionRule {
    fn optimize(
        &self,
        plan: Arc<dyn ExecutionPlan>,
        _config: &ConfigOptions,
    ) -> Result<Arc<dyn ExecutionPlan>> {
        let result = plan.transform_up(|node: Arc<dyn ExecutionPlan>| {
            let new_node: Arc<dyn ExecutionPlan> = if node.as_any().is::<FilterExec>() {
                // Lower any IN-lists in the predicate before wrapping.
                let rebuilt: Option<Arc<dyn ExecutionPlan>> = {
                    let fe = node.as_any().downcast_ref::<FilterExec>().unwrap();
                    let lowered = lower_in_lists(fe.predicate().clone())?;
                    if lowered.transformed {
                        let mut f = FilterExec::try_new(lowered.data, fe.input().clone())?;
                        if let Some(proj) = fe.projection() {
                            f = f.with_projection(Some(proj.clone()))?;
                        }
                        Some(Arc::new(f) as Arc<dyn ExecutionPlan>)
                    } else {
                        None
                    }
                };
                Arc::new(GpuFilterExec::new(rebuilt.unwrap_or(node)))
            } else if node.as_any().is::<ProjectionExec>() {
                let rebuilt: Option<Arc<dyn ExecutionPlan>> = {
                    let pe = node.as_any().downcast_ref::<ProjectionExec>().unwrap();
                    let mut changed = false;
                    let mut new_exprs: Vec<(Arc<dyn PhysicalExpr>, String)> =
                        Vec::with_capacity(pe.expr().len());
                    for (e, alias) in pe.expr() {
                        let lowered = lower_in_lists(e.clone())?;
                        changed |= lowered.transformed;
                        new_exprs.push((lowered.data, alias.clone()));
                    }
                    if changed {
                        Some(Arc::new(ProjectionExec::try_new(new_exprs, pe.input().clone())?)
                            as Arc<dyn ExecutionPlan>)
                    } else {
                        None
                    }
                };
                Arc::new(GpuProjectExec::new(rebuilt.unwrap_or(node)))
            } else if node.as_any().is::<AggregateExec>() {
                Arc::new(GpuAggregateExec::new(node))
            } else if node.as_any().is::<HashJoinExec>() {
                // Lower any IN-lists in the residual join filter before wrapping.
                let rebuilt: Option<Arc<dyn ExecutionPlan>> = {
                    let hj = node.as_any().downcast_ref::<HashJoinExec>().unwrap();
                    match hj.filter() {
                        Some(jf) => {
                            let lowered = lower_in_lists(jf.expression().clone())?;
                            if lowered.transformed {
                                let new_filter = JoinFilter::new(
                                    lowered.data,
                                    jf.column_indices().to_vec(),
                                    jf.schema().clone(),
                                );
                                let h = HashJoinExec::try_new(
                                    hj.left().clone(),
                                    hj.right().clone(),
                                    hj.on().to_vec(),
                                    Some(new_filter),
                                    hj.join_type(),
                                    hj.projection.clone(),
                                    *hj.partition_mode(),
                                    hj.null_equals_null(),
                                )?;
                                Some(Arc::new(h) as Arc<dyn ExecutionPlan>)
                            } else {
                                None
                            }
                        }
                        None => None,
                    }
                };
                Arc::new(GpuHashJoinExec::new(rebuilt.unwrap_or(node)))
            } else if node.as_any().is::<CrossJoinExec>() {
                Arc::new(GpuCrossJoinExec::new(node))
            } else if node.as_any().is::<NestedLoopJoinExec>() {
                Arc::new(GpuNestedLoopJoinExec::new(node))
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
            } else if node.as_any().is::<UnionExec>() {
                Arc::new(GpuUnionExec::new(node))
            } else if node.as_any().is::<InterleaveExec>() {
                Arc::new(GpuInterleaveExec::new(node))
            } else if node.as_any().is::<GlobalLimitExec>() {
                Arc::new(GpuGlobalLimitExec::new(node))
            } else if node.as_any().is::<WindowAggExec>()
                || node.as_any().is::<BoundedWindowAggExec>()
            {
                Arc::new(GpuWindowExec::new(node))
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
pub struct SubtreeMemory {
    /// Peak GPU memory as bytes per scan-batch-row N:
    /// `peak_bytes = subtree_max_row_bytes * N`.
    pub subtree_max_row_bytes: usize,
    /// Output row width in bytes (per output row).
    pub output_width: usize,
    /// Ratio of output rows to original batch size N.
    /// 1.0 means row count is preserved; <1.0 after filters; >1.0 after fan-out joins.
    pub output_row_ratio: f64,
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

pub(crate) fn analyze_memory_with(
    plan: &Arc<dyn ExecutionPlan>,
    selectivity: &dyn SelectivityEstimator,
    cardinality: &dyn CardinalityEstimator,
) -> SubtreeMemory {
    let output_width = row_width(&plan.schema());
    let children = plan.children();

    // Leaf node (ParquetExec, etc.): just the output batch.
    if children.is_empty() {
        return SubtreeMemory {
            subtree_max_row_bytes: output_width,
            output_width,
            output_row_ratio: 1.0,
        };
    }

    match plan.name() {
        // Filter: input batch (child rows) + output batch (filtered rows).
        "GpuFilterExec" => {
            let child = analyze_memory_with(children[0], selectivity, cardinality);
            let sel = selectivity.estimate(plan);
            let output_rows_bytes = (sel * output_width as f64) as usize;
            let input_rows_bytes = (child.output_row_ratio * child.output_width as f64) as usize;
            SubtreeMemory {
                subtree_max_row_bytes: child
                    .subtree_max_row_bytes
                    .max(input_rows_bytes + output_rows_bytes),
                output_width,
                output_row_ratio: child.output_row_ratio * sel,
            }
        }
        // Projection / aggregation: input batch + output batch, row count preserved.
        "GpuProjectExec" | "GpuAggregateExec" => {
            let child = analyze_memory_with(children[0], selectivity, cardinality);
            let input_rows_bytes = (child.output_row_ratio * child.output_width as f64) as usize;
            let output_rows_bytes = (child.output_row_ratio * output_width as f64) as usize;
            SubtreeMemory {
                subtree_max_row_bytes: child
                    .subtree_max_row_bytes
                    .max(input_rows_bytes + output_rows_bytes),
                output_width,
                output_row_ratio: child.output_row_ratio,
            }
        }
        // Hash join: build side + probe batch + output batch.
        "GpuHashJoinExec" => {
            let build = analyze_memory_with(children[0], selectivity, cardinality);
            let probe = analyze_memory_with(children[1], selectivity, cardinality);
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
            }
        }
        // Sort: input + workspace (index array, ~2× input).
        "GpuSortExec" => {
            let child = analyze_memory_with(children[0], selectivity, cardinality);
            let input_bytes = (child.output_row_ratio * child.output_width as f64) as usize;
            SubtreeMemory {
                subtree_max_row_bytes: child.subtree_max_row_bytes.max(2 * input_bytes),
                output_width,
                output_row_ratio: child.output_row_ratio,
            }
        }
        // UNION ALL (concatenate the rows of all inputs). Must mirror what
        // execute_union actually does today, branching on input count:
        //   - single input  → std::move (true pass-through): peak = child peak.
        //   - multiple inputs → all input tables are held live, then the
        //     cudf::concatenate output is allocated → peak ≈ Σ(inputs) + output,
        //     and the output row count is the *sum* of child cardinalities.
        // Undercounting here would feed GpuMemoryBudgetRule too small a
        // subtree_max_row_bytes → too large a batch size → OOM.
        // (Once the multi-partition / true-pass-through model lands in #34, the
        // concat moves up into GpuCoalescePartitionsExec and this can revert to a
        // plain pass-through.)
        "GpuUnionExec" | "GpuInterleaveExec" => {
            let child_results: Vec<_> = children
                .iter()
                .map(|c| analyze_memory_with(c, selectivity, cardinality))
                .collect();
            let max_child_peak = child_results
                .iter()
                .map(|c| c.subtree_max_row_bytes)
                .max()
                .unwrap_or(output_width);
            if child_results.len() <= 1 {
                let output_row_ratio =
                    child_results.first().map(|c| c.output_row_ratio).unwrap_or(1.0);
                SubtreeMemory {
                    subtree_max_row_bytes: max_child_peak,
                    output_width,
                    output_row_ratio,
                }
            } else {
                let inputs_bytes: usize = child_results
                    .iter()
                    .map(|c| (c.output_row_ratio * c.output_width as f64) as usize)
                    .sum();
                let output_row_ratio: f64 =
                    child_results.iter().map(|c| c.output_row_ratio).sum();
                let output_bytes = (output_row_ratio * output_width as f64) as usize;
                let own = inputs_bytes + output_bytes;
                SubtreeMemory {
                    subtree_max_row_bytes: max_child_peak.max(own),
                    output_width,
                    output_row_ratio,
                }
            }
        }
        // Everything else (CoalescePartitions, Repartition, CoalesceBatches, etc.):
        // pass-through — peak is the max of children, ratio is max of children.
        _ => {
            let child_results: Vec<_> = children
                .iter()
                .map(|c| analyze_memory_with(c, selectivity, cardinality))
                .collect();
            let max_peak = child_results
                .iter()
                .map(|c| c.subtree_max_row_bytes)
                .max()
                .unwrap_or(output_width);
            let max_ratio = child_results
                .iter()
                .map(|c| c.output_row_ratio)
                .fold(1.0_f64, f64::max);
            SubtreeMemory {
                subtree_max_row_bytes: max_peak,
                output_width,
                output_row_ratio: max_ratio,
            }
        }
    }
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
