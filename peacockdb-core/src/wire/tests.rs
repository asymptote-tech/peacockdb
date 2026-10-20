//! The kinds whose recipe is more than one call, the join session first: the `CudfJoin` each
//! join node writes and the calls its session makes, per join type. The trivial kinds are
//! not tested here — the plan goldens run them over every corpus query, which is more
//! coverage than a hand-built node would be.

use super::attach::{accumulate_and_sort, aggregate, aggregate_batches};
use super::generated::peacock::plan as fb;
use super::join::JoinNode;
use super::read::{children, node_at};
use super::writer::Writer;
use super::*;
use crate::plan::AggregateBody;
use crate::plan::RowInterval;
use crate::plan::Schema;
use crate::plan::{AggCall, PlanAgg};
use crate::plan::{BatchLayout, ColumnOrder, NodeKind, PartitionLayout};
use crate::plan::{BinaryOp, Expr, NamedExpr};
use crate::plan::{
    GpuAccumulateBatchesAndSort, GpuAggregate, GpuAggregateBatches, GpuCrossJoin, GpuEmpty,
    GpuFilter, GpuHashJoin, GpuLimit, GpuNestedLoopJoin, GpuUnload,
};
use crate::plan::{JoinFilterColumn, JoinSide};
use datafusion::arrow::array::{ArrayRef, Int64Array, RecordBatch};
use datafusion::arrow::datatypes::{DataType, Field, Schema as ArrowSchema};
use datafusion::common::JoinType;
use datafusion::common::ScalarValue;
use std::any::Any;
use std::sync::Arc;

mod aggregate_init;

/// An input whose layout and schema the test writes. A recipe function is handed its
/// node and the schemas that node declares it consumes, so that is all a case needs to
/// build — and a stub is what keeps a case about the join rather than about a subtree.
#[derive(Debug)]
struct Given {
    kind: NodeKind,
}

impl Given {
    fn input(batches: BatchLayout, columns: &[&str]) -> Box<dyn GpuNode> {
        Box::new(Given {
            kind: NodeKind::Intermediate {
                layout: PartitionLayout {
                    batch_layout: batches,
                    ..PartitionLayout::new(1)
                },
                schema: columns_of(columns),
            },
        })
    }
}

impl GpuNode for Given {
    fn kind(&self) -> &NodeKind {
        &self.kind
    }

    fn name(&self) -> &'static str {
        "GpuGiven"
    }

    fn children(&self) -> Vec<&dyn GpuNode> {
        Vec::new()
    }

    fn validate_schemas_and_partitions(&self) -> Result<(), crate::plan::PlanError> {
        Ok(())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// A writer whose joins carry no budget of their own: the session's default.
fn writer<'a>() -> Writer<'a> {
    Writer::with_join_scratch(0)
}

fn columns_of(columns: &[&str]) -> Schema {
    Schema::new(Arc::new(ArrowSchema::new(
        columns
            .iter()
            .map(|name| Field::new(*name, DataType::Int64, true))
            .collect::<Vec<Field>>(),
    )))
}

/// `dim(k, label)` joined to `fact(fk, v)`. `filter` is a residual over both sides, which
/// the session evaluates per pair whatever the type.
fn join(join_type: JoinType, filter: bool, projection: Option<Vec<u32>>) -> GpuHashJoin {
    declaring(
        join_type,
        filter,
        projection,
        columns_of(&["k", "label", "fk", "v"]),
    )
}

fn declaring(
    join_type: JoinType,
    filter: bool,
    projection: Option<Vec<u32>>,
    output: Schema,
) -> GpuHashJoin {
    let build = Given::input(BatchLayout::SingleBatch, &["k", "label"]);
    let probe = Given::input(BatchLayout::MultipleBatches, &["fk", "v"]);
    let (residual, columns) = if filter {
        (
            Some(Expr::binary(
                Expr::column(0, "label"),
                BinaryOp::Lt,
                Expr::column(1, "v"),
                DataType::Boolean,
            )),
            label_and_v(),
        )
    } else {
        (None, Vec::new())
    };
    GpuHashJoin::new(
        build,
        probe,
        join_type,
        vec![(0, 0)],
        residual,
        columns,
        false,
        projection,
        output,
    )
}

/// The residual's own schema: the build's `label`, then the probe's `v`.
fn label_and_v() -> Vec<JoinFilterColumn> {
    vec![
        JoinFilterColumn {
            side: JoinSide::Build,
            index: 1,
        },
        JoinFilterColumn {
            side: JoinSide::Probe,
            index: 1,
        },
    ]
}

/// The same two sides under `label < v` and no key.
fn nested_loop(join_type: JoinType) -> GpuNestedLoopJoin {
    GpuNestedLoopJoin::new(
        Given::input(BatchLayout::SingleBatch, &["k", "label"]),
        Given::input(BatchLayout::MultipleBatches, &["fk", "v"]),
        join_type,
        Expr::binary(
            Expr::column(0, "label"),
            BinaryOp::Lt,
            Expr::column(1, "v"),
            DataType::Boolean,
        ),
        label_and_v(),
        None,
        columns_of(&["k", "label", "fk", "v"]),
    )
}

fn cross(projection: Option<Vec<u32>>) -> GpuCrossJoin {
    let output = match &projection {
        Some(_) => columns_of(&["label", "v"]),
        None => columns_of(&["k", "label", "fk", "v"]),
    };
    GpuCrossJoin::new(
        Given::input(BatchLayout::SingleBatch, &["k", "label"]),
        Given::input(BatchLayout::MultipleBatches, &["fk", "v"]),
        projection,
        output,
    )
}

/// The recipe a join publishes and the buffer it wrote, alone in a writer that carries
/// `chunk_bytes`, with the count of fb nodes the buffer holds.
fn written_with(node: JoinNode<'_>, chunk_bytes: u64) -> (Recipe, Vec<u8>, Seq) {
    let mut writer = Writer::with_join_scratch(chunk_bytes);
    let recipe = join::join(
        node,
        &[&columns_of(&["k", "label"]), &columns_of(&["fk", "v"])],
        &mut writer,
    )
    .expect("the join's payload is writable")
    .expect("a join drives the ABI");
    let (bytes, wire_nodes) = writer.finish().expect("one root");
    (recipe, bytes, wire_nodes)
}

fn written(node: JoinNode<'_>) -> (Recipe, Vec<u8>, Seq) {
    written_with(node, 0)
}

/// The `CudfJoin` at the seq a recipe's calls address.
fn cudf_join<'a>(plan: &fb::GpuPlan<'a>, recipe: &Recipe) -> fb::CudfJoin<'a> {
    let seq = recipe.seqs()[0];
    node_at(plan, seq)
        .expect("the walk names every kind")
        .and_then(|node| node.node_as_cudf_join())
        .expect("a join's seq holds a CudfJoin")
}

fn field_names(schema: Option<fb::Schema<'_>>) -> Vec<String> {
    schema
        .and_then(|schema| schema.fields())
        .map(|fields| {
            fields
                .iter()
                .map(|f| f.name().unwrap_or("?").to_string())
                .collect()
        })
        .unwrap_or_default()
}

/// The three plan nodes write one table, which the session reads all of: the type, the
/// keys, the residual, both side schemas and the projection. A leaf, so the writer makes
/// no stub for it — the session is handed its tables, never a child.
#[test]
fn every_join_node_writes_one_cudf_join_leaf() {
    let hash = join(JoinType::Left, true, Some(vec![1, 3]));
    let nested = nested_loop(JoinType::Right);
    let crossed = cross(Some(vec![1, 3]));
    let cases: [(JoinNode<'_>, fb::JoinType, usize, bool, Option<Vec<u32>>); 3] = [
        (
            JoinNode::Hash(&hash),
            fb::JoinType::Left,
            1,
            true,
            Some(vec![1, 3]),
        ),
        (
            JoinNode::NestedLoop(&nested),
            fb::JoinType::Right,
            0,
            true,
            None,
        ),
        (
            JoinNode::Cross(&crossed),
            fb::JoinType::Inner,
            0,
            false,
            Some(vec![1, 3]),
        ),
    ];
    for (node, join_type, keys, filtered, projection) in cases {
        let (recipe, bytes, wire_nodes) = written(node);
        assert_eq!(
            wire_nodes, 1,
            "{join_type:?}: the join and no stub under it"
        );
        let plan = flatbuffers::root::<fb::GpuPlan>(&bytes).expect("the buffer verifies");
        let written = cudf_join(&plan, &recipe);
        assert_eq!(written.join_type(), join_type);
        assert_eq!(written.keys().map_or(0, |keys| keys.len()), keys);
        assert_eq!(written.filter().is_some(), filtered, "{join_type:?}");
        assert_eq!(
            written.filter_columns().map_or(0, |columns| columns.len()),
            if filtered { 2 } else { 0 },
            "{join_type:?}"
        );
        assert_eq!(field_names(written.build_schema()), vec!["k", "label"]);
        assert_eq!(field_names(written.probe_schema()), vec!["fk", "v"]);
        assert_eq!(
            written
                .projection()
                .map(|kept| kept.iter().collect::<Vec<u32>>()),
            projection,
            "{join_type:?}"
        );
        assert!(!written.null_equals_null(), "{join_type:?}");
    }
}

/// A hash join's set semantics reach the session: the flag is per node, and every type
/// honours it there.
#[test]
fn a_join_under_null_equals_null_carries_the_flag() {
    let node = GpuHashJoin::new(
        Given::input(BatchLayout::SingleBatch, &["k", "label"]),
        Given::input(BatchLayout::MultipleBatches, &["fk", "v"]),
        JoinType::LeftAnti,
        vec![(0, 0)],
        None,
        Vec::new(),
        true,
        None,
        columns_of(&["k", "label"]),
    );
    let (recipe, bytes, _) = written(JoinNode::Hash(&node));
    let plan = flatbuffers::root::<fb::GpuPlan>(&bytes).expect("the buffer verifies");
    assert!(cudf_join(&plan, &recipe).null_equals_null());
}

/// The session in the recipe line: built once, probed per batch, and finished only by the
/// five types that owe rows the last probe settles. One seq for all three calls.
#[test]
fn a_join_recipe_line_is_the_session() {
    let line = |node: JoinNode<'_>| written(node).0.to_string();
    let left = join(JoinType::Left, false, None);
    assert_eq!(
        line(JoinNode::Hash(&left)),
        "join_build(#0 CudfJoin); per probe batch: join_probe; at done: join_finish"
    );
    let inner = join(JoinType::Inner, false, None);
    assert_eq!(
        line(JoinNode::Hash(&inner)),
        "join_build(#0 CudfJoin); per probe batch: join_probe"
    );
    for join_type in [
        JoinType::Inner,
        JoinType::Left,
        JoinType::Right,
        JoinType::Full,
        JoinType::LeftSemi,
        JoinType::LeftAnti,
        JoinType::LeftMark,
        JoinType::RightSemi,
        JoinType::RightAnti,
    ] {
        let node = join(join_type, false, None);
        let (recipe, _, _) = written(JoinNode::Hash(&node));
        let finishes = recipe
            .calls
            .iter()
            .any(|call| call.symbol == AbiSymbol::JoinFinish);
        assert_eq!(
            finishes,
            crate::plan::needs_finish(join_type),
            "{join_type:?}"
        );
        assert!(
            recipe
                .calls
                .iter()
                .all(|call| call.target == Some((0, FbKind::Join))),
            "{join_type:?}: one seq, the CudfJoin's"
        );
    }
    let crossed = cross(None);
    assert_eq!(
        line(JoinNode::Cross(&crossed)),
        "join_build(#0 CudfJoin); per probe batch: join_probe",
        "a cross join is an Inner session"
    );
}

/// The session's chunk is the plan's budget: a sixteenth of it in the one mode that sets
/// one, capped at 1 GiB, and 0 — the session's own 1 GiB — everywhere else.
#[test]
fn a_budgeted_join_carries_its_chunk_budget() {
    let sized = crate::test_support::mode_named("tp4_sized").knobs_for("tpch");
    let single = crate::test_support::mode_named("tp1_single").knobs_for("tpch");
    assert_eq!(join_scratch_bytes(&sized), sized.budget / 16);
    assert!(join_scratch_bytes(&sized) > 0);
    assert_eq!(join_scratch_bytes(&single), 0);
    let huge = PlanKnobs {
        budget: 64 << 30,
        ..sized
    };
    assert_eq!(
        join_scratch_bytes(&huge),
        1 << 30,
        "never past the session's own default"
    );

    let node = join(JoinType::Inner, false, None);
    let (recipe, bytes, _) = written_with(JoinNode::Hash(&node), join_scratch_bytes(&sized));
    let plan = flatbuffers::root::<fb::GpuPlan>(&bytes).expect("the buffer verifies");
    assert_eq!(cudf_join(&plan, &recipe).chunk_bytes(), sized.budget / 16);
    let (recipe, bytes, _) = written(JoinNode::Hash(&node));
    let plan = flatbuffers::root::<fb::GpuPlan>(&bytes).expect("the buffer verifies");
    assert_eq!(cudf_join(&plan, &recipe).chunk_bytes(), 0);
}

// #249 — a type the wire's enum cannot name: the plan is not runnable on the device, said at
// plan time with the side and the column, where a `Null` type would have reached the session
// and been refused only on a lane that had no probe batch.
#[test]
fn bug_a_join_side_holding_a_list_column_is_not_runnable() {
    let listed = Schema::new(Arc::new(ArrowSchema::new(vec![
        Field::new("fk", DataType::Int64, true),
        Field::new(
            "tags",
            DataType::List(Arc::new(Field::new("item", DataType::Int32, true))),
            true,
        ),
    ])));
    let probe: Box<dyn GpuNode> = Box::new(Given {
        kind: NodeKind::Intermediate {
            layout: PartitionLayout {
                batch_layout: BatchLayout::MultipleBatches,
                ..PartitionLayout::new(1)
            },
            schema: listed,
        },
    });
    let node = GpuHashJoin::new(
        Given::input(BatchLayout::SingleBatch, &["k", "label"]),
        probe,
        JoinType::Inner,
        vec![(0, 0)],
        None,
        Vec::new(),
        false,
        Some(vec![0, 2]),
        columns_of(&["k", "fk"]),
    );
    let refused = attach_recipes(&node, 0)
        .err()
        .expect("the wire cannot type a list")
        .to_string();
    assert!(refused.contains("probe side"), "{refused}");
    assert!(refused.contains("column tags"), "{refused}");
    assert!(refused.contains("(#249)"), "{refused}");
}

/// Seqs are the post-order positions of the tree that was built. A join session's calls
/// share its one seq, and seqs run on across the plan rather than restarting per node.
#[test]
fn seqs_are_the_post_order_positions_of_what_was_built() {
    let mut writer = writer();
    let build = columns_of(&["k", "label"]);
    let probe = columns_of(&["fk", "v"]);
    let left = join(JoinType::Left, false, None);
    let first = join::join(JoinNode::Hash(&left), &[&build, &probe], &mut writer)
        .expect("writable")
        .expect("a join drives the ABI");
    let inner = join(JoinType::Inner, false, None);
    let second = join::join(JoinNode::Hash(&inner), &[&build, &probe], &mut writer)
        .expect("writable")
        .expect("a join drives the ABI");
    assert_eq!(first.seqs(), vec![0, 0, 0]);
    assert_eq!(second.seqs(), vec![1, 1]);
}

/// A join over two leaves of its own, attached as a plan: the leaves' stubs stay in the
/// pool beside the join, which takes neither, and the walk gathers all three under one
/// structural union — so the stubs are indexed and the join's seq is the one after them.
#[test]
fn a_join_leaves_its_inputs_for_the_walk_to_gather() {
    let plan = attach_recipes(&join(JoinType::Inner, false, None), 0)
        .expect("a join over two given leaves is writable");
    assert_eq!(
        plan.get(2).expect("the join's recipe").seqs(),
        vec![2, 2],
        "two stubs, then the join"
    );
    assert_eq!(
        plan.wire_nodes(),
        4,
        "two stubs, the join and the union over them"
    );
    check_seq_kinds(&plan).expect("every published seq holds the kind it claims");
}

/// Every cell of both dimensions — nine join types crossed with a residual filter — and
/// one claim about each: the recipe finishes exactly where the session does, and DataFusion
/// answers rows at the finish exactly there too.
#[test]
fn the_recipe_and_the_executor_take_the_same_path_through_every_cell() {
    let mut cells = 0;
    for join_type in ALL_TYPES {
        for residual in [false, true] {
            let node = typed_join(join_type, residual);
            let executor_finishes = cpu_finishes(&node);
            let recipe = recipe_for(&node);
            let recipe_finishes = recipe
                .calls
                .iter()
                .any(|call| call.when == CallPattern::AtDone);
            assert_eq!(
                recipe_finishes,
                crate::plan::needs_finish(join_type),
                "{join_type:?} with residual={residual}: {recipe}"
            );
            cells += 1;
            assert_eq!(
                recipe_finishes,
                executor_finishes.expect("every cell is one the executor builds"),
                "{join_type:?} with residual={residual}: the recipe and the cpu executor \
                 disagree about the finish"
            );
        }
    }
    assert_eq!(
        cells, 18,
        "nine types by two residuals, and every cell answered"
    );
}

/// The filtered build-side semi family answers at the finish on the cpu, as the session
/// does and as it does unfiltered (#220).
#[test]
fn the_cpu_finishes_a_filtered_build_side_semi_join_as_the_session_does() {
    for join_type in [JoinType::LeftSemi, JoinType::LeftAnti, JoinType::LeftMark] {
        let node = typed_join(join_type, true);
        let recipe = recipe_for(&node);
        assert!(
            recipe
                .calls
                .iter()
                .any(|call| call.when == CallPattern::AtDone),
            "{join_type:?}: the session finishes"
        );
        assert!(
            cpu_finishes(&node).expect("the cpu builds it"),
            "{join_type:?}: the cpu finishes"
        );
    }
}

const ALL_TYPES: [JoinType; 9] = [
    JoinType::Inner,
    JoinType::Left,
    JoinType::Right,
    JoinType::Full,
    JoinType::LeftSemi,
    JoinType::LeftAnti,
    JoinType::LeftMark,
    JoinType::RightSemi,
    JoinType::RightAnti,
];

/// [`join`] declaring what its type emits, since these cells run the cpu executor, which
/// answers under the node's declaration and refuses one of another width.
fn typed_join(join_type: JoinType, filter: bool) -> GpuHashJoin {
    let emitted = match join_type {
        JoinType::Inner | JoinType::Left | JoinType::Right | JoinType::Full => {
            columns_of(&["k", "label", "fk", "v"])
        }
        JoinType::LeftSemi | JoinType::LeftAnti => columns_of(&["k", "label"]),
        JoinType::LeftMark => Schema::new(Arc::new(ArrowSchema::new(vec![
            Field::new("k", DataType::Int64, true),
            Field::new("label", DataType::Int64, true),
            Field::new("mark", DataType::Boolean, false),
        ]))),
        JoinType::RightSemi | JoinType::RightAnti => columns_of(&["fk", "v"]),
    };
    declaring(join_type, filter, None, emitted)
}

fn recipe_for(node: &GpuHashJoin) -> Recipe {
    written(JoinNode::Hash(node)).0
}

/// `(kind, when)` per call, which is the pair the mapping table states.
fn shape(recipe: &Recipe) -> Vec<(FbKind, CallPattern)> {
    recipe
        .calls
        .iter()
        .map(|call| {
            (
                call.target.expect("a recipe call here addresses a seq").1,
                call.when,
            )
        })
        .collect()
}

/// Whether DataFusion answers rows at `node`'s finish over `dim` = (1, 0), (2, 0) and one
/// `fact` batch, (1, 10): one build row matched with the residual true, one matched by
/// nothing, so each type that finishes has a row to answer there and no other type does.
fn cpu_finishes(node: &GpuHashJoin) -> Result<bool, crate::plan::PlanError> {
    let int64 = |values: &[i64]| Arc::new(Int64Array::from(values.to_vec())) as ArrayRef;
    let build = RecordBatch::try_from_iter([("k", int64(&[1, 2])), ("label", int64(&[0, 0]))])
        .expect("two columns of one length");
    let probe = RecordBatch::try_from_iter([("fk", int64(&[1])), ("v", int64(&[10]))])
        .expect("two columns of one length");
    crate::executor::has_finish_pass(
        node,
        build,
        probe,
        datafusion::execution::context::SessionContext::new().task_ctx(),
    )
}

#[test]
fn an_accumulating_sort_sorts_every_batch_and_merges_what_the_lane_kept() {
    let node = GpuAccumulateBatchesAndSort::new(
        Given::input(BatchLayout::MultipleBatches, &["a"]),
        vec![ColumnOrder {
            column: 0,
            ascending: true,
            nulls_first: false,
        }],
        Some(10),
    );
    let recipe = accumulate_and_sort(&node, &[&columns_of(&["a"])], &mut writer())
        .expect("the sort's payloads are writable")
        .expect("an accumulating sort drives the ABI");
    assert_eq!(
        recipe
            .calls
            .iter()
            .map(|call| (call.target.unwrap().1, call.when, call.inputs.clone()))
            .collect::<Vec<_>>(),
        vec![
            (FbKind::Sort, CallPattern::PerBatch, vec![Input::Batch]),
            (
                FbKind::SortPreservingMerge,
                CallPattern::AtDone,
                vec![Input::LaneBatches]
            ),
        ]
    );
}

#[test]
fn a_batch_aggregate_runs_the_same_pair_at_a_compaction_and_at_done() {
    let body = AggregateBody {
        group_by: vec![Expr::column(0, "k")],
        grouping_sets: Vec::new(),
        null_exprs: Vec::new(),
        aggs: vec![AggCall {
            func: PlanAgg::Sum,
            args: vec![Expr::column(1, "n")],
            outputs: vec![Field::new("sum(n)", DataType::Int64, true)],
        }],
        finalize: None,
    };
    let node = GpuAggregateBatches::new(
        Given::input(BatchLayout::MultipleBatches, &["k", "n"]),
        body,
        columns_of(&["k", "sum(n)"]),
        columns_of(&["k", "sum(n)"]),
    );
    let recipe = aggregate_batches(&node, &[&columns_of(&["k", "n"])], &mut writer())
        .expect("the aggregate's payloads are writable")
        .expect("a batch aggregate drives the ABI");
    assert_eq!(
        shape(&recipe),
        vec![
            (FbKind::CoalescePartitions, CallPattern::PerCompaction),
            (
                FbKind::Aggregate { merge: true },
                CallPattern::PerCompaction
            ),
        ],
        "the compaction is the done pass run early, which is what makes the threshold a \
         scheduling decision"
    );
    assert_eq!(recipe.calls[1].inputs, vec![Input::PriorOutput]);
}

/// A finalizing merge adds the project that carries the finalize — ours, so the two
/// engines evaluate the same expression rather than agreeing because two implementations
/// happen to match.
/// One key and one summed column, finalized: the shape the three tests below read, so the
/// list a finalize carries and the row a project has to emit differ by exactly the key.
fn finalizing_merge(finalize: Vec<NamedExpr>, state: &[&str]) -> GpuAggregateBatches {
    GpuAggregateBatches::new(
        Given::input(BatchLayout::MultipleBatches, &["k", "n"]),
        AggregateBody {
            group_by: vec![Expr::column(0, "k")],
            grouping_sets: Vec::new(),
            null_exprs: Vec::new(),
            aggs: vec![AggCall {
                func: PlanAgg::Sum,
                args: vec![Expr::column(1, "n")],
                outputs: vec![Field::new("sum(n)", DataType::Int64, true)],
            }],
            finalize: Some(finalize),
        },
        columns_of(state),
        columns_of(&["k", "sum(n)"]),
    )
}

fn summed() -> Vec<NamedExpr> {
    vec![NamedExpr::new(Expr::column(1, "sum(n)"), "sum(n)")]
}

#[test]
fn a_finalizing_merge_carries_its_finalize_in_a_project_of_its_own() {
    let node = finalizing_merge(summed(), &["k", "sum(n)"]);
    let recipe = aggregate_batches(&node, &[&columns_of(&["k", "n"])], &mut writer())
        .expect("the aggregate's payloads are writable")
        .expect("a batch aggregate drives the ABI");
    assert_eq!(
        shape(&recipe),
        vec![
            (FbKind::CoalescePartitions, CallPattern::PerCompaction),
            (
                FbKind::Aggregate { merge: true },
                CallPattern::PerCompaction
            ),
            (FbKind::Project(ProjectRole::Finalize), CallPattern::AtDone),
        ],
        "the merge runs per compaction and the finalize once, at done"
    );
}

/// The keys are the half the finalize list does not carry, and a project replaces the row:
/// finalized columns alone answer with values and nothing to read them by. Asserted on the
/// buffer rather than on the recipe, since the recipe names the call and not its payload.
#[test]
fn a_finalize_project_emits_the_group_keys_the_finalize_list_leaves_out() {
    let node = finalizing_merge(summed(), &["k", "sum(n)"]);
    let mut writer = writer();
    let recipe = aggregate_batches(&node, &[&columns_of(&["k", "n"])], &mut writer)
        .expect("the aggregate's payloads are writable")
        .expect("a batch aggregate drives the ABI");
    let (bytes, _) = writer.finish().expect("one root");
    let plan = flatbuffers::root::<fb::GpuPlan>(&bytes).expect("the buffer verifies");
    let seq = *recipe.seqs().last().expect("the finalize is the last call");
    let project = node_at(&plan, seq)
        .expect("the walk names every kind")
        .and_then(|node| node.node_as_cudf_project())
        .expect("the last seq is the finalize project");
    let aliases: Vec<&str> = project.aliases().expect("named columns").iter().collect();
    assert_eq!(
        aliases,
        vec!["k", "sum(n)"],
        "the project has to emit the node's whole declared output"
    );
}

/// The keys are taken from the state by position, so a state whose first columns are not
/// the keys fills the row to the right width out of the wrong columns. Held by
/// `GpuAggregate::intermediate`'s documented order today, and by this if that ever moves.
#[test]
fn a_state_whose_first_columns_are_not_the_keys_is_refused() {
    let node = finalizing_merge(summed(), &["j", "sum(n)"]);
    let refused = aggregate_batches(&node, &[&columns_of(&["k", "n"])], &mut writer())
        .expect_err("a state that does not lead with the keys is not projectable");
    let message = format!("{refused}");
    assert!(
        message.contains("column 0 `k`") && message.contains("names it `j`"),
        "the refusal has to name the two spellings: {message}"
    );
}

/// The width check, shown going red: a finalize list that does not account for every
/// non-key output column is a plan that answers with a column missing, which is a wrong
/// answer rather than an error unless this refuses.
#[test]
fn a_finalize_that_does_not_fill_the_declared_output_is_refused() {
    let node = finalizing_merge(Vec::new(), &["k", "sum(n)"]);
    let refused = aggregate_batches(&node, &[&columns_of(&["k", "n"])], &mut writer())
        .expect_err("a finalize short of the declared output is not writable");
    let message = format!("{refused}");
    assert!(
        message.contains("1 keys and 0 columns") && message.contains("declares 2 output"),
        "the refusal has to name both counts: {message}"
    );
}

/// The one literal the wire cannot carry, in a node with an input — so the walk has
/// something to lose. #168 is the corpus case (`mixed-join`'s residual adds an interval to
/// a column), and the failure mode it guards is structural: a placeholder that dropped the
/// inputs its node had taken would leave them unreachable, and the tree would then be
/// shorter than the numbering.
///
/// A leaf would prove nothing here — the counts agree either way when nothing was taken.
fn filter_over_scan_with_an_interval() -> GpuFilter {
    let scan = crate::plan::ScanMetadata {
        file: "/orders.parquet".to_string(),
        groups: vec![crate::plan::RowGroupMeta {
            index: 0,
            rows: 10,
            bytes: 100,
        }],
        can_be_null: vec![false],
    };
    let load = crate::plan::GpuLoadParquet::new(
        "orders".to_string(),
        vec![0],
        vec![vec![vec![0]]],
        &scan,
        columns_of(&["o_orderdate"]),
    );
    let ninety_days = ScalarValue::IntervalMonthDayNano(Some(
        datafusion::arrow::datatypes::IntervalMonthDayNano::new(0, 90, 0),
    ));
    GpuFilter::new(
        Box::new(load),
        Expr::binary(
            Expr::column(0, "o_orderdate"),
            BinaryOp::Lt,
            Expr::Literal(ninety_days),
            DataType::Boolean,
        ),
        None,
        columns_of(&["o_orderdate"]),
    )
}

#[test]
fn a_payload_the_wire_cannot_carry_fails_the_plan_and_names_where() {
    let refused = attach_recipes(&filter_over_scan_with_an_interval(), 0)
        .err()
        .expect("the wire has no interval, so this plan cannot be built");
    let said = refused.to_string();
    // The reason, its ticket, and the seq the node would have taken — the three things a
    // reader of the golden line needs, and the assertion that keeps the Err arm real: it
    // would go quietly green the day the writer learns intervals, having stopped testing
    // anything, so the message is checked rather than merely the failure.
    assert!(said.contains("scalar value"), "{said}");
    assert!(said.contains("(#168)"), "{said}");
    assert!(said.contains(" at #"), "{said}");
}

/// A leaf the registry does not know — the operator harness's stub — emits no seq, so the
/// operator above it is the only wire node and the recipe index is the operator's
/// post-order. A filter takes one child, so the writer stubs the slot: two wire nodes,
/// one recipe. `crate::tests::given::Given` by path: this file's `Given` is a join
/// side over Int64 columns, a different shape.
#[test]
fn a_leaf_outside_the_registry_emits_no_seq_and_its_parent_takes_a_stub() {
    let input = crate::tests::given::columns(&[("k", DataType::Utf8), ("v", DataType::Int64)]);
    let filter = GpuFilter::new(
        crate::tests::given::Given::of(input.clone(), BatchLayout::MultipleBatches),
        Expr::binary(
            Expr::column(1, "v"),
            BinaryOp::Gt,
            Expr::Literal(ScalarValue::Int64(Some(1))),
            DataType::Boolean,
        ),
        None,
        input,
    );
    let plan = attach_recipes(&filter, 0).expect("a filter over a given leaf is writable");
    assert!(plan.get(0).is_none(), "the leaf has no recipe");
    let recipe = plan.get(1).expect("the filter has one");
    assert_eq!(recipe.calls.len(), 1);
    assert_eq!(recipe.calls[0].target.map(|(seq, _)| seq), Some(1));
    assert_eq!(plan.wire_nodes(), 2, "the stub and the filter");
}

/// The seqless three put nothing on the wire themselves, so the leaf's stub is the whole
/// plan: the writer refuses a plan with no root, and so does the C++. A limit over a given
/// leaf is one stub node with one bare-call recipe.
#[test]
fn a_seqless_operator_over_a_given_leaf_is_a_plan_of_one_stub() {
    let input = crate::tests::given::columns(&[("k", DataType::Utf8)]);
    let limit = GpuLimit::new(
        crate::tests::given::Given::of(input, BatchLayout::MultipleBatches),
        RowInterval {
            skip: 1,
            fetch: Some(2),
        },
    );
    let plan = attach_recipes(&limit, 0).expect("writable");
    assert!(plan.get(0).is_none());
    assert_eq!(
        plan.get(1).expect("the limit's").calls[0].symbol,
        AbiSymbol::SliceHandle
    );
    assert_eq!(plan.wire_nodes(), 1, "the stub, and nothing else");
}

/// A relation DataFusion proved empty calls nothing, but it holds its slot on the wire with the
/// stub a parent would have taken, so a plan of nothing else still has a root: `SELECT …
/// WHERE false` plans as an unload over the empty relation alone.
#[test]
fn an_empty_relation_is_a_stub_with_no_recipe() {
    let empty = GpuEmpty::new(columns_of(&["k"]));
    let plan = attach_recipes(&GpuUnload::new(Box::new(empty), None), 0)
        .expect("an unload over an empty relation is writable");
    assert!(plan.get(0).is_none(), "the empty relation makes no call");
    assert_eq!(plan.wire_nodes(), 1, "its stub, and nothing else");
}

/// The Rust child walk is total, as `NodeSession::node_children` is: every kind the schema
/// declares is named, so a kind added to `gpu_plan.fbs` and forgotten in `wire/read.rs` fails
/// here rather than renumbering every seq above it. An empty table verifies as any kind, the
/// schema having no required field.
#[test]
fn the_child_walk_names_every_node_kind() {
    for kind in fb::PlanNodeKind::ENUM_VALUES {
        let mut b = flatbuffers::FlatBufferBuilder::new();
        let table = b.start_table();
        let table = b.end_table(table);
        let node = fb::PlanNode::create(
            &mut b,
            &fb::PlanNodeArgs {
                node_type: *kind,
                // A node of no kind carries no table; the verifier refuses one that does.
                node: (*kind != fb::PlanNodeKind::NONE).then(|| table.as_union_value()),
            },
        );
        b.finish(node, None);
        let node = flatbuffers::root::<fb::PlanNode>(b.finished_data()).expect("it verifies");
        let walked = children(&node);
        if *kind == fb::PlanNodeKind::NONE {
            assert!(walked.is_err(), "a node with no kind is not a leaf");
        } else {
            assert!(
                walked.is_ok(),
                "{kind:?} is missing from the child walk: {walked:?}"
            );
        }
    }
}
