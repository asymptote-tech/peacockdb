//! One session over one operator, and the two moves a batch makes across it.

use std::ffi::c_void;
use std::sync::Arc;

use datafusion::arrow::array::{Array, ArrayRef, StructArray};
use datafusion::arrow::compute::concat_batches;
use datafusion::arrow::datatypes::Field;
use datafusion::arrow::ffi::{FFI_ArrowArray, FFI_ArrowSchema, to_ffi};
use datafusion::arrow::ipc::reader::StreamReader;
use datafusion::arrow::record_batch::RecordBatch;
use peacockdb_ffi::raw::{
    PeacockExecutor, peacock_executor_begin_plan, peacock_executor_create,
    peacock_executor_destroy, peacock_executor_end_plan, peacock_handle_from_arrow,
    peacock_last_error, peacock_result_free, peacock_result_from_handle,
};

use crate::executor::{Batch, CpuBatch, GpuBatch, GpuContext, RowRange};
use crate::plan::GpuNode;
use crate::test_support::GPU_BUDGET;
use crate::wire::attach_recipes;

/// One session over one operator: `attach_recipes` over the node, its bytes handed to
/// `begin_plan`, and the context `GpuBackend::executors_for` reads. Batches go up through
/// [`Device::upload`] and come back through [`Device::fetch`], so a test never writes a
/// file and the scan is never in the path of an operator it is not testing.
pub(crate) struct Device {
    ctx: GpuContext,
}

impl Device {
    pub(crate) fn open(node: &dyn GpuNode) -> Self {
        let recipes = attach_recipes(node).expect("every node's payload is writable");
        let mut executor: *mut PeacockExecutor = std::ptr::null_mut();
        assert_eq!(
            unsafe { peacock_executor_create(GPU_BUDGET as u64, &mut executor) },
            0,
            "executor_create"
        );
        let bytes = recipes.bytes();
        let mut nodes = 0u64;
        let rc = unsafe {
            peacock_executor_begin_plan(executor, bytes.as_ptr(), bytes.len() as u64, &mut nodes)
        };
        assert_eq!(rc, 0, "begin_plan: {}", error_of(executor));
        assert_eq!(nodes as usize, recipes.wire_nodes());
        Self {
            ctx: GpuContext { executor, recipes },
        }
    }

    pub(crate) fn ctx(&self) -> &GpuContext {
        &self.ctx
    }

    /// The batch as the device's table, priced exactly as `CpuBatch` prices the same rows,
    /// so an accumulator's byte threshold trips at the same arrival on both sides.
    pub(crate) fn upload(&self, batch: &RecordBatch) -> GpuBatch {
        let columns: Vec<(Arc<Field>, ArrayRef)> = batch
            .schema()
            .fields()
            .iter()
            .cloned()
            .zip(batch.columns().iter().cloned())
            .collect();
        let table = StructArray::from(columns);
        let (array, schema) = to_ffi(&table.to_data()).expect("arrow exports its own array");
        let mut handle = 0u64;
        let rc = unsafe {
            peacock_handle_from_arrow(
                self.ctx.executor,
                &schema as *const FFI_ArrowSchema as *const c_void,
                &array as *const FFI_ArrowArray as *const c_void,
                &mut handle,
            )
        };
        assert_eq!(rc, 0, "upload: {}", error_of(self.ctx.executor));
        let bytes = CpuBatch::new(batch.clone()).byte_size();
        GpuBatch::new(self.ctx.executor, handle, batch.num_rows(), bytes)
    }

    /// What the device exported, under the schema it exported it with — never the one the
    /// node declared, so a type the device changed reaches the comparator as itself.
    /// `None` is the device shipping nothing: a range naming no rows of a non-empty table.
    pub(crate) fn fetch(&self, batch: GpuBatch, rows: RowRange) -> Option<RecordBatch> {
        let mut ipc: *mut u8 = std::ptr::null_mut();
        let mut len = 0u64;
        let rc = unsafe {
            peacock_result_from_handle(
                self.ctx.executor,
                batch.handle(),
                rows.offset,
                rows.length,
                &mut ipc,
                &mut len,
            )
        };
        assert_eq!(rc, 0, "fetch: {}", error_of(self.ctx.executor));
        if len == 0 {
            return None;
        }
        let bytes = unsafe { std::slice::from_raw_parts(ipc, len as usize) };
        let reader =
            StreamReader::try_new(std::io::Cursor::new(bytes), None).expect("an IPC stream");
        // Off the reader rather than the first batch: a zero-row table is a stream that
        // may carry its schema and no batch at all.
        let schema = reader.schema();
        let batches: Vec<RecordBatch> = reader.collect::<Result<_, _>>().expect("every batch");
        unsafe { peacock_result_free(ipc) };
        Some(concat_batches(&schema, &batches).expect("one table"))
    }
}

impl Drop for Device {
    fn drop(&mut self) {
        unsafe {
            peacock_executor_end_plan(self.ctx.executor);
            peacock_executor_destroy(self.ctx.executor);
        }
    }
}

pub(crate) fn error_of(executor: *mut PeacockExecutor) -> String {
    let message = unsafe { peacock_last_error(executor) };
    if message.is_null() {
        return String::new();
    }
    unsafe { std::ffi::CStr::from_ptr(message) }
        .to_string_lossy()
        .into_owned()
}
