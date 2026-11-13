//! `testdata/benchmarks.csv`: every result the page holds but the fused batches, one row each,
//! from the same trees, a device's statuses decided where the page's are. build-test.md, **The
//! page**.

use std::collections::BTreeMap;

use super::super::{DuckdbStatus, MODES, sirius_results};
use super::decided::{Decided, decided};
use super::{MAIN_MODE, Place, Trees, dataset_rank, gib, mode_rank, ms, query_rank};

const HEADER: &str = "dataset,sf,query,engine,device,mode,load_ms,exec_ms,total_ms,peak_gib,status";

/// The engines, in the order a query's rows list them.
const ENGINES: [&str; 3] = ["peacockdb", "sirius", "duckdb"];

/// Where a row sorts: the page's place at its first mode, then the engine's index in `ENGINES`,
/// the device, and the mode's index in `MODES` (0 for DuckDB, which has none).
type Key = (Place, usize, String, usize);

fn key(place: &Place, engine: usize, device: &str) -> Key {
    let query = Place {
        mode: 0,
        ..place.clone()
    };
    (query, engine, device.to_string(), place.mode)
}

pub(crate) fn render(trees: &Trees, order: &[(String, String)]) -> String {
    let mut lines: BTreeMap<Key, String> = BTreeMap::new();
    for (place, found) in &trees.rows {
        for (at, found) in found {
            let device = &trees.devices[*at];
            let (times, status) = match decided(place, device, Some(found)) {
                Decided::Timed {
                    load_us,
                    execute_us,
                    total_us,
                    peak_bytes,
                    wrong,
                    ..
                } => {
                    let peak = peak_bytes.map_or(String::new(), gib);
                    let times = [ms(load_us), ms(execute_us), ms(total_us), peak];
                    (times, if wrong.is_some() { "wrong" } else { "ok" })
                }
                Decided::Untimed { outcome, .. } => (Default::default(), outcome.status()),
                Decided::Missing => unreachable!("a row with a section is never missing"),
            };
            let mode = MODES[place.mode].ident();
            let at = key(place, 0, &device.name);
            let row = [&device.name, mode.as_str()];
            lines.insert(at, line(place, ENGINES[0], row, times, status));
        }
    }
    for sirius in &trees.siriuses {
        for ((dataset, sf), results) in &sirius.files {
            for row in &results.rows {
                let mut place = place_of(order, dataset, sf, &row.query);
                place.mode = mode_rank(MAIN_MODE);
                let total = row
                    .ms_2nd_min
                    .map_or(String::new(), |ms| format!("{ms:.1}"));
                let times = [String::new(), String::new(), total, String::new()];
                let status = sirius_results::status_name(row.status);
                let at = key(&place, 1, &sirius.device);
                let device = [&sirius.device, MAIN_MODE];
                lines.insert(at, line(&place, ENGINES[1], device, times, status));
            }
        }
    }
    for duckdb in &trees.duckdbs {
        for ((dataset, sf), results) in &duckdb.files {
            for row in &results.rows {
                let place = place_of(order, dataset, sf, &row.query.replace('_', "-"));
                let total = row
                    .ms_2nd_min
                    .map_or(String::new(), |ms| format!("{ms:.1}"));
                let times = [String::new(), String::new(), total, String::new()];
                let status = match row.status {
                    DuckdbStatus::Ok => "ok",
                    DuckdbStatus::Failed => "failed",
                };
                let at = key(&place, 2, &duckdb.name);
                let tree = [duckdb.name.as_str(), ""];
                lines.insert(at, line(&place, ENGINES[2], tree, times, status));
            }
        }
    }
    let mut text = format!("{HEADER}\n");
    for line in lines.values() {
        text.push_str(line);
    }
    text
}

/// A Sirius or DuckDB row's place on the page, at the first mode.
fn place_of(order: &[(String, String)], dataset: &str, sf: &str, query: &str) -> Place {
    Place {
        dataset: (dataset_rank(order, dataset), dataset.to_string()),
        sf: (
            sf.parse()
                .unwrap_or_else(|_| panic!("{dataset}.sf{sf}: an sf of digits")),
            sf.to_string(),
        ),
        query: (query_rank(order, dataset, query), query.to_string()),
        mode: 0,
    }
}

/// One CSV line; `device` is the device and the mode, `times` load, execute, total and peak.
fn line(
    place: &Place,
    engine: &str,
    device: [&str; 2],
    times: [String; 4],
    status: &str,
) -> String {
    let [load, execute, total, peak] = times;
    format!(
        "{},{},{},{engine},{},{},{load},{execute},{total},{peak},{status}\n",
        place.dataset.1, place.sf.1, place.query.1, device[0], device[1]
    )
}
