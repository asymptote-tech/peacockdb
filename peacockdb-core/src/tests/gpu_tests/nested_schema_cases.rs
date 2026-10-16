//! What a `GpuCrossJoin` and a `GpuNestedLoopJoin` hand the device up, read at the handle: the
//! cross join and the nested loop of every type, each with and without a projection, every
//! output handle held to the node's declared schema. Inner and Left over one probe batch, the
//! other seven over two, so the build side's answer at finish is read too. The builders and
//! scripts are `nested_cases.rs`'s. Every case is green or a `bug_` test with its ticket above
//! it; nothing here repairs.

use datafusion::common::JoinType;

use super::join_dimension_cases::crossing_projection;
use super::nested_cases::{cross, inner, left, nested, one_probe, two_probes};
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

operator_case! {
    GpuNestedLoopJoin,
    fn a_right_nested_loop_declares_what_the_device_holds() {
        assert_holds_as_declared(&nested(JoinType::Right, None), two_probes());
    }
}

operator_case! {
    GpuNestedLoopJoin,
    fn a_right_nested_loop_with_a_crossing_projection_declares_the_kept_columns_the_device_holds() {
        let node = nested(JoinType::Right, Some(crossing_projection(JoinType::Right)));
        assert_holds_as_declared(&node, two_probes());
    }
}

operator_case! {
    GpuNestedLoopJoin,
    fn a_full_nested_loop_declares_what_the_device_holds() {
        assert_holds_as_declared(&nested(JoinType::Full, None), two_probes());
    }
}

operator_case! {
    GpuNestedLoopJoin,
    fn a_full_nested_loop_with_a_crossing_projection_declares_the_kept_columns_the_device_holds() {
        let node = nested(JoinType::Full, Some(crossing_projection(JoinType::Full)));
        assert_holds_as_declared(&node, two_probes());
    }
}

operator_case! {
    GpuNestedLoopJoin,
    fn a_left_semi_nested_loop_declares_what_the_device_holds() {
        assert_holds_as_declared(&nested(JoinType::LeftSemi, None), two_probes());
    }
}

operator_case! {
    GpuNestedLoopJoin,
    fn a_left_semi_nested_loop_with_a_crossing_projection_declares_the_kept_columns_the_device_holds() {
        let node = nested(JoinType::LeftSemi, Some(crossing_projection(JoinType::LeftSemi)));
        assert_holds_as_declared(&node, two_probes());
    }
}

operator_case! {
    GpuNestedLoopJoin,
    fn a_left_anti_nested_loop_declares_what_the_device_holds() {
        assert_holds_as_declared(&nested(JoinType::LeftAnti, None), two_probes());
    }
}

operator_case! {
    GpuNestedLoopJoin,
    fn a_left_anti_nested_loop_with_a_crossing_projection_declares_the_kept_columns_the_device_holds() {
        let node = nested(JoinType::LeftAnti, Some(crossing_projection(JoinType::LeftAnti)));
        assert_holds_as_declared(&node, two_probes());
    }
}

operator_case! {
    GpuNestedLoopJoin,
    fn a_left_mark_nested_loop_declares_what_the_device_holds() {
        assert_holds_as_declared(&nested(JoinType::LeftMark, None), two_probes());
    }
}

operator_case! {
    GpuNestedLoopJoin,
    fn a_left_mark_nested_loop_with_a_crossing_projection_declares_the_kept_columns_the_device_holds() {
        let node = nested(JoinType::LeftMark, Some(crossing_projection(JoinType::LeftMark)));
        assert_holds_as_declared(&node, two_probes());
    }
}

operator_case! {
    GpuNestedLoopJoin,
    fn a_right_semi_nested_loop_declares_what_the_device_holds() {
        assert_holds_as_declared(&nested(JoinType::RightSemi, None), two_probes());
    }
}

operator_case! {
    GpuNestedLoopJoin,
    fn a_right_semi_nested_loop_with_a_crossing_projection_declares_the_kept_columns_the_device_holds() {
        let node = nested(JoinType::RightSemi, Some(crossing_projection(JoinType::RightSemi)));
        assert_holds_as_declared(&node, two_probes());
    }
}

operator_case! {
    GpuNestedLoopJoin,
    fn a_right_anti_nested_loop_declares_what_the_device_holds() {
        assert_holds_as_declared(&nested(JoinType::RightAnti, None), two_probes());
    }
}

operator_case! {
    GpuNestedLoopJoin,
    fn a_right_anti_nested_loop_with_a_crossing_projection_declares_the_kept_columns_the_device_holds() {
        let node = nested(JoinType::RightAnti, Some(crossing_projection(JoinType::RightAnti)));
        assert_holds_as_declared(&node, two_probes());
    }
}
