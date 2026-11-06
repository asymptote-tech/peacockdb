//! The fused batches' tables on bench-rows' report page: one per fused file, a row per batch,
//! read through the fused format's own reader.

#[cfg(test)]
mod tests;

use std::path::Path;

use crate::test_support::{
    FAILED, ReportCell, ReportTable, SKIPPED, ordered_sections, report_gib, report_ms, section_href,
};

use super::parse_fused_section;

/// The fused file's name in each `<device>/fused/<ds>.sf<sf>/`.
const FUSED_FILE: &str = "tp1_single.fused.txt";

const COLUMNS: [&str; 8] = [
    "batch",
    "queries",
    "outcome",
    "fused load (ms)",
    "fused execute (ms)",
    "accountant's peak ≥ (GiB)",
    "cap",
    "reads",
];

pub(crate) fn fused_report_tables(root: &Path) -> Vec<ReportTable> {
    fused_files(root)
        .into_iter()
        .map(|(device, dataset, sf)| {
            let rel = format!("{device}/fused/{dataset}.sf{sf}/{FUSED_FILE}");
            let text =
                std::fs::read_to_string(root.join(&rel)).unwrap_or_else(|e| panic!("{rel}: {e}"));
            let href = |line: usize| section_href(&format!("benchmark-results/{rel}"), line);
            // `ordered_sections` opens a section at every `== ` line, so the two zip.
            let headers = text
                .lines()
                .enumerate()
                .filter(|(_, line)| line.starts_with("== "));
            let rows = headers
                .zip(ordered_sections(&text))
                .map(|((at, _), (batch, body))| row(batch, &body, href(at + 1)))
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

/// One batch's row: a marker, a failed batch, an out-of-memory one or a measured one.
fn row(batch: String, body: &str, href: String) -> Vec<ReportCell> {
    let none = || cell("—");
    let link = ReportCell {
        href: Some(href),
        ..cell(batch)
    };
    if body.starts_with(SKIPPED) {
        let mut row = vec![link, none(), classed("same set as all", "skip")];
        row.extend((0..5).map(|_| none()));
        return row;
    }
    if let Some(reason) = body.strip_prefix(FAILED) {
        let reason = reason.lines().next().unwrap_or("");
        let mut row = vec![link, none(), classed(format!("failed: {reason}"), "off")];
        row.extend((0..5).map(|_| none()));
        return row;
    }
    let section = parse_fused_section(body).expect("neither a marker nor a failed batch");
    let field = |key: &str| {
        section
            .field(key)
            .unwrap_or_else(|| panic!("a fused section without `{key}`"))
    };
    let cap = field("cap");
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
        cell(field("queries")),
        outcome,
        load,
        execute,
        peak,
        cell(cap),
        cell(field("reads")),
    ]
}
