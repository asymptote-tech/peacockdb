//! A corpus query planned at one mode and run whole through the driver on both engines. The
//! harness drives one node and never runs its children, so this is the one way to put a
//! planned shape — a stub under a join, a key the planner promoted — on the device's wire.

use datafusion::arrow::record_batch::RecordBatch;

use crate::executor::{CpuBackend, CpuBatch, GpuBackend, run};
use crate::test_support::{Mode, data_dir_for, queries_dir_for};

/// Each engine's batches for `query` of `dataset` (sf1) at `mode`, cpu first.
pub(crate) async fn answers_on_both(
    dataset: &str,
    query: &str,
    mode: &Mode,
) -> (Vec<RecordBatch>, Vec<RecordBatch>) {
    let what = format!("{query} at {}", mode.name);
    let sql = std::fs::read_to_string(queries_dir_for(dataset).join(format!("{query}.sql")))
        .unwrap_or_else(|why| panic!("{what}: the query text: {why}"));
    let ctx = crate::register_tables_for(
        crate::build_session_state(mode.target_partitions),
        &data_dir_for(dataset, "1"),
    )
    .await
    .unwrap_or_else(|why| panic!("{what}: register {dataset}: {why}"));
    let plan = ctx
        .sql(&sql)
        .await
        .unwrap_or_else(|why| panic!("{what}: {why}"))
        .create_physical_plan()
        .await
        .unwrap_or_else(|why| panic!("{what}: {why}"));
    let (tree, _) = crate::planner::plan(&plan, mode.knobs_for(dataset))
        .unwrap_or_else(|why| panic!("{what}: {why}"));
    let unwrapped = |batches: Vec<CpuBatch>| -> Vec<RecordBatch> {
        batches.iter().map(|b| b.record_batch().clone()).collect()
    };
    let cpu = run::<CpuBackend>(tree.as_ref(), &ctx.task_ctx(), None)
        .unwrap_or_else(|why| panic!("{what} on the cpu: {why}"));
    let device = super::device::Device::open(tree.as_ref());
    let gpu = run::<GpuBackend>(tree.as_ref(), device.ctx(), None)
        .unwrap_or_else(|why| panic!("{what} on the device: {why}"));
    (unwrapped(cpu.batches), unwrapped(gpu.batches))
}
