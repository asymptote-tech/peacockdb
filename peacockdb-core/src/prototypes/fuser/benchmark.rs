//! The fused benchmark's host-side rules: which errors are out of memory, where a batch's
//! section and record go, how a rerun replaces its record rows, and what a batch reads of the
//! sf40 baseline. `device` is the run itself.

#[cfg(not(feature = "rust-only"))]
mod device;
#[cfg(test)]
mod tests;

use std::path::{Path, PathBuf};

use crate::executor::{RunError, nodes_as_recorded};
use crate::plan::GpuNode;
use crate::test_support::testdata_root;
use crate::test_support::{COLUMNS, FAILED, Provenance, RunMeta, SKIPPED, append_records_to};
use crate::test_support::{ordered_sections, parse_benchmark_section, provenance_fields};
use crate::wire::{FbKind, attach_recipes};

use super::{Baseline, DEVICE, batches};

/// One fused batch timed on the device into its section and record rows.
#[cfg(not(feature = "rust-only"))]
pub(crate) async fn benchmark_fused(dataset: &str, sf: &str, batch: &str) {
    device::benchmark_fused(dataset, sf, batch).await
}

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

/// `calibration/<DEVICE>/<dataset>.sf<sf>/loads.tsv`: pinned-loads' load times, which price a
/// re-read.
pub(crate) fn load_times_path(dataset: &str, sf: &str) -> PathBuf {
    testdata_root()
        .join("calibration")
        .join(DEVICE)
        .join(format!("{dataset}.sf{sf}"))
        .join("loads.tsv")
}

/// By the tree's pre-order, the seq of each load's `CudfScan` call, `None` at every other node:
/// the key `loads.tsv` prices a load by.
pub(crate) fn scan_seqs(tree: &dyn GpuNode, join_scratch: u64) -> Vec<Option<u64>> {
    let recipes = attach_recipes(tree, join_scratch).unwrap_or_else(|e| panic!("recipes: {e}"));
    let nodes = nodes_as_recorded(tree).unwrap_or_else(|e| panic!("the tree indexes: {e}"));
    nodes
        .iter()
        .map(|(_, post_order)| {
            let recipe = recipes.get(*post_order)?;
            recipe.calls.iter().find_map(|call| match call.target {
                Some((seq, FbKind::Scan)) => Some(u64::from(seq)),
                _ => None,
            })
        })
        .collect()
}

/// The sections of the fused file, in `BATCH_SIZES` order: a batch the list does not exceed is
/// a marker.
pub(crate) fn declared_batches(list: &[String]) -> Vec<(String, Option<String>)> {
    batches(list)
        .into_iter()
        .map(|(name, batch)| {
            (
                name.to_string(),
                batch.err().map(|why| format!("{SKIPPED}{why}")),
            )
        })
        .collect()
}

/// An out-of-memory batch's `--- run ---`: the five provenance keys alone, which is what a
/// resumed run keeps it by.
pub(crate) fn out_of_memory_run(provenance: &Provenance) -> String {
    let mut run = String::from("--- run ---\n");
    for (key, value) in provenance_fields(provenance) {
        run.push_str(&format!("{key}={value}\n"));
    }
    run
}

/// Whether `text` holds a section for `batch` measured or out of memory under `provenance`:
/// a resumed run keeps it. Every section of that name is read, not the first.
pub(crate) fn holds_this_run(text: &str, batch: &str, provenance: &Provenance) -> bool {
    let five = provenance_fields(provenance);
    ordered_sections(text)
        .iter()
        .filter(|(name, _)| name == batch)
        .any(|(_, body)| {
            !body.starts_with(FAILED)
                && super::parse_fused_section(body).is_some_and(|section| {
                    section.trailer.len() >= five.len()
                        && section.trailer[section.trailer.len() - five.len()..] == five
                })
        })
}

/// The index of the run worth reporting: second-smallest by time, as the tree benchmark picks.
pub(crate) fn second_smallest_at(times: &[u64]) -> usize {
    let mut order: Vec<usize> = (0..times.len()).collect();
    order.sort_by_key(|at| times[*at]);
    order[1]
}

/// The sf40 baseline of a batch: each query's section of the device's `tp1_single` tree, in
/// batch order. Panics naming a query whose section is missing or not measured: no fallback.
pub(crate) fn baseline_for(text: &str, queries: &[String]) -> Baseline {
    let sections = ordered_sections(text);
    let parsed: Vec<_> = queries
        .iter()
        .map(|query| {
            let (_, body) = sections
                .iter()
                .find(|(name, _)| name == query)
                .unwrap_or_else(|| panic!("the sf40 baseline has no section for {query}"));
            parse_benchmark_section(body)
                .unwrap_or_else(|| panic!("the sf40 baseline did not measure {query}: {body}"))
        })
        .collect();
    Baseline::of(&parsed)
}
