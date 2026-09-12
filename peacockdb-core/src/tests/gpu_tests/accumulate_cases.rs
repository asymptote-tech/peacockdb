//! `GpuCoalesceAllBatches`, `GpuAccumulateBatchesAndSort` and `GpuMergeSortedPartitions`
//! through the harness: a lane's batches or N lanes' worth, both backends, the slots
//! compared as emitted. Every case is green or a `bug_` test with its ticket above it;
//! nothing here repairs. A sorted run is a slice of one synthetic batch dealt round-robin,
//! so no `id` is in two runs and a merge of them interleaves every one.

use datafusion::arrow::array::UInt32Array;
use datafusion::arrow::compute::take_record_batch;
use datafusion::arrow::record_batch::RecordBatch;

use super::script::{Outcome, Script, run_both};
use crate::plan::{
    ColumnOrder, GpuAccumulateBatchesAndSort, GpuCoalesceAllBatches, GpuMergeSortedPartitions,
    GpuNode, PartitionLayout, Schema, SortOrder,
};
use crate::tests::compare::{Order, assert_same};
use crate::tests::given::Given;
use crate::tests::synthetic::{schema, synthetic};

fn by_id() -> Vec<ColumnOrder> {
    vec![ColumnOrder {
        column: 0,
        ascending: true,
        nulls_first: false,
    }]
}

/// `synthetic(rows, seed)` dealt into `n` runs, row `r` to run `r % n`: each run is sorted
/// by `id`, no `id` is in two, and every run has rows a merge must interleave.
fn runs(rows: usize, seed: u64, n: usize) -> Vec<RecordBatch> {
    let whole = synthetic(rows, seed);
    (0..n)
        .map(|run| {
            let rows = UInt32Array::from_iter_values((run..rows).step_by(n).map(|r| r as u32));
            take_record_batch(&whole, &rows).expect("a selection of the batch")
        })
        .collect()
}

/// A `bug_` test's assertion: both answered, each with exactly these slots.
fn each_answers(outcome: &Outcome, cpu: &[Vec<RecordBatch>], gpu: &[Vec<RecordBatch>]) {
    assert_same(
        cpu,
        outcome.cpu.as_ref().expect("the cpu answers"),
        Order::AsEmitted,
    );
    assert_same(
        gpu,
        outcome.gpu.as_ref().expect("the device answers"),
        Order::AsEmitted,
    );
}

/// One lane whose batches are each sorted by `id`, which is what a `GpuSort` above a scan
/// declares and what both accumulating sorts require of their input.
fn sorted_lanes(n: usize) -> Box<dyn GpuNode> {
    Given::with_layout(
        Schema::new(schema()),
        PartitionLayout {
            sort_order: SortOrder::batch_sorted(by_id()),
            ..PartitionLayout::new(n)
        },
    )
}

// `GpuCoalesceAllBatches`.

fn coalesce() -> GpuCoalesceAllBatches {
    GpuCoalesceAllBatches::new(Given::with_layout(
        Schema::new(schema()),
        PartitionLayout::new(1),
    ))
}

operator_case! {
    GpuCoalesceAllBatches,
    fn several_batches_coalesce_to_one() {
        let stream = vec![synthetic(16, 1), synthetic(24, 2), synthetic(8, 3)];
        run_both(&coalesce(), Script::Accumulate(stream)).same(Order::AsEmitted);
    }
}

operator_case! {
    GpuCoalesceAllBatches,
    fn one_batch_coalesces_to_itself() {
        run_both(&coalesce(), Script::Accumulate(vec![synthetic(16, 1)])).same(Order::AsEmitted);
    }
}

// Empty inputs, each its own case. #173's site: a collapse of no handles is a call neither
// backend makes, so both answer nothing.
operator_case! {
    GpuCoalesceAllBatches,
    fn no_batch_coalesces_to_nothing_on_both() {
        run_both(&coalesce(), Script::Accumulate(Vec::new())).same(Order::AsEmitted);
    }
}

operator_case! {
    GpuCoalesceAllBatches,
    fn one_zero_row_batch_coalesces_to_zero_rows() {
        run_both(&coalesce(), Script::Accumulate(vec![synthetic(0, 1)])).same(Order::AsEmitted);
    }
}

operator_case! {
    GpuCoalesceAllBatches,
    fn a_zero_row_batch_among_others_adds_nothing() {
        let stream = vec![synthetic(16, 1), synthetic(0, 2), synthetic(16, 3)];
        run_both(&coalesce(), Script::Accumulate(stream)).same(Order::AsEmitted);
    }
}

// `GpuAccumulateBatchesAndSort`.

fn sorted(fetch: Option<usize>) -> GpuAccumulateBatchesAndSort {
    GpuAccumulateBatchesAndSort::new(sorted_lanes(1), by_id(), fetch)
}

operator_case! {
    GpuAccumulateBatchesAndSort,
    fn sorted_batches_merge_into_one_sorted_stream() {
        run_both(&sorted(None), Script::Accumulate(runs(48, 1, 3))).same(Order::AsEmitted);
    }
}

operator_case! {
    GpuAccumulateBatchesAndSort,
    fn a_fetch_cuts_the_merged_stream() {
        run_both(&sorted(Some(10)), Script::Accumulate(runs(40, 1, 2))).same(Order::AsEmitted);
    }
}

operator_case! {
    GpuAccumulateBatchesAndSort,
    fn one_sorted_batch_is_itself() {
        run_both(&sorted(None), Script::Accumulate(vec![synthetic(16, 1)])).same(Order::AsEmitted);
    }
}

// #204 — the device's merge slices to its fetch only where it merges, and one input is
// concatenated instead: the cpu answers the top 5, the device all 16.
operator_case! {
    GpuAccumulateBatchesAndSort,
    fn bug_a_fetch_over_one_sorted_batch_is_not_applied_on_the_device() {
        let batch = synthetic(16, 1);
        let outcome = run_both(&sorted(Some(5)), Script::Accumulate(vec![batch.clone()]));
        each_answers(
            &outcome,
            &[Vec::new(), vec![batch.slice(0, 5)]],
            &[Vec::new(), vec![batch]],
        );
    }
}

// Empty inputs, each its own case. #173's site again: a merge of no runs.
operator_case! {
    GpuAccumulateBatchesAndSort,
    fn no_sorted_batch_is_nothing_on_both() {
        run_both(&sorted(None), Script::Accumulate(Vec::new())).same(Order::AsEmitted);
    }
}

// #205 — DataFusion's sort over zero rows yields no batch, which the cpu accumulator reads
// as a lane that received nothing; the device answers the zero-row batch it was given.
operator_case! {
    GpuAccumulateBatchesAndSort,
    fn bug_one_zero_row_batch_sorts_to_nothing_on_the_cpu() {
        let outcome = run_both(&sorted(None), Script::Accumulate(vec![synthetic(0, 1)]));
        each_answers(
            &outcome,
            &[Vec::new(), Vec::new()],
            &[Vec::new(), vec![synthetic(0, 1)]],
        );
    }
}

operator_case! {
    GpuAccumulateBatchesAndSort,
    fn a_zero_row_batch_among_sorted_others_adds_nothing() {
        let [first, second]: [RecordBatch; 2] = runs(32, 1, 2).try_into().expect("two runs");
        let stream = vec![first, synthetic(0, 2), second];
        run_both(&sorted(None), Script::Accumulate(stream)).same(Order::AsEmitted);
    }
}

// #205 — the same with a fetch, which neither side has rows to apply.
operator_case! {
    GpuAccumulateBatchesAndSort,
    fn bug_a_fetch_over_zero_rows_is_nothing_on_the_cpu() {
        let outcome = run_both(&sorted(Some(5)), Script::Accumulate(vec![synthetic(0, 1)]));
        each_answers(
            &outcome,
            &[Vec::new(), Vec::new()],
            &[Vec::new(), vec![synthetic(0, 1)]],
        );
    }
}

// `GpuMergeSortedPartitions`. `Script::Lanes` drives every lane in order, each lane's
// batches then its `Done`, so lane 0 is always done before lane 1's rows arrive.

fn merged(lanes: usize, fetch: Option<usize>) -> GpuMergeSortedPartitions {
    GpuMergeSortedPartitions::new(sorted_lanes(lanes), by_id(), fetch)
}

/// Each run in a lane of its own.
fn one_per_lane(runs: Vec<RecordBatch>) -> Vec<Vec<RecordBatch>> {
    runs.into_iter().map(|run| vec![run]).collect()
}

operator_case! {
    GpuMergeSortedPartitions,
    fn three_sorted_lanes_merge_into_one() {
        let lanes = one_per_lane(runs(48, 1, 3));
        run_both(&merged(3, None), Script::Lanes(lanes)).same(Order::AsEmitted);
    }
}

operator_case! {
    GpuMergeSortedPartitions,
    fn two_batches_per_lane_all_go_into_one_merge() {
        let [a, b, c, d]: [RecordBatch; 4] = runs(48, 1, 4).try_into().expect("four runs");
        let lanes = vec![vec![a, b], vec![c, d]];
        run_both(&merged(2, None), Script::Lanes(lanes)).same(Order::AsEmitted);
    }
}

operator_case! {
    GpuMergeSortedPartitions,
    fn a_fetch_cuts_the_merged_lanes() {
        let lanes = one_per_lane(runs(32, 1, 2));
        run_both(&merged(2, Some(7)), Script::Lanes(lanes)).same(Order::AsEmitted);
    }
}

operator_case! {
    GpuMergeSortedPartitions,
    fn an_empty_lane_is_skipped_by_both() {
        let [first, second]: [RecordBatch; 2] = runs(24, 1, 2).try_into().expect("two runs");
        let lanes = vec![vec![first], Vec::new(), vec![second]];
        run_both(&merged(3, None), Script::Lanes(lanes)).same(Order::AsEmitted);
    }
}

operator_case! {
    GpuMergeSortedPartitions,
    fn a_lane_done_before_any_batch_is_skipped_by_the_merge() {
        let [first, second]: [RecordBatch; 2] = runs(24, 1, 2).try_into().expect("two runs");
        let lanes = vec![Vec::new(), vec![first], vec![second]];
        run_both(&merged(3, None), Script::Lanes(lanes)).same(Order::AsEmitted);
    }
}

// #204 — one populated lane is one input to the device's merge, and its fetch goes
// unapplied: the cpu answers the top 5 at the last `Done`, the device all 16.
operator_case! {
    GpuMergeSortedPartitions,
    fn bug_a_fetch_over_one_populated_lane_is_not_applied_on_the_device() {
        let batch = synthetic(16, 1);
        let lanes = vec![vec![batch.clone()], Vec::new()];
        let outcome = run_both(&merged(2, Some(5)), Script::Lanes(lanes));
        each_answers(
            &outcome,
            &[Vec::new(), Vec::new(), vec![batch.slice(0, 5)]],
            &[Vec::new(), Vec::new(), vec![batch]],
        );
    }
}

// Empty inputs, each its own case. #173's site: every lane `Done` with nothing is a merge
// of no runs.
operator_case! {
    GpuMergeSortedPartitions,
    fn every_lane_done_with_nothing_is_nothing_on_both() {
        let lanes = vec![Vec::new(), Vec::new()];
        run_both(&merged(2, None), Script::Lanes(lanes)).same(Order::AsEmitted);
    }
}

operator_case! {
    GpuMergeSortedPartitions,
    fn a_zero_row_lane_beside_lanes_with_rows_is_skipped() {
        let [first, second]: [RecordBatch; 2] = runs(24, 1, 2).try_into().expect("two runs");
        let lanes = vec![vec![first], vec![synthetic(0, 2)], vec![second]];
        run_both(&merged(3, None), Script::Lanes(lanes)).same(Order::AsEmitted);
    }
}

// #205 — the merge's sort over zero rows yields no batch on the cpu too; the device
// answers zero rows at the last `Done`.
operator_case! {
    GpuMergeSortedPartitions,
    fn bug_every_lane_a_zero_row_batch_is_nothing_on_the_cpu() {
        let lanes = vec![vec![synthetic(0, 1)], vec![synthetic(0, 2)]];
        let outcome = run_both(&merged(2, None), Script::Lanes(lanes));
        each_answers(
            &outcome,
            &[Vec::new(), Vec::new(), Vec::new(), Vec::new()],
            &[Vec::new(), Vec::new(), Vec::new(), vec![synthetic(0, 1)]],
        );
    }
}

operator_case! {
    GpuMergeSortedPartitions,
    fn lane_zero_done_before_lane_one_arrives() {
        let lanes = vec![Vec::new(), vec![synthetic(16, 2)]];
        run_both(&merged(2, None), Script::Lanes(lanes)).same(Order::AsEmitted);
    }
}
