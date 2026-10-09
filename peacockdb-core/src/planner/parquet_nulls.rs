//! Does a column hold a NULL? One reader, off the parquet footers, at plan time.
//!
//! Two callers ask it and must not drift, because the answer decides a rewrite and a
//! refusal: the partitioner reads it per surviving row group in the pass that also reads
//! bytes and rows (`translator/scan_mapping/parquet_meta.rs`), and the logical nullability
//! tracer has only a path and a column name. [`nulls_possible`] is the rule they share.

use datafusion::parquet::file::reader::{FileReader, SerializedFileReader};
use datafusion::parquet::file::statistics::Statistics;

use crate::plan::PlanError;

/// Whether a column chunk's statistic leaves a NULL possible. No statistic is not a
/// promise of no nulls.
pub(crate) fn nulls_possible(statistics: Option<&Statistics>) -> bool {
    statistics
        .and_then(|statistics| statistics.null_count_opt())
        .is_none_or(|count| count > 0)
}

/// Whether `column` of the parquet at `path` holds a NULL in any row group.
///
/// Every group is read: pruning belongs to the partitioner, which has a predicate, and this
/// answers about the column as the data holds it.
pub(crate) fn column_may_hold_null(path: &str, column: &str) -> Result<bool, PlanError> {
    let file = std::fs::File::open(path).map_err(|e| PlanError::Invalid(format!("{path}: {e}")))?;
    let reader =
        SerializedFileReader::new(file).map_err(|e| PlanError::Invalid(format!("{path}: {e}")))?;
    let metadata = reader.metadata();
    // A row group carries one chunk per leaf column in schema order, so the position in the
    // descriptor is the position in the group — the flat-schema assumption `parquet_meta.rs`
    // makes for bytes, here for statistics.
    let position = metadata
        .file_metadata()
        .schema_descr()
        .columns()
        .iter()
        .position(|descriptor| descriptor.name() == column)
        .ok_or_else(|| PlanError::Invalid(format!("{path}: no column {column}")))?;
    Ok(metadata
        .row_groups()
        .iter()
        .any(|group| nulls_possible(group.columns()[position].statistics())))
}
