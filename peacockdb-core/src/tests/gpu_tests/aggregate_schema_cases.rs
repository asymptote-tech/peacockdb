//! What a `GpuAggregate` and a `GpuAggregateBatches` hand the device up, read at the handle:
//! the init's state, the merge's, and the finalize's columns, for each aggregate the
//! decomposition table knows, grouped on each key type and global, every output handle held
//! to the node's declared schema — `PlanAgg::state_type`'s, as the planner writes it. The
//! builders are `aggregate_cases.rs`'s and `aggregate_dimension_cases.rs`'s. Every case is
//! green or a `bug_` test with its ticket above it; nothing here repairs.

use std::sync::Arc;

use datafusion::arrow::array::{ArrayRef, Int64Array};
use datafusion::arrow::compute::cast;
use datafusion::arrow::datatypes::DataType;
use datafusion::arrow::record_batch::RecordBatch;

use super::aggregate_cases::{
    GroupKey, KEY, body, call, decimal_sum, grouping_sets, init, init_by, input, merge_by,
    merge_over, merge_sum, planned_decimal_average, state_cut, sum_state, welford_init,
    welford_merge, welford_merge_by, welford_partial,
};
use super::aggregate_dimension_cases::{
    DATE, STRING, dispersion_finalize, keyless_welford_partial, rollup_welford_init,
    welford_init_global,
};
use super::script::{Script, assert_holds_as_declared};
use crate::plan::{AggFunc, BatchLayout, BinaryOp, Expr, GpuAggregate, NamedExpr, PlanAgg, Schema};
use crate::tests::given::{Given, columns};
use crate::tests::synthetic::{decimals, schema};

fn sum_i64() -> crate::plan::AggCall {
    call(
        PlanAgg::Sum,
        Expr::column(3, "i64"),
        "sum(i64)",
        DataType::Int64,
    )
}

fn count_i32() -> crate::plan::AggCall {
    call(
        PlanAgg::Count,
        Expr::column(2, "i32"),
        "count(i32)",
        DataType::Int64,
    )
}

// `GpuAggregate`: the init's state, one aggregate at a time, grouped on `key` and global.

operator_case! {
    GpuAggregate,
    fn a_sum_over_int64_declares_the_int64_state_the_device_holds() {
        assert_holds_as_declared(&init(true, vec![sum_i64()]), Script::Accumulate(vec![input()]));
    }
}

operator_case! {
    GpuAggregate,
    fn a_global_sum_over_int64_declares_the_int64_state_the_device_holds() {
        assert_holds_as_declared(&init(false, vec![sum_i64()]), Script::Accumulate(vec![input()]));
    }
}

// `sum` widens a decimal by ten digits of precision and keeps its scale, which is all the
// device holds of the declaration.
operator_case! {
    GpuAggregate,
    fn a_global_sum_over_a_decimal_declares_the_scale_2_state_the_device_holds() {
        assert_holds_as_declared(&decimal_sum(), Script::Accumulate(vec![decimals(64, 1)]));
    }
}

operator_case! {
    GpuAggregate,
    fn a_sum_over_a_decimal_declares_the_scale_2_state_the_device_holds() {
        let aggs = vec![call(
            PlanAgg::Sum,
            Expr::column(1, "dec"),
            "sum(dec)",
            DataType::Decimal128(28, 2),
        )];
        let state = columns(&[("id", DataType::Int64), ("sum(dec)", DataType::Decimal128(28, 2))]);
        let node = GpuAggregate::new(
            Given::of(Schema::new(decimals(0, 0).schema()), BatchLayout::MultipleBatches),
            body(vec![Expr::column(0, "id")], aggs, None),
            state.clone(),
            state,
        );
        assert_holds_as_declared(&node, Script::Accumulate(vec![decimals(64, 1)]));
    }
}

operator_case! {
    GpuAggregate,
    fn a_count_declares_the_int64_state_the_device_holds() {
        assert_holds_as_declared(&init(true, vec![count_i32()]), Script::Accumulate(vec![input()]));
    }
}

operator_case! {
    GpuAggregate,
    fn a_global_count_declares_the_int64_state_the_device_holds() {
        assert_holds_as_declared(&init(false, vec![count_i32()]), Script::Accumulate(vec![input()]));
    }
}

operator_case! {
    GpuAggregate,
    fn a_min_over_int32_declares_the_int32_state_the_device_holds() {
        let aggs = vec![call(PlanAgg::Min, Expr::column(2, "i32"), "min(i32)", DataType::Int32)];
        assert_holds_as_declared(&init(true, aggs), Script::Accumulate(vec![input()]));
    }
}

operator_case! {
    GpuAggregate,
    fn a_max_over_utf8_declares_the_string_state_the_device_holds() {
        let aggs = vec![call(PlanAgg::Max, Expr::column(5, "s"), "max(s)", DataType::Utf8)];
        assert_holds_as_declared(&init(true, aggs), Script::Accumulate(vec![input()]));
    }
}

operator_case! {
    GpuAggregate,
    fn a_global_min_over_date32_declares_the_timestamp_days_state_the_device_holds() {
        let aggs = vec![call(PlanAgg::Min, Expr::column(6, "d"), "min(d)", DataType::Date32)];
        assert_holds_as_declared(&init(false, aggs), Script::Accumulate(vec![input()]));
    }
}

operator_case! {
    GpuAggregate,
    fn a_global_max_over_float64_declares_the_float64_state_the_device_holds() {
        let aggs = vec![call(PlanAgg::Max, Expr::column(4, "f64"), "max(f64)", DataType::Float64)];
        assert_holds_as_declared(&init(false, aggs), Script::Accumulate(vec![input()]));
    }
}

/// `avg`'s state over `i64`, `[$sum, $count]` as the decomposition orders it.
fn avg_i64_state() -> Vec<crate::plan::AggCall> {
    vec![
        call(
            PlanAgg::Sum,
            Expr::column(3, "i64"),
            "avg(i64)$sum",
            DataType::Int64,
        ),
        call(
            PlanAgg::Count,
            Expr::column(3, "i64"),
            "avg(i64)$count",
            DataType::Int64,
        ),
    ]
}

operator_case! {
    GpuAggregate,
    fn an_avg_over_int64_declares_the_sum_and_count_state_the_device_holds() {
        assert_holds_as_declared(&init(true, avg_i64_state()), Script::Accumulate(vec![input()]));
    }
}

operator_case! {
    GpuAggregate,
    fn a_global_avg_over_a_decimal_declares_the_scale_2_sum_and_int64_count_the_device_holds() {
        let aggs = vec![
            call(PlanAgg::Sum, Expr::column(1, "dec"), "avg(dec)$sum", DataType::Decimal128(28, 2)),
            call(PlanAgg::Count, Expr::column(1, "dec"), "avg(dec)$count", DataType::Int64),
        ];
        let state = columns(&[
            ("avg(dec)$sum", DataType::Decimal128(28, 2)),
            ("avg(dec)$count", DataType::Int64),
        ]);
        let node = GpuAggregate::new(
            Given::of(Schema::new(decimals(0, 0).schema()), BatchLayout::MultipleBatches),
            body(Vec::new(), aggs, None),
            state.clone(),
            state,
        );
        assert_holds_as_declared(&node, Script::Accumulate(vec![decimals(64, 1)]));
    }
}

// The folded triple's three columns, each under the name the plan declares for it.
operator_case! {
    GpuAggregate,
    fn stddev_holds_its_three_declared_state_names() {
        assert_holds_as_declared(&welford_init(), Script::Accumulate(vec![input()]));
    }
}

// The global Welford holds the triple the plan declares, at the init and at the merge.
operator_case! {
    GpuAggregate,
    fn a_global_stddev_holds_its_declared_welford_state() {
        assert_holds_as_declared(&welford_init_global(), Script::Accumulate(vec![input()]));
    }
}

operator_case! {
    GpuAggregateBatches,
    fn a_global_stddev_merge_holds_its_declared_welford_state() {
        let partial = |seed| keyless_welford_partial(AggFunc::Stddev, seed);
        let node = welford_merge_by(false, AggFunc::Stddev, None);
        assert_holds_as_declared(&node, Script::Accumulate(vec![partial(1), partial(2)]));
    }
}

// The group key's type: a string, a date, and two keys.

operator_case! {
    GpuAggregate,
    fn a_sum_grouped_on_utf8_declares_the_string_key_the_device_holds() {
        assert_holds_as_declared(&init_by(&[STRING], vec![sum_i64()]), Script::Accumulate(vec![input()]));
    }
}

operator_case! {
    GpuAggregate,
    fn a_sum_grouped_on_date32_declares_the_timestamp_days_key_the_device_holds() {
        assert_holds_as_declared(&init_by(&[DATE], vec![sum_i64()]), Script::Accumulate(vec![input()]));
    }
}

operator_case! {
    GpuAggregate,
    fn a_sum_grouped_on_two_keys_declares_both_keys_the_device_holds() {
        let node = init_by(&[KEY, STRING], vec![sum_i64()]);
        assert_holds_as_declared(&node, Script::Accumulate(vec![input()]));
    }
}

// The single-node shortcut: init and finalize on one node, `avg` as `sum / count`.
operator_case! {
    GpuAggregate,
    fn a_finalizing_init_declares_the_float64_average_the_device_holds() {
        let intermediate = columns(&[
            ("key", DataType::Int32),
            ("avg(i64)$sum", DataType::Int64),
            ("avg(i64)$count", DataType::Int64),
        ]);
        let output = columns(&[("key", DataType::Int32), ("avg", DataType::Float64)]);
        let as_f64 = |ordinal: u32, name: &str| Expr::Cast {
            expr: Box::new(Expr::column(ordinal, name)),
            target: DataType::Float64,
        };
        let divide = Expr::binary(
            as_f64(1, "avg(i64)$sum"),
            BinaryOp::Divide,
            as_f64(2, "avg(i64)$count"),
            DataType::Float64,
        );
        let node = GpuAggregate::new(
            Given::of(Schema::new(schema()), BatchLayout::MultipleBatches),
            body(
                vec![Expr::column(1, "key")],
                avg_i64_state(),
                Some(vec![NamedExpr::new(divide, "avg")]),
            ),
            intermediate,
            output,
        );
        assert_holds_as_declared(&node, Script::Accumulate(vec![input()]));
    }
}

operator_case! {
    GpuAggregate,
    fn grouping_sets_hold_the_uint8_grouping_id_the_plan_declares() {
        let node = grouping_sets(vec![vec![false, false], vec![false, true], vec![true, true]]);
        assert_holds_as_declared(&node, Script::Accumulate(vec![input()]));
    }
}

// #280: the grouping-set path builds each Welford triple, and the column after them is the
// `sum`'s own.
operator_case! {
    GpuAggregate,
    fn a_rollup_with_a_stddev_a_var_and_a_sum_declares_the_state_the_device_holds() {
        assert_holds_as_declared(&rollup_welford_init(), Script::Accumulate(vec![input()]));
    }
}

// `GpuAggregateBatches`: merged state at done, then the finalize's columns.

fn two_sum_states() -> Script {
    Script::Accumulate(vec![sum_state(32, 1), sum_state(32, 2)])
}

/// Two arrivals of `[keys…, name]` cut from the fixture's `column`.
fn two_states(keys: &[GroupKey], column: usize, name: &str, ty: DataType) -> Script {
    Script::Accumulate(vec![
        state_cut(32, 1, keys, column, name, ty.clone()),
        state_cut(32, 2, keys, column, name, ty),
    ])
}

operator_case! {
    GpuAggregateBatches,
    fn a_sum_merge_declares_the_int64_state_the_device_holds() {
        assert_holds_as_declared(&merge_sum(false), two_sum_states());
    }
}

operator_case! {
    GpuAggregateBatches,
    fn a_min_merge_over_int32_declares_the_int32_state_the_device_holds() {
        let node = merge_by(&[KEY], PlanAgg::Min, "min(i32)", DataType::Int32);
        assert_holds_as_declared(&node, two_states(&[KEY], 2, "min(i32)", DataType::Int32));
    }
}

operator_case! {
    GpuAggregateBatches,
    fn a_max_merge_over_utf8_declares_the_string_state_the_device_holds() {
        let node = merge_by(&[KEY], PlanAgg::Max, "max(s)", DataType::Utf8);
        assert_holds_as_declared(&node, two_states(&[KEY], 5, "max(s)", DataType::Utf8));
    }
}

// A count merges by sum, over a column named as the count.
operator_case! {
    GpuAggregateBatches,
    fn a_count_merge_declares_the_int64_state_the_device_holds() {
        let node = merge_by(&[KEY], PlanAgg::Sum, "count(i32)", DataType::Int64);
        assert_holds_as_declared(&node, two_states(&[KEY], 3, "count(i32)", DataType::Int64));
    }
}

operator_case! {
    GpuAggregateBatches,
    fn a_sum_merge_grouped_on_utf8_declares_the_string_key_the_device_holds() {
        let node = merge_by(&[STRING], PlanAgg::Sum, "sum(i64)", DataType::Int64);
        assert_holds_as_declared(&node, two_states(&[STRING], 3, "sum(i64)", DataType::Int64));
    }
}

operator_case! {
    GpuAggregateBatches,
    fn a_sum_merge_grouped_on_date32_declares_the_timestamp_days_key_the_device_holds() {
        let node = merge_by(&[DATE], PlanAgg::Sum, "sum(i64)", DataType::Int64);
        assert_holds_as_declared(&node, two_states(&[DATE], 3, "sum(i64)", DataType::Int64));
    }
}

operator_case! {
    GpuAggregateBatches,
    fn a_sum_merge_grouped_on_two_keys_declares_both_keys_the_device_holds() {
        let node = merge_by(&[KEY, STRING], PlanAgg::Sum, "sum(i64)", DataType::Int64);
        assert_holds_as_declared(&node, two_states(&[KEY, STRING], 3, "sum(i64)", DataType::Int64));
    }
}

/// One `avg(dec)` partial per row: the decimal at the state's width, counted once.
fn decimal_avg_partial(seed: u64) -> RecordBatch {
    let (state, _, _) = planned_decimal_average();
    let d = decimals(32, seed);
    let sums = cast(d.column(1), &DataType::Decimal128(28, 2)).expect("a wider decimal");
    let ones: ArrayRef = Arc::new(Int64Array::from(vec![1i64; 32]));
    RecordBatch::try_new(state.fields.clone(), vec![sums, ones]).expect("the state's two columns")
}

fn decimal_avg_aggs() -> Vec<crate::plan::AggCall> {
    vec![
        call(
            PlanAgg::Sum,
            Expr::column(0, "avg(dec)$sum"),
            "avg(dec)$sum",
            DataType::Decimal128(28, 2),
        ),
        call(
            PlanAgg::Sum,
            Expr::column(1, "avg(dec)$count"),
            "avg(dec)$count",
            DataType::Int64,
        ),
    ]
}

operator_case! {
    GpuAggregateBatches,
    fn a_global_avg_merge_over_a_decimal_declares_the_scale_2_sum_and_int64_count_the_device_holds() {
        let (state, _, _) = planned_decimal_average();
        let node = merge_over(state.clone(), body(Vec::new(), decimal_avg_aggs(), None), state);
        let arrivals = vec![decimal_avg_partial(1), decimal_avg_partial(2)];
        assert_holds_as_declared(&node, Script::Accumulate(arrivals));
    }
}

// The merge's `MERGE_M2` children, named as the init's are.
operator_case! {
    GpuAggregateBatches,
    fn stddev_merge_holds_its_three_declared_state_names() {
        let partial = |seed| welford_partial(AggFunc::Stddev, seed);
        let arrivals = vec![partial(1), partial(2), partial(3)];
        assert_holds_as_declared(&welford_merge(), Script::Accumulate(arrivals));
    }
}

// The finalize: what the project at done hands up.

operator_case! {
    GpuAggregateBatches,
    fn a_sum_finalize_declares_the_int64_total_the_device_holds() {
        assert_holds_as_declared(&merge_sum(true), two_sum_states());
    }
}

// The declared output is `Decimal128(22, 6)`: the finalize divides at the sum's scale and
// casts to the declaration, and the device's pre-scaled divide lands at the same scale.
operator_case! {
    GpuAggregateBatches,
    fn an_avg_finalize_over_a_decimal_declares_the_scale_6_average_the_device_holds() {
        let (state, output, finalize) = planned_decimal_average();
        let node = merge_over(state, body(Vec::new(), decimal_avg_aggs(), Some(finalize)), output);
        let arrivals = vec![decimal_avg_partial(1), decimal_avg_partial(2)];
        assert_holds_as_declared(&node, Script::Accumulate(arrivals));
    }
}

operator_case! {
    GpuAggregateBatches,
    fn a_stddev_finalize_declares_the_float64_the_device_holds() {
        let node = welford_merge_by(
            true,
            AggFunc::Stddev,
            Some(dispersion_finalize(AggFunc::Stddev, 1)),
        );
        let arrivals = vec![welford_partial(AggFunc::Stddev, 1), welford_partial(AggFunc::Stddev, 2)];
        assert_holds_as_declared(&node, Script::Accumulate(arrivals));
    }
}

operator_case! {
    GpuAggregateBatches,
    fn a_var_finalize_declares_the_float64_the_device_holds() {
        let node = welford_merge_by(true, AggFunc::Var, Some(dispersion_finalize(AggFunc::Var, 1)));
        let arrivals = vec![welford_partial(AggFunc::Var, 1), welford_partial(AggFunc::Var, 2)];
        assert_holds_as_declared(&node, Script::Accumulate(arrivals));
    }
}
