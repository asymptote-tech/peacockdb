//! One arm per DataFusion node kind: what this engine's tree carries in its place.
//!
//! A conscious decision per kind — nothing is carried over implicitly, and an unrecognized
//! node is a plan-time error naming it. What is reused is DataFusion's planning: the
//! coercions, the decimal scales, the per-aggregate state schemas. What is not is its
//! execution semantics, which annotating a tree with wrappers carries along by accident.

use std::sync::Arc;

use datafusion::arrow::datatypes::Schema as ArrowSchema;
use datafusion::datasource::physical_plan::ParquetExec;
use datafusion::physical_plan::ExecutionPlan;
use datafusion::physical_plan::Partitioning;
use datafusion::physical_plan::aggregates::AggregateExec;
use datafusion::physical_plan::coalesce_batches::CoalesceBatchesExec;
use datafusion::physical_plan::coalesce_partitions::CoalescePartitionsExec;
use datafusion::physical_plan::filter::FilterExec;
use datafusion::physical_plan::joins::{CrossJoinExec, HashJoinExec, NestedLoopJoinExec};
use datafusion::physical_plan::limit::{GlobalLimitExec, LocalLimitExec};
use datafusion::physical_plan::placeholder_row::PlaceholderRowExec;
use datafusion::physical_plan::projection::ProjectionExec;
use datafusion::physical_plan::repartition::RepartitionExec;
use datafusion::physical_plan::sorts::sort::SortExec;
use datafusion::physical_plan::sorts::sort_preserving_merge::SortPreservingMergeExec;
use datafusion::physical_plan::union::{InterleaveExec, UnionExec};
use datafusion::physical_plan::windows::{BoundedWindowAggExec, WindowAggExec};

use super::PerBatchSort;
use super::Translator;
use super::aggregate::aggregate;
use super::common::*;
use super::expr::translate_expr;
use super::scan_mapping::{parquet_table_name, partition, survivor_metadata};
use crate::plan::BatchLayout;
use crate::plan::PlanError;
use crate::plan::Schema;
use crate::plan::{Batching, RowGroupMeta};
use crate::plan::{Expr, NamedExpr};
use crate::plan::{
    GpuAccumulateBatchesAndSort, GpuCoalesceAllBatches, GpuCrossJoin, GpuEmitPartitions, GpuFilter,
    GpuHashJoin, GpuInterleave, GpuLimit, GpuLoadParquet, GpuMergePartitions,
    GpuMergeSortedPartitions, GpuNestedLoopJoin, GpuProject, GpuSort, GpuUnion,
};
use crate::plan::{GpuNode, RowInterval};
use crate::plan::{NestedLoopJoinType, capability};

pub(crate) fn node(
    t: &Translator,
    plan: &Arc<dyn ExecutionPlan>,
) -> Result<Box<dyn GpuNode>, PlanError> {
    let any = plan.as_any();

    if let Some(parquet) = any.downcast_ref::<ParquetExec>() {
        return source(t, parquet);
    }
    if let Some(filter) = any.downcast_ref::<FilterExec>() {
        let input = node(t, filter.input())?;
        let predicate = translate_expr(filter.predicate(), &filter.input().schema())?;
        let projection = filter
            .projection()
            .map(|columns| columns.iter().map(|c| *c as u32).collect());
        return Ok(Box::new(GpuFilter::new(
            input,
            predicate,
            projection,
            Schema::new(filter.schema()),
        )));
    }
    if let Some(project) = any.downcast_ref::<ProjectionExec>() {
        let input = node(t, project.input())?;
        let input_schema = project.input().schema();
        let mut exprs = Vec::with_capacity(project.expr().len());
        for (expr, name) in project.expr() {
            exprs.push(NamedExpr::new(translate_expr(expr, &input_schema)?, name));
        }
        return Ok(Box::new(GpuProject::new(
            input,
            exprs,
            Schema::new(project.schema()),
        )));
    }
    if let Some(sort) = any.downcast_ref::<SortExec>() {
        return self::sort(t, sort);
    }
    if let Some(coalesce) = any.downcast_ref::<CoalesceBatchesExec>() {
        // The target batch size goes; batching is this mode's own concern (#139).
        // A fetch does not: DataFusion's limit pushdown parks a limit here, and
        // dropping the node with it would answer a count over three rows with the
        // count of all of them.
        if let Some(fetch) = coalesce.fetch() {
            let interval = RowInterval {
                skip: 0,
                fetch: Some(fetch as u64),
            };
            return mid_plan_limit(t, coalesce.input(), interval);
        }
        return node(t, coalesce.input());
    }
    if any.is::<GlobalLimitExec>() || any.is::<LocalLimitExec>() {
        let (input, interval) = limit_interval(plan).expect("a limit carries an interval");
        return mid_plan_limit(t, &input, interval);
    }
    if let Some(aggregate) = any.downcast_ref::<AggregateExec>() {
        return self::aggregate(t, aggregate);
    }
    if let Some(cross) = any.downcast_ref::<CrossJoinExec>() {
        let build = build_side(t, cross.left())?;
        // With no key to co-locate on, both sides are one lane — the probe as much as
        // the build, though only the build is also one batch.
        let probe = merged(node(t, cross.right())?);
        return Ok(Box::new(GpuCrossJoin::new(
            build,
            probe,
            None,
            Schema::new(cross.schema()),
        )));
    }
    if let Some(nested) = any.downcast_ref::<NestedLoopJoinExec>() {
        return nested_loop_join(t, nested);
    }
    if let Some(repartition) = any.downcast_ref::<RepartitionExec>() {
        return self::repartition(t, repartition);
    }
    if let Some(coalesce) = any.downcast_ref::<CoalescePartitionsExec>() {
        let input = node(t, coalesce.input())?;
        return Ok(merged(input));
    }
    if let Some(merge) = any.downcast_ref::<SortPreservingMergeExec>() {
        return sort_preserving_merge(t, merge);
    }
    if let Some(join) = any.downcast_ref::<HashJoinExec>() {
        return hash_join(t, join);
    }
    if let Some(union) = any.downcast_ref::<UnionExec>() {
        let branches = branches(t, union.inputs().iter(), &union.schema())?;
        return Ok(Box::new(GpuUnion::new(
            branches,
            Schema::new(union.schema()),
        )));
    }
    if let Some(interleave) = any.downcast_ref::<InterleaveExec>() {
        let branches = branches(t, interleave.inputs().iter(), &interleave.schema())?;
        // DataFusion interleaves where its branches share a hash, so lane p of one
        // belongs beside lane p of the next. This mode decides its own lane counts —
        // a cross join or a small source can put one branch on a single lane — and
        // branches that no longer agree have no shared distribution to interleave on.
        let first = branches[0].kind().layout().expect("a branch is not a sink");
        let agreed = branches.iter().all(|branch| {
            let layout = branch.kind().layout().expect("a branch is not a sink");
            layout.n == first.n && layout.key_distribution == first.key_distribution
        });
        let schema = Schema::new(interleave.schema());
        return Ok(if agreed {
            Box::new(GpuInterleave::new(branches, schema))
        } else {
            Box::new(GpuUnion::new(branches, schema))
        });
    }
    if any.is::<WindowAggExec>() || any.is::<BoundedWindowAggExec>() {
        return Err(PlanError::Unsupported(format!(
            "{}: window functions (#143)",
            plan.name()
        )));
    }

    if any.is::<PlaceholderRowExec>() {
        return Err(PlanError::Unsupported(format!(
            "{}: an aggregate DataFusion answered from statistics, so there is no \
             aggregate left to translate (#158)",
            plan.name()
        )));
    }

    Err(PlanError::Unsupported(format!("plan node {}", plan.name())))
}

/// A hash repartition is the shuffle: merge the lanes into one, then scatter that one
/// into N by the same murmur3 both engines use. Round-robin carries no key, so it says
/// nothing this mode acts on and leaves no node.
pub(crate) fn repartition(
    t: &Translator,
    repartition: &RepartitionExec,
) -> Result<Box<dyn GpuNode>, PlanError> {
    let input = node(t, repartition.input())?;
    match repartition.partitioning() {
        Partitioning::Hash(exprs, n) => {
            let keys = hash_key_ordinals(exprs, &repartition.input().schema())?;
            Ok(shuffled(input, keys, *n))
        }
        // Round-robin carries no key, so it says nothing this mode acts on; an unknown
        // partitioning is a claim we cannot read, and guessing at it would be a lane
        // assignment nobody chose.
        Partitioning::RoundRobinBatch(_) => Ok(input),
        Partitioning::UnknownPartitioning(n) => Err(PlanError::Unsupported(format!(
            "a repartition into {n} lanes by an unstated rule"
        ))),
    }
}

/// The shuffle, as both halves: nothing to merge below one lane, and a lane that
/// arrived as one batch per lane is worth re-coalescing so the scatter is one call
/// rather than one per lane's batch.
pub(crate) fn shuffled(input: Box<dyn GpuNode>, keys: Vec<u32>, n: usize) -> Box<dyn GpuNode> {
    let was_single_batch = batches(input.as_ref()) == BatchLayout::SingleBatch;
    let mut input = merged(input);
    if was_single_batch && batches(input.as_ref()) != BatchLayout::SingleBatch {
        input = Box::new(GpuCoalesceAllBatches::new(input));
    }
    Box::new(GpuEmitPartitions::new(input, keys, n))
}

/// A merge over one lane is not a no-op to be optimized away later — it is a node
/// this plan does not have, and the golden shows it as one.
pub(crate) fn merged(input: Box<dyn GpuNode>) -> Box<dyn GpuNode> {
    if lanes(input.as_ref()) > 1 {
        Box::new(GpuMergePartitions::new(input))
    } else {
        input
    }
}

/// An N-into-1 order-preserving merge: the per-lane sort below it stays a per-batch
/// sort, and this node is the accumulator, so no per-lane accumulator is emitted.
pub(crate) fn sort_preserving_merge(
    t: &Translator,
    merge: &SortPreservingMergeExec,
) -> Result<Box<dyn GpuNode>, PlanError> {
    let (input, keys) = match merge.input().as_any().downcast_ref::<SortExec>() {
        Some(sort) => {
            let sorted = per_batch_sort(t, sort)?;
            (sorted.node, sorted.keys)
        }
        None => {
            let input = node(t, merge.input())?;
            let keys = sort_key_ordinals(merge.expr(), &merge.input().schema())?;
            (input, keys)
        }
    };
    Ok(Box::new(GpuMergeSortedPartitions::new(
        input,
        keys,
        merge.fetch(),
    )))
}

pub(crate) fn hash_join(
    t: &Translator,
    join: &HashJoinExec,
) -> Result<Box<dyn GpuNode>, PlanError> {
    let has_filter = join.filter().is_some();
    let capability = capability(*join.join_type(), has_filter)?;

    let mut keys = Vec::with_capacity(join.on().len());
    for (left, right) in join.on() {
        // Each side's key is an ordinal into that side, so each is read against that
        // side's schema.
        keys.push((
            column_ordinal_of(left, &join.left().schema(), "join key")?,
            column_ordinal_of(right, &join.right().schema(), "join key")?,
        ));
    }

    // DataFusion collects its left input, so the build side is already left; what the
    // planner adds is the one batch per lane the join reads it as.
    let mut build = node(t, join.left())?;
    let mut probe = node(t, join.right())?;
    if !co_partitioned(build.as_ref(), probe.as_ref(), &keys) {
        // DataFusion broadcasts a small build side rather than hashing both, so the
        // sides are not co-located and this mode has no broadcast to do it with
        // (#140). One lane is what is left.
        build = merged(build);
        probe = merged(probe);
    }
    if batches(build.as_ref()) != BatchLayout::SingleBatch {
        build = Box::new(GpuCoalesceAllBatches::new(build));
    }
    if !capability.probe_streams && batches(probe.as_ref()) != BatchLayout::SingleBatch {
        probe = Box::new(GpuCoalesceAllBatches::new(probe));
    }

    let (filter, filter_columns) = match join.filter() {
        Some(filter) => (
            Some(translate_expr(filter.expression(), filter.schema())?),
            filter_column_map(filter)?,
        ),
        None => (None, Vec::new()),
    };
    Ok(Box::new(GpuHashJoin::new(
        build,
        probe,
        *join.join_type(),
        keys,
        filter,
        filter_columns,
        join.null_equals_null(),
        join.projection
            .as_ref()
            .map(|columns| columns.iter().map(|c| *c as u32).collect()),
        Schema::new(join.schema()),
    )))
}

/// Union branches are planned independently, so one column can arrive as a different
/// type per branch. The cast is a node rather than something the executor does behind
/// the plan's back (#41), and it is the same rule as everywhere else: every cast is
/// explicit.
pub(crate) fn branches<'a>(
    t: &Translator,
    inputs: impl Iterator<Item = &'a Arc<dyn ExecutionPlan>>,
    declared: &ArrowSchema,
) -> Result<Vec<Box<dyn GpuNode>>, PlanError> {
    let mut branches = Vec::new();
    for input in inputs {
        let translated = node(t, input)?;
        branches.push(cast_branch(translated, &input.schema(), declared));
    }
    Ok(branches)
}

pub(crate) fn cast_branch(
    branch: Box<dyn GpuNode>,
    schema: &ArrowSchema,
    declared: &ArrowSchema,
) -> Box<dyn GpuNode> {
    let differs = schema
        .fields()
        .iter()
        .zip(declared.fields().iter())
        .any(|(field, out)| field.data_type() != out.data_type());
    if !differs {
        return branch;
    }
    let exprs: Vec<NamedExpr> = schema
        .fields()
        .iter()
        .zip(declared.fields().iter())
        .enumerate()
        .map(|(index, (field, out))| {
            let column = Expr::column(index as u32, field.name());
            let expr = if field.data_type() == out.data_type() {
                column
            } else {
                Expr::Cast {
                    expr: Box::new(column),
                    target: out.data_type().clone(),
                }
            };
            NamedExpr::new(expr, out.name())
        })
        .collect();
    Box::new(GpuProject::new(
        branch,
        exprs,
        Schema::new(Arc::new(declared.clone())),
    ))
}

/// Sources are reached in one order, left to right, so the estimator's figures are
/// consumed in that order — the same order it indexed them in.
pub(crate) fn batching_for_source(t: &Translator) -> Batching {
    let seq = t.next_source.get();
    t.next_source.set(seq + 1);
    match (t.batching, t.source_targets.get(seq)) {
        (Batching::Sized { .. }, Some(target)) => Batching::Sized {
            target_batch_bytes: *target as usize,
        },
        (batching, _) => batching,
    }
}

/// The small-table rule: a source under the threshold plans one lane whatever the
/// target, and the nodes a one-lane region does not need are then never emitted. It
/// bites only while batching is on, which is what makes the threshold mean anything.
pub(crate) fn lanes_for(t: &Translator, survivors: &[RowGroupMeta], limit: Option<usize>) -> usize {
    // A limit DataFusion pushed into the scan is the whole answer wherever it erased
    // the limit node above it — `SELECT * FROM nation LIMIT 3` at tp4 plans as a bare
    // scan carrying limit=3. Every lane would honour it, so N lanes would return N
    // times the rows; one lane is what makes the loader's own limit the answer.
    if limit.is_some() {
        return 1;
    }
    let bytes: u64 = survivors.iter().map(|group| group.bytes).sum();
    match t.batching {
        // With one batch per lane there is nothing for a size threshold to size, so the
        // rule that drops a source to one lane has nothing to act on either.
        Batching::Off => t.target_partitions,
        _ if bytes < t.small_table_bytes => 1,
        _ => t.target_partitions,
    }
}

pub(crate) fn source(t: &Translator, parquet: &ParquetExec) -> Result<Box<dyn GpuNode>, PlanError> {
    let config = parquet.base_config();
    let scan = survivor_metadata(parquet)?;
    let lanes = lanes_for(t, &scan.groups, config.limit);
    let partition_groups = partition(&scan.groups, lanes, batching_for_source(t))?;

    let projection = match &config.projection {
        Some(columns) => columns.iter().map(|c| *c as u32).collect(),
        None => (0..config.file_schema.fields().len() as u32).collect(),
    };
    Ok(Box::new(GpuLoadParquet::new(
        parquet_table_name(parquet).unwrap_or_default(),
        projection,
        partition_groups,
        &scan,
        config.limit,
        Schema::new(parquet.schema()),
    )))
}

/// A sort becomes a per-batch sort plus the accumulator that makes the whole stream
/// ordered, and the `fetch` is replicated onto both: the top n of a union is the top
/// n of each part's top n, which is what keeps a top-N memory-bounded.
pub(crate) fn sort(t: &Translator, sort: &SortExec) -> Result<Box<dyn GpuNode>, PlanError> {
    let sorted = per_batch_sort(t, sort)?;
    Ok(Box::new(GpuAccumulateBatchesAndSort::new(
        sorted.node,
        sorted.keys,
        sorted.fetch,
    )))
}

/// The per-batch half of the decomposition, without deciding which accumulator goes
/// above it: one lane's stream sorted is an accumulate-and-sort, an N-into-1 is a
/// merge, and only the parent knows which it needs.
pub(crate) fn per_batch_sort(t: &Translator, sort: &SortExec) -> Result<PerBatchSort, PlanError> {
    let input = node(t, sort.input())?;
    let keys = sort_key_ordinals(sort.expr(), &sort.input().schema())?;
    let fetch = sort.fetch();
    Ok(PerBatchSort {
        node: Box::new(GpuSort::new(input, keys.clone(), fetch)),
        keys,
        fetch,
    })
}

/// Mid-plan, the interval is a real node over a one-lane stream. Its input is NOT
/// required to be one batch: requiring that would read the whole of a subquery's
/// table to answer for a hundred rows.
pub(crate) fn mid_plan_limit(
    t: &Translator,
    input: &Arc<dyn ExecutionPlan>,
    interval: RowInterval,
) -> Result<Box<dyn GpuNode>, PlanError> {
    let mut input = node(t, input)?;
    if lanes(input.as_ref()) > 1 {
        input = Box::new(GpuMergePartitions::new(input));
    }
    Ok(Box::new(GpuLimit::new(input, interval)))
}

/// A join's build side is always one batch, and the planner is what makes it one.
pub(crate) fn build_side(
    t: &Translator,
    plan: &Arc<dyn ExecutionPlan>,
) -> Result<Box<dyn GpuNode>, PlanError> {
    let mut build = merged(node(t, plan)?);
    if batches(build.as_ref()) != BatchLayout::SingleBatch {
        build = Box::new(GpuCoalesceAllBatches::new(build));
    }
    Ok(build)
}

pub(crate) fn nested_loop_join(
    t: &Translator,
    join: &NestedLoopJoinExec,
) -> Result<Box<dyn GpuNode>, PlanError> {
    use datafusion::common::JoinType;
    let join_type = match join.join_type() {
        JoinType::Inner => NestedLoopJoinType::Inner,
        JoinType::Left => NestedLoopJoinType::Left,
        other => {
            return Err(PlanError::Unsupported(format!(
                "nested-loop join type {other:?} — the executor rejects anything but \
                 Inner and Left (#160)"
            )));
        }
    };
    // DataFusion uses a nested-loop join with no predicate where the product itself is
    // what the query asked for — tpcds q9 pairs one-row aggregates that way — and that
    // is a cross join by any other name.
    let Some(filter) = join.filter() else {
        let build = build_side(t, join.left())?;
        let probe = node(t, join.right())?;
        return Ok(Box::new(GpuCrossJoin::new(
            build,
            probe,
            projected(join.projection()),
            Schema::new(join.schema()),
        )));
    };
    let build = build_side(t, join.left())?;
    let mut probe = merged(node(t, join.right())?);
    // A Left form emits its unmatched build rows in the same pass, so it cannot
    // stream: #136's finish trick accumulates keys and a predicate join has none.
    if join_type == NestedLoopJoinType::Left && batches(probe.as_ref()) != BatchLayout::SingleBatch
    {
        probe = Box::new(GpuCoalesceAllBatches::new(probe));
    }
    // The filter is written against a table of its own, so its ordinals travel with
    // the map that says which side each of them came from.
    let predicate = translate_expr(filter.expression(), filter.schema())?;
    let filter_columns = filter_column_map(filter)?;
    Ok(Box::new(GpuNestedLoopJoin::new(
        build,
        probe,
        join_type,
        predicate,
        filter_columns,
        projected(join.projection()),
        Schema::new(join.schema()),
    )))
}
