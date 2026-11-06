//! The fused benchmark's host-side rules: which errors are out of memory, where a batch's
//! section and record go, and how a rerun replaces its record rows.

#[cfg(test)]
mod tests;

use std::path::{Path, PathBuf};

use crate::executor::RunError;
use crate::test_support::{COLUMNS, RunMeta, append_records_to, testdata_root};

pub(crate) fn is_out_of_memory(error: &RunError) -> bool {
    let text = error.to_string().to_lowercase();
    ["out of memory", "out_of_memory", "bad_alloc"]
        .iter()
        .any(|sign| text.contains(sign))
}

pub(crate) fn fused_results_file(device: &str, dataset: &str, sf: &str) -> PathBuf {
    testdata_root()
        .join("benchmark-results")
        .join(device)
        .join("fused")
        .join(format!("{dataset}.sf{sf}"))
        .join("tp1_single.fused.txt")
}

/// The fused record, gitignored: the metrics pass reads it.
pub(crate) fn fused_record_path(dataset: &str, sf: &str) -> PathBuf {
    testdata_root()
        .join("calibration")
        .join("fused")
        .join(format!("{dataset}.sf{sf}"))
        .join("records.tsv")
}

/// The test that runs `batch` of `dataset`, which is also the record's `query` for its rows.
pub(crate) fn case_name(dataset: &str, batch: &str) -> String {
    format!("bench_fused_{dataset}_sf40_{batch}_tp1_single")
}

pub(crate) fn replace_case_rows(path: &Path, rows: &[String], meta: &RunMeta<'_>) {
    if let Ok(text) = std::fs::read_to_string(path) {
        let query = COLUMNS
            .iter()
            .position(|column| *column == "query")
            .expect("a record row names its query");
        let mine =
            |line: &&str| !line.starts_with('#') && line.split('\t').nth(query) == Some(meta.query);
        if text.lines().any(|line| mine(&line)) {
            let kept: Vec<&str> = text.lines().filter(|line| !mine(line)).collect();
            let staged = path.with_extension("tsv.new");
            std::fs::write(&staged, format!("{}\n", kept.join("\n")))
                .unwrap_or_else(|e| panic!("staging {}: {e}", path.display()));
            std::fs::rename(&staged, path)
                .unwrap_or_else(|e| panic!("publishing {}: {e}", path.display()));
        }
    }
    append_records_to(path, rows, meta);
}
