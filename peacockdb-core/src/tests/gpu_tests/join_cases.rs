//! `GpuHashJoin` through the harness: a build batch first and once, the probe streamed,
//! nine types over the same two prefixed synthetic sides joined on `key`. The scripts
//! follow the capability matrix, and the filtered forms exist only where it admits one.
//! Every case is green or a `bug_` test with its ticket above it; nothing here repairs.

use std::sync::Arc;

use datafusion::arrow::array::Int32Array;
use datafusion::arrow::compute::cast;
use datafusion::arrow::compute::kernels::numeric::rem;
use datafusion::arrow::datatypes::{DataType, Field, Schema as ArrowSchema};
use datafusion::arrow::record_batch::RecordBatch;
use datafusion::common::JoinType;

use super::script::{Outcome, Script, run_both};
use crate::plan::{
    BatchLayout, BinaryOp, Expr, GpuHashJoin, GpuNode, JoinFilterColumn, JoinSide, Schema,
};
use crate::tests::compare::{Order, assert_same, same};
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

/// `b_s = p_s`, a string comparison: the AST has no string ops, so the column path
/// evaluates it.
fn string_residual() -> (Option<Expr>, Vec<JoinFilterColumn>) {
    (
        Some(Expr::binary(
            Expr::column(0, "b_s"),
            BinaryOp::Eq,
            Expr::column(1, "p_s"),
            DataType::Boolean,
        )),
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

fn hash_join_with(
    join_type: JoinType,
    null_equals_null: bool,
    residual: (Option<Expr>, Vec<JoinFilterColumn>),
    projection: Option<Vec<u32>>,
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
        vec![(1, 1)],
        filter,
        filter_columns,
        null_equals_null,
        projection,
        Schema::new(Arc::new(ArrowSchema::new(fields))),
    )
}

fn hash_join(
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
/// nearly every join, a string, a date. `Utf8ViewDeclared` retypes nothing in the batch —
/// the leaf *declares* `Utf8View` over `Utf8` data, the corpus's own situation, since
/// cuDF's `from_arrow` cannot upload a `Utf8View` array. `Utf8` is that key's oracle: the
/// same strings under the type the data has.
#[derive(Clone, Copy)]
enum Key {
    Composite,
    Int64,
    Utf8,
    Utf8ViewDeclared,
    Date32,
}

impl Key {
    /// What column 1 is cast to in the batch. `Utf8` for the declared view: the data.
    fn data_type(self) -> DataType {
        match self {
            Key::Composite => DataType::Int32,
            Key::Int64 => DataType::Int64,
            Key::Utf8 | Key::Utf8ViewDeclared => DataType::Utf8,
            Key::Date32 => DataType::Date32,
        }
    }

    /// What the leaf declares column 1 as.
    fn declared_type(self) -> DataType {
        match self {
            Key::Utf8ViewDeclared => DataType::Utf8View,
            other => other.data_type(),
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

/// `side(prefix)` with column 1 declared as the key's declared type.
fn keyed_side(prefix: &str, key: Key) -> Vec<Field> {
    side(prefix)
        .into_iter()
        .enumerate()
        .map(|(i, f)| {
            if i == 1 {
                Field::new(f.name(), key.declared_type(), true)
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

fn keyed_zero_row_probe(key: Key) -> Script {
    script(
        Some(prefixed(&keyed(32, 11, key), "b_")),
        vec![prefixed(&keyed(0, 3, key), "p_")],
    )
}

/// Every column but the key, both sides where kept: what a Utf8View-keyed case projects to,
/// so the join is what the comparison reads and not the export (#183).
fn without_key(join_type: JoinType) -> Vec<u32> {
    let one_side = |base: u32| (0..8u32).filter(|i| *i != 1).map(move |i| base + i);
    match join_type {
        JoinType::Inner | JoinType::Right => one_side(0).chain(one_side(8)).collect(),
        JoinType::LeftAnti => one_side(0).collect(),
        other => unreachable!("{other:?}"),
    }
}

/// The device under a declared-Utf8View key answers as the cpu under a Utf8 key, the key
/// projected away on both. The cpu cannot take the declaration itself: its join
/// concatenates the build side under the declared schema and refuses `Utf8` data there,
/// so the same join on a `Utf8` key is the oracle — the harness gap is in the detail file.
fn device_on_a_declared_utf8view_key_answers_as_the_cpu_on_a_utf8_key(
    join_type: JoinType,
    script: fn(Key) -> Script,
) {
    let projection = Some(without_key(join_type));
    let declared = hash_join_keyed(join_type, Key::Utf8ViewDeclared, projection.clone());
    let device = run_both(&declared, script(Key::Utf8ViewDeclared));
    let oracle = run_both(
        &hash_join_keyed(join_type, Key::Utf8, projection),
        script(Key::Utf8),
    );
    assert_same(
        oracle.cpu.as_ref().expect("the cpu answers on a Utf8 key"),
        device
            .gpu
            .as_ref()
            .unwrap_or_else(|why| panic!("gpu refused: {}", why.message)),
        Order::Any,
    );
}

/// The arrow type the device handed up in `slot` at `column`.
fn exported_type(outcome: &Outcome, slot: usize, column: usize) -> DataType {
    let gpu = outcome.gpu.as_ref().expect("the device answers");
    gpu[slot][0].schema().field(column).data_type().clone()
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
///
/// Two assertions, because the first is relative. The oracle comparison names the direction
/// of the divergence; the second says there is one. #59 can close either way — the device
/// honouring the flag, or the cpu hardcoding `EQUAL` too — and both end with the backends
/// agreeing under the default, which is what turns this red and retires the pin.
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
    assert!(
        same(
            device.cpu.as_ref().expect("the cpu answers"),
            device.gpu.as_ref().expect("the device answers"),
            Order::Any,
        )
        .is_err(),
        "the backends agree under the SQL default: #59 is closed, and this pin with it"
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

// A streamed probe: two batches, every type. Left and Full refuse their first batch
// already, so the shape is named and the refusal is the same one.

// #152 — the first call erased the build side and nothing copies it.
operator_case! {
    GpuHashJoin,
    fn bug_an_inner_join_refuses_its_second_probe_batch_on_the_device() {
        gpu_refuses_with(&run_both(&join(JoinType::Inner), two_probes()), BUILD_COPY);
    }
}

// #152 — the key project and the join both read the probe batch, and nothing copies it.
operator_case! {
    GpuHashJoin,
    fn bug_a_left_join_over_two_probe_batches_refuses_the_first_on_the_device() {
        gpu_refuses_with(&run_both(&join(JoinType::Left), two_probes()), PROBE_COPY);
    }
}

// #152 — the first call erased the build side and nothing copies it.
operator_case! {
    GpuHashJoin,
    fn bug_a_right_join_refuses_its_second_probe_batch_on_the_device() {
        gpu_refuses_with(&run_both(&join(JoinType::Right), two_probes()), BUILD_COPY);
    }
}

// #152 — the key project and the join both read the probe batch, and nothing copies it.
operator_case! {
    GpuHashJoin,
    fn bug_a_full_join_over_two_probe_batches_refuses_the_first_on_the_device() {
        gpu_refuses_with(&run_both(&join(JoinType::Full), two_probes()), PROBE_COPY);
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
// both engines; the SQL default is where anti and mark part company (#59, above). Left and
// Full have no case here: the device refuses their first probe batch before the flag matters.

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

// The other half of #59's RightAnti pin: under `true` the two engines agree outright.
operator_case! {
    GpuHashJoin,
    fn a_right_anti_join_under_null_equals_null_agrees() {
        run_both(&hash_join(JoinType::RightAnti, true, false, None), one_probe()).same(Order::Any);
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

// #59 — the device hardcodes EQUAL for the null keys of an anti or mark join.
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

// A crossing projection on every type the device reaches: reordered, every key dropped,
// both sides where the type keeps both. Over two probe batches where the device streams,
// so the per-batch join and the finish both run under it; the probe-side types whose
// second batch is #152's refusal (pinned above) take one. Left and Full are that refusal
// before any projection. The anti and mark forms take a script with misses and no null
// pair, so they have rows to project and #59 (pinned above) nothing to match.

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
// null pair for #59. A declared-Utf8View key is read against the Utf8-keyed join, the key
// projected away on both; one case per path keeps it, and is #183's pin at the join.

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
    fn an_inner_join_on_a_declared_utf8view_key_answers_on_the_device_as_on_a_utf8_key() {
        device_on_a_declared_utf8view_key_answers_as_the_cpu_on_a_utf8_key(JoinType::Inner, keyed_script);
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
    fn a_right_join_on_a_declared_utf8view_key_answers_on_the_device_as_on_a_utf8_key() {
        device_on_a_declared_utf8view_key_answers_as_the_cpu_on_a_utf8_key(JoinType::Right, keyed_script);
    }
}

operator_case! {
    GpuHashJoin,
    fn a_left_anti_join_on_a_composite_key_agrees() {
        let node = hash_join_keyed(JoinType::LeftAnti, Key::Composite, None);
        run_both(&node, keyed_anti_script(Key::Composite)).same(Order::Any);
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

operator_case! {
    GpuHashJoin,
    fn a_left_anti_join_on_a_declared_utf8view_key_answers_on_the_device_as_on_a_utf8_key() {
        device_on_a_declared_utf8view_key_answers_as_the_cpu_on_a_utf8_key(JoinType::LeftAnti, keyed_anti_script);
    }
}

// #183 — the join keeps its declared-Utf8View key, and the device hands it up as `Utf8`;
// at a sink that is the unload pin's refusal. The cpu cannot take the declaration in this
// harness (its join refuses `Utf8` data under it, the detail file's gap) and is not read.
operator_case! {
    GpuHashJoin,
    fn bug_an_inner_join_keeping_a_declared_utf8view_key_hands_it_up_as_utf8_from_the_device() {
        let node = hash_join_keyed(JoinType::Inner, Key::Utf8ViewDeclared, None);
        let outcome = run_both(&node, keyed_script(Key::Utf8ViewDeclared));
        assert_eq!(exported_type(&outcome, 0, 1), DataType::Utf8, "b_key");
        assert_eq!(exported_type(&outcome, 0, 9), DataType::Utf8, "p_key");
    }
}

// #183 — the same at the swap: Right keeps both sides, so both keys come back `Utf8`.
operator_case! {
    GpuHashJoin,
    fn bug_a_right_join_keeping_a_declared_utf8view_key_hands_it_up_as_utf8_from_the_device() {
        let node = hash_join_keyed(JoinType::Right, Key::Utf8ViewDeclared, None);
        let outcome = run_both(&node, keyed_script(Key::Utf8ViewDeclared));
        assert_eq!(exported_type(&outcome, 0, 1), DataType::Utf8, "b_key");
        assert_eq!(exported_type(&outcome, 0, 9), DataType::Utf8, "p_key");
    }
}

// #183 — the same at the finish, whose slot is the second: LeftAnti keeps the build side.
operator_case! {
    GpuHashJoin,
    fn bug_a_left_anti_join_keeping_a_declared_utf8view_key_hands_it_up_as_utf8_from_the_device() {
        let node = hash_join_keyed(JoinType::LeftAnti, Key::Utf8ViewDeclared, None);
        let outcome = run_both(&node, keyed_anti_script(Key::Utf8ViewDeclared));
        assert_eq!(exported_type(&outcome, 1, 1), DataType::Utf8, "b_key");
    }
}

// The key's empty shape: a declared-Utf8View key over zero rows crosses the boundary once.
operator_case! {
    GpuHashJoin,
    fn an_inner_join_on_a_declared_utf8view_key_over_a_zero_row_probe_answers_zero_rows_on_the_device() {
        device_on_a_declared_utf8view_key_answers_as_the_cpu_on_a_utf8_key(JoinType::Inner, keyed_zero_row_probe);
    }
}

// The residual in the combinations the corpus has and task 9 did not: with a projection,
// under `null_equals_null`, over a streamed probe, and on the column path — a string
// comparison and a decimal cast, neither of which the AST takes.

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
    fn an_inner_join_with_a_string_residual_agrees() {
        run_both(&hash_join_with(JoinType::Inner, false, string_residual(), None), one_probe()).same(Order::Any);
    }
}

operator_case! {
    GpuHashJoin,
    fn an_inner_join_with_a_decimal_residual_agrees() {
        run_both(&hash_join_with(JoinType::Inner, false, decimal_residual(), None), one_probe()).same(Order::Any);
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

// #152 — the first call erased the build side and nothing copies it.
operator_case! {
    GpuHashJoin,
    fn bug_inner_with_a_zero_row_probe_between_two_with_rows_refuses_the_second_on_the_device() {
        gpu_refuses_with(&run_both(&join(JoinType::Inner), empty_between()), BUILD_COPY);
    }
}

// #152 — the first call erased the build side and nothing copies it.
operator_case! {
    GpuHashJoin,
    fn bug_inner_finishing_after_only_zero_row_probes_refuses_the_second_on_the_device() {
        gpu_refuses_with(&run_both(&join(JoinType::Inner), only_empty_probes()), BUILD_COPY);
    }
}

// #152 — the key project and the join both read the probe batch, and nothing copies it.
operator_case! {
    GpuHashJoin,
    fn bug_left_over_a_zero_row_build_refuses_the_probe_on_the_device() {
        gpu_refuses_with(&run_both(&join(JoinType::Left), empty_build()), PROBE_COPY);
    }
}

// #152 — the key project and the join both read the probe batch, and nothing copies it.
operator_case! {
    GpuHashJoin,
    fn bug_left_over_a_zero_row_probe_refuses_it_on_the_device() {
        gpu_refuses_with(&run_both(&join(JoinType::Left), empty_probe()), PROBE_COPY);
    }
}

// #152 — the key project and the join both read the probe batch, and nothing copies it.
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

// #152 — the key project and the join both read the probe batch, and nothing copies it.
operator_case! {
    GpuHashJoin,
    fn bug_left_with_a_zero_row_probe_between_two_with_rows_refuses_the_first_on_the_device() {
        gpu_refuses_with(&run_both(&join(JoinType::Left), empty_between()), PROBE_COPY);
    }
}

// #152 — the key project and the join both read the probe batch, and nothing copies it.
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

// #175 — with no build side the join owes its probe rows, over a table that does not exist.
operator_case! {
    GpuHashJoin,
    fn bug_right_with_no_build_batch_is_refused_on_both() {
        both_refuse_with(&run_both(&join(JoinType::Right), no_build()), NO_BUILD);
    }
}

// #152 — the first call erased the build side and nothing copies it.
operator_case! {
    GpuHashJoin,
    fn bug_right_with_a_zero_row_probe_between_two_with_rows_refuses_the_second_on_the_device() {
        gpu_refuses_with(&run_both(&join(JoinType::Right), empty_between()), BUILD_COPY);
    }
}

// #152 — the first call erased the build side and nothing copies it.
operator_case! {
    GpuHashJoin,
    fn bug_right_finishing_after_only_zero_row_probes_refuses_the_second_on_the_device() {
        gpu_refuses_with(&run_both(&join(JoinType::Right), only_empty_probes()), BUILD_COPY);
    }
}

// #152 — the key project and the join both read the probe batch, and nothing copies it.
operator_case! {
    GpuHashJoin,
    fn bug_full_over_a_zero_row_build_refuses_the_probe_on_the_device() {
        gpu_refuses_with(&run_both(&join(JoinType::Full), empty_build()), PROBE_COPY);
    }
}

// #152 — the key project and the join both read the probe batch, and nothing copies it.
operator_case! {
    GpuHashJoin,
    fn bug_full_over_a_zero_row_probe_refuses_it_on_the_device() {
        gpu_refuses_with(&run_both(&join(JoinType::Full), empty_probe()), PROBE_COPY);
    }
}

// #152 — the key project and the join both read the probe batch, and nothing copies it.
operator_case! {
    GpuHashJoin,
    fn bug_full_over_both_sides_empty_refuses_the_probe_on_the_device() {
        gpu_refuses_with(&run_both(&join(JoinType::Full), both_empty()), PROBE_COPY);
    }
}

// #175 — with no build side the join owes its probe rows, over a table that does not exist.
operator_case! {
    GpuHashJoin,
    fn bug_full_with_no_build_batch_is_refused_on_both() {
        both_refuse_with(&run_both(&join(JoinType::Full), no_build()), NO_BUILD);
    }
}

// #152 — the key project and the join both read the probe batch, and nothing copies it.
operator_case! {
    GpuHashJoin,
    fn bug_full_with_a_zero_row_probe_between_two_with_rows_refuses_the_first_on_the_device() {
        gpu_refuses_with(&run_both(&join(JoinType::Full), empty_between()), PROBE_COPY);
    }
}

// #152 — the key project and the join both read the probe batch, and nothing copies it.
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

// #152 — the first call erased the build side and nothing copies it.
operator_case! {
    GpuHashJoin,
    fn bug_right_semi_with_a_zero_row_probe_between_two_with_rows_refuses_the_second_on_the_device() {
        gpu_refuses_with(&run_both(&join(JoinType::RightSemi), empty_between()), BUILD_COPY);
    }
}

// #152 — the first call erased the build side and nothing copies it.
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

// #59 — the device hardcodes EQUAL for the null keys of an anti or mark join.
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

// #175 — with no build side the join owes its probe rows, over a table that does not exist.
operator_case! {
    GpuHashJoin,
    fn bug_right_anti_with_no_build_batch_is_refused_on_both() {
        both_refuse_with(&run_both(&join(JoinType::RightAnti), no_build()), NO_BUILD);
    }
}

// #152 — the first call erased the build side and nothing copies it.
operator_case! {
    GpuHashJoin,
    fn bug_right_anti_with_a_zero_row_probe_between_two_with_rows_refuses_the_second_on_the_device() {
        gpu_refuses_with(&run_both(&join(JoinType::RightAnti), empty_between()), BUILD_COPY);
    }
}

// #152 — the first call erased the build side and nothing copies it.
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

// #59 — the device hardcodes EQUAL for the null keys of an anti or mark join.
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
