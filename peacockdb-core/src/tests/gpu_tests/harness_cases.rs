//! The seqless three, and the helper round trip — the cases whose recipe puts nothing on
//! the wire, so a wrong helper has nowhere to hide.

use super::device::Device;
use crate::executor::RowRange;
use crate::plan::{BatchLayout, GpuUnload, Schema};
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
