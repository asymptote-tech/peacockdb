//! Which columns can be NULL, on either tree, and the join shape refused because of it.
//!
//! Two analyses of one question: `can_be_null` reads a planned GpuNode tree, and the
//! rewrites that run before one exists read a DataFusion `LogicalPlan` with
//! `logical_can_be_null`. Both read the data, not the declared types — every column in both
//! benchmarks is declared nullable, keys included. Three join types hardcode
//! `cudf::null_equality::EQUAL` whatever the plan's flag says (`cpp/src/operators/join.cpp`),
//! which is set semantics rather than SQL's, so a NULL that can meet a NULL there is refused.
//! Where either analysis cannot decide, a column is possibly-NULL: a false positive costs a
//! refusal or a rewrite a reader can see, a false negative is a wrong answer.

use datafusion::common::tree_node::{TreeNode, TreeNodeRecursion};
use datafusion::common::{Column, JoinType, plan_err};
use datafusion::datasource::listing::ListingTable;
use datafusion::datasource::source_as_provider;
use datafusion::logical_expr::expr::InSubquery;
use datafusion::logical_expr::{Expr as LogicalExpr, Join, LogicalPlan, Subquery, TableScan};

use super::parquet_nulls::column_may_hold_null;
use crate::plan::Expr;
use crate::plan::GpuNode;
use crate::plan::PlanError;
use crate::plan::{NodeRef, as_node_ref};

/// Refuses an anti or mark join whose NULLs can meet under SQL semantics. Everything else
/// plans: semi honours the flag, and `null_equals_null=true` is asking for the equality the
/// executor hardcodes.
pub(crate) fn refuse_null_unsafe_joins(root: &dyn GpuNode) -> Result<(), PlanError> {
    for child in root.children() {
        refuse_null_unsafe_joins(child)?;
    }
    let NodeRef::Join(join) = as_node_ref(root) else {
        return Ok(());
    };
    if !hardcodes_null_equality(join.join_type) || join.null_equals_null {
        return Ok(());
    }
    let (build, probe) = (root.children()[0], root.children()[1]);
    let (build_nulls, probe_nulls) = (can_be_null(build), can_be_null(probe));
    let meeting = join.keys.iter().find(|(b, p)| {
        nullable_at(&build_nulls, *b) && nullable_at(&probe_nulls, *p)
    });
    match meeting {
        Some((b, p)) => Err(PlanError::Unsupported(format!(
            "{:?} join on a key that can be NULL on both sides (build @{b}, probe @{p}) with \
             null_equals_null=false: the executor matches NULL to NULL there whatever the \
             flag says, which is set semantics rather than SQL's (#59, #80)",
            join.join_type
        ))),
        None => Ok(()),
    }
}

/// The three where the executor's `null_equality` is not the plan's to choose.
fn hardcodes_null_equality(join_type: JoinType) -> bool {
    matches!(
        join_type,
        JoinType::LeftAnti | JoinType::RightAnti | JoinType::LeftMark
    )
}

fn nullable_at(columns: &[bool], ordinal: u32) -> bool {
    columns.get(ordinal as usize).copied().unwrap_or(true)
}

/// Whether `column` of `plan`'s output can hold a NULL, read off the data where the column
/// traces to a parquet scan. Anything the trace cannot follow is possibly-NULL.
pub(crate) fn logical_can_be_null(plan: &LogicalPlan, column: &Column) -> bool {
    match plan {
        LogicalPlan::TableScan(scan) => {
            scan_column_may_hold_null(scan, &column.name).unwrap_or(true)
        }
        LogicalPlan::Projection(project) => {
            match project
                .schema
                .index_of_column(column)
                .ok()
                .map(|i| &project.expr[i])
            {
                Some(LogicalExpr::Column(inner)) => logical_can_be_null(&project.input, inner),
                Some(LogicalExpr::Alias(alias)) => match alias.expr.as_ref() {
                    LogicalExpr::Column(inner) => logical_can_be_null(&project.input, inner),
                    _ => true,
                },
                _ => true,
            }
        }
        LogicalPlan::Filter(filter) => logical_can_be_null(&filter.input, column),
        LogicalPlan::Sort(sort) => logical_can_be_null(&sort.input, column),
        LogicalPlan::Limit(limit) => logical_can_be_null(&limit.input, column),
        // An alias renames the relation, not the column: the input answers for the bare name.
        LogicalPlan::SubqueryAlias(aliased) => {
            logical_can_be_null(&aliased.input, &Column::from_name(column.name.clone()))
        }
        LogicalPlan::Join(join) => join_can_be_null(join, column),
        _ => true,
    }
}

/// The side that answers for `column`, where the join preserves it. A side the join pads is
/// nullable whatever it holds, and a name on both sides decides nothing.
fn join_can_be_null(join: &Join, column: &Column) -> bool {
    let (left, right) = (join.left.schema(), join.right.schema());
    let on_left = match (left.has_column(column), right.has_column(column)) {
        (true, false) => true,
        (false, true) => false,
        _ => return true,
    };
    let preserved = match join.join_type {
        JoinType::Inner => true,
        JoinType::Left | JoinType::LeftSemi | JoinType::LeftAnti | JoinType::LeftMark => on_left,
        JoinType::Right | JoinType::RightSemi | JoinType::RightAnti => !on_left,
        JoinType::Full => false,
    };
    if !preserved {
        return true;
    }
    let side = if on_left { &join.left } else { &join.right };
    logical_can_be_null(side, column)
}

fn scan_column_may_hold_null(scan: &TableScan, name: &str) -> Result<bool, PlanError> {
    let source = source_as_provider(&scan.source).map_err(|e| PlanError::Invalid(e.to_string()))?;
    let listing = source
        .as_any()
        .downcast_ref::<ListingTable>()
        .ok_or_else(|| PlanError::Unsupported("not a listing table".into()))?;
    let mut any = false;
    for table_url in listing.table_paths() {
        // `prefix()` is an object-store `Path` with no leading '/', not a filesystem path;
        // the URL (`file:///…`) is, through `Url::to_file_path`.
        let path = url::Url::parse(table_url.as_str())
            .ok()
            .and_then(|url| url.to_file_path().ok())
            .ok_or_else(|| {
                PlanError::Unsupported(format!("not a local file: {}", table_url.as_str()))
            })?;
        any |= column_may_hold_null(&path.to_string_lossy(), name)?;
    }
    Ok(any)
}

/// Whether `x [NOT] IN (S)` can meet a NULL: `x` on the outer side, or `S`'s one output
/// column. An operand that is not a bare column cannot be traced, so it counts as nullable.
pub(crate) fn in_subquery_may_meet_null(
    x: &LogicalExpr,
    sub: &Subquery,
    outer: &LogicalPlan,
) -> bool {
    let x_may_be_null = match x {
        LogicalExpr::Column(column) => logical_can_be_null(outer, column),
        _ => true,
    };
    let y = &sub.subquery.schema().columns()[0];
    x_may_be_null || logical_can_be_null(&sub.subquery, y)
}

/// An `IN` or `NOT IN` below a `NOT`, an `IS [NOT] NULL`, a comparison or a function reads
/// three-valued truth, and the joins DataFusion decorrelates it into answer two-valued — a
/// mark is never NULL. Where it can meet a NULL it is refused rather than answered wrong.
pub(crate) fn refuse_nullable_in_off_the_spine(
    expr: &LogicalExpr,
    outer: &LogicalPlan,
) -> datafusion::common::Result<()> {
    let mut refused = None;
    expr.apply(|node| {
        if let LogicalExpr::InSubquery(InSubquery { expr, subquery, .. }) = node {
            if in_subquery_may_meet_null(expr, subquery, outer) {
                refused = Some(node.to_string());
                return Ok(TreeNodeRecursion::Stop);
            }
        }
        Ok(TreeNodeRecursion::Continue)
    })?;
    match refused {
        Some(what) => plan_err!(
            "an IN or NOT IN that can meet a NULL, read outside a WHERE's AND/OR spine, is \
             refused (#250): {what}"
        ),
        None => Ok(()),
    }
}

/// Per output column of this node, whether it can be NULL.
pub(crate) fn can_be_null(node: &dyn GpuNode) -> Vec<bool> {
    let width = node
        .kind()
        .schema()
        .map_or(0, |schema| schema.fields.fields().len());
    let child = |index: usize| -> Vec<bool> {
        node.children()
            .get(index)
            .map(|child| can_be_null(*child))
            .unwrap_or_default()
    };

    match as_node_ref(node) {
        // The leaf: what the surviving row groups actually hold.
        NodeRef::LoadParquet(load) => load.can_be_null.clone(),

        // A projection re-numbers columns and can compute new ones, so each is its own
        // question; a filter's projection is the same question over a subset.
        NodeRef::Project(project) => project
            .exprs
            .iter()
            .map(|named| expr_can_be_null(&named.expr, &child(0)))
            .collect(),
        NodeRef::Filter(filter) => {
            let input = child(0);
            match &filter.projection {
                Some(kept) => kept.iter().map(|c| nullable_at(&input, *c)).collect(),
                None => input,
            }
        }

        // Rows move, are dropped or are re-ordered; a column that could not be NULL below
        // still cannot be above.
        NodeRef::Sort(_)
        | NodeRef::AccumulateBatchesAndSort(_)
        | NodeRef::MergeSortedPartitions(_)
        | NodeRef::CoalesceAllBatches(_)
        | NodeRef::MergePartitions(_)
        | NodeRef::EmitPartitions(_)
        | NodeRef::Limit(_)
        | NodeRef::Unload(_) => child(0),

        // Branches disagree by column, so a column is nullable if it is in any branch.
        NodeRef::Union(_) | NodeRef::Interleave(_) => {
            let mut columns = vec![false; width];
            for index in 0..node.children().len() {
                for (position, nullable) in child(index).iter().enumerate() {
                    if position < columns.len() {
                        columns[position] |= *nullable;
                    }
                }
            }
            columns
        }

        // A group key cannot become NULL by being grouped on — except where a grouping set
        // substitutes one deliberately, which the plan already carries as null_exprs. An
        // aggregate's own output can be NULL: a sum over no rows is.
        NodeRef::Aggregate(aggregate) => aggregate_can_be_null(&aggregate.body, &child(0), width),
        NodeRef::AggregateBatches(aggregate) => {
            aggregate_can_be_null(&aggregate.body, &child(0), width)
        }

        // An outer join null-pads the side it does not preserve. The projection is over the
        // joined table, so the padding is read there and then narrowed.
        NodeRef::Join(join) => {
            let joined = joined_can_be_null(join.join_type, &child(0), &child(1));
            match &join.projection {
                Some(kept) => kept.iter().map(|c| nullable_at(&joined, *c)).collect(),
                None => joined,
            }
        }
        NodeRef::CrossJoin(_) => [child(0), child(1)].concat(),
        NodeRef::NestedLoopJoin(join) => {
            use crate::plan::NestedLoopJoinType;
            let (build, probe) = (child(0), child(1));
            match join.join_type {
                NestedLoopJoinType::Inner => [build, probe].concat(),
                // Left keeps its build rows and pads the probe.
                NestedLoopJoinType::Left => {
                    [build, vec![true; probe.len()]].concat()
                }
            }
        }
    }
}

/// The joined table, before any projection: build columns then probe columns, with the
/// unpreserved side padded. The semi family emits one side only.
fn joined_can_be_null(join_type: JoinType, build: &[bool], probe: &[bool]) -> Vec<bool> {
    let padded = |columns: &[bool]| vec![true; columns.len()];
    match join_type {
        JoinType::Inner => [build.to_vec(), probe.to_vec()].concat(),
        JoinType::Left => [build.to_vec(), padded(probe)].concat(),
        JoinType::Right => [padded(build), probe.to_vec()].concat(),
        JoinType::Full => [padded(build), padded(probe)].concat(),
        JoinType::LeftSemi | JoinType::LeftAnti => build.to_vec(),
        JoinType::RightSemi | JoinType::RightAnti => probe.to_vec(),
        // The mark column is a boolean the join computes and never NULL.
        JoinType::LeftMark => [build.to_vec(), vec![false]].concat(),
    }
}

fn aggregate_can_be_null(
    body: &crate::plan::AggregateBody,
    input: &[bool],
    width: usize,
) -> Vec<bool> {
    let mut columns: Vec<bool> = body
        .group_by
        .iter()
        .map(|key| expr_can_be_null(key, input))
        .collect();
    // A grouping set substitutes NULL for the keys it excludes, so any key some set drops
    // can be NULL in the output — and the grouping id itself never is.
    for mask in &body.grouping_sets {
        for (position, dropped) in mask.iter().enumerate() {
            if *dropped && position < columns.len() {
                columns[position] = true;
            }
        }
    }
    if !body.grouping_sets.is_empty() {
        columns.push(false);
    }
    // Everything the aggregators and the finalize expressions produce: a sum over no rows
    // is NULL, and a stddev under its ddof is NULL by construction.
    columns.resize(width.max(columns.len()), true);
    columns.truncate(width);
    columns
}

/// An expression can be NULL unless every path to a value is known not to be. A bare column
/// asks its input; an operator asks its operands; anything this does not model says yes.
fn expr_can_be_null(expr: &Expr, input: &[bool]) -> bool {
    match expr {
        Expr::Column(reference) => nullable_at(input, reference.index),
        Expr::Literal(value) => value.is_null(),
        Expr::Binary { left, right, .. } => {
            expr_can_be_null(left, input) || expr_can_be_null(right, input)
        }
        Expr::Cast { expr, .. } => expr_can_be_null(expr, input),
        // A predicate answers true or false about a NULL rather than becoming one.
        Expr::Unary { op, arg } => match op {
            crate::plan::UnaryOp::IsNull | crate::plan::UnaryOp::IsNotNull => false,
            _ => expr_can_be_null(arg, input),
        },
        // No ELSE is an implicit NULL, and every branch is a value the CASE can return.
        Expr::Case { when_then, else_expr, .. } => match else_expr {
            None => true,
            Some(otherwise) => {
                expr_can_be_null(otherwise, input)
                    || when_then
                        .iter()
                        .any(|(_, then)| expr_can_be_null(then, input))
            }
        },
        Expr::Like { expr, pattern, .. } => {
            expr_can_be_null(expr, input) || expr_can_be_null(pattern, input)
        }
        // coalesce is the shape that makes a general rule wrong here, so this does not
        // guess: a function's result is possibly-NULL.
        Expr::ScalarFunction { .. } => true,
    }
}
