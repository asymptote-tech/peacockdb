//! `GpuCrossJoin` and `GpuNestedLoopJoin` through the harness: a build batch first and
//! once, the probe streamed where the matrix lets it — the cross join and the Inner
//! nested loop stream, the Left nested loop takes one probe batch — over the same two
//! prefixed synthetic sides the hash join uses. Every case is green or a `bug_` test with
//! its ticket above it; nothing here repairs.

use std::sync::Arc;

use datafusion::arrow::datatypes::{DataType, Field, Schema as ArrowSchema};
use datafusion::arrow::record_batch::RecordBatch;

use super::script::{Outcome, Script, each_answers, run_both};
use crate::plan::{
    BatchLayout, BinaryOp, Expr, GpuCrossJoin, GpuNestedLoopJoin, GpuNode, JoinFilterColumn,
    JoinSide, NestedLoopJoinType, Schema,
};
use crate::tests::compare::{Order, assert_same};
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

/// `[build columns…, probe columns…]`, the probe side nullable where a Left form pads it,
/// as DataFusion's join schema declares it and so as the planner would.
fn joined(pads_probe: bool) -> Vec<Field> {
    let probe = side("p_").into_iter().map(|f| {
        let nullable = pads_probe || f.is_nullable();
        f.with_nullable(nullable)
    });
    side("b_").into_iter().chain(probe).collect()
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

fn cross(projection: Option<Vec<u32>>) -> GpuCrossJoin {
    let output = output(joined(false), &projection);
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
/// what `is_ast_able` refuses, so this predicate takes the cross-then-mask path.
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

/// Inner streams its probe; Left takes one batch, since a predicate join has no keys for
/// the finish trick to accumulate.
fn nested_with(
    join_type: NestedLoopJoinType,
    predicate: (Expr, Vec<JoinFilterColumn>),
    projection: Option<Vec<u32>>,
) -> GpuNestedLoopJoin {
    let (filter, filter_columns) = predicate;
    let (pads_probe, probe_batches) = match join_type {
        NestedLoopJoinType::Inner => (false, BatchLayout::MultipleBatches),
        NestedLoopJoinType::Left => (true, BatchLayout::SingleBatch),
    };
    let output = output(joined(pads_probe), &projection);
    GpuNestedLoopJoin::new(
        leaf("b_", BatchLayout::SingleBatch),
        leaf("p_", probe_batches),
        join_type,
        filter,
        filter_columns,
        projection,
        output,
    )
}

fn nested(join_type: NestedLoopJoinType, projection: Option<Vec<u32>>) -> GpuNestedLoopJoin {
    nested_with(join_type, residual(), projection)
}

fn inner(projection: Option<Vec<u32>>) -> GpuNestedLoopJoin {
    nested(NestedLoopJoinType::Inner, projection)
}

fn left(projection: Option<Vec<u32>>) -> GpuNestedLoopJoin {
    nested(NestedLoopJoinType::Left, projection)
}

fn script(build: Option<RecordBatch>, probe: Vec<RecordBatch>) -> Script {
    Script::Join { build, probe }
}

fn one_probe() -> Script {
    script(Some(build_batch(16)), vec![probe_batch(16, 21)])
}

fn two_probes() -> Script {
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

/// A zero-row table under the crossed schema, which is what the device emits over an
/// empty side.
fn crossed_nothing() -> RecordBatch {
    RecordBatch::new_empty(Arc::new(ArrowSchema::new(joined(false))))
}

// The `bug_` assertions: a refusal pinned by the stable part of its message, both sides
// pinned to hand-written slots, and a dropped projection pinned as "the answer is the
// unprojected one".

fn gpu_refuses_with(outcome: &Outcome, message: &str) {
    let why = outcome.gpu_refuses();
    assert!(why.contains(message), "{why}");
}

fn cpu_refuses_with(outcome: &Outcome, message: &str) {
    let why = outcome.cpu_refuses();
    assert!(why.contains(message), "{why}");
}

/// The cpu's answer to `node` with every column, projected to `keep` after the fact —
/// what a join that applied its projection would have emitted.
fn cpu_projected(node: &dyn GpuNode, script: Script, keep: &[usize]) -> Vec<Vec<RecordBatch>> {
    let every_column = run_both(node, script);
    every_column
        .cpu
        .expect("the cpu answers")
        .iter()
        .map(|slot| {
            slot.iter()
                .map(|b| b.project(keep).expect("ordinals in range"))
                .collect()
        })
        .collect()
}

const BUILD_COPY: &str =
    "probe batch 2 has no build side left, since the call for batch 1 erased it (#152)";
const TWO_OF_SIXTEEN: &str = "number of columns(16) must match number of fields(2) in schema";
const THREE_OF_SIXTEEN: &str = "number of columns(16) must match number of fields(3) in schema";
const INNER_ONLY: &str = "non-AST-able NestedLoopJoin filter is only supported for Inner joins";

// The cross join: the product of the two sides, every column of both.

operator_case! {
    GpuCrossJoin,
    fn a_cross_join_is_the_product_on_both() {
        run_both(&cross(None), one_probe()).same(Order::Any);
    }
}

// #152 — the recipe copies the build side per probe batch, and the first call erased it.
operator_case! {
    GpuCrossJoin,
    fn bug_a_cross_join_refuses_its_second_probe_batch_on_the_device() {
        gpu_refuses_with(&run_both(&cross(None), two_probes()), BUILD_COPY);
    }
}

// #207 — neither backend applies a cross join's projection: the cpu refuses the sixteen
// columns it declared two of, and the device hands all sixteen up.
operator_case! {
    GpuCrossJoin,
    fn bug_a_cross_join_projection_is_dropped_on_both() {
        let outcome = run_both(&cross(Some(vec![0, 8])), one_probe());
        cpu_refuses_with(&outcome, TWO_OF_SIXTEEN);
        let every_column = run_both(&cross(None), one_probe());
        assert_same(
            every_column.cpu.as_ref().expect("the cpu answers"),
            outcome.gpu.as_ref().expect("the device answers"),
            Order::Any,
        );
    }
}

// #208 — DataFusion's cross join over an empty left side ends without a batch; the
// device's is a zero-row table.
operator_case! {
    GpuCrossJoin,
    fn bug_a_cross_join_over_a_zero_row_build_is_nothing_on_the_cpu() {
        let outcome = run_both(&cross(None), empty_build());
        each_answers(&outcome, &[vec![], vec![]], &[vec![crossed_nothing()], vec![]]);
    }
}

operator_case! {
    GpuCrossJoin,
    fn a_cross_join_over_a_zero_row_probe_is_zero_rows() {
        run_both(&cross(None), empty_probe()).same(Order::Any);
    }
}

// #208 — the same with the probe side empty too.
operator_case! {
    GpuCrossJoin,
    fn bug_a_cross_join_over_both_sides_empty_is_nothing_on_the_cpu() {
        let outcome = run_both(&cross(None), both_empty());
        each_answers(&outcome, &[vec![], vec![]], &[vec![crossed_nothing()], vec![]]);
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

// #152 — the Inner recipe copies the build side per probe batch, and the first call
// erased it.
operator_case! {
    GpuNestedLoopJoin,
    fn bug_an_inner_nested_loop_join_refuses_its_second_probe_batch_on_the_device() {
        gpu_refuses_with(&run_both(&inner(None), two_probes()), BUILD_COPY);
    }
}

// #190 — the cpu passes `NestedLoopJoinExec` no projection and refuses its own sixteen
// columns; the device applies the node's.
operator_case! {
    GpuNestedLoopJoin,
    fn bug_an_inner_nested_loop_join_projection_is_dropped_on_the_cpu() {
        let outcome = run_both(&inner(Some(vec![0, 8])), one_probe());
        cpu_refuses_with(&outcome, TWO_OF_SIXTEEN);
        assert_same(
            &cpu_projected(&inner(None), one_probe(), &[0, 8]),
            outcome.gpu.as_ref().expect("the device answers"),
            Order::Any,
        );
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

// The Left nested loop: one probe batch, the unmatched build rows padded.

operator_case! {
    GpuNestedLoopJoin,
    fn a_left_nested_loop_join_pads_the_unmatched() {
        run_both(&left(None), one_probe()).same(Order::Any);
    }
}

// #190 — the Left form the same way.
operator_case! {
    GpuNestedLoopJoin,
    fn bug_a_left_nested_loop_join_projection_is_dropped_on_the_cpu() {
        let outcome = run_both(&left(Some(vec![0, 8])), one_probe());
        cpu_refuses_with(&outcome, TWO_OF_SIXTEEN);
        assert_same(
            &cpu_projected(&left(None), one_probe(), &[0, 8]),
            outcome.gpu.as_ref().expect("the device answers"),
            Order::Any,
        );
    }
}

operator_case! {
    GpuNestedLoopJoin,
    fn a_left_nested_loop_join_over_a_zero_row_build_is_zero_rows() {
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

// The predicate the AST cannot take: a decimal operand sends the device down the
// cross-then-mask path, whose mask columns are read in `filter_columns` order.

operator_case! {
    GpuNestedLoopJoin,
    fn an_inner_nested_loop_join_with_a_decimal_predicate_agrees() {
        let node = nested_with(NestedLoopJoinType::Inner, decimal_residual(), None);
        run_both(&node, one_probe()).same(Order::Any);
    }
}

// #190 — the cpu passes `NestedLoopJoinExec` no projection on this path too; the device
// applies the node's after the mask.
operator_case! {
    GpuNestedLoopJoin,
    fn bug_an_inner_nested_loop_join_with_a_decimal_predicate_and_a_projection_is_dropped_on_the_cpu() {
        let node = nested_with(NestedLoopJoinType::Inner, decimal_residual(), Some(vec![13, 4, 8]));
        let outcome = run_both(&node, one_probe());
        cpu_refuses_with(&outcome, THREE_OF_SIXTEEN);
        let unprojected = nested_with(NestedLoopJoinType::Inner, decimal_residual(), None);
        assert_same(
            &cpu_projected(&unprojected, one_probe(), &[13, 4, 8]),
            outcome.gpu.as_ref().expect("the device answers"),
            Order::Any,
        );
    }
}

// #215 — the planner admits a Left nested loop over any predicate; the device's
// cross-then-mask path is written for Inner alone and throws at its guard.
operator_case! {
    GpuNestedLoopJoin,
    fn bug_a_left_nested_loop_join_with_a_decimal_predicate_is_refused_on_the_device() {
        let node = nested_with(NestedLoopJoinType::Left, decimal_residual(), None);
        gpu_refuses_with(&run_both(&node, one_probe()), INNER_ONLY);
    }
}

// #215 on the device and #190 on the cpu: with a projection neither side answers.
operator_case! {
    GpuNestedLoopJoin,
    fn bug_a_left_nested_loop_join_with_a_decimal_predicate_and_a_projection_is_refused_on_both() {
        let node = nested_with(NestedLoopJoinType::Left, decimal_residual(), Some(vec![13, 4, 8]));
        let outcome = run_both(&node, one_probe());
        let cpu = &outcome.cpu.as_ref().expect_err("the cpu drops the projection").message;
        assert!(cpu.contains(THREE_OF_SIXTEEN), "{cpu}");
        let gpu = &outcome.gpu.as_ref().expect_err("the device has no Left mask path").message;
        assert!(gpu.contains(INNER_ONLY), "{gpu}");
    }
}

// The path's empty shapes, Inner only: Left is a refusal whatever the input.

operator_case! {
    GpuNestedLoopJoin,
    fn an_inner_nested_loop_join_with_a_decimal_predicate_over_an_empty_build_answers_nothing() {
        let node = nested_with(NestedLoopJoinType::Inner, decimal_residual(), None);
        run_both(&node, empty_build()).same(Order::Any);
    }
}

operator_case! {
    GpuNestedLoopJoin,
    fn an_inner_nested_loop_join_with_a_decimal_predicate_over_an_empty_probe_answers_nothing() {
        let node = nested_with(NestedLoopJoinType::Inner, decimal_residual(), None);
        run_both(&node, empty_probe()).same(Order::Any);
    }
}
