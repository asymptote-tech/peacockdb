use crate::plan::*;
use datafusion::arrow::datatypes::{DataType, Field, Schema as ArrowSchema};
use datafusion::common::{JoinType, ScalarValue};
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
        check_canonical_form_of_roots(&[tree.as_ref()]),
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
    assert_eq!(check_canonical_form_of_roots(&[tree.as_ref()]), Ok(()));
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
    assert_eq!(check_canonical_form_of_roots(&[tree.as_ref()]), Ok(()));
}

#[test]
fn a_plan_that_does_not_end_in_a_crossing_is_refused() {
    invalid(validate(plain_source().as_ref()), "GpuUnload");
}

#[test]
fn a_sink_below_a_root_is_refused() {
    // The inner unload already moved its rows to the host, so whatever reads it is reading
    // something no longer on the device — under one root, or under the second of two.
    let message = "GpuUnload: a sink feeds another node — every GpuUnload is a root";
    let tree = rooted(rooted(plain_source()));
    invalid(validate(tree.as_ref()), message);
    let roots = [rooted(plain_source()), rooted(rooted(plain_source()))];
    invalid(
        validate_roots(&[roots[0].as_ref(), roots[1].as_ref()]),
        message,
    );
}

#[test]
fn every_root_is_a_sink() {
    let roots = [rooted(plain_source()), plain_source()];
    invalid(
        validate_roots(&[roots[0].as_ref(), roots[1].as_ref()]),
        "the planner roots it in GpuUnload",
    );
}

#[test]
fn the_validation_walk_visits_a_shared_node_once() {
    let load = crate::tests::plans::source("s", 1)
        .into_any_box()
        .downcast::<GpuLoadParquet>()
        .expect("plans::source builds a loader");
    let shared = SharedSource::new(*load);
    let roots = [
        rooted(crate::tests::plans::filter(Box::new(shared.clone()))),
        rooted(crate::tests::plans::project(Box::new(shared))),
    ];
    let roots = [roots[0].as_ref(), roots[1].as_ref()];
    let dag = PlanDag::of(&roots);
    let names: Vec<&str> = super::walk_order(&dag).map(|node| node.name()).collect();
    assert_eq!(
        names,
        [
            "GpuLoadParquet",
            "GpuFilter",
            "GpuUnload",
            "GpuProject",
            "GpuUnload"
        ]
    );
    assert_eq!(validate_roots(&roots), Ok(()));
    assert_eq!(check_canonical_form_of_roots(&roots), Ok(()));
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

/// A sort over a two-column source, declaring `declared` instead of the schema its
/// constructor derived. The planner never builds it; a test rewriting a planned tree can.
fn sort_declaring(declared: &[&str]) -> Box<dyn GpuNode> {
    let input = source(schema_of(&["a", "b"]), PartitionLayout::new(1));
    let mut sort = GpuSort::new(
        input,
        vec![ColumnOrder {
            column: 0,
            ascending: true,
            nulls_first: false,
        }],
        None,
    );
    let layout = sort.kind.layout().expect("a sort has a layout").clone();
    sort.kind = NodeKind::Intermediate {
        layout,
        schema: schema_of(declared),
    };
    Box::new(sort)
}

#[test]
fn a_pass_through_node_declaring_fewer_columns_than_its_input_is_refused() {
    // A sort carries its input's columns. Comparing them pairwise stops at the shorter
    // list, so a dropped column passed both checks.
    invalid(
        validate(rooted(sort_declaring(&["a"])).as_ref()),
        "GpuSort: it declares 1 columns and its input produces 2",
    );
}

#[test]
fn a_pass_through_node_declaring_more_columns_than_its_input_is_refused() {
    invalid(
        validate(rooted(sort_declaring(&["a", "b", "c"])).as_ref()),
        "GpuSort: it declares 3 columns and its input produces 2",
    );
}

#[test]
fn a_pass_through_node_declaring_its_inputs_columns_passes() {
    validate(rooted(sort_declaring(&["a", "b"])).as_ref())
        .expect("a sort carrying both columns is valid");
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

#[test]
fn a_node_declaring_a_view_type_is_refused() {
    // cuDF has one string layout and exports it as Utf8; a schema declaring Utf8View names
    // a container the device never holds, and the sink would refuse the batch (#183).
    let tree = rooted(source(
        one_column("s", DataType::Utf8View),
        PartitionLayout::new(1),
    ));
    invalid(
        validate(tree.as_ref()),
        "GpuDeclaring: column 0 s: Utf8View is a view type the device cannot hold",
    );
}

#[test]
fn a_decimal256_column_is_refused() {
    // The device's type is {type_id, scale}, and the only decimal the loader, the kernels
    // and the export agree on is DECIMAL128; a wider or narrower one has no crossing.
    let tree = rooted(source(
        one_column("d", DataType::Decimal256(50, 2)),
        PartitionLayout::new(1),
    ));
    invalid(
        validate(tree.as_ref()),
        "GpuDeclaring: column 0 d: Decimal256(50, 2) is not Decimal128, the one decimal that crosses to the device",
    );
}

#[test]
fn a_cast_to_a_decimal256_is_refused() {
    let input = source(
        one_column("d", DataType::Decimal128(10, 2)),
        PartitionLayout::new(1),
    );
    let project = Box::new(GpuProject::new(
        input,
        vec![NamedExpr::new(
            Expr::Cast {
                expr: Box::new(Expr::column(0, "d")),
                target: DataType::Decimal256(50, 2),
            },
            "w",
        )],
        one_column("w", DataType::Decimal128(10, 2)),
    ));
    invalid(
        validate(rooted(project).as_ref()),
        "GpuProject: a cast target is Decimal256(50, 2), not Decimal128, the one decimal that crosses to the device",
    );
}

#[test]
fn a_cast_to_a_view_type_is_refused() {
    let input = source(one_column("s", DataType::Utf8), PartitionLayout::new(1));
    let project = Box::new(GpuProject::new(
        input,
        vec![NamedExpr::new(
            Expr::Cast {
                expr: Box::new(Expr::column(0, "s")),
                target: DataType::Utf8View,
            },
            "v",
        )],
        one_column("v", DataType::Utf8),
    ));
    invalid(
        validate(rooted(project).as_ref()),
        "GpuProject: a cast target is Utf8View, a view type the device cannot hold",
    );
}

#[test]
fn a_residual_filter_naming_a_view_literal_is_refused() {
    // The residual is checked through collect_column_refs rather than check_column_refs,
    // so the literal would escape a rule that lives only on the latter's path.
    let build = source(
        one_column("k", DataType::Utf8),
        PartitionLayout {
            batch_layout: BatchLayout::SingleBatch,
            ..PartitionLayout::new(1)
        },
    );
    let probe = source(one_column("fk", DataType::Utf8), PartitionLayout::new(1));
    let filter = Expr::binary(
        Expr::column(0, "k"),
        BinaryOp::Eq,
        Expr::Literal(ScalarValue::Utf8View(Some("x".to_string()))),
        DataType::Boolean,
    );
    let join = Box::new(GpuHashJoin::new(
        build,
        probe,
        JoinType::Inner,
        vec![(0, 0)],
        Some(filter),
        vec![JoinFilterColumn {
            side: JoinSide::Build,
            index: 0,
        }],
        false,
        None,
        Schema::new(Arc::new(ArrowSchema::new(vec![
            Field::new("k", DataType::Utf8, true),
            Field::new("fk", DataType::Utf8, true),
        ]))),
    ));
    invalid(
        validate(rooted(join).as_ref()),
        "GpuHashJoin: a literal is Utf8View, a view type the device cannot hold",
    );
}

/// A grouping-set aggregate over one Utf8 key, summing `n`, with the NULL it substitutes
/// for the masked key and the intermediate schema both given — the two places an
/// aggregate names a type that no `check_column_refs` path and no `kind().schema()` reads.
fn grouping_set_aggregate(masked_key: Expr, intermediate: Schema) -> Box<dyn GpuNode> {
    let input = source(
        Schema::new(Arc::new(ArrowSchema::new(vec![
            Field::new("k", DataType::Utf8, true),
            Field::new("n", DataType::Int64, true),
        ]))),
        PartitionLayout::new(1),
    );
    let body = AggregateBody {
        group_by: vec![Expr::column(0, "k")],
        grouping_sets: vec![vec![false], vec![true]],
        null_exprs: vec![masked_key],
        aggs: vec![AggCall {
            func: PlanAgg::Sum,
            args: vec![Expr::column(1, "n")],
            outputs: vec![Field::new("n", DataType::Int64, true)],
        }],
        finalize: None,
    };
    let declared = Schema::new(Arc::new(ArrowSchema::new(vec![
        Field::new("k", DataType::Utf8, true),
        Field::new("__grouping_id", DataType::UInt8, false),
        Field::new("n", DataType::Int64, true),
    ])));
    Box::new(GpuAggregate::new(input, body, intermediate, declared))
}

#[test]
fn a_grouping_sets_null_of_a_view_type_is_refused() {
    let intermediate = Schema::new(Arc::new(ArrowSchema::new(vec![
        Field::new("k", DataType::Utf8, true),
        Field::new("__grouping_id", DataType::UInt8, false),
        Field::new("n", DataType::Int64, true),
    ])));
    let aggregate =
        grouping_set_aggregate(Expr::Literal(ScalarValue::Utf8View(None)), intermediate);
    invalid(
        validate(rooted(aggregate).as_ref()),
        "GpuAggregate: a literal is Utf8View, a view type the device cannot hold",
    );
}

#[test]
fn an_aggregate_whose_intermediate_names_a_view_type_is_refused() {
    let intermediate = Schema::new(Arc::new(ArrowSchema::new(vec![
        Field::new("k", DataType::Utf8View, true),
        Field::new("__grouping_id", DataType::UInt8, false),
        Field::new("n", DataType::Int64, true),
    ])));
    let aggregate = grouping_set_aggregate(Expr::Literal(ScalarValue::Utf8(None)), intermediate);
    invalid(
        validate(rooted(aggregate).as_ref()),
        "GpuAggregate: intermediate column 0 k: Utf8View is a view type the device cannot hold",
    );
}

// #63 — a device table of no columns reads as no rows, so no node may declare one.

#[test]
fn a_project_of_no_expression_is_refused() {
    let project = Box::new(GpuProject::new(
        plain_source(),
        Vec::new(),
        Schema::new(Arc::new(ArrowSchema::empty())),
    ));
    invalid(
        validate(rooted(project).as_ref()),
        "GpuProject: it declares no column",
    );
}

/// A scan of row group 0 projecting no column, declaring `schema`.
fn scan_of_no_column(schema: Schema) -> Box<dyn GpuNode> {
    let scan = ScanMetadata {
        file: "/nowhere.parquet".to_string(),
        groups: vec![RowGroupMeta {
            index: 0,
            rows: 5,
            bytes: 0,
            compressed_bytes: 0,
        }],
        can_be_null: Vec::new(),
    };
    Box::new(GpuLoadParquet::new(
        "t".to_string(),
        Vec::new(),
        vec![vec![vec![0]]],
        &scan,
        schema,
    ))
}

#[test]
fn a_scan_of_no_column_declares_the_rowmarker() {
    assert_eq!(
        validate(rooted(scan_of_no_column(Schema::rowmarker())).as_ref()),
        Ok(())
    );
    invalid(
        validate(rooted(scan_of_no_column(one_column("a", DataType::Int64))).as_ref()),
        "reads no column, so it declares __rowmarker__",
    );
}

// -- holds and buffers over several roots ------------------------------------------

fn shared_source(table: &str) -> SharedSource {
    let load = crate::tests::plans::source(table, 1)
        .into_any_box()
        .downcast::<GpuLoadParquet>()
        .expect("plans::source builds a loader");
    SharedSource::new(*load)
}

fn read(source: &SharedSource) -> Box<dyn GpuNode> {
    Box::new(source.clone())
}

fn buffered(input: Box<dyn GpuNode>) -> Box<dyn GpuNode> {
    Box::new(new_hold_buffer(input))
}

fn refs(roots: &[Box<dyn GpuNode>]) -> Vec<&dyn GpuNode> {
    roots.iter().map(|root| root.as_ref()).collect()
}

fn unbuffered(
    hold: usize,
    reader: usize,
    slot: usize,
    producer: usize,
) -> (usize, usize, usize, usize) {
    (hold, reader, slot, producer)
}

/// The refusal's four indices, or a panic naming whatever came back instead.
fn refused_edge(result: Result<(), PlanError>) -> (usize, usize, usize, usize) {
    match result {
        Err(PlanError::UnbufferedHold {
            hold,
            reader,
            slot,
            producer,
            ..
        }) => (hold, reader, slot, producer),
        other => panic!("expected an unbuffered hold, got {other:?}"),
    }
}

/// A = `unload(join(coalesce(b), filter(S)))`, B = `unload(project(S))`.
fn a_shared_source_read_inside_a_hold() -> Vec<Box<dyn GpuNode>> {
    use crate::tests::plans::{coalesce_all, filter, project, projected_join, source, unload};
    let s = shared_source("s");
    vec![
        unload(projected_join(
            coalesce_all(source("b", 1)),
            filter(read(&s)),
        )),
        unload(project(read(&s))),
    ]
}

/// A = `unload(join(coalesce(S1), S2))`, B = `unload(join(coalesce(S2), S1))`: each build
/// waits on the source the other one's probe holds.
fn builds_waiting_on_each_others_probe(buffers: bool) -> Vec<Box<dyn GpuNode>> {
    use crate::tests::plans::{coalesce_all, projected_join, unload};
    let (s1, s2) = (shared_source("s1"), shared_source("s2"));
    let probe = |source: &SharedSource| match buffers {
        true => buffered(read(source)),
        false => read(source),
    };
    vec![
        unload(projected_join(coalesce_all(read(&s1)), probe(&s2))),
        unload(projected_join(coalesce_all(read(&s2)), probe(&s1))),
    ]
}

#[test]
fn an_unbuffered_reader_of_a_shared_source_inside_a_hold_is_refused() {
    let roots = a_shared_source_read_inside_a_hold();
    // 0 unload, 1 join, 2 coalesce, 3 b, 4 filter, 5 S, 6 unload, 7 project.
    let refused = validate_roots(&refs(&roots));
    assert_eq!(refused_edge(refused.clone()), unbuffered(1, 4, 0, 5));
    assert_eq!(
        refused.expect_err("refused").to_string(),
        "invalid plan: GpuFilter (node 4) reads GpuLoadParquet (node 5) from outside the hold \
         of GpuHashJoin (node 1); a GpuHoldBuffer must sit on that edge"
    );
}

#[test]
fn a_join_reading_one_source_on_both_sides_without_a_buffer_is_refused() {
    use crate::tests::plans::{coalesce_all, projected_join, unload};
    let s = shared_source("s");
    // 0 unload, 1 join, 2 coalesce, 3 S.
    let roots = vec![unload(projected_join(coalesce_all(read(&s)), read(&s)))];
    assert_eq!(
        refused_edge(validate_roots(&refs(&roots))),
        unbuffered(1, 1, 1, 3)
    );
}

#[test]
fn a_buffer_on_that_edge_passes() {
    use crate::tests::plans::{coalesce_all, filter, project, projected_join, source, unload};
    let s = shared_source("s");
    let roots = vec![
        unload(projected_join(
            coalesce_all(source("b", 1)),
            filter(buffered(read(&s))),
        )),
        unload(project(read(&s))),
    ];
    assert_eq!(validate_roots(&refs(&roots)), Ok(()));
    let s = shared_source("s");
    let roots = vec![unload(projected_join(
        coalesce_all(read(&s)),
        buffered(read(&s)),
    ))];
    assert_eq!(validate_roots(&refs(&roots)), Ok(()));
}

#[test]
fn two_roots_whose_builds_wait_on_each_others_probe_are_refused_without_buffers() {
    let roots = builds_waiting_on_each_others_probe(false);
    // 0 unload, 1 join, 2 coalesce, 3 S1, 4 S2, 5 unload, 6 join, 7 coalesce.
    assert_eq!(
        refused_edge(validate_roots(&refs(&roots))),
        unbuffered(1, 1, 1, 4)
    );
    let roots = builds_waiting_on_each_others_probe(true);
    assert_eq!(validate_dag(&refs(&roots), None), Ok(()));
}

#[test]
fn a_tree_never_needs_a_buffer() {
    for fixture in crate::tests::rebuild::every_kind() {
        let root = rooted(fixture);
        let dag = PlanDag::of(&[root.as_ref()]);
        assert_eq!(super::holds_are_buffered(&dag), Ok(()), "{root:?}");
    }
}

#[test]
fn a_hold_cycle_is_refused_by_name() {
    assert_eq!(
        super::first_cycle(&[(1, 5), (5, 9), (9, 1)]),
        Some(vec![1, 5, 9])
    );
    assert_eq!(
        super::first_cycle(&[(1, 5), (5, 9), (9, 5)]),
        Some(vec![5, 9])
    );
    assert_eq!(super::first_cycle(&[(1, 5), (5, 9), (1, 9)]), None);
    assert_eq!(super::first_cycle(&[]), None);
    let roots = builds_waiting_on_each_others_probe(false);
    let dag = PlanDag::of(&refs(&roots));
    let refused = super::hold_cycle(&dag, vec![1, 6]);
    assert!(matches!(&refused, PlanError::HoldCycle { holds, .. } if *holds == vec![1, 6]));
    assert_eq!(
        refused.to_string(),
        "invalid plan: holds wait on each other: GpuHashJoin (node 1) → GpuHashJoin (node 6)"
    );
}

#[test]
fn a_join_waits_on_the_hold_its_build_side_sits_in() {
    use crate::tests::plans::{coalesce_all, filter, projected_join, source, unload};
    // The inner join is the outer one's probe side, so the inner build is held by the outer
    // join: 0 unload, 1 outer, 2 coalesce, 3 c, 4 inner, 5 coalesce, 6 a, 7 filter, 8 b.
    let inner = projected_join(coalesce_all(source("a", 1)), filter(source("b", 1)));
    let root = unload(projected_join(coalesce_all(source("c", 1)), inner));
    let dag = PlanDag::of(&[root.as_ref()]);
    assert_eq!(super::wait_for_edges(&dag), vec![(4, 1)]);
    assert_eq!(validate_dag(&[root.as_ref()], None), Ok(()));
}

#[test]
fn validate_dag_refuses_what_its_parts_refuse() {
    use crate::tests::plans::{coalesce_all, projected_join, unload};
    let s = shared_source("s");
    let shapes = vec![
        a_shared_source_read_inside_a_hold(),
        builds_waiting_on_each_others_probe(false),
        vec![unload(projected_join(coalesce_all(read(&s)), read(&s)))],
        vec![rooted(rooted(plain_source()))],
    ];
    for roots in shapes {
        let refused = validate_roots(&refs(&roots));
        assert!(refused.is_err(), "{roots:?}");
        assert_eq!(validate_dag(&refs(&roots), None), refused);
    }
}

#[test]
fn buffering_each_refused_edge_makes_a_plan_valid() {
    use crate::tests::share::buffer_unbuffered_holds;
    let (roots, inserted) =
        buffer_unbuffered_holds(builds_waiting_on_each_others_probe(false), None);
    assert_eq!(inserted, 2);
    assert_eq!(validate_dag(&refs(&roots), None), Ok(()));
    let (roots, inserted) = buffer_unbuffered_holds(a_shared_source_read_inside_a_hold(), None);
    assert_eq!(inserted, 1);
    let dag = PlanDag::of(&refs(&roots));
    assert_eq!(dag.nodes[4].node.name(), "GpuFilter");
    assert!(dag.is_buffer(dag.nodes[4].children[0]));
    assert_eq!(validate_dag(&refs(&roots), None), Ok(()));
}
