//! Row-group metadata for the partitioner, read at plan time.
//!
//! What it adds to the pruning pass is the per-group rows and bytes the mapping needs.
//! Bytes are the parquet
//! column-chunk totals over the projected columns: a varchar's width is a property of
//! the data, and the file already knows it.

use crate::plan::ScanMetadata;
use std::collections::BTreeSet;

use datafusion::datasource::physical_plan::ParquetExec;
use datafusion::parquet::file::reader::{FileReader, SerializedFileReader};

use super::rowgroup_prune::surviving_row_groups;
use crate::plan::PlanError;
use crate::plan::RowGroupMeta;

/// The table a scan reads, named after the parquet file rather than declared anywhere:
/// DataFusion's `ParquetExec` carries paths, and the plan text and every node above it
/// name the table.
pub(crate) fn parquet_table_name(parquet: &ParquetExec) -> Option<String> {
    let file = parquet.base_config().file_groups.first()?.first()?;
    file.object_meta
        .location
        .to_string()
        .rsplit('/')
        .next()?
        .strip_suffix(".parquet")
        .map(String::from)
}

pub(crate) fn survivor_metadata(parquet: &ParquetExec) -> Result<ScanMetadata, PlanError> {
    let config = parquet.base_config();
    // At tp>1 DataFusion splits ONE file into several byte-range entries, all with the same
    // path, so entries are not files. Genuinely several files would each have their own row
    // groups, and this mapping addresses one file's — measuring the first and sizing the
    // whole plan from it would be a wrong answer rather than an error.
    let paths: BTreeSet<String> = config
        .file_groups
        .iter()
        .flatten()
        .map(|file| format!("/{}", file.object_meta.location))
        .collect();
    if paths.len() > 1 {
        return Err(PlanError::Unsupported(format!(
            "a scan over {} files: the row-group mapping addresses one file",
            paths.len()
        )));
    }
    // A local path, since the mapping is read off the file here at plan time. An object
    // store's location would simply not open, which is an error rather than a wrong answer.
    let path = paths
        .into_iter()
        .next()
        .ok_or_else(|| PlanError::Invalid("a scan with no files".to_string()))?;

    let file =
        std::fs::File::open(&path).map_err(|e| PlanError::Invalid(format!("{path}: {e}")))?;
    let reader =
        SerializedFileReader::new(file).map_err(|e| PlanError::Invalid(format!("{path}: {e}")))?;

    // The projection indexes the file schema, and a column chunk sits at the same
    // position: every table this engine reads is flat.
    let projected: Vec<usize> = match &config.projection {
        Some(columns) => columns.clone(),
        None => (0..config.file_schema.fields().len()).collect(),
    };
    let survivors = surviving_row_groups(parquet);

    let mut metadata = Vec::new();
    let mut can_be_null = vec![false; projected.len()];
    for (index, group) in reader.metadata().row_groups().iter().enumerate() {
        let index = index as u32;
        if survivors
            .as_ref()
            .is_some_and(|kept| !kept.contains(&index))
        {
            continue;
        }
        // A projection index with no column chunk would silently make the source look
        // smaller, and bytes decide both the batch size and the lane count.
        let mut bytes: i64 = 0;
        for column in &projected {
            let chunk = group.columns().get(*column).ok_or_else(|| {
                PlanError::Invalid(format!(
                    "{path}: projected column {column} has no chunk in row group {index}, so \
                     the schema is not flat and a projection index is not a chunk position"
                ))
            })?;
            bytes += chunk.uncompressed_size();
        }
        for (position, column) in projected.iter().enumerate() {
            let nulls = group.columns()[*column]
                .statistics()
                .and_then(|statistics| statistics.null_count_opt());
            // No statistic is not a promise of no nulls.
            can_be_null[position] |= nulls.is_none_or(|count| count > 0);
        }
        metadata.push(RowGroupMeta {
            index,
            rows: group.num_rows() as u64,
            bytes: bytes as u64,
        });
    }
    Ok(ScanMetadata {
        file: path,
        groups: metadata,
        can_be_null,
    })
}

#[cfg(test)]
mod tests;
