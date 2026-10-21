//! The engine's [`Expr`] into the wire's `fb::Expr`.
//!
//! Every variant writes, `Sqrt` included: the schema gained it so that a finalizing
//! aggregate carries its own finalize rather than leaving the arithmetic to the C++.

use datafusion::arrow::datatypes::DataType;
use datafusion::common::ScalarValue;
use flatbuffers::{FlatBufferBuilder, WIPOffset};

use super::generated::peacock::plan as fb;
use super::serialize::{convert_data_type, serialize_scalar_value};

use crate::plan::PlanError;
use crate::plan::{BinaryOp, Expr, UnaryOp};

/// Write one expression, children first, as FlatBuffers requires.
pub(crate) fn write_expr<'a>(
    b: &mut FlatBufferBuilder<'a>,
    expr: &Expr,
) -> Result<WIPOffset<fb::Expr<'a>>, PlanError> {
    let (node_type, node) = match expr {
        Expr::Column(reference) => {
            let name = b.create_string(&reference.name);
            let column = fb::ColumnRef::create(
                b,
                &fb::ColumnRefArgs {
                    index: reference.index,
                    name: Some(name),
                },
            );
            (fb::ExprNode::ColumnRef, column.as_union_value())
        }
        // An interval reaches the wire only as the operand the Binary arm writes itself: the
        // device has no other place to put one, inside a CASE or as a function's argument.
        Expr::Literal(value) if is_interval(value) => {
            return Err(PlanError::Unsupported(
                "an interval literal outside a date or naive timestamp ± interval (#249)"
                    .to_string(),
            ));
        }
        Expr::Literal(value) => return write_literal(b, value),
        Expr::Binary {
            left,
            op,
            right,
            out_type,
        } => {
            let shifts = shifts_a_date_by_a_literal(left, *op, right, out_type)?;
            let left = write_operand(b, left, shifts)?;
            let right = write_operand(b, right, shifts)?;
            // The declared decimal output travels with the op: cuDF derives its own
            // fixed-point result scale, division most visibly, and the C++ reproduces
            // DataFusion's from these two rather than re-deriving the coercion.
            let (out_decimal_precision, out_decimal_scale) = decimal_parts(out_type);
            let binary = fb::BinaryExprNode::create(
                b,
                &fb::BinaryExprNodeArgs {
                    left: Some(left),
                    op: binary_op(*op),
                    right: Some(right),
                    out_decimal_precision,
                    out_decimal_scale,
                },
            );
            (fb::ExprNode::BinaryExprNode, binary.as_union_value())
        }
        Expr::Unary { op, arg } => {
            let arg = write_expr(b, arg)?;
            let unary = fb::UnaryExprNode::create(
                b,
                &fb::UnaryExprNodeArgs {
                    op: unary_op(*op),
                    arg: Some(arg),
                },
            );
            (fb::ExprNode::UnaryExprNode, unary.as_union_value())
        }
        Expr::Cast { expr, target } => {
            let inner = write_expr(b, expr)?;
            let (decimal_precision, decimal_scale) = decimal_parts(target);
            let cast = fb::CastExprNode::create(
                b,
                &fb::CastExprNodeArgs {
                    expr: Some(inner),
                    target_type: data_type(target)?,
                    decimal_precision,
                    decimal_scale,
                },
            );
            (fb::ExprNode::CastExprNode, cast.as_union_value())
        }
        Expr::Like {
            expr,
            pattern,
            negated,
            case_insensitive,
        } => {
            let inner = write_expr(b, expr)?;
            let pattern = write_expr(b, pattern)?;
            let like = fb::LikeExprNode::create(
                b,
                &fb::LikeExprNodeArgs {
                    expr: Some(inner),
                    pattern: Some(pattern),
                    negated: *negated,
                    case_insensitive: *case_insensitive,
                },
            );
            (fb::ExprNode::LikeExprNode, like.as_union_value())
        }
        Expr::Case {
            comparand,
            when_then,
            else_expr,
        } => {
            let comparand = comparand
                .as_ref()
                .map(|expr| write_expr(b, expr))
                .transpose()?;
            let mut pairs = Vec::with_capacity(when_then.len());
            for (when, then) in when_then {
                let when = write_expr(b, when)?;
                let then = write_expr(b, then)?;
                pairs.push(fb::CaseWhenThen::create(
                    b,
                    &fb::CaseWhenThenArgs {
                        when: Some(when),
                        then: Some(then),
                    },
                ));
            }
            let pairs = b.create_vector(&pairs);
            let otherwise = else_expr
                .as_ref()
                .map(|expr| write_expr(b, expr))
                .transpose()?;
            let case = fb::CaseExprNode::create(
                b,
                &fb::CaseExprNodeArgs {
                    expr: comparand,
                    when_thens: Some(pairs),
                    else_expr: otherwise,
                },
            );
            (fb::ExprNode::CaseExprNode, case.as_union_value())
        }
        Expr::ScalarFunction {
            name,
            args,
            return_type,
            nullable,
        } => {
            let mut written = Vec::with_capacity(args.len());
            for arg in args {
                written.push(write_expr(b, arg)?);
            }
            let args = b.create_vector(&written);
            let name = b.create_string(name);
            let (return_decimal_precision, return_decimal_scale) = decimal_parts(return_type);
            let function = fb::ScalarFunctionExprNode::create(
                b,
                &fb::ScalarFunctionExprNodeArgs {
                    name: Some(name),
                    args: Some(args),
                    return_type: data_type(return_type)?,
                    return_decimal_precision,
                    return_decimal_scale,
                    nullable: *nullable,
                },
            );
            (
                fb::ExprNode::ScalarFunctionExprNode,
                function.as_union_value(),
            )
        }
    };
    Ok(fb::Expr::create(
        b,
        &fb::ExprArgs {
            node_type,
            node: Some(node),
        },
    ))
}

/// A literal as a whole wire expression. The ticket rides a refusal because it is what a reader
/// of a golden gets: the wire still has no Time, Duration, Struct or List literal (#249).
fn write_literal<'a>(
    b: &mut FlatBufferBuilder<'a>,
    value: &ScalarValue,
) -> Result<WIPOffset<fb::Expr<'a>>, PlanError> {
    let scalar = serialize_scalar_value(b, value)
        .map_err(|why| PlanError::Unsupported(format!("{why} (#249)")))?;
    let literal = fb::LiteralExpr::create(
        b,
        &fb::LiteralExprArgs {
            value: Some(scalar),
        },
    );
    Ok(fb::Expr::create(
        b,
        &fb::ExprArgs {
            node_type: fb::ExprNode::LiteralExpr,
            node: Some(literal.as_union_value()),
        },
    ))
}

fn is_interval(value: &ScalarValue) -> bool {
    matches!(value.data_type(), DataType::Interval(_))
}

/// A binary's operand: an interval literal is written here when `shifts` accepted it, since
/// `write_expr` refuses one standing anywhere else.
fn write_operand<'a>(
    b: &mut FlatBufferBuilder<'a>,
    operand: &Expr,
    shifts: bool,
) -> Result<WIPOffset<fb::Expr<'a>>, PlanError> {
    match operand {
        Expr::Literal(value) if shifts && is_interval(value) => write_literal(b, value),
        _ => write_expr(b, operand),
    }
}

/// Whether this binary shifts a date or a naive timestamp by an interval literal operand, the
/// one interval shape the device runs (`add_interval`, `cpp/src/interval.cpp`); `false` where no
/// operand is an interval literal. Any other binary over one is refused, naming #249: a zoned
/// timestamp because arrow adds months and days in its zone and the device has none; a `Date64`
/// because the device holds it as milliseconds and would floor a sub-day part DataFusion drops.
fn shifts_a_date_by_a_literal(
    left: &Expr,
    op: BinaryOp,
    right: &Expr,
    out_type: &DataType,
) -> Result<bool, PlanError> {
    let literal_interval = |e: &Expr| matches!(e, Expr::Literal(v) if is_interval(v));
    let (on_left, on_right) = (literal_interval(left), literal_interval(right));
    if !on_left && !on_right {
        return Ok(false);
    }
    let shifts_a_date = matches!(out_type, DataType::Date32 | DataType::Timestamp(_, None))
        && (op == BinaryOp::Plus || (op == BinaryOp::Minus && !on_left));
    if shifts_a_date {
        return Ok(true);
    }
    Err(PlanError::Unsupported(format!(
        "an interval literal under {op:?} answering {out_type:?}: the device runs only a date or \
         naive timestamp ± interval (#249)"
    )))
}

/// Precision and scale, or `(0, 0)`, which the wire reads as "not a decimal".
fn decimal_parts(data_type: &DataType) -> (u8, i8) {
    match data_type {
        DataType::Decimal128(precision, scale) => (*precision, *scale),
        _ => (0, 0),
    }
}

/// The fb type for an arrow one, or a refusal naming the ticket that will add it.
///
/// The ticket is not decoration. A plan the engine runs on the cpu and cannot carry to the
/// device renders a `not runnable` line in its plan golden, and
/// `every_refusal_names_a_ticket_that_exists` requires every one of those to cite a ticket in
/// parentheses — so a type with no word on the wire has to say who is going to give it one.
/// Everything still unnamed waits on #249.
fn data_type(data_type: &DataType) -> Result<fb::DataType, PlanError> {
    convert_data_type(data_type).map_err(|why| PlanError::Unsupported(format!("{why} (#249)")))
}

fn binary_op(op: BinaryOp) -> fb::BinaryOp {
    match op {
        BinaryOp::Eq => fb::BinaryOp::Eq,
        BinaryOp::NotEq => fb::BinaryOp::NotEq,
        BinaryOp::Lt => fb::BinaryOp::Lt,
        BinaryOp::LtEq => fb::BinaryOp::LtEq,
        BinaryOp::Gt => fb::BinaryOp::Gt,
        BinaryOp::GtEq => fb::BinaryOp::GtEq,
        BinaryOp::Plus => fb::BinaryOp::Plus,
        BinaryOp::Minus => fb::BinaryOp::Minus,
        BinaryOp::Multiply => fb::BinaryOp::Multiply,
        BinaryOp::Divide => fb::BinaryOp::Divide,
        BinaryOp::Modulo => fb::BinaryOp::Modulo,
        BinaryOp::And => fb::BinaryOp::And,
        BinaryOp::Or => fb::BinaryOp::Or,
        BinaryOp::BitwiseAnd => fb::BinaryOp::BitwiseAnd,
        BinaryOp::BitwiseOr => fb::BinaryOp::BitwiseOr,
        BinaryOp::BitwiseXor => fb::BinaryOp::BitwiseXor,
        BinaryOp::BitwiseShiftLeft => fb::BinaryOp::BitwiseShiftLeft,
        BinaryOp::BitwiseShiftRight => fb::BinaryOp::BitwiseShiftRight,
        BinaryOp::StringConcat => fb::BinaryOp::StringConcat,
        BinaryOp::IsDistinctFrom => fb::BinaryOp::IsDistinctFrom,
        BinaryOp::IsNotDistinctFrom => fb::BinaryOp::IsNotDistinctFrom,
    }
}

fn unary_op(op: UnaryOp) -> fb::UnaryOp {
    match op {
        UnaryOp::Not => fb::UnaryOp::Not,
        UnaryOp::IsNull => fb::UnaryOp::IsNull,
        UnaryOp::IsNotNull => fb::UnaryOp::IsNotNull,
        UnaryOp::Negative => fb::UnaryOp::Negative,
        UnaryOp::Sqrt => fb::UnaryOp::Sqrt,
    }
}

#[cfg(test)]
mod tests;
