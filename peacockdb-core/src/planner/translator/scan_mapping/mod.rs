//! The scan mapping: which row groups a scan reads, and how they are cut into lanes and
//! batches.
//!
//! Three entry points over 720 lines, all three called from `Translator::source` — pruning
//! reads the parquet statistics, the metadata pass turns what survives into row counts and
//! bytes, and the partitioner cuts that into a lane-major list of batches. Nothing outside
//! the translator asks any of them, which is what makes this a subcomponent rather than a
//! peer: as a sibling of `translator` it would be the design's only subcomponent-to-
//! subcomponent edge.

mod parquet_meta;
mod partition;
mod rowgroup_prune;

use datafusion::datasource::physical_plan::ParquetExec;

use crate::plan::{Batching, PlanError, RowGroupMeta, ScanMetadata};

/// The table a scan reads, by the name the query used.
pub(crate) fn parquet_table_name(parquet: &ParquetExec) -> Option<String> {
    parquet_meta::parquet_table_name(parquet)
}

/// The row groups pruning leaves, with the rows and projected bytes of each.
pub(crate) fn survivor_metadata(parquet: &ParquetExec) -> Result<ScanMetadata, PlanError> {
    parquet_meta::survivor_metadata(parquet)
}

/// The surviving row groups cut into lanes, then into batches — lane-major, and whole row
/// groups throughout, which is the granularity the scan can address.
pub(crate) fn partition(
    survivors: &[RowGroupMeta],
    n_partitions: usize,
    batching: Batching,
) -> Result<Vec<Vec<Vec<u32>>>, PlanError> {
    partition::partition(survivors, n_partitions, batching)
}
