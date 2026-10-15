//! `GpuEmitPartitions` through the harness: one batch in, N lanes out, each lane a slot of
//! its own — so a row the two engines put in different lanes is a wrong slot, not a passing
//! multiset. One case per key type the planner can emit, over `key_types`, since a key type
//! with no case is a key type no two-engine run has agreed on. Every case is green or a
//! `bug_` test with its ticket above it; nothing here repairs.

use std::sync::Arc;

use datafusion::arrow::array::{ArrayRef, Int32Array, new_null_array};
use datafusion::arrow::datatypes::DataType;
use datafusion::arrow::record_batch::RecordBatch;

use super::script::{Script, run_both};
use crate::plan::{BatchLayout, GpuEmitPartitions, GpuNode, Schema};
use crate::tests::compare::Order;
use crate::tests::given::Given;
use crate::tests::synthetic::{key_types, key_types_schema, schema, synthetic};

fn input() -> RecordBatch {
    synthetic(64, 1)
}

fn given() -> Box<dyn GpuNode> {
    Given::of(Schema::new(schema()), BatchLayout::MultipleBatches)
}

pub(crate) fn emit(keys: Vec<u32>, lanes: usize) -> GpuEmitPartitions {
    GpuEmitPartitions::new(given(), keys, lanes)
}

/// `input()` with its `key` column replaced.
fn with_key(key: ArrayRef) -> RecordBatch {
    let batch = input();
    let mut columns = batch.columns().to_vec();
    columns[1] = key;
    RecordBatch::try_new(batch.schema(), columns).expect("one Int32 column for another")
}

operator_case! {
    GpuEmitPartitions,
    fn four_lanes_on_an_int_key_place_every_row_the_same() {
        run_both(&emit(vec![1], 4), Script::Emit(vec![input()])).same(Order::Any);
    }
}

operator_case! {
    GpuEmitPartitions,
    fn sixty_four_lanes_leave_most_empty_and_agree_on_all() {
        run_both(&emit(vec![1], 64), Script::Emit(vec![synthetic(8, 1)])).same(Order::Any);
    }
}

// Every other row's key null, the rest spread over the lanes: a null key hashes to the
// seed's lane, so one slot holds all thirty-two null rows and no other slot holds one —
// which lane it is, the murmur conformance gate says.
operator_case! {
    GpuEmitPartitions,
    fn null_keys_land_in_the_same_lane() {
        let half_null: ArrayRef = Arc::new(Int32Array::from_iter(
            (0..64).map(|row| (row % 2 == 1).then_some(row)),
        ));
        let outcome = run_both(&emit(vec![1], 4), Script::Emit(vec![with_key(half_null)]));
        outcome.same(Order::Any);
        let null_rows_per_lane: Vec<usize> = outcome
            .cpu
            .as_ref()
            .expect("the cpu answers")
            .iter()
            .map(|lane| lane[0].column(1).null_count())
            .collect();
        assert_eq!(null_rows_per_lane.iter().sum::<usize>(), 32);
        assert_eq!(
            null_rows_per_lane.iter().filter(|n| **n > 0).count(),
            1,
            "{null_rows_per_lane:?}"
        );
    }
}

operator_case! {
    GpuEmitPartitions,
    fn two_keys_combine_the_same_way() {
        run_both(&emit(vec![1, 5], 4), Script::Emit(vec![input()])).same(Order::Any);
    }
}

operator_case! {
    GpuEmitPartitions,
    fn a_string_key_places_every_row_the_same() {
        run_both(&emit(vec![5], 4), Script::Emit(vec![input()])).same(Order::Any);
    }
}

operator_case! {
    GpuEmitPartitions,
    fn an_int64_key_places_every_row_the_same() {
        run_both(&emit(vec![3], 4), Script::Emit(vec![input()])).same(Order::Any);
    }
}

operator_case! {
    GpuEmitPartitions,
    fn a_date_key_places_every_row_the_same() {
        run_both(&emit(vec![6], 4), Script::Emit(vec![input()])).same(Order::Any);
    }
}

operator_case! {
    GpuEmitPartitions,
    fn each_batch_is_scattered_on_its_own() {
        let stream = vec![synthetic(16, 1), synthetic(16, 3)];
        run_both(&emit(vec![1], 4), Script::Emit(stream)).same(Order::Any);
    }
}

// Empty inputs, each its own case. `emit` answers N lanes whatever it was handed — the
// driver checks the count — so a zero-row batch in is N slots out, and how each side fills
// them is the assertion.
operator_case! {
    GpuEmitPartitions,
    fn a_zero_row_batch_scatters_into_n_zero_row_lanes() {
        let outcome = run_both(&emit(vec![1], 4), Script::Emit(vec![synthetic(0, 1)]));
        outcome.same(Order::Any);
        assert_eq!(outcome.cpu.as_ref().expect("the cpu answers").len(), 4);
    }
}

// One key value everywhere: every row lands in one lane and three lanes get nothing.
operator_case! {
    GpuEmitPartitions,
    fn a_batch_of_one_key_leaves_n_minus_one_lanes_empty() {
        let one_key: ArrayRef = Arc::new(Int32Array::from(vec![Some(3); 64]));
        run_both(&emit(vec![1], 4), Script::Emit(vec![with_key(one_key)])).same(Order::Any);
    }
}

// Every key null: one lane, `pmod(seed, N)`, and the other lanes empty.
operator_case! {
    GpuEmitPartitions,
    fn a_batch_of_all_null_keys_lands_in_one_lane() {
        let nulls = new_null_array(&DataType::Int32, 64);
        run_both(&emit(vec![1], 4), Script::Emit(vec![with_key(nulls)])).same(Order::Any);
    }
}

operator_case! {
    GpuEmitPartitions,
    fn a_stream_of_zero_row_rows_zero_row_is_scattered_per_batch() {
        let stream = vec![synthetic(0, 1), synthetic(16, 2), synthetic(0, 3)];
        run_both(&emit(vec![1], 4), Script::Emit(stream)).same(Order::Any);
    }
}

// --- one case per key type -----------------------------------------------------
//
// `key_types`' column order is the ordinal each case names; the batch carries a null in every
// key column, both zeros and both NaN signs in the floats, values past the signed maximum in
// the unsigned pair, and values past i64 in `dec38`.

fn key_types_given() -> Box<dyn GpuNode> {
    Given::of(
        Schema::new(key_types_schema()),
        BatchLayout::MultipleBatches,
    )
}

pub(crate) fn key_types_emit(keys: Vec<u32>, lanes: usize) -> GpuEmitPartitions {
    GpuEmitPartitions::new(key_types_given(), keys, lanes)
}

/// Both engines put every row of a scatter on `key` in the same lane.
fn every_row_in_the_same_lane_on(key: u32) {
    run_both(
        &key_types_emit(vec![key], 4),
        Script::Emit(vec![key_types(96, 5)]),
    )
    .same(Order::Any);
}

operator_case! {
    GpuEmitPartitions,
    fn an_int8_key_places_every_row_the_same() { every_row_in_the_same_lane_on(1) }
}

operator_case! {
    GpuEmitPartitions,
    fn an_int16_key_places_every_row_the_same() { every_row_in_the_same_lane_on(2) }
}

operator_case! {
    GpuEmitPartitions,
    fn an_int32_key_places_every_row_the_same() { every_row_in_the_same_lane_on(3) }
}

operator_case! {
    GpuEmitPartitions,
    fn an_int64_key_in_the_key_type_set_places_every_row_the_same() {
        every_row_in_the_same_lane_on(4)
    }
}

// #206, and the one place the rule departs from comet: every NaN is canonicalized to a single
// bit pattern before hashing, where comet hashes a float's raw bytes. That restores Spark, whose
// doubleToLongBits already collapses NaNs. -0.0 hashing as +0.0 is comet's rule and Spark's, and
// is not a departure.
operator_case! {
    GpuEmitPartitions,
    fn a_float32_key_places_every_row_the_same() { every_row_in_the_same_lane_on(5) }
}

operator_case! {
    GpuEmitPartitions,
    fn a_float64_key_places_every_row_the_same() { every_row_in_the_same_lane_on(6) }
}

// #206 — a boolean hashes as comet's i32, 0 or 1.
operator_case! {
    GpuEmitPartitions,
    fn a_boolean_key_places_every_row_the_same() { every_row_in_the_same_lane_on(7) }
}

operator_case! {
    GpuEmitPartitions,
    fn a_date32_key_in_the_key_type_set_places_every_row_the_same() {
        every_row_in_the_same_lane_on(8)
    }
}

// #240 — every unit as its i64, which is the same bytes, so the unit cannot change a lane.
operator_case! {
    GpuEmitPartitions,
    fn a_timestamp_second_key_places_every_row_the_same() { every_row_in_the_same_lane_on(9) }
}

operator_case! {
    GpuEmitPartitions,
    fn a_timestamp_millisecond_key_places_every_row_the_same() {
        every_row_in_the_same_lane_on(10)
    }
}

operator_case! {
    GpuEmitPartitions,
    fn a_timestamp_microsecond_key_places_every_row_the_same() {
        every_row_in_the_same_lane_on(11)
    }
}

operator_case! {
    GpuEmitPartitions,
    fn a_timestamp_nanosecond_key_places_every_row_the_same() {
        every_row_in_the_same_lane_on(12)
    }
}

// #95 — 16 bytes on both engines at either precision, the cpu widening to (38, s) first.
operator_case! {
    GpuEmitPartitions,
    fn a_narrow_decimal_key_places_every_row_the_same() { every_row_in_the_same_lane_on(13) }
}

operator_case! {
    GpuEmitPartitions,
    fn a_wide_decimal_key_places_every_row_the_same() { every_row_in_the_same_lane_on(14) }
}

operator_case! {
    GpuEmitPartitions,
    fn a_utf8_key_in_the_key_type_set_places_every_row_the_same() {
        every_row_in_the_same_lane_on(15)
    }
}

// Spark has no unsigned types, so the widening is our rule on both engines: u8/u16 to i32,
// u32 to i64 by value, u64 to i64 by its bits.
operator_case! {
    GpuEmitPartitions,
    fn a_uint8_key_places_every_row_the_same() { every_row_in_the_same_lane_on(16) }
}

operator_case! {
    GpuEmitPartitions,
    fn a_uint16_key_places_every_row_the_same() { every_row_in_the_same_lane_on(17) }
}

operator_case! {
    GpuEmitPartitions,
    fn a_uint32_key_places_every_row_the_same() { every_row_in_the_same_lane_on(18) }
}

operator_case! {
    GpuEmitPartitions,
    fn a_uint64_key_places_every_row_the_same() { every_row_in_the_same_lane_on(19) }
}

// A float, a wide decimal, a string and a date in one key: the running seed has to chain
// across four different encodings, which no single-type case can show.
operator_case! {
    GpuEmitPartitions,
    fn a_mixed_composite_key_places_every_row_the_same() {
        let node = key_types_emit(vec![6, 14, 15, 8], 4);
        run_both(&node, Script::Emit(vec![key_types(96, 5)])).same(Order::Any);
    }
}

// The normalizing cast runs on zero rows for bool, every timestamp unit, the decimals and
// the unsigned four, and the scatter still answers N empty lanes.
operator_case! {
    GpuEmitPartitions,
    fn a_zero_row_batch_scatters_on_every_key_type() {
        for key in 1..20 {
            let node = key_types_emit(vec![key], 4);
            let outcome = run_both(&node, Script::Emit(vec![key_types(0, 1)]));
            outcome.same(Order::Any);
            assert_eq!(outcome.cpu.as_ref().expect("the cpu answers").len(), 4, "key {key}");
        }
    }
}
