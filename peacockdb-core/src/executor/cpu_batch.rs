//! [`CpuBatch`](super::CpuBatch)'s side of [`Batch`](super::Batch).

use super::{Batch, CpuBatch};
use crate::common::{batch_varlen_content_bytes, logical_size_from_schema};

impl Batch for CpuBatch {
    fn num_rows(&self) -> usize {
        self.batch.num_rows()
    }

    /// The plan's declared width times the rows, plus what the var-length columns
    /// actually hold — the same formula the device is priced by, and by every other
    /// component in the tree. Arrow's `get_array_memory_size` is what was ALLOCATED:
    /// 64-byte aligned buffers and an absent validity bitmap where the device counts one,
    /// so the two engines put different numbers on one node of one query.
    fn byte_size(&self) -> usize {
        logical_size_from_schema(
            &self.batch.schema(),
            self.batch.num_rows(),
            batch_varlen_content_bytes(&self.batch),
        )
    }
}
