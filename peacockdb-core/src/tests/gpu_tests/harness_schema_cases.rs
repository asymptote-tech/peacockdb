//! The reader against the uploader, and the mid-plan limit: what the device holds at a
//! handle the harness itself made, read without moving a row. An unload answers host rows
//! and holds no handle, so it has no schema case. Every case is green or a `bug_` test with
//! its ticket above it; nothing here repairs.

use datafusion::arrow::record_batch::RecordBatch;

use super::device::Device;
use super::harness_cases::{empty_plan, limit_over};
use super::script::{Script, assert_holds_as_declared};
use crate::test_support::{device_divergence, device_schema_of};
use crate::tests::synthetic::{decimals, synthetic};

/// The upload's schema read back at its handle, held to the batch's own.
fn uploaded_reads_back_as_itself(batch: RecordBatch) {
    let device = Device::open(&empty_plan());
    let held = device.schema_of(&device.upload(&batch));
    assert_eq!(held, device_schema_of(&batch.schema()));
    assert_eq!(device_divergence(&batch.schema(), &held), None);
}

#[test]
fn an_uploaded_batch_of_every_synthetic_type_reads_back_as_its_own_schema() {
    uploaded_reads_back_as_itself(synthetic(64, 1));
}

#[test]
fn an_uploaded_decimal_batch_reads_back_at_its_scale() {
    uploaded_reads_back_as_itself(decimals(64, 1));
}

#[test]
fn an_uploaded_zero_row_batch_reads_back_as_its_own_schema() {
    uploaded_reads_back_as_itself(synthetic(0, 1));
}

operator_case! {
    GpuLimit,
    fn a_limit_declares_the_schema_the_device_holds_at_each_slice() {
        let (node, stream) = limit_over(12, Some(8), 16, 2);
        assert_holds_as_declared(&node, Script::Accumulate(stream));
    }
}
