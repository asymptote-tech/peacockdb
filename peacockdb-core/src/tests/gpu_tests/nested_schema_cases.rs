//! What a `GpuCrossJoin` and a `GpuNestedLoopJoin` hand the device up, read at the handle:
//! each with and without a projection, every output handle held to the node's declared
//! schema. One probe batch each. The builders and scripts are `nested_cases.rs`'s. Every case is green or a `bug_` test with its ticket
//! above it; nothing here repairs.

use super::nested_cases::{cross, inner, left, one_probe};
use super::script::assert_holds_as_declared;

operator_case! {
    GpuCrossJoin,
    fn a_cross_join_declares_both_sides_the_device_holds() {
        assert_holds_as_declared(&cross(None), one_probe());
    }
}

operator_case! {
    GpuCrossJoin,
    fn a_cross_join_with_a_projection_declares_the_kept_columns_the_device_holds() {
        assert_holds_as_declared(&cross(Some(vec![0, 8])), one_probe());
    }
}

operator_case! {
    GpuNestedLoopJoin,
    fn an_inner_nested_loop_declares_both_sides_the_device_holds() {
        assert_holds_as_declared(&inner(None), one_probe());
    }
}

operator_case! {
    GpuNestedLoopJoin,
    fn an_inner_nested_loop_with_a_projection_declares_the_kept_columns_the_device_holds() {
        assert_holds_as_declared(&inner(Some(vec![13, 4, 8, 6])), one_probe());
    }
}

operator_case! {
    GpuNestedLoopJoin,
    fn a_left_nested_loop_declares_both_sides_the_device_holds() {
        assert_holds_as_declared(&left(None), one_probe());
    }
}

operator_case! {
    GpuNestedLoopJoin,
    fn a_left_nested_loop_with_a_projection_declares_the_kept_columns_the_device_holds() {
        assert_holds_as_declared(&left(Some(vec![13, 4, 8, 6])), one_probe());
    }
}
