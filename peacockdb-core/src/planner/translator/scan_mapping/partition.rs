//! The row-group → (partition, batch) mapping: one pure policy, computed once at plan
//! time, whose output everything else consumes verbatim — `GpuLoadParquet` stores it,
//! the plan golden renders it as `partition_groups=[...]`, the loader executes it,
//! validation checks the declared lane count against it.
//!
//! Balancing is by row count and batching is by bytes, and the bytes are the parquet
//! column-chunk totals over the projected columns: a varchar's width is a property of
//! the data, and the file metadata already holds the answer.

use crate::plan::PlanError;
use crate::plan::{Batching, RowGroupMeta};

/// Survivors → partitions → batches → row-group indices.
///
/// Contiguous chunks balanced by row count; within a chunk, consecutive row groups pack
/// greedily while bytes stay under target. A row group over target is still its own
/// batch — one row group is the minimum granularity, and the planner always emits a plan
/// (the accountant owns the runtime consequence, #142). Contiguity is policy, not a cuDF
/// requirement; changing it regenerates goldens and is treated as that.
pub(crate) fn partition(
    survivors: &[RowGroupMeta],
    n_partitions: usize,
    batching: Batching,
) -> Result<Vec<Vec<Vec<u32>>>, PlanError> {
    if survivors.is_empty() {
        return Err(PlanError::Invalid(
            "no surviving row groups: what an empty scan means is the caller's decision, \
             not an empty map — the wire format reads that as one unmapped partition"
                .to_string(),
        ));
    }
    if n_partitions == 0 {
        return Err(PlanError::Invalid(
            "a source needs at least one lane to emit into".to_string(),
        ));
    }

    Ok(balanced_chunks(survivors, n_partitions)
        .into_iter()
        .map(|chunk| batches_of(&survivors[chunk], batching))
        .collect())
}

/// Contiguous ranges over the survivors, each holding about its share of the rows.
/// Chunks are empty only where there were fewer survivors than lanes: an empty lane is
/// an ordinary shape here, as it is for a hash that lands no key.
fn balanced_chunks(survivors: &[RowGroupMeta], n_partitions: usize) -> Vec<std::ops::Range<usize>> {
    let total: u64 = survivors.iter().map(|g| g.rows).sum();
    let mut chunks = Vec::with_capacity(n_partitions);
    let mut index = 0;
    let mut taken = 0;

    for part in 0..n_partitions {
        let want = (total - taken).div_ceil((n_partitions - part) as u64);
        let start = index;
        let mut got = 0;
        while index < survivors.len() {
            // Stop where taking the next group would land further from this lane's share
            // than stopping does. Overshooting instead costs the balance bound: two full
            // row groups against a want of one and a half puts the whole tail in one lane.
            let next = got + survivors[index].rows;
            if index > start && next.abs_diff(want) >= got.abs_diff(want) {
                break;
            }
            got = next;
            index += 1;
        }
        chunks.push(start..index);
        taken += got;
    }

    // The last lane's share is everything left, and every group takes it closer to that.
    debug_assert_eq!(
        index,
        survivors.len(),
        "a lane's worth of row groups went nowhere"
    );
    chunks
}

fn batches_of(chunk: &[RowGroupMeta], batching: Batching) -> Vec<Vec<u32>> {
    if chunk.is_empty() {
        return Vec::new();
    }
    let target = match batching {
        Batching::Off => return vec![chunk.iter().map(|g| g.index).collect()],
        Batching::PerRowGroup => return chunk.iter().map(|g| vec![g.index]).collect(),
        Batching::Sized { target_batch_bytes } => target_batch_bytes as u64,
    };

    let mut batches: Vec<Vec<u32>> = Vec::new();
    let mut current: Vec<u32> = Vec::new();
    let mut bytes = 0;
    for group in chunk {
        if !current.is_empty() && bytes + group.bytes > target {
            batches.push(std::mem::take(&mut current));
            bytes = 0;
        }
        current.push(group.index);
        bytes += group.bytes;
    }
    if !current.is_empty() {
        batches.push(current);
    }
    batches
}

#[cfg(test)]
mod tests;
