//! What a `GpuFilter`, a `GpuProject` and a `GpuSort` hand the device up, read at the
//! handle: every case runs the node on the device alone and holds each output handle to
//! the node's declared schema under cuDF's projection (`test_support::device_divergence`).
//! The node and script builders are `exec_cases.rs`'s. Every case is green or a `bug_` test
//! with its ticket above it; nothing here repairs.

use datafusion::arrow::datatypes::DataType;
use datafusion::common::ScalarValue;

use super::exec_cases::{
    by, cast_to, declared_decimal_types, filter, function, gt, keep_id, like, lit_i32, lit_i64,
    lit_str, project, project_over, sort,
};
use super::script::{Script, assert_holds_as_declared, divergences_on_device};
use crate::plan::{BinaryOp, Expr, UnaryOp};
use crate::tests::synthetic::{decimals, synthetic};

fn input() -> Vec<datafusion::arrow::record_batch::RecordBatch> {
    vec![synthetic(64, 1)]
}

// `GpuFilter`.

operator_case! {
    GpuFilter,
    fn a_filter_declares_the_input_schema_the_device_holds() {
        assert_holds_as_declared(&filter(gt(2, "i32", lit_i32(0)), None), Script::Exec(input()));
    }
}

operator_case! {
    GpuFilter,
    fn a_filter_with_a_projection_declares_the_kept_columns_the_device_holds() {
        let node = filter(gt(0, "id", lit_i64(10)), Some(vec![0, 5, 7]));
        assert_holds_as_declared(&node, Script::Exec(input()));
    }
}

// `GpuProject`: arithmetic on each numeric type.

fn arithmetic(ordinal: u32, name: &str, op: BinaryOp, rhs: Expr, ty: DataType) -> Expr {
    Expr::binary(Expr::column(ordinal, name), op, rhs, ty)
}

operator_case! {
    GpuProject,
    fn a_plus_over_int32_declares_the_int32_the_device_holds() {
        let sum = arithmetic(2, "i32", BinaryOp::Plus, lit_i32(7), DataType::Int32);
        let node = project(vec![keep_id(), (sum, "sum", DataType::Int32)]);
        assert_holds_as_declared(&node, Script::Exec(input()));
    }
}

operator_case! {
    GpuProject,
    fn a_multiply_over_int64_declares_the_int64_the_device_holds() {
        let product = arithmetic(3, "i64", BinaryOp::Multiply, lit_i64(3), DataType::Int64);
        let node = project(vec![keep_id(), (product, "product", DataType::Int64)]);
        assert_holds_as_declared(&node, Script::Exec(input()));
    }
}

operator_case! {
    GpuProject,
    fn a_multiply_over_float64_declares_the_float64_the_device_holds() {
        let two = Expr::Literal(ScalarValue::Float64(Some(2.0)));
        let scaled = arithmetic(4, "f64", BinaryOp::Multiply, two, DataType::Float64);
        let node = project(vec![keep_id(), (scaled, "scaled", DataType::Float64)]);
        assert_holds_as_declared(&node, Script::Exec(input()));
    }
}

operator_case! {
    GpuProject,
    fn a_plus_over_a_decimal_declares_the_scale_2_the_device_holds() {
        let dec = decimals(64, 1);
        let (add_type, _) = declared_decimal_types();
        let doubled = arithmetic(1, "dec", BinaryOp::Plus, Expr::column(1, "dec"), add_type.clone());
        let node = project_over(dec.schema(), vec![keep_id(), (doubled, "doubled", add_type)]);
        assert_holds_as_declared(&node, Script::Exec(vec![dec]));
    }
}

// The divide's declared scale is the sum's plus four: the device's pre-scaled divide lands
// there rather than at cuDF's own quotient scale.
operator_case! {
    GpuProject,
    fn a_divide_over_a_decimal_declares_the_scale_6_the_device_holds() {
        let dec = decimals(64, 1);
        let (_, divide_type) = declared_decimal_types();
        let two = Expr::Literal(ScalarValue::Decimal128(Some(200), 3, 2));
        let halved = arithmetic(1, "dec", BinaryOp::Divide, two, divide_type.clone());
        let node = project_over(dec.schema(), vec![keep_id(), (halved, "halved", divide_type)]);
        assert_holds_as_declared(&node, Script::Exec(vec![dec]));
    }
}

operator_case! {
    GpuProject,
    fn a_minus_and_a_modulo_over_int64_declare_the_int64_the_device_holds() {
        let difference = arithmetic(3, "i64", BinaryOp::Minus, Expr::column(0, "id"), DataType::Int64);
        let remainder = arithmetic(3, "i64", BinaryOp::Modulo, lit_i64(7), DataType::Int64);
        let node = project(vec![
            keep_id(),
            (difference, "difference", DataType::Int64),
            (remainder, "remainder", DataType::Int64),
        ]);
        assert_holds_as_declared(&node, Script::Exec(input()));
    }
}

// The casts the corpus emits.

operator_case! {
    GpuProject,
    fn a_cast_from_int32_to_int64_declares_the_int64_the_device_holds() {
        let widened = cast_to(2, "i32", DataType::Int64);
        let node = project(vec![keep_id(), (widened, "widened", DataType::Int64)]);
        assert_holds_as_declared(&node, Script::Exec(input()));
    }
}

operator_case! {
    GpuProject,
    fn a_cast_from_int64_to_a_decimal_declares_the_scale_0_the_device_holds() {
        let declared = DataType::Decimal128(20, 0);
        let node = project(vec![keep_id(), (cast_to(3, "i64", declared.clone()), "as_decimal", declared)]);
        assert_holds_as_declared(&node, Script::Exec(input()));
    }
}

operator_case! {
    GpuProject,
    fn a_cast_from_a_decimal_to_float64_declares_the_float64_the_device_holds() {
        let dec = decimals(64, 1);
        let node = project_over(
            dec.schema(),
            vec![keep_id(), (cast_to(1, "dec", DataType::Float64), "as_f64", DataType::Float64)],
        );
        assert_holds_as_declared(&node, Script::Exec(vec![dec]));
    }
}

operator_case! {
    GpuProject,
    fn a_cast_from_a_decimal_to_a_wider_decimal_declares_the_scale_the_device_holds() {
        let dec = decimals(64, 1);
        let declared = DataType::Decimal128(28, 4);
        let node = project_over(
            dec.schema(),
            vec![keep_id(), (cast_to(1, "dec", declared.clone()), "rescaled", declared)],
        );
        assert_holds_as_declared(&node, Script::Exec(vec![dec]));
    }
}

// The union's two branches (`union.cpp`): one column declared as the union's decimal on
// both sides, held by its own type on one and by an `Int64` literal cast on the other. A
// union is a forwarder with no executor, so each branch's cast project is checked alone and
// the concatenation is the corpus's to prove.

operator_case! {
    GpuProject,
    fn a_union_branch_keeping_its_decimal_declares_the_scale_2_the_device_holds() {
        let dec = decimals(64, 1);
        let unions = DataType::Decimal128(18, 2);
        let node = project_over(dec.schema(), vec![keep_id(), (Expr::column(1, "dec"), "amount", unions)]);
        assert_holds_as_declared(&node, Script::Exec(vec![dec]));
    }
}

operator_case! {
    GpuProject,
    fn a_union_branch_casting_an_int64_literal_to_the_decimal_declares_the_scale_2_the_device_holds() {
        let unions = DataType::Decimal128(18, 2);
        let zero = Expr::Cast {
            expr: Box::new(Expr::Literal(ScalarValue::Int64(Some(0)))),
            target: unions.clone(),
        };
        let node = project(vec![keep_id(), (zero, "amount", unions)]);
        assert_holds_as_declared(&node, Script::Exec(input()));
    }
}

// Each scalar function the dispatch admits (`expr.cpp`, `build_column_scalar_fn`), the
// unary forms, CASE and LIKE.

operator_case! {
    GpuProject,
    fn an_upper_over_utf8_declares_the_string_the_device_holds() {
        let loud = function("upper", vec![Expr::column(5, "s")], DataType::Utf8);
        let node = project(vec![keep_id(), (loud, "loud", DataType::Utf8)]);
        assert_holds_as_declared(&node, Script::Exec(input()));
    }
}

operator_case! {
    GpuProject,
    fn a_lower_over_utf8_declares_the_string_the_device_holds() {
        let quiet = function("lower", vec![Expr::column(5, "s")], DataType::Utf8);
        let node = project(vec![keep_id(), (quiet, "quiet", DataType::Utf8)]);
        assert_holds_as_declared(&node, Script::Exec(input()));
    }
}

operator_case! {
    GpuProject,
    fn a_substr_over_utf8_declares_the_string_the_device_holds() {
        let sub = function("substr", vec![Expr::column(5, "s"), lit_i64(2), lit_i64(3)], DataType::Utf8);
        let node = project(vec![keep_id(), (sub, "sub", DataType::Utf8)]);
        assert_holds_as_declared(&node, Script::Exec(input()));
    }
}

operator_case! {
    GpuProject,
    fn a_concat_over_utf8_declares_the_string_the_device_holds() {
        let twice = function("concat", vec![Expr::column(5, "s"), Expr::column(5, "s")], DataType::Utf8);
        let node = project(vec![keep_id(), (twice, "twice", DataType::Utf8)]);
        assert_holds_as_declared(&node, Script::Exec(input()));
    }
}

operator_case! {
    GpuProject,
    fn a_coalesce_over_int32_declares_the_int32_the_device_holds() {
        let filled = function("coalesce", vec![Expr::column(1, "key"), lit_i32(0)], DataType::Int32);
        let node = project(vec![keep_id(), (filled, "filled", DataType::Int32)]);
        assert_holds_as_declared(&node, Script::Exec(input()));
    }
}

operator_case! {
    GpuProject,
    fn a_round_over_float64_declares_the_float64_the_device_holds() {
        let rounded = function("round", vec![Expr::column(4, "f64"), lit_i64(1)], DataType::Float64);
        let node = project(vec![keep_id(), (rounded, "rounded", DataType::Float64)]);
        assert_holds_as_declared(&node, Script::Exec(input()));
    }
}

operator_case! {
    GpuProject,
    fn an_abs_over_int64_declares_the_int64_the_device_holds() {
        let magnitude = function("abs", vec![Expr::column(3, "i64")], DataType::Int64);
        let node = project(vec![keep_id(), (magnitude, "magnitude", DataType::Int64)]);
        assert_holds_as_declared(&node, Script::Exec(input()));
    }
}

fn date_part(field: &str) -> Expr {
    function(
        "date_part",
        vec![lit_str(field), Expr::column(6, "d")],
        DataType::Int32,
    )
}

// #191 — cuDF extracts every field as `INT16` and the `date_part` arm hands it up so where
// the plan declares `Int32`; the fix is on `date-part-return-type`. Wanted: `None`.
operator_case! {
    GpuProject,
    fn bug_a_date_part_year_holds_int16_where_the_plan_declares_int32() {
        let node = project(vec![keep_id(), (date_part("year"), "year", DataType::Int32)]);
        assert_eq!(
            divergences_on_device(&node, Script::Exec(input())),
            vec![Some("1 year: Int32 vs INT16".to_string())]
        );
    }
}

operator_case! {
    GpuProject,
    fn bug_a_date_part_month_holds_int16_where_the_plan_declares_int32() {
        let node = project(vec![keep_id(), (date_part("month"), "month", DataType::Int32)]);
        assert_eq!(
            divergences_on_device(&node, Script::Exec(input())),
            vec![Some("1 month: Int32 vs INT16".to_string())]
        );
    }
}

operator_case! {
    GpuProject,
    fn bug_a_date_part_day_holds_int16_where_the_plan_declares_int32() {
        let node = project(vec![keep_id(), (date_part("day"), "day", DataType::Int32)]);
        assert_eq!(
            divergences_on_device(&node, Script::Exec(input())),
            vec![Some("1 day: Int32 vs INT16".to_string())]
        );
    }
}

operator_case! {
    GpuProject,
    fn a_sqrt_over_float64_declares_the_float64_the_device_holds() {
        let magnitude = function("abs", vec![Expr::column(4, "f64")], DataType::Float64);
        let root = Expr::unary(UnaryOp::Sqrt, magnitude);
        let node = project(vec![keep_id(), (root, "root", DataType::Float64)]);
        assert_holds_as_declared(&node, Script::Exec(input()));
    }
}

operator_case! {
    GpuProject,
    fn the_unary_predicates_and_a_negation_declare_what_the_device_holds() {
        let node = project(vec![
            keep_id(),
            (Expr::unary(UnaryOp::IsNull, Expr::column(1, "key")), "key is null", DataType::Boolean),
            (Expr::unary(UnaryOp::Not, Expr::column(7, "b")), "not b", DataType::Boolean),
            (Expr::unary(UnaryOp::Negative, Expr::column(3, "i64")), "-i64", DataType::Int64),
        ]);
        assert_holds_as_declared(&node, Script::Exec(input()));
    }
}

operator_case! {
    GpuProject,
    fn a_comparison_and_a_conjunction_declare_the_bool8_the_device_holds() {
        let positive = gt(2, "i32", lit_i32(0));
        let both = Expr::binary(positive.clone(), BinaryOp::And, Expr::column(7, "b"), DataType::Boolean);
        let node = project(vec![
            keep_id(),
            (positive, "positive", DataType::Boolean),
            (both, "positive and b", DataType::Boolean),
        ]);
        assert_holds_as_declared(&node, Script::Exec(input()));
    }
}

operator_case! {
    GpuProject,
    fn a_search_case_over_int32_declares_the_int32_the_device_holds() {
        let case = Expr::Case {
            comparand: None,
            when_then: vec![(Expr::column(7, "b"), lit_i32(1))],
            else_expr: Some(Box::new(lit_i32(0))),
        };
        let node = project(vec![keep_id(), (case, "flag", DataType::Int32)]);
        assert_holds_as_declared(&node, Script::Exec(input()));
    }
}

operator_case! {
    GpuProject,
    fn a_like_over_utf8_declares_the_bool8_the_device_holds() {
        let node = project(vec![keep_id(), (like("%a", false, false), "ends_in_a", DataType::Boolean)]);
        assert_holds_as_declared(&node, Script::Exec(input()));
    }
}

operator_case! {
    GpuProject,
    fn a_string_literal_column_declares_the_string_the_device_holds() {
        let node = project(vec![keep_id(), (lit_str("x"), "x", DataType::Utf8)]);
        assert_holds_as_declared(&node, Script::Exec(input()));
    }
}

operator_case! {
    GpuProject,
    fn a_typed_null_int64_literal_declares_the_int64_the_device_holds() {
        let nothing = Expr::Literal(ScalarValue::Int64(None));
        let node = project(vec![keep_id(), (nothing, "nothing", DataType::Int64)]);
        assert_holds_as_declared(&node, Script::Exec(input()));
    }
}

// `GpuSort`.

operator_case! {
    GpuSort,
    fn a_sort_declares_the_input_schema_the_device_holds() {
        let node = sort(vec![by(2, true, true), by(0, true, false)], None);
        assert_holds_as_declared(&node, Script::Exec(input()));
    }
}

operator_case! {
    GpuSort,
    fn a_sort_with_a_fetch_declares_the_input_schema_the_device_holds() {
        let node = sort(vec![by(5, true, false), by(0, true, false)], Some(5));
        assert_holds_as_declared(&node, Script::Exec(input()));
    }
}
