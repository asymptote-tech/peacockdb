//! What a keyless init owes over no input, on both engines: over no batch and over zero-row
//! batches, the row `PlanAgg::empty_state` gives, or the shortcut's finalized identity row;
//! and a grouped init over nothing answers nothing. The init is a batch accumulator, so a
//! zero-row batch is a slot of its own with nothing in it, and the identity row is the done
//! slot's.

use datafusion::arrow::array::ArrayRef;
use datafusion::arrow::datatypes::{DataType, Field};
use datafusion::arrow::record_batch::RecordBatch;
use datafusion::common::ScalarValue;

use super::aggregate_cases::{
    body, call, decimal_sum, given, init, input, planned_decimal_average, rows_per_slot,
};
use super::script::{Script, each_answers, run_both};
use crate::plan::{
    AggFunc, AggSpec, AggStateColumns, BatchLayout, Expr, GpuAggregate, GpuNode, NamedExpr,
    PlanAgg, Schema, decomposition, finalize,
};
use crate::tests::compare::Order;
use crate::tests::given::{Given, columns};
use crate::tests::synthetic::{decimals, synthetic};

/// Every aggregate SQL can ask for: the device answers each keyless one over no rows,
/// the Welford pair through the grouped request builder on one constant key.
const EVERY_FUNC: [AggFunc; 7] = [
    AggFunc::Sum,
    AggFunc::Min,
    AggFunc::Max,
    AggFunc::Count,
    AggFunc::Avg,
    AggFunc::Stddev,
    AggFunc::Var,
];

fn sql_name(func: AggFunc) -> &'static str {
    match func {
        AggFunc::Sum => "sum",
        AggFunc::Min => "min",
        AggFunc::Max => "max",
        AggFunc::Count => "count",
        AggFunc::Avg => "avg",
        AggFunc::Stddev => "stddev",
        AggFunc::Var => "var",
    }
}

/// The keyless init `decompose` writes for `func(i64)`, its state typed by `state_type` —
/// finalized where `shortcut`, as the single-node shortcut is.
pub(crate) fn keyless_init(func: AggFunc, shortcut: bool) -> GpuAggregate {
    let output = format!("{}(i64)", sql_name(func));
    let mut fields = Vec::new();
    let mut aggs = Vec::new();
    for (suffix, agg) in decomposition(func).state {
        let state_type = agg.state_type(&DataType::Int64).expect("typed over Int64");
        let column = format!("{output}{suffix}");
        aggs.push(call(
            *agg,
            Expr::column(3, "i64"),
            &column,
            state_type.clone(),
        ));
        fields.push((column, state_type));
    }
    let named: Vec<(&str, DataType)> = fields
        .iter()
        .map(|(name, ty)| (name.as_str(), ty.clone()))
        .collect();
    let mut state = columns(&named);
    if matches!(func, AggFunc::Stddev | AggFunc::Var) {
        state.agg_state = vec![AggStateColumns {
            output: output.clone(),
            func,
            ddof: 1,
            positions: vec![0, 1, 2],
        }];
    }
    if !shortcut {
        return GpuAggregate::new(given(), body(Vec::new(), aggs, None), state.clone(), state);
    }
    let out_type = match func {
        AggFunc::Avg | AggFunc::Stddev | AggFunc::Var => DataType::Float64,
        _ => fields[0].1.clone(),
    };
    let state_fields: Vec<Field> = state
        .fields
        .fields()
        .iter()
        .map(|field| field.as_ref().clone())
        .collect();
    let expr = finalize(AggSpec { func, ddof: 1 }, &state_fields, 0, &out_type);
    GpuAggregate::new(
        given(),
        body(Vec::new(), aggs, Some(vec![NamedExpr::new(expr, &output)])),
        state,
        columns(&[(output.as_str(), out_type)]),
    )
}

/// `empty_state`'s row for `node`'s state, every argument typed `input`. `pub(crate)`, as
/// [`empty_state_row_over`].
pub(crate) fn empty_state_row(node: &GpuAggregate, input: &DataType) -> RecordBatch {
    empty_state_row_over(node, &vec![input.clone(); node.body.aggs.len()])
}

/// `empty_state`'s row for `node`'s state, each aggregator's argument typed as `inputs` says,
/// in aggregator order. `pub(crate)`: the keyless Welford cases read it.
pub(crate) fn empty_state_row_over(node: &GpuAggregate, inputs: &[DataType]) -> RecordBatch {
    assert_eq!(
        inputs.len(),
        node.body.aggs.len(),
        "one input type per aggregator"
    );
    let arrays: Vec<ArrayRef> = node
        .body
        .aggs
        .iter()
        .zip(inputs)
        .map(|(agg, input)| {
            agg.func
                .empty_state(input)
                .expect("every init state is typed")
                .to_array()
                .expect("one row")
        })
        .collect();
    RecordBatch::try_new(node.intermediate().fields.clone(), arrays).expect("the state's columns")
}

/// The slots a keyless init answers over `zero_row_batches` zero-row batches: nothing per
/// batch, and `row` at done. `pub(crate)`, as [`empty_state_row`].
pub(crate) fn at_done(zero_row_batches: usize, row: RecordBatch) -> Vec<Vec<RecordBatch>> {
    let mut slots = vec![Vec::new(); zero_row_batches];
    slots.push(vec![row]);
    slots
}

/// `zero_row_batches` zero-row batches of `synthetic`'s columns.
fn zero_row_batches(count: usize) -> Vec<RecordBatch> {
    (0..count).map(|seed| synthetic(0, seed as u64)).collect()
}

// A keyless init owes one row whatever arrived: over no batch and over zero-row batches,
// the row `empty_state` gives, on both engines.
operator_case! {
    GpuAggregate,
    fn a_keyless_init_over_no_input_answers_its_empty_state_on_both() {
        for func in EVERY_FUNC {
            let node = keyless_init(func, false);
            let row = empty_state_row(&node, &DataType::Int64);
            for count in [0, 2] {
                let outcome = run_both(&node, Script::Accumulate(zero_row_batches(count)));
                let slots = at_done(count, row.clone());
                each_answers(&outcome, &slots, &slots);
            }
        }
    }
}

// The shortcut finalizes the identity row: count 0, every other output NULL — `avg`'s
// NULL sum over a zero count included, which divides to NULL rather than failing — over no
// batch and over two zero-row batches.
operator_case! {
    GpuAggregate,
    fn a_keyless_shortcut_over_no_input_answers_count_0_and_nulls_on_both() {
        for func in EVERY_FUNC {
            let node = keyless_init(func, true);
            let output = node.kind().schema().expect("not a sink").fields.clone();
            let value = match func {
                AggFunc::Count => ScalarValue::Int64(Some(0)),
                _ => ScalarValue::try_from(output.field(0).data_type()).expect("a typed NULL"),
            };
            let row = RecordBatch::try_new(output, vec![value.to_array().expect("one row")])
                .expect("one column");
            for count in [0, 2] {
                let outcome = run_both(&node, Script::Accumulate(zero_row_batches(count)));
                let slots = at_done(count, row.clone());
                each_answers(&outcome, &slots, &slots);
            }
        }
    }
}

// The decimal sum's NULL carries the declared Decimal128(28, 2) on both: the device's done
// call builds its zero-row input from the declared Decimal128(18, 2), scale included.
operator_case! {
    GpuAggregate,
    fn a_decimal_sum_over_no_input_answers_a_null_of_its_declared_type_on_both() {
        let node = decimal_sum();
        let row = empty_state_row(&node, &DataType::Decimal128(18, 2));
        let outcome = run_both(&node, Script::Accumulate(Vec::new()));
        let slots = at_done(0, row);
        each_answers(&outcome, &slots, &slots);
    }
}

// The decimal avg's shortcut over nothing: a NULL sum over a zero count cast to Decimal128
// (22, 0), divided — NULL on both, never a divide-by-zero.
operator_case! {
    GpuAggregate,
    fn a_decimal_average_shortcut_over_no_input_answers_null_on_both() {
        let (state, output, finalize_list) = planned_decimal_average();
        let aggs = vec![
            call(PlanAgg::Sum, Expr::column(1, "dec"), "avg(dec)$sum", DataType::Decimal128(28, 2)),
            call(PlanAgg::Count, Expr::column(1, "dec"), "avg(dec)$count", DataType::Int64),
        ];
        let node = GpuAggregate::new(
            Given::of(Schema::new(decimals(0, 0).schema()), BatchLayout::MultipleBatches),
            body(Vec::new(), aggs, Some(finalize_list)),
            state,
            output.clone(),
        );
        let null = ScalarValue::try_from(output.fields.field(0).data_type()).expect("typed");
        let row = RecordBatch::try_new(output.fields.clone(), vec![null.to_array().expect("one")])
            .expect("one column");
        let outcome = run_both(&node, Script::Accumulate(vec![decimals(0, 1)]));
        let slots = at_done(1, row);
        each_answers(&outcome, &slots, &slots);
    }
}

// A grouped init over nothing owes no groups and emits nothing at all.
operator_case! {
    GpuAggregate,
    fn a_grouped_init_over_no_input_answers_nothing_on_both() {
        let node = init(
            true,
            vec![call(PlanAgg::Sum, Expr::column(3, "i64"), "sum(i64)", DataType::Int64)],
        );
        for count in [0, 2] {
            let outcome = run_both(&node, Script::Accumulate(zero_row_batches(count)));
            let slots = vec![Vec::new(); count + 1];
            each_answers(&outcome, &slots, &slots);
        }
    }
}

// Zero-row batches among batches with rows: no call and no row of their own, and no done
// call — the rows are what the batches with rows answer.
operator_case! {
    GpuAggregate,
    fn zero_row_batches_among_others_change_no_row_on_both() {
        let node = init(
            false,
            vec![
                call(PlanAgg::Count, Expr::column(2, "i32"), "count(i32)", DataType::Int64),
                call(PlanAgg::Sum, Expr::column(3, "i64"), "sum(i64)", DataType::Int64),
            ],
        );
        let arrivals = vec![synthetic(0, 1), input(), synthetic(0, 2), synthetic(64, 2)];
        let outcome = run_both(&node, Script::Accumulate(arrivals));
        outcome.same(Order::Any);
        assert_eq!(
            rows_per_slot(outcome.cpu.as_ref().expect("the cpu answers")),
            vec![0, 1, 0, 1, 0],
            "a row per batch with rows, nothing at done"
        );
    }
}
