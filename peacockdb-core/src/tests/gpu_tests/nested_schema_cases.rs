//! What a `GpuCrossJoin` and a `GpuNestedLoopJoin` hand the device up, read at the handle:
//! each with and without a projection, every output handle held to the node's declared
//! schema. One probe batch each, since the second is #152's refusal. The builders and
//! scripts are `nested_cases.rs`'s. Every case is green or a `bug_` test with its ticket
//! above it; nothing here repairs.

use super::nested_cases::{cross, inner, left, one_probe};
use super::script::{assert_holds_as_declared, divergences_on_device};

operator_case! {
    GpuCrossJoin,
    fn a_cross_join_declares_both_sides_the_device_holds() {
        assert_holds_as_declared(&cross(None), one_probe());
    }
}

// #207 — the device hands a cross join's sixteen columns up whatever the node's projection
// declares: two declared, sixteen held, the second one the build side's key.
operator_case! {
    GpuCrossJoin,
    fn bug_a_cross_join_with_a_projection_holds_every_column_on_the_device() {
        assert_eq!(
            divergences_on_device(&cross(Some(vec![0, 8])), one_probe()),
            vec![Some(
                "2 columns declared, 16 held; 1 p_id: Int64 vs b_key INT32".to_string()
            )]
        );
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
