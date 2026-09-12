//! `GpuAggregate` and `GpuAggregateBatches` through the harness. The init runs over
//! `synthetic`; a merge consumes state, so its arrivals are state batches this file builds
//! from `synthetic`'s own columns. Every declared type is what the planner would have
//! written — DataFusion's `state_fields`, read off a planning run — because the cpu holds
//! its accumulators to the declaration and a wrong one is a refusal rather than a finding.

use std::sync::Arc;

use datafusion::arrow::array::{Array, ArrayRef, AsArray, Int32Array, Int64Array, UInt64Array};
use datafusion::arrow::compute::cast;
use datafusion::arrow::datatypes::{
    DataType, Field, Float64Type, Int32Type, Schema as ArrowSchema, UInt8Type,
};
use datafusion::arrow::record_batch::RecordBatch;
use datafusion::common::ScalarValue;

use super::script::{Outcome, Script, run_both};
use crate::plan::{
    AggCall, AggFunc, AggStateColumns, AggregateBody, BatchLayout, BinaryOp, Expr, GpuAggregate,
    GpuAggregateBatches, GpuNode, NamedExpr, PlanAgg, Schema,
};
use crate::tests::compare::{Order, assert_same};
use crate::tests::given::{Given, columns};
use crate::tests::synthetic::{decimals, schema, synthetic};

fn input() -> RecordBatch {
    synthetic(64, 1)
}

fn given() -> Box<dyn GpuNode> {
    Given::of(Schema::new(schema()), BatchLayout::MultipleBatches)
}

fn call(func: PlanAgg, arg: Expr, out: &str, ty: DataType) -> AggCall {
    AggCall {
        func,
        args: vec![arg],
        outputs: vec![Field::new(out, ty, true)],
    }
}

fn body(
    group_by: Vec<Expr>,
    aggs: Vec<AggCall>,
    finalize: Option<Vec<NamedExpr>>,
) -> AggregateBody {
    AggregateBody {
        group_by,
        grouping_sets: Vec::new(),
        null_exprs: Vec::new(),
        aggs,
        finalize,
    }
}

/// `[key?, state…]` under the names and types the calls declare.
fn state_of(grouped: bool, aggs: &[AggCall]) -> Schema {
    let mut fields: Vec<(&str, DataType)> = Vec::new();
    if grouped {
        fields.push(("key", DataType::Int32));
    }
    for call in aggs {
        for field in &call.outputs {
            fields.push((field.name().as_str(), field.data_type().clone()));
        }
    }
    columns(&fields)
}

/// The Welford triple's state, `[key, count, mean, m2]` in DataFusion's own types, with
/// the `agg_state` annotation that says the three belong to one `stddev` — without it both
/// backends refuse `M2` and `MergeM2` (`plan/aggregate.rs`, `welford_owners`).
fn welford_state() -> Schema {
    Schema {
        fields: Arc::new(ArrowSchema::new(vec![
            Field::new("key", DataType::Int32, true),
            Field::new("stddev(f64)$count", DataType::UInt64, true),
            Field::new("stddev(f64)$mean", DataType::Float64, true),
            Field::new("stddev(f64)$m2", DataType::Float64, true),
        ])),
        group_keys: vec![0],
        agg_state: vec![AggStateColumns {
            output: "stddev(f64)".to_string(),
            func: AggFunc::Stddev,
            ddof: 1,
            positions: vec![1, 2, 3],
        }],
    }
}

/// A `bug_` test's assertion: the device answered, and with exactly this one batch.
fn gpu_answered(outcome: &Outcome, expected: RecordBatch, order: Order) {
    let gpu = outcome
        .gpu
        .as_ref()
        .unwrap_or_else(|why| panic!("gpu refused: {}", why.message));
    assert_same(&[vec![expected]], gpu, order);
}

fn cpu_slot(outcome: &Outcome, at: usize) -> &RecordBatch {
    &outcome.cpu.as_ref().expect("the cpu answers")[at][0]
}

fn batch_of(columns: Vec<(&str, ArrayRef)>) -> RecordBatch {
    RecordBatch::try_from_iter(columns).expect("columns of one length")
}

/// A Welford state's `[key, count]` as the device answers it: the count exported Int64
/// (#163), both under the aggregate's alias, since `aggregate.cpp` names a struct's children
/// by it — unobservable past the sink, which relabels by the declared schema, so no ticket
/// carries the names.
fn welford_keys_and_counts_as_exported(batch: &RecordBatch, count_type: &DataType) -> RecordBatch {
    batch_of(vec![
        ("key", batch.column(0).clone()),
        ("stddev(f64)", cast(batch.column(1), count_type).unwrap()),
    ])
}

/// The mean and m2 by key. Welford's update is order-dependent and the mean is not
/// dyadic, so the two engines agree to the last few digits and no further — the corpus
/// compares a stddev under `golden_approx_std` at this same relative 1e-11
/// (`test_support/corpus_gpu.rs`), and the harness's exact compare has no such mode.
fn welford_moments_by_key(batch: &RecordBatch) -> Vec<(Option<i32>, f64, f64)> {
    let keys = batch.column(0).as_primitive::<Int32Type>();
    let means = batch.column(2).as_primitive::<Float64Type>();
    let m2s = batch.column(3).as_primitive::<Float64Type>();
    let mut rows: Vec<(Option<i32>, f64, f64)> = (0..batch.num_rows())
        .map(|row| {
            (
                keys.is_valid(row).then(|| keys.value(row)),
                means.value(row),
                m2s.value(row),
            )
        })
        .collect();
    rows.sort_by(|a, b| a.0.cmp(&b.0));
    rows
}

/// The device's Welford state against the cpu's at slot `at`: keys and counts exactly, the
/// count in the device's type, and the moments to a relative 1e-11.
fn welford_answered(outcome: &Outcome, at: usize) {
    let cpu = cpu_slot(outcome, at);
    let gpu = &outcome.gpu.as_ref().expect("the device answers")[at][0];
    assert_same(
        &[vec![welford_keys_and_counts_as_exported(
            cpu,
            &DataType::Int64,
        )]],
        &[vec![welford_keys_and_counts_as_exported(
            gpu,
            &DataType::Int64,
        )]],
        Order::Any,
    );
    for ((key, c_mean, c_m2), (_, g_mean, g_m2)) in welford_moments_by_key(cpu)
        .into_iter()
        .zip(welford_moments_by_key(gpu))
    {
        let close = |a: f64, b: f64| (a - b).abs() <= 1e-11 * a.abs().max(b.abs()).max(1.0);
        assert!(
            close(c_mean, g_mean),
            "key {key:?}: mean {c_mean} vs {g_mean}"
        );
        assert!(close(c_m2, g_m2), "key {key:?}: m2 {c_m2} vs {g_m2}");
    }
}

/// The cpu's grouping-set state as the device answers it: `__grouping_id` Int32 with the
/// device's bit order, which sets bit `i` for masked key `i` where DataFusion puts the
/// first key highest — over two keys, 1 and 2 swap (#65).
fn grouping_sets_as_exported(cpu: &RecordBatch) -> RecordBatch {
    let gid: ArrayRef = Arc::new(Int32Array::from_iter(
        cpu.column(2)
            .as_primitive::<UInt8Type>()
            .iter()
            .map(|g| g.map(|g| (i32::from(g & 1) << 1) | i32::from(g >> 1))),
    ));
    batch_of(vec![
        ("key", cpu.column(0).clone()),
        ("b", cpu.column(1).clone()),
        ("__grouping_id", gid),
        ("sum(i64)", cpu.column(3).clone()),
    ])
}

// `GpuAggregate`: aggregators over raw rows, state out — or, with a finalize, the
// single-node shortcut.

/// `aggs` over `synthetic`, grouped by `key` where `grouped`, nothing finalized.
fn init(grouped: bool, aggs: Vec<AggCall>) -> GpuAggregate {
    let state = state_of(grouped, &aggs);
    let group_by = if grouped {
        vec![Expr::column(1, "key")]
    } else {
        Vec::new()
    };
    GpuAggregate::new(given(), body(group_by, aggs, None), state.clone(), state)
}

operator_case! {
    GpuAggregate,
    fn a_grouped_sum_min_max_count_agree() {
        let node = init(
            true,
            vec![
                call(PlanAgg::Sum, Expr::column(3, "i64"), "sum(i64)", DataType::Int64),
                call(PlanAgg::Min, Expr::column(2, "i32"), "min(i32)", DataType::Int32),
                call(PlanAgg::Max, Expr::column(5, "s"), "max(s)", DataType::Utf8),
                call(PlanAgg::Count, Expr::column(2, "i32"), "count(i32)", DataType::Int64),
            ],
        );
        run_both(&node, Script::Exec(vec![input()])).same(Order::Any);
    }
}

operator_case! {
    GpuAggregate,
    fn a_global_sum_min_max_count_agree() {
        let node = init(
            false,
            vec![
                call(PlanAgg::Sum, Expr::column(3, "i64"), "sum(i64)", DataType::Int64),
                call(PlanAgg::Min, Expr::column(6, "d"), "min(d)", DataType::Date32),
                call(PlanAgg::Max, Expr::column(4, "f64"), "max(f64)", DataType::Float64),
                call(PlanAgg::Count, Expr::column(5, "s"), "count(s)", DataType::Int64),
            ],
        );
        run_both(&node, Script::Exec(vec![input()])).same(Order::Any);
    }
}

fn welford_init() -> GpuAggregate {
    let state = welford_state();
    let aggs = vec![
        call(
            PlanAgg::Count,
            Expr::column(4, "f64"),
            "stddev(f64)$count",
            DataType::UInt64,
        ),
        call(
            PlanAgg::Mean,
            Expr::column(4, "f64"),
            "stddev(f64)$mean",
            DataType::Float64,
        ),
        call(
            PlanAgg::M2,
            Expr::column(4, "f64"),
            "stddev(f64)$m2",
            DataType::Float64,
        ),
    ];
    GpuAggregate::new(
        given(),
        body(vec![Expr::column(1, "key")], aggs, None),
        state.clone(),
        state,
    )
}

// #163 — cuDF's Welford count exports Int64 where every plan declares UInt64.
operator_case! {
    GpuAggregate,
    fn bug_a_welford_init_exports_its_count_as_int64() {
        let outcome = run_both(&welford_init(), Script::Exec(vec![input()]));
        welford_answered(&outcome, 0);
    }
}

/// DataFusion's `sum` of a `Decimal128(18, 2)` declares `Decimal128(28, 2)` — the precision
/// plus ten, capped at 38 — read off `SELECT sum(dec) FROM t`.
fn decimal_sum() -> GpuAggregate {
    let state = columns(&[("sum(dec)", DataType::Decimal128(28, 2))]);
    let aggs = vec![call(
        PlanAgg::Sum,
        Expr::column(1, "dec"),
        "sum(dec)",
        DataType::Decimal128(28, 2),
    )];
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

// #187 — the device exports every decimal at precision 38 whatever was declared; the sum
// itself is the cpu's.
operator_case! {
    GpuAggregate,
    fn bug_a_decimal_sum_is_exported_at_precision_38() {
        let outcome = run_both(&decimal_sum(), Script::Exec(vec![decimals(64, 1)]));
        let cpu = cpu_slot(&outcome, 0);
        let widened = batch_of(vec![(
            "sum(dec)",
            cast(cpu.column(0), &DataType::Decimal128(38, 2)).unwrap(),
        )]);
        gpu_answered(&outcome, widened, Order::Any);
    }
}

// The single-node shortcut: init aggregators and finalize expressions on one node. The
// finalize indexes `[keys…, state…]`; `avg` is `sum / count` over the state.
operator_case! {
    GpuAggregate,
    fn the_single_node_shortcut_finalizes_an_average() {
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
        let aggs = vec![
            call(PlanAgg::Sum, Expr::column(3, "i64"), "avg(i64)$sum", DataType::Int64),
            call(PlanAgg::Count, Expr::column(3, "i64"), "avg(i64)$count", DataType::Int64),
        ];
        let node = GpuAggregate::new(
            given(),
            body(
                vec![Expr::column(1, "key")],
                aggs,
                Some(vec![NamedExpr::new(divide, "avg")]),
            ),
            intermediate,
            output,
        );
        run_both(&node, Script::Exec(vec![input()])).same(Order::Any);
    }
}

/// Two keys, `key` and `b`, over the sets given; `__grouping_id` is `UInt8` for up to
/// eight keys, as DataFusion's partial declares it, and each masked key takes a typed NULL.
fn grouping_sets(sets: Vec<Vec<bool>>) -> GpuAggregate {
    let state = columns(&[
        ("key", DataType::Int32),
        ("b", DataType::Boolean),
        ("__grouping_id", DataType::UInt8),
        ("sum(i64)", DataType::Int64),
    ]);
    let body = AggregateBody {
        group_by: vec![Expr::column(1, "key"), Expr::column(7, "b")],
        grouping_sets: sets,
        null_exprs: vec![
            Expr::Literal(ScalarValue::Int32(None)),
            Expr::Literal(ScalarValue::Boolean(None)),
        ],
        aggs: vec![call(
            PlanAgg::Sum,
            Expr::column(3, "i64"),
            "sum(i64)",
            DataType::Int64,
        )],
        finalize: None,
    };
    GpuAggregate::new(given(), body, state.clone(), state)
}

// #65 — three sets, both keys, `key` alone, neither: the rows agree, and the id is the
// device's own encoding in its own type.
operator_case! {
    GpuAggregate,
    fn bug_grouping_sets_carry_the_devices_own_id_and_type() {
        let node = grouping_sets(vec![
            vec![false, false],
            vec![false, true],
            vec![true, true],
        ]);
        let outcome = run_both(&node, Script::Exec(vec![input()]));
        let expected = grouping_sets_as_exported(cpu_slot(&outcome, 0));
        gpu_answered(&outcome, expected, Order::Any);
    }
}

// Empty inputs, each its own case.
operator_case! {
    GpuAggregate,
    fn a_grouped_aggregate_over_zero_rows_is_zero_rows_on_both() {
        let node = init(
            true,
            vec![call(PlanAgg::Sum, Expr::column(3, "i64"), "sum(i64)", DataType::Int64)],
        );
        run_both(&node, Script::Exec(vec![synthetic(0, 1)])).same(Order::Any);
    }
}

// A global aggregate owes its identity row whatever arrived: count 0, sum NULL.
operator_case! {
    GpuAggregate,
    fn a_global_aggregate_over_zero_rows_keeps_its_identity_row() {
        let node = init(
            false,
            vec![
                call(PlanAgg::Count, Expr::column(2, "i32"), "count(i32)", DataType::Int64),
                call(PlanAgg::Sum, Expr::column(3, "i64"), "sum(i64)", DataType::Int64),
            ],
        );
        run_both(&node, Script::Exec(vec![synthetic(0, 1)])).same(Order::Any);
    }
}

// #65 — zero rows on both, under the device's Int32 id.
operator_case! {
    GpuAggregate,
    fn bug_grouping_sets_over_zero_rows_are_zero_rows_under_the_devices_id_type() {
        let node = grouping_sets(vec![vec![false, false], vec![true, true]]);
        let outcome = run_both(&node, Script::Exec(vec![synthetic(0, 1)]));
        let expected = grouping_sets_as_exported(cpu_slot(&outcome, 0));
        assert_eq!(expected.num_rows(), 0);
        gpu_answered(&outcome, expected, Order::Any);
    }
}

// `GpuAggregateBatches`: state in, merged state out at done, or the finalized columns.

/// `[key, sum(i64)]` rows: the state a partial sum would have emitted, one per row of
/// `synthetic(rows, seed)`, so a merge over several of them has duplicate keys to fold.
fn sum_state(rows: usize, seed: u64) -> RecordBatch {
    let source = synthetic(rows, seed);
    RecordBatch::try_new(
        columns(&[("key", DataType::Int32), ("sum(i64)", DataType::Int64)])
            .fields
            .clone(),
        vec![source.column(1).clone(), source.column(3).clone()],
    )
    .expect("two of the fixture's columns")
}

fn merge_over(state: Schema, body: AggregateBody, output: Schema) -> GpuAggregateBatches {
    GpuAggregateBatches::new(
        Given::of(state.clone(), BatchLayout::MultipleBatches),
        body,
        state,
        output,
    )
}

fn merge_sum(finalize: bool) -> GpuAggregateBatches {
    let state = columns(&[("key", DataType::Int32), ("sum(i64)", DataType::Int64)]);
    let output = if finalize {
        columns(&[("key", DataType::Int32), ("total", DataType::Int64)])
    } else {
        state.clone()
    };
    let aggs = vec![call(
        PlanAgg::Sum,
        Expr::column(1, "sum(i64)"),
        "sum(i64)",
        DataType::Int64,
    )];
    let finalize = finalize.then(|| vec![NamedExpr::new(Expr::column(1, "sum(i64)"), "total")]);
    merge_over(
        state,
        body(vec![Expr::column(0, "key")], aggs, finalize),
        output,
    )
}

operator_case! {
    GpuAggregateBatches,
    fn a_merge_emits_state_at_done() {
        let arrivals = vec![
            sum_state(32, 1),
            sum_state(32, 2),
            sum_state(0, 3),
            sum_state(32, 4),
        ];
        run_both(&merge_sum(false), Script::Accumulate(arrivals)).same(Order::Any);
    }
}

operator_case! {
    GpuAggregateBatches,
    fn a_merge_with_a_finalize_emits_the_projected_columns() {
        let arrivals = vec![sum_state(32, 1), sum_state(32, 2)];
        run_both(&merge_sum(true), Script::Accumulate(arrivals)).same(Order::Any);
    }
}

// The threshold is 1 MiB on the cpu and 64 MiB on the device (`{cpu,gpu}_backend/backend.rs`),
// so the device's is the one to size for: a `sum_state` row is 12 bytes and change by the
// shared formula, so 1.5M rows is about 18 MiB, four arrivals cross 64 MiB, and the
// fifth is folded at done. The fold must not change the answer on either.
operator_case! {
    GpuAggregateBatches,
    fn arrivals_crossing_the_compaction_threshold_fold_the_same() {
        let arrivals = (1..=5).map(|seed| sum_state(1_500_000, seed)).collect();
        run_both(&merge_sum(false), Script::Accumulate(arrivals)).same(Order::Any);
    }
}

// A count merges by sum: the merge aggregator is `Sum` over a column named `count(…)`.
operator_case! {
    GpuAggregateBatches,
    fn a_count_merges_by_sum() {
        let state = columns(&[("key", DataType::Int32), ("count(i32)", DataType::Int64)]);
        let aggs = vec![call(
            PlanAgg::Sum,
            Expr::column(1, "count(i32)"),
            "count(i32)",
            DataType::Int64,
        )];
        let node = merge_over(
            state.clone(),
            body(vec![Expr::column(0, "key")], aggs, None),
            state,
        );
        let counts = |seed| {
            let s = sum_state(32, seed);
            let ones: ArrayRef = Arc::new(Int64Array::from(vec![1i64; s.num_rows()]));
            RecordBatch::try_new(s.schema(), vec![s.column(0).clone(), ones]).unwrap()
        };
        run_both(&node, Script::Accumulate(vec![counts(1), counts(2)])).same(Order::Any);
    }
}

/// `merge_m2` is not per column: the three state columns go in as one call's arguments.
fn welford_merge() -> GpuAggregateBatches {
    let state = welford_state();
    let aggs = vec![AggCall {
        func: PlanAgg::MergeM2,
        args: vec![
            Expr::column(1, "stddev(f64)$count"),
            Expr::column(2, "stddev(f64)$mean"),
            Expr::column(3, "stddev(f64)$m2"),
        ],
        outputs: state.fields.fields()[1..]
            .iter()
            .map(|f| f.as_ref().clone())
            .collect(),
    }];
    merge_over(
        state.clone(),
        body(vec![Expr::column(0, "key")], aggs, None),
        state,
    )
}

/// One Welford partial per row of `synthetic(32, seed)`: count 1, mean `f64`, m2 0 —
/// what an init over one row emits.
fn welford_partial(seed: u64) -> RecordBatch {
    let s = synthetic(32, seed);
    let ones: ArrayRef = Arc::new(UInt64Array::from(vec![1u64; 32]));
    let zeros: ArrayRef = Arc::new(datafusion::arrow::array::Float64Array::from(vec![0.0; 32]));
    RecordBatch::try_new(
        welford_state().fields.clone(),
        vec![s.column(1).clone(), ones, s.column(4).clone(), zeros],
    )
    .unwrap()
}

// #163 — the merged count comes back Int64 too: cuDF's `merge_m2` takes an Int32 count and
// the device widens what it returns to Int64, never to the declared UInt64.
operator_case! {
    GpuAggregateBatches,
    fn bug_a_welford_merge_exports_its_count_as_int64() {
        let arrivals = vec![welford_partial(1), welford_partial(2), welford_partial(3)];
        let outcome = run_both(&welford_merge(), Script::Accumulate(arrivals));
        welford_answered(&outcome, 3);
    }
}

/// `SELECT avg(dec) FROM t` over `dec DECIMAL(18, 2)`, as the planner writes it: DataFusion
/// declares the state `[count UInt64, sum Decimal128(18, 2)]` and the output
/// `Decimal128(22, 6)`; the decomposition orders the state `$sum, $count`, and the
/// finalize (`plan/aggregates.rs`) is the sum cast to the output type over the count cast
/// to that precision at scale 0.
fn planned_decimal_average() -> (Schema, Schema, Vec<NamedExpr>) {
    let out = DataType::Decimal128(22, 6);
    let state = columns(&[
        ("avg(dec)$sum", DataType::Decimal128(18, 2)),
        ("avg(dec)$count", DataType::UInt64),
    ]);
    let output = columns(&[("avg(dec)", out.clone())]);
    let divide = Expr::binary(
        Expr::Cast {
            expr: Box::new(Expr::column(0, "avg(dec)$sum")),
            target: out.clone(),
        },
        BinaryOp::Divide,
        Expr::Cast {
            expr: Box::new(Expr::column(1, "avg(dec)$count")),
            target: DataType::Decimal128(22, 0),
        },
        out,
    );
    (state, output, vec![NamedExpr::new(divide, "avg(dec)")])
}

// #163 — the declared output is never checked against the expression that produces it:
// arrow types the finalize's divide at (26,10) where the planner declares (22,6), and the
// cpu refuses at `declared_as`. What the device answers is not read past the refusal.
operator_case! {
    GpuAggregateBatches,
    fn bug_a_decimal_average_is_refused_on_the_cpu() {
        let (state, output, finalize) = planned_decimal_average();
        let aggs = vec![
            call(
                PlanAgg::Sum,
                Expr::column(0, "avg(dec)$sum"),
                "avg(dec)$sum",
                DataType::Decimal128(18, 2),
            ),
            call(
                PlanAgg::Sum,
                Expr::column(1, "avg(dec)$count"),
                "avg(dec)$count",
                DataType::UInt64,
            ),
        ];
        let node = merge_over(state.clone(), body(Vec::new(), aggs, Some(finalize)), output);
        // One partial per row: the decimal itself, counted once.
        let partial = |seed| {
            let d = decimals(32, seed);
            let ones: ArrayRef = Arc::new(UInt64Array::from(vec![1u64; 32]));
            RecordBatch::try_new(state.fields.clone(), vec![d.column(1).clone(), ones]).unwrap()
        };
        let outcome = run_both(&node, Script::Accumulate(vec![partial(1), partial(2)]));
        let why = outcome.cpu_refuses();
        assert!(
            why.contains("expected Decimal128(22, 6) but found Decimal128(26, 10)"),
            "{why}"
        );
    }
}

// Empty inputs, each its own case.
operator_case! {
    GpuAggregateBatches,
    fn a_merge_over_no_arrival_answers_nothing_on_both() {
        run_both(&merge_sum(false), Script::Accumulate(Vec::new())).same(Order::Any);
    }
}

operator_case! {
    GpuAggregateBatches,
    fn a_merge_over_one_zero_row_arrival_is_zero_rows_on_both() {
        run_both(&merge_sum(false), Script::Accumulate(vec![sum_state(0, 1)])).same(Order::Any);
    }
}

operator_case! {
    GpuAggregateBatches,
    fn a_zero_row_arrival_among_others_changes_nothing() {
        let arrivals = vec![sum_state(32, 1), sum_state(0, 2), sum_state(32, 3)];
        run_both(&merge_sum(false), Script::Accumulate(arrivals)).same(Order::Any);
    }
}

operator_case! {
    GpuAggregateBatches,
    fn a_finalize_over_no_arrival_answers_nothing_on_both() {
        run_both(&merge_sum(true), Script::Accumulate(Vec::new())).same(Order::Any);
    }
}

// #199 — a global merge over no arrival owes its identity row, count 0. The cpu answers one
// row, and it is sum's identity rather than count's — NULL, since a count merges by sum —
// and the device answers nothing; SQL's 0 is on neither.
operator_case! {
    GpuAggregateBatches,
    fn bug_a_global_merge_over_no_arrival_answers_nothing_on_the_device() {
        let state = columns(&[("count(i32)", DataType::Int64)]);
        let aggs = vec![call(
            PlanAgg::Sum,
            Expr::column(0, "count(i32)"),
            "count(i32)",
            DataType::Int64,
        )];
        let node = merge_over(state.clone(), body(Vec::new(), aggs, None), state);
        let outcome = run_both(&node, Script::Accumulate(Vec::new()));
        let null_count: ArrayRef = Arc::new(Int64Array::from(vec![None::<i64>]));
        assert_same(
            &[vec![batch_of(vec![("count(i32)", null_count)])]],
            outcome.cpu.as_ref().expect("the cpu answers"),
            Order::Any,
        );
        let gpu = outcome.gpu.as_ref().expect("the device answers");
        assert_eq!(gpu.len(), 1, "one slot, the done");
        assert!(gpu[0].is_empty(), "and nothing in it");
    }
}
