//! The benchmark report page, `testdata/benchmark-report.html`: every device tree under
//! `benchmark-results/` side by side, through the tree's one reader; build-test.md, **The page**.
//!
//! - A device is a directory directly under the root holding a `<ds>.sf<sf>/<mode>.benchmark.txt`
//!   for one of `MODES`. One row per (dataset, sf, query, mode) some device has a section for.
//! - A timed cell is load, execute, total and peak, a pre-bench-rows section's load derived and
//!   its peak `—`. Any other cell spans the four: the list's `skipped` note where it has one,
//!   else the failure's or the marker's reason, else `—`.
//! - No time, no host, no absolute path: the same trees give the same bytes.

#[cfg(test)]
mod tests;

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use super::{
    CaseStatus, DeviceCase, FAILED, MODES, REPORT_REPO, ReportCell, ReportTable, SKIPPED,
    ordered_sections, parse_benchmark_section, parse_device_cases, testdata_root,
};

/// What the peak column claims, and no more.
const PEAK_MEANS: &str = "the accountant's peak, a lower bound on device memory";

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
"#;

pub(crate) fn path() -> PathBuf {
    testdata_root().join("benchmark-report.html")
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

pub(crate) fn render(root: &Path, order: &[(String, String)], extra: &[ReportTable]) -> String {
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
                rows.entry(place)
                    .or_default()
                    .insert(at, Found { rel, line, body });
            }
        }
    }
    page(&devices, &rows, extra)
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
         <p><b>PeacockDB benchmark results</b> — every device tree side by side, one row per \
         query and mode. Times are device-side, in ms: the total is load plus execute. The peak \
         is {PEAK_MEANS}, in GiB.</p>\n<ul>\n"
    );
    for ((dataset, sf), places) in &groups {
        let id = format!("{}-sf{}", dataset.1, sf.1);
        let label = format!("{} sf{}", dataset.1, sf.1);
        let _ = writeln!(
            html,
            "<li><a href=\"#{}\">{}</a> — {} rows</li>",
            escape(&id),
            escape(&label),
            places.len()
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
        html.push_str(&dataset_table(&dataset.1, &sf.1, places, devices, rows));
    }
    for table in extra {
        html.push_str(&extra_table(table));
    }
    html.push_str("</body></html>\n");
    html
}

fn dataset_table(
    dataset: &str,
    sf: &str,
    places: &[&Place],
    devices: &[Device],
    rows: &BTreeMap<Place, BTreeMap<usize, Found>>,
) -> String {
    let id = escape(&format!("{dataset}-sf{sf}"));
    let mut html = format!(
        "<h2 id=\"{id}\">{}</h2>\n<table>\n<tr><th rowspan=\"2\">query</th><th \
         rowspan=\"2\">mode</th>",
        escape(&format!("{dataset} sf{sf}"))
    );
    for device in devices {
        let _ = write!(html, "<th colspan=\"4\">{}</th>", escape(&device.name));
    }
    html.push_str("</tr>\n<tr>");
    for _ in devices {
        let _ = write!(
            html,
            "<th>load (ms)</th><th>execute (ms)</th><th>total (ms)</th><th title=\"{}\">peak \
             ≥ (GiB)</th>",
            escape(PEAK_MEANS)
        );
    }
    html.push_str("</tr>\n");
    for place in places {
        let mode = MODES[place.mode].ident();
        let _ = write!(
            html,
            "<tr><td>{}</td><td>{}</td>",
            escape(&place.query.1),
            escape(&mode)
        );
        for (at, device) in devices.iter().enumerate() {
            html.push_str(&cells(place, &mode, device, rows[*place].get(&at)));
        }
        html.push_str("</tr>\n");
    }
    html.push_str("</table>\n");
    html
}

/// One device's four cells for a row, or one cell across them.
fn cells(place: &Place, mode: &str, device: &Device, found: Option<&Found>) -> String {
    let case = device.cases.iter().find(|case| {
        case.dataset == place.dataset.1
            && case.sf == place.sf.1
            && case.query.replace('_', "-") == place.query.1
    });
    let timed = found.and_then(|found| parse_benchmark_section(&found.body).map(|s| (found, s)));
    if let Some((found, section)) = timed {
        let total = section
            .count("device_us")
            .unwrap_or_else(|| panic!("{}:{}: no device_us", found.rel, found.line));
        let load = section
            .count("load_us")
            .unwrap_or_else(|| section.tree_load_us());
        let execute = section.count("execute_us").unwrap_or(total - load);
        let peak = section
            .count("peak_bytes")
            .map_or("—".to_string(), |bytes| {
                format!("{:.2}", bytes as f64 / 1073741824.0)
            });
        let wrong = case
            .filter(|case| case.status == CaseStatus::Wrong && case.modes.iter().any(|m| m == mode))
            .map_or(String::new(), |case| {
                format!(
                    " <span class=\"missing\">wrong answer {}</span>",
                    escape(&case.note)
                )
            });
        return format!(
            "<td class=\"n\">{}</td><td class=\"n\">{}</td><td class=\"n\">{}{wrong}</td><td \
             class=\"n\">{}</td>",
            ms(load),
            ms(execute),
            linked(&ms(total), found),
            escape(&peak)
        );
    }
    let skipped = case.filter(|case| case.status == CaseStatus::Skipped);
    let (text, class) = match (skipped, found) {
        (Some(case), _) => (format!("skipped: {}", case.note), "skip"),
        (None, Some(found)) => match found.body.strip_prefix(FAILED) {
            Some(rest) => (format!("failed: {}", first_line(rest)), "off"),
            None => (
                format!(
                    "skipped: {}",
                    first_line(found.body.trim_start_matches(SKIPPED))
                ),
                "skip",
            ),
        },
        (None, None) => ("—".to_string(), "na"),
    };
    let text = match found {
        Some(found) => linked(&text, found),
        None => escape(&text),
    };
    format!("<td colspan=\"4\" class=\"{class}\">{text}</td>")
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

fn report_cell(cell: &ReportCell) -> String {
    let class = cell.class.map_or(String::new(), |class| {
        format!(" class=\"{}\"", escape(class))
    });
    let text = match &cell.href {
        Some(href) => format!("<a href=\"{}\">{}</a>", escape(href), escape(&cell.text)),
        None => escape(&cell.text),
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
fn ms(us: u64) -> String {
    format!("{:.1}", us as f64 / 1000.0)
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
