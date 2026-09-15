use super::*;
use datafusion::arrow::datatypes::DataType;
#[test]
fn an_interval_literal_prints_the_parts_that_are_not_zero() {
    use datafusion::arrow::datatypes::IntervalMonthDayNano;
    let ninety_days = Expr::Literal(ScalarValue::IntervalMonthDayNano(Some(
        IntervalMonthDayNano::new(0, 90, 0),
    )));
    // Quoted on the same rule as a name: an interval reads as several tokens, and a
    // plan line is a comma-separated list.
    assert_eq!(expr_text(&ninety_days), "`90 days`");
    let mixed = Expr::Literal(ScalarValue::IntervalMonthDayNano(Some(
        IntervalMonthDayNano::new(2, 1, 500),
    )));
    assert_eq!(expr_text(&mixed), "`2 mons 1 days 500 nanos`");
    let nothing = Expr::Literal(ScalarValue::IntervalMonthDayNano(Some(
        IntervalMonthDayNano::new(0, 0, 0),
    )));
    assert_eq!(expr_text(&nothing), "`0 days`");
}

#[test]
fn a_decimal_literal_prints_as_a_value_rather_than_its_parts() {
    let money = Expr::Literal(ScalarValue::Decimal128(Some(-123_456), 15, 2));
    assert_eq!(expr_text(&money), "-1234.56");
    let whole = Expr::Literal(ScalarValue::Decimal128(Some(7), 15, 0));
    assert_eq!(expr_text(&whole), "7");
}

#[test]
fn every_expression_form_renders_readably() {
    use crate::plan::{BinaryOp, UnaryOp};
    let column = Expr::column(2, "s");
    let cast = Expr::Cast {
        expr: Box::new(Expr::column(0, "a")),
        target: DataType::Decimal128(38, 6),
    };
    assert_eq!(expr_text(&cast), "CAST(a@0 AS Decimal128(38,6))");

    let like = Expr::Like {
        expr: Box::new(column.clone()),
        pattern: Box::new(Expr::Literal(ScalarValue::Utf8(Some("%x%".to_string())))),
        negated: true,
        case_insensitive: false,
    };
    assert_eq!(expr_text(&like), "s@2 NOT LIKE %x%");

    // A nested operator is parenthesized, so precedence is read off the line.
    let nested = Expr::binary(
        Expr::binary(
            Expr::column(0, "a"),
            BinaryOp::Plus,
            Expr::column(1, "b"),
            DataType::Int64,
        ),
        BinaryOp::Gt,
        Expr::Literal(ScalarValue::Int64(Some(3))),
        DataType::Boolean,
    );
    assert_eq!(expr_text(&nested), "(a@0 + b@1) > 3");

    let guard = Expr::Case {
        comparand: None,
        when_then: vec![(
            Expr::unary(UnaryOp::IsNull, Expr::column(0, "a")),
            Expr::Literal(ScalarValue::Int64(None)),
        )],
        else_expr: Some(Box::new(Expr::unary(UnaryOp::Sqrt, Expr::column(0, "a")))),
    };
    assert_eq!(
        expr_text(&guard),
        "CASE WHEN a@0 IS NULL THEN NULL ELSE sqrt(a@0) END"
    );

    let call = Expr::ScalarFunction {
        name: "date_part".to_string(),
        args: vec![
            Expr::Literal(ScalarValue::Utf8(Some("year".to_string()))),
            column,
        ],
        return_type: DataType::Int32,
        nullable: true,
    };
    assert_eq!(expr_text(&call), "date_part(year, s@2)");
}
