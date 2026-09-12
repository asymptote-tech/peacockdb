//! The seqless three, and the helper round trip — the cases whose recipe puts nothing on
//! the wire, so a wrong helper has nowhere to hide.

use datafusion::arrow::record_batch::RecordBatch;

use super::device::Device;
use super::script::{Script, run_both};
use crate::executor::RowRange;
use crate::plan::{BatchLayout, GpuLimit, GpuUnload, RowInterval, Schema};
use crate::tests::compare::{Order, assert_same};
use crate::tests::given::Given;
use crate::tests::synthetic::synthetic;

/// Any node over a given leaf opens a session; the unload's is the emptiest plan there is.
fn empty_plan() -> GpuUnload {
    GpuUnload::new(
        Given::of(
            Schema::new(synthetic(0, 0).schema()),
            BatchLayout::MultipleBatches,
        ),
        None,
    )
}

/// The context an executor is built from carries the plan the session loaded: an unload
/// over a given leaf is one stub node, and `open` checked the device counted the same.
#[test]
fn the_session_holds_the_plan_of_one_stub_it_loaded() {
    let device = Device::open(&empty_plan());
    assert_eq!(device.ctx().recipes.wire_nodes(), 1);
    assert!(
        device.ctx().recipes.get(0).is_none(),
        "the leaf has no recipe"
    );
    assert!(device.ctx().recipes.get(1).is_some(), "the unload has one");
}

#[test]
fn an_uploaded_batch_comes_back_as_itself() {
    let device = Device::open(&empty_plan());
    let batch = synthetic(64, 1);
    let back = device
        .fetch(device.upload(&batch), RowRange::WHOLE)
        .expect("a whole export ships");
    assert_same(&[vec![batch]], &[vec![back]], Order::AsEmitted);
}

#[test]
fn a_row_range_ships_those_rows_in_order() {
    let device = Device::open(&empty_plan());
    let batch = synthetic(64, 1);
    let back = device
        .fetch(
            device.upload(&batch),
            RowRange {
                offset: 10,
                length: 5,
            },
        )
        .expect("five rows ship");
    assert_same(&[vec![batch.slice(10, 5)]], &[vec![back]], Order::AsEmitted);
}

#[test]
fn a_range_past_the_end_ships_nothing() {
    let device = Device::open(&empty_plan());
    let back = device.fetch(
        device.upload(&synthetic(8, 1)),
        RowRange {
            offset: 8,
            length: 1,
        },
    );
    assert!(back.is_none());
}

#[test]
fn a_range_over_the_end_is_clamped() {
    let device = Device::open(&empty_plan());
    let batch = synthetic(8, 1);
    let back = device
        .fetch(
            device.upload(&batch),
            RowRange {
                offset: 6,
                length: 100,
            },
        )
        .expect("two rows ship");
    assert_same(&[vec![batch.slice(6, 2)]], &[vec![back]], Order::AsEmitted);
}

#[test]
fn zero_rows_round_trip_as_zero_rows_under_the_schema() {
    let device = Device::open(&empty_plan());
    let batch = synthetic(0, 1);
    let back = device
        .fetch(device.upload(&batch), RowRange::WHOLE)
        .expect("the schema ships");
    assert_same(&[vec![batch]], &[vec![back]], Order::AsEmitted);
}

// `GpuUnload` through `executors_for` on both backends: `CpuUnload`'s slice against
// `GpuExport`'s row range, the first operator through `run_both`.

fn unload_over(rows: usize) -> (GpuUnload, RecordBatch) {
    let batch = synthetic(rows, 2);
    let node = GpuUnload::new(
        Given::of(Schema::new(batch.schema()), BatchLayout::MultipleBatches),
        None,
    );
    (node, batch)
}

#[test]
#[should_panic(expected = "the script's shape is not the node's category")]
fn a_script_of_another_shape_is_refused_before_either_backend_runs() {
    let (node, batch) = unload_over(4);
    run_both(&node, Script::Exec(vec![batch]));
}

#[test]
fn an_unload_hands_the_whole_batch_over_on_both_backends() {
    let (node, batch) = unload_over(64);
    run_both(
        &node,
        Script::Unload {
            batch,
            rows: RowRange::WHOLE,
        },
    )
    .same(Order::AsEmitted);
}

#[test]
fn an_unload_over_a_range_hands_those_rows_over() {
    let (node, batch) = unload_over(64);
    run_both(
        &node,
        Script::Unload {
            batch,
            rows: RowRange {
                offset: 20,
                length: 7,
            },
        },
    )
    .same(Order::AsEmitted);
}

#[test]
fn an_unload_clamps_a_range_over_the_end_the_same_way() {
    let (node, batch) = unload_over(64);
    run_both(
        &node,
        Script::Unload {
            batch,
            rows: RowRange {
                offset: 60,
                length: 100,
            },
        },
    )
    .same(Order::AsEmitted);
}

#[test]
fn an_unload_of_a_range_past_the_end_is_zero_rows_on_both() {
    let (node, batch) = unload_over(8);
    run_both(
        &node,
        Script::Unload {
            batch,
            rows: RowRange {
                offset: 8,
                length: 4,
            },
        },
    )
    .same(Order::AsEmitted);
}

// Empty inputs, each its own case.
#[test]
fn an_unload_of_a_zero_row_batch_is_zero_rows_under_the_schema_on_both() {
    let (node, batch) = unload_over(0);
    run_both(
        &node,
        Script::Unload {
            batch,
            rows: RowRange::WHOLE,
        },
    )
    .same(Order::AsEmitted);
}

#[test]
fn a_range_over_a_zero_row_batch_is_zero_rows_on_both() {
    let (node, batch) = unload_over(0);
    run_both(
        &node,
        Script::Unload {
            batch,
            rows: RowRange {
                offset: 0,
                length: 5,
            },
        },
    )
    .same(Order::AsEmitted);
}

// `GpuLimit` through `executors_for`: the mid-plan limit streams and holds nothing, so
// one slot per batch and an empty one at done — a batch outside the interval is a slot
// both sides leave empty, which is not a zero-row batch.

/// A limit over a stream of `batches` batches of `rows` rows each.
fn limit_over(
    skip: u64,
    fetch: Option<u64>,
    rows: usize,
    batches: usize,
) -> (GpuLimit, Vec<RecordBatch>) {
    let stream: Vec<RecordBatch> = (0..batches)
        .map(|i| synthetic(rows, 10 + i as u64))
        .collect();
    let node = GpuLimit::new(
        Given::of(
            Schema::new(stream[0].schema()),
            BatchLayout::MultipleBatches,
        ),
        RowInterval { skip, fetch },
    );
    (node, stream)
}

#[test]
fn an_interval_inside_one_batch_slices_that_batch() {
    let (node, stream) = limit_over(3, Some(4), 16, 1);
    run_both(&node, Script::Accumulate(stream)).same(Order::AsEmitted);
}

#[test]
fn an_interval_straddling_two_batches_slices_both() {
    let (node, stream) = limit_over(12, Some(8), 16, 2);
    run_both(&node, Script::Accumulate(stream)).same(Order::AsEmitted);
}

#[test]
fn batches_entirely_outside_the_interval_produce_nothing() {
    let (node, stream) = limit_over(40, Some(4), 16, 4);
    run_both(&node, Script::Accumulate(stream)).same(Order::AsEmitted);
}

#[test]
fn a_skip_alone_drops_the_prefix_and_keeps_the_rest() {
    let (node, stream) = limit_over(20, None, 16, 3);
    run_both(&node, Script::Accumulate(stream)).same(Order::AsEmitted);
}

#[test]
fn a_stream_of_several_batches_is_cut_at_the_same_two_edges() {
    let (node, stream) = limit_over(5, Some(30), 8, 6);
    run_both(&node, Script::Accumulate(stream)).same(Order::AsEmitted);
}

// Empty inputs, each its own case.
#[test]
fn a_stream_of_one_zero_row_batch_answers_nothing_on_both() {
    let (node, stream) = limit_over(0, Some(4), 0, 1);
    run_both(&node, Script::Accumulate(stream)).same(Order::AsEmitted);
}

#[test]
fn a_zero_row_batch_inside_a_stream_counts_no_rows_on_both() {
    let (node, mut stream) = limit_over(10, Some(10), 8, 3);
    stream.insert(1, synthetic(0, 99));
    run_both(&node, Script::Accumulate(stream)).same(Order::AsEmitted);
}

#[test]
fn a_stream_of_nothing_but_zero_row_batches_answers_nothing_on_both() {
    let (node, stream) = limit_over(2, Some(4), 0, 3);
    run_both(&node, Script::Accumulate(stream)).same(Order::AsEmitted);
}

#[test]
fn an_interval_no_batch_reaches_answers_nothing_on_both() {
    let (node, stream) = limit_over(1000, Some(4), 8, 3);
    run_both(&node, Script::Accumulate(stream)).same(Order::AsEmitted);
}
