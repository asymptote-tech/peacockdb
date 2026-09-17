//! `GpuHashJoin` along the dimensions the corpus varies and task 9 held constant: a
//! projection on every type the device reaches, the key's type on the three code paths,
//! the residual's combinations. The builders here are this file's; the sides, scripts and
//! `hash_join` are `join_cases.rs`'s. Every case is green or a `bug_` test with its ticket
//! above it; nothing here repairs.

use std::collections::HashSet;
use std::sync::Arc;

use datafusion::arrow::array::{Array, AsArray, Int32Array};
use datafusion::arrow::compute::kernels::numeric::rem;
use datafusion::arrow::compute::{cast, concat_batches};
use datafusion::arrow::datatypes::{DataType, Field, Int64Type, Schema as ArrowSchema};
use datafusion::arrow::record_batch::RecordBatch;
use datafusion::common::{JoinType, ScalarValue};

use super::join_cases::{
    BUILD_COPY, build_batch, empty_build, gpu_refuses_with, hash_join, hash_join_with, one_probe,
    padded, probe_batch, residual, script, side, two_probes,
};
use super::script::{Script, run_both};
use crate::executor::BackendError;
use crate::plan::{
    BatchLayout, BinaryOp, Expr, GpuHashJoin, GpuNode, JoinFilterColumn, JoinSide, Schema,
};
use crate::tests::compare::{Order, Slot};
use crate::tests::given::Given;
use crate::tests::synthetic::{prefixed, synthetic};

fn string_columns() -> Vec<JoinFilterColumn> {
    vec![
        JoinFilterColumn {
            side: JoinSide::Build,
            index: 5,
        },
        JoinFilterColumn {
            side: JoinSide::Probe,
            index: 5,
        },
    ]
}

fn strings_equal() -> Expr {
    Expr::binary(
        Expr::column(0, "b_s"),
        BinaryOp::Eq,
        Expr::column(1, "p_s"),
        DataType::Boolean,
    )
}

/// `b_s = p_s`: two string columns of one type, which `is_ast_able` admits — it refuses a
/// string only as a literal — so cuDF's AST evaluates the comparison.
fn string_residual() -> (Option<Expr>, Vec<JoinFilterColumn>) {
    (Some(strings_equal()), string_columns())
}

/// `b_s = p_s AND p_s <> ''`: the literal is what `is_ast_able` refuses, so the column path
/// evaluates the conjunction and the literal comparison (`cudf::binary_operation` over
/// strings); the equality beneath is an AST subtree of its own.
fn string_literal_residual() -> (Option<Expr>, Vec<JoinFilterColumn>) {
    let not_empty = Expr::binary(
        Expr::column(1, "p_s"),
        BinaryOp::NotEq,
        Expr::Literal(ScalarValue::Utf8(Some(String::new()))),
        DataType::Boolean,
    );
    (
        Some(Expr::binary(
            strings_equal(),
            BinaryOp::And,
            not_empty,
            DataType::Boolean,
        )),
        string_columns(),
    )
}

/// `CAST(b_i64 AS DECIMAL(20, 0)) < CAST(p_i64 AS DECIMAL(20, 0))`: a decimal operand is
/// what `is_ast_able` refuses, so the column path evaluates it.
fn decimal_residual() -> (Option<Expr>, Vec<JoinFilterColumn>) {
    let dec = |i: u32, name: &str| Expr::Cast {
        expr: Box::new(Expr::column(i, name)),
        target: DataType::Decimal128(20, 0),
    };
    (
        Some(Expr::binary(
            dec(0, "b_i64"),
            BinaryOp::Lt,
            dec(1, "p_i64"),
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

/// A projection over `output_of(join_type)` that reorders, drops every key, and takes
/// from both sides where the type keeps both: what the corpus does 634 times.
fn crossing_projection(join_type: JoinType) -> Vec<u32> {
    match join_type {
        // both sides: probe's s, build's f64, probe's id, build's d
        JoinType::Inner | JoinType::Right => vec![13, 4, 8, 6],
        // build side only: d, f64, id
        JoinType::LeftSemi | JoinType::LeftAnti => vec![6, 4, 0],
        // build side and the mark: mark, s, id
        JoinType::LeftMark => vec![8, 5, 0],
        // probe side only: b, i64, id
        JoinType::RightSemi | JoinType::RightAnti => vec![7, 3, 0],
        JoinType::Left | JoinType::Full => unreachable!("#152 refuses the first probe batch"),
    }
}

/// The key types the corpus joins on that task 9 did not: a composite key, the `Int64` on
/// nearly every join, a string, a date. Strings are `Utf8`, the type the data has and the
/// spec declares.
#[derive(Clone, Copy)]
enum Key {
    Composite,
    Int64,
    Utf8,
    Date32,
}

impl Key {
    /// What column 1 is cast to in the batch, and what the leaf declares it as.
    fn data_type(self) -> DataType {
        match self {
            Key::Composite => DataType::Int32,
            Key::Int64 => DataType::Int64,
            Key::Utf8 => DataType::Utf8,
            Key::Date32 => DataType::Date32,
        }
    }

    fn pairs(self) -> Vec<(u32, u32)> {
        match self {
            Key::Composite => vec![(1, 1), (2, 2)],
            _ => vec![(1, 1)],
        }
    }
}

/// `synthetic(rows, seed)` with its `key` column cast to the key's data type, under a field
/// of that type. `key` has seven values and nulls, so every side has matches, misses and a
/// null to leave out. The composite's second key is `i32 % 5`: `i32` itself has a thousand
/// values, and a pair over it never matches.
fn keyed(rows: usize, seed: u64, key: Key) -> RecordBatch {
    let batch = synthetic(rows, seed);
    let mut columns = batch.columns().to_vec();
    columns[1] = cast(&columns[1], &key.data_type()).expect("Int32 casts to every key type");
    if let Key::Composite = key {
        columns[2] = rem(&columns[2], &Int32Array::new_scalar(5)).expect("Int32 % Int32");
    }
    let fields: Vec<Field> = batch
        .schema()
        .fields()
        .iter()
        .enumerate()
        .map(|(i, f)| {
            if i == 1 {
                Field::new("key", key.data_type(), true)
            } else {
                f.as_ref().clone()
            }
        })
        .collect();
    RecordBatch::try_new(Arc::new(ArrowSchema::new(fields)), columns)
        .expect("the same columns under the keyed schema")
}

/// `side(prefix)` with column 1 declared as the key's type.
fn keyed_side(prefix: &str, key: Key) -> Vec<Field> {
    side(prefix)
        .into_iter()
        .enumerate()
        .map(|(i, f)| {
            if i == 1 {
                Field::new(f.name(), key.data_type(), true)
            } else {
                f
            }
        })
        .collect()
}

/// `hash_join` over keyed leaves, on the three types whose code paths differ: the
/// per-batch join, the side swap, the accumulated-keys finish. Same output rules as
/// `output_of`, over the keyed fields.
fn hash_join_keyed(join_type: JoinType, key: Key, projection: Option<Vec<u32>>) -> GpuHashJoin {
    let b = keyed_side("b_", key);
    let p = keyed_side("p_", key);
    let fields: Vec<Field> = match join_type {
        JoinType::Inner => [b.clone(), p.clone()].concat(),
        JoinType::Right => [padded(b.clone()), p.clone()].concat(),
        JoinType::LeftAnti => b.clone(),
        other => unreachable!("{other:?} is not one of the three key-path types"),
    };
    let fields = match &projection {
        None => fields,
        Some(keep) => keep.iter().map(|i| fields[*i as usize].clone()).collect(),
    };
    let leaf = |fields: Vec<Field>, batches: BatchLayout| -> Box<dyn GpuNode> {
        Given::of(Schema::new(Arc::new(ArrowSchema::new(fields))), batches)
    };
    GpuHashJoin::new(
        leaf(b, BatchLayout::SingleBatch),
        leaf(p, BatchLayout::MultipleBatches),
        join_type,
        key.pairs(),
        None,
        Vec::new(),
        false,
        projection,
        Schema::new(Arc::new(ArrowSchema::new(fields))),
    )
}

/// A probe over the whole key domain: every build key matched, and null keys on both sides.
fn keyed_script(key: Key) -> Script {
    script(
        Some(prefixed(&keyed(32, 11, key), "b_")),
        vec![prefixed(&keyed(48, 3, key), "p_")],
    )
}

/// A four-row probe with no null in either key column, so an anti join has build rows to
/// keep and #59 has no null pair to match; the composite still has one match.
fn keyed_anti_script(key: Key) -> Script {
    script(
        Some(prefixed(&keyed(32, 11, key), "b_")),
        vec![prefixed(&keyed(4, 3, key), "p_")],
    )
}

/// The eight-row probe `keyed_anti_script` was cut down from: row 4's `i32` is null, so on
/// the composite key it is a null pair against every build row that shares its `key` and
/// carries a null `i32` of its own.
fn composite_anti_script_with_a_null_second_key() -> Script {
    script(
        Some(prefixed(&keyed(32, 11, Key::Composite), "b_")),
        vec![prefixed(&keyed(8, 3, Key::Composite), "p_")],
    )
}

/// Two probe batches with no null key and two key values never drawn, so an anti or mark
/// form has build rows to keep and #59 has no null pair to match.
fn probes_with_misses() -> Script {
    script(
        Some(build_batch(32)),
        vec![probe_batch(4, 21), probe_batch(4, 22)],
    )
}

/// A build with no null key and one key value never drawn, so a right anti has probe rows
/// to keep and #59 has no null pair to match.
fn build_with_misses() -> Script {
    script(Some(build_batch(8)), vec![probe_batch(48, 21)])
}

// A crossing projection on every type the device reaches: reordered, every key dropped,
// both sides where the type keeps both. Over two probe batches where the device streams,
// so the per-batch join and the finish both run under it; the probe-side types whose
// second batch is #152's refusal (pinned in `join_cases.rs`) take one. Left and Full are
// that refusal before any projection. The anti and mark forms take a script with misses
// and no null pair, so they have rows to project and #59 (pinned in `join_cases.rs`)
// nothing to match.

operator_case! {
    GpuHashJoin,
    fn a_right_join_over_one_probe_batch_with_a_crossing_projection_agrees() {
        let node = hash_join(JoinType::Right, false, false, Some(crossing_projection(JoinType::Right)));
        run_both(&node, one_probe()).same(Order::Any);
    }
}

// #152 — the first call erased the build side and nothing copies it; the projection is
// proved over one batch above.
operator_case! {
    GpuHashJoin,
    fn bug_a_right_join_with_a_crossing_projection_refuses_its_second_probe_batch_on_the_device() {
        let node = hash_join(JoinType::Right, false, false, Some(crossing_projection(JoinType::Right)));
        gpu_refuses_with(&run_both(&node, two_probes()), BUILD_COPY);
    }
}

operator_case! {
    GpuHashJoin,
    fn a_right_semi_join_with_a_crossing_projection_agrees() {
        let node = hash_join(JoinType::RightSemi, false, false, Some(crossing_projection(JoinType::RightSemi)));
        run_both(&node, one_probe()).same(Order::Any);
    }
}

operator_case! {
    GpuHashJoin,
    fn a_right_anti_join_with_a_crossing_projection_agrees() {
        let node = hash_join(JoinType::RightAnti, false, false, Some(crossing_projection(JoinType::RightAnti)));
        run_both(&node, build_with_misses()).same(Order::Any);
    }
}

operator_case! {
    GpuHashJoin,
    fn a_left_semi_join_with_a_crossing_projection_agrees() {
        let node = hash_join(JoinType::LeftSemi, false, false, Some(crossing_projection(JoinType::LeftSemi)));
        run_both(&node, two_probes()).same(Order::Any);
    }
}

operator_case! {
    GpuHashJoin,
    fn a_left_anti_join_with_a_crossing_projection_agrees() {
        let node = hash_join(JoinType::LeftAnti, false, false, Some(crossing_projection(JoinType::LeftAnti)));
        run_both(&node, probes_with_misses()).same(Order::Any);
    }
}

operator_case! {
    GpuHashJoin,
    fn a_left_mark_join_with_a_crossing_projection_agrees() {
        let node = hash_join(JoinType::LeftMark, false, false, Some(crossing_projection(JoinType::LeftMark)));
        run_both(&node, probes_with_misses()).same(Order::Any);
    }
}

// The projection's empty shape: a zero-row build on the swapping type, which owes every
// probe row padded, and on the finishing type, which owes nothing.

operator_case! {
    GpuHashJoin,
    fn right_over_a_zero_row_build_with_a_crossing_projection_pads_every_probe_row() {
        let node = hash_join(JoinType::Right, false, false, Some(crossing_projection(JoinType::Right)));
        run_both(&node, empty_build()).same(Order::Any);
    }
}

operator_case! {
    GpuHashJoin,
    fn left_anti_over_a_zero_row_build_with_a_crossing_projection_answers_nothing() {
        let node = hash_join(JoinType::LeftAnti, false, false, Some(crossing_projection(JoinType::LeftAnti)));
        run_both(&node, empty_build()).same(Order::Any);
    }
}

// The key's type on each of the three code paths: Inner is the per-batch join, Right has
// cuDF swap the sides and the indices swapped back, LeftAnti is the accumulated-keys
// finish. The anti cases take the probe with misses, so there are rows to keep and no
// null pair for #59. String keys are `Utf8`, the type the spec declares.

operator_case! {
    GpuHashJoin,
    fn an_inner_join_on_a_composite_key_agrees() {
        run_both(&hash_join_keyed(JoinType::Inner, Key::Composite, None), keyed_script(Key::Composite))
            .same(Order::Any);
    }
}

operator_case! {
    GpuHashJoin,
    fn an_inner_join_on_an_int64_key_agrees() {
        run_both(&hash_join_keyed(JoinType::Inner, Key::Int64, None), keyed_script(Key::Int64))
            .same(Order::Any);
    }
}

operator_case! {
    GpuHashJoin,
    fn an_inner_join_on_a_utf8_key_agrees() {
        run_both(&hash_join_keyed(JoinType::Inner, Key::Utf8, None), keyed_script(Key::Utf8))
            .same(Order::Any);
    }
}

operator_case! {
    GpuHashJoin,
    fn an_inner_join_on_a_date32_key_agrees() {
        run_both(&hash_join_keyed(JoinType::Inner, Key::Date32, None), keyed_script(Key::Date32))
            .same(Order::Any);
    }
}

operator_case! {
    GpuHashJoin,
    fn a_right_join_on_a_composite_key_agrees() {
        run_both(&hash_join_keyed(JoinType::Right, Key::Composite, None), keyed_script(Key::Composite))
            .same(Order::Any);
    }
}

operator_case! {
    GpuHashJoin,
    fn a_right_join_on_an_int64_key_agrees() {
        run_both(&hash_join_keyed(JoinType::Right, Key::Int64, None), keyed_script(Key::Int64))
            .same(Order::Any);
    }
}

operator_case! {
    GpuHashJoin,
    fn a_right_join_on_a_utf8_key_agrees() {
        run_both(&hash_join_keyed(JoinType::Right, Key::Utf8, None), keyed_script(Key::Utf8))
            .same(Order::Any);
    }
}

operator_case! {
    GpuHashJoin,
    fn a_right_join_on_a_date32_key_agrees() {
        run_both(&hash_join_keyed(JoinType::Right, Key::Date32, None), keyed_script(Key::Date32))
            .same(Order::Any);
    }
}

operator_case! {
    GpuHashJoin,
    fn a_left_anti_join_on_a_composite_key_agrees() {
        let node = hash_join_keyed(JoinType::LeftAnti, Key::Composite, None);
        run_both(&node, keyed_anti_script(Key::Composite)).same(Order::Any);
    }
}

// #59 — the anti join's hardcoded `EQUAL` reaches the second column of a composite key: a
// null `i32` on the probe matches the build rows of its `key` whose `i32` is null too, and
// the device drops them where SQL keeps them. The cpu keeps 30 of 32 build rows, the
// device 27, and every row the device alone dropped carries the null.
operator_case! {
    GpuHashJoin,
    fn bug_a_left_anti_join_on_a_composite_key_matches_a_null_in_the_second_column_on_the_device() {
        let node = hash_join_keyed(JoinType::LeftAnti, Key::Composite, None);
        let outcome = run_both(&node, composite_anti_script_with_a_null_second_key());
        let kept = |side: &Result<Vec<Slot>, BackendError>| -> RecordBatch {
            let batches: Vec<RecordBatch> = side.as_ref().expect("answers").concat();
            concat_batches(&batches[0].schema(), &batches).expect("one schema")
        };
        let (cpu, gpu) = (kept(&outcome.cpu), kept(&outcome.gpu));
        assert_eq!(cpu.num_rows(), 30, "the cpu keeps every build row no probe pair equals");
        assert_eq!(gpu.num_rows(), 27, "the device matches the null pair too");
        let device_kept: HashSet<i64> =
            gpu.column(0).as_primitive::<Int64Type>().values().iter().copied().collect();
        let ids = cpu.column(0).as_primitive::<Int64Type>();
        let dropped_beyond_the_cpu: Vec<usize> = (0..cpu.num_rows())
            .filter(|r| !device_kept.contains(&ids.value(*r)))
            .collect();
        assert_eq!(dropped_beyond_the_cpu.len(), 3);
        for row in dropped_beyond_the_cpu {
            assert!(
                cpu.column(2).is_null(row),
                "build row {} was dropped over a non-null second key",
                ids.value(row)
            );
        }
    }
}

operator_case! {
    GpuHashJoin,
    fn a_left_anti_join_on_an_int64_key_agrees() {
        let node = hash_join_keyed(JoinType::LeftAnti, Key::Int64, None);
        run_both(&node, keyed_anti_script(Key::Int64)).same(Order::Any);
    }
}

operator_case! {
    GpuHashJoin,
    fn a_left_anti_join_on_a_utf8_key_agrees() {
        let node = hash_join_keyed(JoinType::LeftAnti, Key::Utf8, None);
        run_both(&node, keyed_anti_script(Key::Utf8)).same(Order::Any);
    }
}

operator_case! {
    GpuHashJoin,
    fn a_left_anti_join_on_a_date32_key_agrees() {
        let node = hash_join_keyed(JoinType::LeftAnti, Key::Date32, None);
        run_both(&node, keyed_anti_script(Key::Date32)).same(Order::Any);
    }
}

// The key's empty shape: a string key over zero rows crosses the boundary once.
operator_case! {
    GpuHashJoin,
    fn an_inner_join_on_a_utf8_key_over_a_zero_row_probe_answers_zero_rows() {
        let zero_row_probe = script(
            Some(prefixed(&keyed(32, 11, Key::Utf8), "b_")),
            vec![prefixed(&keyed(0, 3, Key::Utf8), "p_")],
        );
        run_both(&hash_join_keyed(JoinType::Inner, Key::Utf8, None), zero_row_probe).same(Order::Any);
    }
}

// The residual in the combinations the corpus has and task 9 did not: with a projection,
// under `null_equals_null`, over a streamed probe, a string comparison on the AST, and the
// column path — a string literal and a decimal cast, which the AST does not take.

operator_case! {
    GpuHashJoin,
    fn an_inner_join_with_a_residual_and_a_crossing_projection_agrees() {
        let node = hash_join_with(JoinType::Inner, false, residual(), Some(crossing_projection(JoinType::Inner)));
        run_both(&node, one_probe()).same(Order::Any);
    }
}

operator_case! {
    GpuHashJoin,
    fn an_inner_join_with_a_residual_under_null_equals_null_agrees() {
        run_both(&hash_join_with(JoinType::Inner, true, residual(), None), one_probe()).same(Order::Any);
    }
}

// #152 — the first call erased the build side and nothing copies it, residual or not.
operator_case! {
    GpuHashJoin,
    fn bug_an_inner_join_with_a_residual_refuses_its_second_probe_batch_on_the_device() {
        let node = hash_join_with(JoinType::Inner, false, residual(), None);
        gpu_refuses_with(&run_both(&node, two_probes()), BUILD_COPY);
    }
}

operator_case! {
    GpuHashJoin,
    fn an_inner_join_with_a_string_residual_on_the_ast_agrees() {
        run_both(&hash_join_with(JoinType::Inner, false, string_residual(), None), one_probe()).same(Order::Any);
    }
}

operator_case! {
    GpuHashJoin,
    fn an_inner_join_with_a_string_literal_residual_on_the_column_path_agrees() {
        let node = hash_join_with(JoinType::Inner, false, string_literal_residual(), None);
        run_both(&node, one_probe()).same(Order::Any);
    }
}

operator_case! {
    GpuHashJoin,
    fn an_inner_join_with_a_decimal_residual_agrees() {
        run_both(&hash_join_with(JoinType::Inner, false, decimal_residual(), None), one_probe()).same(Order::Any);
    }
}
