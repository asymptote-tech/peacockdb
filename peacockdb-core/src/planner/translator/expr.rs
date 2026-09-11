//! DataFusion `PhysicalExpr` → the mode's [`Expr`].
//!
//! One conscious decision per expression kind; an unrecognized kind is a plan-time error
//! naming it, never a silent pass-through. Types are read back off DataFusion — a binary
//! op's declared output type in particular, since cuDF's own fixed-point result scale
//! differs from DataFusion's.

use std::sync::Arc;

use datafusion::arrow::datatypes::{DataType, Schema};
use datafusion::logical_expr::Operator;
use datafusion::physical_expr::ScalarFunctionExpr;
use datafusion::physical_expr::expressions::{
    BinaryExpr, CaseExpr, CastExpr, Column, InListExpr, IsNotNullExpr, IsNullExpr, LikeExpr,
    Literal, NegativeExpr, NotExpr,
};
use datafusion::physical_plan::PhysicalExpr;

use crate::plan::PlanError;
use crate::plan::{BinaryOp, ColumnRef, Expr, UnaryOp};

pub(crate) fn translate_expr(
    expr: &Arc<dyn PhysicalExpr>,
    input_schema: &Schema,
) -> Result<Expr, PlanError> {
    let any = expr.as_any();

    if let Some(col) = any.downcast_ref::<Column>() {
        return Ok(Expr::Column(ColumnRef {
            index: col.index() as u32,
            name: col.name().to_string(),
        }));
    }
    if let Some(lit) = any.downcast_ref::<Literal>() {
        return Ok(Expr::Literal(lit.value().clone()));
    }
    if let Some(bin) = any.downcast_ref::<BinaryExpr>() {
        let out_type = bin.data_type(input_schema).map_err(|e| {
            PlanError::Invalid(format!("binary expression has no output type: {e}"))
        })?;
        return Ok(Expr::binary(
            translate_expr(bin.left(), input_schema)?,
            translate_operator(bin.op())?,
            translate_expr(bin.right(), input_schema)?,
            out_type,
        ));
    }
    if let Some(not) = any.downcast_ref::<NotExpr>() {
        return Ok(Expr::unary(
            UnaryOp::Not,
            translate_expr(not.arg(), input_schema)?,
        ));
    }
    if let Some(is_null) = any.downcast_ref::<IsNullExpr>() {
        return Ok(Expr::unary(
            UnaryOp::IsNull,
            translate_expr(is_null.arg(), input_schema)?,
        ));
    }
    if let Some(is_not_null) = any.downcast_ref::<IsNotNullExpr>() {
        return Ok(Expr::unary(
            UnaryOp::IsNotNull,
            translate_expr(is_not_null.arg(), input_schema)?,
        ));
    }
    if let Some(neg) = any.downcast_ref::<NegativeExpr>() {
        return Ok(Expr::unary(
            UnaryOp::Negative,
            translate_expr(neg.arg(), input_schema)?,
        ));
    }
    if let Some(cast) = any.downcast_ref::<CastExpr>() {
        return Ok(Expr::Cast {
            expr: Box::new(translate_expr(cast.expr(), input_schema)?),
            target: cast.cast_type().clone(),
        });
    }
    if let Some(like) = any.downcast_ref::<LikeExpr>() {
        return Ok(Expr::Like {
            expr: Box::new(translate_expr(like.expr(), input_schema)?),
            pattern: Box::new(translate_expr(like.pattern(), input_schema)?),
            negated: like.negated(),
            case_insensitive: like.case_insensitive(),
        });
    }
    if let Some(case) = any.downcast_ref::<CaseExpr>() {
        let comparand = match case.expr() {
            Some(e) => Some(Box::new(translate_expr(e, input_schema)?)),
            None => None,
        };
        let mut when_then = Vec::with_capacity(case.when_then_expr().len());
        for (when, then) in case.when_then_expr() {
            when_then.push((
                translate_expr(when, input_schema)?,
                translate_expr(then, input_schema)?,
            ));
        }
        let else_expr = match case.else_expr() {
            Some(e) => Some(Box::new(translate_expr(e, input_schema)?)),
            None => None,
        };
        return Ok(Expr::Case {
            comparand,
            when_then,
            else_expr,
        });
    }
    if let Some(in_list) = any.downcast_ref::<InListExpr>() {
        return expand_in_list(in_list, input_schema);
    }
    if let Some(func) = any.downcast_ref::<ScalarFunctionExpr>() {
        let mut args = Vec::with_capacity(func.args().len());
        for arg in func.args() {
            args.push(translate_expr(arg, input_schema)?);
        }
        return Ok(Expr::ScalarFunction {
            name: func.name().to_string(),
            args,
            return_type: func.return_type().clone(),
            nullable: func.nullable(),
        });
    }

    Err(PlanError::Unsupported(format!("expression {expr} (#162)")))
}

/// `x IN (a, b)` becomes `(x = a) OR (x = b)`, and `NOT IN` its negation: cuDF's AST has
/// no IN opcode, so the lowering has to happen somewhere and the IR is where this mode
/// can state it. Legacy lowers the same shape in its wrapper rule.
fn expand_in_list(in_list: &InListExpr, input_schema: &Schema) -> Result<Expr, PlanError> {
    if in_list.list().is_empty() {
        return Err(PlanError::Unsupported("IN with an empty list".to_string()));
    }
    let target = translate_expr(in_list.expr(), input_schema)?;
    let mut chain: Option<Expr> = None;
    for item in in_list.list() {
        let eq = Expr::binary(
            target.clone(),
            BinaryOp::Eq,
            translate_expr(item, input_schema)?,
            DataType::Boolean,
        );
        chain = Some(match chain {
            None => eq,
            Some(acc) => Expr::binary(acc, BinaryOp::Or, eq, DataType::Boolean),
        });
    }
    let chain = chain.expect("the list is not empty");
    Ok(if in_list.negated() {
        Expr::unary(UnaryOp::Not, chain)
    } else {
        chain
    })
}

fn translate_operator(op: &Operator) -> Result<BinaryOp, PlanError> {
    Ok(match op {
        Operator::Eq => BinaryOp::Eq,
        Operator::NotEq => BinaryOp::NotEq,
        Operator::Lt => BinaryOp::Lt,
        Operator::LtEq => BinaryOp::LtEq,
        Operator::Gt => BinaryOp::Gt,
        Operator::GtEq => BinaryOp::GtEq,
        Operator::Plus => BinaryOp::Plus,
        Operator::Minus => BinaryOp::Minus,
        Operator::Multiply => BinaryOp::Multiply,
        Operator::Divide => BinaryOp::Divide,
        Operator::Modulo => BinaryOp::Modulo,
        Operator::And => BinaryOp::And,
        Operator::Or => BinaryOp::Or,
        Operator::BitwiseAnd => BinaryOp::BitwiseAnd,
        Operator::BitwiseOr => BinaryOp::BitwiseOr,
        Operator::BitwiseXor => BinaryOp::BitwiseXor,
        Operator::BitwiseShiftLeft => BinaryOp::BitwiseShiftLeft,
        Operator::BitwiseShiftRight => BinaryOp::BitwiseShiftRight,
        Operator::StringConcat => BinaryOp::StringConcat,
        Operator::IsDistinctFrom => BinaryOp::IsDistinctFrom,
        Operator::IsNotDistinctFrom => BinaryOp::IsNotDistinctFrom,
        other => return Err(PlanError::Unsupported(format!("binary operator {other} (#162)"))),
    })
}

#[cfg(test)]
mod tests;
