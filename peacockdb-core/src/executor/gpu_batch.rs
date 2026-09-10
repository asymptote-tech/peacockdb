//! [`GpuBatch`](super::GpuBatch)'s trait impls, and the one way out of it that skips
//! the release.

use std::fmt;
use std::mem::ManuallyDrop;

use peacockdb_ffi::raw::{PeacockExecutor, peacock_handle_release};

use super::{Batch, GpuBatch};

/// Hand the handle to an FFI call that consumes it. The release is skipped because
/// C++ has erased the registry entry: releasing again would use a dead handle.
pub(crate) fn consume(batch: GpuBatch) -> (*mut PeacockExecutor, u64) {
    let batch = ManuallyDrop::new(batch);
    (batch.executor, batch.handle)
}

impl Batch for GpuBatch {
    fn num_rows(&self) -> usize {
        self.num_rows
    }

    fn byte_size(&self) -> usize {
        self.byte_size
    }
}

impl Drop for GpuBatch {
    fn drop(&mut self) {
        unsafe { peacock_handle_release(self.executor, self.handle) };
    }
}

impl fmt::Debug for GpuBatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GpuBatch")
            .field("handle", &self.handle)
            .field("num_rows", &self.num_rows)
            .finish()
    }
}
