//! DataFusion physical plan → the engine's node tree.
//!
//! A conscious decision per DataFusion node kind: nothing is carried over implicitly and
//! an unrecognized node is a plan-time error naming it. What is reused is DataFusion's
//! planning — the coercions, the decimal scales, the per-aggregate state schemas — and
//! not its execution semantics, which is what annotating a tree with wrappers carries
//! along by accident.

use common::limit_interval;
use nodes::node;
use std::sync::Arc;

#[cfg(test)]
use datafusion::arrow::datatypes::Schema as ArrowSchema;
#[cfg(test)]
use datafusion::physical_expr::PhysicalExpr;
use datafusion::physical_plan::ExecutionPlan;

use crate::plan::Batching;
use crate::plan::ColumnOrder;
#[cfg(test)]
use crate::plan::Expr;
use crate::plan::GpuNode;
use crate::plan::GpuUnload;
use crate::plan::PlanError;
mod aggregate;
mod common;
mod expr;
mod nodes;
mod scan_mapping;

#[cfg(test)]
mod schema_tests;
#[cfg(test)]
mod tests;

pub(crate) struct Translator {
    /// Lanes a source is partitioned into, before the small-table rule.
    pub(crate) target_partitions: usize,
    pub(crate) batching: Batching,
    /// Batch sizes the estimator derived, one per source in the order translation reaches
    /// them. Empty on the first pass, when there is nothing derived yet.
    source_targets: Vec<u64>,
    next_source: std::cell::Cell<usize>,
    /// A source reading fewer bytes than this plans one lane whatever the target, and the
    /// nodes a one-lane region does not need are then not emitted at all. Bytes rather
    /// than rows because a narrow table of many rows reads less than a wide table of few;
    /// measured on the columns this scan projects, of the row groups pruning left it, so
    /// it is a property of the scan and not of the table.
    pub(crate) small_table_bytes: u64,
}

impl Translator {
    pub(crate) fn new(target_partitions: usize, batching: Batching) -> Self {
        Self {
            target_partitions,
            batching,
            source_targets: Vec::new(),
            next_source: std::cell::Cell::new(0),
            small_table_bytes: 0,
        }
    }

    /// How many sources this translator reached, which is what pairs a pass with the one
    /// that sized it: the two address a source by nothing but that order.
    pub(crate) fn sources_reached(&self) -> usize {
        self.next_source.get()
    }

    /// The second pass: each source gets the batch size the estimator solved for it,
    /// rather than the one number the first pass had to assume.
    pub(crate) fn with_source_targets(mut self, targets: Vec<u64>) -> Self {
        self.source_targets = targets;
        self
    }

    pub(crate) fn with_small_table_bytes(mut self, bytes: u64) -> Self {
        self.small_table_bytes = bytes;
        self
    }

    /// The root, with the limit lowering rule applied: a root-adjacent limit is not a
    /// node at all — its interval becomes the unload's, because a limit over a stream
    /// about to leave the device is a statement about which rows are worth moving.
    pub(crate) fn translate(
        &self,
        root: &Arc<dyn ExecutionPlan>,
    ) -> Result<Box<dyn GpuNode>, PlanError> {
        match limit_interval(root) {
            Some((input, interval)) => {
                let input = node(self, &input)?;
                Ok(Box::new(GpuUnload::new(input, Some(interval))))
            }
            None => Ok(Box::new(GpuUnload::new(node(self, root)?, None))),
        }
    }
}

/// A sort's per-batch half, before the parent decides which accumulator goes above it.
struct PerBatchSort {
    node: Box<dyn GpuNode>,
    keys: Vec<ColumnOrder>,
    fetch: Option<usize>,
}

/// One DataFusion physical expression in this engine's own vocabulary.
///
/// `#[cfg(test)]` and here rather than in `expr`, because `expr` is this subcomponent's own
/// and only this file may be named from outside it. The caller is `planner/mod.rs`, which
/// relays it to a test in another component.
#[cfg(test)]
pub(crate) fn translate_expr(
    expr: &Arc<dyn PhysicalExpr>,
    input_schema: &ArrowSchema,
) -> Result<Expr, PlanError> {
    expr::translate_expr(expr, input_schema)
}
