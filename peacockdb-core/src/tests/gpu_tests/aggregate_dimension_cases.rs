//! `GpuAggregate` and `GpuAggregateBatches` along the dimensions the corpus varies and task
//! 9 held constant: the group key's type, `count(*)` and expression arguments, every merge
//! arm with rows, the global Welford init and the dispersion finalize. The builders are
//! `aggregate_cases.rs`'s; every case is green or a `bug_` test with its ticket above it,
//! and nothing here repairs.

use std::sync::Arc;

use datafusion::arrow::array::{ArrayRef, Int64Array, UInt8Array};
use datafusion::arrow::compute::{
    SortColumn, concat_batches, lexsort_to_indices, take_record_batch,
};
use datafusion::arrow::datatypes::DataType;
use datafusion::arrow::record_batch::RecordBatch;
use datafusion::common::ScalarValue;

use super::aggregate_cases::{
    GroupKey, KEY, body, call, cpu_slot, gpu_slot, init, init_by, input, merge_by, merge_over,
    state_cut, welford_init_aggs, welford_merge_by, welford_partial, welford_state_by,
};
use super::script::{Outcome, Script, each_answers, run_both};
use crate::plan::{
    AggCall, AggFunc, AggSpec, AggStateColumns, AggregateBody, BatchLayout, BinaryOp, Expr,
    GpuAggregate, GpuAggregateBatches, NamedExpr, PlanAgg, Schema, finalize,
};
use crate::tests::compare::{Order, same_within_welford};
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

pub(crate) const DATE: GroupKey = (6, "d", DataType::Date32);
const ID: GroupKey = (0, "id", DataType::Int64);
const BOOL: GroupKey = (7, "b", DataType::Boolean);
pub(crate) const STRING: GroupKey = (5, "s", DataType::Utf8);

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
        run_both(&node, Script::Accumulate(vec![input_twice()])).same(Order::Any);
    }
}

operator_case! {
    GpuAggregate,
    fn a_sum_grouped_on_an_int64_agrees() {
        let node = init_by(&[ID], vec![sum_i64()]);
        run_both(&node, Script::Accumulate(vec![input_twice()])).same(Order::Any);
    }
}

operator_case! {
    GpuAggregate,
    fn a_sum_grouped_on_a_string_agrees() {
        let node = init_by(&[STRING], vec![sum_i64()]);
        run_both(&node, Script::Accumulate(vec![input()])).same(Order::Any);
    }
}

operator_case! {
    GpuAggregate,
    fn a_sum_grouped_on_an_int32_and_a_string_agrees() {
        let node = init_by(&[KEY, STRING], vec![sum_i64()]);
        run_both(&node, Script::Accumulate(vec![input()])).same(Order::Any);
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
        run_both(&init(true, vec![count_star()]), Script::Accumulate(vec![input()])).same(Order::Any);
    }
}

operator_case! {
    GpuAggregate,
    fn a_global_count_star_agrees() {
        run_both(&init(false, vec![count_star()]), Script::Accumulate(vec![input()])).same(Order::Any);
    }
}

// `count(key)` grouped on `key`: the null group's argument is null on every row, so its
// count is the one a group with nothing to count gets.
operator_case! {
    GpuAggregate,
    fn a_grouped_count_of_an_all_null_group_agrees() {
        let aggs = vec![call(PlanAgg::Count, Expr::column(1, "key"), "count(key)", DataType::Int64)];
        run_both(&init(true, aggs), Script::Accumulate(vec![input()])).same(Order::Any);
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
        run_both(&init(true, aggs), Script::Accumulate(vec![input()])).same(Order::Any);
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
        run_both(&init(true, aggs), Script::Accumulate(vec![input()])).same(Order::Any);
    }
}

// Empty inputs, each its own case: `count(*)` over a zero-row batch, which the init
// releases uncalled. Grouped, it owes no groups and answers nothing.
operator_case! {
    GpuAggregate,
    fn a_grouped_count_star_over_a_zero_row_batch_answers_nothing_on_both() {
        let outcome =
            run_both(&init(true, vec![count_star()]), Script::Accumulate(vec![synthetic(0, 1)]));
        let nothing = vec![Vec::new(); 2];
        each_answers(&outcome, &nothing, &nothing);
    }
}

operator_case! {
    GpuAggregate,
    fn a_global_count_star_over_zero_rows_keeps_its_identity_row() {
        run_both(&init(false, vec![count_star()]), Script::Accumulate(vec![synthetic(0, 1)]))
            .same(Order::Any);
    }
}

// Every merge arm with rows to fold. A keyless merge is `cudf::reduce`'s arm; the grouped
// `Min` and `Max` arms had no case at all. A count
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

operator_case! {
    GpuAggregateBatches,
    fn a_keyless_welford_merge_agrees() {
        let partial = |seed| keyless_welford_partial(AggFunc::Stddev, seed);
        let node = welford_merge_by(false, AggFunc::Stddev, None);
        let outcome = run_both(&node, Script::Accumulate(vec![partial(1), partial(2)]));
        same_within_welford(cpu_slot(&outcome, 2), gpu_slot(&outcome, 2), false, &[1, 2]);
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

// Empty inputs, each its own case: `a_global_merge_over_no_arrival_answers_nothing_on_both`
// is the no-arrival case, this its zero-row neighbour.
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
// `stddev` a `Sqrt`. Keyless, the device runs the triple through the grouped request builder
// on one constant key, and both agree.

/// The global Welford init: the triple over `f64` with no group.
pub(crate) fn welford_init_global() -> GpuAggregate {
    let state = welford_state_by(false, AggFunc::Stddev);
    GpuAggregate::new(
        Given::of(Schema::new(schema()), BatchLayout::MultipleBatches),
        body(Vec::new(), welford_init_aggs(), None),
        state.clone(),
        state,
    )
}

operator_case! {
    GpuAggregate,
    fn a_global_welford_init_agrees() {
        let outcome = run_both(&welford_init_global(), Script::Accumulate(vec![input()]));
        same_within_welford(cpu_slot(&outcome, 0), gpu_slot(&outcome, 0), false, &[1, 2]);
    }
}

/// The planner's own finalize for `func` — `Stddev` or `Var` — over the merged triple
/// its state declares, which sits after `keys` key columns; named as that state's output.
pub(crate) fn dispersion_finalize(func: AggFunc, keys: u32) -> NamedExpr {
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

/// The keyless merge finalized, the `Float64` to `WELFORD_RELATIVE` for the reason
/// `finalized_within_welford` gives.
fn global_finalized_within_welford(func: AggFunc) {
    let outcome = global_finalize_outcome(func);
    same_within_welford(cpu_slot(&outcome, 2), gpu_slot(&outcome, 2), false, &[0]);
}

operator_case! {
    GpuAggregateBatches,
    fn a_global_stddev_finalize_agrees_within_welford() {
        global_finalized_within_welford(AggFunc::Stddev);
    }
}

operator_case! {
    GpuAggregateBatches,
    fn a_global_var_finalize_agrees_within_welford() {
        global_finalized_within_welford(AggFunc::Var);
    }
}

// A Welford under a grouping set (#280): the set path builds the triple through the one
// request builder, and the `sum` after the triples reads its own column, not a neighbour's.

/// One key under ROLLUP, then a `stddev(f64)`, a `var_pop(f64)` and a `sum(i64)` after them.
/// Both triples are over `f64`, which `welford_init` shows has no all-NULL group in `input()`
/// (all-NULL groups are welford-device's).
fn rollup_welford_state() -> Schema {
    let triple = |out: &str| {
        [
            ("$count", DataType::Int64),
            ("$mean", DataType::Float64),
            ("$m2", DataType::Float64),
        ]
        .map(|(suffix, ty)| (format!("{out}{suffix}"), ty))
    };
    let mut named: Vec<(String, DataType)> = vec![
        ("key".to_string(), DataType::Int32),
        ("__grouping_id".to_string(), DataType::UInt8),
    ];
    named.extend(triple("stddev(f64)"));
    named.extend(triple("var_pop(f64)"));
    named.push(("sum(i64)".to_string(), DataType::Int64));
    let fields: Vec<(&str, DataType)> =
        named.iter().map(|(n, t)| (n.as_str(), t.clone())).collect();
    Schema {
        group_keys: vec![0, 1],
        agg_state: vec![
            AggStateColumns {
                output: "stddev(f64)".into(),
                func: AggFunc::Stddev,
                ddof: 1,
                positions: vec![2, 3, 4],
            },
            AggStateColumns {
                output: "var_pop(f64)".into(),
                func: AggFunc::Var,
                ddof: 0,
                positions: vec![5, 6, 7],
            },
        ],
        ..columns(&fields)
    }
}

pub(crate) fn rollup_welford_init() -> GpuAggregate {
    let triple = |out: &str| -> Vec<AggCall> {
        [
            (PlanAgg::Count, "$count", DataType::Int64),
            (PlanAgg::Mean, "$mean", DataType::Float64),
            (PlanAgg::M2, "$m2", DataType::Float64),
        ]
        .into_iter()
        .map(|(func, suffix, ty)| call(func, Expr::column(4, "f64"), &format!("{out}{suffix}"), ty))
        .collect()
    };
    let mut aggs = triple("stddev(f64)");
    aggs.extend(triple("var_pop(f64)"));
    aggs.push(sum_i64());
    let state = rollup_welford_state();
    let body = AggregateBody {
        group_by: vec![Expr::column(1, "key")],
        grouping_sets: vec![vec![false], vec![true]],
        null_exprs: vec![Expr::Literal(ScalarValue::Int32(None))],
        aggs,
        finalize: None,
    };
    GpuAggregate::new(
        Given::of(Schema::new(schema()), BatchLayout::MultipleBatches),
        body,
        state.clone(),
        state,
    )
}

fn rollup_welford_merge() -> GpuAggregateBatches {
    let state = rollup_welford_state();
    let field = |at: u32| state.fields.field(at as usize).clone();
    let merged = |at: u32| AggCall {
        func: PlanAgg::MergeM2,
        args: (at..at + 3)
            .map(|i| Expr::column(i, field(i).name()))
            .collect(),
        outputs: (at..at + 3).map(field).collect(),
    };
    let aggs = vec![
        merged(2),
        merged(5),
        call(
            PlanAgg::Sum,
            Expr::column(8, "sum(i64)"),
            "sum(i64)",
            DataType::Int64,
        ),
    ];
    let group_by = vec![Expr::column(0, "key"), Expr::column(1, "__grouping_id")];
    merge_over(state.clone(), body(group_by, aggs, None), state)
}

/// Rows by `__grouping_id`, then `key`, NULLs first: a rollup repeats a key across its sets, so
/// both sides are paired by position once in this order.
fn by_set_then_key(batch: &RecordBatch) -> RecordBatch {
    let order = lexsort_to_indices(
        &[
            SortColumn {
                values: batch.column(1).clone(),
                options: None,
            },
            SortColumn {
                values: batch.column(0).clone(),
                options: None,
            },
        ],
        None,
    )
    .expect("sortable keys");
    take_record_batch(batch, &order).expect("the rows in that order")
}

const ROLLUP_WELFORD_MOMENTS: [usize; 4] = [3, 4, 6, 7];

operator_case! {
    GpuAggregate,
    fn a_rollup_with_a_stddev_a_var_and_a_sum_agrees() {
        let outcome = run_both(&rollup_welford_init(), Script::Accumulate(vec![input()]));
        same_within_welford(
            &by_set_then_key(cpu_slot(&outcome, 0)),
            &by_set_then_key(gpu_slot(&outcome, 0)),
            false,
            &ROLLUP_WELFORD_MOMENTS,
        );
    }
}

operator_case! {
    GpuAggregateBatches,
    fn a_rollup_welford_merge_agrees() {
        let partial = |seed| {
            let init = run_both(&rollup_welford_init(), Script::Accumulate(vec![synthetic(64, seed)]));
            cpu_slot(&init, 0).clone()
        };
        let outcome = run_both(&rollup_welford_merge(), Script::Accumulate(vec![partial(1), partial(2)]));
        same_within_welford(
            &by_set_then_key(cpu_slot(&outcome, 2)),
            &by_set_then_key(gpu_slot(&outcome, 2)),
            false,
            &ROLLUP_WELFORD_MOMENTS,
        );
    }
}
