//! The aggregate vocabulary and the decomposition registry.
//!
//! An aggregate node carries no phase: it declares aggregators over its input and,
//! where it finishes the aggregate, one expression per output column. The split into
//! init / merge / finalize is this table, and adding an aggregate is a row here rather
//! than an arm in C++. State *types* are `state_type`'s — the aggregator that produces
//! each column, which both engines run — and the state *names* are ours, since the golden
//! and every later reference read them; DataFusion's `state_fields()` supplies the arity.

use super::{AggFunc, AggSpec, Decomposition, Merge, PlanAgg};
use datafusion::arrow::datatypes::{DataType, Field};

use super::PlanError;
use super::{BinaryOp, Expr, UnaryOp};

#[cfg(test)]
mod tests;

pub(crate) fn resolve(name: &str) -> Result<AggSpec, PlanError> {
    let (func, ddof) = match name {
        "sum" => (AggFunc::Sum, 0),
        "min" => (AggFunc::Min, 0),
        "max" => (AggFunc::Max, 0),
        "count" => (AggFunc::Count, 0),
        "avg" => (AggFunc::Avg, 0),
        "stddev" | "stddev_samp" => (AggFunc::Stddev, 1),
        "stddev_pop" => (AggFunc::Stddev, 0),
        "var" | "var_samp" | "variance" => (AggFunc::Var, 1),
        "var_pop" => (AggFunc::Var, 0),
        other => {
            return Err(PlanError::Unsupported(format!(
                "aggregate {other}: no decomposition (#161)"
            )));
        }
    };
    Ok(AggSpec { func, ddof })
}

pub(crate) fn decomposition(func: AggFunc) -> Decomposition {
    const WELFORD: Decomposition = Decomposition {
        state: &[
            ("$count", PlanAgg::Count),
            ("$mean", PlanAgg::Mean),
            ("$m2", PlanAgg::M2),
        ],
        merge: Merge::Combined(PlanAgg::MergeM2),
    };
    match func {
        AggFunc::Sum => Decomposition {
            state: &[("", PlanAgg::Sum)],
            merge: Merge::PerColumn(&[PlanAgg::Sum]),
        },
        AggFunc::Min => Decomposition {
            state: &[("", PlanAgg::Min)],
            merge: Merge::PerColumn(&[PlanAgg::Min]),
        },
        AggFunc::Max => Decomposition {
            state: &[("", PlanAgg::Max)],
            merge: Merge::PerColumn(&[PlanAgg::Max]),
        },
        // A count merges by SUM. The one place where naming the merge separately from
        // the init is the difference between a right and a wrong answer.
        AggFunc::Count => Decomposition {
            state: &[("", PlanAgg::Count)],
            merge: Merge::PerColumn(&[PlanAgg::Sum]),
        },
        AggFunc::Avg => Decomposition {
            state: &[("$sum", PlanAgg::Sum), ("$count", PlanAgg::Count)],
            merge: Merge::PerColumn(&[PlanAgg::Sum, PlanAgg::Sum]),
        },
        AggFunc::Stddev | AggFunc::Var => WELFORD,
    }
}

impl PlanAgg {
    /// The type of the state column this aggregator produces, given its argument's type.
    /// Both engines produce it and the plan declares it; no wildcard, so a new aggregator
    /// says its type here or does not compile.
    pub(crate) fn state_type(self, input: &DataType) -> Result<DataType, PlanError> {
        use DataType::*;
        Ok(match self {
            Self::Sum => match input {
                // DataFusion's `sum::return_type`, quoted: Spark's DECIMAL(min(38, p + 10), s).
                Decimal128(p, s) => Decimal128((*p + 10).min(38), *s),
                t if t.is_signed_integer() => Int64,
                t if t.is_unsigned_integer() => UInt64,
                t if t.is_floating() => Float64,
                other => return Err(PlanError::Unsupported(format!("sum over {other}"))),
            },
            Self::Min | Self::Max => input.clone(),
            Self::Count => Int64,
            Self::Mean | Self::M2 => Float64,
            Self::MergeM2 => {
                unreachable!("MergeM2 merges the Welford triple and declares no state")
            }
        })
    }
}

/// The expression that turns merged state into the aggregate's output column. A rename
/// for the five simple aggregates, a divide for `avg`, and a `CASE` over a `sqrt` for
/// the Welford pair — all of them ordinary IR, which is what replaces the hardwired
/// `avg_div` and `std_finalize` arms.
pub(crate) fn finalize(spec: AggSpec, state: &[Field], state_at: u32, out_type: &DataType) -> Expr {
    let column = |offset: usize| Expr::column(state_at + offset as u32, state[offset].name());
    match spec.func {
        AggFunc::Sum | AggFunc::Min | AggFunc::Max | AggFunc::Count => column(0),
        AggFunc::Avg => match out_type {
            // The sum divides at its own scale: arrow's quotient carries four digits more,
            // the scale DataFusion declares, truncated as DataFusion's `avg` and cuDF
            // truncate, and the cast narrows the precision alone. The count as an exact
            // decimal is what puts both operands on the device's pre-scaled divide.
            DataType::Decimal128(p, _) => Expr::Cast {
                expr: Box::new(Expr::binary(
                    column(0),
                    BinaryOp::Divide,
                    Expr::Cast {
                        expr: Box::new(column(1)),
                        target: DataType::Decimal128(*p, 0),
                    },
                    out_type.clone(),
                )),
                target: out_type.clone(),
            },
            other => Expr::binary(
                Expr::Cast {
                    expr: Box::new(column(0)),
                    target: other.clone(),
                },
                BinaryOp::Divide,
                Expr::Cast {
                    expr: Box::new(column(1)),
                    target: other.clone(),
                },
                other.clone(),
            ),
        },
        AggFunc::Stddev | AggFunc::Var => {
            // Every operand in the output's type, the count cast up to it before anything
            // is subtracted: both engines evaluate this expression, and arrow refuses
            // Float64 / Int64 outright rather than widening it.
            let counted = Expr::Cast {
                expr: Box::new(column(0)),
                target: out_type.clone(),
            };
            let ddof = Expr::Literal(number_of(out_type, spec.ddof as f64));
            let denominator = Expr::binary(counted, BinaryOp::Minus, ddof, out_type.clone());
            let quotient = Expr::binary(
                column(2),
                BinaryOp::Divide,
                denominator.clone(),
                out_type.clone(),
            );
            let value = match spec.func {
                AggFunc::Stddev => Expr::unary(UnaryOp::Sqrt, quotient),
                _ => quotient,
            };
            // A group with count <= ddof has no dispersion to report, so it is NULL
            // rather than a division by zero or a root of a negative.
            Expr::Case {
                comparand: None,
                when_then: vec![(
                    Expr::binary(
                        denominator,
                        BinaryOp::LtEq,
                        Expr::Literal(number_of(out_type, 0.0)),
                        DataType::Boolean,
                    ),
                    Expr::Literal(null_of(out_type)),
                )],
                else_expr: Some(Box::new(value)),
            }
        }
    }
}

/// A number in the type an aggregate declares it outputs. Exhaustive on purpose: a
/// dispersion output that is not a float is a decomposition that has changed, and picking
/// a default here would move the arithmetic rather than say so.
fn number_of(data_type: &DataType, value: f64) -> datafusion::common::ScalarValue {
    use datafusion::common::ScalarValue;
    match data_type {
        DataType::Float64 => ScalarValue::Float64(Some(value)),
        DataType::Float32 => ScalarValue::Float32(Some(value as f32)),
        other => panic!("a stddev or variance declared {other}, which no aggregate does"),
    }
}

fn null_of(data_type: &DataType) -> datafusion::common::ScalarValue {
    datafusion::common::ScalarValue::try_from(data_type)
        .expect("an aggregate output type has a null scalar")
}
