//! Whether a query's answer rests on an order a device join does not keep. cuco's hash probe
//! and cuDF's conditional joins write their pairs through an atomic cursor, so a hash or
//! nested-loop join emits its rows in a different order run to run. A selection that keeps rows
//! by position above one — a limit with no sort between, or a sorted cut between tied rows that
//! differ — then keeps different rows per run, and a device cell over it flaps. A cross join is
//! not such a join: cuDF and DataFusion both emit it build-row-major. Read off DataFusion's plan
//! at one partition, since the hazard is the query's and not a mode's, and for a sorted cut off
//! the rows either side of it.

use std::sync::Arc;

use datafusion::arrow::array::{Array, RecordBatch};
use datafusion::arrow::compute::concat_batches;
use datafusion::common::ScalarValue;
use datafusion::common::tree_node::{Transformed, TreeNode};
use datafusion::execution::context::SessionContext;
use datafusion::physical_expr::LexOrdering;
use datafusion::physical_plan::aggregates::AggregateExec;
use datafusion::physical_plan::joins::{
    HashJoinExec, NestedLoopJoinExec, SortMergeJoinExec, SymmetricHashJoinExec,
};
use datafusion::physical_plan::limit::{GlobalLimitExec, LocalLimitExec};
use datafusion::physical_plan::sorts::sort::SortExec;
use datafusion::physical_plan::sorts::sort_preserving_merge::SortPreservingMergeExec;
use datafusion::physical_plan::{ExecutionPlan, collect};

use super::corpus::{query_text, session_for};

#[cfg(test)]
mod tests;

/// The corpus case: `query` at one partition, refused naming the hazard if it has one. A
/// `data_fusion_subset` line's device cells are refused before this, by
/// `an_undetermined_lines_rows_are_compared_only_against_committed_files`.
pub(crate) async fn device_order_case(dataset: &str, sf: &str, query: &str) {
    let ctx = session_for(dataset, sf, 1).await;
    if let Some(why) = hazard_in(&ctx, &query_text(dataset, query)).await {
        panic!(
            "{dataset}/{query} has a device cell, and its answer depends on an order a device \
             join does not keep, so the cell would flap: {why}"
        );
    }
}

/// What makes `sql`'s answer depend on a device join's order, or `None`.
pub(crate) async fn hazard_in(ctx: &SessionContext, sql: &str) -> Option<String> {
    let plan = ctx
        .sql(sql)
        .await
        .unwrap_or_else(|why| panic!("the query does not plan: {why}"))
        .create_physical_plan()
        .await
        .unwrap_or_else(|why| panic!("the query has no physical plan: {why}"));
    let mut cuts = Vec::new();
    if let Some(why) = unsorted_selection(&plan, 0, &mut cuts) {
        return Some(why);
    }
    for (sort, positions) in cuts {
        if let Some(why) = tie_at_a_cut(ctx, &sort, &positions).await {
            return Some(why);
        }
    }
    None
}

/// A device join: one whose emission order is not reproducible.
fn is_device_join(plan: &dyn ExecutionPlan) -> bool {
    let any = plan.as_any();
    any.is::<HashJoinExec>()
        || any.is::<NestedLoopJoinExec>()
        || any.is::<SortMergeJoinExec>()
        || any.is::<SymmetricHashJoinExec>()
}

fn ordering_of(plan: &dyn ExecutionPlan) -> Option<&LexOrdering> {
    let any = plan.as_any();
    if let Some(sort) = any.downcast_ref::<SortExec>() {
        return Some(sort.expr());
    }
    any.downcast_ref::<SortPreservingMergeExec>()
        .map(|merge| merge.expr())
}

fn joins_below(plan: &Arc<dyn ExecutionPlan>) -> bool {
    is_device_join(plan.as_ref()) || plan.children().into_iter().any(joins_below)
}

/// Whether `plan`'s output is the same whatever order its input arrives in: a sort, or a
/// keyless aggregate's one row.
fn fixes_its_order(plan: &dyn ExecutionPlan) -> bool {
    ordering_of(plan).is_some()
        || plan
            .as_any()
            .downcast_ref::<AggregateExec>()
            .is_some_and(|aggregate| aggregate.group_expr().expr().is_empty())
}

/// A device join reached from here without passing a node that fixes the order above it.
fn reaches_a_join_unsorted(plan: &Arc<dyn ExecutionPlan>) -> bool {
    if fixes_its_order(plan.as_ref()) {
        return false;
    }
    is_device_join(plan.as_ref()) || plan.children().into_iter().any(reaches_a_join_unsorted)
}

/// Walks the plan: the first positional selection that reaches a device join with nothing
/// between them fixing the order is the answer; every sorted cut with a device join below is
/// collected with the positions it cuts at — its fetch, and the offset of a limit just above.
fn unsorted_selection(
    plan: &Arc<dyn ExecutionPlan>,
    skip_above: usize,
    cuts: &mut Vec<(Arc<dyn ExecutionPlan>, Vec<usize>)>,
) -> Option<String> {
    let any = plan.as_any();
    let skip = any
        .downcast_ref::<GlobalLimitExec>()
        .map_or(0, |limit| limit.skip());
    if ordering_of(plan.as_ref()).is_some() {
        if let Some(fetch) = plan.fetch() {
            if joins_below(plan) {
                cuts.push((plan.clone(), vec![skip_above, fetch]));
            }
        }
    } else {
        let positional =
            any.is::<GlobalLimitExec>() || any.is::<LocalLimitExec>() || plan.fetch().is_some();
        if positional
            && (is_device_join(plan.as_ref())
                || plan.children().into_iter().any(reaches_a_join_unsorted))
        {
            return Some(format!(
                "`{}` keeps rows by position with no sort, and no keyless aggregate, between it \
                 and a hash or nested-loop join",
                plan.name()
            ));
        }
    }
    plan.children()
        .into_iter()
        .find_map(|child| unsorted_selection(child, skip, cuts))
}

/// The sort re-run with one row more, every sort below it cutting at the same row widened to
/// match: a tie at a cut is two rows equal on every key that differ anywhere else.
async fn tie_at_a_cut(
    ctx: &SessionContext,
    sort: &Arc<dyn ExecutionPlan>,
    positions: &[usize],
) -> Option<String> {
    let fetch = sort.fetch().expect("a cut is a sort with a fetch");
    let widened = sort
        .clone()
        .transform_down(|node| {
            Ok(match node.fetch() {
                Some(n) if n == fetch && ordering_of(node.as_ref()).is_some() => {
                    Transformed::yes(node.with_fetch(Some(n + 1)).expect("a sort takes a fetch"))
                }
                _ => Transformed::no(node),
            })
        })
        .expect("the rewrite does not fail")
        .data;
    let batches = collect(widened, ctx.task_ctx())
        .await
        .unwrap_or_else(|why| panic!("the widened sort does not run: {why}"));
    let rows = concat_batches(&sort.schema(), &batches).expect("one schema");
    let keys = ordering_of(sort.as_ref()).expect("a cut is a sort");
    positions
        .iter()
        .filter(|&&at| at > 0 && at < rows.num_rows())
        .find_map(|&at| {
            let tied = keys.iter().all(|key| {
                let column = key
                    .expr
                    .evaluate(&rows)
                    .and_then(|value| value.into_array(rows.num_rows()))
                    .expect("a sort key evaluates over its own input");
                value_at(&column, at - 1) == value_at(&column, at)
            });
            (tied && differ(&rows, at - 1, at)).then(|| {
                format!(
                    "`{}` cuts at row {at} between two rows that tie on its keys and differ",
                    sort.name()
                )
            })
        })
}

fn value_at(column: &dyn Array, row: usize) -> ScalarValue {
    ScalarValue::try_from_array(column, row).expect("every arrow type has a scalar")
}

fn differ(rows: &RecordBatch, a: usize, b: usize) -> bool {
    rows.columns()
        .iter()
        .any(|column| value_at(column.as_ref(), a) != value_at(column.as_ref(), b))
}
