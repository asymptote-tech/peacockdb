//! The node line: the name, the node's own fields, its layout and its declared schema.

use std::fmt::Write as _;

use datafusion::arrow::datatypes::DataType;

use super::expr_text::{expr_text, join_filter_text};
use crate::plan::Schema;
use crate::plan::{AggCall, PlanAgg};
use crate::plan::{AggregateBody, NodeRef, as_node_ref};
use crate::plan::{BatchLayout, ColumnOrder, KeyDistribution, PartitionLayout, SortOrder};
use crate::plan::{Expr, NamedExpr};
use crate::plan::{GpuNode, RowInterval};

pub(crate) fn render_plan(root: &dyn GpuNode) -> String {
    let mut text = String::new();
    render_node(root, 0, &mut text);
    text
}

fn render_node(node: &dyn GpuNode, depth: usize, text: &mut String) {
    let _ = writeln!(text, "{}{}", "  ".repeat(depth), node_line(node));
    for child in node.children() {
        render_node(child, depth + 1, text);
    }
}

fn node_line(node: &dyn GpuNode) -> String {
    let mut parts = node_line_parts(node);
    if let Some(schema) = node.kind().schema() {
        parts.push(schema_text(schema));
    }
    join_parts(parts)
}

/// The name, the node's own fields and its layout — everything both goldens carry. The
/// declared schema is the plan golden's alone and what the run produced is the execution
/// golden's, so each caller appends its own tail.
pub(crate) fn node_line_parts(node: &dyn GpuNode) -> Vec<String> {
    let mut parts = vec![node.name().to_string()];
    parts.extend(node_fields(node));
    if let Some(layout) = node.kind().layout() {
        parts.push(layout_text(layout, schema_of(node)));
    }
    parts
}

/// `Name: field, field` — the name alone where a node has no fields at all.
pub(crate) fn join_parts(mut parts: Vec<String>) -> String {
    let mut line = parts.remove(0);
    if !parts.is_empty() {
        let _ = write!(line, ": {}", parts.join(", "));
    }
    line
}

fn schema_of(node: &dyn GpuNode) -> Option<&Schema> {
    node.kind().schema()
}

/// What this node was asked to do — the parameters that decide its answer, and nothing
/// that is derived from its children or repeated by the layout.
fn node_fields(node: &dyn GpuNode) -> Vec<String> {
    let input_schema = node.children().first().and_then(|child| schema_of(*child));
    let mut fields = Vec::new();
    match as_node_ref(node) {
        NodeRef::LoadParquet(load) => {
            fields.push(format!("table={}", load.table));
            let schema = schema_of(node);
            let projected: Vec<String> = load
                .projection
                .iter()
                .enumerate()
                .map(|(position, file_ordinal)| {
                    format!("{}@{file_ordinal}", name_at(schema, position as u32))
                })
                .collect();
            fields.push(format!("projections=[{}]", projected.join(", ")));
            // The mapping verbatim: partitions outermost, batches within them, row groups
            // innermost. Which batch sits in which partition is the whole content.
            fields.push(format!(
                "partition_groups={}",
                nested(&load.partition_groups)
            ));
            if let Some(limit) = load.limit {
                fields.push(format!("limit={limit}"));
            }
        }
        NodeRef::Filter(filter) => {
            fields.push(format!("predicate={}", expr_text(&filter.predicate)));
            if let Some(projection) = &filter.projection {
                let kept: Vec<String> = projection
                    .iter()
                    .map(|ordinal| format!("{}@{ordinal}", name_at(input_schema, *ordinal)))
                    .collect();
                fields.push(format!("projection=[{}]", kept.join(", ")));
            }
        }
        NodeRef::Project(project) => {
            fields.push(format!("exprs=[{}]", named_exprs(&project.exprs)));
        }
        NodeRef::Sort(sort) => {
            fields.push(format!("by=[{}]", sort_keys(&sort.keys, input_schema)));
            push_fetch(&mut fields, sort.fetch);
        }
        NodeRef::AccumulateBatchesAndSort(accumulator) => {
            fields.push(format!(
                "by=[{}]",
                sort_keys(&accumulator.keys, input_schema)
            ));
            push_fetch(&mut fields, accumulator.fetch);
        }
        NodeRef::MergeSortedPartitions(merge) => {
            fields.push(format!("by=[{}]", sort_keys(&merge.keys, input_schema)));
            push_fetch(&mut fields, merge.fetch);
        }
        NodeRef::Limit(limit) => fields.extend(interval_fields(limit.interval)),
        NodeRef::Unload(unload) => {
            if let Some(interval) = unload.interval {
                fields.extend(interval_fields(interval));
            }
        }
        NodeRef::Aggregate(aggregate) => aggregate_fields(&mut fields, &aggregate.body),
        NodeRef::AggregateBatches(aggregate) => aggregate_fields(&mut fields, &aggregate.body),
        NodeRef::Join(join) => {
            fields.push(format!("join_type={:?}", join.join_type));
            let build = schema_of(node.children()[0]);
            let probe = schema_of(node.children()[1]);
            let on: Vec<String> = join
                .keys
                .iter()
                .map(|(left, right)| {
                    format!(
                        "({}@{left}, {}@{right})",
                        name_at(build, *left),
                        name_at(probe, *right)
                    )
                })
                .collect();
            fields.push(format!("on=[{}]", on.join(", ")));
            if let Some(filter) = &join.filter {
                fields.push(format!(
                    "filter={}",
                    join_filter_text(filter, &join.filter_columns, build, probe)
                ));
            }
            // Only the non-default prints: the SQL default is false, which is what nearly
            // every join carries, and a line saying so on each of them would say nothing.
            if join.null_equals_null {
                fields.push("null_equals_null=true".to_string());
            }
            projection_field(&mut fields, join.projection.as_ref(), build, probe);
        }
        NodeRef::NestedLoopJoin(join) => {
            let (build, probe) = (schema_of(node.children()[0]), schema_of(node.children()[1]));
            fields.push(format!("join_type={:?}", join.join_type));
            fields.push(format!(
                "filter={}",
                join_filter_text(&join.filter, &join.filter_columns, build, probe)
            ));
            projection_field(&mut fields, join.projection.as_ref(), build, probe);
        }
        NodeRef::CrossJoin(join) => projection_field(
            &mut fields,
            join.projection.as_ref(),
            schema_of(node.children()[0]),
            schema_of(node.children()[1]),
        ),
        NodeRef::EmitPartitions(emit) => {
            let keys: Vec<String> = emit
                .hash_keys
                .iter()
                .map(|key| format!("{}@{key}", name_at(input_schema, *key)))
                .collect();
            fields.push(format!("hash=[{}]", keys.join(", ")));
        }
        NodeRef::CoalesceAllBatches(_)
        | NodeRef::MergePartitions(_)
        | NodeRef::Union(_)
        | NodeRef::Interleave(_) => {}
    }
    fields
}

/// Ordinals into the crossed table, so they are named from both sides in order — the same
/// rule as every other reference.
fn projection_field(
    fields: &mut Vec<String>,
    projection: Option<&Vec<u32>>,
    build: Option<&Schema>,
    probe: Option<&Schema>,
) {
    let Some(projection) = projection else {
        return;
    };
    let build_width = build
        .map(|schema| schema.fields.fields().len() as u32)
        .unwrap_or(0);
    let joined: Vec<String> = projection
        .iter()
        .map(|ordinal| {
            if *ordinal < build_width {
                format!("{}@{ordinal}", name_at(build, *ordinal))
            } else {
                format!("{}@{ordinal}", name_at(probe, *ordinal - build_width))
            }
        })
        .collect();
    fields.push(format!("projection=[{}]", joined.join(", ")));
}

fn aggregate_fields(fields: &mut Vec<String>, body: &AggregateBody) {
    let keys: Vec<String> = body.group_by.iter().map(expr_text).collect();
    fields.push(format!("group_by=[{}]", keys.join(", ")));
    if !body.grouping_sets.is_empty() {
        // The keys each set groups on — the complement of the mask, which is the half a
        // reader checks against the rollup the query asked for.
        let sets: Vec<String> = body
            .grouping_sets
            .iter()
            .map(|mask| {
                let held: Vec<&str> = mask
                    .iter()
                    .enumerate()
                    .filter(|(_, is_null)| !**is_null)
                    .filter_map(|(index, _)| keys.get(index).map(String::as_str))
                    .collect();
                format!("[{}]", held.join(", "))
            })
            .collect();
        fields.push(format!("grouping_sets=[{}]", sets.join(", ")));
    }
    let aggs: Vec<String> = body.aggs.iter().map(agg_call_text).collect();
    fields.push(format!("aggs=[{}]", aggs.join(", ")));
    if let Some(finalize) = &body.finalize {
        fields.push(format!("final=[{}]", named_exprs(finalize)));
    }
}

fn agg_call_text(call: &AggCall) -> String {
    let args: Vec<String> = call.args.iter().map(expr_text).collect();
    let outputs: Vec<&str> = call
        .outputs
        .iter()
        .map(|field| field.name().as_str())
        .collect();
    let produced = if outputs.len() == 1 {
        quoted(outputs[0])
    } else {
        format!(
            "[{}]",
            outputs
                .iter()
                .map(|name| quoted(name))
                .collect::<Vec<_>>()
                .join(", ")
        )
    };
    format!(
        "{}({}) as {produced}",
        plan_agg_name(call.func),
        args.join(", ")
    )
}

fn plan_agg_name(func: PlanAgg) -> &'static str {
    match func {
        PlanAgg::Sum => "sum",
        PlanAgg::Min => "min",
        PlanAgg::Max => "max",
        PlanAgg::Count => "count",
        PlanAgg::Mean => "mean",
        PlanAgg::M2 => "m2",
        PlanAgg::MergeM2 => "merge_m2",
    }
}

fn interval_fields(interval: RowInterval) -> Vec<String> {
    let mut fields = vec![format!("skip={}", interval.skip)];
    if let Some(fetch) = interval.fetch {
        fields.push(format!("fetch={fetch}"));
    }
    fields
}

fn push_fetch(fields: &mut Vec<String>, fetch: Option<usize>) {
    if let Some(fetch) = fetch {
        fields.push(format!("fetch={fetch}"));
    }
}

/// Lane count and batch layout always; a hash or an order only where one is declared,
/// since a line saying a node declares nothing says nothing.
fn layout_text(layout: &PartitionLayout, schema: Option<&Schema>) -> String {
    let mut text = format!("lanes={}", layout.n);
    let _ = write!(
        text,
        ", batches={}",
        match layout.batch_layout {
            BatchLayout::SingleBatch => "single",
            BatchLayout::MultipleBatches => "multiple",
        }
    );
    if let KeyDistribution::ByHash { hash_keys } = &layout.key_distribution {
        let keys: Vec<String> = hash_keys
            .iter()
            .map(|key| format!("{}@{key}", name_at(schema, *key)))
            .collect();
        let _ = write!(text, ", hashed_on=[{}]", keys.join(", "));
    }
    if let SortOrder::BatchSorted { columns } = &layout.sort_order {
        let _ = write!(text, ", sorted_on=[{}]", sort_keys(columns, schema));
    }
    text
}

pub(crate) fn schema_text(schema: &Schema) -> String {
    let columns: Vec<String> = schema
        .fields
        .fields()
        .iter()
        .map(|field| format!("{}:{}", quoted(field.name()), type_text(field.data_type())))
        .collect();
    format!("schema=[{}]", columns.join(", "))
}

/// Arrow's own rendering, minus the noise a plan reader does not need. Decimal precision
/// and scale stay: an explicit cast's target is unreadable without them.
pub(crate) fn type_text(data_type: &DataType) -> String {
    match data_type {
        DataType::Decimal128(precision, scale) => format!("Decimal128({precision},{scale})"),
        other => format!("{other:?}"),
    }
}

fn sort_keys(keys: &[ColumnOrder], schema: Option<&Schema>) -> String {
    keys.iter()
        .map(|key| {
            format!(
                "{}@{} {} {}",
                name_at(schema, key.column),
                key.column,
                if key.ascending { "asc" } else { "desc" },
                if key.nulls_first {
                    "nulls_first"
                } else {
                    "nulls_last"
                }
            )
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn named_exprs(exprs: &[NamedExpr]) -> String {
    exprs
        .iter()
        .map(|named| {
            let rendered = expr_text(&named.expr);
            match &named.expr {
                Expr::Column(reference) if reference.name == named.name => rendered,
                _ => format!("{rendered} as {}", quoted(&named.name)),
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// A name as a single token: bare where it already is one, in backticks where it holds a
/// character this rendering punctuates with — whitespace, a comma, a bracket, an `@`, or a
/// backtick, which doubles.
///
/// A name is not always an identifier: DataFusion names an aggregate output by its own
/// expression text, and tpcds aliases like `order count`. Unquoted, `order count@0 as
/// order count` leaves a reader no way to see where the name ends. Backticks rather than
/// quotes because a name can hold a rendered literal — `Utf8("PROMO%")` — and doubling
/// those is the unreadable half of the problem.
pub(crate) fn quoted(name: &str) -> String {
    let plain = !name.is_empty()
        && !name
            .chars()
            .any(|c| c.is_whitespace() || matches!(c, ',' | '[' | ']' | '@' | '`'));
    if plain {
        name.to_string()
    } else {
        format!("`{}`", name.replace('`', "``"))
    }
}

/// The name a schema declares at that position. An empty name is what a reference past
/// the end renders as, and validation is what refuses the plan.
pub(crate) fn name_at(schema: Option<&Schema>, index: u32) -> String {
    schema
        .and_then(|schema| schema.fields.fields().get(index as usize))
        .map(|field| quoted(field.name()))
        .unwrap_or_else(|| "?".to_string())
}

fn nested(groups: &[Vec<Vec<u32>>]) -> String {
    let partitions: Vec<String> = groups
        .iter()
        .map(|batches| {
            let rendered: Vec<String> = batches
                .iter()
                .map(|row_groups| {
                    format!(
                        "[{}]",
                        row_groups
                            .iter()
                            .map(u32::to_string)
                            .collect::<Vec<_>>()
                            .join(",")
                    )
                })
                .collect();
            format!("[{}]", rendered.join(","))
        })
        .collect();
    format!("[{}]", partitions.join(","))
}
