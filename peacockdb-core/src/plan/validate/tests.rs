use crate::plan::*;
use datafusion::arrow::datatypes::{DataType, Field, Schema as ArrowSchema};
use std::any::Any;
use std::sync::Arc;

/// A node whose kind the test writes: the structural rules are about what a node
/// declares of itself, and the translation layer is what makes those declarations
/// agree with the schema — so a plan that breaks one is unreachable from sql.
#[derive(Debug)]
struct Declaring {
    kind: NodeKind,
    children: Vec<Box<dyn GpuNode>>,
}

impl GpuNode for Declaring {
    fn kind(&self) -> &NodeKind {
        &self.kind
    }

    fn name(&self) -> &'static str {
        "GpuDeclaring"
    }

    fn children(&self) -> Vec<&dyn GpuNode> {
        self.children.iter().map(|c| c.as_ref()).collect()
    }

    fn validate_schemas_and_partitions(&self) -> Result<(), PlanError> {
        Ok(())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

fn schema_of(columns: &[&str]) -> Schema {
    Schema::new(Arc::new(ArrowSchema::new(
        columns
            .iter()
            .map(|name| Field::new(*name, DataType::Int64, true))
            .collect::<Vec<Field>>(),
    )))
}

/// A source, since a leaf that is not one is itself a finding below.
fn source(schema: Schema, layout: PartitionLayout) -> Box<dyn GpuNode> {
    Box::new(Declaring {
        kind: NodeKind::Source { layout, schema },
        children: Vec::new(),
    })
}

fn plain_source() -> Box<dyn GpuNode> {
    source(schema_of(&["a"]), PartitionLayout::new(1))
}

/// The tree as the planner shapes it: whatever is given, under an unload.
fn rooted(input: Box<dyn GpuNode>) -> Box<dyn GpuNode> {
    Box::new(GpuUnload::new(input, None))
}

fn invalid(result: Result<(), PlanError>, mentions: &str) {
    match result {
        Err(PlanError::Invalid(what)) => assert!(
            what.contains(mentions),
            "the error names the wrong thing: {what}"
        ),
        other => panic!("expected an invalid plan naming {mentions}, got {other:?}"),
    }
}

#[test]
fn a_tree_the_planner_shapes_passes_both_halves() {
    let tree = rooted(Box::new(GpuMergePartitions::new(plain_source())));
    assert_eq!(validate(tree.as_ref()), Ok(()));
}

#[test]
fn a_limit_whose_only_consumer_is_the_sink_is_refused() {
    // Root-adjacent is the other lowering: the interval belongs to the unload, which is
    // what lets the driver release a batch it wants none of rather than move it and
    // throw the rows away.
    let tree = rooted(Box::new(GpuLimit::new(
        plain_source(),
        RowInterval {
            skip: 0,
            fetch: Some(5),
        },
    )));
    invalid(
        validate(tree.as_ref()),
        "a limit feeding only the sink is not a node",
    );
    invalid(
        check_canonical_form(tree.as_ref()),
        "a limit feeding only the sink is not a node",
    );
}

#[test]
fn a_limit_with_a_real_consumer_above_it_is_the_shape_that_passes() {
    let tree = rooted(Box::new(GpuMergePartitions::new(Box::new(GpuLimit::new(
        plain_source(),
        RowInterval {
            skip: 2,
            fetch: Some(5),
        },
    )))));
    assert_eq!(check_canonical_form(tree.as_ref()), Ok(()));
    assert_eq!(validate(tree.as_ref()), Ok(()));
}

#[test]
fn the_unloads_own_interval_is_the_root_adjacent_lowering_and_not_a_finding() {
    let tree: Box<dyn GpuNode> = Box::new(GpuUnload::new(
        plain_source(),
        Some(RowInterval {
            skip: 3,
            fetch: Some(20),
        }),
    ));
    assert_eq!(check_canonical_form(tree.as_ref()), Ok(()));
}

#[test]
fn a_plan_that_does_not_end_in_a_crossing_is_refused() {
    invalid(validate(plain_source().as_ref()), "GpuUnload");
}

#[test]
fn a_sink_below_the_root_is_refused() {
    // Two unloads: the inner one already moved its rows to the host, so the outer
    // one is reading something that is no longer on the device.
    let tree = rooted(rooted(plain_source()));
    invalid(validate(tree.as_ref()), "already crossed to the host");
}

#[test]
fn an_intermediate_with_no_input_is_refused() {
    let leaf = Box::new(Declaring {
        kind: NodeKind::Intermediate {
            layout: PartitionLayout::new(1),
            schema: schema_of(&["a"]),
        },
        children: Vec::new(),
    });
    invalid(
        validate(rooted(leaf).as_ref()),
        "only a source has no input",
    );
}

#[test]
fn a_source_with_an_input_is_refused() {
    let loaded = Box::new(Declaring {
        kind: NodeKind::Source {
            layout: PartitionLayout::new(1),
            schema: schema_of(&["a"]),
        },
        children: vec![plain_source()],
    });
    invalid(validate(rooted(loaded).as_ref()), "takes no input");
}

#[test]
fn a_node_declaring_no_lanes_is_refused() {
    let empty = source(schema_of(&["a"]), PartitionLayout::new(0));
    invalid(validate(rooted(empty).as_ref()), "no lanes");
}

#[test]
fn a_hash_key_past_the_columns_it_emits_is_refused() {
    let layout = PartitionLayout {
        key_distribution: KeyDistribution::ByHash { hash_keys: vec![3] },
        ..PartitionLayout::new(4)
    };
    // The node-local checks read a node's keys against its input; this one reads the
    // claim it makes about its own output, which nothing else looks at.
    let node = source(schema_of(&["a", "b"]), layout);
    invalid(
        validate(rooted(node).as_ref()),
        "hash key @3 is past the 2 columns",
    );
}

#[test]
fn a_sort_key_past_the_columns_it_emits_is_refused() {
    let layout = PartitionLayout {
        sort_order: SortOrder::batch_sorted(vec![ColumnOrder {
            column: 2,
            ascending: true,
            nulls_first: false,
        }]),
        batch_layout: BatchLayout::SingleBatch,
        ..PartitionLayout::new(1)
    };
    let node = source(schema_of(&["a", "b"]), layout);
    invalid(
        validate(rooted(node).as_ref()),
        "sort key @2 is past the 2 columns",
    );
}

#[test]
fn a_group_key_past_the_columns_it_emits_is_refused() {
    let mut schema = schema_of(&["k", "n"]);
    schema.group_keys = vec![0, 5];
    let node = source(schema, PartitionLayout::new(1));
    invalid(
        validate(rooted(node).as_ref()),
        "group key @5 is past the 2 columns",
    );
}

fn state_at(positions: Vec<u32>) -> AggStateColumns {
    AggStateColumns {
        output: "avg(l_quantity)".to_string(),
        func: AggFunc::Avg,
        ddof: 0,
        positions,
    }
}

#[test]
fn a_state_column_past_the_columns_it_emits_is_refused() {
    let mut schema = schema_of(&["k", "avg$sum", "avg$count"]);
    schema.group_keys = vec![0];
    schema.agg_state = vec![state_at(vec![1, 4])];
    let node = source(schema, PartitionLayout::new(1));
    invalid(
        validate(rooted(node).as_ref()),
        "avg(l_quantity) state @4 is past the 3 columns",
    );
}

#[test]
fn an_aggregate_declaring_state_in_no_column_is_refused() {
    let mut schema = schema_of(&["k"]);
    schema.group_keys = vec![0];
    schema.agg_state = vec![state_at(Vec::new())];
    let node = source(schema, PartitionLayout::new(1));
    invalid(validate(rooted(node).as_ref()), "state in no column");
}

#[test]
fn state_declared_in_a_group_key_is_refused() {
    // The two overlap only if the positions were derived against a different column
    // order, and a merge reading a key as state would merge the thing it groups by.
    let mut schema = schema_of(&["k", "avg$sum"]);
    schema.group_keys = vec![0];
    schema.agg_state = vec![state_at(vec![0, 1])];
    let node = source(schema, PartitionLayout::new(1));
    invalid(
        validate(rooted(node).as_ref()),
        "state in a column that is also a group key",
    );
}

#[test]
fn a_hash_nothing_below_scattered_is_refused() {
    // The shape a join minted before it read its children: lanes hashed on a column
    // nothing ever scattered by, which a co-partitioned join above would then trust.
    let claiming = source(
        schema_of(&["k"]),
        PartitionLayout {
            key_distribution: KeyDistribution::ByHash { hash_keys: vec![0] },
            ..PartitionLayout::new(4)
        },
    );
    invalid(
        validate(rooted(claiming).as_ref()),
        "nothing below it scattered them",
    );
}

#[test]
fn an_order_nothing_below_sorted_is_refused() {
    let claiming = source(
        schema_of(&["a"]),
        PartitionLayout {
            sort_order: SortOrder::batch_sorted(vec![ColumnOrder {
                column: 0,
                ascending: true,
                nulls_first: false,
            }]),
            ..PartitionLayout::new(1)
        },
    );
    invalid(
        validate(rooted(claiming).as_ref()),
        "nothing below it sorted any",
    );
}

/// A schema of one column, of the type given: the pair below is Int64 against
/// Decimal128(15,2), because a decimal read as its neighbour is what these rules
/// exist to catch and what no per-node byte count can show.
fn one_column(name: &str, data_type: DataType) -> Schema {
    Schema::new(Arc::new(ArrowSchema::new(vec![Field::new(
        name, data_type, true,
    )])))
}

#[test]
fn a_column_that_changes_type_across_an_edge_is_refused() {
    // A filter drops rows and keeps columns, so declaring a decimal where its input
    // holds an integer is a claim about a column nobody produced. Both engines derive
    // their per-node bytes from this same declaration, so it costs the same on either
    // and shows up only in the answer.
    let input = source(one_column("a", DataType::Int64), PartitionLayout::new(1));
    let filter = Box::new(GpuFilter::new(
        input,
        Expr::column(0, "a"),
        None,
        one_column("a", DataType::Decimal128(15, 2)),
    ));
    invalid(
        validate(rooted(filter).as_ref()),
        "declares a as Decimal128(15, 2) where its input holds a as Int64",
    );
}

#[test]
fn a_node_declaring_more_columns_than_it_produces_is_refused() {
    let input = source(one_column("a", DataType::Int64), PartitionLayout::new(1));
    let project = Box::new(GpuProject::new(
        input,
        vec![NamedExpr::new(Expr::column(0, "a"), "a")],
        Schema::new(Arc::new(ArrowSchema::new(vec![
            Field::new("a", DataType::Int64, true),
            Field::new("b", DataType::Int64, true),
        ]))),
    ));
    invalid(
        validate(rooted(project).as_ref()),
        "it declares 2 columns and its expression list produces 1",
    );
}

#[test]
fn an_aggregate_declaring_a_width_its_body_does_not_produce_is_refused() {
    // Keys plus state where it hands state on. The aggregate is the node whose schema
    // carries the annotations a merge reads, so a width slip there mis-numbers the
    // state columns rather than only the output.

    let input = source(one_column("n", DataType::Int64), PartitionLayout::new(1));
    let body = AggregateBody {
        group_by: Vec::new(),
        grouping_sets: Vec::new(),
        null_exprs: Vec::new(),
        aggs: vec![AggCall {
            func: PlanAgg::Sum,
            args: vec![Expr::column(0, "n")],
            outputs: vec![Field::new("n", DataType::Int64, true)],
        }],
        finalize: None,
    };
    let declared = Schema::new(Arc::new(ArrowSchema::new(vec![
        Field::new("n", DataType::Int64, true),
        Field::new("spare", DataType::Int64, true),
    ])));
    let aggregate = Box::new(GpuAggregate::new(input, body, declared.clone(), declared));
    invalid(
        validate(rooted(aggregate).as_ref()),
        "it declares 2 columns and its keys and its state columns produces 1",
    );
}

#[test]
fn a_plan_emitting_a_different_column_count_from_the_query_is_refused() {
    let tree = rooted(source(
        one_column("a", DataType::Int64),
        PartitionLayout::new(1),
    ));
    let asked = ArrowSchema::new(vec![
        Field::new("a", DataType::Int64, true),
        Field::new("b", DataType::Int64, true),
    ]);
    invalid(
        check_output_schema(tree.as_ref(), &asked),
        "the plan emits 1 columns and the query asked for 2",
    );
}

#[test]
fn a_plan_emitting_a_column_of_another_type_than_the_query_is_refused() {
    // The layer's contract with its caller: every node below is checked against its
    // own children, so a tree can be internally consistent and answer in a type the
    // query did not ask for.
    let tree = rooted(source(
        one_column("total", DataType::Int64),
        PartitionLayout::new(1),
    ));
    let asked = ArrowSchema::new(vec![Field::new("total", DataType::Decimal128(15, 2), true)]);
    invalid(
        check_output_schema(tree.as_ref(), &asked),
        "the plan emits total as Int64 where the query asked for total as Decimal128(15, 2)",
    );
}

#[test]
fn a_childs_complaint_comes_before_its_parents() {
    // Post-order, so the deepest defect is the one reported: a parent's message names
    // a fix that would not help if its input is already wrong.
    let broken = source(schema_of(&["a"]), PartitionLayout::new(0));
    let tree = rooted(Box::new(GpuMergePartitions::new(broken)));
    invalid(validate(tree.as_ref()), "no lanes");
}
