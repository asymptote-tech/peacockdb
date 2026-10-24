//! The Welford triple on both engines where `aggregate_cases.rs` does not reach: groups with no
//! valid value, a NaN, lanes that disagree on what a group holds, and the keyless node, which
//! the device runs through the grouped request builder on one constant key. The builders are
//! `aggregate_cases.rs`'s; every case is an agreement case.

use std::sync::Arc;

use datafusion::arrow::array::{Array, AsArray, Float64Array, Int32Array};
use datafusion::arrow::datatypes::{DataType, Float64Type, Int32Type, Int64Type};
use datafusion::arrow::record_batch::RecordBatch;

use super::aggregate_cases::{
    body, call, cpu_slot, given, gpu_slot, input, merge_over, welford_init, welford_init_aggs,
    welford_merge, welford_merge_by,
};
use super::aggregate_dimension_cases::{dispersion_finalize, welford_init_global};
use super::aggregate_identity_cases::{at_done, empty_state_row, empty_state_row_over};
use super::script::{Outcome, Script, each_answers, run_both};
use crate::plan::{
    AggCall, AggFunc, AggSpec, AggStateColumns, BatchLayout, Expr, GpuAggregate,
    GpuAggregateBatches, NamedExpr, PlanAgg, Schema, finalize,
};
use crate::tests::compare::{Order, same_within_welford};
use crate::tests::given::{Given, columns};
use crate::tests::synthetic::{decimals, synthetic};

/// `synthetic`'s columns with `key` and `f64` laid out by the case: each group's values.
fn keyed_f64(keys: &[i32], values: &[Option<f64>]) -> RecordBatch {
    let s = synthetic(keys.len(), 1);
    let mut columns = s.columns().to_vec();
    columns[1] = Arc::new(Int32Array::from(keys.to_vec()));
    columns[4] = Arc::new(Float64Array::from(values.to_vec()));
    RecordBatch::try_new(s.schema(), columns).expect("synthetic's schema")
}

fn row_of(batch: &RecordBatch, key: i32) -> usize {
    let keys = batch.column(0).as_primitive::<Int32Type>();
    (0..batch.num_rows())
        .find(|&row| keys.is_valid(row) && keys.value(row) == key)
        .unwrap_or_else(|| panic!("no group {key}"))
}

fn float_at(batch: &RecordBatch, column: usize, row: usize) -> Option<f64> {
    let values = batch.column(column).as_primitive::<Float64Type>();
    values.is_valid(row).then(|| values.value(row))
}

/// `[key, count, mean, m2]`'s state for `key`.
fn state_at(batch: &RecordBatch, key: i32) -> (Option<i64>, Option<f64>, Option<f64>) {
    let row = row_of(batch, key);
    let counts = batch.column(1).as_primitive::<Int64Type>();
    (
        counts.is_valid(row).then(|| counts.value(row)),
        float_at(batch, 2, row),
        float_at(batch, 3, row),
    )
}

/// Two lanes' batches: key 1 holds 1.5 and 4.0 on the first and only NULLs on the second; key 2
/// holds only NULLs on both.
fn lanes() -> [RecordBatch; 2] {
    [
        keyed_f64(&[1, 1, 2, 2], &[Some(1.5), Some(4.0), None, None]),
        keyed_f64(&[1, 1, 2, 2], &[None, None, None, None]),
    ]
}

/// Each lane's init on each engine, then `merge` over that engine's own states, so the
/// device's merge meets the device's init states, never the cpu's. The done slot of each.
fn merged_by_each(merge: &GpuAggregateBatches) -> (RecordBatch, RecordBatch) {
    let inits: Vec<Outcome> = lanes()
        .into_iter()
        .map(|lane| run_both(&welford_init(), Script::Accumulate(vec![lane])))
        .collect();
    let cpu_states = inits.iter().map(|init| cpu_slot(init, 0).clone()).collect();
    let gpu_states = inits.iter().map(|init| gpu_slot(init, 0).clone()).collect();
    let on_cpu = run_both(merge, Script::Accumulate(cpu_states));
    let on_gpu = run_both(merge, Script::Accumulate(gpu_states));
    (cpu_slot(&on_cpu, 2).clone(), gpu_slot(&on_gpu, 2).clone())
}

// A group whose values are all NULL inits to DataFusion's (0, 0.0, 0.0): no NULL in the
// state, on the device as on the cpu.
operator_case! {
    GpuAggregate,
    fn an_all_null_group_inits_to_the_empty_triple_on_both() {
        let outcome = run_both(&welford_init(), Script::Accumulate(vec![lanes()[0].clone()]));
        same_within_welford(cpu_slot(&outcome, 0), gpu_slot(&outcome, 0), true, &[2, 3]);
        assert_eq!(state_at(gpu_slot(&outcome, 0), 2), (Some(0), Some(0.0), Some(0.0)));
    }
}

// Key 1 merges a real state with an empty one; key 2 merges two empty ones, which cuDF's
// MERGE_M2 answers with NULL moments unless the readback replaces them.
operator_case! {
    GpuAggregateBatches,
    fn a_group_with_values_on_one_lane_and_nulls_on_another_merges_as_the_cpu() {
        let (cpu, gpu) = merged_by_each(&welford_merge());
        same_within_welford(&cpu, &gpu, true, &[2, 3]);
        assert_eq!(state_at(&gpu, 1), (Some(2), Some(2.75), Some(3.125)));
        assert_eq!(state_at(&gpu, 2), (Some(0), Some(0.0), Some(0.0)));
    }
}

operator_case! {
    GpuAggregateBatches,
    fn a_group_all_null_on_every_lane_finalizes_to_null_on_both() {
        let finalized = welford_merge_by(
            true,
            AggFunc::Stddev,
            Some(dispersion_finalize(AggFunc::Stddev, 1)),
        );
        let (cpu, gpu) = merged_by_each(&finalized);
        same_within_welford(&cpu, &gpu, true, &[1]);
        assert_eq!(float_at(&gpu, 1, row_of(&gpu, 2)), None, "no lane holds a value for key 2");
        assert!(float_at(&gpu, 1, row_of(&gpu, 1)).is_some());
    }
}

// A NaN is a value: counted, and its moments NaN on both, which `same_within_welford`'s
// tolerance cannot compare, so the case reads the state itself.
operator_case! {
    GpuAggregate,
    fn a_nan_is_a_value_on_both() {
        let batch = keyed_f64(&[1, 1], &[Some(1.0), Some(f64::NAN)]);
        let outcome = run_both(&welford_init(), Script::Accumulate(vec![batch]));
        for (engine, state) in [("cpu", cpu_slot(&outcome, 0)), ("device", gpu_slot(&outcome, 0))] {
            let (count, mean, m2) = state_at(state, 1);
            assert_eq!(count, Some(2), "{engine}");
            assert!(
                mean.is_some_and(f64::is_nan) && m2.is_some_and(f64::is_nan),
                "{engine}: {mean:?} {m2:?}"
            );
        }
    }
}

/// `count(i32)`, `stddev(f64)`, `sum(i64)`, `min(d)` and `max(s)` with no key, as
/// `SELECT count(i32), stddev(f64), sum(i64), min(d), max(s)` plans: a Welford beside what a
/// keyless node reduces when it holds none, a `Date32` and a `Utf8` among them, which the
/// routed node takes to a groupby `MIN`/`MAX` and over nothing to a typed NULL.
fn keyless_mixed_state() -> Schema {
    let mut state = columns(&[
        ("count(i32)", DataType::Int64),
        ("stddev(f64)$count", DataType::Int64),
        ("stddev(f64)$mean", DataType::Float64),
        ("stddev(f64)$m2", DataType::Float64),
        ("sum(i64)", DataType::Int64),
        ("min(d)", DataType::Date32),
        ("max(s)", DataType::Utf8),
    ]);
    state.agg_state = vec![AggStateColumns {
        output: "stddev(f64)".to_string(),
        func: AggFunc::Stddev,
        ddof: 1,
        positions: vec![1, 2, 3],
    }];
    state
}

fn keyless_mixed_init() -> GpuAggregate {
    let mut aggs = vec![call(
        PlanAgg::Count,
        Expr::column(2, "i32"),
        "count(i32)",
        DataType::Int64,
    )];
    aggs.extend(welford_init_aggs());
    aggs.push(call(
        PlanAgg::Sum,
        Expr::column(3, "i64"),
        "sum(i64)",
        DataType::Int64,
    ));
    aggs.push(call(
        PlanAgg::Min,
        Expr::column(6, "d"),
        "min(d)",
        DataType::Date32,
    ));
    aggs.push(call(
        PlanAgg::Max,
        Expr::column(5, "s"),
        "max(s)",
        DataType::Utf8,
    ));
    let state = keyless_mixed_state();
    GpuAggregate::new(given(), body(Vec::new(), aggs, None), state.clone(), state)
}

fn keyless_mixed_merge() -> GpuAggregateBatches {
    let state = keyless_mixed_state();
    let field = |at: usize| state.fields.field(at).clone();
    let aggs = vec![
        // A count merges by sum.
        call(
            PlanAgg::Sum,
            Expr::column(0, "count(i32)"),
            "count(i32)",
            DataType::Int64,
        ),
        AggCall {
            func: PlanAgg::MergeM2,
            args: (1..4usize)
                .map(|at| Expr::column(at as u32, field(at).name()))
                .collect(),
            outputs: (1..4usize).map(field).collect(),
        },
        call(
            PlanAgg::Sum,
            Expr::column(4, "sum(i64)"),
            "sum(i64)",
            DataType::Int64,
        ),
        call(
            PlanAgg::Min,
            Expr::column(5, "min(d)"),
            "min(d)",
            DataType::Date32,
        ),
        call(
            PlanAgg::Max,
            Expr::column(6, "max(s)"),
            "max(s)",
            DataType::Utf8,
        ),
    ];
    merge_over(state.clone(), body(Vec::new(), aggs, None), state)
}

// With rows, and over nothing: count 0, the triple (0, 0.0, 0.0), and the sum, min and max
// NULL at their types, the Date32 and Utf8 NULLs default-constructed by keyless_empty_state.
operator_case! {
    GpuAggregate,
    fn a_keyless_stddev_beside_other_aggregates_agrees_over_rows_and_over_nothing() {
        let node = keyless_mixed_init();
        let outcome = run_both(&node, Script::Accumulate(vec![input()]));
        same_within_welford(cpu_slot(&outcome, 0), gpu_slot(&outcome, 0), false, &[2, 3]);
        let over_nothing = run_both(&node, Script::Accumulate(Vec::new()));
        let inputs = [
            DataType::Int32,
            DataType::Float64,
            DataType::Float64,
            DataType::Float64,
            DataType::Int64,
            DataType::Date32,
            DataType::Utf8,
        ];
        let slots = at_done(0, empty_state_row_over(&node, &inputs));
        each_answers(&over_nothing, &slots, &slots);
    }
}

operator_case! {
    GpuAggregateBatches,
    fn a_keyless_stddev_merge_beside_other_aggregates_agrees() {
        let partial = |seed| {
            let init = run_both(&keyless_mixed_init(), Script::Accumulate(vec![synthetic(64, seed)]));
            cpu_slot(&init, 0).clone()
        };
        let arrivals = vec![partial(1), partial(2), partial(3)];
        let outcome = run_both(&keyless_mixed_merge(), Script::Accumulate(arrivals));
        same_within_welford(cpu_slot(&outcome, 3), gpu_slot(&outcome, 3), false, &[2, 3]);
    }
}

/// `stddev(dec)` and `sum(dec)` with no key over `decimals`' `Decimal128(18, 2)`: the Welford
/// casts to `Float64`, the sum widens to `Decimal128(28, 2)`.
fn keyless_decimal_init() -> GpuAggregate {
    let triple = [
        ("$count", PlanAgg::Count, DataType::Int64),
        ("$mean", PlanAgg::Mean, DataType::Float64),
        ("$m2", PlanAgg::M2, DataType::Float64),
    ];
    let mut aggs: Vec<AggCall> = triple
        .iter()
        .map(|(suffix, agg, ty)| {
            call(
                *agg,
                Expr::column(1, "dec"),
                &format!("stddev(dec){suffix}"),
                ty.clone(),
            )
        })
        .collect();
    aggs.push(call(
        PlanAgg::Sum,
        Expr::column(1, "dec"),
        "sum(dec)",
        DataType::Decimal128(28, 2),
    ));
    let mut state = columns(&[
        ("stddev(dec)$count", DataType::Int64),
        ("stddev(dec)$mean", DataType::Float64),
        ("stddev(dec)$m2", DataType::Float64),
        ("sum(dec)", DataType::Decimal128(28, 2)),
    ]);
    state.agg_state = vec![AggStateColumns {
        output: "stddev(dec)".to_string(),
        func: AggFunc::Stddev,
        ddof: 1,
        positions: vec![0, 1, 2],
    }];
    GpuAggregate::new(
        Given::of(
            Schema::new(decimals(0, 0).schema()),
            BatchLayout::MultipleBatches,
        ),
        body(Vec::new(), aggs, None),
        state.clone(),
        state,
    )
}

// The routed node's sum is a groupby SUM, not cudf::reduce's; over nothing its NULL keeps the
// declared scale, which the done call's zero-row table carries from the field.
operator_case! {
    GpuAggregate,
    fn a_keyless_stddev_beside_a_decimal_sum_agrees_over_rows_and_over_nothing() {
        let node = keyless_decimal_init();
        let outcome = run_both(&node, Script::Accumulate(vec![decimals(64, 1)]));
        same_within_welford(cpu_slot(&outcome, 0), gpu_slot(&outcome, 0), false, &[1, 2]);
        let over_nothing = run_both(&node, Script::Accumulate(Vec::new()));
        let slots = at_done(0, empty_state_row(&node, &DataType::Decimal128(18, 2)));
        each_answers(&over_nothing, &slots, &slots);
    }
}

/// A keyless `func(f64)` at `ddof` that finalizes itself, as the single-node shortcut writes
/// it, named as the planner names it.
fn keyless_dispersion_shortcut(func: AggFunc, ddof: u32) -> GpuAggregate {
    let output = match (func, ddof) {
        (AggFunc::Stddev, 1) => "stddev(f64)",
        (AggFunc::Stddev, 0) => "stddev_pop(f64)",
        (AggFunc::Var, 1) => "var(f64)",
        (AggFunc::Var, 0) => "var_pop(f64)",
        other => panic!("{other:?} is not a dispersion form"),
    };
    let triple = [
        ("$count", PlanAgg::Count, DataType::Int64),
        ("$mean", PlanAgg::Mean, DataType::Float64),
        ("$m2", PlanAgg::M2, DataType::Float64),
    ];
    let aggs: Vec<AggCall> = triple
        .iter()
        .map(|(suffix, agg, ty)| {
            call(
                *agg,
                Expr::column(4, "f64"),
                &format!("{output}{suffix}"),
                ty.clone(),
            )
        })
        .collect();
    let fields: Vec<_> = aggs.iter().map(|agg| agg.outputs[0].clone()).collect();
    let named: Vec<(&str, DataType)> = fields
        .iter()
        .map(|field| (field.name().as_str(), field.data_type().clone()))
        .collect();
    let mut state = columns(&named);
    state.agg_state = vec![AggStateColumns {
        output: output.to_string(),
        func,
        ddof,
        positions: vec![0, 1, 2],
    }];
    let expr = finalize(AggSpec { func, ddof }, &fields, 0, &DataType::Float64);
    GpuAggregate::new(
        given(),
        body(Vec::new(), aggs, Some(vec![NamedExpr::new(expr, output)])),
        state,
        columns(&[(output, DataType::Float64)]),
    )
}

// One row: count 1, m2 0. The finalize's `count - ddof <= 0` makes the sample forms NULL;
// the population forms divide 0 by 1.
operator_case! {
    GpuAggregate,
    fn one_row_is_null_for_the_sample_forms_and_0_for_the_population_forms_on_both() {
        let forms = [
            (AggFunc::Stddev, 1, None),
            (AggFunc::Stddev, 0, Some(0.0)),
            (AggFunc::Var, 1, None),
            (AggFunc::Var, 0, Some(0.0)),
        ];
        for (func, ddof, want) in forms {
            let outcome = run_both(
                &keyless_dispersion_shortcut(func, ddof),
                Script::Accumulate(vec![synthetic(1, 1)]),
            );
            outcome.same(Order::Any);
            assert_eq!(float_at(cpu_slot(&outcome, 0), 0, 0), want, "{func:?} at ddof {ddof}");
        }
    }
}

// Only NULLs, no key: (0, 0.0, 0.0) on both, exactly.
operator_case! {
    GpuAggregate,
    fn a_keyless_welford_over_only_nulls_inits_to_the_empty_triple_on_both() {
        let batch = keyed_f64(&[1, 1, 2], &[None, None, None]);
        let outcome = run_both(&welford_init_global(), Script::Accumulate(vec![batch]));
        outcome.same(Order::Any);
        let state = gpu_slot(&outcome, 0);
        assert_eq!(state.column(0).as_primitive::<Int64Type>().value(0), 0);
        assert_eq!((float_at(state, 1, 0), float_at(state, 2, 0)), (Some(0.0), Some(0.0)));
    }
}
