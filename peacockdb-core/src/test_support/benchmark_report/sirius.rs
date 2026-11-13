//! The main table's Sirius group and its ratio line: a main-table device's
//! `<ds>.sf<sf>/sirius.tsv`, its total only, and Σ Sirius / Σ DuckDB over the queries both answered
//! `ok`. build-test.md, **The page**.

use std::collections::BTreeMap;
use std::path::Path;

use super::super::{
    DuckdbResults, DuckdbStatus, SiriusResults, SiriusRow, SiriusStatus, sirius_results,
};
use super::{Device, MODES_ONLY, Place, escape, section_href, wrong_mark};

/// Why a timed Sirius row is `unverified`.
const UNVERIFIED_MEANS: &str = "no DuckDB answer to check it against";

/// One Sirius group: the device tree it was measured in and its files by (dataset, sf).
pub(crate) struct Sirius {
    pub(crate) device: String,
    pub(crate) files: BTreeMap<(String, String), SiriusResults>,
}

/// Every main-table device with a `<ds>.sf<sf>/sirius.tsv`, in the devices' order.
pub(crate) fn siriuses(root: &Path, devices: &[Device]) -> Vec<Sirius> {
    let mut found = Vec::new();
    for device in devices {
        if MODES_ONLY.contains(&device.name.as_str()) {
            continue;
        }
        let mut files = BTreeMap::new();
        for dir in std::fs::read_dir(root.join(&device.name))
            .into_iter()
            .flatten()
            .flatten()
        {
            let file = dir.path().join("sirius.tsv");
            let dirname = dir.file_name().to_string_lossy().into_owned();
            let Some((dataset, sf)) = dirname.split_once(".sf") else {
                continue;
            };
            if !file.is_file() {
                continue;
            }
            let text = std::fs::read_to_string(&file)
                .unwrap_or_else(|e| panic!("{}: {e}", file.display()));
            files.insert(
                (dataset.to_string(), sf.to_string()),
                sirius_results::parse(&text),
            );
        }
        if !files.is_empty() {
            let device = device.name.clone();
            found.push(Sirius { device, files });
        }
    }
    found
}

/// What one Sirius total is, as build-test.md's Sirius protocol states it, and what each file was
/// measured with, from its `# ` line; nothing without a file.
pub(crate) fn sirius_intro(siriuses: &[Sirius]) -> String {
    let named: Vec<String> = siriuses
        .iter()
        .flat_map(|sirius| {
            sirius.files.iter().map(move |((dataset, sf), results)| {
                // The file writes a space in a value as `_`.
                let of = |key: &str| {
                    results
                        .preamble
                        .iter()
                        .find(|(k, _)| k == key)
                        .map_or("?".to_string(), |(_, v)| v.replace('_', " "))
                };
                format!(
                    "{}, {dataset} sf{sf}: Sirius {}, DuckDB {}, libcudf {}, driver {}, device {}",
                    sirius.device,
                    of("sirius"),
                    of("duckdb"),
                    of("libcudf"),
                    of("driver"),
                    of("device")
                )
            })
        })
        .collect();
    if named.is_empty() {
        return String::new();
    }
    format!(
        "<p>Sirius's total is the wall time around the statement and an Arrow fetch of its result, \
         in ms: the second-smallest of the timed runs after one warm-up, from the parquet bytes \
         pinned in Sirius's host cache. On some rows Sirius's log counts reads outside the pin \
         (likely bloom filters) inside the timed runs. {}.</p>\n",
        escape(&named.join("; "))
    )
}

/// A Sirius group's three cells for a row: load and execute `—`, and the total linked to its
/// line, with what a `wrong` (and its note) or `unverified` row is, or the reason it has no time;
/// `—` without a row. `rel` is the file's path under `testdata/`.
pub(crate) fn sirius_cells(place: &Place, results: Option<&SiriusResults>, rel: &str) -> String {
    let na = "<td class=\"na\">—</td>";
    let Some((index, row)) = results.and_then(|results| sirius_row(results, &place.query.1)) else {
        return format!("{na}{na}{na}");
    };
    // Rows start on the file's third line, after the preamble and the column line.
    let href = escape(&section_href(rel, index + 3));
    let link = |text: &str| format!("<a href=\"{href}\">{}</a>", escape(text));
    let time = || {
        link(&format!(
            "{:.1}",
            row.ms_2nd_min.expect("a timed row has a time")
        ))
    };
    let (class, text) = match row.status {
        SiriusStatus::Ok => ("n", time()),
        SiriusStatus::Wrong => ("n", format!("{}{}", time(), wrong_mark(&row.note))),
        SiriusStatus::Unverified => (
            "n",
            format!(
                "{} <span class=\"source\" title=\"{UNVERIFIED_MEANS}\">unverified</span>",
                time()
            ),
        ),
        SiriusStatus::Failed => ("off", link(&format!("failed: {}", row.note))),
        SiriusStatus::Skipped => ("skip", link(&format!("skipped: {}", row.note))),
    };
    format!("{na}{na}<td class=\"{class}\">{text}</td>")
}

/// `query`'s row in a Sirius file, with its index among the rows. The first is the only one:
/// `parse_sirius_results` refuses a query listed twice.
fn sirius_row<'r>(results: &'r SiriusResults, query: &str) -> Option<(usize, &'r SiriusRow)> {
    results
        .rows
        .iter()
        .enumerate()
        .find(|(_, row)| row.query == query)
}

/// `Σ total Sirius / Σ total DuckDB` over the main table's queries both answered `ok`, `—` where
/// none did; `None` without a Sirius file for the dataset.
pub(crate) fn ratio_line(
    queries: &[&str],
    sirius: Option<&SiriusResults>,
    duckdb: Option<&DuckdbResults>,
) -> Option<String> {
    let sirius = sirius?;
    let duckdb_ok = |query: &str| {
        let rows = duckdb.map_or(&[][..], |duckdb| duckdb.rows.as_slice());
        // The first is the only one: `parse_duckdb_results` refuses a query listed twice.
        rows.iter()
            .find(|row| row.query.replace('_', "-") == query)
            .filter(|row| row.status == DuckdbStatus::Ok)
            .and_then(|row| row.ms_2nd_min)
    };
    let both: Vec<(f64, f64)> = queries
        .iter()
        .filter_map(|query| {
            let (_, row) = sirius_row(sirius, query)?;
            let ours = row.ms_2nd_min.filter(|_| row.status == SiriusStatus::Ok)?;
            Some((ours, duckdb_ok(query)?))
        })
        .collect();
    let ratio = match both.is_empty() {
        true => "—".to_string(),
        false => {
            let (ours, theirs) = both
                .iter()
                .fold((0.0, 0.0), |(a, b), (o, t)| (a + o, b + t));
            format!("{:.2}", ours / theirs)
        }
    };
    Some(format!(
        "<p>Σ total Sirius / Σ total DuckDB = {ratio} over {} of {} queries</p>\n",
        both.len(),
        queries.len()
    ))
}
