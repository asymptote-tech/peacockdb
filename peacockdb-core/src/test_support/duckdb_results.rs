//! DuckDB's results, `testdata/benchmark-results/<tree>/<ds>.sf<sf>/duckdb.tsv`, as
//! `benchmarks/duckdb_minimal.sh --results` writes them; build-test.md, "Corpus benchmarks".
//! This is the format's one reader: the page's DuckDB column goes through it, and scripts that
//! read the file are tested against `testdata/fixtures/duckdb-results.tsv`, as this is.

#[cfg(test)]
mod tests;

use std::collections::BTreeSet;

use super::{DuckdbResults, DuckdbRow, DuckdbStatus};

/// The column line, in the order the script writes the columns.
const COLUMNS: &str = "query\tms_2nd_min\truns_ms\tworkset_bytes\tdisk_read_bytes\twarm\tstatus";

/// What a failed row writes for each value it has none of.
const NONE: &str = "-";

pub(crate) fn parse(text: &str) -> DuckdbResults {
    let mut lines = text.lines();
    let first = lines.next().unwrap_or("");
    let preamble = first
        .strip_prefix("# ")
        .map(|tokens| tokens.split(' ').map(token).collect::<Option<Vec<_>>>())
        .unwrap_or(None)
        .unwrap_or_else(|| refused(1, first, "not a `# ` line of key=value words"));
    let header = lines.next().unwrap_or("");
    if header != COLUMNS {
        refused(2, header, &format!("the column line is not {COLUMNS:?}"));
    }
    let mut seen = BTreeSet::new();
    let rows = lines
        .enumerate()
        .map(|(index, line)| {
            let number = index + 3;
            let row = row(line).unwrap_or_else(|why| refused(number, line, &why));
            if !seen.insert(row.query.clone()) {
                refused(number, line, "the query is listed twice");
            }
            row
        })
        .collect();
    DuckdbResults { preamble, rows }
}

fn token(word: &str) -> Option<(String, String)> {
    let (key, value) = word.split_once('=')?;
    (!key.is_empty()).then(|| (key.to_string(), value.to_string()))
}

fn refused(number: usize, line: &str, why: &str) -> ! {
    panic!("duckdb results line {number} {line:?}: {why}")
}

/// One row, or why the format refuses it.
fn row(line: &str) -> Result<DuckdbRow, String> {
    let fields: Vec<&str> = line.split('\t').collect();
    let [query, ms, runs, workset, read, warm, status] = fields[..] else {
        return Err(format!("not seven fields: {COLUMNS:?}"));
    };
    let workset_bytes = optional(workset, "workset_bytes")?;
    match status {
        "ok" => {
            let ms_2nd_min = Some(number(ms, "ms_2nd_min")?);
            let runs_ms = runs
                .split(',')
                .map(|run| number(run, "runs_ms"))
                .collect::<Result<Vec<_>, _>>()?;
            let disk_read_bytes = Some(count(read, "disk_read_bytes")?);
            if !["yes", "partial", "NO"].contains(&warm) {
                return Err(format!("warm {warm:?} is not yes, partial or NO"));
            }
            Ok(DuckdbRow {
                query: query.to_string(),
                ms_2nd_min,
                runs_ms,
                workset_bytes,
                disk_read_bytes,
                warm: warm.to_string(),
                status: DuckdbStatus::Ok,
            })
        }
        "failed" if [ms, runs, read, warm].iter().all(|value| *value == NONE) => Ok(DuckdbRow {
            query: query.to_string(),
            ms_2nd_min: None,
            runs_ms: Vec::new(),
            workset_bytes,
            disk_read_bytes: None,
            warm: warm.to_string(),
            status: DuckdbStatus::Failed,
        }),
        "failed" => Err(format!(
            "a failed row has no time: ms_2nd_min, runs_ms, disk_read_bytes and warm are {NONE:?}"
        )),
        other => Err(format!("unknown status {other:?} (expected ok or failed)")),
    }
}

fn number(text: &str, column: &str) -> Result<f64, String> {
    text.parse::<f64>()
        .ok()
        .filter(|value| value.is_finite() && *value >= 0.0)
        .ok_or_else(|| format!("{column} {text:?} is not a number of milliseconds"))
}

fn count(text: &str, column: &str) -> Result<u64, String> {
    text.parse()
        .map_err(|_| format!("{column} {text:?} is not a count of bytes"))
}

fn optional(text: &str, column: &str) -> Result<Option<u64>, String> {
    match text {
        NONE => Ok(None),
        text => count(text, column).map(Some),
    }
}
