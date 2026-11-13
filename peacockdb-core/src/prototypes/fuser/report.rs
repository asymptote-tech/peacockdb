//! The fused batches' tables on bench-rows' report page: one per fused file, a row per batch,
//! read through the fused format's own reader. Beside each batch stand its queries run alone:
//! the sf40 baseline's load and execute summed, and its loads per table, which each fused read
//! count is out of.

#[cfg(test)]
mod tests;

use std::path::Path;

use crate::test_support::{
    FAILED, ReportCell, ReportTable, SKIPPED, ordered_sections, parse_benchmark_section,
    report_gib, report_ms, section_href,
};

use super::parse_fused_section;

/// The fused file's name in each `<device>/fused/<ds>.sf<sf>/`.
const FUSED_FILE: &str = "tp1_single.fused.txt";

/// The baseline's file in each `<device>/<ds>.sf<sf>/`: the batch's queries run alone.
const BASELINE_FILE: &str = "tp1_single.benchmark.txt";

/// Queries per line of the queries cell, so the row fits the page without scrolling sideways.
const QUERIES_PER_LINE: usize = 10;

const COLUMNS: [&str; 10] = [
    "batch",
    "queries",
    "outcome",
    "nonfused total load (ms)",
    "fused load (ms)",
    "nonfused total execute (ms)",
    "fused execute (ms)",
    "accountant's peak ≥ (GiB)",
    "cap",
    "reads (fused/nonfused)",
];

pub(crate) fn fused_report_tables(root: &Path) -> Vec<ReportTable> {
    fused_files(root)
        .into_iter()
        .map(|(device, dataset, sf)| {
            let rel = format!("{device}/fused/{dataset}.sf{sf}/{FUSED_FILE}");
            let text =
                std::fs::read_to_string(root.join(&rel)).unwrap_or_else(|e| panic!("{rel}: {e}"));
            let href = |line: usize| section_href(&format!("benchmark-results/{rel}"), line);
            let base_rel = format!("{device}/{dataset}.sf{sf}/{BASELINE_FILE}");
            let baseline = std::fs::read_to_string(root.join(&base_rel))
                .map(|text| ordered_sections(&text))
                .map_err(|e| format!("{base_rel}: {e}"));
            // `ordered_sections` opens a section at every `== ` line, so the two zip.
            let headers = text
                .lines()
                .enumerate()
                .filter(|(_, line)| line.starts_with("== "));
            let rows = headers
                .zip(ordered_sections(&text))
                .map(|((at, _), (batch, body))| row(batch, &body, href(at + 1), &baseline))
                .collect();
            ReportTable {
                id: format!("fused-{device}-{dataset}-sf{sf}"),
                heading: format!("Fused batches: {device}, {dataset} sf{sf}"),
                columns: COLUMNS.iter().map(|column| column.to_string()).collect(),
                rows,
            }
        })
        .collect()
}

/// `(device, dataset, sf)` of every fused file under `root`, by device name, then dataset, then
/// sf as a number.
fn fused_files(root: &Path) -> Vec<(String, String, String)> {
    let names = |dir: &Path| -> Vec<String> {
        std::fs::read_dir(dir)
            .into_iter()
            .flatten()
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect()
    };
    let mut found: Vec<(String, String, u64, String)> = Vec::new();
    for device in names(root) {
        let fused = root.join(&device).join("fused");
        for set in names(&fused) {
            let Some((dataset, sf)) = set.split_once(".sf") else {
                continue;
            };
            let Ok(scale) = sf.parse::<u64>() else {
                continue;
            };
            if fused.join(&set).join(FUSED_FILE).is_file() {
                found.push((device.clone(), dataset.to_string(), scale, sf.to_string()));
            }
        }
    }
    found.sort();
    found
        .into_iter()
        .map(|(device, dataset, _, sf)| (device, dataset, sf))
        .collect()
}

fn cell(text: impl Into<String>) -> ReportCell {
    ReportCell {
        text: text.into(),
        href: None,
        class: None,
    }
}

fn classed(text: impl Into<String>, class: &'static str) -> ReportCell {
    ReportCell {
        class: Some(class),
        ..cell(text)
    }
}

/// The batch's queries run alone, off the baseline's sections: load and execute summed, and the
/// loads of each table.
struct Alone {
    load_us: u64,
    execute_us: u64,
    reads: Vec<(String, usize)>,
}

/// Panics naming a query the baseline does not hold or did not measure: no fallback.
fn alone(baseline: &Result<Vec<(String, String)>, String>, queries: &[&str]) -> Alone {
    let sections = baseline
        .as_ref()
        .unwrap_or_else(|why| panic!("the fused batches' baseline: {why}"));
    let mut sum = Alone {
        load_us: 0,
        execute_us: 0,
        reads: Vec::new(),
    };
    for query in queries {
        // The tree's writer puts each query in once: the first is the one, as the fuser reads it.
        let (_, body) = sections
            .iter()
            .find(|(name, _)| name == query)
            .unwrap_or_else(|| panic!("the fused batches' baseline has no section for {query}"));
        let section = parse_benchmark_section(body)
            .unwrap_or_else(|| panic!("the fused batches' baseline did not measure {query}"));
        let trailer = |key: &str| {
            section
                .count(key)
                .unwrap_or_else(|| panic!("{query}: the baseline's trailer carries no {key}"))
        };
        sum.load_us += trailer("load_us");
        sum.execute_us += trailer("execute_us");
        let loads = section
            .nodes
            .iter()
            .filter(|node| node.line.name == "GpuLoadParquet");
        for node in loads {
            let table = node
                .line
                .field("table")
                .unwrap_or_else(|| panic!("{query}: a GpuLoadParquet without a table"));
            match sum.reads.iter_mut().find(|(name, _)| name == table) {
                Some((_, n)) => *n += 1,
                None => sum.reads.push((table.to_string(), 1)),
            }
        }
    }
    sum
}

/// A fused section's `[a,b,c]` list, its items.
fn items(list: &str) -> Vec<&str> {
    let inner = list
        .strip_prefix('[')
        .and_then(|rest| rest.strip_suffix(']'))
        .unwrap_or_else(|| panic!("a fused list is bracketed, not {list}"));
    inner.split(',').filter(|item| !item.is_empty()).collect()
}

/// The queries, `QUERIES_PER_LINE` to a line.
fn queries_cell(queries: &[&str]) -> String {
    queries
        .chunks(QUERIES_PER_LINE)
        .map(|line| line.join(", "))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Each fused `table:n` as `table:n/m`, one to a line, `m` the table's loads run alone.
fn reads_cell(reads: &str, alone: &Alone) -> String {
    items(reads)
        .into_iter()
        .map(|read| {
            let (table, _) = read
                .split_once(':')
                .unwrap_or_else(|| panic!("a fused read is table:count, not {read}"));
            let of = alone
                .reads
                .iter()
                .find(|(name, _)| name == table)
                .map_or(0, |(_, n)| *n);
            format!("{read}/{of}")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// One batch's row: a marker, a failed batch, an out-of-memory one or a measured one.
fn row(
    batch: String,
    body: &str,
    href: String,
    baseline: &Result<Vec<(String, String)>, String>,
) -> Vec<ReportCell> {
    let none = || cell("—");
    let link = ReportCell {
        href: Some(href),
        ..cell(batch)
    };
    if body.starts_with(SKIPPED) {
        let mut row = vec![link, none(), classed("same set as all", "skip")];
        row.extend((3..COLUMNS.len()).map(|_| none()));
        return row;
    }
    if let Some(reason) = body.strip_prefix(FAILED) {
        let reason = reason.lines().next().unwrap_or("");
        let mut row = vec![link, none(), classed(format!("failed: {reason}"), "off")];
        row.extend((3..COLUMNS.len()).map(|_| none()));
        return row;
    }
    let section = parse_fused_section(body).expect("neither a marker nor a failed batch");
    let field = |key: &str| {
        section
            .field(key)
            .unwrap_or_else(|| panic!("a fused section without `{key}`"))
    };
    let cap = field("cap");
    let queries = items(field("queries"));
    let alone = alone(baseline, &queries);
    let (outcome, load, execute, peak) = match field("outcome") {
        "measured" => (
            cell("measured"),
            cell(report_ms(
                section.count("load_us").expect("a measured load_us"),
            )),
            cell(report_ms(
                section.count("execute_us").expect("a measured execute_us"),
            )),
            cell(report_gib(
                section.count("peak_bytes").expect("a measured peak_bytes"),
            )),
        ),
        "out-of-memory" => (
            classed(format!("out of memory (cap {cap})"), "off"),
            none(),
            none(),
            none(),
        ),
        other => panic!("a fused section's outcome is measured or out-of-memory, not {other}"),
    };
    vec![
        link,
        cell(queries_cell(&queries)),
        outcome,
        cell(report_ms(alone.load_us)),
        load,
        cell(report_ms(alone.execute_us)),
        execute,
        peak,
        cell(cap),
        cell(reads_cell(field("reads"), &alone)),
    ]
}
