//! What the translation arms share: the ordinals a DataFusion node names, and the two
//! questions asked of a node this engine has already built.

use super::expr::translate_expr;
use std::sync::Arc;

use datafusion::arrow::datatypes::Schema as ArrowSchema;
use datafusion::physical_expr::{LexOrdering, PhysicalExpr};
use datafusion::physical_plan::ExecutionPlan;
use datafusion::physical_plan::coalesce_batches::CoalesceBatchesExec;
use datafusion::physical_plan::joins::utils::JoinFilter;
use datafusion::physical_plan::limit::{GlobalLimitExec, LocalLimitExec};

use crate::plan::Expr;
use crate::plan::PlanError;
use crate::plan::{BatchLayout, ColumnOrder, KeyDistribution};
use crate::plan::{GpuNode, RowInterval};
use crate::plan::{JoinFilterColumn, JoinSide};

/// DataFusion's projection as this layer numbers ordinals. A join whose projection is
/// dropped declares the projected columns and emits every one it crossed, which shifts
/// every reference above it.
pub(crate) fn projected(projection: Option<&Vec<usize>>) -> Option<Vec<u32>> {
    projection.map(|columns| columns.iter().map(|c| *c as u32).collect())
}

/// Lane p of one side holds what can match lane p of the other only if both were scattered
/// on their own join keys, in key order — equal lane counts are not the same fact, and two
/// four-lane scans agree on nothing.
pub(crate) fn co_partitioned(
    build: &dyn GpuNode,
    probe: &dyn GpuNode,
    keys: &[(u32, u32)],
) -> bool {
    let scattered_on = |node: &dyn GpuNode, on: Vec<u32>| {
        let layout = node.kind().layout().expect("a sink cannot be an input");
        layout.n == 1 || layout.key_distribution == KeyDistribution::ByHash { hash_keys: on }
    };
    lanes(build) == lanes(probe)
        && scattered_on(build, keys.iter().map(|(b, _)| *b).collect())
        && scattered_on(probe, keys.iter().map(|(_, p)| *p).collect())
}

/// Hash keys are ordinals into the node's input, so anything else is a shape this mode
/// does not plan rather than something to evaluate on the way to the hash.
pub(crate) fn hash_key_ordinals(
    exprs: &[Arc<dyn PhysicalExpr>],
    input_schema: &ArrowSchema,
) -> Result<Vec<u32>, PlanError> {
    let mut keys = Vec::with_capacity(exprs.len());
    for expr in exprs {
        keys.push(column_ordinal_of(expr, input_schema, "hash key")?);
    }
    Ok(keys)
}

pub(crate) fn sort_key_ordinals(
    exprs: &LexOrdering,
    input_schema: &ArrowSchema,
) -> Result<Vec<ColumnOrder>, PlanError> {
    let mut keys = Vec::with_capacity(exprs.len());
    for key in exprs.iter() {
        keys.push(ColumnOrder {
            column: column_ordinal_of(&key.expr, input_schema, "sort key")?,
            ascending: !key.options.descending,
            nulls_first: key.options.nulls_first,
        });
    }
    Ok(keys)
}

pub(crate) fn column_ordinal_of(
    expr: &Arc<dyn PhysicalExpr>,
    input_schema: &ArrowSchema,
    site: &str,
) -> Result<u32, PlanError> {
    match translate_expr(expr, input_schema)? {
        Expr::Column(reference) => Ok(reference.index),
        _ => Err(PlanError::Unsupported(format!(
            "{site} {expr} is an expression rather than a column"
        ))),
    }
}

/// Which side each column of a join filter's own table came from. Exhaustive: DataFusion
/// has a third side for a mark join's synthesized column, and a filter reading it would
/// otherwise be silently rebased onto the probe.
pub(crate) fn filter_column_map(filter: &JoinFilter) -> Result<Vec<JoinFilterColumn>, PlanError> {
    let mut columns = Vec::with_capacity(filter.column_indices().len());
    for column in filter.column_indices() {
        let side = match column.side {
            datafusion::common::JoinSide::Left => JoinSide::Build,
            datafusion::common::JoinSide::Right => JoinSide::Probe,
            datafusion::common::JoinSide::None => {
                return Err(PlanError::Unsupported(
                    "a join filter reading the mark column: it belongs to neither side, so \
                     this mode has no ordinal to rebase it onto"
                        .to_string(),
                ));
            }
        };
        columns.push(JoinFilterColumn {
            side,
            index: column.index as u32,
        });
    }
    Ok(columns)
}

/// A limit's interval, whichever node carries it. `LocalLimitExec` has no skip.
pub(crate) fn limit_interval(
    plan: &Arc<dyn ExecutionPlan>,
) -> Option<(Arc<dyn ExecutionPlan>, RowInterval)> {
    let any = plan.as_any();
    if let Some(global) = any.downcast_ref::<GlobalLimitExec>() {
        return Some((
            global.input().clone(),
            RowInterval {
                skip: global.skip() as u64,
                fetch: global.fetch().map(|n| n as u64),
            },
        ));
    }
    if let Some(local) = any.downcast_ref::<LocalLimitExec>() {
        return Some((
            local.input().clone(),
            RowInterval {
                skip: 0,
                fetch: Some(local.fetch() as u64),
            },
        ));
    }
    // Not reachable from today's planner: where a limit is root-adjacent DataFusion leaves
    // a GlobalLimitExec there and uses a coalesce's fetch only as the bound pushed below
    // it. The arm is here so both paths agree if that ever changes — one query must not
    // have two plan shapes depending on where the fetch was parked.
    if let Some(coalesce) = any.downcast_ref::<CoalesceBatchesExec>() {
        return coalesce.fetch().map(|fetch| {
            (
                coalesce.input().clone(),
                RowInterval {
                    skip: 0,
                    fetch: Some(fetch as u64),
                },
            )
        });
    }
    None
}

pub(crate) fn lanes(node: &dyn GpuNode) -> usize {
    node.kind().layout().expect("a sink cannot be an input").n
}

pub(crate) fn batches(node: &dyn GpuNode) -> BatchLayout {
    node.kind()
        .layout()
        .expect("a sink cannot be an input")
        .batch_layout
}
