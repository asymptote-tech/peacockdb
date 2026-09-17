//! `state_type` against the accumulators that produce the state: the table says what type
//! each aggregator's state column has, and the cpu's DataFusion accumulator is run to show
//! it produces that type. The decomposition walk is what proves `MergeM2` is never asked.

use super::*;
use datafusion::arrow::array::{ArrayRef, Int64Array, RecordBatch};
use datafusion::arrow::compute::cast;
use datafusion::arrow::datatypes::DataType::*;
use datafusion::arrow::datatypes::Schema as ArrowSchema;
use datafusion::common::ScalarValue;
use datafusion::execution::context::SessionContext;
use datafusion::logical_expr::AggregateUDF;
use datafusion::logical_expr::type_coercion::functions::data_types_with_aggregate_udf;
use datafusion::physical_expr::aggregate::AggregateExprBuilder;
use datafusion::physical_expr::expressions::Column;
use std::sync::Arc;

#[test]
fn a_count_state_is_int64_whatever_it_counts() {
    for input in [Int32, Utf8, Decimal128(15, 2), Date32, Null] {
        assert_eq!(
            PlanAgg::Count.state_type(&input).unwrap(),
            Int64,
            "count over {input}"
        );
    }
}

/// DataFusion's `sum::return_type` after its coercion: a decimal gains ten digits of
/// precision up to 38, integers widen to 64 bits keeping their sign, floats to `Float64`.
#[test]
fn a_sum_state_is_datafusions_sum_type() {
    assert_eq!(
        PlanAgg::Sum.state_type(&Decimal128(15, 2)).unwrap(),
        Decimal128(25, 2)
    );
    assert_eq!(
        PlanAgg::Sum.state_type(&Decimal128(30, 6)).unwrap(),
        Decimal128(38, 6)
    );
    assert_eq!(PlanAgg::Sum.state_type(&Int32).unwrap(), Int64);
    assert_eq!(PlanAgg::Sum.state_type(&Int64).unwrap(), Int64);
    assert_eq!(PlanAgg::Sum.state_type(&UInt16).unwrap(), UInt64);
    assert_eq!(PlanAgg::Sum.state_type(&Float32).unwrap(), Float64);
    assert_eq!(PlanAgg::Sum.state_type(&Float64).unwrap(), Float64);
}

#[test]
fn a_sum_over_a_string_is_refused() {
    let refused = PlanAgg::Sum.state_type(&Utf8).unwrap_err();
    assert_eq!(refused, PlanError::Unsupported("sum over Utf8".to_string()));
}

#[test]
fn min_and_max_keep_the_input_type() {
    for input in [Utf8, Date32, Decimal128(15, 2), Int32, Float64] {
        assert_eq!(
            PlanAgg::Min.state_type(&input).unwrap(),
            input,
            "min over {input}"
        );
        assert_eq!(
            PlanAgg::Max.state_type(&input).unwrap(),
            input,
            "max over {input}"
        );
    }
}

#[test]
fn the_welford_moments_are_float64() {
    for input in [Int32, Float64, Decimal128(15, 2)] {
        assert_eq!(PlanAgg::Mean.state_type(&input).unwrap(), Float64);
        assert_eq!(PlanAgg::M2.state_type(&input).unwrap(), Float64);
    }
}

/// Every `state` slice and every `PerColumn` list of every decomposition, typed. `MergeM2`
/// sits in neither — it is a `Combined` merge rule — so this walk is what makes its
/// `unreachable!` a checked fact rather than a convention.
#[test]
fn every_decomposition_types_every_state_it_names() {
    let every_func = [
        AggFunc::Sum,
        AggFunc::Min,
        AggFunc::Max,
        AggFunc::Count,
        AggFunc::Avg,
        AggFunc::Stddev,
        AggFunc::Var,
    ];
    for func in every_func {
        let rule = decomposition(func);
        for (suffix, agg) in rule.state {
            agg.state_type(&Decimal128(15, 2))
                .unwrap_or_else(|why| panic!("{func:?}{suffix}: {why:?}"));
        }
        if let Merge::PerColumn(funcs) = rule.merge {
            for agg in funcs {
                agg.state_type(&Decimal128(15, 2))
                    .unwrap_or_else(|why| panic!("{func:?}'s merge by {agg:?}: {why:?}"));
            }
        }
    }
}

/// Four rows of `input`, cast to what the aggregate coerces its argument to — the cast the
/// logical planner inserts before this engine sees the aggregate.
fn four_rows(udaf: &AggregateUDF, input: &DataType) -> (Arc<ArrowSchema>, ArrayRef) {
    let ints: ArrayRef = Arc::new(Int64Array::from(vec![Some(2), None, Some(4), Some(6)]));
    let coerced = data_types_with_aggregate_udf(std::slice::from_ref(input), udaf)
        .expect("the argument coerces")[0]
        .clone();
    let column = cast(
        &cast(&ints, input).expect("the ints cast to the input"),
        &coerced,
    )
    .expect("the input casts to what the aggregate takes");
    let schema = Arc::new(ArrowSchema::new(vec![Field::new("x", coerced, true)]));
    (schema, column)
}

/// The state DataFusion's `name` accumulator produces over four rows of `input`, as the
/// cpu produces it: one type per state column.
fn produced_state(name: &str, input: &DataType) -> Vec<DataType> {
    let udaf = SessionContext::new()
        .state()
        .aggregate_functions()
        .get(name)
        .cloned()
        .unwrap_or_else(|| panic!("`{name}` is a DataFusion aggregate"));
    let (schema, column) = four_rows(&udaf, input);
    let batch = RecordBatch::try_new(schema.clone(), vec![column]).expect("one column");
    let mut accumulator = AggregateExprBuilder::new(udaf, vec![Arc::new(Column::new("x", 0))])
        .schema(schema)
        .alias(name)
        .build()
        .expect("the aggregate builds")
        .create_accumulator()
        .expect("and has an accumulator");
    accumulator
        .update_batch(batch.columns())
        .expect("four rows accumulate");
    accumulator
        .state()
        .expect("state")
        .iter()
        .map(ScalarValue::data_type)
        .collect()
}

/// The producer half of the table: for each arm, the cpu accumulator behind it produces
/// the type `state_type` declares. `Count` is `count`'s — the one producer whose count is
/// not, the Welford triple's, is cast at the init (`cpu_backend/mod.rs`).
#[test]
fn every_arm_declares_the_type_its_cpu_accumulator_produces() {
    let producers: [(PlanAgg, &str, usize); 6] = [
        (PlanAgg::Sum, "sum", 0),
        (PlanAgg::Min, "min", 0),
        (PlanAgg::Max, "max", 0),
        (PlanAgg::Count, "count", 0),
        (PlanAgg::Mean, "stddev", 1),
        (PlanAgg::M2, "stddev", 2),
    ];
    let inputs = [Int32, Int64, UInt16, Float32, Float64, Decimal128(15, 2)];
    for (arm, name, column) in producers {
        for input in &inputs {
            let declared = arm.state_type(input).unwrap();
            let produced = produced_state(name, input);
            assert_eq!(
                produced[column], declared,
                "{arm:?} over {input}: `{name}` produces {produced:?}"
            );
        }
    }
}

fn avg_state(sum: DataType) -> Vec<Field> {
    vec![
        Field::new("avg(x)$sum", sum, true),
        Field::new("avg(x)$count", Int64, true),
    ]
}

/// Arrow's decimal divide truncates at the numerator's scale plus four, which is the scale
/// DataFusion declares for an `avg` — so the sum divides at its own scale, and the cast
/// the finalize asks for narrows the precision only. A numerator cast up to the output's
/// scale first would land four digits past it, and the cast back rounds where DataFusion's
/// own `avg` and cuDF truncate: one digit off in the last place, on the cpu alone.
#[test]
fn a_decimal_avg_finalize_divides_the_sum_at_its_own_scale_and_casts_to_the_declared_output() {
    let out = Decimal128(22, 6);
    let spec = AggSpec {
        func: AggFunc::Avg,
        ddof: 0,
    };
    let expr = finalize(spec, &avg_state(Decimal128(28, 2)), 0, &out);
    let Expr::Cast {
        expr: divide,
        target,
    } = expr
    else {
        panic!("the finalize is a cast over the divide: {expr:?}");
    };
    assert_eq!(target, out);
    let Expr::Binary {
        left,
        op: BinaryOp::Divide,
        right,
        out_type,
    } = divide.as_ref()
    else {
        panic!("{divide:?}");
    };
    assert_eq!(out_type, &out);
    assert_eq!(
        left.as_ref(),
        &Expr::column(0, "avg(x)$sum"),
        "the sum, uncast"
    );
    assert_eq!(
        right.as_ref(),
        &Expr::Cast {
            expr: Box::new(Expr::column(1, "avg(x)$count")),
            target: Decimal128(22, 0),
        },
        "the count as an exact decimal of the output's precision"
    );
}

/// A float divide over float operands is already the declared type; nothing to ask for.
#[test]
fn a_float_avg_finalize_is_the_bare_divide() {
    let spec = AggSpec {
        func: AggFunc::Avg,
        ddof: 0,
    };
    let expr = finalize(spec, &avg_state(Float64), 0, &Float64);
    assert!(
        matches!(
            expr,
            Expr::Binary {
                op: BinaryOp::Divide,
                ..
            }
        ),
        "{expr:?}"
    );
}
