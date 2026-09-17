//! What a `GpuEmitPartitions` hands the device up, read at each lane's handle: the scatter
//! on every key type the kernel admits, every lane held to the node's declared schema. The
//! builder is `emit_cases.rs`'s. Every case is green or a `bug_` test with its ticket above
//! it; nothing here repairs.

use super::emit_cases::emit;
use super::script::{Script, assert_holds_as_declared};
use crate::tests::synthetic::synthetic;

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
