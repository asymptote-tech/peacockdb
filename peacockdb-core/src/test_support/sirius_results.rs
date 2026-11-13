//! Sirius's results, `testdata/benchmark-results/h100-80gb-hbm3/<ds>.sf<sf>/sirius.tsv`, as the
//! Sirius runner writes them. This is the format's one reader and its one Rust writer, the
//! inverse of each other; the runner's tests hold its output to `testdata/fixtures/
//! sirius-results.tsv`, as this module's do. The status a timed row's answer sets is decided
//! here too, from its verdict against DuckDB's.

#[cfg(test)]
mod tests;

use std::collections::BTreeSet;
use std::path::PathBuf;

use super::{AnswerVerdict, SiriusResults, SiriusRow, SiriusStatus, testdata_root};

/// The `# ` line's keys, in the order the runner writes them.
const KEYS: [&str; 7] = [
    "sirius", "duckdb", "libcudf", "cuda", "driver", "device", "host",
];

/// The column line, in the order the runner writes the columns.
const COLUMNS: &str = "query\tms_2nd_min\truns_ms\tstatus\tnote";

/// What a row writes for a value it has none of.
const NONE: &str = "-";

pub(crate) fn parse(text: &str) -> SiriusResults {
    let mut lines = text.lines();
    let first = lines.next().unwrap_or("");
    let preamble = first
        .strip_prefix("# ")
        .map(|tokens| tokens.split(' ').map(token).collect::<Option<Vec<_>>>())
        .unwrap_or(None)
        .filter(|pairs| pairs.iter().map(|(key, _)| key.as_str()).eq(KEYS))
        .unwrap_or_else(|| {
            refused(
                1,
                first,
                &format!("not a `# ` line of key=value for {KEYS:?}"),
            )
        });
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
    SiriusResults { preamble, rows }
}

pub(crate) fn write(results: &SiriusResults) -> String {
    let preamble: Vec<String> = results
        .preamble
        .iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect();
    let mut text = format!("# {}\n{COLUMNS}\n", preamble.join(" "));
    for row in &results.rows {
        let ms = row
            .ms_2nd_min
            .map_or_else(|| NONE.to_string(), |ms| format!("{ms:.1}"));
        let runs = if row.runs_ms.is_empty() {
            NONE.to_string()
        } else {
            let runs: Vec<String> = row.runs_ms.iter().map(|ms| format!("{ms:.1}")).collect();
            runs.join(",")
        };
        let note = if row.note.is_empty() { NONE } else { &row.note };
        let status = status_name(row.status);
        text.push_str(&format!("{}\t{ms}\t{runs}\t{status}\t{note}\n", row.query));
    }
    text
}

pub(crate) fn path(device: &str, dataset: &str, sf: &str) -> PathBuf {
    testdata_root()
        .join("benchmark-results")
        .join(device)
        .join(format!("{dataset}.sf{sf}"))
        .join("sirius.tsv")
}

pub(crate) fn status_for(
    recorded: SiriusStatus,
    verdict: Option<AnswerVerdict>,
) -> Result<SiriusStatus, String> {
    match (recorded, verdict) {
        (SiriusStatus::Failed | SiriusStatus::Skipped, _) => Ok(recorded),
        (_, Some(AnswerVerdict::Meets)) => Ok(SiriusStatus::Ok),
        (_, Some(AnswerVerdict::Differs(_))) => Ok(SiriusStatus::Wrong),
        (_, Some(AnswerVerdict::NoReference)) => Ok(SiriusStatus::Unverified),
        (_, Some(AnswerVerdict::Unanswered(said))) => Err(format!("no status can be set: {said}")),
        (_, None) => unreachable!("a {recorded:?} row has a time, so its answer is judged"),
    }
}

fn token(word: &str) -> Option<(String, String)> {
    let (key, value) = word.split_once('=')?;
    (!key.is_empty()).then(|| (key.to_string(), value.to_string()))
}

fn refused(number: usize, line: &str, why: &str) -> ! {
    panic!("sirius results line {number} {line:?}: {why}")
}

/// A status as the file spells it.
pub(crate) fn status_name(status: SiriusStatus) -> &'static str {
    match status {
        SiriusStatus::Ok => "ok",
        SiriusStatus::Wrong => "wrong",
        SiriusStatus::Unverified => "unverified",
        SiriusStatus::Failed => "failed",
        SiriusStatus::Skipped => "skipped",
    }
}

/// One row, or why the format refuses it.
fn row(line: &str) -> Result<SiriusRow, String> {
    let fields: Vec<&str> = line.split('\t').collect();
    let [query, ms, runs, status, note] = fields[..] else {
        return Err(format!("not five fields: {COLUMNS:?}"));
    };
    let status = match status {
        "ok" => SiriusStatus::Ok,
        "wrong" => SiriusStatus::Wrong,
        "unverified" => SiriusStatus::Unverified,
        "failed" => SiriusStatus::Failed,
        "skipped" => SiriusStatus::Skipped,
        other => {
            return Err(format!(
                "unknown status {other:?} (expected ok, wrong, unverified, failed or skipped)"
            ));
        }
    };
    let timed = matches!(
        status,
        SiriusStatus::Ok | SiriusStatus::Wrong | SiriusStatus::Unverified
    );
    let (ms_2nd_min, runs_ms) = match (timed, ms, runs) {
        (true, NONE, _) | (true, _, NONE) => {
            return Err(format!(
                "an ok, wrong or unverified row has a time: ms_2nd_min and runs_ms are not {NONE:?}"
            ));
        }
        (true, ms, runs) => (
            Some(number(ms, "ms_2nd_min")?),
            runs.split(',')
                .map(|run| number(run, "runs_ms"))
                .collect::<Result<Vec<_>, _>>()?,
        ),
        (false, NONE, NONE) => (None, Vec::new()),
        (false, _, _) => {
            return Err(format!(
                "a failed or skipped row has no time: ms_2nd_min and runs_ms are {NONE:?}"
            ));
        }
    };
    Ok(SiriusRow {
        query: query.to_string(),
        ms_2nd_min,
        runs_ms,
        status,
        note: if note == NONE {
            String::new()
        } else {
            note.to_string()
        },
    })
}

fn number(text: &str, column: &str) -> Result<f64, String> {
    text.parse::<f64>()
        .ok()
        .filter(|value| value.is_finite() && *value >= 0.0)
        .ok_or_else(|| format!("{column} {text:?} is not a number of milliseconds"))
}
