//! What a `GpuEmitPartitions` hands the device up, read at each lane's handle: the scatter
//! on every key type the kernel admits, every lane held to the node's declared schema. The
//! builder is `emit_cases.rs`'s. Every case is green or a `bug_` test with its ticket above
//! it; nothing here repairs.

use super::emit_cases::{emit, key_types_emit};
use super::script::{Script, assert_holds_as_declared};
use crate::tests::synthetic::{key_types, synthetic};

operator_case! {
    GpuEmitPartitions,
    fn a_scatter_on_an_int32_key_declares_what_the_device_holds_in_every_lane() {
        assert_holds_as_declared(&emit(vec![1], 4), Script::Emit(vec![synthetic(64, 1)]));
    }
}

operator_case! {
    GpuEmitPartitions,
    fn a_scatter_on_an_int64_key_declares_what_the_device_holds_in_every_lane() {
        assert_holds_as_declared(&emit(vec![3], 4), Script::Emit(vec![synthetic(64, 1)]));
    }
}

operator_case! {
    GpuEmitPartitions,
    fn a_scatter_on_a_utf8_key_declares_what_the_device_holds_in_every_lane() {
        assert_holds_as_declared(&emit(vec![5], 4), Script::Emit(vec![synthetic(64, 1)]));
    }
}

operator_case! {
    GpuEmitPartitions,
    fn a_scatter_on_a_date32_key_declares_what_the_device_holds_in_every_lane() {
        assert_holds_as_declared(&emit(vec![6], 4), Script::Emit(vec![synthetic(64, 1)]));
    }
}

operator_case! {
    GpuEmitPartitions,
    fn a_scatter_on_a_composite_key_declares_what_the_device_holds_in_every_lane() {
        assert_holds_as_declared(&emit(vec![1, 5], 4), Script::Emit(vec![synthetic(64, 1)]));
    }
}

// Sixty-four lanes over eight rows: most lanes are zero-row tables, each under the schema.
operator_case! {
    GpuEmitPartitions,
    fn a_scatter_into_sixty_four_lanes_declares_what_the_device_holds_in_the_empty_ones() {
        assert_holds_as_declared(&emit(vec![1], 64), Script::Emit(vec![synthetic(8, 1)]));
    }
}

// --- every key type, held as declared ------------------------------------------
//
// A normalizing cast in the kernel is hash-only: the scattered lane must keep the original
// column, under the declared type. That is the half a lane comparison cannot see, since both
// engines could agree on the lane and the device still hand up a widened column.

/// Every lane of a scatter on `key_types`' column `key` holds what the node declared.
fn holds_as_declared_on(key: u32) {
    assert_holds_as_declared(
        &key_types_emit(vec![key], 4),
        Script::Emit(vec![key_types(64, 1)]),
    );
}

macro_rules! key_type_schema_case {
    ($name:ident, $key:expr) => {
        operator_case! {
            GpuEmitPartitions,
            fn $name() { holds_as_declared_on($key) }
        }
    };
}

key_type_schema_case!(a_scatter_on_an_int8_key_declares_what_the_device_holds, 1);
key_type_schema_case!(a_scatter_on_an_int16_key_declares_what_the_device_holds, 2);
key_type_schema_case!(a_scatter_on_a_float32_key_declares_what_the_device_holds, 5);
key_type_schema_case!(a_scatter_on_a_float64_key_declares_what_the_device_holds, 6);
key_type_schema_case!(a_scatter_on_a_boolean_key_declares_what_the_device_holds, 7);
key_type_schema_case!(
    a_scatter_on_a_timestamp_second_key_declares_what_the_device_holds,
    9
);
key_type_schema_case!(
    a_scatter_on_a_timestamp_millisecond_key_declares_what_the_device_holds,
    10
);
key_type_schema_case!(
    a_scatter_on_a_timestamp_microsecond_key_declares_what_the_device_holds,
    11
);
key_type_schema_case!(
    a_scatter_on_a_timestamp_nanosecond_key_declares_what_the_device_holds,
    12
);
key_type_schema_case!(
    a_scatter_on_a_narrow_decimal_key_declares_what_the_device_holds,
    13
);
key_type_schema_case!(
    a_scatter_on_a_wide_decimal_key_declares_what_the_device_holds,
    14
);
key_type_schema_case!(a_scatter_on_a_uint8_key_declares_what_the_device_holds, 16);
key_type_schema_case!(a_scatter_on_a_uint16_key_declares_what_the_device_holds, 17);
key_type_schema_case!(a_scatter_on_a_uint32_key_declares_what_the_device_holds, 18);
key_type_schema_case!(a_scatter_on_a_uint64_key_declares_what_the_device_holds, 19);
