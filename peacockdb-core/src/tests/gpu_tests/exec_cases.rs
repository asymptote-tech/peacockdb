//! `GpuFilter`, `GpuProject` and `GpuSort` through the harness: one synthetic batch or a
//! few, a hand-built node over a `Given` leaf, both backends, the slots compared exactly.
//! Every case is green or a `bug_` test with its ticket above it; nothing here repairs.

use std::sync::Arc;

use datafusion::arrow::array::{ArrayRef, Int64Array};
use datafusion::arrow::compute::{
    SortColumn, SortOptions, cast, lexsort_to_indices, take_record_batch,
};
use datafusion::arrow::datatypes::DataType;
use datafusion::arrow::record_batch::RecordBatch;
use datafusion::common::ScalarValue;

use super::script::{Outcome, Script, run_both};
use crate::plan::{
    BatchLayout, BinaryOp, ColumnOrder, Expr, GpuFilter, GpuNode, GpuProject, GpuSort, NamedExpr,
    Schema,
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

fn lit_i32(v: i32) -> Expr {
    Expr::Literal(ScalarValue::Int32(Some(v)))
}

fn lit_i64(v: i64) -> Expr {
    Expr::Literal(ScalarValue::Int64(Some(v)))
}

fn lit_str(s: &str) -> Expr {
    Expr::Literal(ScalarValue::Utf8(Some(s.into())))
}

fn keep_id() -> (Expr, &'static str, DataType) {
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

fn filter(predicate: Expr, projection: Option<Vec<u32>>) -> GpuFilter {
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

fn gt(ordinal: u32, name: &str, literal: Expr) -> Expr {
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

fn project(exprs: Vec<(Expr, &str, DataType)>) -> GpuProject {
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
    GpuProject::new(given(), named, schema)
}

/// DataFusion 45's result types for `SELECT dec + dec, dec / 2.00 FROM t` over
/// `dec DECIMAL(18, 2)`: arrow-arith's `decimal_op` rules, which `get_result_type` applies
/// to the two operand types — add is `max(p - s) + max(s) + 1` at the wider scale, divide
/// is `s1 + 4` for the scale and `p1 - s1 + s2 + scale` for the precision.
fn declared_decimal_types() -> (DataType, DataType) {
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

// #187 — the device exports every decimal at precision 38 whatever was declared. The values
// are the cpu's; only the precision of the two computed columns moves, and the scale holds.
operator_case! {
    GpuProject,
    fn bug_decimal_arithmetic_is_exported_at_precision_38() {
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
        let outcome = run_both(&node, Script::Exec(vec![dec]));
        let cpu = &outcome.cpu.as_ref().expect("the cpu answers")[0][0];
        let widened = batch_of(vec![
            ("id", cpu.column(0).clone()),
            ("doubled", cast(cpu.column(1), &DataType::Decimal128(38, 2)).unwrap()),
            ("halved", cast(cpu.column(2), &DataType::Decimal128(38, 6)).unwrap()),
        ]);
        gpu_answered(&outcome, widened, Order::AsEmitted);
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

// #198 — a typed NULL inside an AST expression is a typed zero on the device: `i32 + NULL`
// is NULL on the cpu and `i32` on the device, null only where `i32` was.
operator_case! {
    GpuProject,
    fn bug_a_typed_null_in_arithmetic_is_the_column_on_the_device() {
        let plus_null = Expr::binary(
            Expr::column(2, "i32"),
            BinaryOp::Plus,
            Expr::Literal(ScalarValue::Int32(None)),
            DataType::Int32,
        );
        let node = project(vec![keep_id(), (plus_null, "plus_null", DataType::Int32)]);
        let outcome = run_both(&node, Script::Exec(vec![input()]));
        let unchanged = batch_of(vec![
            ("id", input().column(0).clone()),
            ("plus_null", input().column(2).clone()),
        ]);
        gpu_answered(&outcome, unchanged, Order::AsEmitted);
    }
}

// #198 — a project asks `is_ast_able` before `build_column`, so a bare numeric NULL in a
// select list takes the AST path and is a column of zeros rather than of nulls.
operator_case! {
    GpuProject,
    fn bug_a_typed_null_literal_is_a_column_of_zeros_on_the_device() {
        let nothing = Expr::Literal(ScalarValue::Int64(None));
        let node = project(vec![keep_id(), (nothing, "nothing", DataType::Int64)]);
        let outcome = run_both(&node, Script::Exec(vec![input()]));
        let zeros: ArrayRef = Arc::new(Int64Array::from(vec![0; input().num_rows()]));
        let zeroed = batch_of(vec![("id", input().column(0).clone()), ("nothing", zeros)]);
        gpu_answered(&outcome, zeroed, Order::AsEmitted);
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

fn by(column: u32, ascending: bool, nulls_first: bool) -> ColumnOrder {
    ColumnOrder {
        column,
        ascending,
        nulls_first,
    }
}

fn sort(keys: Vec<ColumnOrder>, fetch: Option<usize>) -> GpuSort {
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
