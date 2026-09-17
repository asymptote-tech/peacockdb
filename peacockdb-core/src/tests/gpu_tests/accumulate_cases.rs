//! `GpuCoalesceAllBatches`, `GpuAccumulateBatchesAndSort` and `GpuMergeSortedPartitions`
//! through the harness: a lane's batches or N lanes' worth, both backends, the slots
//! compared as emitted. Every case is green or a `bug_` test with its ticket above it;
//! nothing here repairs. A sorted run is a slice of one synthetic batch dealt round-robin,
//! so no `id` is in two runs and a merge of them interleaves every one.

use datafusion::arrow::array::UInt32Array;
use datafusion::arrow::compute::{SortColumn, SortOptions, lexsort_to_indices, take_record_batch};
use datafusion::arrow::record_batch::RecordBatch;

use super::script::{Script, each_answers, run_both};
use crate::plan::{
    ColumnOrder, GpuAccumulateBatchesAndSort, GpuCoalesceAllBatches, GpuMergeSortedPartitions,
    GpuNode, PartitionLayout, Schema, SortOrder,
};
use crate::tests::compare::Order;
use crate::tests::given::Given;
use crate::tests::synthetic::{schema, synthetic};

fn by(column: u32, ascending: bool, nulls_first: bool) -> ColumnOrder {
    ColumnOrder {
        column,
        ascending,
        nulls_first,
    }
}

fn by_id() -> Vec<ColumnOrder> {
    vec![by(0, true, false)]
}

/// `batch` ordered by arrow under `order`: what a run sorted upstream looks like, and what a
/// merge of such runs owes.
fn ordered(batch: &RecordBatch, order: &[ColumnOrder]) -> RecordBatch {
    let keys: Vec<SortColumn> = order
        .iter()
        .map(|key| SortColumn {
            values: batch.column(key.column as usize).clone(),
            options: Some(SortOptions {
                descending: !key.ascending,
                nulls_first: key.nulls_first,
            }),
        })
        .collect();
    let indices = lexsort_to_indices(&keys, None).expect("every synthetic type sorts");
    take_record_batch(batch, &indices).expect("a permutation of the batch")
}

/// `synthetic(rows, seed)` dealt into `n` runs, row `r` to run `r % n`: each run is sorted
/// by `id`, no `id` is in two, and every run has rows a merge must interleave.
pub(crate) fn runs(rows: usize, seed: u64, n: usize) -> Vec<RecordBatch> {
    runs_ordered(rows, seed, n, &by_id())
}

/// `runs`, each run sorted by `order` instead, so the input is what the node declares.
fn runs_ordered(rows: usize, seed: u64, n: usize, order: &[ColumnOrder]) -> Vec<RecordBatch> {
    let whole = synthetic(rows, seed);
    (0..n)
        .map(|run| {
            let rows = UInt32Array::from_iter_values((run..rows).step_by(n).map(|r| r as u32));
            let run = take_record_batch(&whole, &rows).expect("a selection of the batch");
            ordered(&run, order)
        })
        .collect()
}

/// `n` lanes whose batches are each sorted by `order`, which is what a `GpuSort` above a
/// scan declares and what both accumulating sorts require of their input.
fn lanes_ordered(n: usize, order: Vec<ColumnOrder>) -> Box<dyn GpuNode> {
    Given::with_layout(
        Schema::new(schema()),
        PartitionLayout {
            sort_order: SortOrder::batch_sorted(order),
            ..PartitionLayout::new(n)
        },
    )
}

// `GpuCoalesceAllBatches`.

pub(crate) fn coalesce() -> GpuCoalesceAllBatches {
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

pub(crate) fn sorted(fetch: Option<usize>) -> GpuAccumulateBatchesAndSort {
    sorted_by(by_id(), fetch)
}

fn sorted_by(order: Vec<ColumnOrder>, fetch: Option<usize>) -> GpuAccumulateBatchesAndSort {
    GpuAccumulateBatchesAndSort::new(lanes_ordered(1, order.clone()), order, fetch)
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

// The keys the corpus merges on: descending, nullable with the nulls at either end, two
// of them. `key` carries the nulls and `id` behind it keeps the order total.

fn key_descending_nulls_first() -> Vec<ColumnOrder> {
    vec![by(1, false, true), by(0, true, false)]
}

operator_case! {
    GpuAccumulateBatchesAndSort,
    fn an_accumulating_sort_on_a_descending_key_agrees() {
        let order = vec![by(0, false, false)];
        let runs = runs_ordered(48, 1, 3, &order);
        run_both(&sorted_by(order, None), Script::Accumulate(runs)).same(Order::AsEmitted);
    }
}

/// Forty-four rows in eleven runs, so every null `key` — row 10, 21, 32, 43 — is in the last
/// run alone and each run is sorted under either null order. That is the one shape where
/// the partition merge, whose comparator puts a descending key's nulls at the wrong end
/// (#202), still has inputs sorted as it expects and answers a permutation; over runs each
/// carrying a null, cuDF's merge precondition is broken and it answers the rows the pin
/// below the merge cases states. The accumulating sort re-sorts each batch on the device
/// before it merges, so its inputs hold under any run shape; it takes the same runs for
/// the same expected batch.
fn null_keys_in_one_run(order: &[ColumnOrder]) -> Vec<RecordBatch> {
    runs_ordered(44, 1, 11, order)
}

/// The device's answer under #202: the same rows, the descending key's nulls at the end
/// the plan did not declare.
fn nulls_flipped(order: &[ColumnOrder]) -> Vec<ColumnOrder> {
    order
        .iter()
        .map(|key| by(key.column, key.ascending, !key.nulls_first))
        .collect()
}

// #202 — the merge in `node_session.cpp` maps `nulls_first` as `sort.cpp` does; the cpu
// answers the nulls first, the device last.
operator_case! {
    GpuAccumulateBatchesAndSort,
    fn bug_an_accumulating_sort_on_a_descending_key_nulls_first_puts_them_last_on_the_device() {
        let order = key_descending_nulls_first();
        let runs = null_keys_in_one_run(&order);
        let outcome = run_both(&sorted_by(order.clone(), None), Script::Accumulate(runs));
        let whole = synthetic(44, 1);
        let mut cpu = vec![Vec::new(); 11];
        cpu.push(vec![ordered(&whole, &order)]);
        let mut gpu = vec![Vec::new(); 11];
        gpu.push(vec![ordered(&whole, &nulls_flipped(&order))]);
        each_answers(&outcome, &cpu, &gpu);
    }
}

operator_case! {
    GpuAccumulateBatchesAndSort,
    fn an_accumulating_sort_on_a_nullable_key_nulls_first_agrees() {
        let order = vec![by(1, true, true), by(0, true, false)];
        let runs = runs_ordered(48, 1, 3, &order);
        run_both(&sorted_by(order, None), Script::Accumulate(runs)).same(Order::AsEmitted);
    }
}

operator_case! {
    GpuAccumulateBatchesAndSort,
    fn an_accumulating_sort_on_a_nullable_key_nulls_last_agrees() {
        let order = vec![by(1, true, false), by(0, true, false)];
        let runs = runs_ordered(48, 1, 3, &order);
        run_both(&sorted_by(order, None), Script::Accumulate(runs)).same(Order::AsEmitted);
    }
}

operator_case! {
    GpuAccumulateBatchesAndSort,
    fn an_accumulating_sort_on_two_keys_agrees() {
        let order = vec![by(7, true, false), by(0, true, false)];
        let runs = runs_ordered(48, 1, 3, &order);
        run_both(&sorted_by(order, None), Script::Accumulate(runs)).same(Order::AsEmitted);
    }
}

// `GpuMergeSortedPartitions`. `Script::Lanes` drives every lane in order, each lane's
// batches then its `Done`, so lane 0 is always done before lane 1's rows arrive.

pub(crate) fn merged(lanes: usize, fetch: Option<usize>) -> GpuMergeSortedPartitions {
    merged_by(lanes, by_id(), fetch)
}

fn merged_by(
    lanes: usize,
    order: Vec<ColumnOrder>,
    fetch: Option<usize>,
) -> GpuMergeSortedPartitions {
    GpuMergeSortedPartitions::new(lanes_ordered(lanes, order.clone()), order, fetch)
}

/// Each run in a lane of its own.
pub(crate) fn one_per_lane(runs: Vec<RecordBatch>) -> Vec<Vec<RecordBatch>> {
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

// The same keys through the partition merge, and four lanes.

operator_case! {
    GpuMergeSortedPartitions,
    fn a_merge_on_a_descending_key_agrees() {
        let order = vec![by(0, false, false)];
        let lanes = one_per_lane(runs_ordered(48, 1, 3, &order));
        run_both(&merged_by(3, order, None), Script::Lanes(lanes)).same(Order::AsEmitted);
    }
}

// #202 — the same site through the partition merge: eleven lanes, a batch and a done each.
operator_case! {
    GpuMergeSortedPartitions,
    fn bug_a_merge_on_a_descending_key_nulls_first_puts_them_last_on_the_device() {
        let order = key_descending_nulls_first();
        let lanes = one_per_lane(null_keys_in_one_run(&order));
        let outcome = run_both(&merged_by(11, order.clone(), None), Script::Lanes(lanes));
        let whole = synthetic(44, 1);
        let mut cpu = vec![Vec::new(); 21];
        cpu.push(vec![ordered(&whole, &order)]);
        let mut gpu = vec![Vec::new(); 21];
        gpu.push(vec![ordered(&whole, &nulls_flipped(&order))]);
        each_answers(&outcome, &cpu, &gpu);
    }
}

// #202 — the same key over three runs dealt round-robin, each carrying a null: the runs are
// sorted as the plan says and not as the device's merge reads them, cuDF's merge precondition
// is broken, and the device answers 48 rows with three ids three times each and six ids never
// — the rows it answered, read off the device, by `id`. No plan reaches this shape today
// (every run the merge sees was sorted by the same mapping); it is why the two sites in #202
// move together. Deterministic, so pinned as it is; the cpu answers the merge.
operator_case! {
    GpuMergeSortedPartitions,
    fn bug_a_merge_on_a_descending_key_nulls_first_over_runs_each_carrying_a_null_duplicates_and_drops_rows_on_the_device() {
        let order = key_descending_nulls_first();
        let lanes = one_per_lane(runs_ordered(48, 1, 3, &order));
        let outcome = run_both(&merged_by(3, order.clone(), None), Script::Lanes(lanes));
        let whole = synthetic(48, 1);
        let answered = UInt32Array::from(vec![
            10, 21, 0, 33, 39, 18, 45, 39, 5, 18, 29, 45, 39, 18, 45, 47, 16, 23, 34, 36, 38, 8,
            15, 31, 40, 44, 1, 6, 12, 27, 37, 42, 3, 9, 11, 20, 35, 41, 46, 2, 4, 7, 13, 24, 25,
            26, 28, 30,
        ]);
        let mut cpu = vec![Vec::new(); 5];
        cpu.push(vec![ordered(&whole, &order)]);
        let mut gpu = vec![Vec::new(); 5];
        gpu.push(vec![take_record_batch(&whole, &answered).expect("ids of the batch")]);
        each_answers(&outcome, &cpu, &gpu);
    }
}

operator_case! {
    GpuMergeSortedPartitions,
    fn a_merge_on_a_nullable_key_nulls_first_agrees() {
        let order = vec![by(1, true, true), by(0, true, false)];
        let lanes = one_per_lane(runs_ordered(48, 1, 3, &order));
        run_both(&merged_by(3, order, None), Script::Lanes(lanes)).same(Order::AsEmitted);
    }
}

operator_case! {
    GpuMergeSortedPartitions,
    fn a_merge_on_a_nullable_key_nulls_last_agrees() {
        let order = vec![by(1, true, false), by(0, true, false)];
        let lanes = one_per_lane(runs_ordered(48, 1, 3, &order));
        run_both(&merged_by(3, order, None), Script::Lanes(lanes)).same(Order::AsEmitted);
    }
}

operator_case! {
    GpuMergeSortedPartitions,
    fn a_merge_on_two_keys_agrees() {
        let order = vec![by(7, true, false), by(0, true, false)];
        let lanes = one_per_lane(runs_ordered(48, 1, 3, &order));
        run_both(&merged_by(3, order, None), Script::Lanes(lanes)).same(Order::AsEmitted);
    }
}

operator_case! {
    GpuMergeSortedPartitions,
    fn four_sorted_lanes_merge_into_one() {
        let lanes = one_per_lane(runs(64, 1, 4));
        run_both(&merged(4, None), Script::Lanes(lanes)).same(Order::AsEmitted);
    }
}

// The empty shape on a descending key: one lane empty beside lanes with rows.
operator_case! {
    GpuMergeSortedPartitions,
    fn a_descending_merge_with_one_lane_empty_beside_lanes_with_rows_agrees() {
        let order = vec![by(0, false, false)];
        let [first, second]: [RecordBatch; 2] = runs_ordered(24, 1, 2, &order).try_into().expect("two runs");
        let lanes = vec![vec![first], Vec::new(), vec![second]];
        run_both(&merged_by(3, order, None), Script::Lanes(lanes)).same(Order::AsEmitted);
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
