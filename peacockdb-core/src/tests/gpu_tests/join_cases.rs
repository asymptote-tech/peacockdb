//! `GpuHashJoin` through the harness: a build batch first and once, the probe streamed,
//! nine types over the same two prefixed synthetic sides joined on `key`, each with and
//! without a residual filter.
//! The sides, scripts and `hash_join` are `pub(crate)` for `join_dimension_cases.rs`.
//! Every case is green or a `bug_` test with its ticket above it; nothing here repairs.

use std::sync::Arc;

use datafusion::arrow::array::Float64Array;
use datafusion::arrow::datatypes::{DataType, Field, Schema as ArrowSchema};
use datafusion::arrow::record_batch::RecordBatch;
use datafusion::common::JoinType;

use super::script::{Script, run_both};
use crate::plan::{
    BatchLayout, BinaryOp, Expr, GpuHashJoin, GpuNode, JoinFilterColumn, JoinSide, Schema,
};
use crate::tests::compare::Order;
use crate::tests::given::Given;
use crate::tests::synthetic::{prefixed, synthetic};

use super::aggregate_cases::{float_key_batch, rows};

pub(crate) fn build_batch(rows: usize) -> RecordBatch {
    prefixed(&synthetic(rows, 11), "b_")
}

pub(crate) fn probe_batch(rows: usize, seed: u64) -> RecordBatch {
    prefixed(&synthetic(rows, seed), "p_")
}

pub(crate) fn side(prefix: &str) -> Vec<Field> {
    prefixed(&synthetic(0, 0), prefix)
        .schema()
        .fields()
        .iter()
        .map(|f| f.as_ref().clone())
        .collect()
}

pub(crate) fn padded(fields: Vec<Field>) -> Vec<Field> {
    fields.into_iter().map(|f| f.with_nullable(true)).collect()
}

/// What each type emits before any projection: both sides, the build side, the probe
/// side, or the build side and a mark. A side an outer join pads is nullable, as
/// DataFusion's join schema declares it and so as the planner would. `nested_cases.rs`
/// declares its nested loops by it too.
pub(crate) fn output_of(join_type: JoinType) -> Vec<Field> {
    match join_type {
        JoinType::Inner => [side("b_"), side("p_")].concat(),
        JoinType::Left => [side("b_"), padded(side("p_"))].concat(),
        JoinType::Right => [padded(side("b_")), side("p_")].concat(),
        JoinType::Full => [padded(side("b_")), padded(side("p_"))].concat(),
        JoinType::LeftSemi | JoinType::LeftAnti => side("b_"),
        JoinType::RightSemi | JoinType::RightAnti => side("p_"),
        JoinType::LeftMark => [
            side("b_"),
            vec![Field::new("mark", DataType::Boolean, false)],
        ]
        .concat(),
    }
}

/// `b_i64 < p_i64` over the filter's own schema, `[build columns…, probe columns…]` as
/// `filter_columns` lists them: 0 is the build's `i64`, 1 the probe's.
pub(crate) fn residual() -> (Option<Expr>, Vec<JoinFilterColumn>) {
    (
        Some(Expr::binary(
            Expr::column(0, "b_i64"),
            BinaryOp::Lt,
            Expr::column(1, "p_i64"),
            DataType::Boolean,
        )),
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

pub(crate) fn hash_join_with(
    join_type: JoinType,
    null_equals_null: bool,
    residual: (Option<Expr>, Vec<JoinFilterColumn>),
    projection: Option<Vec<u32>>,
) -> GpuHashJoin {
    hash_join_on(
        join_type,
        null_equals_null,
        residual,
        projection,
        vec![(1, 1)],
    )
}

/// `hash_join_with` on a stated key pair, for a case that is about the key's type.
pub(crate) fn hash_join_on(
    join_type: JoinType,
    null_equals_null: bool,
    residual: (Option<Expr>, Vec<JoinFilterColumn>),
    projection: Option<Vec<u32>>,
    keys: Vec<(u32, u32)>,
) -> GpuHashJoin {
    let (filter, filter_columns) = residual;
    let fields = output_of(join_type);
    let fields = match &projection {
        None => fields,
        Some(keep) => keep.iter().map(|i| fields[*i as usize].clone()).collect(),
    };
    let leaf = |prefix: &str, batches: BatchLayout| -> Box<dyn GpuNode> {
        Given::of(
            Schema::new(Arc::new(ArrowSchema::new(side(prefix)))),
            batches,
        )
    };
    GpuHashJoin::new(
        leaf("b_", BatchLayout::SingleBatch),
        leaf("p_", BatchLayout::MultipleBatches),
        join_type,
        keys,
        filter,
        filter_columns,
        null_equals_null,
        projection,
        Schema::new(Arc::new(ArrowSchema::new(fields))),
    )
}

pub(crate) fn hash_join(
    join_type: JoinType,
    null_equals_null: bool,
    filtered: bool,
    projection: Option<Vec<u32>>,
) -> GpuHashJoin {
    let residual = if filtered {
        residual()
    } else {
        (None, Vec::new())
    };
    hash_join_with(join_type, null_equals_null, residual, projection)
}

fn join(join_type: JoinType) -> GpuHashJoin {
    hash_join(join_type, false, false, None)
}

pub(crate) fn script(build: Option<RecordBatch>, probe: Vec<RecordBatch>) -> Script {
    Script::Join { build, probe }
}

pub(crate) fn one_probe() -> Script {
    script(Some(build_batch(32)), vec![probe_batch(48, 21)])
}

pub(crate) fn two_probes() -> Script {
    script(
        Some(build_batch(32)),
        vec![probe_batch(24, 21), probe_batch(24, 22)],
    )
}

// The empty shapes, one script each; a case names the type and the shape.

pub(crate) fn empty_build() -> Script {
    script(Some(build_batch(0)), vec![probe_batch(16, 21)])
}

fn empty_probe() -> Script {
    script(Some(build_batch(32)), vec![probe_batch(0, 21)])
}

fn both_empty() -> Script {
    script(Some(build_batch(0)), vec![probe_batch(0, 21)])
}

fn no_build() -> Script {
    script(None, Vec::new())
}

/// No build batch, and a probe batch the three probe-preserving types owe rows for (#212).
fn no_build_then_a_probe() -> Script {
    script(None, vec![probe_batch(16, 21)])
}

fn empty_between() -> Script {
    script(
        Some(build_batch(32)),
        vec![probe_batch(16, 21), probe_batch(0, 22), probe_batch(16, 23)],
    )
}

fn only_empty_probes() -> Script {
    script(
        Some(build_batch(32)),
        vec![probe_batch(0, 21), probe_batch(0, 22)],
    )
}

/// The build set and the finish called with no probe call between (#173).
fn no_probe() -> Script {
    script(Some(build_batch(32)), Vec::new())
}

// One probe batch, every type.

operator_case! {
    GpuHashJoin,
    fn an_inner_join_over_one_probe_batch_agrees() {
        run_both(&join(JoinType::Inner), one_probe()).same(Order::Any);
    }
}

operator_case! {
    GpuHashJoin,
    fn a_left_join_over_one_probe_batch_agrees() {
        run_both(&join(JoinType::Left), one_probe()).same(Order::Any);
    }
}

operator_case! {
    GpuHashJoin,
    fn a_right_join_over_one_probe_batch_agrees() {
        run_both(&join(JoinType::Right), one_probe()).same(Order::Any);
    }
}

operator_case! {
    GpuHashJoin,
    fn a_full_join_over_one_probe_batch_agrees() {
        run_both(&join(JoinType::Full), one_probe()).same(Order::Any);
    }
}

operator_case! {
    GpuHashJoin,
    fn a_left_semi_join_over_one_probe_batch_agrees() {
        run_both(&join(JoinType::LeftSemi), one_probe()).same(Order::Any);
    }
}

operator_case! {
    GpuHashJoin,
    fn a_right_semi_join_over_one_probe_batch_agrees() {
        run_both(&join(JoinType::RightSemi), one_probe()).same(Order::Any);
    }
}

operator_case! {
    GpuHashJoin,
    fn a_left_anti_join_keeps_its_null_key_build_rows() {
        run_both(&hash_join(JoinType::LeftAnti, false, false, None), one_probe()).same(Order::Any);
    }
}

operator_case! {
    GpuHashJoin,
    fn a_right_anti_join_keeps_its_null_key_probe_rows() {
        run_both(&hash_join(JoinType::RightAnti, false, false, None), one_probe()).same(Order::Any);
    }
}

operator_case! {
    GpuHashJoin,
    fn a_left_mark_join_marks_its_null_key_build_rows_false() {
        run_both(&hash_join(JoinType::LeftMark, false, false, None), one_probe()).same(Order::Any);
    }
}

// A streamed probe: two batches, every type.

operator_case! {
    GpuHashJoin,
    fn an_inner_join_over_two_probe_batches_agrees() {
        run_both(&join(JoinType::Inner), two_probes()).same(Order::Any);
    }
}

operator_case! {
    GpuHashJoin,
    fn a_left_join_over_two_probe_batches_agrees() {
        run_both(&join(JoinType::Left), two_probes()).same(Order::Any);
    }
}

operator_case! {
    GpuHashJoin,
    fn a_right_join_over_two_probe_batches_agrees() {
        run_both(&join(JoinType::Right), two_probes()).same(Order::Any);
    }
}

operator_case! {
    GpuHashJoin,
    fn a_full_join_over_two_probe_batches_agrees() {
        run_both(&join(JoinType::Full), two_probes()).same(Order::Any);
    }
}

operator_case! {
    GpuHashJoin,
    fn a_left_semi_join_over_two_probe_batches_agrees() {
        run_both(&join(JoinType::LeftSemi), two_probes()).same(Order::Any);
    }
}

operator_case! {
    GpuHashJoin,
    fn a_right_semi_join_over_two_probe_batches_agrees() {
        run_both(&join(JoinType::RightSemi), two_probes()).same(Order::Any);
    }
}

operator_case! {
    GpuHashJoin,
    fn a_left_anti_join_over_two_probe_batches_keeps_its_null_key_build_rows() {
        run_both(&hash_join(JoinType::LeftAnti, false, false, None), two_probes()).same(Order::Any);
    }
}

operator_case! {
    GpuHashJoin,
    fn a_right_anti_join_over_two_probe_batches_agrees() {
        run_both(&join(JoinType::RightAnti), two_probes()).same(Order::Any);
    }
}

operator_case! {
    GpuHashJoin,
    fn a_left_mark_join_over_two_probe_batches_marks_its_null_key_build_rows_false() {
        run_both(&hash_join(JoinType::LeftMark, false, false, None), two_probes()).same(Order::Any);
    }
}

// `key` is null on every eleventh row of both sides. `true` matches them to each other on
// both engines, and the SQL default matches them to nothing, anti and mark included.

operator_case! {
    GpuHashJoin,
    fn null_equals_null_matches_null_keys_on_an_inner_join() {
        run_both(&hash_join(JoinType::Inner, true, false, None), one_probe()).same(Order::Any);
    }
}

operator_case! {
    GpuHashJoin,
    fn null_equals_null_is_honoured_by_a_right_join() {
        run_both(&hash_join(JoinType::Right, true, false, None), one_probe()).same(Order::Any);
    }
}

operator_case! {
    GpuHashJoin,
    fn null_equals_null_is_honoured_by_a_semi_join() {
        run_both(&hash_join(JoinType::LeftSemi, true, false, None), one_probe()).same(Order::Any);
    }
}

operator_case! {
    GpuHashJoin,
    fn null_equals_null_is_honoured_by_a_right_semi_join() {
        run_both(&hash_join(JoinType::RightSemi, true, false, None), one_probe()).same(Order::Any);
    }
}

operator_case! {
    GpuHashJoin,
    fn an_anti_join_under_null_equals_null_agrees() {
        run_both(&hash_join(JoinType::LeftAnti, true, false, None), one_probe()).same(Order::Any);
    }
}

operator_case! {
    GpuHashJoin,
    fn a_mark_join_under_null_equals_null_agrees() {
        run_both(&hash_join(JoinType::LeftMark, true, false, None), one_probe()).same(Order::Any);
    }
}

operator_case! {
    GpuHashJoin,
    fn a_right_anti_join_under_null_equals_null_agrees() {
        run_both(&hash_join(JoinType::RightAnti, true, false, None), one_probe()).same(Order::Any);
    }
}

// A residual filter, every type. The build-side semi family answers at finish as it does
// unfiltered; the outer forms pad what the residual leaves unmatched (#153), and the
// right-handed semi forms keep or drop each probe row by it (#159). The probe streams under
// every one, so the five lifted forms and the semi family run over two probe batches.

operator_case! {
    GpuHashJoin,
    fn an_inner_join_with_a_residual_filter_agrees() {
        run_both(&hash_join(JoinType::Inner, false, true, None), one_probe()).same(Order::Any);
    }
}

operator_case! {
    GpuHashJoin,
    fn a_left_semi_join_with_a_residual_filter_agrees() {
        run_both(&hash_join(JoinType::LeftSemi, false, true, None), one_probe()).same(Order::Any);
    }
}

operator_case! {
    GpuHashJoin,
    fn a_left_anti_join_with_a_residual_filter_agrees() {
        run_both(&hash_join(JoinType::LeftAnti, false, true, None), one_probe()).same(Order::Any);
    }
}

operator_case! {
    GpuHashJoin,
    fn a_left_mark_join_with_a_residual_filter_agrees() {
        run_both(&hash_join(JoinType::LeftMark, false, true, None), one_probe()).same(Order::Any);
    }
}

operator_case! {
    GpuHashJoin,
    fn a_left_join_with_a_residual_filter_agrees() {
        run_both(&hash_join(JoinType::Left, false, true, None), two_probes()).same(Order::Any);
    }
}

operator_case! {
    GpuHashJoin,
    fn a_right_join_with_a_residual_filter_agrees() {
        run_both(&hash_join(JoinType::Right, false, true, None), two_probes()).same(Order::Any);
    }
}

operator_case! {
    GpuHashJoin,
    fn a_full_join_with_a_residual_filter_agrees() {
        run_both(&hash_join(JoinType::Full, false, true, None), two_probes()).same(Order::Any);
    }
}

operator_case! {
    GpuHashJoin,
    fn a_right_semi_join_with_a_residual_filter_agrees() {
        run_both(&hash_join(JoinType::RightSemi, false, true, None), two_probes()).same(Order::Any);
    }
}

operator_case! {
    GpuHashJoin,
    fn a_right_anti_join_with_a_residual_filter_agrees() {
        run_both(&hash_join(JoinType::RightAnti, false, true, None), two_probes()).same(Order::Any);
    }
}

operator_case! {
    GpuHashJoin,
    fn a_left_semi_join_with_a_residual_filter_over_two_probe_batches_agrees() {
        run_both(&hash_join(JoinType::LeftSemi, false, true, None), two_probes()).same(Order::Any);
    }
}

operator_case! {
    GpuHashJoin,
    fn a_left_anti_join_with_a_residual_filter_over_two_probe_batches_agrees() {
        run_both(&hash_join(JoinType::LeftAnti, false, true, None), two_probes()).same(Order::Any);
    }
}

operator_case! {
    GpuHashJoin,
    fn a_left_mark_join_with_a_residual_filter_over_two_probe_batches_agrees() {
        run_both(&hash_join(JoinType::LeftMark, false, true, None), two_probes()).same(Order::Any);
    }
}

operator_case! {
    GpuHashJoin,
    fn a_projection_keeps_the_same_columns() {
        run_both(&hash_join(JoinType::Inner, false, false, Some(vec![0, 5, 8, 13])), one_probe()).same(Order::Any);
    }
}

// Empty inputs, one case per type and shape. A zero-row build batch owes what no build
// batch owes: the six types built from build rows drain, and the three that keep probe
// rows pad over it on both engines.

operator_case! {
    GpuHashJoin,
    fn inner_over_a_zero_row_build_answers_nothing() {
        run_both(&join(JoinType::Inner), empty_build()).same(Order::Any);
    }
}

operator_case! {
    GpuHashJoin,
    fn inner_over_a_zero_row_probe_agrees() {
        run_both(&join(JoinType::Inner), empty_probe()).same(Order::Any);
    }
}

operator_case! {
    GpuHashJoin,
    fn inner_over_both_sides_empty_agrees() {
        run_both(&join(JoinType::Inner), both_empty()).same(Order::Any);
    }
}

operator_case! {
    GpuHashJoin,
    fn inner_with_no_build_batch_is_never_probed() {
        run_both(&join(JoinType::Inner), no_build()).same(Order::Any);
    }
}

operator_case! {
    GpuHashJoin,
    fn inner_with_a_zero_row_probe_between_two_with_rows_agrees() {
        run_both(&join(JoinType::Inner), empty_between()).same(Order::Any);
    }
}

operator_case! {
    GpuHashJoin,
    fn inner_finishing_after_only_zero_row_probes_agrees() {
        run_both(&join(JoinType::Inner), only_empty_probes()).same(Order::Any);
    }
}

operator_case! {
    GpuHashJoin,
    fn left_over_a_zero_row_build_agrees() {
        run_both(&join(JoinType::Left), empty_build()).same(Order::Any);
    }
}

operator_case! {
    GpuHashJoin,
    fn left_over_a_zero_row_probe_pads_every_build_row() {
        run_both(&join(JoinType::Left), empty_probe()).same(Order::Any);
    }
}

operator_case! {
    GpuHashJoin,
    fn left_over_both_sides_empty_agrees() {
        run_both(&join(JoinType::Left), both_empty()).same(Order::Any);
    }
}

operator_case! {
    GpuHashJoin,
    fn left_with_no_build_batch_is_never_probed() {
        run_both(&join(JoinType::Left), no_build()).same(Order::Any);
    }
}

operator_case! {
    GpuHashJoin,
    fn left_with_a_zero_row_probe_between_two_with_rows_agrees() {
        run_both(&join(JoinType::Left), empty_between()).same(Order::Any);
    }
}

operator_case! {
    GpuHashJoin,
    fn left_finishing_after_only_zero_row_probes_agrees() {
        run_both(&join(JoinType::Left), only_empty_probes()).same(Order::Any);
    }
}

operator_case! {
    GpuHashJoin,
    fn left_finishing_with_no_probe_batch_pads_every_build_row() {
        run_both(&join(JoinType::Left), no_probe()).same(Order::Any);
    }
}

operator_case! {
    GpuHashJoin,
    fn right_over_a_zero_row_build_pads_every_probe_row() {
        run_both(&join(JoinType::Right), empty_build()).same(Order::Any);
    }
}

operator_case! {
    GpuHashJoin,
    fn right_over_a_zero_row_probe_agrees() {
        run_both(&join(JoinType::Right), empty_probe()).same(Order::Any);
    }
}

operator_case! {
    GpuHashJoin,
    fn right_over_both_sides_empty_agrees() {
        run_both(&join(JoinType::Right), both_empty()).same(Order::Any);
    }
}

// #212 — a build side that produced no batch: both engines run the join over an empty
// build table.
operator_case! {
    GpuHashJoin,
    fn right_with_no_build_batch_pads_every_probe_row() {
        let outcome = run_both(&join(JoinType::Right), no_build_then_a_probe());
        outcome.same(Order::Any);
        let device = outcome.gpu.as_ref().expect("the session answers an absent build");
        assert_eq!(rows(device), 16, "every probe row, padded");
    }
}

operator_case! {
    GpuHashJoin,
    fn right_with_a_zero_row_probe_between_two_with_rows_agrees() {
        run_both(&join(JoinType::Right), empty_between()).same(Order::Any);
    }
}

operator_case! {
    GpuHashJoin,
    fn right_finishing_after_only_zero_row_probes_agrees() {
        run_both(&join(JoinType::Right), only_empty_probes()).same(Order::Any);
    }
}

operator_case! {
    GpuHashJoin,
    fn full_over_a_zero_row_build_pads_every_probe_row() {
        run_both(&join(JoinType::Full), empty_build()).same(Order::Any);
    }
}

operator_case! {
    GpuHashJoin,
    fn full_over_a_zero_row_probe_pads_every_build_row() {
        run_both(&join(JoinType::Full), empty_probe()).same(Order::Any);
    }
}

operator_case! {
    GpuHashJoin,
    fn full_over_both_sides_empty_agrees() {
        run_both(&join(JoinType::Full), both_empty()).same(Order::Any);
    }
}

// #212 — a build side that produced no batch: both engines run the join over an empty
// build table.
operator_case! {
    GpuHashJoin,
    fn full_with_no_build_batch_pads_every_probe_row() {
        let outcome = run_both(&join(JoinType::Full), no_build_then_a_probe());
        outcome.same(Order::Any);
        let device = outcome.gpu.as_ref().expect("the session answers an absent build");
        assert_eq!(rows(device), 16, "every probe row, padded");
    }
}

operator_case! {
    GpuHashJoin,
    fn full_with_a_zero_row_probe_between_two_with_rows_agrees() {
        run_both(&join(JoinType::Full), empty_between()).same(Order::Any);
    }
}

operator_case! {
    GpuHashJoin,
    fn full_finishing_after_only_zero_row_probes_agrees() {
        run_both(&join(JoinType::Full), only_empty_probes()).same(Order::Any);
    }
}

operator_case! {
    GpuHashJoin,
    fn full_finishing_with_no_probe_batch_pads_every_build_row() {
        run_both(&join(JoinType::Full), no_probe()).same(Order::Any);
    }
}

operator_case! {
    GpuHashJoin,
    fn left_semi_over_a_zero_row_build_answers_nothing() {
        run_both(&join(JoinType::LeftSemi), empty_build()).same(Order::Any);
    }
}

operator_case! {
    GpuHashJoin,
    fn left_semi_over_a_zero_row_probe_agrees() {
        run_both(&join(JoinType::LeftSemi), empty_probe()).same(Order::Any);
    }
}

operator_case! {
    GpuHashJoin,
    fn left_semi_over_both_sides_empty_agrees() {
        run_both(&join(JoinType::LeftSemi), both_empty()).same(Order::Any);
    }
}

operator_case! {
    GpuHashJoin,
    fn left_semi_with_no_build_batch_is_never_probed() {
        run_both(&join(JoinType::LeftSemi), no_build()).same(Order::Any);
    }
}

operator_case! {
    GpuHashJoin,
    fn left_semi_with_a_zero_row_probe_between_two_with_rows_agrees() {
        run_both(&join(JoinType::LeftSemi), empty_between()).same(Order::Any);
    }
}

operator_case! {
    GpuHashJoin,
    fn left_semi_finishing_after_only_zero_row_probes_agrees() {
        run_both(&join(JoinType::LeftSemi), only_empty_probes()).same(Order::Any);
    }
}

operator_case! {
    GpuHashJoin,
    fn left_semi_finishing_with_no_probe_batch_answers_nothing() {
        run_both(&join(JoinType::LeftSemi), no_probe()).same(Order::Any);
    }
}

operator_case! {
    GpuHashJoin,
    fn right_semi_over_a_zero_row_build_answers_nothing() {
        run_both(&join(JoinType::RightSemi), empty_build()).same(Order::Any);
    }
}

operator_case! {
    GpuHashJoin,
    fn right_semi_over_a_zero_row_probe_agrees() {
        run_both(&join(JoinType::RightSemi), empty_probe()).same(Order::Any);
    }
}

operator_case! {
    GpuHashJoin,
    fn right_semi_over_both_sides_empty_agrees() {
        run_both(&join(JoinType::RightSemi), both_empty()).same(Order::Any);
    }
}

operator_case! {
    GpuHashJoin,
    fn right_semi_with_no_build_batch_is_never_probed() {
        run_both(&join(JoinType::RightSemi), no_build()).same(Order::Any);
    }
}

operator_case! {
    GpuHashJoin,
    fn right_semi_with_a_zero_row_probe_between_two_with_rows_agrees() {
        run_both(&join(JoinType::RightSemi), empty_between()).same(Order::Any);
    }
}

operator_case! {
    GpuHashJoin,
    fn right_semi_finishing_after_only_zero_row_probes_agrees() {
        run_both(&join(JoinType::RightSemi), only_empty_probes()).same(Order::Any);
    }
}

operator_case! {
    GpuHashJoin,
    fn left_anti_over_a_zero_row_build_answers_nothing() {
        run_both(&join(JoinType::LeftAnti), empty_build()).same(Order::Any);
    }
}

operator_case! {
    GpuHashJoin,
    fn left_anti_over_a_zero_row_probe_agrees() {
        run_both(&join(JoinType::LeftAnti), empty_probe()).same(Order::Any);
    }
}

operator_case! {
    GpuHashJoin,
    fn left_anti_over_both_sides_empty_agrees() {
        run_both(&join(JoinType::LeftAnti), both_empty()).same(Order::Any);
    }
}

operator_case! {
    GpuHashJoin,
    fn left_anti_with_no_build_batch_is_never_probed() {
        run_both(&join(JoinType::LeftAnti), no_build()).same(Order::Any);
    }
}

operator_case! {
    GpuHashJoin,
    fn left_anti_with_a_zero_row_probe_between_two_with_rows_agrees() {
        run_both(&hash_join(JoinType::LeftAnti, false, false, None), empty_between()).same(Order::Any);
    }
}

operator_case! {
    GpuHashJoin,
    fn left_anti_finishing_after_only_zero_row_probes_agrees() {
        run_both(&join(JoinType::LeftAnti), only_empty_probes()).same(Order::Any);
    }
}

operator_case! {
    GpuHashJoin,
    fn left_anti_finishing_with_no_probe_batch_answers_every_build_row() {
        run_both(&join(JoinType::LeftAnti), no_probe()).same(Order::Any);
    }
}

operator_case! {
    GpuHashJoin,
    fn right_anti_over_a_zero_row_build_keeps_every_probe_row() {
        run_both(&join(JoinType::RightAnti), empty_build()).same(Order::Any);
    }
}

operator_case! {
    GpuHashJoin,
    fn right_anti_over_a_zero_row_probe_agrees() {
        run_both(&join(JoinType::RightAnti), empty_probe()).same(Order::Any);
    }
}

operator_case! {
    GpuHashJoin,
    fn right_anti_over_both_sides_empty_agrees() {
        run_both(&join(JoinType::RightAnti), both_empty()).same(Order::Any);
    }
}

// #212 — a build side that produced no batch: both engines run the join over an empty
// build table.
operator_case! {
    GpuHashJoin,
    fn right_anti_with_no_build_batch_returns_every_probe_row() {
        let outcome = run_both(&join(JoinType::RightAnti), no_build_then_a_probe());
        outcome.same(Order::Any);
        let device = outcome.gpu.as_ref().expect("the session answers an absent build");
        assert_eq!(rows(device), 16, "every probe row");
    }
}

operator_case! {
    GpuHashJoin,
    fn right_anti_with_a_zero_row_probe_between_two_with_rows_agrees() {
        run_both(&join(JoinType::RightAnti), empty_between()).same(Order::Any);
    }
}

operator_case! {
    GpuHashJoin,
    fn right_anti_finishing_after_only_zero_row_probes_agrees() {
        run_both(&join(JoinType::RightAnti), only_empty_probes()).same(Order::Any);
    }
}

operator_case! {
    GpuHashJoin,
    fn left_mark_over_a_zero_row_build_answers_nothing() {
        run_both(&join(JoinType::LeftMark), empty_build()).same(Order::Any);
    }
}

operator_case! {
    GpuHashJoin,
    fn left_mark_over_a_zero_row_probe_agrees() {
        run_both(&join(JoinType::LeftMark), empty_probe()).same(Order::Any);
    }
}

operator_case! {
    GpuHashJoin,
    fn left_mark_over_both_sides_empty_agrees() {
        run_both(&join(JoinType::LeftMark), both_empty()).same(Order::Any);
    }
}

operator_case! {
    GpuHashJoin,
    fn left_mark_with_no_build_batch_is_never_probed() {
        run_both(&join(JoinType::LeftMark), no_build()).same(Order::Any);
    }
}

operator_case! {
    GpuHashJoin,
    fn left_mark_with_a_zero_row_probe_between_two_with_rows_agrees() {
        run_both(&hash_join(JoinType::LeftMark, false, false, None), empty_between()).same(Order::Any);
    }
}

operator_case! {
    GpuHashJoin,
    fn left_mark_finishing_after_only_zero_row_probes_agrees() {
        run_both(&join(JoinType::LeftMark), only_empty_probes()).same(Order::Any);
    }
}

operator_case! {
    GpuHashJoin,
    fn left_mark_finishing_with_no_probe_batch_marks_every_build_row_false() {
        run_both(&join(JoinType::LeftMark), no_probe()).same(Order::Any);
    }
}

// #243 — the cpu's float *equality*, unchanged by this task: Inner over `b_f64 = p_f64`, the
// cpu pairs equal bits (0↔0, -0↔-0, NaN↔NaN, 1↔1: four rows) and the device pairs equal
// values with NaNs equal (0 and -0 both ways, each NaN with the probe's NaN, 1: seven). One
// probe batch and one lane, so the lane rule is not what is being measured.
operator_case! {
    GpuHashJoin,
    fn bug_a_float_join_key_misses_negative_zero_and_nan_pairs_on_the_cpu() {
        let node = hash_join_on(JoinType::Inner, false, (None, Vec::new()), None, vec![(4, 4)]);
        let build = prefixed(&float_key_batch(), "b_");
        let probe = {
            let batch = synthetic(5, 2);
            let mut columns = batch.columns().to_vec();
            columns[4] = Arc::new(Float64Array::from(vec![
                Some(0.0),
                Some(-0.0),
                Some(f64::NAN),
                None,
                Some(1.0),
            ]));
            prefixed(
                &RecordBatch::try_new(batch.schema(), columns).expect("the same shape"),
                "p_",
            )
        };
        let outcome = run_both(&node, script(Some(build), vec![probe]));
        assert_eq!(rows(outcome.cpu.as_ref().expect("the cpu answers")), 4);
        assert_eq!(rows(outcome.gpu.as_ref().expect("the device answers")), 7);
    }
}
