//! `GpuFilter`, `GpuProject` and `GpuSort` through the harness: one synthetic batch or a
//! few, a hand-built node over a `Given` leaf, both backends, the slots compared exactly.
//! Every case is green or a `bug_` test with its ticket above it; nothing here repairs.

use std::sync::Arc;

use datafusion::arrow::array::{ArrayRef, AsArray, BooleanArray};
use datafusion::arrow::compute::{
    SortColumn, SortOptions, cast, lexsort_to_indices, take_record_batch,
};
use datafusion::arrow::datatypes::{DataType, Field, Schema as ArrowSchema};
use datafusion::arrow::record_batch::RecordBatch;
use datafusion::common::ScalarValue;

use super::script::{Outcome, Script, each_answers, run_both};
use crate::plan::{
    BatchLayout, BinaryOp, ColumnOrder, Expr, GpuFilter, GpuNode, GpuProject, GpuSort, NamedExpr,
    Schema, UnaryOp,
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

pub(crate) fn lit_i32(v: i32) -> Expr {
    Expr::Literal(ScalarValue::Int32(Some(v)))
}

pub(crate) fn lit_i64(v: i64) -> Expr {
    Expr::Literal(ScalarValue::Int64(Some(v)))
}

pub(crate) fn lit_str(s: &str) -> Expr {
    Expr::Literal(ScalarValue::Utf8(Some(s.into())))
}

pub(crate) fn keep_id() -> (Expr, &'static str, DataType) {
    (Expr::column(0, "id"), "id", DataType::Int64)
}

/// A `bug_` test's assertion: the device answered, and with exactly this one batch.
fn gpu_answered(outcome: &Outcome, expected: RecordBatch, order: Order) {
    let gpu = outcome
        .gpu
        .as_ref()
        .unwrap_or_else(|why| panic!("gpu refused: {}", why.message));
    assert_same(&[vec![expected]], gpu, order);
}

fn batch_of(columns: Vec<(&str, ArrayRef)>) -> RecordBatch {
    RecordBatch::try_from_iter(columns).expect("columns of one length")
}

// `GpuFilter`.

pub(crate) fn filter(predicate: Expr, projection: Option<Vec<u32>>) -> GpuFilter {
    let fields = match &projection {
        None => schema(),
        Some(keep) => Arc::new(
            schema()
                .project(&keep.iter().map(|i| *i as usize).collect::<Vec<_>>())
                .expect("every kept ordinal is one of the fixture's"),
        ),
    };
    GpuFilter::new(given(), predicate, projection, Schema::new(fields))
}

pub(crate) fn gt(ordinal: u32, name: &str, literal: Expr) -> Expr {
    Expr::binary(
        Expr::column(ordinal, name),
        BinaryOp::Gt,
        literal,
        DataType::Boolean,
    )
}

operator_case! {
    GpuFilter,
    fn a_filter_on_an_int_keeps_the_same_rows() {
        let node = filter(gt(2, "i32", lit_i32(0)), None);
        run_both(&node, Script::Exec(vec![input()])).same(Order::Any);
    }
}

operator_case! {
    GpuFilter,
    fn a_filter_on_a_string_keeps_the_same_rows() {
        let predicate = Expr::binary(
            Expr::column(5, "s"),
            BinaryOp::Eq,
            lit_str("beta"),
            DataType::Boolean,
        );
        run_both(&filter(predicate, None), Script::Exec(vec![input()])).same(Order::Any);
    }
}

// `i32` is null on every fifth row; a null predicate is not true, so those rows go on
// both, whatever the comparison would have said of a value.
operator_case! {
    GpuFilter,
    fn a_null_predicate_drops_the_row_on_both() {
        let node = filter(gt(2, "i32", lit_i32(-1000)), None);
        run_both(&node, Script::Exec(vec![input()])).same(Order::Any);
    }
}

operator_case! {
    GpuFilter,
    fn a_filter_also_projects() {
        let node = filter(gt(0, "id", lit_i64(10)), Some(vec![0, 5, 7]));
        run_both(&node, Script::Exec(vec![input()])).same(Order::Any);
    }
}

operator_case! {
    GpuFilter,
    fn every_row_passing_is_the_input() {
        let node = filter(gt(0, "id", lit_i64(-1)), None);
        run_both(&node, Script::Exec(vec![input()])).same(Order::AsEmitted);
    }
}

operator_case! {
    GpuFilter,
    fn no_row_passing_is_zero_rows_under_the_schema() {
        let node = filter(gt(0, "id", lit_i64(1000)), None);
        run_both(&node, Script::Exec(vec![input()])).same(Order::AsEmitted);
    }
}

// Empty inputs, each its own case.
operator_case! {
    GpuFilter,
    fn a_filter_over_a_zero_row_batch_is_zero_rows_on_both() {
        let node = filter(gt(2, "i32", lit_i32(0)), None);
        run_both(&node, Script::Exec(vec![synthetic(0, 1)])).same(Order::AsEmitted);
    }
}

operator_case! {
    GpuFilter,
    fn a_zero_row_batch_between_two_with_rows_is_filtered_on_its_own() {
        let node = filter(gt(2, "i32", lit_i32(0)), None);
        let stream = vec![synthetic(16, 1), synthetic(0, 2), synthetic(16, 3)];
        run_both(&node, Script::Exec(stream)).same(Order::AsEmitted);
    }
}

// `GpuProject`. The declared type of each expression is DataFusion's own coercion, which
// is what the planner would have written; the comparison is of what each engine produced.

pub(crate) fn project(exprs: Vec<(Expr, &str, DataType)>) -> GpuProject {
    project_over(schema(), exprs)
}

/// A function by the name the planner emits, `nullable` as every corpus function is.
pub(crate) fn function(name: &str, args: Vec<Expr>, return_type: DataType) -> Expr {
    Expr::ScalarFunction {
        name: name.to_string(),
        args,
        return_type,
        nullable: true,
    }
}

/// `project` over a leaf declaring `input`, for a batch that is not the fixture.
pub(crate) fn project_over(input: Arc<ArrowSchema>, exprs: Vec<(Expr, &str, DataType)>) -> GpuProject {
    let schema = columns(
        &exprs
            .iter()
            .map(|(_, name, ty)| (*name, ty.clone()))
            .collect::<Vec<_>>(),
    );
    let named = exprs
        .into_iter()
        .map(|(expr, name, _)| NamedExpr::new(expr, name))
        .collect();
    GpuProject::new(
        Given::of(Schema::new(input), BatchLayout::MultipleBatches),
        named,
        schema,
    )
}

/// DataFusion 45's result types for `SELECT dec + dec, dec / 2.00 FROM t` over
/// `dec DECIMAL(18, 2)`: arrow-arith's `decimal_op` rules, which `get_result_type` applies
/// to the two operand types — add is `max(p - s) + max(s) + 1` at the wider scale, divide
/// is `s1 + 4` for the scale and `p1 - s1 + s2 + scale` for the precision.
pub(crate) fn declared_decimal_types() -> (DataType, DataType) {
    (DataType::Decimal128(19, 2), DataType::Decimal128(24, 6))
}

operator_case! {
    GpuProject,
    fn a_column_copy_is_the_column() {
        let node = project(vec![(Expr::column(5, "s"), "s", DataType::Utf8), keep_id()]);
        run_both(&node, Script::Exec(vec![input()])).same(Order::AsEmitted);
    }
}

operator_case! {
    GpuProject,
    fn int_arithmetic_agrees() {
        let sum = Expr::binary(
            Expr::column(2, "i32"),
            BinaryOp::Plus,
            lit_i32(7),
            DataType::Int32,
        );
        let product = Expr::binary(
            Expr::column(3, "i64"),
            BinaryOp::Multiply,
            lit_i64(3),
            DataType::Int64,
        );
        let node = project(vec![
            keep_id(),
            (sum, "sum", DataType::Int32),
            (product, "product", DataType::Int64),
        ]);
        run_both(&node, Script::Exec(vec![input()])).same(Order::AsEmitted);
    }
}

operator_case! {
    GpuProject,
    fn float_arithmetic_agrees_bit_for_bit() {
        let scaled = Expr::binary(
            Expr::column(4, "f64"),
            BinaryOp::Multiply,
            Expr::Literal(ScalarValue::Float64(Some(2.0))),
            DataType::Float64,
        );
        let node = project(vec![keep_id(), (scaled, "scaled", DataType::Float64)]);
        run_both(&node, Script::Exec(vec![input()])).same(Order::AsEmitted);
    }
}

// Both computed columns come back at the precision the node declares, the scale read off
// the column: a cuDF binop lands the sum and the quotient at 38, and the export relabels.
operator_case! {
    GpuProject,
    fn decimal_arithmetic_is_exported_at_its_declared_precision() {
        let dec = decimals(64, 1);
        let (add_type, divide_type) = declared_decimal_types();
        let doubled = Expr::binary(
            Expr::column(1, "dec"),
            BinaryOp::Plus,
            Expr::column(1, "dec"),
            add_type.clone(),
        );
        let halved = Expr::binary(
            Expr::column(1, "dec"),
            BinaryOp::Divide,
            Expr::Literal(ScalarValue::Decimal128(Some(200), 3, 2)),
            divide_type.clone(),
        );
        let schema = columns(&[
            ("id", DataType::Int64),
            ("doubled", add_type),
            ("halved", divide_type),
        ]);
        let node = GpuProject::new(
            Given::of(Schema::new(dec.schema()), BatchLayout::MultipleBatches),
            vec![
                NamedExpr::new(Expr::column(0, "id"), "id"),
                NamedExpr::new(doubled, "doubled"),
                NamedExpr::new(halved, "halved"),
            ],
            schema,
        );
        run_both(&node, Script::Exec(vec![dec])).same(Order::AsEmitted);
    }
}

operator_case! {
    GpuProject,
    fn a_cast_widens_an_int_on_both() {
        let widened = Expr::Cast {
            expr: Box::new(Expr::column(2, "i32")),
            target: DataType::Int64,
        };
        let node = project(vec![keep_id(), (widened, "widened", DataType::Int64)]);
        run_both(&node, Script::Exec(vec![input()])).same(Order::AsEmitted);
    }
}

// #203 — `cudf::cast` has no string target, and the column path refuses rather than
// converting.
operator_case! {
    GpuProject,
    fn bug_a_cast_to_text_is_refused_on_the_device() {
        let as_text = Expr::Cast {
            expr: Box::new(Expr::column(1, "key")),
            target: DataType::Utf8,
        };
        let node = project(vec![keep_id(), (as_text, "as_text", DataType::Utf8)]);
        let outcome = run_both(&node, Script::Exec(vec![input()]));
        assert!(
            outcome
                .gpu_refuses()
                .contains("cast to STRING from a non-string type not supported in column path"),
            "{}",
            outcome.gpu_refuses()
        );
    }
}

operator_case! {
    GpuProject,
    fn a_search_case_agrees() {
        let case = Expr::Case {
            comparand: None,
            when_then: vec![(Expr::column(7, "b"), lit_i32(1))],
            else_expr: Some(Box::new(lit_i32(0))),
        };
        let node = project(vec![keep_id(), (case, "flag", DataType::Int32)]);
        run_both(&node, Script::Exec(vec![input()])).same(Order::AsEmitted);
    }
}

// #57 — value-form CASE is withheld on the column path since the fold on the comparand
// came back wrong; the device refuses it, the cpu answers.
operator_case! {
    GpuProject,
    fn bug_a_value_case_is_refused_on_the_device() {
        let case = Expr::Case {
            comparand: Some(Box::new(Expr::column(1, "key"))),
            when_then: vec![(lit_i32(1), lit_str("one"))],
            else_expr: Some(Box::new(lit_str("other"))),
        };
        let node = project(vec![keep_id(), (case, "word", DataType::Utf8)]);
        let outcome = run_both(&node, Script::Exec(vec![input()]));
        assert!(
            outcome
                .gpu_refuses()
                .contains("value-form CASE not supported in column path"),
            "{}",
            outcome.gpu_refuses()
        );
    }
}

operator_case! {
    GpuProject,
    fn like_agrees() {
        let like = Expr::Like {
            expr: Box::new(Expr::column(5, "s")),
            pattern: Box::new(lit_str("%a")),
            negated: false,
            case_insensitive: false,
        };
        let node = project(vec![keep_id(), (like, "ends_in_a", DataType::Boolean)]);
        run_both(&node, Script::Exec(vec![input()])).same(Order::AsEmitted);
    }
}

// `upper` takes the column path in the C++ (`expr.cpp`, `build_column`), not the AST one.
operator_case! {
    GpuProject,
    fn a_scalar_function_agrees() {
        let upper = Expr::ScalarFunction {
            name: "upper".into(),
            args: vec![Expr::column(5, "s")],
            return_type: DataType::Utf8,
            nullable: true,
        };
        let node = project(vec![keep_id(), (upper, "loud", DataType::Utf8)]);
        run_both(&node, Script::Exec(vec![input()])).same(Order::AsEmitted);
    }
}

// A typed NULL crosses the wire as `is_null` with no value, and the AST path builds it
// invalid: `i32 + NULL` is NULL on both engines, and a bare NULL is a null column of its type.
operator_case! {
    GpuProject,
    fn a_typed_null_in_arithmetic_is_null() {
        let plus_null = Expr::binary(
            Expr::column(2, "i32"),
            BinaryOp::Plus,
            Expr::Literal(ScalarValue::Int32(None)),
            DataType::Int32,
        );
        let node = project(vec![keep_id(), (plus_null, "plus_null", DataType::Int32)]);
        run_both(&node, Script::Exec(vec![input()])).same(Order::AsEmitted);
    }
}

operator_case! {
    GpuProject,
    fn a_typed_null_literal_is_a_null_column() {
        let nothing = Expr::Literal(ScalarValue::Int64(None));
        let node = project(vec![keep_id(), (nothing, "nothing", DataType::Int64)]);
        run_both(&node, Script::Exec(vec![input()])).same(Order::AsEmitted);
    }
}

// #210 — a bare decimal literal is AST-able and cuDF's AST has no fixed-point literal, so
// the device computes a Float64 column where the plan declares the decimal, and the export,
// told the declared precision, refuses it by name as a column that is not a decimal.
operator_case! {
    GpuProject,
    fn bug_a_bare_decimal_literal_is_a_float64_column_on_the_device() {
        let one_and_a_half = Expr::Literal(ScalarValue::Decimal128(Some(15), 3, 1));
        let node = project(vec![
            keep_id(),
            (one_and_a_half, "one_and_a_half", DataType::Decimal128(3, 1)),
        ]);
        let outcome = run_both(&node, Script::Exec(vec![input()]));
        let why = outcome.gpu_refuses();
        assert!(
            why.contains("column one_and_a_half is not a decimal but was given precision 3"),
            "{why}"
        );
    }
}

// The expressions the corpus projects that task 9 did not: the other arithmetic, a
// predicate and a conjunction as columns, the unary forms, the casts to and from decimal,
// date and text, the functions, LIKE's other forms, a typed-NULL branch, a literal column.

operator_case! {
    GpuProject,
    fn a_minus_and_a_modulo_agree() {
        let difference = Expr::binary(
            Expr::column(3, "i64"),
            BinaryOp::Minus,
            Expr::column(0, "id"),
            DataType::Int64,
        );
        let remainder = Expr::binary(
            Expr::column(3, "i64"),
            BinaryOp::Modulo,
            lit_i64(7),
            DataType::Int64,
        );
        let node = project(vec![
            keep_id(),
            (difference, "i64 - id", DataType::Int64),
            (remainder, "i64 % 7", DataType::Int64),
        ]);
        run_both(&node, Script::Exec(vec![input()])).same(Order::AsEmitted);
    }
}

operator_case! {
    GpuProject,
    fn a_comparison_and_a_conjunction_as_columns_agree() {
        let positive = gt(2, "i32", lit_i32(0));
        let both = Expr::binary(
            positive.clone(),
            BinaryOp::And,
            Expr::column(7, "b"),
            DataType::Boolean,
        );
        let either = Expr::binary(
            positive.clone(),
            BinaryOp::Or,
            Expr::column(7, "b"),
            DataType::Boolean,
        );
        let node = project(vec![
            keep_id(),
            (positive, "positive", DataType::Boolean),
            (both, "positive and b", DataType::Boolean),
            (either, "positive or b", DataType::Boolean),
        ]);
        run_both(&node, Script::Exec(vec![input()])).same(Order::AsEmitted);
    }
}

operator_case! {
    GpuProject,
    fn the_unary_forms_agree() {
        let node = project(vec![
            keep_id(),
            (Expr::unary(UnaryOp::IsNull, Expr::column(1, "key")), "key is null", DataType::Boolean),
            (Expr::unary(UnaryOp::IsNotNull, Expr::column(1, "key")), "key is not null", DataType::Boolean),
            (Expr::unary(UnaryOp::Not, Expr::column(7, "b")), "not b", DataType::Boolean),
            (Expr::unary(UnaryOp::Negative, Expr::column(3, "i64")), "-i64", DataType::Int64),
        ]);
        run_both(&node, Script::Exec(vec![input()])).same(Order::AsEmitted);
    }
}

pub(crate) fn cast_to(ordinal: u32, name: &str, target: DataType) -> Expr {
    Expr::Cast {
        expr: Box::new(Expr::column(ordinal, name)),
        target,
    }
}

// A cast to a narrow decimal reads back at the precision the node declares: cuDF's cast
// lands the column at 38, and the export relabels it to the declaration.
operator_case! {
    GpuProject,
    fn a_cast_to_decimal_is_exported_at_its_declared_precision() {
        let declared = DataType::Decimal128(20, 0);
        let node = project(vec![
            keep_id(),
            (cast_to(3, "i64", declared.clone()), "as_decimal", declared),
        ]);
        run_both(&node, Script::Exec(vec![input()])).same(Order::AsEmitted);
    }
}

operator_case! {
    GpuProject,
    fn a_decimal_cast_to_float64_agrees() {
        let dec = decimals(64, 1);
        let node = project_over(
            dec.schema(),
            vec![keep_id(), (cast_to(1, "dec", DataType::Float64), "as_f64", DataType::Float64)],
        );
        run_both(&node, Script::Exec(vec![dec])).same(Order::AsEmitted);
    }
}

// #203 — the same arm refuses every cast to text, a date's included.
operator_case! {
    GpuProject,
    fn bug_a_date_cast_to_text_is_refused_on_the_device() {
        let node = project(vec![keep_id(), (cast_to(6, "d", DataType::Utf8), "as_text", DataType::Utf8)]);
        let outcome = run_both(&node, Script::Exec(vec![input()]));
        assert!(
            outcome
                .gpu_refuses()
                .contains("cast to STRING from a non-string type not supported in column path"),
            "{}",
            outcome.gpu_refuses()
        );
    }
}

/// `input()` with `d` cast to `Utf8` by arrow, so a text-to-date cast has dates to parse.
fn dates_as_text() -> RecordBatch {
    let batch = input();
    let mut columns = batch.columns().to_vec();
    columns[6] = cast(&columns[6], &DataType::Utf8).expect("a date renders");
    let fields: Vec<Field> = batch
        .schema()
        .fields()
        .iter()
        .map(|f| match f.name().as_str() {
            "d" => Field::new("d", DataType::Utf8, true),
            _ => f.as_ref().clone(),
        })
        .collect();
    RecordBatch::try_new(Arc::new(ArrowSchema::new(fields)), columns).expect("d retyped")
}

// #218 — the cast arm hands a string column to `cudf::cast`, which parses nothing.
operator_case! {
    GpuProject,
    fn bug_a_text_cast_to_date_is_refused_on_the_device() {
        let batch = dates_as_text();
        let node = project_over(
            batch.schema(),
            vec![keep_id(), (cast_to(6, "d", DataType::Date32), "as_date", DataType::Date32)],
        );
        let outcome = run_both(&node, Script::Exec(vec![batch]));
        assert!(
            outcome
                .gpu_refuses()
                .contains("Column type must be numeric or chrono or decimal32/64/128"),
            "{}",
            outcome.gpu_refuses()
        );
    }
}

// #191 — cuDF extracts a year as `Int16`, and the device hands it up so where the plan
// declares `Int32`; the values are the cpu's.
operator_case! {
    GpuProject,
    fn bug_a_year_extracted_from_a_date_is_exported_as_int16() {
        let year = function("date_part", vec![lit_str("year"), Expr::column(6, "d")], DataType::Int32);
        let node = project(vec![keep_id(), (year, "year", DataType::Int32)]);
        let outcome = run_both(&node, Script::Exec(vec![input()]));
        let cpu = &outcome.cpu.as_ref().expect("the cpu answers")[0][0];
        let narrowed = batch_of(vec![
            ("id", cpu.column(0).clone()),
            ("year", cast(cpu.column(1), &DataType::Int16).unwrap()),
        ]);
        gpu_answered(&outcome, narrowed, Order::AsEmitted);
    }
}

operator_case! {
    GpuProject,
    fn a_substring_agrees() {
        let sub = function("substr", vec![Expr::column(5, "s"), lit_i64(2), lit_i64(3)], DataType::Utf8);
        let node = project(vec![keep_id(), (sub, "sub", DataType::Utf8)]);
        run_both(&node, Script::Exec(vec![input()])).same(Order::AsEmitted);
    }
}

operator_case! {
    GpuProject,
    fn a_coalesce_agrees() {
        let filled = function("coalesce", vec![Expr::column(1, "key"), lit_i32(0)], DataType::Int32);
        let node = project(vec![keep_id(), (filled, "filled", DataType::Int32)]);
        run_both(&node, Script::Exec(vec![input()])).same(Order::AsEmitted);
    }
}

operator_case! {
    GpuProject,
    fn a_concat_agrees() {
        let twice = function("concat", vec![Expr::column(5, "s"), Expr::column(5, "s")], DataType::Utf8);
        let node = project(vec![keep_id(), (twice, "twice", DataType::Utf8)]);
        run_both(&node, Script::Exec(vec![input()])).same(Order::AsEmitted);
    }
}

operator_case! {
    GpuProject,
    fn a_lower_agrees() {
        let quiet = function("lower", vec![Expr::column(5, "s")], DataType::Utf8);
        let node = project(vec![keep_id(), (quiet, "quiet", DataType::Utf8)]);
        run_both(&node, Script::Exec(vec![input()])).same(Order::AsEmitted);
    }
}

operator_case! {
    GpuProject,
    fn a_round_to_one_place_agrees() {
        let rounded = function("round", vec![Expr::column(4, "f64"), lit_i64(1)], DataType::Float64);
        let node = project(vec![keep_id(), (rounded, "rounded", DataType::Float64)]);
        run_both(&node, Script::Exec(vec![input()])).same(Order::AsEmitted);
    }
}

pub(crate) fn like(pattern: &str, negated: bool, case_insensitive: bool) -> Expr {
    Expr::Like {
        expr: Box::new(Expr::column(5, "s")),
        pattern: Box::new(lit_str(pattern)),
        negated,
        case_insensitive,
    }
}

operator_case! {
    GpuProject,
    fn not_like_agrees() {
        let node = project(vec![keep_id(), (like("b%", true, false), "not_b", DataType::Boolean)]);
        run_both(&node, Script::Exec(vec![input()])).same(Order::AsEmitted);
    }
}

// #219 — the LIKE arm reads `negated` and never `case_insensitive`, so `ILIKE 'B%'` is a
// case-sensitive `LIKE 'B%'` on the device: false on every word, null where `s` is.
operator_case! {
    GpuProject,
    fn bug_ilike_is_case_sensitive_on_the_device() {
        let node = project(vec![keep_id(), (like("B%", false, true), "b_any_case", DataType::Boolean)]);
        let outcome = run_both(&node, Script::Exec(vec![input()]));
        let none: ArrayRef = Arc::new(BooleanArray::from_iter(
            input().column(5).as_string::<i32>().iter().map(|word| word.map(|_| false)),
        ));
        let expected = batch_of(vec![("id", input().column(0).clone()), ("b_any_case", none)]);
        gpu_answered(&outcome, expected, Order::AsEmitted);
    }
}

operator_case! {
    GpuProject,
    fn a_case_with_a_typed_null_branch_agrees() {
        let case = Expr::Case {
            comparand: None,
            when_then: vec![(Expr::column(7, "b"), Expr::Literal(ScalarValue::Int64(None)))],
            else_expr: Some(Box::new(Expr::column(3, "i64"))),
        };
        let node = project(vec![keep_id(), (case, "unless_b", DataType::Int64)]);
        run_both(&node, Script::Exec(vec![input()])).same(Order::AsEmitted);
    }
}

operator_case! {
    GpuProject,
    fn a_string_literal_as_a_column_agrees() {
        let node = project(vec![keep_id(), (lit_str("x"), "x", DataType::Utf8)]);
        run_both(&node, Script::Exec(vec![input()])).same(Order::AsEmitted);
    }
}

// Empty inputs, each its own case.
operator_case! {
    GpuProject,
    fn a_project_over_a_zero_row_batch_is_zero_rows_on_both() {
        let next = Expr::binary(
            Expr::column(2, "i32"),
            BinaryOp::Plus,
            lit_i32(1),
            DataType::Int32,
        );
        let node = project(vec![keep_id(), (next, "next", DataType::Int32)]);
        run_both(&node, Script::Exec(vec![synthetic(0, 1)])).same(Order::AsEmitted);
    }
}

operator_case! {
    GpuProject,
    fn a_zero_row_batch_between_two_with_rows_is_projected_on_its_own() {
        let node = project(vec![keep_id(), (Expr::column(5, "s"), "s", DataType::Utf8)]);
        let stream = vec![synthetic(16, 1), synthetic(0, 2), synthetic(16, 3)];
        run_both(&node, Script::Exec(stream)).same(Order::AsEmitted);
    }
}

// `GpuSort`. `id` is unique, so it is every single-key sort's key or tie-breaker; `i32`
// carries nulls and duplicates, so it is the nulls-first/last key with `id` behind it.

pub(crate) fn by(column: u32, ascending: bool, nulls_first: bool) -> ColumnOrder {
    ColumnOrder {
        column,
        ascending,
        nulls_first,
    }
}

pub(crate) fn sort(keys: Vec<ColumnOrder>, fetch: Option<usize>) -> GpuSort {
    GpuSort::new(given(), keys, fetch)
}

operator_case! {
    GpuSort,
    fn an_ascending_sort_emits_the_same_order() {
        let node = sort(vec![by(0, true, false)], None);
        run_both(&node, Script::Exec(vec![input()])).same(Order::AsEmitted);
    }
}

operator_case! {
    GpuSort,
    fn a_descending_sort_emits_the_same_order() {
        let node = sort(vec![by(0, false, false)], None);
        run_both(&node, Script::Exec(vec![input()])).same(Order::AsEmitted);
    }
}

operator_case! {
    GpuSort,
    fn nulls_first_then_by_id() {
        let node = sort(vec![by(2, true, true), by(0, true, false)], None);
        run_both(&node, Script::Exec(vec![input()])).same(Order::AsEmitted);
    }
}

operator_case! {
    GpuSort,
    fn nulls_last_then_by_id() {
        let node = sort(vec![by(2, true, false), by(0, true, false)], None);
        run_both(&node, Script::Exec(vec![input()])).same(Order::AsEmitted);
    }
}

/// `input()` ordered by arrow, for the order the device answers when it is not the plan's.
fn input_ordered_by(i32_descending: bool, i32_nulls_first: bool) -> RecordBatch {
    let batch = input();
    let key = |ordinal: usize, descending: bool, nulls_first: bool| SortColumn {
        values: batch.column(ordinal).clone(),
        options: Some(SortOptions {
            descending,
            nulls_first,
        }),
    };
    let keys = [
        key(2, i32_descending, i32_nulls_first),
        key(0, false, false),
    ];
    let indices = lexsort_to_indices(&keys, None).expect("ints sort");
    take_record_batch(&batch, &indices).expect("a permutation of the batch")
}

// #202 — on a descending key the device places nulls at the end the plan did not declare:
// `nulls_first` maps straight onto `cudf::null_order`, which cuDF applies before it flips
// the direction.
operator_case! {
    GpuSort,
    fn bug_a_descending_key_with_nulls_last_puts_them_first_on_the_device() {
        let node = sort(vec![by(2, false, false), by(0, true, false)], None);
        let outcome = run_both(&node, Script::Exec(vec![input()]));
        gpu_answered(&outcome, input_ordered_by(true, true), Order::AsEmitted);
    }
}

// #202 — the other direction of the same mapping.
operator_case! {
    GpuSort,
    fn bug_a_descending_key_with_nulls_first_puts_them_last_on_the_device() {
        let node = sort(vec![by(2, false, true), by(0, true, false)], None);
        let outcome = run_both(&node, Script::Exec(vec![input()]));
        gpu_answered(&outcome, input_ordered_by(true, false), Order::AsEmitted);
    }
}

operator_case! {
    GpuSort,
    fn a_fetch_keeps_the_same_top_n() {
        let node = sort(vec![by(3, true, false), by(0, true, false)], Some(5));
        run_both(&node, Script::Exec(vec![input()])).same(Order::AsEmitted);
    }
}

operator_case! {
    GpuSort,
    fn each_batch_is_sorted_on_its_own() {
        let stream = vec![synthetic(16, 1), synthetic(16, 2), synthetic(8, 3)];
        let node = sort(vec![by(0, false, false)], None);
        run_both(&node, Script::Exec(stream)).same(Order::AsEmitted);
    }
}

// The sorts the corpus writes: a `fetch` with a descending key, a fetch at and past the
// batch, a fetch of nothing, and string and date keys with `id` behind them for a total
// order.

operator_case! {
    GpuSort,
    fn a_fetch_with_a_descending_key_keeps_the_same_top_n() {
        let node = sort(vec![by(0, false, false)], Some(5));
        run_both(&node, Script::Exec(vec![input()])).same(Order::AsEmitted);
    }
}

operator_case! {
    GpuSort,
    fn a_fetch_at_the_row_count_is_the_whole_batch() {
        let node = sort(vec![by(0, false, false)], Some(64));
        run_both(&node, Script::Exec(vec![input()])).same(Order::AsEmitted);
    }
}

operator_case! {
    GpuSort,
    fn a_fetch_past_the_row_count_is_the_whole_batch() {
        let node = sort(vec![by(0, false, false)], Some(100));
        run_both(&node, Script::Exec(vec![input()])).same(Order::AsEmitted);
    }
}

// #217 — `sort.cpp` slices only for a fetch above zero, the wire's -1 being "none", so a
// fetch of nothing keeps every row on the device: the cpu answers zero rows, the device the
// batch, which an ascending `id` leaves as it was.
operator_case! {
    GpuSort,
    fn bug_a_fetch_of_zero_keeps_every_row_on_the_device() {
        let node = sort(vec![by(0, true, false)], Some(0));
        let outcome = run_both(&node, Script::Exec(vec![input()]));
        each_answers(&outcome, &[vec![input().slice(0, 0)]], &[vec![input()]]);
    }
}

operator_case! {
    GpuSort,
    fn a_string_key_then_by_id() {
        let node = sort(vec![by(5, true, false), by(0, true, false)], None);
        run_both(&node, Script::Exec(vec![input()])).same(Order::AsEmitted);
    }
}

operator_case! {
    GpuSort,
    fn a_date_key_then_by_id() {
        let node = sort(vec![by(6, true, false), by(0, true, false)], None);
        run_both(&node, Script::Exec(vec![input()])).same(Order::AsEmitted);
    }
}

// Empty inputs, each its own case.
operator_case! {
    GpuSort,
    fn a_sort_over_a_zero_row_batch_is_zero_rows_on_both() {
        let node = sort(vec![by(0, false, false)], None);
        run_both(&node, Script::Exec(vec![synthetic(0, 1)])).same(Order::AsEmitted);
    }
}

operator_case! {
    GpuSort,
    fn a_zero_row_batch_between_two_with_rows_is_sorted_on_its_own() {
        let node = sort(vec![by(0, false, false)], Some(5));
        let stream = vec![synthetic(16, 1), synthetic(0, 2), synthetic(16, 3)];
        run_both(&node, Script::Exec(stream)).same(Order::AsEmitted);
    }
}
