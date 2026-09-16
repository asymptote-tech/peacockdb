//! `GpuAggregate` and `GpuAggregateBatches` along the dimensions the corpus varies and task
//! 9 held constant: the group key's type, `count(*)` and expression arguments, every merge
//! arm with rows, the global Welford init and the dispersion finalize. The builders are
//! `aggregate_cases.rs`'s; every case is green or a `bug_` test with its ticket above it,
//! and nothing here repairs.

use std::sync::Arc;

use datafusion::arrow::array::{ArrayRef, AsArray, Int64Array, UInt8Array};
use datafusion::arrow::compute::concat_batches;
use datafusion::arrow::datatypes::{DataType, Float64Type, UInt64Type};
use datafusion::arrow::record_batch::RecordBatch;
use datafusion::common::ScalarValue;

use super::aggregate_cases::{
    GroupKey, KEY, body, call, cpu_slot, gpu_slot, init, init_by, input, merge_by, merge_over,
    state_cut, welford_init_aggs, welford_merge_by, welford_partial, welford_state_by,
};
use super::script::{Outcome, Script, run_both};
use crate::plan::{
    AggCall, AggFunc, AggSpec, BatchLayout, BinaryOp, Expr, GpuAggregate, NamedExpr, PlanAgg,
    Schema, finalize,
};
use crate::tests::compare::{Order, close, same_within_welford};
use crate::tests::given::{Given, columns};
use crate::tests::synthetic::{schema, synthetic};

fn sum_i64() -> AggCall {
    call(
        PlanAgg::Sum,
        Expr::column(3, "i64"),
        "sum(i64)",
        DataType::Int64,
    )
}

const DATE: GroupKey = (6, "d", DataType::Date32);
const ID: GroupKey = (0, "id", DataType::Int64);
const BOOL: GroupKey = (7, "b", DataType::Boolean);
const STRING: GroupKey = (5, "s", DataType::Utf8);

/// `input()` twice as one batch: every `id` and every date twice, so an aggregate that never
/// folds two equal keys is a different row count, and every sum is the row's doubled.
fn input_twice() -> RecordBatch {
    concat_batches(&schema(), &[input(), input()]).expect("one schema")
}

// The group key's type at the init: the corpus groups on strings and dates, on the
// `Int64` keys of nearly every join, and on more than one column. Strings are `Utf8`;
// no case declares a view type.

operator_case! {
    GpuAggregate,
    fn a_sum_grouped_on_a_date_agrees() {
        let node = init_by(&[DATE], vec![sum_i64()]);
        run_both(&node, Script::Exec(vec![input_twice()])).same(Order::Any);
    }
}

operator_case! {
    GpuAggregate,
    fn a_sum_grouped_on_an_int64_agrees() {
        let node = init_by(&[ID], vec![sum_i64()]);
        run_both(&node, Script::Exec(vec![input_twice()])).same(Order::Any);
    }
}

operator_case! {
    GpuAggregate,
    fn a_sum_grouped_on_a_string_agrees() {
        let node = init_by(&[STRING], vec![sum_i64()]);
        run_both(&node, Script::Exec(vec![input()])).same(Order::Any);
    }
}

operator_case! {
    GpuAggregate,
    fn a_sum_grouped_on_an_int32_and_a_string_agrees() {
        let node = init_by(&[KEY, STRING], vec![sum_i64()]);
        run_both(&node, Script::Exec(vec![input()])).same(Order::Any);
    }
}

// The same keys carried through the merge: state cut from `synthetic` under each key's
// own column, two arrivals with duplicate keys to fold.

fn sum_states_by(keys: &[GroupKey]) -> Script {
    let cut = |seed| state_cut(32, seed, keys, 3, "sum(i64)", DataType::Int64);
    Script::Accumulate(vec![cut(1), cut(2)])
}

/// `sum_states_by` with the one arrival twice, so a key drawn from 20,000 dates folds.
fn sum_states_by_twice(keys: &[GroupKey]) -> Script {
    let cut = |seed| state_cut(32, seed, keys, 3, "sum(i64)", DataType::Int64);
    Script::Accumulate(vec![cut(1), cut(1)])
}

operator_case! {
    GpuAggregateBatches,
    fn a_sum_merge_grouped_on_a_date_agrees() {
        let node = merge_by(&[DATE], PlanAgg::Sum, "sum(i64)", DataType::Int64);
        run_both(&node, sum_states_by_twice(&[DATE])).same(Order::Any);
    }
}

operator_case! {
    GpuAggregateBatches,
    fn a_sum_merge_grouped_on_an_int64_agrees() {
        let node = merge_by(&[ID], PlanAgg::Sum, "sum(i64)", DataType::Int64);
        run_both(&node, sum_states_by(&[ID])).same(Order::Any);
    }
}

operator_case! {
    GpuAggregateBatches,
    fn a_sum_merge_grouped_on_a_string_agrees() {
        let node = merge_by(&[STRING], PlanAgg::Sum, "sum(i64)", DataType::Int64);
        run_both(&node, sum_states_by(&[STRING])).same(Order::Any);
    }
}

operator_case! {
    GpuAggregateBatches,
    fn a_sum_merge_grouped_on_an_int32_and_a_string_agrees() {
        let node = merge_by(&[KEY, STRING], PlanAgg::Sum, "sum(i64)", DataType::Int64);
        run_both(&node, sum_states_by(&[KEY, STRING])).same(Order::Any);
    }
}

// `count(*)` is `count(1)` in every plan — DataFusion rewrites the star to the literal, so
// the two are one shape here — and an expression argument is materialised by the
// aggregate itself, with no project beneath it.

fn count_star() -> AggCall {
    call(
        PlanAgg::Count,
        Expr::Literal(ScalarValue::Int64(Some(1))),
        "count(*)",
        DataType::Int64,
    )
}

operator_case! {
    GpuAggregate,
    fn a_grouped_count_star_agrees() {
        run_both(&init(true, vec![count_star()]), Script::Exec(vec![input()])).same(Order::Any);
    }
}

operator_case! {
    GpuAggregate,
    fn a_global_count_star_agrees() {
        run_both(&init(false, vec![count_star()]), Script::Exec(vec![input()])).same(Order::Any);
    }
}

// `count(key)` grouped on `key`: the null group's argument is null on every row, so its
// count is the one a group with nothing to count gets.
operator_case! {
    GpuAggregate,
    fn a_grouped_count_of_an_all_null_group_agrees() {
        let aggs = vec![call(PlanAgg::Count, Expr::column(1, "key"), "count(key)", DataType::Int64)];
        run_both(&init(true, aggs), Script::Exec(vec![input()])).same(Order::Any);
    }
}

operator_case! {
    GpuAggregate,
    fn a_grouped_sum_of_a_product_agrees() {
        let widened = Expr::Cast {
            expr: Box::new(Expr::column(2, "i32")),
            target: DataType::Int64,
        };
        let product = Expr::binary(
            Expr::column(3, "i64"),
            BinaryOp::Multiply,
            widened,
            DataType::Int64,
        );
        let aggs = vec![call(PlanAgg::Sum, product, "sum(i64 * i32)", DataType::Int64)];
        run_both(&init(true, aggs), Script::Exec(vec![input()])).same(Order::Any);
    }
}

// #56's shape — `sum(CASE WHEN s = 'beta' THEN i64 ELSE 0 END)`, the string equality
// inside the aggregate's argument, 48 times over in the corpus.
operator_case! {
    GpuAggregate,
    fn a_grouped_sum_of_a_case_over_a_string_equality_agrees() {
        let is_beta = Expr::binary(
            Expr::column(5, "s"),
            BinaryOp::Eq,
            Expr::Literal(ScalarValue::Utf8(Some("beta".into()))),
            DataType::Boolean,
        );
        let case = Expr::Case {
            comparand: None,
            when_then: vec![(is_beta, Expr::column(3, "i64"))],
            else_expr: Some(Box::new(Expr::Literal(ScalarValue::Int64(Some(0))))),
        };
        let aggs = vec![call(PlanAgg::Sum, case, "sum(case)", DataType::Int64)];
        run_both(&init(true, aggs), Script::Exec(vec![input()])).same(Order::Any);
    }
}

// Empty inputs, each its own case: `count(*)` over a zero-row batch.
operator_case! {
    GpuAggregate,
    fn a_grouped_count_star_over_zero_rows_is_zero_rows_on_both() {
        run_both(&init(true, vec![count_star()]), Script::Exec(vec![synthetic(0, 1)]))
            .same(Order::Any);
    }
}

operator_case! {
    GpuAggregate,
    fn a_global_count_star_over_zero_rows_keeps_its_identity_row() {
        run_both(&init(false, vec![count_star()]), Script::Exec(vec![synthetic(0, 1)]))
            .same(Order::Any);
    }
}

// Every merge arm with rows to fold. A keyless merge is `cudf::reduce`'s arm, which only
// #199's no-arrival pin drove; the grouped `Min` and `Max` arms had no case at all. A count
// merges by `Sum` in every plan (`sum(count(*))`), so the keyless sum case is its merge.

fn keyless_states(column: usize, name: &str, ty: DataType) -> Script {
    let cut = |seed| state_cut(8, seed, &[], column, name, ty.clone());
    Script::Accumulate(vec![cut(1), cut(2), cut(3)])
}

operator_case! {
    GpuAggregateBatches,
    fn a_keyless_sum_merge_over_arrivals_with_rows_agrees() {
        let node = merge_by(&[], PlanAgg::Sum, "sum(i64)", DataType::Int64);
        run_both(&node, keyless_states(3, "sum(i64)", DataType::Int64)).same(Order::Any);
    }
}

operator_case! {
    GpuAggregateBatches,
    fn a_keyless_min_merge_over_arrivals_with_rows_agrees() {
        let node = merge_by(&[], PlanAgg::Min, "min(i32)", DataType::Int32);
        run_both(&node, keyless_states(2, "min(i32)", DataType::Int32)).same(Order::Any);
    }
}

operator_case! {
    GpuAggregateBatches,
    fn a_keyless_max_merge_over_arrivals_with_rows_agrees() {
        let node = merge_by(&[], PlanAgg::Max, "max(i32)", DataType::Int32);
        run_both(&node, keyless_states(2, "max(i32)", DataType::Int32)).same(Order::Any);
    }
}

fn grouped_states(column: usize, name: &str, ty: DataType) -> Script {
    let cut = |seed| state_cut(32, seed, &[KEY], column, name, ty.clone());
    Script::Accumulate(vec![cut(1), cut(2)])
}

operator_case! {
    GpuAggregateBatches,
    fn a_grouped_min_merge_over_arrivals_with_rows_agrees() {
        let node = merge_by(&[KEY], PlanAgg::Min, "min(i32)", DataType::Int32);
        run_both(&node, grouped_states(2, "min(i32)", DataType::Int32)).same(Order::Any);
    }
}

operator_case! {
    GpuAggregateBatches,
    fn a_grouped_max_merge_over_arrivals_with_rows_agrees() {
        let node = merge_by(&[KEY], PlanAgg::Max, "max(i32)", DataType::Int32);
        run_both(&node, grouped_states(2, "max(i32)", DataType::Int32)).same(Order::Any);
    }
}

/// `welford_partial(func, seed)` with its key dropped: what a global init over one row emits.
fn keyless_welford_partial(func: AggFunc, seed: u64) -> RecordBatch {
    welford_partial(func, seed)
        .project(&[1, 2, 3])
        .expect("the triple")
}

// #216 — the same arm at the merge: the `stddev` it reduces is over the state's first
// column, the count — 1 per row and 0 where the row's value is null — so the device
// answers the sample stddev of the counts where the cpu answers the merged triple.
operator_case! {
    GpuAggregateBatches,
    fn bug_a_keyless_welford_merge_answers_the_stddev_of_its_counts_on_the_device() {
        let partial = |seed| keyless_welford_partial(AggFunc::Stddev, seed);
        let arrivals = vec![partial(1), partial(2)];
        let counts: Vec<f64> = arrivals
            .iter()
            .flat_map(|p| p.column(0).as_primitive::<UInt64Type>().values().iter())
            .map(|c| *c as f64)
            .collect();
        let mean = counts.iter().sum::<f64>() / counts.len() as f64;
        let m2: f64 = counts.iter().map(|c| (c - mean) * (c - mean)).sum();
        let stddev_of_counts = (m2 / (counts.len() - 1) as f64).sqrt();
        let node = welford_merge_by(false, AggFunc::Stddev, None);
        let outcome = run_both(&node, Script::Accumulate(arrivals));
        assert_eq!(cpu_slot(&outcome, 2).num_columns(), 3, "the cpu merges the triple");
        let gpu = gpu_slot(&outcome, 2);
        assert_eq!((gpu.num_columns(), gpu.num_rows()), (1, 1), "one finished value");
        assert_eq!(gpu.schema().field(0).name(), "stddev(f64)");
        let answered = gpu.column(0).as_primitive::<Float64Type>().value(0);
        assert!(
            close(answered, stddev_of_counts),
            "device {answered:e}, the counts' stddev {stddev_of_counts:e}"
        );
    }
}

// A merge-level finalize that computes: `avg`'s divide at done over `[key, sum, count]`,
// the count declared Int64 for the reason the single-node shortcut gives.
operator_case! {
    GpuAggregateBatches,
    fn a_merge_finalize_that_divides_agrees() {
        let state = columns(&[
            ("key", DataType::Int32),
            ("avg(i64)$sum", DataType::Int64),
            ("avg(i64)$count", DataType::Int64),
        ]);
        let output = columns(&[("key", DataType::Int32), ("avg(i64)", DataType::Float64)]);
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
        let aggs = vec![
            call(PlanAgg::Sum, Expr::column(1, "avg(i64)$sum"), "avg(i64)$sum", DataType::Int64),
            call(PlanAgg::Sum, Expr::column(2, "avg(i64)$count"), "avg(i64)$count", DataType::Int64),
        ];
        let node = merge_over(
            state.clone(),
            body(
                vec![Expr::column(0, "key")],
                aggs,
                Some(vec![NamedExpr::new(divide, "avg(i64)")]),
            ),
            output,
        );
        // One partial per row: the value itself, counted once.
        let partial = |seed| {
            let s = state_cut(32, seed, &[KEY], 3, "avg(i64)$sum", DataType::Int64);
            let ones: ArrayRef = Arc::new(Int64Array::from(vec![1i64; 32]));
            RecordBatch::try_new(
                state.fields.clone(),
                vec![s.column(0).clone(), s.column(1).clone(), ones],
            )
            .unwrap()
        };
        run_both(&node, Script::Accumulate(vec![partial(1), partial(2)])).same(Order::Any);
    }
}

// A merge over grouping-set state, `[keys, __grouping_id, state]`: every node above an
// init that expands sets groups on the id as one more key, in the `UInt8` DataFusion
// declares for it.
operator_case! {
    GpuAggregateBatches,
    fn a_merge_over_grouping_set_state_agrees() {
        let state = columns(&[
            ("key", DataType::Int32),
            ("b", DataType::Boolean),
            ("__grouping_id", DataType::UInt8),
            ("sum(i64)", DataType::Int64),
        ]);
        let aggs = vec![call(PlanAgg::Sum, Expr::column(3, "sum(i64)"), "sum(i64)", DataType::Int64)];
        let group_by = vec![
            Expr::column(0, "key"),
            Expr::column(1, "b"),
            Expr::column(2, "__grouping_id"),
        ];
        let node = merge_over(state.clone(), body(group_by, aggs, None), state.clone());
        // Three ids over the rows, so each key pair folds under more than one set.
        let arrival = |seed| {
            let s = state_cut(32, seed, &[KEY, BOOL], 3, "sum(i64)", DataType::Int64);
            let ids: ArrayRef = Arc::new(UInt8Array::from_iter_values((0..32).map(|r| (r % 3) as u8)));
            RecordBatch::try_new(
                state.fields.clone(),
                vec![s.column(0).clone(), s.column(1).clone(), ids, s.column(2).clone()],
            )
            .unwrap()
        };
        run_both(&node, Script::Accumulate(vec![arrival(1), arrival(2)])).same(Order::Any);
    }
}

// Empty inputs, each its own case: #199 is the no-arrival pin, this is its zero-row
// neighbour.
operator_case! {
    GpuAggregateBatches,
    fn a_keyless_sum_merge_over_zero_row_arrivals_agrees() {
        let node = merge_by(&[], PlanAgg::Sum, "sum(i64)", DataType::Int64);
        let cut = |seed| state_cut(0, seed, &[], 3, "sum(i64)", DataType::Int64);
        run_both(&node, Script::Accumulate(vec![cut(1), cut(2)])).same(Order::Any);
    }
}

// The Welford triple with no key, and the dispersion finalize as the planner writes it —
// `plan::finalize` over the merged state: a `CASE` over the count, a typed NULL, and for
// `stddev` a `Sqrt`. Grouped, both agree; keyless, the device has no triple to finalize
// under `stddev` and no arm at all for `var`.

/// The global Welford init: the triple over `f64` with no group.
fn welford_init_global() -> GpuAggregate {
    let state = welford_state_by(false, AggFunc::Stddev);
    GpuAggregate::new(
        Given::of(Schema::new(schema()), BatchLayout::MultipleBatches),
        body(Vec::new(), welford_init_aggs(), None),
        state.clone(),
        state,
    )
}

// #216 — the device's keyless path has no Welford arm: it reduces `stddev` to the one
// finished `Float64` the SQL aggregate would answer, where the plan declares the
// `[count, mean, m2]` state the cpu emits. The value is the sample stddev the cpu's state
// finalizes to, so the merge above it is what goes wrong.
operator_case! {
    GpuAggregate,
    fn bug_a_global_welford_init_answers_a_finished_stddev_on_the_device() {
        let outcome = run_both(&welford_init_global(), Script::Exec(vec![input()]));
        let cpu = cpu_slot(&outcome, 0);
        assert_eq!(cpu.num_columns(), 3, "the cpu emits the triple");
        let gpu = gpu_slot(&outcome, 0);
        assert_eq!(
            gpu.schema().fields().iter().map(|f| (f.name().as_str(), f.data_type().clone())).collect::<Vec<_>>(),
            vec![("stddev(f64)", DataType::Float64)]
        );
        let count = cpu.column(0).as_primitive::<UInt64Type>().value(0) as f64;
        let m2 = cpu.column(2).as_primitive::<Float64Type>().value(0);
        let finished = gpu.column(0).as_primitive::<Float64Type>().value(0);
        assert!(close(finished, (m2 / (count - 1.0)).sqrt()), "device {finished:e}");
    }
}

/// The planner's own finalize for `func` — `Stddev` or `Var` — over the merged triple
/// its state declares, which sits after `keys` key columns; named as that state's output.
fn dispersion_finalize(func: AggFunc, keys: u32) -> NamedExpr {
    let state = welford_state_by(keys == 1, func);
    let fields: Vec<_> = state.fields.fields()[keys as usize..]
        .iter()
        .map(|f| f.as_ref().clone())
        .collect();
    let owner = &state.agg_state[0];
    let spec = AggSpec {
        func: owner.func,
        ddof: owner.ddof,
    };
    NamedExpr::new(
        finalize(spec, &fields, keys, &DataType::Float64),
        &owner.output,
    )
}

/// Both answered the done slot, the key exactly and the finalized `Float64` to
/// `WELFORD_RELATIVE`: the two merged states differ in their last digits, and a root or a
/// quotient of them does too.
fn finalized_within_welford(func: AggFunc) {
    let node = welford_merge_by(true, func, Some(dispersion_finalize(func, 1)));
    let arrivals = vec![welford_partial(func, 1), welford_partial(func, 2)];
    let outcome = run_both(&node, Script::Accumulate(arrivals));
    same_within_welford(cpu_slot(&outcome, 2), gpu_slot(&outcome, 2), true, &[1]);
}

operator_case! {
    GpuAggregateBatches,
    fn a_grouped_stddev_finalize_agrees_within_welford() {
        finalized_within_welford(AggFunc::Stddev);
    }
}

operator_case! {
    GpuAggregateBatches,
    fn a_grouped_var_finalize_agrees_within_welford() {
        finalized_within_welford(AggFunc::Var);
    }
}

/// The keyless merge under `func`'s finalize, over two global partials.
fn global_finalize_outcome(func: AggFunc) -> Outcome {
    let node = welford_merge_by(false, func, Some(dispersion_finalize(func, 0)));
    let arrivals = vec![
        keyless_welford_partial(func, 1),
        keyless_welford_partial(func, 2),
    ];
    run_both(&node, Script::Accumulate(arrivals))
}

// #216 — over the one column the keyless merge answers, the finalize's `m2` reference is
// out of range; the cpu answers the finalized stddev.
operator_case! {
    GpuAggregateBatches,
    fn bug_a_global_stddev_finalize_is_refused_on_the_device() {
        let outcome = global_finalize_outcome(AggFunc::Stddev);
        assert!(
            outcome.gpu_refuses().contains("ColumnRef index 2 out of range (cols=1)"),
            "{}",
            outcome.gpu_refuses()
        );
    }
}

// #216 — the variance never reaches its finalize: the keyless path knows `stddev` alone
// (`is_stddev_name`), so a `var` name falls through to `make_reduce_agg` and the aggregate
// itself is refused; the cpu answers the finalized variance.
operator_case! {
    GpuAggregateBatches,
    fn bug_a_keyless_var_merge_is_refused_as_unsupported_on_the_device() {
        let outcome = global_finalize_outcome(AggFunc::Var);
        assert!(
            outcome
                .gpu_refuses()
                .contains("[in CudfAggregate] unsupported aggregate function: var"),
            "{}",
            outcome.gpu_refuses()
        );
    }
}
