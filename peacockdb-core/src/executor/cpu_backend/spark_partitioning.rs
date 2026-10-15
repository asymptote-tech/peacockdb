//! Which lane a row belongs to, Spark-murmur3 — the one rule both CPU paths and the GPU
//! kernel answer with.
//!
//! `peacock::partitioning::spark_hash_partition` is the device's copy and the live
//! conformance gate proves the two agree bit for bit, so a second spelling on this side
//! would be a divergence nothing else could see: the rows would still be joined, in the
//! wrong lanes, and every per-partition count would drift from its golden.
//!
//! Deliberately not DataFusion's `RepartitionExec`, whose ahash lands the same key in a
//! different partition.

use std::sync::Arc;

use datafusion::arrow::array::{ArrayRef, AsArray};
use datafusion::arrow::compute::cast;
use datafusion::arrow::datatypes::{DataType, Float32Type, Float64Type, Int64Type, UInt64Type};
use datafusion::arrow::record_batch::RecordBatch;
use datafusion::error::{DataFusionError, Result as DfResult};
use datafusion::physical_expr::PhysicalExpr;
use datafusion_comet_spark_expr::hash_funcs::murmur3::create_murmur3_hashes;

/// Spark's `HashPartitioning` seed; comet and the GPU kernel both initialize to it.
const SEED: u32 = 42;

/// Spark `pmod` (positive modulo): a signed murmur3 hash into `[0, n)`. Must match the
/// GPU kernel's exactly — negative hashes wrap the same way — or per-partition row counts
/// diverge from the golden.
pub(crate) fn pmod(hash: i32, n: i32) -> i32 {
    ((hash % n) + n) % n
}

/// The rows of `batch` that belong to each of `lanes` lanes, in row order.
pub(crate) fn rows_per_lane(
    batch: &RecordBatch,
    hash_exprs: &[Arc<dyn PhysicalExpr>],
    lanes: usize,
) -> DfResult<Vec<Vec<u32>>> {
    let rows = batch.num_rows();
    let keys = hash_keys(batch, hash_exprs)?;
    let mut hashes = vec![SEED; rows];
    if rows > 0 {
        create_murmur3_hashes(&keys, &mut hashes)
            .map_err(|error| DataFusionError::External(format!("comet murmur3: {error}").into()))?;
    }
    let n = lanes as i32;
    let mut per_lane: Vec<Vec<u32>> = vec![Vec::new(); lanes];
    for (row, hash) in hashes.iter().enumerate() {
        per_lane[pmod(*hash as i32, n) as usize].push(row as u32);
    }
    Ok(per_lane)
}

/// Every NaN as Rust's `NAN` (`0x7ff8000000000000` / `0x7fc00000`), so comet — which hashes
/// a float by its bits — puts NaN and -NaN in one lane. The device's float arm does the same.
/// -0.0 comet already folds onto +0.0.
fn canonical_nans(array: ArrayRef) -> ArrayRef {
    match array.data_type() {
        DataType::Float64 => Arc::new(
            array
                .as_primitive::<Float64Type>()
                .unary::<_, Float64Type>(|v| if v.is_nan() { f64::NAN } else { v }),
        ) as ArrayRef,
        DataType::Float32 => Arc::new(
            array
                .as_primitive::<Float32Type>()
                .unary::<_, Float32Type>(|v| if v.is_nan() { f32::NAN } else { v }),
        ) as ArrayRef,
        _ => array,
    }
}

/// The key columns as comet hashes them, after the normalizations that make the two engines
/// agree where comet's own rule would not.
fn hash_keys(batch: &RecordBatch, hash_exprs: &[Arc<dyn PhysicalExpr>]) -> DfResult<Vec<ArrayRef>> {
    hash_exprs
        .iter()
        .map(|expr| {
            let array = expr.evaluate(batch)?.into_array(batch.num_rows())?;
            let array = match array.data_type() {
                // Widened to precision 38 so comet hashes the unscaled value's 16 bytes at
                // every precision — the bytes the device's int128 holds. Without it comet
                // hashes 8 at p ≤ 18, a width the device cannot tell from its columns: cuDF
                // keeps no precision and the loader widens every decimal.
                DataType::Decimal128(p, s) if *p < 38 => {
                    cast(&array, &DataType::Decimal128(38, *s))?
                }
                // Spark has no unsigned types, so there is no placement to match: the rule is
                // ours and the device's switch applies the same one. u64 goes by its bits
                // because a u64 past i64::MAX has no i64 value to cast to.
                DataType::UInt8 | DataType::UInt16 => cast(&array, &DataType::Int32)?,
                DataType::UInt32 => cast(&array, &DataType::Int64)?,
                DataType::UInt64 => Arc::new(
                    array
                        .as_primitive::<UInt64Type>()
                        .unary::<_, Int64Type>(|v| v as i64),
                ) as ArrayRef,
                _ => array,
            };
            Ok(canonical_nans(array))
        })
        .collect()
}

#[cfg(test)]
mod tests;
