use std::path::PathBuf;

use clap::Parser;
use datafusion::arrow::util::pretty::print_batches;
use peacockdb_core::batch_partitioned::cpu_backend::backend::CpuBackend;
use peacockdb_core::batch_partitioned::driver::batch_partitioned_driver;
use peacockdb_core::batch_partitioned::plan::{
    BatchSizing, PlanKnobs, SMALL_TABLE_BYTES, plan_batch_partitioned,
};
use peacockdb_core::{build_session_state, register_tables_for};

#[derive(Parser)]
#[command(name = "peacockdb", about = "GPU-accelerated analytical database")]
struct Cli {
    /// Directory of Parquet files; each file becomes a table named after its stem.
    #[arg(long)]
    data_dir: PathBuf,

    /// SQL query to execute.
    #[arg(long)]
    query: String,

    /// Lanes to plan for (defaults to number of CPUs).
    #[arg(long)]
    target_partitions: Option<usize>,

    /// Memory budget in bytes the batch sizes are derived from (defaults to 2 GiB).
    #[arg(long, default_value_t = 2 * 1024 * 1024 * 1024)]
    memory_budget: u64,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    let target_partitions = cli.target_partitions.unwrap_or_else(num_cpus::get);
    let ctx = register_tables_for(build_session_state(target_partitions), &cli.data_dir).await?;
    let plan = ctx.sql(&cli.query).await?.create_physical_plan().await?;
    let knobs = PlanKnobs {
        target_partitions,
        sizing: BatchSizing::Budgeted,
        budget: cli.memory_budget,
        small_table_bytes: SMALL_TABLE_BYTES,
    };
    let (tree, _memory) = plan_batch_partitioned(&plan, knobs)
        .map_err(|why| anyhow::anyhow!("this query cannot be planned: {why}"))?;
    let report = batch_partitioned_driver::<CpuBackend>(tree.as_ref(), &ctx.task_ctx(), None)
        .map_err(|why| anyhow::anyhow!("{why}"))?;

    let batches: Vec<_> = report
        .batches
        .iter()
        .map(|batch| batch.record_batch().clone())
        .collect();
    print_batches(&batches)?;

    Ok(())
}
