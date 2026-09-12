//! `GpuHashJoin` through the harness: a build batch first and once, the probe streamed,
//! nine types over the same two prefixed synthetic sides joined on `key`. The scripts
//! follow the capability matrix, and the filtered forms exist only where it admits one.
//! Every case is green or a `bug_` test with its ticket above it; nothing here repairs.

use std::sync::Arc;

use datafusion::arrow::datatypes::{DataType, Field, Schema as ArrowSchema};
use datafusion::arrow::record_batch::RecordBatch;
use datafusion::common::JoinType;

use super::script::{Outcome, Script, run_both};
use crate::plan::{
    BatchLayout, BinaryOp, Expr, GpuHashJoin, GpuNode, JoinFilterColumn, JoinSide, Schema,
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

fn padded(fields: Vec<Field>) -> Vec<Field> {
    fields.into_iter().map(|f| f.with_nullable(true)).collect()
}

/// What each type emits before any projection: both sides, the build side, the probe
/// side, or the build side and a mark. A side an outer join pads is nullable, as
/// DataFusion's join schema declares it and so as the planner would.
fn output_of(join_type: JoinType) -> Vec<Field> {
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
fn residual() -> (Option<Expr>, Vec<JoinFilterColumn>) {
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

fn hash_join(
    join_type: JoinType,
    null_equals_null: bool,
    filtered: bool,
    projection: Option<Vec<u32>>,
) -> GpuHashJoin {
    let (filter, filter_columns) = if filtered {
        residual()
    } else {
        (None, Vec::new())
    };
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
        vec![(1, 1)],
        filter,
        filter_columns,
        null_equals_null,
        projection,
        Schema::new(Arc::new(ArrowSchema::new(fields))),
    )
}

fn join(join_type: JoinType) -> GpuHashJoin {
    hash_join(join_type, false, false, None)
}

fn script(build: Option<RecordBatch>, probe: Vec<RecordBatch>) -> Script {
    Script::Join { build, probe }
}

fn one_probe() -> Script {
    script(Some(build_batch(32)), vec![probe_batch(48, 21)])
}

fn two_probes() -> Script {
    script(
        Some(build_batch(32)),
        vec![probe_batch(24, 21), probe_batch(24, 22)],
    )
}

// The empty shapes, one script each; a case names the type and the shape.

fn empty_build() -> Script {
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

/// The build set and the finish called with no probe call between: the finish whose
/// probe produced no keys at all, #173's one refusing site.
fn no_probe() -> Script {
    script(Some(build_batch(32)), Vec::new())
}

// The `bug_` assertions: a refusal pinned by the stable part of its message, and a
// null-key divergence pinned as "the device under `false` answers as the cpu under `true`".

fn gpu_refuses_with(outcome: &Outcome, message: &str) {
    let why = outcome.gpu_refuses();
    assert!(why.contains(message), "{why}");
}

fn both_refuse_with(outcome: &Outcome, message: &str) {
    for (side, answer) in [("cpu", &outcome.cpu), ("gpu", &outcome.gpu)] {
        let why = &answer.as_ref().expect_err(side).message;
        assert!(why.contains(message), "{side}: {why}");
    }
}

/// #59 — anti and mark hardcode `EQUAL` on the device and take the flag on the cpu, so the
/// device's answer under the SQL default is the cpu's answer with the flag set.
fn device_answers_as_if_null_equals_null(
    join_type: JoinType,
    filtered: bool,
    script: fn() -> Script,
) {
    let device = run_both(&hash_join(join_type, false, filtered, None), script());
    let oracle = run_both(&hash_join(join_type, true, filtered, None), script());
    assert_same(
        oracle.cpu.as_ref().expect("the cpu answers"),
        device.gpu.as_ref().expect("the device answers"),
        Order::Any,
    );
}

const BUILD_COPY: &str =
    "probe batch 2 has no build side left, since the call for batch 1 erased it (#152)";
const PROBE_COPY: &str = "this join's recipe copies its probe batch — the key project keeps the keys and the join below it reads the same batch — and the ABI has no copy";
const NO_KEYS: &str = "this lane's probe was empty, so its finish has no keys to join against";
const NO_BUILD: &str = "this lane's build side is empty, and what this join owes is its probe side — which takes a call over a build table that does not exist (#175)";

// One probe batch, every type.

operator_case! {
    GpuHashJoin,
    fn an_inner_join_over_one_probe_batch_agrees() {
        run_both(&join(JoinType::Inner), one_probe()).same(Order::Any);
    }
}

// #152 — a left join's key project and per-call join both read the probe batch, and the
// ABI has no copy: the first batch is refused.
operator_case! {
    GpuHashJoin,
    fn bug_a_left_join_refuses_its_first_probe_batch_on_the_device() {
        gpu_refuses_with(&run_both(&join(JoinType::Left), one_probe()), PROBE_COPY);
    }
}

operator_case! {
    GpuHashJoin,
    fn a_right_join_over_one_probe_batch_agrees() {
        run_both(&join(JoinType::Right), one_probe()).same(Order::Any);
    }
}

// #152 — a full join's key project and per-call join both read the probe batch, and the
// ABI has no copy: the first batch is refused.
operator_case! {
    GpuHashJoin,
    fn bug_a_full_join_refuses_its_first_probe_batch_on_the_device() {
        gpu_refuses_with(&run_both(&join(JoinType::Full), one_probe()), PROBE_COPY);
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

// #59 — the device hardcodes EQUAL for the null keys of an anti or mark join.
operator_case! {
    GpuHashJoin,
    fn bug_a_left_anti_join_drops_null_key_build_rows_on_the_device() {
        device_answers_as_if_null_equals_null(JoinType::LeftAnti, false, one_probe);
    }
}

// #59 — the device hardcodes EQUAL for the null keys of an anti or mark join.
operator_case! {
    GpuHashJoin,
    fn bug_a_right_anti_join_drops_null_key_probe_rows_on_the_device() {
        device_answers_as_if_null_equals_null(JoinType::RightAnti, false, one_probe);
    }
}

// #59 — the device hardcodes EQUAL for the null keys of an anti or mark join.
operator_case! {
    GpuHashJoin,
    fn bug_a_left_mark_join_marks_null_key_build_rows_true_on_the_device() {
        device_answers_as_if_null_equals_null(JoinType::LeftMark, false, one_probe);
    }
}

// A streamed probe: two batches, every type that streams. Left and Full refuse their first
// batch already, so their streamed form has no separate case.

// #152 — the first call erased the build side and nothing copies it.
operator_case! {
    GpuHashJoin,
    fn bug_an_inner_join_refuses_its_second_probe_batch_on_the_device() {
        gpu_refuses_with(&run_both(&join(JoinType::Inner), two_probes()), BUILD_COPY);
    }
}

// #152 — the first call erased the build side and nothing copies it.
operator_case! {
    GpuHashJoin,
    fn bug_a_right_join_refuses_its_second_probe_batch_on_the_device() {
        gpu_refuses_with(&run_both(&join(JoinType::Right), two_probes()), BUILD_COPY);
    }
}

operator_case! {
    GpuHashJoin,
    fn a_left_semi_join_over_two_probe_batches_agrees() {
        run_both(&join(JoinType::LeftSemi), two_probes()).same(Order::Any);
    }
}

// #152 — the first call erased the build side and nothing copies it.
operator_case! {
    GpuHashJoin,
    fn bug_a_right_semi_join_refuses_its_second_probe_batch_on_the_device() {
        gpu_refuses_with(&run_both(&join(JoinType::RightSemi), two_probes()), BUILD_COPY);
    }
}

// #59 — across a streamed probe too.
operator_case! {
    GpuHashJoin,
    fn bug_a_left_anti_join_over_two_probe_batches_drops_null_key_build_rows_on_the_device() {
        device_answers_as_if_null_equals_null(JoinType::LeftAnti, false, two_probes);
    }
}

// #152 — the first call erased the build side and nothing copies it.
operator_case! {
    GpuHashJoin,
    fn bug_a_right_anti_join_refuses_its_second_probe_batch_on_the_device() {
        gpu_refuses_with(&run_both(&join(JoinType::RightAnti), two_probes()), BUILD_COPY);
    }
}

// #59 — across a streamed probe too.
operator_case! {
    GpuHashJoin,
    fn bug_a_left_mark_join_over_two_probe_batches_marks_null_key_build_rows_true_on_the_device() {
        device_answers_as_if_null_equals_null(JoinType::LeftMark, false, two_probes);
    }
}

// `key` is null on every eleventh row of both sides. `true` matches them to each other on
// both engines; the SQL default is where anti and mark part company (#59, above).

operator_case! {
    GpuHashJoin,
    fn null_equals_null_matches_null_keys_on_an_inner_join() {
        run_both(&hash_join(JoinType::Inner, true, false, None), one_probe()).same(Order::Any);
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

// A residual filter where the matrix admits one: Inner, and the build-side semi family
// over a single-batch probe. Left/Right/Full with one are refused at plan time (#153),
// RightSemi/RightAnti too (#159), so those have no row.

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

// #59 — the `mixed_*` variants hardcode EQUAL too.
operator_case! {
    GpuHashJoin,
    fn bug_a_left_anti_join_with_a_residual_filter_drops_null_key_build_rows_on_the_device() {
        device_answers_as_if_null_equals_null(JoinType::LeftAnti, true, one_probe);
    }
}

operator_case! {
    GpuHashJoin,
    fn bug_a_left_mark_join_with_a_residual_filter_marks_null_key_build_rows_true_on_the_device() {
        device_answers_as_if_null_equals_null(JoinType::LeftMark, true, one_probe);
    }
}

operator_case! {
    GpuHashJoin,
    fn a_projection_keeps_the_same_columns() {
        run_both(&hash_join(JoinType::Inner, false, false, Some(vec![0, 5, 8, 13])), one_probe()).same(Order::Any);
    }
}

// Empty inputs, one case per type and shape. A zero-row build batch is a build table, and
// the device pads over it like the cpu; only a build lane with no batch at all is #175.

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
    fn bug_inner_with_a_zero_row_probe_between_two_with_rows_refuses_the_second_on_the_device() {
        gpu_refuses_with(&run_both(&join(JoinType::Inner), empty_between()), BUILD_COPY);
    }
}

operator_case! {
    GpuHashJoin,
    fn bug_inner_finishing_after_only_zero_row_probes_refuses_the_second_on_the_device() {
        gpu_refuses_with(&run_both(&join(JoinType::Inner), only_empty_probes()), BUILD_COPY);
    }
}

operator_case! {
    GpuHashJoin,
    fn bug_left_over_a_zero_row_build_refuses_the_probe_on_the_device() {
        gpu_refuses_with(&run_both(&join(JoinType::Left), empty_build()), PROBE_COPY);
    }
}

operator_case! {
    GpuHashJoin,
    fn bug_left_over_a_zero_row_probe_refuses_it_on_the_device() {
        gpu_refuses_with(&run_both(&join(JoinType::Left), empty_probe()), PROBE_COPY);
    }
}

operator_case! {
    GpuHashJoin,
    fn bug_left_over_both_sides_empty_refuses_the_probe_on_the_device() {
        gpu_refuses_with(&run_both(&join(JoinType::Left), both_empty()), PROBE_COPY);
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
    fn bug_left_with_a_zero_row_probe_between_two_with_rows_refuses_the_first_on_the_device() {
        gpu_refuses_with(&run_both(&join(JoinType::Left), empty_between()), PROBE_COPY);
    }
}

operator_case! {
    GpuHashJoin,
    fn bug_left_finishing_after_only_zero_row_probes_refuses_the_first_on_the_device() {
        gpu_refuses_with(&run_both(&join(JoinType::Left), only_empty_probes()), PROBE_COPY);
    }
}

// #173 — the finish over no keys owes every build row padded with a typed NULL per probe column.
operator_case! {
    GpuHashJoin,
    fn bug_left_finishing_with_no_probe_batch_is_refused_on_the_device() {
        let outcome = run_both(&join(JoinType::Left), no_probe());
        gpu_refuses_with(&outcome, NO_KEYS);
        gpu_refuses_with(&outcome, "every build row padded with a typed NULL per probe column");
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

operator_case! {
    GpuHashJoin,
    fn bug_right_with_no_build_batch_is_refused_on_both() {
        both_refuse_with(&run_both(&join(JoinType::Right), no_build()), NO_BUILD);
    }
}

operator_case! {
    GpuHashJoin,
    fn bug_right_with_a_zero_row_probe_between_two_with_rows_refuses_the_second_on_the_device() {
        gpu_refuses_with(&run_both(&join(JoinType::Right), empty_between()), BUILD_COPY);
    }
}

operator_case! {
    GpuHashJoin,
    fn bug_right_finishing_after_only_zero_row_probes_refuses_the_second_on_the_device() {
        gpu_refuses_with(&run_both(&join(JoinType::Right), only_empty_probes()), BUILD_COPY);
    }
}

operator_case! {
    GpuHashJoin,
    fn bug_full_over_a_zero_row_build_refuses_the_probe_on_the_device() {
        gpu_refuses_with(&run_both(&join(JoinType::Full), empty_build()), PROBE_COPY);
    }
}

operator_case! {
    GpuHashJoin,
    fn bug_full_over_a_zero_row_probe_refuses_it_on_the_device() {
        gpu_refuses_with(&run_both(&join(JoinType::Full), empty_probe()), PROBE_COPY);
    }
}

operator_case! {
    GpuHashJoin,
    fn bug_full_over_both_sides_empty_refuses_the_probe_on_the_device() {
        gpu_refuses_with(&run_both(&join(JoinType::Full), both_empty()), PROBE_COPY);
    }
}

operator_case! {
    GpuHashJoin,
    fn bug_full_with_no_build_batch_is_refused_on_both() {
        both_refuse_with(&run_both(&join(JoinType::Full), no_build()), NO_BUILD);
    }
}

operator_case! {
    GpuHashJoin,
    fn bug_full_with_a_zero_row_probe_between_two_with_rows_refuses_the_first_on_the_device() {
        gpu_refuses_with(&run_both(&join(JoinType::Full), empty_between()), PROBE_COPY);
    }
}

operator_case! {
    GpuHashJoin,
    fn bug_full_finishing_after_only_zero_row_probes_refuses_the_first_on_the_device() {
        gpu_refuses_with(&run_both(&join(JoinType::Full), only_empty_probes()), PROBE_COPY);
    }
}

// #173 — the finish over no keys owes every build row padded with a typed NULL per probe column.
operator_case! {
    GpuHashJoin,
    fn bug_full_finishing_with_no_probe_batch_is_refused_on_the_device() {
        let outcome = run_both(&join(JoinType::Full), no_probe());
        gpu_refuses_with(&outcome, NO_KEYS);
        gpu_refuses_with(&outcome, "every build row padded with a typed NULL per probe column");
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

// #173 — the finish over no keys owes no rows, which is a table of no rows.
operator_case! {
    GpuHashJoin,
    fn bug_left_semi_finishing_with_no_probe_batch_is_refused_on_the_device() {
        let outcome = run_both(&join(JoinType::LeftSemi), no_probe());
        gpu_refuses_with(&outcome, NO_KEYS);
        gpu_refuses_with(&outcome, "no rows, which is a table of no rows");
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
    fn bug_right_semi_with_a_zero_row_probe_between_two_with_rows_refuses_the_second_on_the_device() {
        gpu_refuses_with(&run_both(&join(JoinType::RightSemi), empty_between()), BUILD_COPY);
    }
}

operator_case! {
    GpuHashJoin,
    fn bug_right_semi_finishing_after_only_zero_row_probes_refuses_the_second_on_the_device() {
        gpu_refuses_with(&run_both(&join(JoinType::RightSemi), only_empty_probes()), BUILD_COPY);
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
    fn bug_left_anti_with_a_zero_row_probe_between_two_with_rows_drops_null_key_build_rows_on_the_device() {
        device_answers_as_if_null_equals_null(JoinType::LeftAnti, false, empty_between);
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

operator_case! {
    GpuHashJoin,
    fn bug_right_anti_with_no_build_batch_is_refused_on_both() {
        both_refuse_with(&run_both(&join(JoinType::RightAnti), no_build()), NO_BUILD);
    }
}

operator_case! {
    GpuHashJoin,
    fn bug_right_anti_with_a_zero_row_probe_between_two_with_rows_refuses_the_second_on_the_device() {
        gpu_refuses_with(&run_both(&join(JoinType::RightAnti), empty_between()), BUILD_COPY);
    }
}

operator_case! {
    GpuHashJoin,
    fn bug_right_anti_finishing_after_only_zero_row_probes_refuses_the_second_on_the_device() {
        gpu_refuses_with(&run_both(&join(JoinType::RightAnti), only_empty_probes()), BUILD_COPY);
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
    fn bug_left_mark_with_a_zero_row_probe_between_two_with_rows_marks_null_key_build_rows_true_on_the_device() {
        device_answers_as_if_null_equals_null(JoinType::LeftMark, false, empty_between);
    }
}

operator_case! {
    GpuHashJoin,
    fn left_mark_finishing_after_only_zero_row_probes_agrees() {
        run_both(&join(JoinType::LeftMark), only_empty_probes()).same(Order::Any);
    }
}

// #173 — the finish over no keys owes every build row with a false mark.
operator_case! {
    GpuHashJoin,
    fn bug_left_mark_finishing_with_no_probe_batch_is_refused_on_the_device() {
        let outcome = run_both(&join(JoinType::LeftMark), no_probe());
        gpu_refuses_with(&outcome, NO_KEYS);
        gpu_refuses_with(&outcome, "every build row with a false mark");
    }
}
