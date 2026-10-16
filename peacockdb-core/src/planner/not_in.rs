//! `NOT IN` as SQL means it: a logical rewrite, nothing in the executor (#80).
//!
//! DataFusion 45 plans `x NOT IN (S)` as the anti join of `NOT EXISTS (S AND y = x)`, which
//! is the wrong answer wherever `x` or `y` is NULL. This rule rewrites such a `NOT IN` on a
//! filter's AND/OR spine — where a row is kept on true alone, so a NULL and a false drop it
//! alike — into the form that is exact there. Off the spine the NULL is read as a value and
//! the shape is refused instead (#250). The rule runs ahead of
//! `decorrelate_predicate_subquery` and visits subqueries' filters too, since a `NOT IN`
//! inside an `EXISTS` is otherwise decorrelated before it is seen.
//! `llm-wiki/tasks/join-rewrite-design.md` §3.5 carries the reasoning and the two forms.

use std::sync::Arc;

use datafusion::common::tree_node::{Transformed, TreeNode};
use datafusion::common::{ExprSchema, Result};
use datafusion::functions_aggregate::expr_fn::count;
use datafusion::logical_expr::expr::{Exists, InSubquery};
use datafusion::logical_expr::{
    BinaryExpr, Expr, Filter, LogicalPlan, LogicalPlanBuilder, Operator, Subquery, lit,
};
use datafusion::optimizer::{OptimizerConfig, OptimizerRule};

use super::NullAwareNotIn;
use super::nullability::{in_subquery_may_meet_null, refuse_nullable_in_off_the_spine};

impl OptimizerRule for NullAwareNotIn {
    fn name(&self) -> &str {
        "null_aware_not_in"
    }

    /// `None`, because the walk is the rule's own: `ApplyOrder` does not enter the plans
    /// hanging off expressions, and a `NOT IN` inside an `EXISTS` lives in one.
    fn apply_order(&self) -> Option<datafusion::optimizer::ApplyOrder> {
        None
    }

    fn rewrite(
        &self,
        plan: LogicalPlan,
        _config: &dyn OptimizerConfig,
    ) -> Result<Transformed<LogicalPlan>> {
        plan.transform_down_with_subqueries(|node| match node {
            LogicalPlan::Filter(filter) => {
                let input = Arc::clone(&filter.input);
                let normalized = negation_normal_form(filter.predicate.clone(), false);
                let folded = normalized.transformed;
                let rewritten = spine(normalized.data, &input)?;
                if !folded && !rewritten.transformed {
                    return Ok(Transformed::no(LogicalPlan::Filter(filter)));
                }
                Ok(Transformed::yes(LogicalPlan::Filter(Filter::try_new(
                    rewritten.data,
                    input,
                )?)))
            }
            other => Ok(Transformed::no(other)),
        })
    }
}

/// Negation normal form: every `NOT` pushed down to a leaf. De Morgan and double negation
/// hold in SQL's three-valued logic, so the predicate means what it meant; a `NOT` reaching
/// an `IN` or `EXISTS` leaf flips its `negated` flag instead of wrapping it. Afterwards only
/// `AND` and `OR` sit above an `IN` that was reached through connectives and `NOT`s.
fn negation_normal_form(expr: Expr, negated: bool) -> Transformed<Expr> {
    match expr {
        Expr::Not(inner) => Transformed::yes(negation_normal_form(*inner, !negated).data),
        Expr::BinaryExpr(BinaryExpr {
            left,
            op: op @ (Operator::And | Operator::Or),
            right,
        }) => {
            let (left, right) = (
                negation_normal_form(*left, negated),
                negation_normal_form(*right, negated),
            );
            let op = match (op, negated) {
                (Operator::And, true) => Operator::Or,
                (Operator::Or, true) => Operator::And,
                (op, _) => op,
            };
            let changed = negated || left.transformed || right.transformed;
            let expr = Expr::BinaryExpr(BinaryExpr::new(
                Box::new(left.data),
                op,
                Box::new(right.data),
            ));
            if changed {
                Transformed::yes(expr)
            } else {
                Transformed::no(expr)
            }
        }
        Expr::InSubquery(subquery) if negated => Transformed::yes(Expr::InSubquery(InSubquery {
            negated: !subquery.negated,
            ..subquery
        })),
        Expr::Exists(exists) if negated => Transformed::yes(Expr::Exists(Exists {
            negated: !exists.negated,
            ..exists
        })),
        leaf if negated => Transformed::yes(Expr::Not(Box::new(leaf))),
        leaf => Transformed::no(leaf),
    }
}

/// The AND/OR spine of a filter's predicate: where a row is kept on true alone, so a NULL
/// and a false agree and the two-valued rewrite below is exact.
fn spine(expr: Expr, outer: &Arc<LogicalPlan>) -> Result<Transformed<Expr>> {
    match expr {
        Expr::BinaryExpr(BinaryExpr {
            left,
            op: op @ (Operator::And | Operator::Or),
            right,
        }) => {
            let (left, right) = (spine(*left, outer)?, spine(*right, outer)?);
            let changed = left.transformed || right.transformed;
            let expr = Expr::BinaryExpr(BinaryExpr::new(
                Box::new(left.data),
                op,
                Box::new(right.data),
            ));
            Ok(if changed {
                Transformed::yes(expr)
            } else {
                Transformed::no(expr)
            })
        }
        Expr::InSubquery(InSubquery {
            expr,
            subquery,
            negated: true,
        }) if in_subquery_may_meet_null(&expr, &subquery, outer) => {
            Ok(Transformed::yes(rewrite_not_in(*expr, subquery, outer)?))
        }
        // A positive IN on the spine is left alone: a semi or mark join's false and SQL's
        // NULL drop a row alike there, nullable or not.
        leaf @ Expr::InSubquery(_) => Ok(Transformed::no(leaf)),
        // Anything else ends the spine, and an IN read inside it is off the spine (#250).
        other => {
            refuse_nullable_in_off_the_spine(&other, outer)?;
            Ok(Transformed::no(other))
        }
    }
}

/// `x NOT IN (S)` as a predicate that is true exactly where SQL's is, given that a NULL and
/// a false are the same answer here. §3.5 derives both forms.
fn rewrite_not_in(x: Expr, sub: Subquery, outer: &Arc<LogicalPlan>) -> Result<Expr> {
    let y = Expr::Column(sub.subquery.schema().columns()[0].clone());
    let s = LogicalPlanBuilder::from(sub.subquery.as_ref().clone());
    let outer_x = to_outer_refs(&x, outer)?;
    if sub.outer_ref_columns.is_empty() {
        // DataFusion 45 cannot plan an uncorrelated NOT EXISTS, so S's empty and its
        // NULL-holding cases are asked as counts instead.
        Ok(not_exists(y.clone().eq(outer_x), s.clone())?
            .and(count_of(s.clone().filter(y.is_null())?)?.eq(lit(0i64)))
            .and(x.is_not_null().or(count_of(s)?.eq(lit(0i64)))))
    } else {
        Ok(not_exists(y.clone().eq(outer_x.clone()), s.clone())?
            .and(not_exists(y.is_null(), s.clone())?)
            .and(not_exists(outer_x.is_null(), s)?))
    }
}

/// `NOT EXISTS (S WHERE condition)`, with the outer columns the condition introduced
/// declared on the subquery so DataFusion decorrelates it.
fn not_exists(condition: Expr, s: LogicalPlanBuilder) -> Result<Expr> {
    let plan = s.filter(condition)?.build()?;
    let outer_ref_columns = plan.all_out_ref_exprs();
    Ok(Expr::Exists(Exists {
        subquery: Subquery {
            subquery: Arc::new(plan),
            outer_ref_columns,
        },
        negated: true,
    }))
}

/// `(SELECT count(1) FROM S)` as a scalar subquery.
///
/// `count(lit(1i32))`, never `count(*)`: DataFusion spells `count(*)` as `count(Int64(1))`
/// (`COUNT_STAR_EXPANSION`), and over an unfiltered S its `AggregateStatistics` answers that
/// from the table's statistics with a `PlaceholderRowExec`, which the translator refuses
/// (#158). An `Int32` literal counts the same rows and is not recognized as `count(*)`.
fn count_of(s: LogicalPlanBuilder) -> Result<Expr> {
    let plan = s
        .aggregate(Vec::<Expr>::new(), vec![count(lit(1i32))])?
        .build()?;
    Ok(Expr::ScalarSubquery(Subquery {
        subquery: Arc::new(plan),
        outer_ref_columns: vec![],
    }))
}

/// Every column of `x` as an outer reference, which is what makes the rewritten `S` filter
/// correlated and so decorrelatable.
fn to_outer_refs(x: &Expr, outer: &Arc<LogicalPlan>) -> Result<Expr> {
    x.clone()
        .transform(|expr| match expr {
            Expr::Column(column) => {
                let data_type = outer.schema().data_type(&column)?.clone();
                Ok(Transformed::yes(Expr::OuterReferenceColumn(
                    data_type, column,
                )))
            }
            other => Ok(Transformed::no(other)),
        })
        .map(|transformed| transformed.data)
}
