// Serialize a DataFusion GPU physical plan tree into a FlatBuffer.
//
// Walks the `ExecutionPlan` tree produced by GpuExecutionRule, extracts the
// inner DataFusion nodes (FilterExec, ProjectionExec, etc.), and writes the
// corresponding FlatBuffer plan via the generated `peacock::plan` types.

use std::sync::Arc;

use datafusion::arrow::datatypes::{DataType as ArrowDataType, SchemaRef};
use datafusion::common::ScalarValue as DfScalarValue;
use datafusion::datasource::physical_plan::ParquetExec;
use datafusion::physical_expr::expressions::{
    BinaryExpr, CastExpr, Column, IsNotNullExpr, IsNullExpr, Literal, NegativeExpr, NotExpr,
};
use datafusion::physical_plan::aggregates::{AggregateExec, AggregateMode as DfAggMode};
use datafusion::physical_plan::filter::FilterExec;
use datafusion::common::JoinType as DfJoinType;
use datafusion::physical_plan::joins::HashJoinExec;
use datafusion::physical_plan::projection::ProjectionExec;
use datafusion::physical_plan::sorts::sort::SortExec;
use datafusion::physical_plan::ExecutionPlan;
use datafusion::physical_plan::PhysicalExpr;
use flatbuffers::{FlatBufferBuilder, WIPOffset};

use crate::generated::gpu_plan_generated::peacock::plan as fb;
use crate::gpu_rule::{
    GpuAggregateExec, GpuCoalesceBatchesExec, GpuCoalescePartitionsExec, GpuFilterExec,
    GpuHashJoinExec, GpuProjectExec, GpuRepartitionExec, GpuScanExec, GpuSortExec,
    GpuSortPreservingMergeExec,
};

/// Serialize an entire GPU execution plan tree into a FlatBuffer byte vector.
///
/// Returns `Err` if the plan contains nodes that cannot be serialized (e.g.
/// unsupported expression types or plan nodes)
pub fn serialize_plan(plan: &Arc<dyn ExecutionPlan>) -> Result<Vec<u8>, String> {
    let mut builder = FlatBufferBuilder::with_capacity(4096);
    let root = serialize_plan_node(&mut builder, plan)?;
    let gpu_plan = fb::GpuPlan::create(&mut builder, &fb::GpuPlanArgs { root: Some(root) });
    builder.finish(gpu_plan, None);
    Ok(builder.finished_data().to_vec())
}

// ---------------------------------------------------------------------------
// Plan nodes
// ---------------------------------------------------------------------------

fn serialize_plan_node<'a>(
    b: &mut FlatBufferBuilder<'a>,
    plan: &Arc<dyn ExecutionPlan>,
) -> Result<WIPOffset<fb::PlanNode<'a>>, String> {
    let output_schema = serialize_schema(b, &plan.schema());

    let (node_type, node_offset) = if let Some(scan) = plan.as_any().downcast_ref::<GpuScanExec>()
    {
        serialize_gpu_scan(b, scan)?
    } else if plan.as_any().is::<GpuFilterExec>() {
        serialize_gpu_filter(b, plan)?
    } else if plan.as_any().is::<GpuProjectExec>() {
        serialize_gpu_project(b, plan)?
    } else if plan.as_any().is::<GpuAggregateExec>() {
        serialize_gpu_aggregate(b, plan)?
    } else if plan.as_any().is::<GpuHashJoinExec>() {
        serialize_gpu_hash_join(b, plan)?
    } else if plan.as_any().is::<GpuSortExec>() {
        serialize_gpu_sort(b, plan)?
    } else if plan.as_any().is::<GpuCoalesceBatchesExec>() {
        serialize_gpu_coalesce_batches(b, plan)?
    } else if plan.as_any().is::<GpuCoalescePartitionsExec>() {
        serialize_gpu_coalesce_partitions(b, plan)?
    } else if plan.as_any().is::<GpuRepartitionExec>() {
        serialize_gpu_repartition(b, plan)?
    } else if plan.as_any().is::<GpuSortPreservingMergeExec>() {
        serialize_gpu_sort_preserving_merge(b, plan)?
    } else {
        return Err(format!("unsupported plan node: {}", plan.name()));
    };

    Ok(fb::PlanNode::create(
        b,
        &fb::PlanNodeArgs {
            node_type,
            node: Some(node_offset),
            output_schema: Some(output_schema),
        },
    ))
}

// --- GpuScanExec ---

fn serialize_gpu_scan<'a>(
    b: &mut FlatBufferBuilder<'a>,
    scan: &GpuScanExec,
) -> Result<(fb::PlanNodeKind, WIPOffset<flatbuffers::UnionWIPOffset>), String> {
    // The inner plan is a ParquetExec.
    let inner = scan.inner();
    let parquet = inner
        .as_any()
        .downcast_ref::<ParquetExec>()
        .ok_or_else(|| "GpuScanExec inner is not ParquetExec".to_string())?;

    let config = parquet.base_config();

    // Collect file paths.
    let path_strings: Vec<String> = config
        .file_groups
        .iter()
        .flat_map(|group| {
            group
                .iter()
                .map(|pf| {
                    let loc = pf.object_meta.location.to_string();
                    if loc.starts_with('/') { loc } else { format!("/{loc}") }
                })
        })
        .collect();
    let paths: Vec<_> = path_strings.iter().map(|s| b.create_string(s)).collect();
    let file_paths = b.create_vector(&paths);

    let file_schema = serialize_schema(b, &config.file_schema);

    let projection = config.projection.as_ref().map(|proj| {
        let indices: Vec<u32> = proj.iter().map(|&i| i as u32).collect();
        b.create_vector(&indices)
    });

    let limit = config.limit.unwrap_or(0) as u64;

    let gpu_scan = fb::GpuScan::create(
        b,
        &fb::GpuScanArgs {
            file_paths: Some(file_paths),
            file_schema: Some(file_schema),
            projection,
            batch_size: scan.gpu_batch_size as u32,
            limit,
        },
    );

    Ok((fb::PlanNodeKind::GpuScan, gpu_scan.as_union_value()))
}

// --- GpuFilterExec ---

fn serialize_gpu_filter<'a>(
    b: &mut FlatBufferBuilder<'a>,
    plan: &Arc<dyn ExecutionPlan>,
) -> Result<(fb::PlanNodeKind, WIPOffset<flatbuffers::UnionWIPOffset>), String> {
    let gpu_filter = plan
        .as_any()
        .downcast_ref::<GpuFilterExec>()
        .unwrap();
    let filter = gpu_filter
        .inner()
        .as_any()
        .downcast_ref::<FilterExec>()
        .ok_or("GpuFilterExec inner is not FilterExec")?;

    let predicate = serialize_expr(b, filter.predicate())?;
    let input_plan = filter.input();
    let input = serialize_plan_node(b, input_plan)?;

    let node = fb::GpuFilter::create(
        b,
        &fb::GpuFilterArgs {
            predicate: Some(predicate),
            input: Some(input),
        },
    );
    Ok((fb::PlanNodeKind::GpuFilter, node.as_union_value()))
}

// --- GpuProjectExec ---

fn serialize_gpu_project<'a>(
    b: &mut FlatBufferBuilder<'a>,
    plan: &Arc<dyn ExecutionPlan>,
) -> Result<(fb::PlanNodeKind, WIPOffset<flatbuffers::UnionWIPOffset>), String> {
    let gpu_proj = plan
        .as_any()
        .downcast_ref::<GpuProjectExec>()
        .unwrap();
    let proj = gpu_proj
        .inner()
        .as_any()
        .downcast_ref::<ProjectionExec>()
        .ok_or("GpuProjectExec inner is not ProjectionExec")?;

    let mut exprs = Vec::new();
    let mut alias_offsets = Vec::new();
    for (expr, alias) in proj.expr() {
        exprs.push(serialize_expr(b, expr)?);
        alias_offsets.push(b.create_string(alias));
    }
    let exprs_vec = b.create_vector(&exprs);
    let aliases_vec = b.create_vector(&alias_offsets);

    let input = serialize_plan_node(b, proj.input())?;

    let node = fb::GpuProject::create(
        b,
        &fb::GpuProjectArgs {
            exprs: Some(exprs_vec),
            aliases: Some(aliases_vec),
            input: Some(input),
        },
    );
    Ok((fb::PlanNodeKind::GpuProject, node.as_union_value()))
}

// --- GpuAggregateExec ---

fn serialize_gpu_aggregate<'a>(
    b: &mut FlatBufferBuilder<'a>,
    plan: &Arc<dyn ExecutionPlan>,
) -> Result<(fb::PlanNodeKind, WIPOffset<flatbuffers::UnionWIPOffset>), String> {
    let gpu_agg = plan
        .as_any()
        .downcast_ref::<GpuAggregateExec>()
        .unwrap();
    let agg = gpu_agg
        .inner()
        .as_any()
        .downcast_ref::<AggregateExec>()
        .ok_or("GpuAggregateExec inner is not AggregateExec")?;

    let mode = match agg.mode() {
        DfAggMode::Partial => fb::AggregateMode::Partial,
        DfAggMode::Final => fb::AggregateMode::Final,
        DfAggMode::FinalPartitioned => fb::AggregateMode::FinalPartitioned,
        DfAggMode::Single => fb::AggregateMode::Single,
        DfAggMode::SinglePartitioned => fb::AggregateMode::SinglePartitioned,
    };

    let group_by = agg.group_expr();
    let mut group_exprs = Vec::new();
    let mut group_names = Vec::new();
    for (expr, name) in group_by.expr() {
        group_exprs.push(serialize_expr(b, expr)?);
        group_names.push(b.create_string(name));
    }
    let group_exprs_vec = b.create_vector(&group_exprs);
    let group_names_vec = b.create_vector(&group_names);

    let mut aggr_funcs = Vec::new();
    for aggr in agg.aggr_expr() {
        let func_name = b.create_string(aggr.fun().name());
        let alias = b.create_string(aggr.name());
        let mut arg_offsets = Vec::new();
        for arg in aggr.expressions() {
            arg_offsets.push(serialize_expr(b, &arg)?);
        }
        let args = b.create_vector(&arg_offsets);
        let func = fb::AggregateFuncNode::create(
            b,
            &fb::AggregateFuncNodeArgs {
                name: Some(func_name),
                args: Some(args),
                distinct: aggr.is_distinct(),
                alias: Some(alias),
            },
        );
        aggr_funcs.push(func);
    }
    let aggr_funcs_vec = b.create_vector(&aggr_funcs);

    let input = serialize_plan_node(b, agg.input())?;

    let node = fb::GpuAggregate::create(
        b,
        &fb::GpuAggregateArgs {
            mode,
            group_exprs: Some(group_exprs_vec),
            group_names: Some(group_names_vec),
            aggr_funcs: Some(aggr_funcs_vec),
            input: Some(input),
        },
    );
    Ok((fb::PlanNodeKind::GpuAggregate, node.as_union_value()))
}

// --- GpuHashJoinExec ---

fn serialize_gpu_hash_join<'a>(
    b: &mut FlatBufferBuilder<'a>,
    plan: &Arc<dyn ExecutionPlan>,
) -> Result<(fb::PlanNodeKind, WIPOffset<flatbuffers::UnionWIPOffset>), String> {
    let gpu_join = plan
        .as_any()
        .downcast_ref::<GpuHashJoinExec>()
        .unwrap();
    let join = gpu_join
        .inner()
        .as_any()
        .downcast_ref::<HashJoinExec>()
        .ok_or("GpuHashJoinExec inner is not HashJoinExec")?;

    let join_type = match join.join_type() {
        DfJoinType::Inner => fb::JoinType::Inner,
        DfJoinType::Left => fb::JoinType::Left,
        DfJoinType::Right => fb::JoinType::Right,
        DfJoinType::Full => fb::JoinType::Full,
        DfJoinType::LeftSemi => fb::JoinType::LeftSemi,
        DfJoinType::RightSemi => fb::JoinType::RightSemi,
        DfJoinType::LeftAnti => fb::JoinType::LeftAnti,
        DfJoinType::RightAnti => fb::JoinType::RightAnti,
        other => return Err(format!("unsupported join type: {other:?}")),
    };

    let mut keys = Vec::new();
    for (left_key, right_key) in join.on() {
        let left = serialize_expr(b, left_key)?;
        let right = serialize_expr(b, right_key)?;
        keys.push(fb::JoinKey::create(
            b,
            &fb::JoinKeyArgs {
                left: Some(left),
                right: Some(right),
            },
        ));
    }
    let keys_vec = b.create_vector(&keys);

    let filter = if let Some(jf) = join.filter() {
        Some(serialize_expr(b, jf.expression())?)
    } else {
        None
    };

    let left = serialize_plan_node(b, join.left())?;
    let right = serialize_plan_node(b, join.right())?;

    let projection = join.projection.as_ref().map(|proj| {
        let indices: Vec<u32> = proj.iter().map(|&i| i as u32).collect();
        b.create_vector(&indices)
    });

    let node = fb::GpuHashJoin::create(
        b,
        &fb::GpuHashJoinArgs {
            join_type,
            keys: Some(keys_vec),
            filter,
            left: Some(left),
            right: Some(right),
            projection,
        },
    );
    Ok((fb::PlanNodeKind::GpuHashJoin, node.as_union_value()))
}

// --- GpuSortExec ---

fn serialize_gpu_sort<'a>(
    b: &mut FlatBufferBuilder<'a>,
    plan: &Arc<dyn ExecutionPlan>,
) -> Result<(fb::PlanNodeKind, WIPOffset<flatbuffers::UnionWIPOffset>), String> {
    let gpu_sort = plan
        .as_any()
        .downcast_ref::<GpuSortExec>()
        .unwrap();
    let sort = gpu_sort
        .inner()
        .as_any()
        .downcast_ref::<SortExec>()
        .ok_or("GpuSortExec inner is not SortExec")?;

    let mut sort_exprs = Vec::new();
    for se in sort.expr().iter() {
        let expr = serialize_expr(b, &se.expr)?;
        sort_exprs.push(fb::SortExprNode::create(
            b,
            &fb::SortExprNodeArgs {
                expr: Some(expr),
                asc: !se.options.descending,
                nulls_first: se.options.nulls_first,
            },
        ));
    }
    let exprs_vec = b.create_vector(&sort_exprs);

    let fetch = sort.fetch().map(|f| f as i64).unwrap_or(-1);

    let input = serialize_plan_node(b, sort.input())?;

    let node = fb::GpuSort::create(
        b,
        &fb::GpuSortArgs {
            exprs: Some(exprs_vec),
            fetch,
            input: Some(input),
        },
    );
    Ok((fb::PlanNodeKind::GpuSort, node.as_union_value()))
}

// --- GpuCoalesceBatchesExec ---

fn serialize_gpu_coalesce_batches<'a>(
    b: &mut FlatBufferBuilder<'a>,
    plan: &Arc<dyn ExecutionPlan>,
) -> Result<(fb::PlanNodeKind, WIPOffset<flatbuffers::UnionWIPOffset>), String> {
    let gpu_cb = plan.as_any().downcast_ref::<GpuCoalesceBatchesExec>().unwrap();
    let cb = gpu_cb
        .inner()
        .as_any()
        .downcast_ref::<datafusion::physical_plan::coalesce_batches::CoalesceBatchesExec>()
        .ok_or("GpuCoalesceBatchesExec inner is not CoalesceBatchesExec")?;

    let input = serialize_plan_node(b, cb.input())?;
    let node = fb::GpuCoalesceBatches::create(
        b,
        &fb::GpuCoalesceBatchesArgs {
            target_batch_size: cb.target_batch_size() as u32,
            input: Some(input),
        },
    );
    Ok((fb::PlanNodeKind::GpuCoalesceBatches, node.as_union_value()))
}

// --- GpuCoalescePartitionsExec ---

fn serialize_gpu_coalesce_partitions<'a>(
    b: &mut FlatBufferBuilder<'a>,
    plan: &Arc<dyn ExecutionPlan>,
) -> Result<(fb::PlanNodeKind, WIPOffset<flatbuffers::UnionWIPOffset>), String> {
    let gpu_cp = plan.as_any().downcast_ref::<GpuCoalescePartitionsExec>().unwrap();
    let cp = gpu_cp
        .inner()
        .as_any()
        .downcast_ref::<datafusion::physical_plan::coalesce_partitions::CoalescePartitionsExec>()
        .ok_or("GpuCoalescePartitionsExec inner is not CoalescePartitionsExec")?;

    let input = serialize_plan_node(b, cp.input())?;
    let node = fb::GpuCoalescePartitions::create(
        b,
        &fb::GpuCoalescePartitionsArgs {
            input: Some(input),
        },
    );
    Ok((fb::PlanNodeKind::GpuCoalescePartitions, node.as_union_value()))
}

// --- GpuRepartitionExec ---

fn serialize_gpu_repartition<'a>(
    b: &mut FlatBufferBuilder<'a>,
    plan: &Arc<dyn ExecutionPlan>,
) -> Result<(fb::PlanNodeKind, WIPOffset<flatbuffers::UnionWIPOffset>), String> {
    use datafusion::physical_plan::repartition::RepartitionExec;
    use datafusion::physical_plan::Partitioning;

    let gpu_rp = plan.as_any().downcast_ref::<GpuRepartitionExec>().unwrap();
    let rp = gpu_rp
        .inner()
        .as_any()
        .downcast_ref::<RepartitionExec>()
        .ok_or("GpuRepartitionExec inner is not RepartitionExec")?;

    let input = serialize_plan_node(b, rp.input())?;

    let (kind, num_partitions, hash_exprs) = match rp.partitioning() {
        Partitioning::RoundRobinBatch(n) => (fb::PartitioningKind::RoundRobinBatch, *n, None),
        Partitioning::Hash(exprs, n) => {
            let mut expr_offsets = Vec::new();
            for expr in exprs {
                expr_offsets.push(serialize_expr(b, expr)?);
            }
            let exprs_vec = b.create_vector(&expr_offsets);
            (fb::PartitioningKind::Hash, *n, Some(exprs_vec))
        }
        Partitioning::UnknownPartitioning(n) => (fb::PartitioningKind::Unknown, *n, None),
    };

    let node = fb::GpuRepartition::create(
        b,
        &fb::GpuRepartitionArgs {
            kind,
            num_partitions: num_partitions as u32,
            hash_exprs,
            input: Some(input),
        },
    );
    Ok((fb::PlanNodeKind::GpuRepartition, node.as_union_value()))
}

// --- GpuSortPreservingMergeExec ---

fn serialize_gpu_sort_preserving_merge<'a>(
    b: &mut FlatBufferBuilder<'a>,
    plan: &Arc<dyn ExecutionPlan>,
) -> Result<(fb::PlanNodeKind, WIPOffset<flatbuffers::UnionWIPOffset>), String> {
    use datafusion::physical_plan::sorts::sort_preserving_merge::SortPreservingMergeExec;

    let gpu_spm = plan.as_any().downcast_ref::<GpuSortPreservingMergeExec>().unwrap();
    let spm = gpu_spm
        .inner()
        .as_any()
        .downcast_ref::<SortPreservingMergeExec>()
        .ok_or("GpuSortPreservingMergeExec inner is not SortPreservingMergeExec")?;

    let input = serialize_plan_node(b, spm.input())?;

    let mut sort_exprs = Vec::new();
    for se in spm.expr().iter() {
        let expr = serialize_expr(b, &se.expr)?;
        sort_exprs.push(fb::SortExprNode::create(
            b,
            &fb::SortExprNodeArgs {
                expr: Some(expr),
                asc: !se.options.descending,
                nulls_first: se.options.nulls_first,
            },
        ));
    }
    let exprs_vec = b.create_vector(&sort_exprs);

    let fetch = spm.fetch().map(|f| f as i64).unwrap_or(-1);

    let node = fb::GpuSortPreservingMerge::create(
        b,
        &fb::GpuSortPreservingMergeArgs {
            exprs: Some(exprs_vec),
            fetch,
            input: Some(input),
        },
    );
    Ok((fb::PlanNodeKind::GpuSortPreservingMerge, node.as_union_value()))
}

// ---------------------------------------------------------------------------
// Expressions
// ---------------------------------------------------------------------------

fn serialize_expr<'a>(
    b: &mut FlatBufferBuilder<'a>,
    expr: &Arc<dyn PhysicalExpr>,
) -> Result<WIPOffset<fb::Expr<'a>>, String> {
    let any = expr.as_any();

    let (node_type, node_offset) = if let Some(col) = any.downcast_ref::<Column>() {
        let name = b.create_string(col.name());
        let cr = fb::ColumnRef::create(
            b,
            &fb::ColumnRefArgs {
                index: col.index() as u32,
                name: Some(name),
            },
        );
        (fb::ExprNode::ColumnRef, cr.as_union_value())
    } else if let Some(lit) = any.downcast_ref::<Literal>() {
        let sv = serialize_scalar_value(b, lit.value())?;
        let le = fb::LiteralExpr::create(b, &fb::LiteralExprArgs { value: Some(sv) });
        (fb::ExprNode::LiteralExpr, le.as_union_value())
    } else if let Some(bin) = any.downcast_ref::<BinaryExpr>() {
        let left = serialize_expr(b, bin.left())?;
        let right = serialize_expr(b, bin.right())?;
        let op = convert_operator(bin.op())?;
        let be = fb::BinaryExprNode::create(
            b,
            &fb::BinaryExprNodeArgs {
                left: Some(left),
                op,
                right: Some(right),
            },
        );
        (fb::ExprNode::BinaryExprNode, be.as_union_value())
    } else if let Some(not) = any.downcast_ref::<NotExpr>() {
        let arg = serialize_expr(b, not.arg())?;
        let ue = fb::UnaryExprNode::create(
            b,
            &fb::UnaryExprNodeArgs {
                op: fb::UnaryOp::Not,
                arg: Some(arg),
            },
        );
        (fb::ExprNode::UnaryExprNode, ue.as_union_value())
    } else if let Some(is_null) = any.downcast_ref::<IsNullExpr>() {
        let arg = serialize_expr(b, is_null.arg())?;
        let ue = fb::UnaryExprNode::create(
            b,
            &fb::UnaryExprNodeArgs {
                op: fb::UnaryOp::IsNull,
                arg: Some(arg),
            },
        );
        (fb::ExprNode::UnaryExprNode, ue.as_union_value())
    } else if let Some(is_not_null) = any.downcast_ref::<IsNotNullExpr>() {
        let arg = serialize_expr(b, is_not_null.arg())?;
        let ue = fb::UnaryExprNode::create(
            b,
            &fb::UnaryExprNodeArgs {
                op: fb::UnaryOp::IsNotNull,
                arg: Some(arg),
            },
        );
        (fb::ExprNode::UnaryExprNode, ue.as_union_value())
    } else if let Some(neg) = any.downcast_ref::<NegativeExpr>() {
        let arg = serialize_expr(b, neg.arg())?;
        let ue = fb::UnaryExprNode::create(
            b,
            &fb::UnaryExprNodeArgs {
                op: fb::UnaryOp::Negative,
                arg: Some(arg),
            },
        );
        (fb::ExprNode::UnaryExprNode, ue.as_union_value())
    } else if let Some(cast) = any.downcast_ref::<CastExpr>() {
        let inner = serialize_expr(b, cast.expr())?;
        let target = convert_data_type(cast.cast_type())?;
        let ce = fb::CastExprNode::create(
            b,
            &fb::CastExprNodeArgs {
                expr: Some(inner),
                target_type: target,
            },
        );
        (fb::ExprNode::CastExprNode, ce.as_union_value())
    } else {
        return Err(format!(
            "unsupported physical expression: {}",
            expr
        ));
    };

    Ok(fb::Expr::create(
        b,
        &fb::ExprArgs {
            node_type,
            node: Some(node_offset),
        },
    ))
}

// ---------------------------------------------------------------------------
// Scalar values
// ---------------------------------------------------------------------------

fn serialize_scalar_value<'a>(
    b: &mut FlatBufferBuilder<'a>,
    sv: &DfScalarValue,
) -> Result<WIPOffset<fb::ScalarValue<'a>>, String> {
    let mut args = fb::ScalarValueArgs::default();

    match sv {
        DfScalarValue::Null => {
            args.type_ = fb::DataType::Null;
        }
        DfScalarValue::Boolean(Some(v)) => {
            args.type_ = fb::DataType::Boolean;
            args.bool_val = *v;
        }
        DfScalarValue::Int8(Some(v)) => {
            args.type_ = fb::DataType::Int8;
            args.int_val = *v as i64;
        }
        DfScalarValue::Int16(Some(v)) => {
            args.type_ = fb::DataType::Int16;
            args.int_val = *v as i64;
        }
        DfScalarValue::Int32(Some(v)) => {
            args.type_ = fb::DataType::Int32;
            args.int_val = *v as i64;
        }
        DfScalarValue::Int64(Some(v)) => {
            args.type_ = fb::DataType::Int64;
            args.int_val = *v;
        }
        DfScalarValue::UInt8(Some(v)) => {
            args.type_ = fb::DataType::UInt8;
            args.uint_val = *v as u64;
        }
        DfScalarValue::UInt16(Some(v)) => {
            args.type_ = fb::DataType::UInt16;
            args.uint_val = *v as u64;
        }
        DfScalarValue::UInt32(Some(v)) => {
            args.type_ = fb::DataType::UInt32;
            args.uint_val = *v as u64;
        }
        DfScalarValue::UInt64(Some(v)) => {
            args.type_ = fb::DataType::UInt64;
            args.uint_val = *v;
        }
        DfScalarValue::Float32(Some(v)) => {
            args.type_ = fb::DataType::Float32;
            args.float_val = *v as f64;
        }
        DfScalarValue::Float64(Some(v)) => {
            args.type_ = fb::DataType::Float64;
            args.float_val = *v;
        }
        DfScalarValue::Utf8(Some(s)) | DfScalarValue::LargeUtf8(Some(s)) => {
            args.type_ = fb::DataType::Utf8;
            args.string_val = Some(b.create_string(s));
        }
        DfScalarValue::Decimal128(Some(v), prec, scale) => {
            args.type_ = fb::DataType::Decimal128;
            args.decimal_hi = (*v >> 64) as i64;
            args.decimal_lo = *v as u64;
            args.decimal_precision = *prec;
            args.decimal_scale = *scale as i8;
        }
        // Treat any None variant as typed null.
        other if other.is_null() => {
            args.type_ = convert_data_type(&other.data_type())?;
        }
        other => {
            return Err(format!("unsupported scalar value: {other:?}"));
        }
    }

    Ok(fb::ScalarValue::create(b, &args))
}

// ---------------------------------------------------------------------------
// Arrow type / operator conversions
// ---------------------------------------------------------------------------

fn convert_data_type(dt: &ArrowDataType) -> Result<fb::DataType, String> {
    Ok(match dt {
        ArrowDataType::Null => fb::DataType::Null,
        ArrowDataType::Boolean => fb::DataType::Boolean,
        ArrowDataType::Int8 => fb::DataType::Int8,
        ArrowDataType::Int16 => fb::DataType::Int16,
        ArrowDataType::Int32 => fb::DataType::Int32,
        ArrowDataType::Int64 => fb::DataType::Int64,
        ArrowDataType::UInt8 => fb::DataType::UInt8,
        ArrowDataType::UInt16 => fb::DataType::UInt16,
        ArrowDataType::UInt32 => fb::DataType::UInt32,
        ArrowDataType::UInt64 => fb::DataType::UInt64,
        ArrowDataType::Float16 => fb::DataType::Float16,
        ArrowDataType::Float32 => fb::DataType::Float32,
        ArrowDataType::Float64 => fb::DataType::Float64,
        ArrowDataType::Utf8 => fb::DataType::Utf8,
        ArrowDataType::LargeUtf8 => fb::DataType::LargeUtf8,
        ArrowDataType::Binary => fb::DataType::Binary,
        ArrowDataType::LargeBinary => fb::DataType::LargeBinary,
        ArrowDataType::Date32 => fb::DataType::Date32,
        ArrowDataType::Date64 => fb::DataType::Date64,
        ArrowDataType::Decimal128(_, _) => fb::DataType::Decimal128,
        ArrowDataType::Utf8View => fb::DataType::Utf8View,
        ArrowDataType::BinaryView => fb::DataType::BinaryView,
        other => return Err(format!("unsupported Arrow data type: {other:?}")),
    })
}

fn convert_operator(
    op: &datafusion::logical_expr::Operator,
) -> Result<fb::BinaryOp, String> {
    use datafusion::logical_expr::Operator as Op;
    Ok(match op {
        Op::Eq => fb::BinaryOp::Eq,
        Op::NotEq => fb::BinaryOp::NotEq,
        Op::Lt => fb::BinaryOp::Lt,
        Op::LtEq => fb::BinaryOp::LtEq,
        Op::Gt => fb::BinaryOp::Gt,
        Op::GtEq => fb::BinaryOp::GtEq,
        Op::Plus => fb::BinaryOp::Plus,
        Op::Minus => fb::BinaryOp::Minus,
        Op::Multiply => fb::BinaryOp::Multiply,
        Op::Divide => fb::BinaryOp::Divide,
        Op::Modulo => fb::BinaryOp::Modulo,
        Op::And => fb::BinaryOp::And,
        Op::Or => fb::BinaryOp::Or,
        Op::BitwiseAnd => fb::BinaryOp::BitwiseAnd,
        Op::BitwiseOr => fb::BinaryOp::BitwiseOr,
        Op::BitwiseXor => fb::BinaryOp::BitwiseXor,
        Op::BitwiseShiftLeft => fb::BinaryOp::BitwiseShiftLeft,
        Op::BitwiseShiftRight => fb::BinaryOp::BitwiseShiftRight,
        Op::StringConcat => fb::BinaryOp::StringConcat,
        Op::IsDistinctFrom => fb::BinaryOp::IsDistinctFrom,
        Op::IsNotDistinctFrom => fb::BinaryOp::IsNotDistinctFrom,
        other => return Err(format!("unsupported binary operator: {other:?}")),
    })
}

// ---------------------------------------------------------------------------
// Schema serialization
// ---------------------------------------------------------------------------

fn serialize_schema<'a>(
    b: &mut FlatBufferBuilder<'a>,
    schema: &SchemaRef,
) -> WIPOffset<fb::Schema<'a>> {
    let fields: Vec<_> = schema
        .fields()
        .iter()
        .map(|f| {
            let name = b.create_string(f.name());
            let dt = convert_data_type(f.data_type()).unwrap_or(fb::DataType::Null);
            fb::Field::create(
                b,
                &fb::FieldArgs {
                    name: Some(name),
                    data_type: dt,
                    nullable: f.is_nullable(),
                },
            )
        })
        .collect();
    let fields_vec = b.create_vector(&fields);
    fb::Schema::create(b, &fb::SchemaArgs { fields: Some(fields_vec) })
}

// ---------------------------------------------------------------------------
// Deserialization: FlatBuffer → ExecutionPlan
// ---------------------------------------------------------------------------

/// Deserialize a FlatBuffer byte buffer into an `ExecutionPlan` tree.
///
/// The reconstructed plan uses the same GPU exec node types
/// (`GpuScanExec`, `GpuFilterExec`, etc.) wrapping real DataFusion nodes
/// built from the serialized expressions and schemas. Pass-through CPU nodes
/// (CoalesceBatches, Repartition, etc.) are not present in the flatbuffer
/// and are therefore not reconstructed.
pub fn deserialize_plan(bytes: &[u8]) -> Result<Arc<dyn ExecutionPlan>, String> {
    let gpu_plan = flatbuffers::root::<fb::GpuPlan>(bytes)
        .map_err(|e| format!("invalid FlatBuffer: {e}"))?;
    let root = gpu_plan
        .root()
        .ok_or("GpuPlan has no root node")?;
    deserialize_plan_node(&root)
}

fn deserialize_plan_node(node: &fb::PlanNode) -> Result<Arc<dyn ExecutionPlan>, String> {
    match node.node_type() {
        fb::PlanNodeKind::GpuScan => {
            let scan = node.node_as_gpu_scan().ok_or("expected GpuScan")?;
            deserialize_gpu_scan(&scan, node)
        }
        fb::PlanNodeKind::GpuFilter => {
            let filter = node.node_as_gpu_filter().ok_or("expected GpuFilter")?;
            deserialize_gpu_filter(&filter, node)
        }
        fb::PlanNodeKind::GpuProject => {
            let proj = node.node_as_gpu_project().ok_or("expected GpuProject")?;
            deserialize_gpu_project(&proj, node)
        }
        fb::PlanNodeKind::GpuAggregate => {
            let agg = node.node_as_gpu_aggregate().ok_or("expected GpuAggregate")?;
            deserialize_gpu_aggregate(&agg, node)
        }
        fb::PlanNodeKind::GpuHashJoin => {
            let join = node.node_as_gpu_hash_join().ok_or("expected GpuHashJoin")?;
            deserialize_gpu_hash_join(&join, node)
        }
        fb::PlanNodeKind::GpuSort => {
            let sort = node.node_as_gpu_sort().ok_or("expected GpuSort")?;
            deserialize_gpu_sort(&sort, node)
        }
        fb::PlanNodeKind::GpuCoalesceBatches => {
            let cb = node.node_as_gpu_coalesce_batches().ok_or("expected GpuCoalesceBatches")?;
            deserialize_gpu_coalesce_batches(&cb)
        }
        fb::PlanNodeKind::GpuCoalescePartitions => {
            let cp = node.node_as_gpu_coalesce_partitions().ok_or("expected GpuCoalescePartitions")?;
            deserialize_gpu_coalesce_partitions(&cp)
        }
        fb::PlanNodeKind::GpuRepartition => {
            let rp = node.node_as_gpu_repartition().ok_or("expected GpuRepartition")?;
            deserialize_gpu_repartition(&rp)
        }
        fb::PlanNodeKind::GpuSortPreservingMerge => {
            let spm = node.node_as_gpu_sort_preserving_merge().ok_or("expected GpuSortPreservingMerge")?;
            deserialize_gpu_sort_preserving_merge(&spm)
        }
        other => Err(format!("unknown PlanNodeKind: {:?}", other)),
    }
}

fn deserialize_schema(schema: &fb::Schema) -> SchemaRef {
    let fields: Vec<datafusion::arrow::datatypes::Field> = schema
        .fields()
        .map(|v| {
            (0..v.len())
                .map(|i| {
                    let f = v.get(i);
                    datafusion::arrow::datatypes::Field::new(
                        f.name().unwrap_or(""),
                        fb_to_arrow_type(f.data_type()),
                        f.nullable(),
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    Arc::new(datafusion::arrow::datatypes::Schema::new(fields))
}

fn fb_to_arrow_type(dt: fb::DataType) -> ArrowDataType {
    match dt {
        fb::DataType::Null => ArrowDataType::Null,
        fb::DataType::Boolean => ArrowDataType::Boolean,
        fb::DataType::Int8 => ArrowDataType::Int8,
        fb::DataType::Int16 => ArrowDataType::Int16,
        fb::DataType::Int32 => ArrowDataType::Int32,
        fb::DataType::Int64 => ArrowDataType::Int64,
        fb::DataType::UInt8 => ArrowDataType::UInt8,
        fb::DataType::UInt16 => ArrowDataType::UInt16,
        fb::DataType::UInt32 => ArrowDataType::UInt32,
        fb::DataType::UInt64 => ArrowDataType::UInt64,
        fb::DataType::Float16 => ArrowDataType::Float16,
        fb::DataType::Float32 => ArrowDataType::Float32,
        fb::DataType::Float64 => ArrowDataType::Float64,
        fb::DataType::Utf8 => ArrowDataType::Utf8,
        fb::DataType::LargeUtf8 => ArrowDataType::LargeUtf8,
        fb::DataType::Binary => ArrowDataType::Binary,
        fb::DataType::LargeBinary => ArrowDataType::LargeBinary,
        fb::DataType::Date32 => ArrowDataType::Date32,
        fb::DataType::Date64 => ArrowDataType::Date64,
        fb::DataType::Decimal128 => ArrowDataType::Decimal128(38, 10),
        fb::DataType::Utf8View => ArrowDataType::Utf8View,
        fb::DataType::BinaryView => ArrowDataType::BinaryView,
        _ => ArrowDataType::Null,
    }
}

fn deserialize_expr(expr: &fb::Expr) -> Result<Arc<dyn PhysicalExpr>, String> {
    match expr.node_type() {
        fb::ExprNode::ColumnRef => {
            let col = expr.node_as_column_ref().ok_or("expected ColumnRef")?;
            Ok(Arc::new(Column::new(
                col.name().unwrap_or(""),
                col.index() as usize,
            )))
        }
        fb::ExprNode::LiteralExpr => {
            let lit = expr.node_as_literal_expr().ok_or("expected LiteralExpr")?;
            let sv = lit.value().ok_or("LiteralExpr has no value")?;
            Ok(Arc::new(Literal::new(deserialize_scalar(&sv)?)))
        }
        fb::ExprNode::BinaryExprNode => {
            let bin = expr
                .node_as_binary_expr_node()
                .ok_or("expected BinaryExprNode")?;
            let left = deserialize_expr(&bin.left().ok_or("BinaryExpr missing left")?)?;
            let right = deserialize_expr(&bin.right().ok_or("BinaryExpr missing right")?)?;
            let op = fb_to_operator(bin.op())?;
            Ok(Arc::new(BinaryExpr::new(left, op, right)))
        }
        fb::ExprNode::UnaryExprNode => {
            let un = expr
                .node_as_unary_expr_node()
                .ok_or("expected UnaryExprNode")?;
            let arg = deserialize_expr(&un.arg().ok_or("UnaryExpr missing arg")?)?;
            match un.op() {
                fb::UnaryOp::Not => Ok(Arc::new(NotExpr::new(arg))),
                fb::UnaryOp::IsNull => Ok(Arc::new(IsNullExpr::new(arg))),
                fb::UnaryOp::IsNotNull => Ok(Arc::new(IsNotNullExpr::new(arg))),
                fb::UnaryOp::Negative => Ok(Arc::new(NegativeExpr::new(arg))),
                other => Err(format!("unsupported UnaryOp: {:?}", other)),
            }
        }
        fb::ExprNode::CastExprNode => {
            let cast = expr
                .node_as_cast_expr_node()
                .ok_or("expected CastExprNode")?;
            let inner = deserialize_expr(&cast.expr().ok_or("CastExpr missing expr")?)?;
            let target = fb_to_arrow_type(cast.target_type());
            Ok(Arc::new(CastExpr::new(inner, target, None)))
        }
        other => Err(format!("unsupported ExprNode type: {:?}", other)),
    }
}

fn deserialize_scalar(sv: &fb::ScalarValue) -> Result<DfScalarValue, String> {
    Ok(match sv.type_() {
        fb::DataType::Null => DfScalarValue::Null,
        fb::DataType::Boolean => DfScalarValue::Boolean(Some(sv.bool_val())),
        fb::DataType::Int8 => DfScalarValue::Int8(Some(sv.int_val() as i8)),
        fb::DataType::Int16 => DfScalarValue::Int16(Some(sv.int_val() as i16)),
        fb::DataType::Int32 => DfScalarValue::Int32(Some(sv.int_val() as i32)),
        fb::DataType::Int64 => DfScalarValue::Int64(Some(sv.int_val())),
        fb::DataType::UInt8 => DfScalarValue::UInt8(Some(sv.uint_val() as u8)),
        fb::DataType::UInt16 => DfScalarValue::UInt16(Some(sv.uint_val() as u16)),
        fb::DataType::UInt32 => DfScalarValue::UInt32(Some(sv.uint_val() as u32)),
        fb::DataType::UInt64 => DfScalarValue::UInt64(Some(sv.uint_val())),
        fb::DataType::Float32 => DfScalarValue::Float32(Some(sv.float_val() as f32)),
        fb::DataType::Float64 => DfScalarValue::Float64(Some(sv.float_val())),
        fb::DataType::Utf8 => {
            DfScalarValue::Utf8(Some(sv.string_val().unwrap_or("").to_string()))
        }
        fb::DataType::LargeUtf8 => {
            DfScalarValue::LargeUtf8(Some(sv.string_val().unwrap_or("").to_string()))
        }
        fb::DataType::Decimal128 => {
            let hi = sv.decimal_hi() as i128;
            let lo = sv.decimal_lo() as i128;
            let val = (hi << 64) | (lo & 0xFFFF_FFFF_FFFF_FFFF);
            DfScalarValue::Decimal128(Some(val), sv.decimal_precision(), sv.decimal_scale() as i8)
        }
        other => return Err(format!("unsupported scalar DataType: {:?}", other)),
    })
}

fn fb_to_operator(op: fb::BinaryOp) -> Result<datafusion::logical_expr::Operator, String> {
    use datafusion::logical_expr::Operator as Op;
    Ok(match op {
        fb::BinaryOp::Eq => Op::Eq,
        fb::BinaryOp::NotEq => Op::NotEq,
        fb::BinaryOp::Lt => Op::Lt,
        fb::BinaryOp::LtEq => Op::LtEq,
        fb::BinaryOp::Gt => Op::Gt,
        fb::BinaryOp::GtEq => Op::GtEq,
        fb::BinaryOp::Plus => Op::Plus,
        fb::BinaryOp::Minus => Op::Minus,
        fb::BinaryOp::Multiply => Op::Multiply,
        fb::BinaryOp::Divide => Op::Divide,
        fb::BinaryOp::Modulo => Op::Modulo,
        fb::BinaryOp::And => Op::And,
        fb::BinaryOp::Or => Op::Or,
        fb::BinaryOp::BitwiseAnd => Op::BitwiseAnd,
        fb::BinaryOp::BitwiseOr => Op::BitwiseOr,
        fb::BinaryOp::BitwiseXor => Op::BitwiseXor,
        fb::BinaryOp::BitwiseShiftLeft => Op::BitwiseShiftLeft,
        fb::BinaryOp::BitwiseShiftRight => Op::BitwiseShiftRight,
        fb::BinaryOp::StringConcat => Op::StringConcat,
        fb::BinaryOp::IsDistinctFrom => Op::IsDistinctFrom,
        fb::BinaryOp::IsNotDistinctFrom => Op::IsNotDistinctFrom,
        other => return Err(format!("unsupported BinaryOp: {:?}", other)),
    })
}

// --- Plan node deserialization ---

use datafusion::datasource::physical_plan::FileScanConfig;
use datafusion::datasource::listing::PartitionedFile;
use datafusion::physical_expr::aggregate::AggregateExprBuilder;
use datafusion::physical_plan::aggregates::PhysicalGroupBy;
use datafusion::physical_expr::PhysicalSortExpr;
use datafusion::arrow::compute::SortOptions;

fn deserialize_gpu_scan(
    scan: &fb::GpuScan,
    node: &fb::PlanNode,
) -> Result<Arc<dyn ExecutionPlan>, String> {
    let file_schema = node
        .output_schema()
        .map(|s| deserialize_schema(&s))
        .unwrap_or_else(|| {
            scan.file_schema()
                .map(|s| deserialize_schema(&s))
                .unwrap_or_else(|| Arc::new(datafusion::arrow::datatypes::Schema::empty()))
        });

    let full_schema = scan
        .file_schema()
        .map(|s| deserialize_schema(&s))
        .unwrap_or_else(|| file_schema.clone());

    let file_groups: Vec<Vec<PartitionedFile>> = scan
        .file_paths()
        .map(|v| {
            (0..v.len())
                .map(|i| {
                    let path = v.get(i);
                    vec![PartitionedFile::new(path.to_string(), 0)]
                })
                .collect()
        })
        .unwrap_or_default();

    let projection = scan.projection().map(|v| {
        (0..v.len()).map(|i| v.get(i) as usize).collect::<Vec<_>>()
    });

    let limit = if scan.limit() > 0 {
        Some(scan.limit() as usize)
    } else {
        None
    };

    let config = FileScanConfig::new(
        datafusion::execution::object_store::ObjectStoreUrl::local_filesystem(),
        full_schema,
    )
    .with_file_groups(file_groups)
    .with_projection(projection)
    .with_limit(limit);

    let parquet = ParquetExec::builder(config).build_arc();

    Ok(Arc::new(GpuScanExec::new(parquet, scan.batch_size() as usize)))
}

fn deserialize_gpu_filter(
    filter: &fb::GpuFilter,
    _node: &fb::PlanNode,
) -> Result<Arc<dyn ExecutionPlan>, String> {
    let input = deserialize_plan_node(&filter.input().ok_or("GpuFilter missing input")?)?;
    let predicate = deserialize_expr(&filter.predicate().ok_or("GpuFilter missing predicate")?)?;
    let filter_exec =
        FilterExec::try_new(predicate, input).map_err(|e| format!("FilterExec: {e}"))?;
    Ok(Arc::new(GpuFilterExec::new(Arc::new(filter_exec))))
}

fn deserialize_gpu_project(
    proj: &fb::GpuProject,
    _node: &fb::PlanNode,
) -> Result<Arc<dyn ExecutionPlan>, String> {
    let input = deserialize_plan_node(&proj.input().ok_or("GpuProject missing input")?)?;
    let exprs_fb = proj.exprs().ok_or("GpuProject missing exprs")?;
    let aliases_fb = proj.aliases().ok_or("GpuProject missing aliases")?;

    let expr_pairs: Vec<(Arc<dyn PhysicalExpr>, String)> = (0..exprs_fb.len())
        .map(|i| {
            let expr = deserialize_expr(&exprs_fb.get(i))?;
            let alias = aliases_fb.get(i).to_string();
            Ok((expr, alias))
        })
        .collect::<Result<_, String>>()?;

    let proj_exec =
        ProjectionExec::try_new(expr_pairs, input).map_err(|e| format!("ProjectionExec: {e}"))?;
    Ok(Arc::new(GpuProjectExec::new(Arc::new(proj_exec))))
}

fn deserialize_gpu_aggregate(
    agg: &fb::GpuAggregate,
    _node: &fb::PlanNode,
) -> Result<Arc<dyn ExecutionPlan>, String> {
    let input = deserialize_plan_node(&agg.input().ok_or("GpuAggregate missing input")?)?;

    let mode = match agg.mode() {
        fb::AggregateMode::Partial => DfAggMode::Partial,
        fb::AggregateMode::Final => DfAggMode::Final,
        fb::AggregateMode::FinalPartitioned => DfAggMode::FinalPartitioned,
        fb::AggregateMode::Single => DfAggMode::Single,
        fb::AggregateMode::SinglePartitioned => DfAggMode::SinglePartitioned,
        other => return Err(format!("unsupported AggregateMode: {:?}", other)),
    };

    // Reconstruct group-by expressions.
    let group_exprs: Vec<(Arc<dyn PhysicalExpr>, String)> = agg
        .group_exprs()
        .zip(agg.group_names())
        .map(|(exprs, names)| {
            (0..exprs.len())
                .map(|i| {
                    let expr = deserialize_expr(&exprs.get(i))?;
                    let name = names.get(i).to_string();
                    Ok((expr, name))
                })
                .collect::<Result<Vec<_>, String>>()
        })
        .transpose()?
        .unwrap_or_default();

    let group_by = PhysicalGroupBy::new_single(group_exprs);

    // Reconstruct aggregate function expressions.
    let input_schema = input.schema();
    let aggr_exprs: Vec<Arc<datafusion::physical_expr::aggregate::AggregateFunctionExpr>> = agg
        .aggr_funcs()
        .map(|funcs| {
            (0..funcs.len())
                .map(|i| {
                    let f = funcs.get(i);
                    let name = f.name().unwrap_or("count");

                    // Reconstruct args.
                    let args: Vec<Arc<dyn PhysicalExpr>> = f
                        .args()
                        .map(|a| {
                            (0..a.len())
                                .map(|j| deserialize_expr(&a.get(j)))
                                .collect::<Result<Vec<_>, _>>()
                        })
                        .transpose()?
                        .unwrap_or_default();

                    // Look up the aggregate UDF by name.
                    let udf = datafusion::functions_aggregate::all_default_aggregate_functions()
                        .into_iter()
                        .find(|u| u.name() == name)
                        .ok_or_else(|| format!("unknown aggregate function: {name}"))?;

                    let alias = f.alias().unwrap_or(f.name().unwrap_or("?"));
                    let mut builder = AggregateExprBuilder::new(udf, args)
                        .schema(input_schema.clone())
                        .alias(alias);

                    if f.distinct() {
                        builder = builder.distinct();
                    }

                    builder.build()
                        .map(|e| Arc::new(e))
                        .map_err(|e| format!("AggregateExprBuilder error: {e}"))
                })
                .collect::<Result<Vec<_>, String>>()
        })
        .transpose()?
        .unwrap_or_default();

    let agg_exec = AggregateExec::try_new(
        mode,
        group_by,
        aggr_exprs,
        vec![None; agg.aggr_funcs().map(|f| f.len()).unwrap_or(0)], // no per-aggregate filters
        input,
        input_schema,
    )
    .map_err(|e| format!("AggregateExec: {e}"))?;

    Ok(Arc::new(GpuAggregateExec::new(Arc::new(agg_exec))))
}

fn deserialize_gpu_hash_join(
    join: &fb::GpuHashJoin,
    _node: &fb::PlanNode,
) -> Result<Arc<dyn ExecutionPlan>, String> {
    let left = deserialize_plan_node(&join.left().ok_or("GpuHashJoin missing left")?)?;
    let right = deserialize_plan_node(&join.right().ok_or("GpuHashJoin missing right")?)?;

    let join_type = match join.join_type() {
        fb::JoinType::Inner => DfJoinType::Inner,
        fb::JoinType::Left => DfJoinType::Left,
        fb::JoinType::Right => DfJoinType::Right,
        fb::JoinType::Full => DfJoinType::Full,
        fb::JoinType::LeftSemi => DfJoinType::LeftSemi,
        fb::JoinType::RightSemi => DfJoinType::RightSemi,
        fb::JoinType::LeftAnti => DfJoinType::LeftAnti,
        fb::JoinType::RightAnti => DfJoinType::RightAnti,
        other => return Err(format!("unsupported JoinType: {:?}", other)),
    };

    let on: Vec<(Arc<dyn PhysicalExpr>, Arc<dyn PhysicalExpr>)> = join
        .keys()
        .map(|keys| {
            (0..keys.len())
                .map(|i| {
                    let k = keys.get(i);
                    let l = deserialize_expr(&k.left().ok_or("JoinKey missing left")?)?;
                    let r = deserialize_expr(&k.right().ok_or("JoinKey missing right")?)?;
                    Ok((l, r))
                })
                .collect::<Result<Vec<_>, String>>()
        })
        .transpose()?
        .unwrap_or_default();

    let projection: Option<Vec<usize>> = join.projection().map(|v| {
        (0..v.len()).map(|i| v.get(i) as usize).collect()
    });

    let join_exec = HashJoinExec::try_new(
        left,
        right,
        on,
        None, // join filter (we serialized it but HashJoinExec::try_new takes JoinFilter, not PhysicalExpr)
        &join_type,
        projection,
        datafusion::physical_plan::joins::PartitionMode::CollectLeft,
        false, // null_equals_null
    )
    .map_err(|e| format!("HashJoinExec: {e}"))?;

    Ok(Arc::new(GpuHashJoinExec::new(Arc::new(join_exec))))
}

fn deserialize_gpu_sort(
    sort: &fb::GpuSort,
    _node: &fb::PlanNode,
) -> Result<Arc<dyn ExecutionPlan>, String> {
    let input = deserialize_plan_node(&sort.input().ok_or("GpuSort missing input")?)?;

    let sort_exprs: Vec<PhysicalSortExpr> = sort
        .exprs()
        .map(|exprs| {
            (0..exprs.len())
                .map(|i| {
                    let se = exprs.get(i);
                    let expr = deserialize_expr(&se.expr().ok_or("SortExpr missing expr")?)?;
                    Ok(PhysicalSortExpr::new(
                        expr,
                        SortOptions {
                            descending: !se.asc(),
                            nulls_first: se.nulls_first(),
                        },
                    ))
                })
                .collect::<Result<Vec<_>, String>>()
        })
        .transpose()?
        .unwrap_or_default();

    let mut sort_exec = SortExec::new(sort_exprs.into(), input);
    if sort.fetch() >= 0 {
        sort_exec = sort_exec.with_fetch(Some(sort.fetch() as usize));
    }

    Ok(Arc::new(GpuSortExec::new(Arc::new(sort_exec))))
}

fn deserialize_gpu_coalesce_batches(
    cb: &fb::GpuCoalesceBatches,
) -> Result<Arc<dyn ExecutionPlan>, String> {
    use datafusion::physical_plan::coalesce_batches::CoalesceBatchesExec;

    let input = deserialize_plan_node(&cb.input().ok_or("GpuCoalesceBatches missing input")?)?;
    let inner: Arc<dyn ExecutionPlan> =
        Arc::new(CoalesceBatchesExec::new(input, cb.target_batch_size() as usize));
    Ok(Arc::new(GpuCoalesceBatchesExec::new(inner)))
}

fn deserialize_gpu_coalesce_partitions(
    cp: &fb::GpuCoalescePartitions,
) -> Result<Arc<dyn ExecutionPlan>, String> {
    use datafusion::physical_plan::coalesce_partitions::CoalescePartitionsExec;

    let input = deserialize_plan_node(&cp.input().ok_or("GpuCoalescePartitions missing input")?)?;
    let inner: Arc<dyn ExecutionPlan> = Arc::new(CoalescePartitionsExec::new(input));
    Ok(Arc::new(GpuCoalescePartitionsExec::new(inner)))
}

fn deserialize_gpu_repartition(
    rp: &fb::GpuRepartition,
) -> Result<Arc<dyn ExecutionPlan>, String> {
    use datafusion::physical_plan::repartition::RepartitionExec;
    use datafusion::physical_plan::Partitioning;

    let input = deserialize_plan_node(&rp.input().ok_or("GpuRepartition missing input")?)?;

    let partitioning = match rp.kind() {
        fb::PartitioningKind::RoundRobinBatch => {
            Partitioning::RoundRobinBatch(rp.num_partitions() as usize)
        }
        fb::PartitioningKind::Hash => {
            let exprs: Vec<Arc<dyn PhysicalExpr>> = rp
                .hash_exprs()
                .map(|v| {
                    (0..v.len())
                        .map(|i| deserialize_expr(&v.get(i)))
                        .collect::<Result<Vec<_>, _>>()
                })
                .transpose()?
                .unwrap_or_default();
            Partitioning::Hash(exprs, rp.num_partitions() as usize)
        }
        fb::PartitioningKind::Unknown => {
            Partitioning::UnknownPartitioning(rp.num_partitions() as usize)
        }
        other => return Err(format!("unsupported PartitioningKind: {:?}", other)),
    };

    let inner: Arc<dyn ExecutionPlan> = Arc::new(
        RepartitionExec::try_new(input, partitioning)
            .map_err(|e| format!("RepartitionExec: {e}"))?,
    );
    Ok(Arc::new(GpuRepartitionExec::new(inner)))
}

fn deserialize_gpu_sort_preserving_merge(
    spm: &fb::GpuSortPreservingMerge,
) -> Result<Arc<dyn ExecutionPlan>, String> {
    use datafusion::physical_plan::sorts::sort_preserving_merge::SortPreservingMergeExec;

    let input = deserialize_plan_node(
        &spm.input().ok_or("GpuSortPreservingMerge missing input")?,
    )?;

    let sort_exprs: Vec<PhysicalSortExpr> = spm
        .exprs()
        .map(|exprs| {
            (0..exprs.len())
                .map(|i| {
                    let se = exprs.get(i);
                    let expr = deserialize_expr(&se.expr().ok_or("SortExpr missing expr")?)?;
                    Ok(PhysicalSortExpr::new(
                        expr,
                        SortOptions {
                            descending: !se.asc(),
                            nulls_first: se.nulls_first(),
                        },
                    ))
                })
                .collect::<Result<Vec<_>, String>>()
        })
        .transpose()?
        .unwrap_or_default();

    let mut merge_exec = SortPreservingMergeExec::new(sort_exprs.into(), input);
    if spm.fetch() >= 0 {
        merge_exec = merge_exec.with_fetch(Some(spm.fetch() as usize));
    }

    Ok(Arc::new(GpuSortPreservingMergeExec::new(Arc::new(merge_exec))))
}