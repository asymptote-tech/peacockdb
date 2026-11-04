//! [`GpuBatch`](super::GpuBatch)'s trait impls, and the one way out of it that skips
//! the release.

use std::fmt;
use std::mem::ManuallyDrop;

use peacockdb_ffi::raw::{PeacockExecutor, peacock_handle_duplicate, peacock_handle_release};

use super::gpu_backend::last_error;
use super::{BackendError, Batch, GpuBatch};

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

    /// The same node's output under a second handle, so a slice or an export of the share
    /// is charged where the original's would be.
    fn share(&self) -> Result<Self, BackendError> {
        let mut handle = 0u64;
        let rc = unsafe { peacock_handle_duplicate(self.executor, self.handle, &mut handle) };
        if rc != 0 {
            return Err(BackendError::new(format!(
                "handle_duplicate({}): {}",
                self.handle,
                last_error(self.executor)
            )));
        }
        Ok(GpuBatch::new(
            self.executor,
            handle,
            self.producer,
            self.num_rows,
            self.byte_size,
        ))
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
