//! The aggregate sequence: what a partial declares, decomposed into the parts each
//! position needs. Which parts are emitted is decided here rather than by the node kinds
//! alone, because the same DataFusion pair becomes a different tree at one lane and at
//! four.

use std::sync::Arc;

use datafusion::arrow::datatypes::{DataType, Field, Fields, Schema as ArrowSchema, SchemaRef};
use datafusion::common::ScalarValue;
use datafusion::physical_expr::aggregate::AggregateFunctionExpr;
use datafusion::physical_plan::aggregates::{AggregateExec, AggregateMode};
use datafusion::physical_plan::coalesce_batches::CoalesceBatchesExec;
use datafusion::physical_plan::coalesce_partitions::CoalescePartitionsExec;
use datafusion::physical_plan::repartition::RepartitionExec;
use datafusion::physical_plan::{ExecutionPlan, Partitioning};

use super::Translator;
use super::common::{batches, hash_key_ordinals, lanes};
use super::expr::translate_expr;
use super::nodes::{merged, node, shuffled};
use crate::plan::BatchLayout;
use crate::plan::GpuNode;
use crate::plan::PlanError;
use crate::plan::{AggCall, AggFunc, Decomposition, Merge, PlanAgg, UnaryOp};
use crate::plan::{AggStateColumns, Schema};
use crate::plan::{AggregateBody, GpuAggregate, GpuAggregateBatches};
use crate::plan::{Expr, NamedExpr};
use crate::plan::{decomposition, finalize, resolve};

mod distinct;

#[cfg(test)]
mod tests;

/// What sits between a partial aggregate and the final one: DataFusion spells a shuffle as
/// a hash repartition and a lane collapse as a coalesce, and which one it chose is what
/// decides whether this aggregate re-lands its rows by key or merges them into one lane.
enum Shuffle {
    None,
    ByHash { keys: Vec<u32>, n: usize },
    Collapse,
}

/// Look through the nodes a shuffle is spelled with. A coalesce carrying a fetch is not
/// one of them — that is a limit, and it stops the walk.
fn shuffle_below(plan: &Arc<dyn ExecutionPlan>) -> (Arc<dyn ExecutionPlan>, Shuffle) {
    let mut node = plan.clone();
    let mut shuffle = Shuffle::None;
    loop {
        let any = node.as_any();
        if let Some(coalesce) = any.downcast_ref::<CoalesceBatchesExec>() {
            if coalesce.fetch().is_some() {
                return (node, shuffle);
            }
            node = coalesce.input().clone();
            continue;
        }
        if let Some(collapse) = any.downcast_ref::<CoalescePartitionsExec>() {
            shuffle = Shuffle::Collapse;
            node = collapse.input().clone();
            continue;
        }
        if let Some(repartition) = any.downcast_ref::<RepartitionExec>() {
            if let Partitioning::Hash(exprs, n) = repartition.partitioning() {
                match hash_key_ordinals(exprs, &repartition.input().schema()) {
                    Ok(keys) => shuffle = Shuffle::ByHash { keys, n: *n },
                    // Not a shape this walk can describe; the node arm will refuse it.
                    Err(_) => return (node, shuffle),
                }
            }
            node = repartition.input().clone();
            continue;
        }
        return (node, shuffle);
    }
}

/// Whether DataFusion declares the state column one of our aggregators produces nullable.
/// With a single state column there is nothing to mismatch; beyond that the aggregator's
/// tag is what names it (`avg(x)[count]`), and a tag with no field is a drift this must
/// not paper over. The type is not read here: it is `state_type`'s.
fn declared_nullable(
    declared: &[Field],
    func: PlanAgg,
    aggregate: &str,
) -> Result<bool, PlanError> {
    if declared.len() == 1 {
        return Ok(declared[0].is_nullable());
    }
    let tag = format!("[{}]", func.tag());
    declared
        .iter()
        .find(|field| field.name().ends_with(&tag))
        .map(Field::is_nullable)
        .ok_or_else(|| {
            PlanError::Invalid(format!(
                "{aggregate}: DataFusion declares no {tag} state column, so this mode's \
                 decomposition of it has drifted"
            ))
        })
}

/// Where one aggregate's init reads from.
enum InitFrom {
    /// The values it was written over: every aggregate but the two below.
    Values,
    /// A DISTINCT aggregate in the outer stage: its own argument rebuilt over the inner
    /// stage's `__distinct_arg` (column 0), DataFusion's casts re-applied, and that
    /// argument's type.
    Deduplicated { arg: Expr, arg_type: DataType },
    /// A companion in the outer stage: the state the inner stage left at these positions.
    State(AggStateColumns),
}

/// Everything one aggregate sequence needs, from a DataFusion pair or built by the
/// DISTINCT lowering.
struct Stage {
    input: Box<dyn GpuNode>,
    input_schema: SchemaRef,
    group_by: Vec<Expr>,
    /// The output's key columns: the group keys, then `__grouping_id` under grouping sets.
    key_fields: Vec<Field>,
    grouping_sets: Vec<Vec<bool>>,
    null_exprs: Vec<Expr>,
    aggregates: Vec<(Arc<AggregateFunctionExpr>, InitFrom)>,
    /// The finished output's schema, keys then one column per aggregate; `None` emits state.
    finished: Option<SchemaRef>,
}

/// What one aggregate node's aggregates become in each position.
struct Decomposed {
    init: Vec<AggCall>,
    merge: Vec<AggCall>,
    state: Vec<Field>,
    finalize: Vec<Expr>,
    /// One per aggregate sql asked for, naming the state columns it decomposed into —
    /// what a merge checks before trusting the positions it is about to merge.
    annotations: Vec<AggStateColumns>,
}

/// One aggregate's state columns and the init calls that produce them.
///
/// The three `InitFrom` arms differ only here: an ordinary aggregate reads its own values,
/// a DISTINCT in the outer stage reads the deduplicated argument, and a companion there
/// reads the inner stage's state through its own merge rule.
///
/// The arms also pair with the decomposition differently. The first two pair by TAG, through
/// `declared_nullable`: DataFusion declares avg as [count, sum] and this table reads
/// [sum, count]. A companion pairs by POSITION, because our state names (`avg(…)$sum`) carry
/// none of DataFusion's `[sum]` tags and the tag lookup would read as drift.
fn state_and_init(
    aggregate: &AggregateFunctionExpr,
    from: &InitFrom,
    rule: Decomposition,
    input_schema: &ArrowSchema,
) -> Result<(Vec<Field>, Vec<AggCall>), PlanError> {
    let mut state = Vec::with_capacity(rule.state.len());
    let mut init = Vec::with_capacity(rule.state.len());
    match from {
        InitFrom::State(cols) => {
            // The outer stage's init runs each state column's merge rule as an ordinary
            // aggregator over the inner stage's state, so its output type is that
            // aggregator's over the inner's — a decimal sum widens again.
            let Merge::PerColumn(funcs) = rule.merge else {
                return Err(PlanError::Invalid(format!(
                    "{}: its state merges in one call, which an init cannot run",
                    aggregate.name()
                )));
            };
            if cols.positions.len() != rule.state.len() {
                return Err(PlanError::Invalid(format!(
                    "{}: the inner stage left {} state columns and this mode decomposes \
                     into {}",
                    aggregate.name(),
                    cols.positions.len(),
                    rule.state.len()
                )));
            }
            for (index, (suffix, _)) in rule.state.iter().enumerate() {
                let at = cols.positions[index] as usize;
                let inner = input_schema.field(at);
                let field = Field::new(
                    format!("{}{suffix}", aggregate.name()),
                    funcs[index].state_type(inner.data_type())?,
                    true,
                );
                init.push(AggCall {
                    func: funcs[index],
                    args: vec![Expr::column(at as u32, inner.name())],
                    outputs: vec![field.clone()],
                });
                state.push(field);
            }
        }
        InitFrom::Values | InitFrom::Deduplicated { .. } => {
            // The state names are ours and the types are `state_type`'s — the aggregator
            // that produces each column, which DataFusion's `state_fields` (its own
            // accumulator's layout, not the one this engine runs) supplies arity and
            // nullability for.
            let (args, arg_type, declared) = match from {
                // A DISTINCT's declared state is DataFusion's list of values, which
                // nothing here runs, so its arity and nullability say nothing about the
                // twin this init is: every column it declares is nullable.
                InitFrom::Deduplicated { arg, arg_type } => {
                    (vec![arg.clone()], arg_type.clone(), None)
                }
                _ => {
                    let declared = aggregate
                        .state_fields()
                        .map_err(|e| PlanError::Invalid(format!("{}: {e}", aggregate.name())))?;
                    if declared.len() != rule.state.len() {
                        return Err(PlanError::Invalid(format!(
                            "{}: DataFusion declares {} state columns and this mode \
                             decomposes into {}",
                            aggregate.name(),
                            declared.len(),
                            rule.state.len()
                        )));
                    }
                    let mut args = Vec::with_capacity(aggregate.expressions().len());
                    for arg in aggregate.expressions() {
                        args.push(translate_expr(&arg, input_schema)?);
                    }
                    let arg_type = match aggregate.expressions().first() {
                        Some(arg) => arg.data_type(input_schema).map_err(|e| {
                            PlanError::Invalid(format!("{}: {e}", aggregate.name()))
                        })?,
                        None => DataType::Null,
                    };
                    (args, arg_type, Some(declared))
                }
            };
            for (suffix, func) in rule.state {
                let nullable = match &declared {
                    Some(declared) => declared_nullable(declared, *func, aggregate.name())?,
                    None => true,
                };
                state.push(Field::new(
                    format!("{}{suffix}", aggregate.name()),
                    func.state_type(&arg_type)?,
                    nullable,
                ));
            }
            for ((_, func), field) in rule.state.iter().zip(state.iter()) {
                init.push(AggCall {
                    func: *func,
                    args: args.clone(),
                    outputs: vec![field.clone()],
                });
            }
        }
    }
    Ok((state, init))
}

fn decompose(
    aggregates: &[(Arc<AggregateFunctionExpr>, InitFrom)],
    input_schema: &ArrowSchema,
    n_keys: usize,
) -> Result<Decomposed, PlanError> {
    let mut decomposed = Decomposed {
        init: Vec::new(),
        merge: Vec::new(),
        state: Vec::new(),
        finalize: Vec::new(),
        annotations: Vec::new(),
    };

    for (aggregate, from) in aggregates {
        if matches!(from, InitFrom::Values) && aggregate.is_distinct() {
            return Err(PlanError::Unsupported(format!(
                "DISTINCT inside {} in a shape the lowering does not handle",
                aggregate.name()
            )));
        }
        let spec = resolve(aggregate.fun().name())?;
        let rule = decomposition(spec.func);
        let state_at = n_keys + decomposed.state.len();
        let (state, init) = state_and_init(aggregate, from, rule, input_schema)?;
        decomposed.init.extend(init);

        let state_columns: Vec<Expr> = state
            .iter()
            .enumerate()
            .map(|(offset, field)| Expr::column((state_at + offset) as u32, field.name()))
            .collect();
        match rule.merge {
            Merge::PerColumn(funcs) => {
                for ((func, column), field) in
                    funcs.iter().zip(state_columns.iter()).zip(state.iter())
                {
                    decomposed.merge.push(AggCall {
                        func: *func,
                        args: vec![column.clone()],
                        outputs: vec![field.clone()],
                    });
                }
            }
            Merge::Combined(func) => decomposed.merge.push(AggCall {
                func,
                args: state_columns,
                outputs: state.clone(),
            }),
        }

        let out_type = aggregate.field().data_type().clone();
        let mut finished = finalize(spec, &state, state_at as u32, &out_type);
        if !matches!(from, InitFrom::Values) {
            // A sum or min/max read off state the outer init widened is cast back to the
            // type DataFusion declares: no executor changes a type the plan did not ask for.
            if matches!(spec.func, AggFunc::Sum | AggFunc::Min | AggFunc::Max)
                && state[0].data_type() != &out_type
            {
                finished = Expr::Cast {
                    expr: Box::new(finished),
                    target: out_type.clone(),
                };
            }
            // A count merged by sum is NULL over an empty keyless input, where SQL says 0.
            if spec.func == AggFunc::Count {
                finished = Expr::Case {
                    comparand: None,
                    when_then: vec![(
                        Expr::unary(UnaryOp::IsNull, finished.clone()),
                        Expr::Literal(ScalarValue::Int64(Some(0))),
                    )],
                    else_expr: Some(Box::new(finished)),
                };
            }
        }
        decomposed.finalize.push(finished);
        decomposed.annotations.push(AggStateColumns {
            output: aggregate.name().to_string(),
            func: spec.func,
            ddof: spec.ddof,
            positions: (state_at..state_at + state.len())
                .map(|position| position as u32)
                .collect(),
        });
        decomposed.state.extend(state);
    }

    Ok(decomposed)
}

pub(crate) fn aggregate(
    t: &Translator,
    exec: &AggregateExec,
) -> Result<Box<dyn GpuNode>, PlanError> {
    match exec.mode() {
        AggregateMode::Partial => aggregate_sequence(t, exec, None, Shuffle::None),
        AggregateMode::Final | AggregateMode::FinalPartitioned => {
            let (below, shuffle) = shuffle_below(exec.input());
            let partial = below
                .as_any()
                .downcast_ref::<AggregateExec>()
                .filter(|partial| matches!(partial.mode(), AggregateMode::Partial))
                .ok_or_else(|| {
                    PlanError::Unsupported(format!(
                        "a final aggregate over {} rather than a partial one",
                        below.name()
                    ))
                })?;
            aggregate_sequence(t, partial, Some(exec), shuffle)
        }
        AggregateMode::Single | AggregateMode::SinglePartitioned => {
            aggregate_sequence(t, exec, Some(exec), Shuffle::None)
        }
    }
}

/// The whole sequence, from the aggregators the partial declares: init per batch, a
/// per-lane merge where a lane holds several batches, the shuffle where the lanes must
/// be re-landed by group key, and the merge that finishes it. Each part is emitted only
/// where this lane count and batch layout need it — a one-lane region never splits, so
/// there is nothing to merge back.
fn aggregate_sequence(
    t: &Translator,
    partial: &AggregateExec,
    finisher: Option<&AggregateExec>,
    shuffle: Shuffle,
) -> Result<Box<dyn GpuNode>, PlanError> {
    if partial.filter_expr().iter().any(Option::is_some) {
        return Err(PlanError::Unsupported(
            "a filtered aggregate (#161)".to_string(),
        ));
    }
    if partial.aggr_expr().iter().any(|a| a.is_distinct()) {
        match distinct::classify(
            partial.aggr_expr(),
            &partial.input().schema(),
            finisher.is_some(),
        ) {
            distinct::Classified::Refuse(err) => return Err(err),
            distinct::Classified::Lower { base } => {
                return distinct::lower(
                    t,
                    partial,
                    finisher.expect("classify saw a finisher"),
                    shuffle,
                    base,
                );
            }
            distinct::Classified::NotOurs => {}
        }
    }
    let input = node(t, partial.input())?;
    let input_schema = partial.input().schema();
    let group = partial.group_expr();

    let mut group_by = Vec::with_capacity(group.expr().len());
    for (expr, _) in group.expr().iter() {
        group_by.push(translate_expr(expr, &input_schema)?);
    }
    // Grouping sets add one output column, `__grouping_id`, which the init emits like
    // any other and everything above groups on beside the keys. Its name and type are
    // DataFusion's, off the partial's own schema.
    let key_columns = group.expr().len() + usize::from(!group.is_single());
    let key_fields: Vec<Field> = (0..key_columns)
        .map(|index| partial.schema().field(index).clone())
        .collect();
    let mut null_exprs = Vec::new();
    for (expr, _) in group.null_expr().iter() {
        null_exprs.push(translate_expr(expr, &input_schema)?);
    }
    let grouping_sets: Vec<Vec<bool>> = if group.is_single() {
        Vec::new()
    } else {
        group.groups().to_vec()
    };

    sequence(
        Stage {
            input,
            input_schema,
            group_by,
            key_fields,
            grouping_sets,
            null_exprs,
            aggregates: partial
                .aggr_expr()
                .iter()
                .map(|a| (a.clone(), InitFrom::Values))
                .collect(),
            finished: finisher.map(|finisher| finisher.schema()),
        },
        shuffle,
    )
}

/// One sequence, from a stage: init per batch, a per-lane merge where a lane holds several
/// batches, the shuffle, and the merge that finishes it.
fn sequence(stage: Stage, shuffle: Shuffle) -> Result<Box<dyn GpuNode>, PlanError> {
    let Stage {
        input,
        input_schema,
        group_by,
        key_fields,
        grouping_sets,
        null_exprs,
        aggregates,
        finished: finished_schema,
    } = stage;

    let decomposed = decompose(&aggregates, &input_schema, key_fields.len())?;
    let intermediate = Schema {
        fields: Arc::new(ArrowSchema::new(Fields::from(
            [key_fields.clone(), decomposed.state.clone()].concat(),
        ))),
        group_keys: (0..key_fields.len() as u32).collect(),
        agg_state: decomposed.annotations.clone(),
    };
    let keys_through: Vec<Expr> = key_fields
        .iter()
        .enumerate()
        .map(|(index, field)| Expr::column(index as u32, field.name()))
        .collect();

    // The output names are DataFusion's, so a finalized column lands where the plan
    // above it expects to read it.
    let finished = finished_schema.map(|names| {
        let finalize: Vec<NamedExpr> = decomposed
            .finalize
            .iter()
            .enumerate()
            .map(|(index, expr)| {
                NamedExpr::new(expr.clone(), names.field(key_fields.len() + index).name())
            })
            .collect();
        // The finalized output holds the keys where the intermediate did and the
        // finalized columns where the state was, so the keys are still annotated and
        // the state is gone.
        let output = Schema {
            fields: names,
            group_keys: (0..key_fields.len() as u32).collect(),
            agg_state: Vec::new(),
        };
        (finalize, output)
    });

    // One batch in one lane is already the whole of every group, so the init node
    // finishes the aggregate itself.
    if let Some((finalize, output)) = &finished
        && batches(input.as_ref()) == BatchLayout::SingleBatch
        && lanes(input.as_ref()) == 1
    {
        return Ok(Box::new(GpuAggregate::new(
            input,
            AggregateBody {
                group_by,
                grouping_sets,
                null_exprs,
                aggs: decomposed.init,
                finalize: Some(finalize.clone()),
            },
            intermediate,
            output.clone(),
        )));
    }

    let mut tree: Box<dyn GpuNode> = Box::new(GpuAggregate::new(
        input,
        AggregateBody {
            group_by,
            grouping_sets,
            null_exprs,
            aggs: decomposed.init,
            finalize: None,
        },
        intermediate.clone(),
        intermediate.clone(),
    ));

    // A merge groups on what the init emitted — keys and, where there was one, the
    // grouping id — and expands nothing: the sets were expanded once, below.
    let merge_body = |aggs: &[AggCall], finalize: Option<Vec<NamedExpr>>| AggregateBody {
        group_by: keys_through.clone(),
        grouping_sets: Vec::new(),
        null_exprs: Vec::new(),
        aggs: aggs.to_vec(),
        finalize,
    };

    // The per-lane half exists to shrink what crosses the shuffle; where the lanes
    // stay put there is nothing for it to do that the finishing merge does not.
    let regrouped = !matches!(shuffle, Shuffle::None) && lanes(tree.as_ref()) > 1;
    if regrouped && batches(tree.as_ref()) != BatchLayout::SingleBatch {
        tree = Box::new(GpuAggregateBatches::new(
            tree,
            merge_body(&decomposed.merge, None),
            intermediate.clone(),
            intermediate.clone(),
        ));
    }

    tree = match shuffle {
        Shuffle::ByHash { keys, n } if lanes(tree.as_ref()) > 1 => shuffled(tree, keys, n),
        // One lane holds every group already: v1 skips the shuffle for a one-lane
        // input exactly as it does for a keyless aggregate.
        Shuffle::ByHash { .. } => tree,
        Shuffle::Collapse => merged(tree),
        Shuffle::None => tree,
    };

    let (finalize, output) = match finished {
        Some((finalize, output)) => (Some(finalize), output),
        None => (None, intermediate.clone()),
    };
    Ok(Box::new(GpuAggregateBatches::new(
        tree,
        merge_body(&decomposed.merge, finalize),
        intermediate,
        output,
    )))
}
