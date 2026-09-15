use super::*;
use datafusion::arrow::datatypes::Field;
use datafusion::common::ScalarValue;
use datafusion::functions::string::upper;
use datafusion::physical_expr::expressions::TryCastExpr;

fn schema() -> Schema {
    Schema::new(vec![
        Field::new("a", DataType::Int64, true),
        Field::new("price", DataType::Decimal128(20, 2), true),
        Field::new("s", DataType::Utf8, true),
    ])
}

fn col(name: &str, index: usize) -> Arc<dyn PhysicalExpr> {
    Arc::new(Column::new(name, index))
}

fn lit(value: ScalarValue) -> Arc<dyn PhysicalExpr> {
    Arc::new(Literal::new(value))
}

fn translate(expr: Arc<dyn PhysicalExpr>) -> Result<Expr, PlanError> {
    translate_expr(&expr, &schema())
}

#[test]
fn a_column_keeps_its_ordinal_and_its_name() {
    assert_eq!(translate(col("a", 0)).unwrap(), Expr::column(0, "a"));
}

#[test]
fn a_literal_keeps_datafusions_scalar() {
    let value = ScalarValue::Decimal128(Some(1234), 20, 2);
    assert_eq!(translate(lit(value.clone())).unwrap(), Expr::Literal(value));
}

#[test]
fn a_binary_op_carries_the_type_datafusion_declared() {
    let expr = Arc::new(BinaryExpr::new(
        col("price", 1),
        Operator::Divide,
        lit(ScalarValue::Decimal128(Some(2), 20, 2)),
    ));
    let Expr::Binary { op, out_type, .. } = translate(expr).unwrap() else {
        panic!("expected a binary expression");
    };
    assert_eq!(op, BinaryOp::Divide);
    // cuDF's own rule for a decimal divide gives scale s_l - s_r, which is 0 here;
    // what travels is the scale DataFusion derived.
    assert_eq!(out_type, DataType::Decimal128(26, 6));
}

#[test]
fn the_four_datafusion_unaries_map_one_for_one() {
    let not = Arc::new(NotExpr::new(lit(ScalarValue::Boolean(Some(true)))));
    let is_null = Arc::new(IsNullExpr::new(col("a", 0)));
    let is_not_null = Arc::new(IsNotNullExpr::new(col("a", 0)));
    let negative = Arc::new(NegativeExpr::new(col("a", 0)));

    assert!(matches!(
        translate(not).unwrap(),
        Expr::Unary {
            op: UnaryOp::Not,
            ..
        }
    ));
    assert!(matches!(
        translate(is_null).unwrap(),
        Expr::Unary {
            op: UnaryOp::IsNull,
            ..
        }
    ));
    assert!(matches!(
        translate(is_not_null).unwrap(),
        Expr::Unary {
            op: UnaryOp::IsNotNull,
            ..
        }
    ));
    assert!(matches!(
        translate(negative).unwrap(),
        Expr::Unary {
            op: UnaryOp::Negative,
            ..
        }
    ));
}

#[test]
fn a_cast_keeps_its_target_precision_and_scale() {
    let expr = Arc::new(CastExpr::new(
        col("a", 0),
        DataType::Decimal128(38, 6),
        None,
    ));
    assert_eq!(
        translate(expr).unwrap(),
        Expr::Cast {
            expr: Box::new(Expr::column(0, "a")),
            target: DataType::Decimal128(38, 6),
        }
    );
}

#[test]
fn a_like_keeps_negation_and_case_sensitivity() {
    let expr = Arc::new(LikeExpr::new(
        true,
        true,
        col("s", 2),
        lit(ScalarValue::Utf8(Some("%x%".to_string()))),
    ));
    let Expr::Like {
        negated,
        case_insensitive,
        ..
    } = translate(expr).unwrap()
    else {
        panic!("expected a like expression");
    };
    assert!(negated && case_insensitive);
}

#[test]
fn both_case_forms_translate_and_keep_their_branches() {
    let search = Arc::new(
        CaseExpr::try_new(
            None,
            vec![(
                Arc::new(IsNullExpr::new(col("a", 0))) as Arc<dyn PhysicalExpr>,
                lit(ScalarValue::Int64(Some(0))),
            )],
            Some(col("a", 0)),
        )
        .unwrap(),
    );
    let Expr::Case {
        comparand,
        when_then,
        else_expr,
    } = translate(search).unwrap()
    else {
        panic!("expected a case expression");
    };
    assert!(comparand.is_none() && when_then.len() == 1 && else_expr.is_some());

    let value = Arc::new(
        CaseExpr::try_new(
            Some(col("a", 0)),
            vec![(
                lit(ScalarValue::Int64(Some(1))),
                lit(ScalarValue::Int64(Some(2))),
            )],
            None,
        )
        .unwrap(),
    );
    let Expr::Case { comparand, .. } = translate(value).unwrap() else {
        panic!("expected a case expression");
    };
    assert_eq!(comparand.as_deref(), Some(&Expr::column(0, "a")));
}

#[test]
fn an_in_list_lowers_to_an_or_chain() {
    let expr = Arc::new(InListExpr::new(
        col("a", 0),
        vec![
            lit(ScalarValue::Int64(Some(1))),
            lit(ScalarValue::Int64(Some(2))),
        ],
        false,
        None,
    ));
    let eq = |value: i64| {
        Expr::binary(
            Expr::column(0, "a"),
            BinaryOp::Eq,
            Expr::Literal(ScalarValue::Int64(Some(value))),
            DataType::Boolean,
        )
    };
    assert_eq!(
        translate(expr).unwrap(),
        Expr::binary(eq(1), BinaryOp::Or, eq(2), DataType::Boolean)
    );
}

#[test]
fn a_negated_in_list_wraps_the_chain_in_a_not() {
    let expr = Arc::new(InListExpr::new(
        col("a", 0),
        vec![lit(ScalarValue::Int64(Some(1)))],
        true,
        None,
    ));
    assert!(matches!(
        translate(expr).unwrap(),
        Expr::Unary {
            op: UnaryOp::Not,
            ..
        }
    ));
}

#[test]
fn an_empty_in_list_is_refused_rather_than_lowered_to_nothing() {
    let expr = Arc::new(InListExpr::new(col("a", 0), vec![], false, None));
    assert!(matches!(
        translate(expr).unwrap_err(),
        PlanError::Unsupported(_)
    ));
}

#[test]
fn a_scalar_function_keeps_its_name_and_return_type() {
    let expr = Arc::new(ScalarFunctionExpr::new(
        "upper",
        upper(),
        vec![col("s", 2)],
        DataType::Utf8,
    ));
    let Expr::ScalarFunction {
        name,
        args,
        return_type,
        ..
    } = translate(expr).unwrap()
    else {
        panic!("expected a scalar function");
    };
    assert_eq!(
        (name.as_str(), args.len(), return_type),
        ("upper", 1, DataType::Utf8)
    );
}

#[test]
fn an_unrecognized_expression_kind_is_refused_and_named() {
    let expr = Arc::new(TryCastExpr::new(col("a", 0), DataType::Int32));
    let err = translate(expr).unwrap_err();
    assert!(
        matches!(&err, PlanError::Unsupported(what) if what.contains("TRY_CAST")),
        "{err}"
    );
}

#[test]
fn an_unrecognized_binary_operator_is_refused_and_named() {
    let expr = Arc::new(BinaryExpr::new(
        col("s", 2),
        Operator::RegexMatch,
        lit(ScalarValue::Utf8(Some("^x".to_string()))),
    ));
    let err = translate(expr).unwrap_err();
    assert!(
        matches!(&err, PlanError::Unsupported(what) if what.contains('~')),
        "{err}"
    );
}
