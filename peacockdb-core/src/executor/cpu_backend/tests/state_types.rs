//! Every decomposition's init, built through the backend, emits exactly the state
//! `PlanAgg::state_type` derives — column by column, with no decimal escape: an init's
//! accumulators are the producers the table was read off, so a difference here is the
//! table being wrong, not a widening to cast away.

use super::*;
use crate::plan::{AggSpec, GpuAggregate, decomposition};
use datafusion::arrow::array::{Decimal128Array, Float64Array};
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
    let (arg, arg_type) = argument(func, ordinal, name, input);
    let rule = decomposition(func);
    let output = format!("{}({name})", sql_name(func));
    let mut fields = vec![("k".to_string(), DataType::Utf8)];
    let mut aggs = Vec::new();
    for (suffix, agg) in rule.state {
        let state_type = agg
            .state_type(&arg_type)
            .expect("the table types every state");
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
    state.group_keys = vec![0];
    if matches!(func, AggFunc::Stddev | AggFunc::Var) {
        state.agg_state = vec![AggStateColumns {
            output,
            func,
            ddof: 1,
            positions: vec![1, 2, 3],
        }];
    }
    let node = GpuAggregate::new(
        Given::of_columns(&INPUT),
        AggregateBody {
            group_by: vec![Expr::column(0, "k")],
            grouping_sets: Vec::new(),
            null_exprs: Vec::new(),
            aggs,
            finalize: None,
        },
        state.clone(),
        state.clone(),
    );
    (node, state)
}

/// The producer half of `state_type`'s table, through the backend: for every aggregate
/// over an `Int32`, a `Decimal128(15, 2)` and a `Float64` argument, the init constructs
/// — `check_state_layout` passing with nothing to escape — and what it emits carries the
/// declared types exactly.
#[test]
fn every_init_emits_the_state_its_decomposition_declares() {
    let every_func = [
        AggFunc::Sum,
        AggFunc::Min,
        AggFunc::Max,
        AggFunc::Count,
        AggFunc::Avg,
        AggFunc::Stddev,
        AggFunc::Var,
    ];
    let arguments = [
        (1, "i", DataType::Int32),
        (2, "d", DataType::Decimal128(15, 2)),
        (3, "f", DataType::Float64),
    ];
    for func in every_func {
        for (ordinal, name, input) in &arguments {
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
