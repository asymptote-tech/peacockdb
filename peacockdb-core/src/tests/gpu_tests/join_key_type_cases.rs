//! Every `key_types` column as a join key, Inner and LeftSemi, on both engines. The two sides
//! are one generator from one seed, so the build's 72 rows are the probe's first 72 and every
//! key column has matches, repeats where its values are few, misses in the probe's last 24 and a
//! NULL on each side — the widest NULL stride is 71. The floats' fifth rows hold -0.0 and the NaNs, which the cpu keys by bits
//! and the device by value (#243, pinned in `join_cases.rs`), so a float case leaves those
//! rows out of both sides and keys on the values the two engines agree on. Last, the two pbench
//! interval residuals over the same sides' `d`: a keyed one and a keyless one the session hoists.

use std::sync::Arc;

use datafusion::arrow::array::BooleanArray;
use datafusion::arrow::compute::filter_record_batch;
use datafusion::arrow::datatypes::{DataType, Field, IntervalMonthDayNano, Schema as ArrowSchema};
use datafusion::arrow::record_batch::RecordBatch;
use datafusion::common::{JoinType, ScalarValue};

use super::aggregate_cases::rows;
use super::join_cases::{output_over, script};
use super::script::run_both;
use crate::plan::{
    BatchLayout, BinaryOp, Expr, GpuHashJoin, GpuNestedLoopJoin, GpuNode, JoinFilterColumn,
    JoinSide, Schema,
};
use crate::tests::compare::Order;
use crate::tests::given::Given;
use crate::tests::synthetic::{key_types, key_types_schema, prefixed};

fn fields(prefix: &str) -> Vec<Field> {
    prefixed(&key_types(0, 0), prefix)
        .schema()
        .fields()
        .iter()
        .map(|f| f.as_ref().clone())
        .collect()
}

/// `rows` rows of `key_types`, without the float specials when `key` is a float column.
fn side(rows: usize, key: usize, prefix: &str) -> RecordBatch {
    let batch = key_types(rows, 5);
    let float = matches!(
        key_types_schema().field(key).data_type(),
        DataType::Float32 | DataType::Float64
    );
    let batch = if float {
        let ordinary = BooleanArray::from_iter((0..rows).map(|row| Some(row % 5 != 0)));
        filter_record_batch(&batch, &ordinary).expect("a mask of the batch's length")
    } else {
        batch
    };
    prefixed(&batch, prefix)
}

fn leaf(prefix: &str, batches: BatchLayout) -> Box<dyn GpuNode> {
    Given::of(
        Schema::new(Arc::new(ArrowSchema::new(fields(prefix)))),
        batches,
    )
}

/// Both engines join on column `key` of each side and answer alike.
fn joins_alike_on(join_type: JoinType, key: usize) {
    let output = output_over(join_type, fields("b_"), fields("p_"));
    let node = GpuHashJoin::new(
        leaf("b_", BatchLayout::SingleBatch),
        leaf("p_", BatchLayout::MultipleBatches),
        join_type,
        vec![(key as u32, key as u32)],
        None,
        Vec::new(),
        false,
        None,
        Schema::new(Arc::new(ArrowSchema::new(output))),
    );
    let build = side(72, key, "b_");
    let probe = side(96, key, "p_");
    run_both(&node, script(Some(build), vec![probe])).same(Order::Any);
}

/// One Inner and one LeftSemi case per column, named for the column's type.
macro_rules! keyed_on {
    ($($key:literal => $inner:ident, $semi:ident;)+) => {
        $(
            operator_case! {
                GpuHashJoin,
                fn $inner() { joins_alike_on(JoinType::Inner, $key) }
            }
            operator_case! {
                GpuHashJoin,
                fn $semi() { joins_alike_on(JoinType::LeftSemi, $key) }
            }
        )+
    };
}

keyed_on! {
    1 => an_inner_join_keyed_on_int8_agrees, a_left_semi_join_keyed_on_int8_agrees;
    2 => an_inner_join_keyed_on_int16_agrees, a_left_semi_join_keyed_on_int16_agrees;
    3 => an_inner_join_keyed_on_int32_agrees, a_left_semi_join_keyed_on_int32_agrees;
    4 => an_inner_join_keyed_on_int64_agrees, a_left_semi_join_keyed_on_int64_agrees;
    5 => an_inner_join_keyed_on_float32_agrees, a_left_semi_join_keyed_on_float32_agrees;
    6 => an_inner_join_keyed_on_float64_agrees, a_left_semi_join_keyed_on_float64_agrees;
    7 => an_inner_join_keyed_on_boolean_agrees, a_left_semi_join_keyed_on_boolean_agrees;
    8 => an_inner_join_keyed_on_date32_agrees, a_left_semi_join_keyed_on_date32_agrees;
    9 => an_inner_join_keyed_on_timestamp_s_agrees, a_left_semi_join_keyed_on_timestamp_s_agrees;
    10 => an_inner_join_keyed_on_timestamp_ms_agrees, a_left_semi_join_keyed_on_timestamp_ms_agrees;
    11 => an_inner_join_keyed_on_timestamp_us_agrees, a_left_semi_join_keyed_on_timestamp_us_agrees;
    12 => an_inner_join_keyed_on_timestamp_ns_agrees, a_left_semi_join_keyed_on_timestamp_ns_agrees;
    13 => an_inner_join_keyed_on_decimal15_agrees, a_left_semi_join_keyed_on_decimal15_agrees;
    14 => an_inner_join_keyed_on_decimal38_agrees, a_left_semi_join_keyed_on_decimal38_agrees;
    15 => an_inner_join_keyed_on_utf8_agrees, a_left_semi_join_keyed_on_utf8_agrees;
    16 => an_inner_join_keyed_on_uint8_agrees, a_left_semi_join_keyed_on_uint8_agrees;
    17 => an_inner_join_keyed_on_uint16_agrees, a_left_semi_join_keyed_on_uint16_agrees;
    18 => an_inner_join_keyed_on_uint32_agrees, a_left_semi_join_keyed_on_uint32_agrees;
    19 => an_inner_join_keyed_on_uint64_agrees, a_left_semi_join_keyed_on_uint64_agrees;
}

/// The list above is every key column, so a column added to `key_types` needs a pair here.
#[test]
fn every_key_types_column_but_the_id_is_keyed_on() {
    assert_eq!(key_types_schema().fields().len(), 1 + 19);
}

const DATE: u32 = 8;

/// `d` plus an interval literal, over the filter's own schema at `ordinal`.
fn date_plus(ordinal: u32, name: &str, months: i32, days: i32) -> Expr {
    Expr::binary(
        Expr::column(ordinal, name),
        BinaryOp::Plus,
        Expr::Literal(ScalarValue::IntervalMonthDayNano(Some(
            IntervalMonthDayNano::new(months, days, 0),
        ))),
        DataType::Date32,
    )
}

fn both_dates() -> Vec<JoinFilterColumn> {
    vec![
        JoinFilterColumn {
            side: JoinSide::Build,
            index: DATE,
        },
        JoinFilterColumn {
            side: JoinSide::Probe,
            index: DATE,
        },
    ]
}

/// The answer's rows, once both engines agree on them.
fn agreed_rows(node: &dyn GpuNode, sides: super::script::Script) -> usize {
    let outcome = run_both(node, sides);
    outcome.same(Order::Any);
    rows(outcome.gpu.as_ref().expect("the device answers"))
}

// pbench's interval-join-keyed, with an interval on both sides: `p_d + 30 days < b_d + 1 month`
// beside a key on `k32`. 35 rows; 3 with both intervals dropped, 59 with only the days, 59 with
// a month taken as 31 days — so the count says the device applied both, and the month as one.
operator_case! {
    GpuHashJoin,
    fn an_inner_join_with_an_interval_residual_agrees() {
        let residual = Expr::binary(
            date_plus(1, "p_d", 0, 30),
            BinaryOp::Lt,
            date_plus(0, "b_d", 1, 0),
            DataType::Boolean,
        );
        let node = GpuHashJoin::new(
            leaf("b_", BatchLayout::SingleBatch),
            leaf("p_", BatchLayout::MultipleBatches),
            JoinType::Inner,
            vec![(3, 3)],
            Some(residual),
            both_dates(),
            false,
            None,
            Schema::new(Arc::new(ArrowSchema::new(output_over(
                JoinType::Inner,
                fields("b_"),
                fields("p_"),
            )))),
        );
        let sides = script(Some(side(72, 3, "b_")), vec![side(96, 3, "p_")]);
        assert_eq!(agreed_rows(&node, sides), 35);
    }
}

// pbench's interval-join-keyless: `b_d + 7 days > p_d`, whose left operand the session hoists
// onto the build side, learning its type from a zero-row table. 410 rows; 386 with the interval
// dropped.
operator_case! {
    GpuNestedLoopJoin,
    fn a_nested_loop_join_on_an_interval_condition_agrees() {
        let condition = Expr::binary(
            date_plus(0, "b_d", 0, 7),
            BinaryOp::Gt,
            Expr::column(1, "p_d"),
            DataType::Boolean,
        );
        let node = GpuNestedLoopJoin::new(
            leaf("b_", BatchLayout::SingleBatch),
            leaf("p_", BatchLayout::MultipleBatches),
            JoinType::Inner,
            condition,
            both_dates(),
            None,
            Schema::new(Arc::new(ArrowSchema::new(output_over(
                JoinType::Inner,
                fields("b_"),
                fields("p_"),
            )))),
        );
        let sides = script(Some(side(24, 0, "b_")), vec![side(32, 0, "p_")]);
        assert_eq!(agreed_rows(&node, sides), 410);
    }
}
