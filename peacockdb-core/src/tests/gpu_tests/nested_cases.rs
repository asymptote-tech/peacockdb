//! `GpuCrossJoin` and `GpuNestedLoopJoin` through the harness: a build batch first and
//! once, the probe streamed — the finishing types answer what they owe at finish — over
//! the same two prefixed synthetic sides the hash join uses, a nested loop of every type.
//! Every case is green or a `bug_` test with its ticket above it; nothing here repairs.

use std::sync::Arc;

use datafusion::arrow::datatypes::{DataType, Field, Schema as ArrowSchema};
use datafusion::arrow::record_batch::RecordBatch;
use datafusion::common::JoinType;

use super::join_cases::output_of;
use super::script::{Outcome, Script, run_both};
use crate::plan::{
    BatchLayout, BinaryOp, Expr, GpuCrossJoin, GpuNestedLoopJoin, GpuNode, JoinFilterColumn,
    JoinSide, Schema,
};
use crate::tests::compare::Order;
use crate::tests::given::Given;
use crate::tests::synthetic::{prefixed, synthetic};

fn build_batch(rows: usize) -> RecordBatch {
    prefixed(&synthetic(rows, 11), "b_")
}

fn probe_batch(rows: usize, seed: u64) -> RecordBatch {
    prefixed(&synthetic(rows, seed), "p_")
}

fn side(prefix: &str) -> Vec<Field> {
    prefixed(&synthetic(0, 0), prefix)
        .schema()
        .fields()
        .iter()
        .map(|f| f.as_ref().clone())
        .collect()
}

fn output(fields: Vec<Field>, projection: &Option<Vec<u32>>) -> Schema {
    let fields = match projection {
        None => fields,
        Some(keep) => keep.iter().map(|i| fields[*i as usize].clone()).collect(),
    };
    Schema::new(Arc::new(ArrowSchema::new(fields)))
}

fn leaf(prefix: &str, batches: BatchLayout) -> Box<dyn GpuNode> {
    Given::of(
        Schema::new(Arc::new(ArrowSchema::new(side(prefix)))),
        batches,
    )
}

pub(crate) fn cross(projection: Option<Vec<u32>>) -> GpuCrossJoin {
    let output = output(output_of(JoinType::Inner), &projection);
    GpuCrossJoin::new(
        leaf("b_", BatchLayout::SingleBatch),
        leaf("p_", BatchLayout::MultipleBatches),
        projection,
        output,
    )
}

/// `b_i64 < p_i64` over the filter's own schema, `[build columns…, probe columns…]` as
/// `filter_columns` lists them: 0 is the build's `i64`, 1 the probe's.
fn residual() -> (Expr, Vec<JoinFilterColumn>) {
    (
        Expr::binary(
            Expr::column(0, "b_i64"),
            BinaryOp::Lt,
            Expr::column(1, "p_i64"),
            DataType::Boolean,
        ),
        vec![
            JoinFilterColumn {
                side: JoinSide::Build,
                index: 3,
            },
            JoinFilterColumn {
                side: JoinSide::Probe,
                index: 3,
            },
        ],
    )
}

/// `CAST(b_i64 AS DECIMAL(20, 0)) > CAST(p_i64 AS DECIMAL(20, 0))`: a decimal operand is
/// what `cudf_ast_can_evaluate` refuses, so this predicate takes the cross-then-mask path.
fn decimal_residual() -> (Expr, Vec<JoinFilterColumn>) {
    let dec = |i: u32, name: &str| Expr::Cast {
        expr: Box::new(Expr::column(i, name)),
        target: DataType::Decimal128(20, 0),
    };
    (
        Expr::binary(
            dec(0, "b_i64"),
            BinaryOp::Gt,
            dec(1, "p_i64"),
            DataType::Boolean,
        ),
        vec![
            JoinFilterColumn {
                side: JoinSide::Build,
                index: 3,
            },
            JoinFilterColumn {
                side: JoinSide::Probe,
                index: 3,
            },
        ],
    )
}

/// `b_s LIKE p_s`: each build string matched against the probe row's string as a pattern —
/// a pattern that is a column, which the device's `LIKE` does not take.
fn like_column_residual() -> (Expr, Vec<JoinFilterColumn>) {
    (
        Expr::Like {
            expr: Box::new(Expr::column(0, "b_s")),
            pattern: Box::new(Expr::column(1, "p_s")),
            negated: false,
            case_insensitive: false,
        },
        vec![
            JoinFilterColumn {
                side: JoinSide::Build,
                index: 5,
            },
            JoinFilterColumn {
                side: JoinSide::Probe,
                index: 5,
            },
        ],
    )
}

/// `b_key <op> p_key`, where `key` is NULL on every eleventh row of either side, so all four
/// arms of `IS [NOT] DISTINCT FROM` occur. `cast` puts both operands under a decimal, which
/// sends the device down the cross-then-mask path instead of the AST.
fn distinct_residual(op: BinaryOp, cast: bool) -> (Expr, Vec<JoinFilterColumn>) {
    let operand = |i: u32, name: &str| {
        let column = Expr::column(i, name);
        if !cast {
            return column;
        }
        Expr::Cast {
            expr: Box::new(column),
            target: DataType::Decimal128(20, 0),
        }
    };
    (
        Expr::binary(
            operand(0, "b_key"),
            op,
            operand(1, "p_key"),
            DataType::Boolean,
        ),
        vec![
            JoinFilterColumn {
                side: JoinSide::Build,
                index: 1,
            },
            JoinFilterColumn {
                side: JoinSide::Probe,
                index: 1,
            },
        ],
    )
}

/// A nested loop of any type, its probe streamed.
fn nested_with(
    join_type: JoinType,
    predicate: (Expr, Vec<JoinFilterColumn>),
    projection: Option<Vec<u32>>,
) -> GpuNestedLoopJoin {
    let (filter, filter_columns) = predicate;
    let output = output(output_of(join_type), &projection);
    GpuNestedLoopJoin::new(
        leaf("b_", BatchLayout::SingleBatch),
        leaf("p_", BatchLayout::MultipleBatches),
        join_type,
        filter,
        filter_columns,
        projection,
        output,
    )
}

fn nested(join_type: JoinType, projection: Option<Vec<u32>>) -> GpuNestedLoopJoin {
    nested_with(join_type, residual(), projection)
}

pub(crate) fn inner(projection: Option<Vec<u32>>) -> GpuNestedLoopJoin {
    nested(JoinType::Inner, projection)
}

pub(crate) fn left(projection: Option<Vec<u32>>) -> GpuNestedLoopJoin {
    nested(JoinType::Left, projection)
}

fn script(build: Option<RecordBatch>, probe: Vec<RecordBatch>) -> Script {
    Script::Join { build, probe }
}

pub(crate) fn one_probe() -> Script {
    script(Some(build_batch(16)), vec![probe_batch(16, 21)])
}

pub(crate) fn two_probes() -> Script {
    script(
        Some(build_batch(16)),
        vec![probe_batch(12, 21), probe_batch(12, 22)],
    )
}

// The empty shapes, one script each; a case names the join and the shape.

fn empty_build() -> Script {
    script(Some(build_batch(0)), vec![probe_batch(16, 21)])
}

fn empty_probe() -> Script {
    script(Some(build_batch(16)), vec![probe_batch(0, 21)])
}

fn both_empty() -> Script {
    script(Some(build_batch(0)), vec![probe_batch(0, 21)])
}

fn no_build() -> Script {
    script(None, Vec::new())
}

// The `bug_` assertion: a refusal pinned by the stable part of its message.

fn gpu_refuses_with(outcome: &Outcome, message: &str) {
    let why = outcome.gpu_refuses();
    assert!(why.contains(message), "{why}");
}

// The cross join: the product of the two sides, every column of both.

operator_case! {
    GpuCrossJoin,
    fn a_cross_join_is_the_product_on_both() {
        run_both(&cross(None), one_probe()).same(Order::Any);
    }
}

operator_case! {
    GpuCrossJoin,
    fn a_cross_join_over_two_probe_batches_agrees() {
        run_both(&cross(None), two_probes()).same(Order::Any);
    }
}

operator_case! {
    GpuCrossJoin,
    fn a_cross_join_with_a_projection_agrees() {
        run_both(&cross(Some(vec![0, 8])), one_probe()).same(Order::Any);
    }
}

// #208 — over a zero-row build both engines owe nothing and drain; the executor's own
// answer, one zero-row batch, is `cpu_backend::tests::join`'s.
operator_case! {
    GpuCrossJoin,
    fn a_cross_join_over_a_zero_row_build_agrees() {
        run_both(&cross(None), empty_build()).same(Order::Any);
    }
}

operator_case! {
    GpuCrossJoin,
    fn a_cross_join_over_a_zero_row_probe_is_zero_rows() {
        run_both(&cross(None), empty_probe()).same(Order::Any);
    }
}

operator_case! {
    GpuCrossJoin,
    fn a_cross_join_over_both_sides_empty_agrees() {
        run_both(&cross(None), both_empty()).same(Order::Any);
    }
}

operator_case! {
    GpuCrossJoin,
    fn a_cross_join_with_no_build_batch_is_never_probed() {
        run_both(&cross(None), no_build()).same(Order::Any);
    }
}

// The Inner nested loop: the predicate decides each pair, and the probe streams.

operator_case! {
    GpuNestedLoopJoin,
    fn an_inner_nested_loop_join_agrees() {
        run_both(&inner(None), one_probe()).same(Order::Any);
    }
}

operator_case! {
    GpuNestedLoopJoin,
    fn an_inner_nested_loop_join_over_two_probe_batches_agrees() {
        run_both(&inner(None), two_probes()).same(Order::Any);
    }
}

operator_case! {
    GpuNestedLoopJoin,
    fn an_inner_nested_loop_join_with_a_projection_agrees() {
        run_both(&inner(Some(vec![0, 8])), one_probe()).same(Order::Any);
    }
}

operator_case! {
    GpuNestedLoopJoin,
    fn an_inner_nested_loop_join_over_a_zero_row_build_is_zero_rows() {
        run_both(&inner(None), empty_build()).same(Order::Any);
    }
}

operator_case! {
    GpuNestedLoopJoin,
    fn an_inner_nested_loop_join_over_a_zero_row_probe_is_zero_rows() {
        run_both(&inner(None), empty_probe()).same(Order::Any);
    }
}

operator_case! {
    GpuNestedLoopJoin,
    fn an_inner_nested_loop_join_with_no_build_batch_is_never_probed() {
        run_both(&inner(None), no_build()).same(Order::Any);
    }
}

// The Left nested loop: one probe batch, the unmatched build rows padded at finish.

operator_case! {
    GpuNestedLoopJoin,
    fn a_left_nested_loop_join_agrees() {
        run_both(&left(None), one_probe()).same(Order::Any);
    }
}

operator_case! {
    GpuNestedLoopJoin,
    fn a_left_nested_loop_join_with_a_projection_agrees() {
        run_both(&left(Some(vec![0, 8])), one_probe()).same(Order::Any);
    }
}

operator_case! {
    GpuNestedLoopJoin,
    fn a_left_nested_loop_join_over_a_zero_row_build_agrees() {
        run_both(&left(None), empty_build()).same(Order::Any);
    }
}

operator_case! {
    GpuNestedLoopJoin,
    fn a_left_nested_loop_join_over_a_zero_row_probe_pads_every_build_row() {
        run_both(&left(None), empty_probe()).same(Order::Any);
    }
}

operator_case! {
    GpuNestedLoopJoin,
    fn a_left_nested_loop_join_with_no_build_batch_is_never_probed() {
        run_both(&left(None), no_build()).same(Order::Any);
    }
}

// The seven types beyond Inner and Left (#160), each over two probe batches: the build-side
// semi family and Full answer the build rows at finish, Right, RightSemi and RightAnti each
// probe row in its own call.

operator_case! {
    GpuNestedLoopJoin,
    fn a_right_nested_loop_join_over_two_probe_batches_agrees() {
        run_both(&nested(JoinType::Right, None), two_probes()).same(Order::Any);
    }
}

operator_case! {
    GpuNestedLoopJoin,
    fn a_full_nested_loop_join_over_two_probe_batches_agrees() {
        run_both(&nested(JoinType::Full, None), two_probes()).same(Order::Any);
    }
}

operator_case! {
    GpuNestedLoopJoin,
    fn a_left_semi_nested_loop_join_over_two_probe_batches_agrees() {
        run_both(&nested(JoinType::LeftSemi, None), two_probes()).same(Order::Any);
    }
}

operator_case! {
    GpuNestedLoopJoin,
    fn a_left_anti_nested_loop_join_over_two_probe_batches_agrees() {
        run_both(&nested(JoinType::LeftAnti, None), two_probes()).same(Order::Any);
    }
}

operator_case! {
    GpuNestedLoopJoin,
    fn a_left_mark_nested_loop_join_over_two_probe_batches_agrees() {
        run_both(&nested(JoinType::LeftMark, None), two_probes()).same(Order::Any);
    }
}

operator_case! {
    GpuNestedLoopJoin,
    fn a_right_semi_nested_loop_join_over_two_probe_batches_agrees() {
        run_both(&nested(JoinType::RightSemi, None), two_probes()).same(Order::Any);
    }
}

operator_case! {
    GpuNestedLoopJoin,
    fn a_right_anti_nested_loop_join_over_two_probe_batches_agrees() {
        run_both(&nested(JoinType::RightAnti, None), two_probes()).same(Order::Any);
    }
}

// The predicate the AST cannot take: a decimal operand sends the device down the
// cross-then-mask path, whose mask columns are read in `filter_columns` order.

operator_case! {
    GpuNestedLoopJoin,
    fn an_inner_nested_loop_join_with_a_decimal_predicate_agrees() {
        let node = nested_with(JoinType::Inner, decimal_residual(), None);
        run_both(&node, one_probe()).same(Order::Any);
    }
}

// The device applies the node's projection after the mask.
operator_case! {
    GpuNestedLoopJoin,
    fn an_inner_nested_loop_join_with_a_decimal_predicate_and_a_projection_agrees() {
        let node = nested_with(JoinType::Inner, decimal_residual(), Some(vec![13, 4, 8]));
        run_both(&node, one_probe()).same(Order::Any);
    }
}

operator_case! {
    GpuNestedLoopJoin,
    fn a_left_nested_loop_join_with_a_decimal_predicate_agrees() {
        let node = nested_with(JoinType::Left, decimal_residual(), None);
        run_both(&node, one_probe()).same(Order::Any);
    }
}

operator_case! {
    GpuNestedLoopJoin,
    fn a_left_nested_loop_join_with_a_decimal_predicate_and_a_projection_agrees() {
        let node = nested_with(JoinType::Left, decimal_residual(), Some(vec![13, 4, 8]));
        run_both(&node, one_probe()).same(Order::Any);
    }
}

// IS [NOT] DISTINCT FROM as the whole condition (review row 9), on either path. The planner
// makes a pair of bare columns a hash key; an expression operand or IS DISTINCT FROM stays a
// loop like these.

operator_case! {
    GpuNestedLoopJoin,
    fn a_full_nested_loop_join_on_is_not_distinct_from_agrees() {
        let node = nested_with(JoinType::Full, distinct_residual(BinaryOp::IsNotDistinctFrom, false), None);
        run_both(&node, two_probes()).same(Order::Any);
    }
}

operator_case! {
    GpuNestedLoopJoin,
    fn a_left_nested_loop_join_on_is_distinct_from_agrees() {
        let node = nested_with(JoinType::Left, distinct_residual(BinaryOp::IsDistinctFrom, false), None);
        run_both(&node, two_probes()).same(Order::Any);
    }
}

operator_case! {
    GpuNestedLoopJoin,
    fn an_inner_nested_loop_join_on_is_not_distinct_from_over_decimals_agrees() {
        let node = nested_with(JoinType::Inner, distinct_residual(BinaryOp::IsNotDistinctFrom, true), None);
        run_both(&node, two_probes()).same(Order::Any);
    }
}

operator_case! {
    GpuNestedLoopJoin,
    fn an_inner_nested_loop_join_on_is_distinct_from_over_decimals_agrees() {
        let node = nested_with(JoinType::Inner, distinct_residual(BinaryOp::IsDistinctFrom, true), None);
        run_both(&node, one_probe()).same(Order::Any);
    }
}

// #246 — a LIKE whose pattern is a column: the cpu answers, the device refuses at
// `expr.cpp`'s guard.
operator_case! {
    GpuNestedLoopJoin,
    fn bug_a_like_against_a_column_pattern_is_refused_on_the_device() {
        let node = nested_with(JoinType::Inner, like_column_residual(), None);
        gpu_refuses_with(&run_both(&node, one_probe()), "LIKE pattern must be a string literal");
    }
}

// The path's empty shapes, Inner only.

operator_case! {
    GpuNestedLoopJoin,
    fn an_inner_nested_loop_join_with_a_decimal_predicate_over_an_empty_build_owes_nothing() {
        let node = nested_with(JoinType::Inner, decimal_residual(), None);
        run_both(&node, empty_build()).same(Order::Any);
    }
}

operator_case! {
    GpuNestedLoopJoin,
    fn an_inner_nested_loop_join_with_a_decimal_predicate_over_an_empty_probe_answers_nothing() {
        let node = nested_with(JoinType::Inner, decimal_residual(), None);
        run_both(&node, empty_probe()).same(Order::Any);
    }
}

// #63 — a side of rows and no column is `[__rowmarker__]`, and a cross join of two such sides
// keeps one marker: its projection names one, since it would otherwise keep nothing.

fn rowmarkers(rows: usize) -> RecordBatch {
    let zeros: datafusion::arrow::array::ArrayRef =
        Arc::new(datafusion::arrow::array::Int8Array::from(vec![0i8; rows]));
    RecordBatch::try_new(Schema::rowmarker().fields.clone(), vec![zeros]).expect("one Int8 column")
}

operator_case! {
    GpuCrossJoin,
    fn a_cross_join_of_two_rowmarker_sides_keeps_one_placeholder() {
        let join = GpuCrossJoin::new(
            Given::of(Schema::rowmarker(), BatchLayout::SingleBatch),
            Given::of(Schema::rowmarker(), BatchLayout::MultipleBatches),
            Some(vec![0]),
            Schema::rowmarker(),
        );
        let outcome = run_both(
            &join,
            script(Some(rowmarkers(3)), vec![rowmarkers(4), rowmarkers(5)]),
        );
        outcome.same(Order::Any);
        let rows: usize = outcome
            .cpu
            .as_ref()
            .expect("the cpu answers")
            .iter()
            .flatten()
            .map(|batch| {
                assert_eq!(batch.schema().fields(), Schema::rowmarker().fields.fields());
                batch.num_rows()
            })
            .sum();
        assert_eq!(rows, 3 * 4 + 3 * 5);
    }
}
