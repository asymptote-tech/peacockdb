//! Every decomposition's init, built through the backend, emits exactly the state
//! `PlanAgg::state_type` derives — column by column, with no decimal escape: an init's
//! accumulators are the producers the table was read off, so a difference here is the
//! table being wrong, not a widening to cast away. The escape is a merge's alone, and
//! both halves of that are pinned here.

use super::*;
use crate::plan::{AggSpec, GpuAggregate, GpuAggregateBatches, NamedExpr, decomposition, finalize};
use datafusion::arrow::array::{Decimal128Array, Float64Array};
use datafusion::common::ScalarValue;
use datafusion::logical_expr::type_coercion::functions::data_types_with_aggregate_udf;

/// The SQL name each aggregate is resolved from, sample form for the Welford pair.
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

const EVERY_FUNC: [AggFunc; 7] = [
    AggFunc::Sum,
    AggFunc::Min,
    AggFunc::Max,
    AggFunc::Count,
    AggFunc::Avg,
    AggFunc::Stddev,
    AggFunc::Var,
];

const ARGUMENTS: [(u32, &str, DataType); 3] = [
    (1, "i", DataType::Int32),
    (2, "d", DataType::Decimal128(15, 2)),
    (3, "f", DataType::Float64),
];

const INPUT: [(&str, DataType); 4] = [
    ("k", DataType::Utf8),
    ("i", DataType::Int32),
    ("d", DataType::Decimal128(15, 2)),
    ("f", DataType::Float64),
];

fn four_rows() -> CpuBatch {
    let k: ArrayRef = Arc::new(StringArray::from(vec![
        Some("a"),
        Some("a"),
        None,
        Some("b"),
    ]));
    let i: ArrayRef = Arc::new(Int32Array::from(vec![Some(2), None, Some(4), Some(6)]));
    let d: ArrayRef = Arc::new(
        Decimal128Array::from(vec![Some(250), Some(-100), None, Some(775)])
            .with_precision_and_scale(15, 2)
            .expect("a Decimal128(15, 2)"),
    );
    let f: ArrayRef = Arc::new(Float64Array::from(vec![
        Some(1.5),
        Some(2.5),
        None,
        Some(4.0),
    ]));
    CpuBatch::new(
        RecordBatch::try_new(
            Arc::new(columns(&INPUT).fields.as_ref().clone()),
            vec![k, i, d, f],
        )
        .expect("the columns fit the schema"),
    )
}

/// The argument as the plan carries it: the column, cast to what DataFusion's logical
/// planner coerces `func`'s argument to before this engine sees the aggregate.
fn argument(func: AggFunc, ordinal: u32, name: &str, input: &DataType) -> (Expr, DataType) {
    let udaf = SessionContext::new()
        .state()
        .aggregate_functions()
        .get(sql_name(func))
        .cloned()
        .expect("a DataFusion aggregate");
    let coerced = data_types_with_aggregate_udf(std::slice::from_ref(input), &udaf)
        .expect("the argument coerces")[0]
        .clone();
    let column = Expr::column(ordinal, name);
    let expr = if &coerced == input {
        column
    } else {
        Expr::Cast {
            expr: Box::new(column),
            target: coerced.clone(),
        }
    };
    (expr, coerced)
}

/// The init `decompose` would build for `func(arg)` grouped by `k`, with its state typed
/// by `state_type`, and the state it declares.
fn init_of(func: AggFunc, ordinal: u32, name: &str, input: &DataType) -> (GpuAggregate, Schema) {
    init_declaring(func, ordinal, name, input, true, |state_type| state_type)
}

/// [`init_of`] with no group key: the init that owes a row whatever arrived.
fn keyless_init_of(func: AggFunc, ordinal: u32, name: &str, input: &DataType) -> GpuAggregate {
    init_declaring(func, ordinal, name, input, false, |state_type| state_type).0
}

/// [`init_parts`] built into the node, beside the state it declares.
fn init_declaring(
    func: AggFunc,
    ordinal: u32,
    name: &str,
    input: &DataType,
    keyed: bool,
    declare: impl Fn(DataType) -> DataType,
) -> (GpuAggregate, Schema) {
    let (body, state) = init_parts(func, ordinal, name, input, keyed, declare);
    let node = GpuAggregate::new(
        Given::of_columns(&INPUT),
        body,
        state.clone(),
        state.clone(),
    );
    (node, state)
}

/// The body and state of the init `decompose` would build for `func(arg)`, grouped by `k`
/// where `keyed`, each state column declared as `declare` says, given `state_type`'s answer
/// — the answer itself, or the type a test wants the declaration wrong by.
fn init_parts(
    func: AggFunc,
    ordinal: u32,
    name: &str,
    input: &DataType,
    keyed: bool,
    declare: impl Fn(DataType) -> DataType,
) -> (AggregateBody, Schema) {
    let (arg, arg_type) = argument(func, ordinal, name, input);
    let rule = decomposition(func);
    let output = format!("{}({name})", sql_name(func));
    let keys = usize::from(keyed) as u32;
    let mut fields = Vec::new();
    if keyed {
        fields.push(("k".to_string(), DataType::Utf8));
    }
    let mut aggs = Vec::new();
    for (suffix, agg) in rule.state {
        let state_type = declare(
            agg.state_type(&arg_type)
                .expect("the table types every state"),
        );
        let column = format!("{output}{suffix}");
        aggs.push(AggCall {
            func: *agg,
            args: vec![arg.clone()],
            outputs: vec![Field::new(&column, state_type.clone(), true)],
        });
        fields.push((column, state_type));
    }
    let named: Vec<(&str, DataType)> = fields
        .iter()
        .map(|(name, kind)| (name.as_str(), kind.clone()))
        .collect();
    let mut state = columns(&named);
    state.group_keys = (0..keys).collect();
    if matches!(func, AggFunc::Stddev | AggFunc::Var) {
        state.agg_state = vec![AggStateColumns {
            output,
            func,
            ddof: 1,
            positions: (keys..keys + 3).collect(),
        }];
    }
    let group_by = match keyed {
        true => vec![Expr::column(0, "k")],
        false => Vec::new(),
    };
    let body = AggregateBody {
        group_by,
        grouping_sets: Vec::new(),
        null_exprs: Vec::new(),
        aggs,
        finalize: None,
    };
    (body, state)
}

/// The producer half of `state_type`'s table, through the backend: for every aggregate
/// over an `Int32`, a `Decimal128(15, 2)` and a `Float64` argument, the init constructs
/// — `check_state_layout` passing with nothing to escape — and what it emits carries the
/// declared types exactly.
#[test]
fn every_init_emits_the_state_its_decomposition_declares() {
    for func in EVERY_FUNC {
        for (ordinal, name, input) in &ARGUMENTS {
            let (node, state) = init_of(func, *ordinal, name, input);
            let spec = AggSpec { func, ddof: 1 };
            let mut exec = CpuExec::aggregate(&node, &columns(&INPUT).fields, ctx())
                .unwrap_or_else(|why| panic!("{spec:?} over {input}: {why}"));
            let (out, _) = exec.exec(four_rows()).expect("four rows aggregate");
            let produced: Vec<(String, DataType)> = out
                .record_batch()
                .schema()
                .fields()
                .iter()
                .map(|field| (field.name().clone(), field.data_type().clone()))
                .collect();
            let declared: Vec<(String, DataType)> = state
                .fields
                .fields()
                .iter()
                .map(|field| (field.name().clone(), field.data_type().clone()))
                .collect();
            assert_eq!(produced, declared, "{spec:?} over {input}");
        }
    }
}

/// The guard an init has and a merge does not. `sum(d)` over a `Decimal128(15, 2)` is
/// produced at `(25, 2)`; declared at the input's own type — the declaration before
/// `state_type` derived it — the init refuses to build, naming the column and both types.
///
/// Red without the phase in `check_state_layout`: the merge's widening escape admits a
/// produced `(25, 2)` over a declared `(15, 2)`, and the init builds on a wrong table.
#[test]
fn an_init_declaring_a_narrower_decimal_than_it_produces_is_refused() {
    let input = DataType::Decimal128(15, 2);
    let (node, _) = init_declaring(AggFunc::Sum, 2, "d", &input, true, |_| input.clone());
    let refused = CpuExec::aggregate(&node, &columns(&INPUT).fields, ctx())
        .err()
        .expect("an init declaring a state its accumulator does not produce is refused");
    let message = refused.to_string();
    assert!(
        message.contains(
            "column 1 is Decimal128(15, 2) in the declared state and Decimal128(25, 2) in \
             the one DataFusion's accumulators produce"
        ),
        "{message}"
    );
}

/// The escape's remaining reason: a merge sums a `$sum` column that is already a sum, and
/// DataFusion's sum over the declared `(25, 2)` produces a `(35, 2)`, wider by the ten
/// digits a decimal sum gains per merge. The merge constructs, and `declared_as` casts
/// each batch back to the declaration.
///
/// Red without the escape: `column 1 is Decimal128(25, 2) in the declared state and
/// Decimal128(35, 2) in the one DataFusion's accumulators produce`.
#[test]
fn a_merge_over_a_widened_decimal_sum_constructs() {
    let sum = DataType::Decimal128(25, 2);
    let mut state = columns(&[("k", DataType::Utf8), ("avg(d)$sum", sum.clone())]);
    state.group_keys = vec![0];
    let body = AggregateBody {
        group_by: vec![Expr::column(0, "k")],
        grouping_sets: Vec::new(),
        null_exprs: Vec::new(),
        aggs: vec![AggCall {
            func: PlanAgg::Sum,
            args: vec![Expr::column(1, "avg(d)$sum")],
            outputs: vec![Field::new("avg(d)$sum", sum, true)],
        }],
        finalize: None,
    };
    let node = GpuAggregateBatches::new(
        Given::of(state.clone(), BatchLayout::MultipleBatches),
        body,
        state.clone(),
        state.clone(),
    );
    CpuAccumulator::aggregate(&node, &state.fields, ctx(), 1 << 20)
        .expect("a merge over a sum DataFusion widens builds");
}

/// [`keyless_init_of`] as the single-node shortcut builds it: the state finalized in the same
/// node, under the type DataFusion's `func` answers over a non-decimal argument — the first
/// state column's for sum, min, max and count, `Float64` for the rest.
fn keyless_shortcut_of(func: AggFunc, ordinal: u32, name: &str, input: &DataType) -> GpuAggregate {
    let (mut body, state) = init_parts(func, ordinal, name, input, false, |state_type| state_type);
    let output = format!("{}({name})", sql_name(func));
    let state_fields: Vec<Field> = state
        .fields
        .fields()
        .iter()
        .map(|field| field.as_ref().clone())
        .collect();
    let out_type = match func {
        AggFunc::Avg | AggFunc::Stddev | AggFunc::Var => DataType::Float64,
        _ => state_fields[0].data_type().clone(),
    };
    let expr = finalize(AggSpec { func, ddof: 1 }, &state_fields, 0, &out_type);
    body.finalize = Some(vec![NamedExpr::new(expr, &output)]);
    GpuAggregate::new(
        Given::of_columns(&INPUT),
        body,
        state,
        columns(&[(output.as_str(), out_type)]),
    )
}

fn zero_rows() -> CpuBatch {
    CpuBatch::new(RecordBatch::new_empty(columns(&INPUT).fields.clone()))
}

/// What a keyless init owes over no input, through the backend: one row, each state column
/// the value `PlanAgg::empty_state` gives — over no batch, and over two zero-row batches,
/// which it releases without running. Every decomposition, the Welford pair included: its
/// `(0, 0.0, 0.0)` is DataFusion's own variance state over no rows.
#[test]
fn every_keyless_init_answers_its_empty_state_over_no_input() {
    for func in EVERY_FUNC {
        for (ordinal, name, input) in &ARGUMENTS {
            let node = keyless_init_of(func, *ordinal, name, input);
            let (_, arg_type) = argument(func, *ordinal, name, input);
            let expected: Vec<ScalarValue> = node
                .body
                .aggs
                .iter()
                .map(|agg| {
                    agg.func
                        .empty_state(&arg_type)
                        .expect("every init state is typed")
                })
                .collect();
            for zero_row_batches in [0, 2] {
                let mut init =
                    CpuAccumulator::aggregate_init(&node, &columns(&INPUT).fields, ctx())
                        .unwrap_or_else(|why| panic!("{func:?} over {input}: {why}"));
                for _ in 0..zero_row_batches {
                    let (out, _) = init.accumulate_and_fetch(zero_rows()).expect("accepted");
                    assert!(
                        out.is_empty(),
                        "{func:?} over {input}: a zero-row batch emits"
                    );
                }
                let (out, _) = init.mark_done_and_fetch().expect("done is accepted");
                let [row] = out.as_slice() else {
                    panic!("{func:?} over {input}: {} batches at done", out.len())
                };
                let row = row.record_batch();
                assert_eq!(row.num_rows(), 1, "{func:?} over {input}");
                for (column, want) in expected.iter().enumerate() {
                    let got = ScalarValue::try_from_array(row.column(column).as_ref(), 0)
                        .expect("a scalar at row 0");
                    assert_eq!(&got, want, "{func:?} over {input}, column {column}");
                }
            }
        }
    }
}

/// A grouped init over no input owes no groups, and emits nothing — not a zero-row batch.
#[test]
fn a_grouped_init_over_no_input_emits_nothing() {
    let (node, _) = init_of(AggFunc::Sum, 1, "i", &DataType::Int32);
    for zero_row_batches in [0, 2] {
        let mut init = CpuAccumulator::aggregate_init(&node, &columns(&INPUT).fields, ctx())
            .expect("the init builds");
        for _ in 0..zero_row_batches {
            assert!(
                init.accumulate_and_fetch(zero_rows())
                    .expect("accepted")
                    .0
                    .is_empty()
            );
        }
        assert!(init.mark_done_and_fetch().expect("done").0.is_empty());
    }
}

/// The shortcut over no input finalizes the identity row, for every function the cpu answers
/// — the Welford pair included, whose device half is welford-device's: count 0, every other
/// output NULL (a sample or population divisor over a zero count is NULL, never an error).
/// Over no batch and over two zero-row batches. A decimal `avg` is the harness's
/// `a_decimal_average_shortcut_over_no_input_answers_null_on_both`.
#[test]
fn every_keyless_shortcut_answers_count_0_and_nulls_over_no_input() {
    for func in EVERY_FUNC {
        for (ordinal, name, input) in [(1, "i", DataType::Int32), (3, "f", DataType::Float64)] {
            let node = keyless_shortcut_of(func, ordinal, name, &input);
            let declared = node.kind().schema().expect("not a sink").fields.clone();
            let want = match func {
                AggFunc::Count => ScalarValue::Int64(Some(0)),
                _ => ScalarValue::try_from(declared.field(0).data_type()).expect("a typed NULL"),
            };
            for zero_row_batches in [0, 2] {
                let mut init =
                    CpuAccumulator::aggregate_init(&node, &columns(&INPUT).fields, ctx())
                        .unwrap_or_else(|why| panic!("{func:?} over {input}: {why}"));
                for _ in 0..zero_row_batches {
                    assert!(
                        init.accumulate_and_fetch(zero_rows())
                            .expect("accepted")
                            .0
                            .is_empty()
                    );
                }
                let (out, _) = init.mark_done_and_fetch().expect("done is accepted");
                let [row] = out.as_slice() else {
                    panic!("{func:?} over {input}: {} batches at done", out.len())
                };
                let row = row.record_batch();
                assert_eq!(row.num_rows(), 1, "{func:?} over {input}");
                assert_eq!(
                    row.column(0).data_type(),
                    declared.field(0).data_type(),
                    "{func:?} over {input}"
                );
                let got = ScalarValue::try_from_array(row.column(0).as_ref(), 0).expect("row 0");
                assert_eq!(got, want, "{func:?} over {input}");
            }
        }
    }
}
