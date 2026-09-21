//! What the three accumulators hand the device up, read at the handle: a coalesce over a
//! lane, the accumulating sort, the sorted merge over lanes, each output handle held to the
//! node's declared schema. The builders are `accumulate_cases.rs`'s. Every case is green or
//! a `bug_` test with its ticket above it; nothing here repairs.

use super::accumulate_cases::{coalesce, merged, one_per_lane, runs, sorted};
use super::script::{Script, assert_holds_as_declared};
use crate::plan::{GpuCoalesceAllBatches, PartitionLayout, Schema};
use crate::tests::given::Given;
use crate::tests::synthetic::{decimals, synthetic};

operator_case! {
    GpuCoalesceAllBatches,
    fn a_coalesce_over_every_synthetic_type_declares_what_the_device_holds() {
        let stream = vec![synthetic(16, 1), synthetic(24, 2)];
        assert_holds_as_declared(&coalesce(), Script::Accumulate(stream));
    }
}

operator_case! {
    GpuCoalesceAllBatches,
    fn a_coalesce_over_a_decimal_declares_the_scale_2_the_device_holds() {
        let node = GpuCoalesceAllBatches::new(Given::with_layout(
            Schema::new(decimals(0, 0).schema()),
            PartitionLayout::new(1),
        ));
        let stream = vec![decimals(16, 1), decimals(24, 2)];
        assert_holds_as_declared(&node, Script::Accumulate(stream));
    }
}

operator_case! {
    GpuCoalesceAllBatches,
    fn a_coalesce_over_one_batch_declares_what_the_device_holds() {
        assert_holds_as_declared(&coalesce(), Script::Accumulate(vec![synthetic(16, 1)]));
    }
}

operator_case! {
    GpuAccumulateBatchesAndSort,
    fn an_accumulating_sort_declares_the_input_schema_the_device_holds() {
        assert_holds_as_declared(&sorted(None), Script::Accumulate(runs(48, 1, 3)));
    }
}

operator_case! {
    GpuAccumulateBatchesAndSort,
    fn an_accumulating_sort_with_a_fetch_declares_the_input_schema_the_device_holds() {
        assert_holds_as_declared(&sorted(Some(10)), Script::Accumulate(runs(40, 1, 2)));
    }
}

operator_case! {
    GpuMergeSortedPartitions,
    fn a_sorted_merge_over_three_lanes_declares_the_input_schema_the_device_holds() {
        let lanes = one_per_lane(runs(48, 1, 3));
        assert_holds_as_declared(&merged(3, None), Script::Lanes(lanes));
    }
}

// The one leaf shape the builders above do not cover: a lane of one batch reaches the
// device as a concatenate of one, not a merge.
operator_case! {
    GpuMergeSortedPartitions,
    fn a_sorted_merge_over_one_lane_declares_the_input_schema_the_device_holds() {
        let lanes = one_per_lane(runs(16, 1, 1));
        assert_holds_as_declared(&merged(1, None), Script::Lanes(lanes));
    }
}
