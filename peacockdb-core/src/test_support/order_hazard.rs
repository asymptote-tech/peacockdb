//! Whether a query's answer rests on an order a device join does not keep. cuco's hash probe
//! and cuDF's conditional joins write their pairs through an atomic cursor, so a hash or
//! nested-loop join emits its rows in a different order run to run. A selection that keeps rows
//! by position above one — a limit with no sort between, or a sorted cut between tied rows that
//! differ — then keeps different rows per run, and a device cell over it flaps. A cross join is
//! not such a join: cuDF and DataFusion both emit it build-row-major. Read off DataFusion's plan
//! at one partition, since the hazard is the query's and not a mode's, and for a sorted cut off
//! the whole tie it falls in.

use std::sync::Arc;

use datafusion::arrow::array::{ArrayRef, RecordBatch};
use datafusion::arrow::compute::concat_batches;
use datafusion::arrow::row::{RowConverter, Rows, SortField};
use datafusion::common::tree_node::{Transformed, TreeNode};
use datafusion::execution::context::SessionContext;
use datafusion::physical_expr::LexOrdering;
use datafusion::physical_plan::aggregates::AggregateExec;
use datafusion::physical_plan::coalesce_batches::CoalesceBatchesExec;
use datafusion::physical_plan::joins::{
    HashJoinExec, NestedLoopJoinExec, SortMergeJoinExec, SymmetricHashJoinExec,
};
use datafusion::physical_plan::limit::{GlobalLimitExec, LocalLimitExec};
use datafusion::physical_plan::projection::ProjectionExec;
use datafusion::physical_plan::sorts::sort::SortExec;
use datafusion::physical_plan::sorts::sort_preserving_merge::SortPreservingMergeExec;
use datafusion::physical_plan::{ExecutionPlan, collect};

use super::corpus::{query_text, session_for};

#[cfg(test)]
mod tests;

/// The corpus case: `query` at one partition, refused naming the hazard if it has one. A
/// `data_fusion_subset` line's device cells are held to a committed section besides, by
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

/// A node whose output row `i` is its input's row `i`, so an offset above it cuts below it too.
fn keeps_positions(plan: &dyn ExecutionPlan) -> bool {
    let any = plan.as_any();
    any.is::<ProjectionExec>() || any.is::<CoalesceBatchesExec>()
}

/// Walks the plan: the first positional selection that reaches a device join with nothing
/// between them fixing the order is the answer; every sorted cut with a device join below is
/// collected with the positions it cuts at — its fetch, and the offset of a limit above it with
/// only nodes that keep positions between.
fn unsorted_selection(
    plan: &Arc<dyn ExecutionPlan>,
    skip_above: usize,
    cuts: &mut Vec<(Arc<dyn ExecutionPlan>, Vec<usize>)>,
) -> Option<String> {
    let any = plan.as_any();
    let skip = match any.downcast_ref::<GlobalLimitExec>() {
        Some(limit) => limit.skip(),
        None if keeps_positions(plan.as_ref()) => skip_above,
        None => 0,
    };
    if ordering_of(plan.as_ref()).is_some() {
        let positions: Vec<usize> = [Some(skip_above), plan.fetch()]
            .into_iter()
            .flatten()
            .filter(|&at| at > 0)
            .collect();
        if !positions.is_empty() && joins_below(plan) {
            cuts.push((plan.clone(), positions));
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

/// The sort re-run on a prefix that holds each cut's whole tie — widened until a row past the
/// tie, or the end of the input, is in it — and every sort below it cutting at the same row
/// widened to match. A tie that spans a cut and holds two rows that differ is a hazard: which of
/// them the cut keeps is the join's emission order.
async fn tie_at_a_cut(
    ctx: &SessionContext,
    sort: &Arc<dyn ExecutionPlan>,
    positions: &[usize],
) -> Option<String> {
    let mut reach = positions.iter().max().expect("a cut has a position") + 1;
    loop {
        let rows = sorted_prefix(ctx, sort, reach).await;
        let keys = ordering_of(sort.as_ref())
            .expect("a cut is a sort")
            .iter()
            .map(|key| {
                key.expr
                    .evaluate(&rows)
                    .and_then(|value| value.into_array(rows.num_rows()))
                    .expect("a sort key evaluates over its own input")
            })
            .collect::<Vec<_>>();
        let (keys, whole) = (comparable(&keys), comparable(rows.columns()));
        let n = rows.num_rows();
        let mut open = false;
        for &at in positions.iter().filter(|&&at| at < n) {
            let tied = |i: usize| keys.row(i) == keys.row(at);
            if !tied(at - 1) {
                continue;
            }
            let Some(end) = (at..n).find(|&i| !tied(i)).or((n < reach).then_some(n)) else {
                open = true;
                continue;
            };
            let start = (0..at).rev().take_while(|&i| tied(i)).last().unwrap_or(at);
            if (start..end).any(|i| whole.row(i) != whole.row(start)) {
                return Some(format!(
                    "`{}` cuts at row {at} inside a tie on its keys that holds rows that differ",
                    sort.name()
                ));
            }
        }
        if !open {
            return None;
        }
        reach *= 2;
    }
}

/// The first `reach` rows of `sort`'s output.
async fn sorted_prefix(
    ctx: &SessionContext,
    sort: &Arc<dyn ExecutionPlan>,
    reach: usize,
) -> RecordBatch {
    let widened = match sort.fetch() {
        None => sort
            .clone()
            .with_fetch(Some(reach))
            .expect("a sort takes a fetch"),
        Some(fetch) => {
            sort.clone()
                .transform_down(|node| {
                    Ok(match node.fetch() {
                        Some(n) if n == fetch && ordering_of(node.as_ref()).is_some() => {
                            Transformed::yes(
                                node.with_fetch(Some(reach)).expect("a sort takes a fetch"),
                            )
                        }
                        _ => Transformed::no(node),
                    })
                })
                .expect("the rewrite does not fail")
                .data
        }
    };
    let batches = collect(widened, ctx.task_ctx())
        .await
        .unwrap_or_else(|why| panic!("the widened sort does not run: {why}"));
    concat_batches(&sort.schema(), &batches).expect("one schema")
}

/// `columns` as rows that compare equal exactly when every value does.
fn comparable(columns: &[ArrayRef]) -> Rows {
    RowConverter::new(
        columns
            .iter()
            .map(|column| SortField::new(column.data_type().clone()))
            .collect(),
    )
    .and_then(|converter| converter.convert_columns(columns))
    .expect("every arrow type the corpus answers converts to rows")
}
