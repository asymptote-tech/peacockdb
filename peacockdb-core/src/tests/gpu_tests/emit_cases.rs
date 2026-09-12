//! `GpuEmitPartitions` through the harness: one batch in, N lanes out, each lane a slot of
//! its own — so a row the two engines put in different lanes is a wrong slot, not a passing
//! multiset. Every case is one lane in and N out, the shape #184 names. Every case is green
//! or a `bug_` test with its ticket above it; nothing here repairs.

use std::sync::Arc;

use datafusion::arrow::array::{ArrayRef, Int32Array, new_null_array};
use datafusion::arrow::datatypes::DataType;
use datafusion::arrow::record_batch::RecordBatch;

use super::script::{Outcome, Script, run_both};
use crate::plan::{BatchLayout, GpuEmitPartitions, GpuNode, Schema};
use crate::tests::compare::Order;
use crate::tests::given::Given;
use crate::tests::synthetic::{decimals, schema, synthetic};

fn input() -> RecordBatch {
    synthetic(64, 1)
}

fn given() -> Box<dyn GpuNode> {
    Given::of(Schema::new(schema()), BatchLayout::MultipleBatches)
}

fn emit(keys: Vec<u32>, lanes: usize) -> GpuEmitPartitions {
    GpuEmitPartitions::new(given(), keys, lanes)
}

/// A `bug_` test's assertion: the device refused at the kernel's key-type switch, naming
/// this cuDF `type_id`, and the cpu answered.
fn refused_key_type(outcome: &Outcome, type_id: u32) {
    let why = outcome.gpu_refuses();
    assert!(
        why.contains("spark_hash_partition.cu")
            && why.contains(&format!("unsupported key column cuDF type_id={type_id}")),
        "{why}"
    );
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

// #206 — the kernel's type switch has no arm for a double (cuDF `FLOAT64`, 10); comet
// hashes it on the cpu.
operator_case! {
    GpuEmitPartitions,
    fn bug_a_float_key_is_refused_on_the_device() {
        let outcome = run_both(&emit(vec![4], 4), Script::Emit(vec![input()]));
        refused_key_type(&outcome, 10);
    }
}

// #206 — nor for a boolean (`BOOL8`, 11).
operator_case! {
    GpuEmitPartitions,
    fn bug_a_boolean_key_is_refused_on_the_device() {
        let outcome = run_both(&emit(vec![7], 4), Script::Emit(vec![input()]));
        refused_key_type(&outcome, 11);
    }
}

operator_case! {
    GpuEmitPartitions,
    fn each_batch_is_scattered_on_its_own() {
        let stream = vec![synthetic(16, 1), synthetic(16, 3)];
        run_both(&emit(vec![1], 4), Script::Emit(stream)).same(Order::Any);
    }
}

// #95 — a decimal key (`DECIMAL128`, 27) is refused at the same switch; #184's q15 is this
// shape. The refusal comes before any export, so #187 is not reached.
operator_case! {
    GpuEmitPartitions,
    fn bug_a_decimal_key_is_refused_on_the_device() {
        let dec = decimals(64, 1);
        let node = GpuEmitPartitions::new(
            Given::of(Schema::new(dec.schema()), BatchLayout::MultipleBatches),
            vec![1],
            8,
        );
        let outcome = run_both(&node, Script::Emit(vec![dec]));
        refused_key_type(&outcome, 27);
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
