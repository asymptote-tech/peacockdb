//! What a `GpuHashJoin` hands the device up, read at the handle: every join type the device
//! runs, with and without a crossing projection, and the three key-path types on every key
//! type the corpus joins on, every output handle held to the node's declared schema. Left
//! and Full refuse their first probe batch (#152, pinned in `join_cases.rs`) and hold
//! nothing to read. The builders and scripts are `join_cases.rs`'s and
//! `join_dimension_cases.rs`'s. Every case is green or a `bug_` test with its ticket above
//! it; nothing here repairs.

use datafusion::common::JoinType;

use super::join_cases::{hash_join, one_probe, two_probes};
use super::join_dimension_cases::{
    Key, build_with_misses, crossing_projection, hash_join_keyed, keyed_anti_script, keyed_script,
    probes_with_misses,
};
use super::script::assert_holds_as_declared;

// Every type on the `Int32` key, the script each type streams under in `join_cases.rs`.

operator_case! {
    GpuHashJoin,
    fn an_inner_join_declares_what_the_device_holds() {
        assert_holds_as_declared(&hash_join(JoinType::Inner, false, false, None), one_probe());
    }
}

operator_case! {
    GpuHashJoin,
    fn an_inner_join_with_a_crossing_projection_declares_the_kept_columns_the_device_holds() {
        let node = hash_join(JoinType::Inner, false, false, Some(crossing_projection(JoinType::Inner)));
        assert_holds_as_declared(&node, one_probe());
    }
}

operator_case! {
    GpuHashJoin,
    fn a_right_join_declares_what_the_device_holds() {
        assert_holds_as_declared(&hash_join(JoinType::Right, false, false, None), one_probe());
    }
}

operator_case! {
    GpuHashJoin,
    fn a_right_join_with_a_crossing_projection_declares_the_kept_columns_the_device_holds() {
        let node = hash_join(JoinType::Right, false, false, Some(crossing_projection(JoinType::Right)));
        assert_holds_as_declared(&node, one_probe());
    }
}

operator_case! {
    GpuHashJoin,
    fn a_left_semi_join_declares_what_the_device_holds() {
        assert_holds_as_declared(&hash_join(JoinType::LeftSemi, false, false, None), two_probes());
    }
}

operator_case! {
    GpuHashJoin,
    fn a_left_semi_join_with_a_crossing_projection_declares_the_kept_columns_the_device_holds() {
        let node = hash_join(JoinType::LeftSemi, false, false, Some(crossing_projection(JoinType::LeftSemi)));
        assert_holds_as_declared(&node, two_probes());
    }
}

operator_case! {
    GpuHashJoin,
    fn a_left_anti_join_declares_what_the_device_holds() {
        assert_holds_as_declared(&hash_join(JoinType::LeftAnti, false, false, None), probes_with_misses());
    }
}

operator_case! {
    GpuHashJoin,
    fn a_left_anti_join_with_a_crossing_projection_declares_the_kept_columns_the_device_holds() {
        let node = hash_join(JoinType::LeftAnti, false, false, Some(crossing_projection(JoinType::LeftAnti)));
        assert_holds_as_declared(&node, probes_with_misses());
    }
}

operator_case! {
    GpuHashJoin,
    fn a_left_mark_join_declares_what_the_device_holds() {
        assert_holds_as_declared(&hash_join(JoinType::LeftMark, false, false, None), probes_with_misses());
    }
}

operator_case! {
    GpuHashJoin,
    fn a_left_mark_join_with_a_crossing_projection_declares_the_kept_columns_the_device_holds() {
        let node = hash_join(JoinType::LeftMark, false, false, Some(crossing_projection(JoinType::LeftMark)));
        assert_holds_as_declared(&node, probes_with_misses());
    }
}

operator_case! {
    GpuHashJoin,
    fn a_right_semi_join_declares_what_the_device_holds() {
        assert_holds_as_declared(&hash_join(JoinType::RightSemi, false, false, None), one_probe());
    }
}

operator_case! {
    GpuHashJoin,
    fn a_right_semi_join_with_a_crossing_projection_declares_the_kept_columns_the_device_holds() {
        let node = hash_join(JoinType::RightSemi, false, false, Some(crossing_projection(JoinType::RightSemi)));
        assert_holds_as_declared(&node, one_probe());
    }
}

operator_case! {
    GpuHashJoin,
    fn a_right_anti_join_declares_what_the_device_holds() {
        assert_holds_as_declared(&hash_join(JoinType::RightAnti, false, false, None), build_with_misses());
    }
}

operator_case! {
    GpuHashJoin,
    fn a_right_anti_join_with_a_crossing_projection_declares_the_kept_columns_the_device_holds() {
        let node = hash_join(JoinType::RightAnti, false, false, Some(crossing_projection(JoinType::RightAnti)));
        assert_holds_as_declared(&node, build_with_misses());
    }
}

// The three key-path types on each key type: the per-batch join, the side swap, the
// accumulated-keys finish. A projection over the keyed output keeps one column of each
// side where the type keeps both, and the build's where it does not.

fn keyed_projection(join_type: JoinType) -> Vec<u32> {
    match join_type {
        JoinType::Inner | JoinType::Right => vec![13, 4, 8, 6],
        JoinType::LeftAnti => vec![6, 4, 0],
        other => unreachable!("{other:?} is not one of the three key-path types"),
    }
}

operator_case! {
    GpuHashJoin,
    fn an_inner_join_on_a_composite_key_declares_what_the_device_holds() {
        let node = hash_join_keyed(JoinType::Inner, Key::Composite, None);
        assert_holds_as_declared(&node, keyed_script(Key::Composite));
    }
}

operator_case! {
    GpuHashJoin,
    fn an_inner_join_on_a_composite_key_with_a_projection_declares_the_kept_columns_the_device_holds() {
        let node = hash_join_keyed(JoinType::Inner, Key::Composite, Some(keyed_projection(JoinType::Inner)));
        assert_holds_as_declared(&node, keyed_script(Key::Composite));
    }
}

operator_case! {
    GpuHashJoin,
    fn an_inner_join_on_an_int64_key_declares_what_the_device_holds() {
        let node = hash_join_keyed(JoinType::Inner, Key::Int64, None);
        assert_holds_as_declared(&node, keyed_script(Key::Int64));
    }
}

operator_case! {
    GpuHashJoin,
    fn an_inner_join_on_an_int64_key_with_a_projection_declares_the_kept_columns_the_device_holds() {
        let node = hash_join_keyed(JoinType::Inner, Key::Int64, Some(keyed_projection(JoinType::Inner)));
        assert_holds_as_declared(&node, keyed_script(Key::Int64));
    }
}

operator_case! {
    GpuHashJoin,
    fn an_inner_join_on_an_utf8_key_declares_what_the_device_holds() {
        let node = hash_join_keyed(JoinType::Inner, Key::Utf8, None);
        assert_holds_as_declared(&node, keyed_script(Key::Utf8));
    }
}

operator_case! {
    GpuHashJoin,
    fn an_inner_join_on_an_utf8_key_with_a_projection_declares_the_kept_columns_the_device_holds() {
        let node = hash_join_keyed(JoinType::Inner, Key::Utf8, Some(keyed_projection(JoinType::Inner)));
        assert_holds_as_declared(&node, keyed_script(Key::Utf8));
    }
}

operator_case! {
    GpuHashJoin,
    fn an_inner_join_on_a_date32_key_declares_what_the_device_holds() {
        let node = hash_join_keyed(JoinType::Inner, Key::Date32, None);
        assert_holds_as_declared(&node, keyed_script(Key::Date32));
    }
}

operator_case! {
    GpuHashJoin,
    fn an_inner_join_on_a_date32_key_with_a_projection_declares_the_kept_columns_the_device_holds() {
        let node = hash_join_keyed(JoinType::Inner, Key::Date32, Some(keyed_projection(JoinType::Inner)));
        assert_holds_as_declared(&node, keyed_script(Key::Date32));
    }
}

operator_case! {
    GpuHashJoin,
    fn a_right_join_on_a_composite_key_declares_what_the_device_holds() {
        let node = hash_join_keyed(JoinType::Right, Key::Composite, None);
        assert_holds_as_declared(&node, keyed_script(Key::Composite));
    }
}

operator_case! {
    GpuHashJoin,
    fn a_right_join_on_a_composite_key_with_a_projection_declares_the_kept_columns_the_device_holds() {
        let node = hash_join_keyed(JoinType::Right, Key::Composite, Some(keyed_projection(JoinType::Right)));
        assert_holds_as_declared(&node, keyed_script(Key::Composite));
    }
}

operator_case! {
    GpuHashJoin,
    fn a_right_join_on_an_int64_key_declares_what_the_device_holds() {
        let node = hash_join_keyed(JoinType::Right, Key::Int64, None);
        assert_holds_as_declared(&node, keyed_script(Key::Int64));
    }
}

operator_case! {
    GpuHashJoin,
    fn a_right_join_on_an_int64_key_with_a_projection_declares_the_kept_columns_the_device_holds() {
        let node = hash_join_keyed(JoinType::Right, Key::Int64, Some(keyed_projection(JoinType::Right)));
        assert_holds_as_declared(&node, keyed_script(Key::Int64));
    }
}

operator_case! {
    GpuHashJoin,
    fn a_right_join_on_an_utf8_key_declares_what_the_device_holds() {
        let node = hash_join_keyed(JoinType::Right, Key::Utf8, None);
        assert_holds_as_declared(&node, keyed_script(Key::Utf8));
    }
}

operator_case! {
    GpuHashJoin,
    fn a_right_join_on_an_utf8_key_with_a_projection_declares_the_kept_columns_the_device_holds() {
        let node = hash_join_keyed(JoinType::Right, Key::Utf8, Some(keyed_projection(JoinType::Right)));
        assert_holds_as_declared(&node, keyed_script(Key::Utf8));
    }
}

operator_case! {
    GpuHashJoin,
    fn a_right_join_on_a_date32_key_declares_what_the_device_holds() {
        let node = hash_join_keyed(JoinType::Right, Key::Date32, None);
        assert_holds_as_declared(&node, keyed_script(Key::Date32));
    }
}

operator_case! {
    GpuHashJoin,
    fn a_right_join_on_a_date32_key_with_a_projection_declares_the_kept_columns_the_device_holds() {
        let node = hash_join_keyed(JoinType::Right, Key::Date32, Some(keyed_projection(JoinType::Right)));
        assert_holds_as_declared(&node, keyed_script(Key::Date32));
    }
}

operator_case! {
    GpuHashJoin,
    fn a_left_anti_join_on_a_composite_key_declares_what_the_device_holds() {
        let node = hash_join_keyed(JoinType::LeftAnti, Key::Composite, None);
        assert_holds_as_declared(&node, keyed_anti_script(Key::Composite));
    }
}

operator_case! {
    GpuHashJoin,
    fn a_left_anti_join_on_a_composite_key_with_a_projection_declares_the_kept_columns_the_device_holds() {
        let node = hash_join_keyed(JoinType::LeftAnti, Key::Composite, Some(keyed_projection(JoinType::LeftAnti)));
        assert_holds_as_declared(&node, keyed_anti_script(Key::Composite));
    }
}

operator_case! {
    GpuHashJoin,
    fn a_left_anti_join_on_an_int64_key_declares_what_the_device_holds() {
        let node = hash_join_keyed(JoinType::LeftAnti, Key::Int64, None);
        assert_holds_as_declared(&node, keyed_anti_script(Key::Int64));
    }
}

operator_case! {
    GpuHashJoin,
    fn a_left_anti_join_on_an_int64_key_with_a_projection_declares_the_kept_columns_the_device_holds() {
        let node = hash_join_keyed(JoinType::LeftAnti, Key::Int64, Some(keyed_projection(JoinType::LeftAnti)));
        assert_holds_as_declared(&node, keyed_anti_script(Key::Int64));
    }
}

operator_case! {
    GpuHashJoin,
    fn a_left_anti_join_on_an_utf8_key_declares_what_the_device_holds() {
        let node = hash_join_keyed(JoinType::LeftAnti, Key::Utf8, None);
        assert_holds_as_declared(&node, keyed_anti_script(Key::Utf8));
    }
}

operator_case! {
    GpuHashJoin,
    fn a_left_anti_join_on_an_utf8_key_with_a_projection_declares_the_kept_columns_the_device_holds() {
        let node = hash_join_keyed(JoinType::LeftAnti, Key::Utf8, Some(keyed_projection(JoinType::LeftAnti)));
        assert_holds_as_declared(&node, keyed_anti_script(Key::Utf8));
    }
}

operator_case! {
    GpuHashJoin,
    fn a_left_anti_join_on_a_date32_key_declares_what_the_device_holds() {
        let node = hash_join_keyed(JoinType::LeftAnti, Key::Date32, None);
        assert_holds_as_declared(&node, keyed_anti_script(Key::Date32));
    }
}

operator_case! {
    GpuHashJoin,
    fn a_left_anti_join_on_a_date32_key_with_a_projection_declares_the_kept_columns_the_device_holds() {
        let node = hash_join_keyed(JoinType::LeftAnti, Key::Date32, Some(keyed_projection(JoinType::LeftAnti)));
        assert_holds_as_declared(&node, keyed_anti_script(Key::Date32));
    }
}
