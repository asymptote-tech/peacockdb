//! A DISTINCT aggregate node lowered to two aggregate sequences.
//!
//! DataFusion removes a DISTINCT only where `f(f(x))` is `f(x)`, so anything else arrives
//! with the flag set. This engine separates init from merge, so the outer level can merge
//! each companion's state instead of re-applying its function: an inner stage groups on
//! `(x, keys)` and dedups the argument, an outer stage groups on `keys` alone. The shape
//! and what stays refused: `llm-wiki/architecture.md`, "DISTINCT lowers to grouping".

use std::sync::Arc;

use datafusion::arrow::datatypes::{DataType, Field, Schema as ArrowSchema};
use datafusion::common::ScalarValue;
use datafusion::logical_expr::Aggregate;
use datafusion::physical_expr::PhysicalExpr;
use datafusion::physical_expr::aggregate::AggregateFunctionExpr;
use datafusion::physical_expr::expressions::CastExpr;
use datafusion::physical_plan::ExecutionPlan;
use datafusion::physical_plan::aggregates::AggregateExec;

use super::super::Translator;
use super::super::expr::translate_expr;
use super::super::nodes::node;
use super::{InitFrom, Shuffle, Stage, sequence};
use crate::plan::PlanError;
use crate::plan::{Expr, GpuNode};
use crate::plan::{GpuProject, NamedExpr, Schema};
use crate::plan::{Merge, decomposition, resolve};

/// The inner stage's first key: the deduplicated argument, its own column even where it is
/// also a group key, since a grouping set may mask a key to NULL and this never is.
pub(crate) const DISTINCT_ARG: &str = "__distinct_arg";

pub(crate) enum Classified {
    /// Every DISTINCT aggregate's argument strips to `base`, and every companion merges
    /// per column.
    Lower {
        base: Arc<dyn PhysicalExpr>,
    },
    Refuse(PlanError),
    /// Not a shape the lowering knows; `decompose`'s check refuses it.
    NotOurs,
}

/// A cast that keeps distinct values distinct, so deduplicating under it or over it is
/// the same set. DataFusion adds these to `sum`'s and `avg`'s argument (`sum.rs`,
/// `average.rs`) and not to `count`'s.
fn keeps_distinct(from: &DataType, to: &DataType) -> bool {
    use DataType::*;
    match (from, to) {
        (Int8, Int16 | Int32 | Int64) | (Int16, Int32 | Int64) | (Int32, Int64) => true,
        (UInt8, Int16 | Int32 | Int64 | UInt16 | UInt32 | UInt64)
        | (UInt16, Int32 | Int64 | UInt32 | UInt64)
        | (UInt32, Int64 | UInt64) => true,
        // Exact below 2^53, which no key reaches.
        (from, Float64) if from.is_integer() => true,
        // Wider on both sides of the point.
        (Decimal128(p, s), Decimal128(p2, s2)) => {
            s2 >= s && (*p2 as i16 - *s2 as i16) >= (*p as i16 - *s as i16)
        }
        // Fifteen significant digits fit a double's 15.95.
        (Decimal128(p, _), Float64) => *p <= 15,
        _ => false,
    }
}

/// The argument under DataFusion's coercion casts that keep distinct values distinct, and
/// those casts' targets, outermost first.
pub(crate) fn stripped(
    expr: &Arc<dyn PhysicalExpr>,
    schema: &ArrowSchema,
) -> (Arc<dyn PhysicalExpr>, Vec<DataType>) {
    let mut casts = Vec::new();
    let mut expr = expr.clone();
    while let Some(cast) = expr.as_any().downcast_ref::<CastExpr>() {
        let Ok(from) = cast.expr().data_type(schema) else {
            break;
        };
        if !keeps_distinct(&from, cast.cast_type()) {
            break;
        }
        casts.push(cast.cast_type().clone());
        let inner = cast.expr().clone();
        expr = inner;
    }
    (expr, casts)
}

pub(crate) fn classify(
    aggregates: &[Arc<AggregateFunctionExpr>],
    input_schema: &ArrowSchema,
    finished: bool,
) -> Classified {
    // A partial with no final above it hands its state on, and a DISTINCT's state is
    // DataFusion's list of values, which nothing here produces.
    if !finished {
        return Classified::NotOurs;
    }
    let mut base: Option<Arc<dyn PhysicalExpr>> = None;
    for aggregate in aggregates.iter().filter(|a| a.is_distinct()) {
        let expressions = aggregate.expressions();
        let [only] = expressions.as_slice() else {
            return Classified::Refuse(PlanError::Unsupported(format!(
                "{}: a DISTINCT over more than one argument (#144)",
                aggregate.name()
            )));
        };
        let (this, _) = stripped(only, input_schema);
        match &base {
            None => base = Some(this),
            Some(first) if first.as_ref() == this.as_ref() => {}
            Some(_) => {
                return Classified::Refuse(PlanError::Unsupported(format!(
                    "{}: a second DISTINCT argument (#144)",
                    aggregate.name()
                )));
            }
        }
    }
    for companion in aggregates.iter().filter(|a| !a.is_distinct()) {
        // An unknown function is left to `decompose`, which refuses it by name.
        if let Ok(spec) = resolve(companion.fun().name())
            && matches!(decomposition(spec.func).merge, Merge::Combined(_))
        {
            return Classified::Refuse(PlanError::Unsupported(format!(
                "{} beside a DISTINCT aggregate: its state merges in one call, which the \
                 outer stage's init cannot run (#261)",
                companion.name()
            )));
        }
    }
    match base {
        Some(base) => Classified::Lower { base },
        None => Classified::NotOurs,
    }
}

/// The inner stage: the stripped argument first, its own column even where it is also a key,
/// then the keys; the companions' inits; state out. This is where the input is translated.
fn inner_stage(
    t: &Translator,
    partial: &AggregateExec,
    shuffle: Shuffle,
    base: &Arc<dyn PhysicalExpr>,
    base_type: &DataType,
    inner_id_type: &DataType,
) -> Result<Box<dyn GpuNode>, PlanError> {
    let input_schema = partial.input().schema();
    let group = partial.group_expr();
    let n = group.expr().len();
    let mut group_by = vec![translate_expr(base, &input_schema)?];
    for (expr, _) in group.expr().iter() {
        group_by.push(translate_expr(expr, &input_schema)?);
    }
    let mut key_fields = vec![Field::new(DISTINCT_ARG, base_type.clone(), true)];
    key_fields.extend((0..n).map(|i| partial.schema().field(i).clone()));
    let (grouping_sets, null_exprs) = if !group.is_single() {
        // The id folds the first key highest; the argument is never masked, so its bit is
        // always 0 and the inner id is DataFusion's over the keys alone. Only its width can
        // differ: n + 1 keys may need the next type.
        key_fields.push(Field::new(
            Aggregate::INTERNAL_GROUPING_ID,
            inner_id_type.clone(),
            false,
        ));
        let masks = group
            .groups()
            .iter()
            .map(|mask| {
                std::iter::once(false)
                    .chain(mask.iter().copied())
                    .collect::<Vec<bool>>()
            })
            .collect();
        let mut nulls = vec![Expr::Literal(
            ScalarValue::try_from(base_type)
                .map_err(|e| PlanError::Invalid(format!("{DISTINCT_ARG}: {e}")))?,
        )];
        for (expr, _) in group.null_expr().iter() {
            nulls.push(translate_expr(expr, &input_schema)?);
        }
        (masks, nulls)
    } else {
        (Vec::new(), Vec::new())
    };
    let companions = partial
        .aggr_expr()
        .iter()
        .filter(|a| !a.is_distinct())
        .map(|a| (a.clone(), InitFrom::Values))
        .collect();
    sequence(
        Stage {
            input: node(t, partial.input())?,
            input_schema: input_schema.clone(),
            group_by,
            key_fields,
            grouping_sets,
            null_exprs,
            aggregates: companions,
            finished: None,
        },
        inner_shuffle(shuffle),
    )
}

/// The two stages. Called before `aggregate_sequence` translates anything, so the input is
/// translated here exactly once: a second translation reaches its sources twice, and
/// tp4-sized's two-pass planner refuses a plan whose passes reach different source counts.
pub(crate) fn lower(
    t: &Translator,
    partial: &AggregateExec,
    finisher: &AggregateExec,
    shuffle: Shuffle,
    base: Arc<dyn PhysicalExpr>,
) -> Result<Box<dyn GpuNode>, PlanError> {
    let input_schema = partial.input().schema();
    let group = partial.group_expr();
    let n = group.expr().len();
    let sets = !group.is_single();
    let base_type = base
        .data_type(&input_schema)
        .map_err(|e| PlanError::Invalid(format!("{DISTINCT_ARG}: {e}")))?;
    let inner_id_type = Aggregate::grouping_id_type(n + 1);
    let inner = inner_stage(t, partial, shuffle, &base, &base_type, &inner_id_type)?;

    // The outer stage: the keys, over the inner's state; no shuffle — the inner's hash is on
    // a subset of these keys, or the inner collapsed to one lane.
    let inner_schema = inner
        .kind()
        .schema()
        .expect("an aggregate is not a sink")
        .clone();
    let mut outer_group: Vec<Expr> = (1..=n)
        .map(|i| Expr::column(i as u32, inner_schema.fields.field(i).name()))
        .collect();
    if sets {
        outer_group.push(Expr::column(
            (n + 1) as u32,
            Aggregate::INTERNAL_GROUPING_ID,
        ));
    }
    // The outer stage declares the id at the inner's width; a project above it narrows the
    // id to DataFusion's where the two differ.
    let finished_fields: Vec<Field> = finisher
        .schema()
        .fields()
        .iter()
        .enumerate()
        .map(|(i, field)| {
            if sets && i == n {
                Field::new(
                    Aggregate::INTERNAL_GROUPING_ID,
                    inner_id_type.clone(),
                    false,
                )
            } else {
                field.as_ref().clone()
            }
        })
        .collect();
    let finished = Arc::new(ArrowSchema::new(finished_fields));
    let mut aggregates = Vec::with_capacity(partial.aggr_expr().len());
    for a in partial.aggr_expr() {
        if a.is_distinct() {
            let (_, casts) = stripped(&a.expressions()[0], &input_schema);
            let mut arg = Expr::column(0, DISTINCT_ARG);
            let mut arg_type = base_type.clone();
            for target in casts.iter().rev() {
                arg = Expr::Cast {
                    expr: Box::new(arg),
                    target: target.clone(),
                };
                arg_type = target.clone();
            }
            aggregates.push((a.clone(), InitFrom::Deduplicated { arg, arg_type }));
        } else {
            let cols = inner_schema
                .agg_state
                .iter()
                .find(|s| s.output == a.name())
                .cloned()
                .ok_or_else(|| {
                    PlanError::Invalid(format!(
                        "{}: the inner stage holds no state for it",
                        a.name()
                    ))
                })?;
            aggregates.push((a.clone(), InitFrom::State(cols)));
        }
    }
    let outer = sequence(
        Stage {
            input: inner,
            input_schema: inner_schema.fields.clone(),
            group_by: outer_group,
            // The keys, plus `__grouping_id` under grouping sets.
            key_fields: (0..n + usize::from(sets))
                .map(|i| finished.field(i).clone())
                .collect(),
            grouping_sets: Vec::new(),
            null_exprs: Vec::new(),
            aggregates,
            finished: Some(finished),
        },
        Shuffle::None,
    )?;

    // Not a cast in the outer `group_by`: `regrouped_key_distribution` and the finalizing
    // merge's co-location check read plain column keys only, so a cast key would lose the
    // shuffle's distribution.
    let wanted = finisher.schema();
    if sets && wanted.field(n).data_type() != &inner_id_type {
        let exprs = wanted
            .fields()
            .iter()
            .enumerate()
            .map(|(i, field)| {
                let column = Expr::column(i as u32, field.name());
                let expr = if i == n {
                    Expr::Cast {
                        expr: Box::new(column),
                        target: field.data_type().clone(),
                    }
                } else {
                    column
                };
                NamedExpr::new(expr, field.name())
            })
            .collect();
        return Ok(Box::new(GpuProject::new(outer, exprs, Schema::new(wanted))));
    }
    Ok(outer)
}

/// DataFusion's shuffle, moved one column right: its key ordinals are the partial's, and
/// the inner stage puts `__distinct_arg` first.
fn inner_shuffle(shuffle: Shuffle) -> Shuffle {
    match shuffle {
        Shuffle::ByHash { keys, n } => Shuffle::ByHash {
            keys: keys.iter().map(|key| key + 1).collect(),
            n,
        },
        other => other,
    }
}
