//! The lane rule's own properties, with no device: which rows the rule puts together.
//! That the *kernel* follows the rule is `gpu_tests::murmur_conformance`'s.

use std::sync::Arc;

use datafusion::arrow::array::{ArrayRef, Float32Array, Float64Array};
use datafusion::arrow::datatypes::{DataType, Field, Schema as ArrowSchema};
use datafusion::arrow::record_batch::RecordBatch;
use datafusion::physical_expr::PhysicalExpr;
use datafusion::physical_expr::expressions::Column;

use super::rows_per_lane;

/// The lane of each row under the production rule, by row index.
fn lanes_of(name: &str, ty: DataType, column: ArrayRef, lanes: usize) -> Vec<usize> {
    let schema = Arc::new(ArrowSchema::new(vec![Field::new(name, ty, true)]));
    let rows = column.len();
    let batch = RecordBatch::try_new(schema, vec![column]).expect("one column");
    let exprs: Vec<Arc<dyn PhysicalExpr>> = vec![Arc::new(Column::new(name, 0))];
    let per_lane = rows_per_lane(&batch, &exprs, lanes).expect("the rule answers");
    let mut out = vec![usize::MAX; rows];
    for (lane, in_lane) in per_lane.iter().enumerate() {
        for row in in_lane {
            out[*row as usize] = lane;
        }
    }
    out
}

/// Every NaN — either sign, any payload — shares a lane, and so do 0.0 and -0.0. comet
/// hashes a float by its bits, so this is the canonicalization and not comet's own rule:
/// without it `NaN` and `-NaN` split at tp4 while the device equates them at tp1.
#[test]
fn every_nan_shares_a_lane_and_so_do_the_two_zeros() {
    let f64s: ArrayRef = Arc::new(Float64Array::from(vec![
        Some(f64::NAN),
        Some(f64::from_bits(f64::NAN.to_bits() | (1 << 63))), // -NaN, as x86's 0/0 gives
        Some(f64::from_bits(0x7ff0_0000_0000_0001)),          // a signalling payload
        Some(0.0),
        Some(-0.0),
    ]));
    let lane = lanes_of("k", DataType::Float64, f64s, 8);
    assert_eq!(lane[0], lane[1], "NaN and -NaN share a lane");
    assert_eq!(lane[0], lane[2], "a signalling NaN shares it too");
    assert_eq!(lane[3], lane[4], "0.0 and -0.0 share a lane");

    let f32s: ArrayRef = Arc::new(Float32Array::from(vec![
        Some(f32::NAN),
        Some(f32::from_bits(0xffc0_0000)),
        Some(0.0f32),
        Some(-0.0f32),
    ]));
    let lane = lanes_of("k", DataType::Float32, f32s, 8);
    assert_eq!(lane[0], lane[1], "f32 NaN and -NaN share a lane");
    assert_eq!(lane[2], lane[3], "f32 0.0 and -0.0 share a lane");
}
