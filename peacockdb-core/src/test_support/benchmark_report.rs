//! The benchmark report page, `testdata/benchmark-report.html`, and `testdata/benchmarks.csv`:
//! every device tree under `benchmark-results/` side by side, through the tree's one reader. The
//! rules are build-test.md's **The page**.
//!
//! - Per dataset, a main table at tp1_single for every device but `MODES_ONLY`, then a collapsed
//!   Modes section with a table per device, its modes side by side under each measure.
//! - A device's `sirius.tsv` is a Sirius group after the devices, with a ratio line above the
//!   table; a DuckDB tree's `duckdb.tsv` is one column after that.
//! - No time, no absolute path, and no host but a DuckDB file's own: the same trees, same bytes.
//! - The CSV is every result of the same trees, its statuses decided where the page's are.

mod csv;
mod decided;
mod sirius;
#[cfg(test)]
mod tests;

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use super::{
    DeviceCase, DuckdbResults, DuckdbStatus, MODES, REPORT_REPO, ReportCell, ReportTable,
    duckdb_results, ordered_sections, parse_device_cases, testdata_root,
};
use decided::{Decided, decided};
use sirius::{Sirius, ratio_line, sirius_cells, sirius_intro, siriuses};

/// What the peak column claims, and no more.
const PEAK_MEANS: &str = "the accountant's peak, a lower bound on device memory";

/// What an `unpinned` load is: its rounds read the files from disk or the page cache, so it is
/// not comparable with a load from pinned memory.
const UNPINNED_MEANS: &str = "read without the pinned host cache: its section's pinned_cache is \
                              off, or the section predates the cache";

/// Devices shown in the Modes sections only: h200's tree predates the pinned cache and the peak,
/// so its tp1_single does not sit beside the current devices'.
const MODES_ONLY: &[&str] = &["h200"];

/// The mode the main table shows.
const MAIN_MODE: &str = "tp1_single";

/// `testdata/report.html`'s rules for the elements this page uses; its `td.g` colours are the
/// classes `off`, `skip` and `na` here.
const STYLE: &str = r#" body { font: 14px/1.4 -apple-system, "Segoe UI", sans-serif; margin: 1.5em; color: #1f2328; }
 table { border-collapse: collapse; margin: .5em 0 1.5em; }
 th, td { border: 1px solid #6e7781; padding: 3px 8px; vertical-align: top; }
 th { background: #f6f8fa; }
 tr:nth-child(even) td { background: #fafbfc; }
 .missing { color: #cf222e; }
 .source { color: #57606a; font-size: 12px; }
 h2 { margin-top: 1.6em; border-bottom: 1px solid #d0d7de; }
 a { color: #0969da; text-decoration: none; }
 a:hover { text-decoration: underline; }
 td.n { text-align: right; font-variant-numeric: tabular-nums; }
 tr td.off { background: #ffebe9; color: #cf222e; }
 tr td.skip { background: #fff8c5; }
 tr td.na { color: #8c959f; text-align: center; }
 td.off a, td.skip a { color: inherit; }
 th.g, td.g { border-left: 3px solid #1f2328; }
 summary { cursor: pointer; font-weight: 600; margin: .5em 0; }
 .wide { overflow-x: auto; max-width: 100%; }
"#;

pub(crate) fn path() -> PathBuf {
    testdata_root().join("benchmark-report.html")
}

pub(crate) fn csv_path() -> PathBuf {
    testdata_root().join("benchmarks.csv")
}

pub(crate) fn section_href(rel: &str, line: usize) -> String {
    format!("https://github.com/{REPORT_REPO}/blob/master/testdata/{rel}#L{line}")
}

/// Where a row sits: each part with its rank first, so the derived order is the page's.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Place {
    dataset: (usize, String),
    sf: (u64, String),
    query: (usize, String),
    mode: usize,
}

/// One device's section for a row: where it is, and its body.
struct Found {
    rel: String,
    line: usize,
    body: String,
}

/// One device column group: its name and its list's rows (none without a `cases.csv`).
struct Device {
    name: String,
    cases: Vec<DeviceCase>,
}

/// One DuckDB column: its tree's name and its files by (dataset, sf).
struct Duckdb {
    name: String,
    files: BTreeMap<(String, String), DuckdbResults>,
}

/// Everything the page and the CSV are rendered from: the device trees' sections by row and
/// device, and the Sirius and DuckDB files beside them.
struct Trees {
    devices: Vec<Device>,
    siriuses: Vec<Sirius>,
    duckdbs: Vec<Duckdb>,
    rows: BTreeMap<Place, BTreeMap<usize, Found>>,
}

pub(crate) fn render(root: &Path, order: &[(String, String)], extra: &[ReportTable]) -> String {
    let trees = trees(root, order);
    page(
        &trees.devices,
        &trees.siriuses,
        &trees.duckdbs,
        &trees.rows,
        extra,
    )
}

pub(crate) fn render_csv(root: &Path, order: &[(String, String)]) -> String {
    csv::render(&trees(root, order), order)
}

fn trees(root: &Path, order: &[(String, String)]) -> Trees {
    let devices = devices(root);
    let mut rows: BTreeMap<Place, BTreeMap<usize, Found>> = BTreeMap::new();
    for (at, device) in devices.iter().enumerate() {
        for (dataset, sf, mode, rel) in files_of(root, &device.name) {
            let text =
                std::fs::read_to_string(root.join(&rel)).unwrap_or_else(|e| panic!("{rel}: {e}"));
            // `ordered_sections` opens a section at every `== ` line, so the two zip.
            let headers = text
                .lines()
                .enumerate()
                .filter(|(_, l)| l.starts_with("== "));
            for ((at_line, _), (query, body)) in headers.zip(ordered_sections(&text)) {
                let line = at_line + 1;
                let place = Place {
                    dataset: (dataset_rank(order, &dataset), dataset.clone()),
                    sf: (sf.parse().expect("an sf of digits"), sf.clone()),
                    query: (query_rank(order, &dataset, &query), query.clone()),
                    mode: mode_rank(&mode),
                };
                let rel = format!("benchmark-results/{rel}");
                let found = Found { rel, line, body };
                if let Some(first) = rows.entry(place).or_default().insert(at, found) {
                    panic!(
                        "{}: {query} has two sections, at lines {} and {line}",
                        first.rel, first.line
                    );
                }
            }
        }
    }
    Trees {
        siriuses: siriuses(root, &devices),
        duckdbs: duckdbs(root),
        devices,
        rows,
    }
}

/// Every directory directly under `root` holding a `<ds>.sf<sf>/duckdb.tsv`, by name.
fn duckdbs(root: &Path) -> Vec<Duckdb> {
    let mut found: BTreeMap<String, Duckdb> = BTreeMap::new();
    for tree in std::fs::read_dir(root).into_iter().flatten().flatten() {
        let name = tree.file_name().to_string_lossy().into_owned();
        for dir in std::fs::read_dir(tree.path())
            .into_iter()
            .flatten()
            .flatten()
        {
            let file = dir.path().join("duckdb.tsv");
            let dirname = dir.file_name().to_string_lossy().into_owned();
            let Some((dataset, sf)) = dirname.split_once(".sf") else {
                continue;
            };
            if !file.is_file() {
                continue;
            }
            let text = std::fs::read_to_string(&file)
                .unwrap_or_else(|e| panic!("{}: {e}", file.display()));
            found
                .entry(name.clone())
                .or_insert_with(|| Duckdb {
                    name: name.clone(),
                    files: BTreeMap::new(),
                })
                .files
                .insert(
                    (dataset.to_string(), sf.to_string()),
                    duckdb_results::parse(&text),
                );
        }
    }
    found.into_values().collect()
}

/// Every directory directly under `root` that holds a mode's benchmark file, by name.
fn devices(root: &Path) -> Vec<Device> {
    let mut names: Vec<String> = std::fs::read_dir(root)
        .unwrap_or_else(|e| panic!("{}: {e}", root.display()))
        .flatten()
        .filter(|entry| entry.path().is_dir())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| !files_of(root, name).is_empty())
        .collect();
    names.sort();
    names
        .into_iter()
        .map(|name| {
            let list = root.join(&name).join("cases.csv");
            let cases = match list.is_file() {
                true => parse_device_cases(&std::fs::read_to_string(&list).unwrap()),
                false => Vec::new(),
            };
            Device { name, cases }
        })
        .collect()
}

/// `(dataset, sf, mode, <device>/<ds>.sf<sf>/<mode>.benchmark.txt)` for every mode file under
/// `device`, sorted, with `/` separators whatever the host's.
fn files_of(root: &Path, device: &str) -> Vec<(String, String, String, String)> {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(root.join(device))
        .into_iter()
        .flatten()
        .flatten()
    {
        let dir = entry.file_name().to_string_lossy().into_owned();
        let Some((dataset, sf)) = dir.split_once(".sf") else {
            continue;
        };
        if sf.is_empty() || !sf.bytes().all(|b| b.is_ascii_digit()) {
            continue;
        }
        for file in std::fs::read_dir(entry.path())
            .into_iter()
            .flatten()
            .flatten()
        {
            let name = file.file_name().to_string_lossy().into_owned();
            let Some(mode) = name.strip_suffix(".benchmark.txt") else {
                continue;
            };
            if MODES.iter().any(|known| known.ident() == mode) {
                let rel = format!("{device}/{dir}/{name}");
                files.push((dataset.to_string(), sf.to_string(), mode.to_string(), rel));
            }
        }
    }
    files.sort();
    files
}

fn dataset_rank(order: &[(String, String)], dataset: &str) -> usize {
    order
        .iter()
        .position(|(of, _)| of == dataset)
        .unwrap_or(usize::MAX)
}

/// A section names a query with `-` where the list's ident has `_` (`aggregate-groupby`).
fn query_rank(order: &[(String, String)], dataset: &str, query: &str) -> usize {
    order
        .iter()
        .position(|(of, ident)| of == dataset && ident.replace('_', "-") == query)
        .unwrap_or(usize::MAX)
}

fn mode_rank(mode: &str) -> usize {
    MODES
        .iter()
        .position(|known| known.ident() == mode)
        .expect("files_of keeps the five modes' files only")
}

fn page(
    devices: &[Device],
    siriuses: &[Sirius],
    duckdbs: &[Duckdb],
    rows: &BTreeMap<Place, BTreeMap<usize, Found>>,
    extra: &[ReportTable],
) -> String {
    let mut groups: BTreeMap<(&(usize, String), &(u64, String)), Vec<&Place>> = BTreeMap::new();
    for place in rows.keys() {
        groups
            .entry((&place.dataset, &place.sf))
            .or_default()
            .push(place);
    }
    let mut html = format!(
        "<!doctype html>\n<html><head><meta charset=\"utf-8\"><title>PeacockDB benchmark \
         results</title>\n<style>\n{STYLE}</style></head><body>\n\
         <p class=\"source\">Generated by the_benchmark_report_matches_the_trees in \
         test_corpus_goldens from testdata/benchmark-results/ · section links @ master</p>\n\
         <p><b>PeacockDB benchmark results</b> — per dataset, a table at {MAIN_MODE}, one row \
         per query, with every device but {} beside DuckDB. Below it, collapsed, the Modes: a \
         table per device, its modes side by side under each measure. PeacockDB's times are \
         device-side, in ms: the total is load plus execute. The peak is {PEAK_MEANS}, in GiB. A load marked \
         unpinned was {UNPINNED_MEANS}.</p>\n",
        MODES_ONLY.join(", ")
    );
    html.push_str(&sirius_intro(siriuses));
    html.push_str(&duckdb_intro(duckdbs));
    html.push_str("<ul>\n");
    for ((dataset, sf), places) in &groups {
        let id = format!("{}-sf{}", dataset.1, sf.1);
        let label = format!("{} sf{}", dataset.1, sf.1);
        let _ = writeln!(
            html,
            "<li><a href=\"#{}\">{}</a> — {} queries</li>",
            escape(&id),
            escape(&label),
            queries_of(places).len()
        );
    }
    for table in extra {
        let _ = writeln!(
            html,
            "<li><a href=\"#{}\">{}</a></li>",
            escape(&table.id),
            escape(&table.heading)
        );
    }
    html.push_str("</ul>\n");
    for ((dataset, sf), places) in &groups {
        html.push_str(&dataset_table(
            &dataset.1, &sf.1, places, devices, siriuses, duckdbs, rows,
        ));
        html.push_str(&modes_section(
            &dataset.1, &sf.1, places, devices, duckdbs, rows,
        ));
    }
    for table in extra {
        html.push_str(&extra_table(table));
    }
    html.push_str("</body></html>\n");
    html
}

/// What each DuckDB file was measured with, from its preamble; nothing without one.
fn duckdb_intro(duckdbs: &[Duckdb]) -> String {
    let named: Vec<String> = duckdbs
        .iter()
        .flat_map(|duckdb| {
            duckdb.files.iter().map(move |((dataset, sf), results)| {
                let of = |key: &str| {
                    results
                        .preamble
                        .iter()
                        .find(|(k, _)| k == key)
                        .map_or("?", |(_, v)| v.as_str())
                };
                format!(
                    "{}, {dataset} sf{sf}: DuckDB {}, {} threads, host {}, floor {} ms",
                    duckdb.name,
                    of("duckdb"),
                    of("threads"),
                    of("host"),
                    of("floor_ms")
                )
            })
        })
        .collect();
    if named.is_empty() {
        return String::new();
    }
    format!(
        "<p>DuckDB's time is one DuckDB CLI process's wall time: the process started, the database \
         opened and the query run, over warm native storage, the second-smallest of its runs, in \
         ms. The floor is SELECT 1 timed the same way, what each time holds besides its query: \
         {}.</p>\n",
        escape(&named.join("; "))
    )
}

/// The group's queries in page order, each once.
fn queries_of<'p>(places: &[&'p Place]) -> Vec<&'p Place> {
    let mut queries: Vec<&Place> = Vec::new();
    for place in places {
        if queries.last().is_none_or(|last| last.query != place.query) {
            queries.push(place);
        }
    }
    queries
}

/// `place` at mode `mode` (an index into `MODES`).
fn at_mode(place: &Place, mode: usize) -> Place {
    Place {
        mode,
        ..place.clone()
    }
}

/// The main table: one row per query, each device but `MODES_ONLY` at `MAIN_MODE`, then each
/// Sirius group, then DuckDB, under the ratio lines. A query no such device has a cell for is
/// left out.
fn dataset_table(
    dataset: &str,
    sf: &str,
    places: &[&Place],
    devices: &[Device],
    siriuses: &[Sirius],
    duckdbs: &[Duckdb],
    rows: &BTreeMap<Place, BTreeMap<usize, Found>>,
) -> String {
    let main = mode_rank(MAIN_MODE);
    let shown: Vec<(usize, &Device)> = devices
        .iter()
        .enumerate()
        .filter(|(_, device)| !MODES_ONLY.contains(&device.name.as_str()))
        .collect();
    let key = (dataset.to_string(), sf.to_string());
    let mut html = "<table>\n<tr><th rowspan=\"2\">query</th>".to_string();
    for (_, device) in &shown {
        let _ = write!(
            html,
            "<th colspan=\"4\">{} · {MAIN_MODE}</th>",
            escape(&device.name)
        );
    }
    for sirius in siriuses {
        let _ = write!(
            html,
            "<th colspan=\"3\">sirius · {}</th>",
            escape(&sirius.device)
        );
    }
    for duckdb in duckdbs {
        let _ = write!(html, "<th>{}</th>", escape(&duckdb.name));
    }
    html.push_str("</tr>\n<tr>");
    for _ in &shown {
        let _ = write!(
            html,
            "<th>load (ms)</th><th>execute (ms)</th><th>total (ms)</th><th title=\"{}\">peak \
             ≥ (GiB)</th>",
            escape(PEAK_MEANS)
        );
    }
    for _ in siriuses {
        html.push_str("<th>load (ms)</th><th>execute (ms)</th><th>total (ms)</th>");
    }
    for _ in duckdbs {
        html.push_str("<th>warm (ms)</th>");
    }
    html.push_str("</tr>\n");
    let mut main_queries = Vec::new();
    for query in queries_of(places) {
        let place = at_mode(query, main);
        let cells: Vec<Shown> = shown
            .iter()
            .map(|(at, device)| shown_of(&place, device, rows.get(&place).and_then(|f| f.get(at))))
            .collect();
        if cells.iter().all(Shown::is_empty) {
            continue;
        }
        main_queries.push(query.query.1.as_str());
        let _ = write!(html, "<tr><td>{}</td>", escape(&place.query.1));
        for cell in &cells {
            html.push_str(&cell.spanned());
        }
        for sirius in siriuses {
            let rel = format!(
                "benchmark-results/{}/{dataset}.sf{sf}/sirius.tsv",
                sirius.device
            );
            html.push_str(&sirius_cells(&place, sirius.files.get(&key), &rel));
        }
        for duckdb in duckdbs {
            html.push_str(&duckdb_cell(&place, duckdb, ""));
        }
        html.push_str("</tr>\n");
    }
    html.push_str("</table>\n");
    let mut head = format!(
        "<h2 id=\"{}\">{}</h2>\n",
        escape(&format!("{dataset}-sf{sf}")),
        escape(&format!("{dataset} sf{sf}"))
    );
    // With no DuckDB tree the line still stands, over no query.
    let duckdb_files: Vec<Option<&DuckdbResults>> = match duckdbs.is_empty() {
        true => vec![None],
        false => duckdbs
            .iter()
            .map(|duckdb| duckdb.files.get(&key))
            .collect(),
    };
    for sirius in siriuses {
        for duckdb in &duckdb_files {
            if let Some(line) = ratio_line(&main_queries, sirius.files.get(&key), *duckdb) {
                head.push_str(&line);
            }
        }
    }
    head + &html
}

/// The collapsed Modes section: per device with a section in the group, a table whose measures
/// each hold one sub-column per mode the device has, then DuckDB. A query whose every cell is
/// empty is left out, and so is the peak where no cell has one.
fn modes_section(
    dataset: &str,
    sf: &str,
    places: &[&Place],
    devices: &[Device],
    duckdbs: &[Duckdb],
    rows: &BTreeMap<Place, BTreeMap<usize, Found>>,
) -> String {
    let mut html = format!(
        "<details><summary>Modes, {}</summary>\n",
        escape(&format!("{dataset} sf{sf}"))
    );
    for (at, device) in devices.iter().enumerate() {
        let mut modes: Vec<usize> = places
            .iter()
            .filter(|place| rows[**place].contains_key(&at))
            .map(|place| place.mode)
            .collect();
        modes.sort();
        modes.dedup();
        if modes.is_empty() {
            continue;
        }
        let table: Vec<(&Place, Vec<Shown>)> = queries_of(places)
            .into_iter()
            .map(|query| {
                let cells = modes
                    .iter()
                    .map(|mode| {
                        let place = at_mode(query, *mode);
                        shown_of(&place, device, rows.get(&place).and_then(|f| f.get(&at)))
                    })
                    .collect();
                (query, cells)
            })
            .filter(|(_, cells): &(_, Vec<Shown>)| !cells.iter().all(Shown::is_empty))
            .collect();
        let peak = table
            .iter()
            .flat_map(|(_, cells)| cells)
            .any(|cell| matches!(cell, Shown::Timed { peak: Some(_), .. }));
        let measures: &[Measure] = match peak {
            true => &[
                Measure::Load,
                Measure::Execute,
                Measure::Total,
                Measure::Peak,
            ],
            false => &[Measure::Load, Measure::Execute, Measure::Total],
        };
        // Where every load is unpinned the heading says so once, which keeps the cells narrow.
        let timed: Vec<bool> = table
            .iter()
            .flat_map(|(_, cells)| cells)
            .filter_map(|cell| match cell {
                Shown::Timed { unpinned, .. } => Some(*unpinned),
                Shown::Other { .. } => None,
            })
            .collect();
        let all_unpinned = !timed.is_empty() && timed.iter().all(|unpinned| *unpinned);
        let _ = writeln!(html, "<h3>{}</h3>", escape(&device.name));
        if all_unpinned {
            let _ = writeln!(
                html,
                "<p class=\"source\">Every load in this table is unpinned, read without the pinned \
                 host cache, so no cell is marked.</p>"
            );
        }
        html.push_str("<div class=\"wide\"><table>\n<tr><th rowspan=\"2\">query</th>");
        for measure in measures {
            let _ = write!(
                html,
                "<th colspan=\"{}\" class=\"g\"{}>{}</th>",
                modes.len(),
                measure.title(),
                measure.heading()
            );
        }
        for duckdb in duckdbs {
            let _ = write!(html, "<th class=\"g\">{}</th>", escape(&duckdb.name));
        }
        html.push_str("</tr>\n<tr>");
        for _ in measures {
            for (nth, mode) in modes.iter().enumerate() {
                let name = escape(&MODES[*mode].ident()).replacen('_', "<br>", 1);
                let _ = write!(html, "<th{}>{name}</th>", group_class(nth, ""));
            }
        }
        for _ in duckdbs {
            html.push_str("<th class=\"g\">warm (ms)</th>");
        }
        html.push_str("</tr>\n");
        for (query, cells) in &table {
            let _ = write!(html, "<tr><td>{}</td>", escape(&query.query.1));
            for measure in measures {
                for (nth, cell) in cells.iter().enumerate() {
                    html.push_str(&cell.measure(*measure, nth, !all_unpinned));
                }
            }
            for duckdb in duckdbs {
                html.push_str(&duckdb_cell(query, duckdb, "g "));
            }
            html.push_str("</tr>\n");
        }
        html.push_str("</table></div>\n");
    }
    html.push_str("</details>\n");
    html
}

/// ` class="…"`, with `g` (the group's left rule) on a measure's first sub-column.
fn group_class(nth: usize, class: &str) -> String {
    match (nth, class) {
        (0, "") => " class=\"g\"".to_string(),
        (0, class) => format!(" class=\"g {class}\""),
        (_, "") => String::new(),
        (_, class) => format!(" class=\"{class}\""),
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Measure {
    Load,
    Execute,
    Total,
    Peak,
}

impl Measure {
    fn heading(self) -> &'static str {
        match self {
            Measure::Load => "load (ms)",
            Measure::Execute => "execute (ms)",
            Measure::Total => "total (ms)",
            Measure::Peak => "peak ≥ (GiB)",
        }
    }

    fn title(self) -> String {
        match self {
            Measure::Peak => format!(" title=\"{}\"", escape(PEAK_MEANS)),
            _ => String::new(),
        }
    }
}

/// One device's cell for a row, as HTML: a timed section's four measures, or one text standing
/// for them all (failed, skipped, or `—`).
enum Shown {
    Timed {
        load: String,
        unpinned: bool,
        execute: String,
        total: String,
        peak: Option<String>,
    },
    Other {
        text: String,
        class: &'static str,
    },
}

impl Shown {
    /// No section and no list row: `—`.
    fn is_empty(&self) -> bool {
        matches!(self, Shown::Other { class: "na", .. })
    }

    /// The main table's four cells, or one across them.
    fn spanned(&self) -> String {
        match self {
            Shown::Timed {
                load,
                unpinned,
                execute,
                total,
                peak,
            } => format!(
                "<td class=\"n\">{load}{}</td><td class=\"n\">{execute}</td><td \
                 class=\"n\">{total}</td><td class=\"n\">{}</td>",
                unpinned_mark(*unpinned),
                peak.as_deref().unwrap_or("—")
            ),
            Shown::Other { text, class } => {
                format!("<td colspan=\"4\" class=\"{class}\">{text}</td>")
            }
        }
    }

    /// A Modes table's cell for `measure`, the `nth` mode under it. Text that is not a time
    /// stands in the total. `mark` says whether an unpinned load is marked in its cell.
    fn measure(&self, measure: Measure, nth: usize, mark: bool) -> String {
        let marked;
        let (text, class) = match self {
            Shown::Timed {
                load,
                unpinned,
                execute,
                total,
                peak,
            } => match measure {
                Measure::Load => {
                    marked = format!("{load}{}", unpinned_mark(mark && *unpinned));
                    (marked.as_str(), "n")
                }
                Measure::Execute => (execute.as_str(), "n"),
                Measure::Total => (total.as_str(), "n"),
                Measure::Peak => (peak.as_deref().unwrap_or("—"), "n"),
            },
            Shown::Other { text, class } if measure == Measure::Total => (text.as_str(), *class),
            Shown::Other { .. } => ("—", "na"),
        };
        format!("<td{}>{text}</td>", group_class(nth, class))
    }
}

/// The `unpinned` marker after a load, or nothing.
fn unpinned_mark(unpinned: bool) -> String {
    match unpinned {
        true => format!(
            " <span class=\"source\" title=\"{}\">unpinned</span>",
            escape(UNPINNED_MEANS)
        ),
        false => String::new(),
    }
}

/// The mark after a `wrong` row's time: `wrong answer`, then the row's note where it has one.
fn wrong_mark(note: &str) -> String {
    let text = match note {
        "" => "wrong answer".to_string(),
        note => format!("wrong answer {note}"),
    };
    format!(" <span class=\"missing\">{}</span>", escape(&text))
}

/// One device's cell for a row at `place`'s mode.
fn shown_of(place: &Place, device: &Device, found: Option<&Found>) -> Shown {
    match decided(place, device, found) {
        Decided::Timed {
            found,
            load_us,
            execute_us,
            total_us,
            peak_bytes,
            pinned,
            wrong,
        } => {
            let wrong = wrong.map_or(String::new(), |note| wrong_mark(&note));
            Shown::Timed {
                load: ms(load_us),
                unpinned: !pinned,
                execute: ms(execute_us),
                total: format!("{}{wrong}", linked(&ms(total_us), found)),
                peak: peak_bytes.map(|bytes| escape(&gib(bytes))),
            }
        }
        Decided::Untimed {
            outcome,
            text,
            found,
        } => Shown::Other {
            text: match found {
                Some(found) => linked(&text, found),
                None => escape(&text),
            },
            class: outcome.class(),
        },
        Decided::Missing => Shown::Other {
            text: "—".to_string(),
            class: "na",
        },
    }
}

/// A DuckDB tree's cell for a row: its time, linked to its line, or `failed`, or `—`. A time
/// whose warm verdict is not `yes` says what it was: it is not the warm protocol's. `lead` comes
/// first in the cell's class.
fn duckdb_cell(place: &Place, duckdb: &Duckdb, lead: &str) -> String {
    let (dataset, sf) = (&place.dataset.1, &place.sf.1);
    let found = duckdb
        .files
        .get(&(dataset.clone(), sf.clone()))
        .and_then(|results| {
            results
                .rows
                .iter()
                .enumerate()
                .find(|(_, row)| row.query.replace('_', "-") == place.query.1)
        });
    let Some((index, row)) = found else {
        return format!("<td class=\"{lead}na\">—</td>");
    };
    match (row.status, row.ms_2nd_min) {
        (DuckdbStatus::Ok, Some(ms)) => {
            let rel = format!(
                "benchmark-results/{}/{dataset}.sf{sf}/duckdb.tsv",
                duckdb.name
            );
            // Rows start on the file's third line, after the preamble and the column line.
            let href = section_href(&rel, index + 3);
            let cold = match row.warm.as_str() {
                "yes" => String::new(),
                warm => format!(" <span class=\"missing\">not warm: {}</span>", escape(warm)),
            };
            format!(
                "<td class=\"{lead}n\"><a href=\"{}\">{}</a>{cold}</td>",
                escape(&href),
                escape(&format!("{ms:.1}"))
            )
        }
        _ => format!("<td class=\"{lead}off\">failed</td>"),
    }
}

fn extra_table(table: &ReportTable) -> String {
    let mut html = format!(
        "<h2 id=\"{}\">{}</h2>\n<table>\n<tr>",
        escape(&table.id),
        escape(&table.heading)
    );
    for column in &table.columns {
        let _ = write!(html, "<th>{}</th>", escape(column));
    }
    html.push_str("</tr>\n");
    for row in &table.rows {
        html.push_str("<tr>");
        for cell in row {
            html.push_str(&report_cell(cell));
        }
        html.push_str("</tr>\n");
    }
    html.push_str("</table>\n");
    html
}

/// A cell's text may span lines: each `\n` is a `<br>`, so a long list stacks its items rather
/// than widening the page.
fn report_cell(cell: &ReportCell) -> String {
    let class = cell.class.map_or(String::new(), |class| {
        format!(" class=\"{}\"", escape(class))
    });
    let shown = escape(&cell.text).replace('\n', "<br>");
    let text = match &cell.href {
        Some(href) => format!("<a href=\"{}\">{shown}</a>", escape(href)),
        None => shown,
    };
    format!("<td{class}>{text}</td>")
}

/// `text`, escaped, linked to the section it came from.
fn linked(text: &str, found: &Found) -> String {
    format!(
        "<a href=\"{}\">{}</a>",
        escape(&section_href(&found.rel, found.line)),
        escape(text)
    )
}

fn first_line(text: &str) -> &str {
    text.lines().next().unwrap_or("")
}

/// Microseconds as milliseconds with one decimal.
pub(crate) fn ms(us: u64) -> String {
    format!("{:.1}", us as f64 / 1000.0)
}

/// Bytes as GiB with two decimals: the peak column's unit.
pub(crate) fn gib(bytes: u64) -> String {
    format!("{:.2}", bytes as f64 / 1073741824.0)
}

/// As Python's `html.escape`, which `ticket_widget.py` writes `report.html` with.
fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#x27;"),
            c => out.push(c),
        }
    }
    out
}
