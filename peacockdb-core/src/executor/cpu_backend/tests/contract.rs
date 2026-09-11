//! The CPU half of the contract in `crate::tests::executor_cases`: every case driven
//! through this backend and checked against the answer both engines owe.
//!
//! Agreement between two engines has its own module here for the reason
//! `test_inc2_conformance` does — it is a claim about the pair, not about either side, and
//! putting it among one side's cases makes it that side's opinion. Each executor's own
//! tests stay in the files beside this one; this is the join between them.

use super::*;
use crate::plan::{
    ColumnOrder, GpuAccumulateBatchesAndSort, GpuAggregate, GpuAggregateBatches,
    GpuCoalesceAllBatches, GpuEmitPartitions, GpuFilter, GpuProject,
};
use crate::tests::executor_cases::{CASES, INPUT, Shape};
use datafusion::arrow::array::UInt8Array;

/// The input as the lane's three batches, which is how the device's row groups deliver it.
fn batches() -> Vec<CpuBatch> {
    INPUT
        .chunks(2)
        .map(|chunk| {
            let keys: ArrayRef = Arc::new(StringArray::from(
                chunk.iter().map(|(k, _)| Some(*k)).collect::<Vec<_>>(),
            ));
            let values: ArrayRef = Arc::new(Int64Array::from(
                chunk.iter().map(|(_, v)| Some(*v)).collect::<Vec<_>>(),
            ));
            CpuBatch::new(
                RecordBatch::try_new(schema_of(&GROUPED).fields.clone(), vec![keys, values])
                    .expect("the rows fit their schema"),
            )
        })
        .collect()
}

/// `k|v` per row, sorted — the shape the table writes its answers in.
fn rendered(batches: &[CpuBatch]) -> Vec<String> {
    let mut out = Vec::new();
    for batch in batches {
        let batch = batch.record_batch();
        for row in 0..batch.num_rows() {
            let cells: Vec<String> = (0..batch.num_columns())
                .map(|column| {
                    match ScalarValue::try_from_array(batch.column(column), row)
                        .expect("a value at every position")
                    {
                        ScalarValue::Utf8(Some(text)) => text,
                        other => other.to_string(),
                    }
                })
                .collect();
            out.push(cells.join("|"));
        }
    }
    out
}

/// The state a `sum(v)` decomposes into, and the merge that folds it.
fn summed(keys: &[(&str, DataType)]) -> (Schema, AggregateBody, AggregateBody) {
    let state = schema_of(&[keys, &[("sum(v)", DataType::Int64)]].concat());
    let group_by: Vec<Expr> = keys
        .iter()
        .enumerate()
        .map(|(ordinal, (name, _))| Expr::column(ordinal as u32, name))
        .collect();
    let init = AggregateBody {
        group_by: group_by.clone(),
        grouping_sets: Vec::new(),
        null_exprs: Vec::new(),
        aggs: vec![AggCall {
            func: PlanAgg::Sum,
            args: vec![Expr::column(1, "v")],
            outputs: vec![Field::new("sum(v)", DataType::Int64, true)],
        }],
        finalize: None,
    };
    let merge = AggregateBody {
        group_by,
        grouping_sets: Vec::new(),
        null_exprs: Vec::new(),
        aggs: vec![AggCall {
            func: PlanAgg::Sum,
            args: vec![Expr::column(keys.len() as u32, "sum(v)")],
            outputs: vec![Field::new("sum(v)", DataType::Int64, true)],
        }],
        finalize: None,
    };
    (state, init, merge)
}

/// One case through this backend, as its lane's three batches. The rows come back in the
/// order the node emitted them, and are compared as a set only where that order is not the
/// answer.
fn answer(shape: Shape) -> Vec<String> {
    let mut rows = emitted(shape);
    if !shape.order_is_the_answer() {
        rows.sort();
    }
    rows
}

fn emitted(shape: Shape) -> Vec<String> {
    let input = schema_of(&GROUPED);
    match shape {
        Shape::Filter { above } => {
            let node = GpuFilter::new(
                Given::of_schema(input.clone()),
                Expr::binary(
                    Expr::column(1, "v"),
                    BinaryOp::Gt,
                    Expr::Literal(ScalarValue::Int64(Some(above))),
                    DataType::Boolean,
                ),
                None,
                input.clone(),
            );
            per_batch(CpuExec::filter(&node, &input.fields, ctx()).expect("the filter builds"))
        }
        Shape::Double => {
            let out = schema_of(&GROUPED);
            let node = GpuProject::new(
                Given::of_schema(input.clone()),
                vec![
                    NamedExpr::new(Expr::column(0, "k"), "k"),
                    NamedExpr::new(
                        Expr::binary(
                            Expr::column(1, "v"),
                            BinaryOp::Multiply,
                            Expr::Literal(ScalarValue::Int64(Some(2))),
                            DataType::Int64,
                        ),
                        "v",
                    ),
                ],
                out,
            );
            per_batch(CpuExec::project(&node, &input.fields, ctx()).expect("the project builds"))
        }
        Shape::SortLane { fetch } => {
            let node = GpuAccumulateBatchesAndSort::new(
                Given::of_schema(input.clone()),
                vec![ColumnOrder {
                    column: 1,
                    ascending: true,
                    nulls_first: false,
                }],
                fetch,
            );
            let accumulator =
                CpuAccumulator::sorted(&node, &input.fields, ctx()).expect("the sort builds");
            at_done(accumulator)
        }
        Shape::CoalesceLane => {
            let node = GpuCoalesceAllBatches::new(Given::of_schema(input.clone()));
            at_done(CpuAccumulator::coalesce(&node, &input.fields))
        }
        Shape::SumByKey { finalize } => {
            let keys = [("k", DataType::Utf8)];
            let (state, init, mut merge) = summed(&keys);
            let output = match finalize {
                true => {
                    merge.finalize = Some(vec![NamedExpr::new(Expr::column(1, "sum(v)"), "total")]);
                    schema_of(&[("k", DataType::Utf8), ("total", DataType::Int64)])
                }
                false => state.clone(),
            };
            merged(input, state, init, merge, output)
        }
        Shape::SumByKeyAndGroupingId => {
            // The merge side of a grouping-set plan: the state's keys are the group list
            // AND the id the init emitted beside it, which is the width this branch's
            // `key_width` decides.
            let keys = [("k", DataType::Utf8), ("__grouping_id", DataType::UInt8)];
            let (state, _, merge) = summed(&keys);
            let with_id = |batch: CpuBatch| {
                let batch = batch.into_record_batch();
                let ids: ArrayRef = Arc::new(UInt8Array::from(vec![0u8; batch.num_rows()]));
                let mut columns = vec![batch.column(0).clone(), ids];
                columns.push(batch.column(1).clone());
                CpuBatch::new(
                    RecordBatch::try_new(state.fields.clone(), columns)
                        .expect("the state's columns"),
                )
            };
            let node = GpuAggregateBatches::new(
                Given::of_schema(state.clone()),
                merge,
                state.clone(),
                state.clone(),
            );
            let mut accumulator = CpuAccumulator::aggregate(&node, &state.fields, ctx(), 1 << 20)
                .expect("the merge builds");
            for batch in batches() {
                accumulator
                    .accumulate_and_fetch(with_id(batch))
                    .expect("the arrival is accepted");
            }
            let (emitted, _) = accumulator.mark_done_and_fetch().expect("done is accepted");
            // The id is a key and comes back beside `k`; the answer names the two columns
            // the table writes, so it is dropped here rather than in the table.
            rendered(&emitted)
                .into_iter()
                .map(|row| {
                    let cells: Vec<&str> = row.split('|').collect();
                    format!("{}|{}", cells[0], cells[2])
                })
                .collect()
        }
        Shape::ScatterLanes { lanes } => {
            let node = GpuEmitPartitions::new(Given::of_schema(input.clone()), vec![0], lanes);
            let mut emitter =
                CpuEmitter::new(&node, lanes, &input.fields).expect("the emitter builds");
            let mut out = Vec::new();
            for batch in batches() {
                let (per_lane, _) = emitter.emit(batch).expect("the scatter runs");
                for (lane, batch) in per_lane.iter().enumerate() {
                    for row in rendered(std::slice::from_ref(batch)) {
                        out.push(format!("{lane}|{row}"));
                    }
                }
            }
            out
        }
    }
}

fn per_batch(mut exec: CpuExec) -> Vec<String> {
    let answered: Vec<CpuBatch> = batches()
        .into_iter()
        .map(|batch| exec.exec(batch).expect("the node runs").0)
        .collect();
    rendered(&answered)
}

fn at_done(mut accumulator: CpuAccumulator) -> Vec<String> {
    for batch in batches() {
        accumulator
            .accumulate_and_fetch(batch)
            .expect("the arrival is accepted");
    }
    let (emitted, _) = accumulator.mark_done_and_fetch().expect("done is accepted");
    rendered(&emitted)
}

fn merged(
    input: Schema,
    state: Schema,
    init: AggregateBody,
    merge: AggregateBody,
    output: Schema,
) -> Vec<String> {
    let init_node = GpuAggregate::new(
        Given::of_schema(input.clone()),
        init,
        state.clone(),
        state.clone(),
    );
    let mut partial =
        CpuExec::aggregate(&init_node, &input.fields, ctx()).expect("the init builds");
    let merge_node = GpuAggregateBatches::new(
        Given::of_schema(state.clone()),
        merge,
        state.clone(),
        output,
    );
    let mut accumulator = CpuAccumulator::aggregate(&merge_node, &state.fields, ctx(), 1 << 20)
        .expect("the merge builds");
    for batch in batches() {
        let (partials, _) = partial.exec(batch).expect("the init runs");
        accumulator
            .accumulate_and_fetch(partials)
            .expect("the arrival is accepted");
    }
    let (emitted, _) = accumulator.mark_done_and_fetch().expect("done is accepted");
    rendered(&emitted)
}

#[test]
fn every_case_answers_what_the_contract_says() {
    for case in CASES {
        assert_eq!(
            answer(case.shape),
            case.expect
                .iter()
                .map(|row| row.to_string())
                .collect::<Vec<String>>(),
            "{}",
            case.name
        );
    }
}
