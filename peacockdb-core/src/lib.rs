pub mod batch_partitioned;
pub mod config;
pub mod executor;
pub mod gpu_rowgroup_prune;
pub mod memory;
pub mod plan_text;
#[allow(unused_imports, dead_code, clippy::all)]
pub mod generated {
    pub mod gpu_plan_generated {
        include!(concat!(env!("OUT_DIR"), "/gpu_plan_generated.rs"));
    }
}
pub mod spark_partitioning;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use datafusion::datasource::file_format::parquet::ParquetFormat;
use datafusion::datasource::listing::{ListingOptions, ListingTable, ListingTableConfig, ListingTableUrl};
use datafusion::execution::context::SessionContext;
use datafusion::execution::SessionStateBuilder;
use datafusion::error::Result;

pub fn build_session_state(
    target_partitions: usize
) -> SessionContext {
    let base = SessionContext::new();
    let mut config = base.state().config().clone();
    config.options_mut().execution.target_partitions = target_partitions;
    let state = SessionStateBuilder::new_from_existing(base.state())
        .with_config(config)
        .build();
    
    SessionContext::new_with_state(state)
}

async fn read_table(path: PathBuf, ctx: &SessionContext) -> Result<(String, Arc<ListingTable>), ()> {
    if path.extension().and_then(|e| e.to_str()) != Some("parquet") {
        ()
    }

    let table_name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| datafusion::error::DataFusionError::Plan(
            format!("could not derive table name from path: {}", path.display()),
        )).unwrap()
        .to_string();

    let table_url = ListingTableUrl::parse(path.to_str().unwrap()).unwrap();
    let format = Arc::new(ParquetFormat::default().with_enable_pruning(true));
    let listing_options = ListingOptions::new(format).with_file_extension(".parquet");

    let resolved_schema = listing_options.infer_schema(&ctx.state(), &table_url).await.unwrap();

    let config = ListingTableConfig::new(table_url)
        .with_listing_options(listing_options)
        .with_schema(resolved_schema);

    let table = Arc::new(ListingTable::try_new(config).unwrap());

    Ok((table_name, table))
}

pub async fn register_tables_for(
    ctx: SessionContext,
    data_dir: &Path
) -> Result<SessionContext> {
    for entry in std::fs::read_dir(data_dir)? {
        let path = entry?.path();
        let Ok((table_name, table)) = read_table(path, &ctx).await else { continue; }; 
        ctx.register_table(&table_name, table)?;
    }

    Ok(ctx)
}
