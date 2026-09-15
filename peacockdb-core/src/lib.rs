//! The engine: a lane holds a stream of batches rather than one resident table.
//!
//! Seven components, each of which is its own `mod.rs` and nothing else. `plan` is the
//! vocabulary — the nodes, the layout and schema they declare, and the rules a tree has to
//! satisfy. `wire` is what crosses to the C++ side. `planner` builds a plan, `executor` runs
//! one, and `plan_text` renders any of it. `common` is the row-byte formula all four price
//! by. `test_support` is the harness's and exists only behind its feature. The reasons
//! behind each shape are in `llm-wiki/architecture.md`.

// Bare `pub` claims the CLI calls the item; the lint holds the claim once the components'
// items are `pub(crate)` (`coding-style.md`, Visibility).
#![warn(unreachable_pub)]

#[cfg(all(feature = "gpu", feature = "rust-only"))]
compile_error!("gpu needs the FFI linked; rust-only removes it. Pass one or neither.");

pub mod common;
pub mod executor;
pub mod plan;
pub mod plan_text;
pub mod planner;
#[cfg(feature = "test-support")]
pub mod test_support;
#[cfg(test)]
mod tests;
pub mod wire;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use datafusion::datasource::file_format::parquet::ParquetFormat;
use datafusion::datasource::listing::{
    ListingOptions, ListingTable, ListingTableConfig, ListingTableUrl,
};
use datafusion::error::Result;
use datafusion::execution::SessionStateBuilder;
use datafusion::execution::context::SessionContext;

pub fn build_session_state(target_partitions: usize) -> SessionContext {
    let base = SessionContext::new();
    let mut config = base.state().config().clone();
    config.options_mut().execution.target_partitions = target_partitions;
    let state = SessionStateBuilder::new_from_existing(base.state())
        .with_config(config)
        .build();

    SessionContext::new_with_state(state)
}

async fn read_table(
    path: PathBuf,
    ctx: &SessionContext,
) -> Result<(String, Arc<ListingTable>), ()> {
    if path.extension().and_then(|e| e.to_str()) != Some("parquet") {
        ()
    }

    let table_name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| {
            datafusion::error::DataFusionError::Plan(format!(
                "could not derive table name from path: {}",
                path.display()
            ))
        })
        .unwrap()
        .to_string();

    let table_url = ListingTableUrl::parse(path.to_str().unwrap()).unwrap();
    let format = Arc::new(ParquetFormat::default().with_enable_pruning(true));
    let listing_options = ListingOptions::new(format).with_file_extension(".parquet");

    let resolved_schema = listing_options
        .infer_schema(&ctx.state(), &table_url)
        .await
        .unwrap();

    let config = ListingTableConfig::new(table_url)
        .with_listing_options(listing_options)
        .with_schema(resolved_schema);

    let table = Arc::new(ListingTable::try_new(config).unwrap());

    Ok((table_name, table))
}

pub async fn register_tables_for(ctx: SessionContext, data_dir: &Path) -> Result<SessionContext> {
    for entry in std::fs::read_dir(data_dir)? {
        let path = entry?.path();
        let Ok((table_name, table)) = read_table(path, &ctx).await else {
            continue;
        };
        ctx.register_table(&table_name, table)?;
    }

    Ok(ctx)
}
