//! `IS [NOT] DISTINCT FROM` on the device (review row 9): the four arms — both NULL, one NULL
//! either way, equal, unequal — through each of the two evaluators, and pbench's
//! `indf-full-join`, whose condition the planner makes a null-equal hash key, run whole. A
//! project rather than a filter, since a filter drops a NULL and a `false` alike and this
//! operator must answer neither NULL. Every case is green or a `bug_` test with its ticket.

use std::sync::Arc;

use datafusion::arrow::array::{ArrayRef, BooleanArray, Decimal128Array, Int64Array};
use datafusion::arrow::datatypes::DataType;
use datafusion::arrow::record_batch::RecordBatch;

use super::exec_cases::project_over;
use super::script::{Script, run_both};
use crate::plan::{BinaryOp, Expr};
use crate::tests::compare::{Order, assert_same};

/// `a` against `b`, a row per arm.
fn arms(a: ArrayRef, b: ArrayRef) -> RecordBatch {
    RecordBatch::try_from_iter([("a", a), ("b", b)]).expect("two columns of one length")
}

const A: [Option<i64>; 5] = [None, None, Some(1), Some(1), Some(1)];
const B: [Option<i64>; 5] = [None, Some(1), None, Some(1), Some(2)];

/// The two operators over `a` and `b`, as one project.
fn both_operators(input: &RecordBatch) -> crate::plan::GpuProject {
    let op = |op: BinaryOp| {
        Expr::binary(
            Expr::column(0, "a"),
            op,
            Expr::column(1, "b"),
            DataType::Boolean,
        )
    };
    project_over(
        input.schema(),
        vec![
            (op(BinaryOp::IsNotDistinctFrom), "indf", DataType::Boolean),
            (op(BinaryOp::IsDistinctFrom), "idf", DataType::Boolean),
        ],
    )
}

/// Both engines answer, alike, and with the arms' truth table: no NULL in either column.
fn answers_the_truth_table(input: RecordBatch) {
    let node = both_operators(&input);
    let outcome = run_both(&node, Script::Exec(vec![input]));
    outcome.same(Order::AsEmitted);
    let expected = RecordBatch::try_from_iter([
        (
            "indf",
            Arc::new(BooleanArray::from(vec![true, false, false, true, false])) as ArrayRef,
        ),
        (
            "idf",
            Arc::new(BooleanArray::from(vec![false, true, true, false, true])) as ArrayRef,
        ),
    ])
    .expect("two columns of one length");
    assert_same(
        &[vec![expected]],
        outcome.cpu.as_ref().expect("the cpu answers"),
        Order::AsEmitted,
    );
}

// Two Int64 columns are what `cudf_ast_can_evaluate` takes: `NULL_EQUAL`, and `NOT` over it.
operator_case! {
    GpuProject,
    fn is_not_distinct_from_answers_every_arm_through_the_ast() {
        let input = arms(
            Arc::new(Int64Array::from(A.to_vec())),
            Arc::new(Int64Array::from(B.to_vec())),
        );
        answers_the_truth_table(input);
    }
}

// A decimal operand is what it refuses: `binary_operation`'s `NULL_EQUALS` and
// `NULL_NOT_EQUALS`.
operator_case! {
    GpuProject,
    fn is_not_distinct_from_answers_every_arm_through_the_column_path() {
        let decimal = |values: [Option<i64>; 5]| -> ArrayRef {
            Arc::new(
                Decimal128Array::from(values.map(|v| v.map(|v| v as i128 * 100)).to_vec())
                    .with_precision_and_scale(18, 2)
                    .expect("18,2 holds 1.00 and 2.00"),
            )
        };
        let input = arms(decimal(A), decimal(B));
        answers_the_truth_table(input);
    }
}

/// pbench's `indf-full-join` at every mode, run whole on both engines: the planner keys the
/// Full join on `d_k IS NOT DISTINCT FROM t_k` with `null_equals_null`, so the NULL keys of
/// either side meet. DuckDB answers 2,009 rows.
#[tokio::test]
async fn a_full_join_on_is_not_distinct_from_agrees_on_both_engines() {
    use crate::test_support::{MODES, batches_to_sorted_str};
    for mode in &MODES {
        let what = format!("indf-full-join at {}", mode.name);
        let (cpu, gpu) = super::whole_plan::answers_on_both("pbench", "indf-full-join", mode).await;
        let rows: usize = cpu.iter().map(RecordBatch::num_rows).sum();
        assert_eq!(rows, 2009, "{what}: the cpu's row count");
        assert_eq!(
            batches_to_sorted_str(&gpu),
            batches_to_sorted_str(&cpu),
            "{what}"
        );
    }
}
