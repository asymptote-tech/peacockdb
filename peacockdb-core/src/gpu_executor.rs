use std::path::Path;

use arrow::ipc::reader::StreamReader;
use arrow::record_batch::RecordBatch;
use datafusion::error::{DataFusionError, Result as DfResult};
use datafusion::execution::context::SessionContext;

use crate::{create_context_with_tables, plan_serializer::serialize_plan};

use peacockdb_ffi::raw::{
    peacock_execute, peacock_executor_create, peacock_executor_destroy, peacock_last_error,
    peacock_result_free, PeacockExecutor,
};

/// Executes SQL queries on the GPU via the C++ peacock_gpu library.
///
/// Lifecycle: `new()` registers tables and creates the C executor; `execute()`
/// serializes the GPU-annotated plan to FlatBuffers, calls `peacock_execute`,
/// and deserializes the Arrow IPC result. `Drop` destroys the C executor.
pub struct GpuExecutor {
    ctx: SessionContext,
    executor: *mut PeacockExecutor,
}

// SAFETY: GpuExecutor has exclusive ownership of the PeacockExecutor pointer.
unsafe impl Send for GpuExecutor {}

impl GpuExecutor {
    pub async fn new(
        data_dir: &Path,
        target_partitions: usize,
        gpu_memory_budget: usize,
    ) -> DfResult<Self> {
        let ctx =
            create_context_with_tables(data_dir, target_partitions, gpu_memory_budget).await?;

        let mut executor: *mut PeacockExecutor = std::ptr::null_mut();
        let rc =
            unsafe { peacock_executor_create(gpu_memory_budget as u64, &mut executor) };
        if rc != 0 {
            return Err(DataFusionError::External(
                format!("peacock_executor_create failed with code {rc}").into(),
            ));
        }

        Ok(Self { ctx, executor })
    }

    /// Execute `sql` on the GPU.
    ///
    /// Steps:
    /// 1. Build a GPU-annotated physical plan via the DataFusion session.
    /// 2. Serialize the plan to FlatBuffers with `serialize_plan`.
    /// 3. Call `peacock_execute` — the C++ engine runs the plan on the GPU.
    /// 4. Deserialize the Arrow IPC stream result back to `Vec<RecordBatch>`.
    ///
    /// Returns an empty vec while the C++ result serialization is not yet
    /// implemented (out_result_len == 0).
    pub async fn execute(&self, sql: &str) -> DfResult<Vec<RecordBatch>> {
        let plan = self.ctx.sql(sql).await?.create_physical_plan().await?;
        let plan_bytes = serialize_plan(&plan)
            .map_err(|e| DataFusionError::External(e.into()))?;

        let mut out_ptr: *mut u8 = std::ptr::null_mut();
        let mut out_len: u64 = 0;

        let rc = unsafe {
            peacock_execute(
                self.executor,
                plan_bytes.as_ptr(),
                plan_bytes.len() as u64,
                &mut out_ptr,
                &mut out_len,
            )
        };

        if rc != 0 {
            let msg = unsafe {
                let ptr = peacock_last_error(self.executor);
                std::ffi::CStr::from_ptr(ptr).to_string_lossy().into_owned()
            };
            return Err(DataFusionError::External(
                format!("peacock_execute failed (code {rc}): {msg}").into(),
            ));
        }

        if out_len == 0 || out_ptr.is_null() {
            return Ok(vec![]);
        }

        let ipc_bytes = unsafe { std::slice::from_raw_parts(out_ptr, out_len as usize) };
        let batches = read_ipc_stream(ipc_bytes)
            .map_err(|e| DataFusionError::External(Box::new(e)))?;
        unsafe { peacock_result_free(out_ptr) };

        Ok(batches)
    }
}

impl Drop for GpuExecutor {
    fn drop(&mut self) {
        if !self.executor.is_null() {
            unsafe { peacock_executor_destroy(self.executor) };
        }
    }
}

fn read_ipc_stream(bytes: &[u8]) -> Result<Vec<RecordBatch>, arrow::error::ArrowError> {
    StreamReader::try_new(std::io::Cursor::new(bytes), None)?.collect()
}
