//! A device's case list, `testdata/benchmark-results/<device>/cases.csv`: which cases its tree
//! holds, and what became of each. build-test.md's "Corpus Benchmarks" carries the format.

#[cfg(test)]
mod tests;

use std::collections::BTreeMap;
use std::path::PathBuf;

use super::{CaseStatus, DEVICE_CASES_HEADER, DeviceCase, MODES, Measured, Mode, testdata_root};

/// The one spelling of each status, read and written.
const STATUSES: [(CaseStatus, &str); 3] = [
    (CaseStatus::Ok, "ok"),
    (CaseStatus::Skipped, "skipped"),
    (CaseStatus::Wrong, "wrong"),
];

pub(crate) fn path(device: &str) -> PathBuf {
    testdata_root()
        .join("benchmark-results")
        .join(device)
        .join("cases.csv")
}

pub(crate) fn parse(text: &str) -> Vec<DeviceCase> {
    let mut lines = text.lines();
    let header = lines.next().unwrap_or("");
    if header != DEVICE_CASES_HEADER {
        refused(
            1,
            header,
            &format!("the first line is not {DEVICE_CASES_HEADER:?}"),
        );
    }
    let mut cases = Vec::new();
    let mut first_at: BTreeMap<(String, String, String), usize> = BTreeMap::new();
    for (index, line) in lines.enumerate() {
        let number = index + 2;
        let case = row(line).unwrap_or_else(|why| refused(number, line, &why));
        let key = (case.dataset.clone(), case.sf.clone(), case.query.clone());
        if let Some(first) = first_at.insert(key, number) {
            refused(
                number,
                line,
                &format!("the case is listed twice, first at line {first}"),
            );
        }
        cases.push(case);
    }
    cases
}

fn refused(number: usize, line: &str, why: &str) -> ! {
    panic!("device case list line {number} {line:?}: {why}")
}

/// One row, or why the format refuses it.
fn row(line: &str) -> Result<DeviceCase, String> {
    let fields: Vec<&str> = line.splitn(6, ',').collect();
    let [dataset, sf, query, modes, status, note] = fields[..] else {
        return Err(format!("not six fields: {DEVICE_CASES_HEADER}"));
    };
    let known: Vec<String> = MODES.iter().map(Mode::ident).collect();
    let modes: Vec<String> = match modes {
        "" => Vec::new(),
        joined => joined.split('|').map(str::to_string).collect(),
    };
    if let Some(unknown) = modes.iter().find(|mode| !known.contains(mode)) {
        return Err(format!(
            "unknown mode {unknown:?} (expected one of {known:?})"
        ));
    }
    let Some(&(status, _)) = STATUSES.iter().find(|(_, spelled)| *spelled == status) else {
        return Err(format!(
            "unknown status {status:?} (expected ok, skipped or wrong)"
        ));
    };
    match status {
        CaseStatus::Skipped if !modes.is_empty() => {
            return Err("a skipped row times no mode; its modes are empty".to_string());
        }
        CaseStatus::Skipped if note.is_empty() => {
            return Err("a skipped row names its reason in the note".to_string());
        }
        CaseStatus::Ok | CaseStatus::Wrong if modes.is_empty() => {
            return Err("an ok or wrong row names the modes it ran".to_string());
        }
        CaseStatus::Ok if !note.is_empty() => {
            return Err("an ok row has no note".to_string());
        }
        CaseStatus::Wrong if !is_ticket(note) => {
            return Err("a wrong row's note is its ticket, #NN".to_string());
        }
        _ => {}
    }
    Ok(DeviceCase {
        dataset: dataset.to_string(),
        sf: sf.to_string(),
        query: query.to_string(),
        modes,
        status,
        note: note.to_string(),
    })
}

/// `#` and one or more digits.
fn is_ticket(note: &str) -> bool {
    note.strip_prefix('#')
        .is_some_and(|number| !number.is_empty() && number.bytes().all(|b| b.is_ascii_digit()))
}

pub(crate) fn render(cases: &[DeviceCase]) -> String {
    let mut text = format!("{DEVICE_CASES_HEADER}\n");
    for case in cases {
        let (_, status) = STATUSES
            .iter()
            .find(|(status, _)| *status == case.status)
            .expect("every status is spelled");
        text.push_str(&format!(
            "{},{},{},{},{status},{}\n",
            case.dataset,
            case.sf,
            case.query,
            case.modes.join("|"),
            case.note
        ));
    }
    text
}

/// Every candidate's mode: its file is considered even when absent, and a query that failed in
/// it owes the later modes no section, since tp4-sized does not run it.
const TP1_SINGLE: &str = "tp1_single";

/// The sf a device list's rows are written at.
const SF: &str = "40";

pub(crate) fn lines(
    candidates: &[(String, String, Vec<String>)],
    modes: &[&str],
    files: &dyn Fn(&str, &str) -> Option<BTreeMap<String, Measured>>,
    wrong: &BTreeMap<(String, String), String>,
) -> Result<Vec<DeviceCase>, String> {
    let carried_modes = |carried: &[String]| -> Vec<&str> {
        modes
            .iter()
            .copied()
            .filter(|mode| carried.iter().any(|m| m == mode))
            .collect()
    };
    // Each file is read once, before any of its sections is looked at.
    let mut sections: BTreeMap<(&str, &str), Option<BTreeMap<String, Measured>>> = BTreeMap::new();
    for (dataset, _, carried) in candidates {
        for mode in carried_modes(carried) {
            sections
                .entry((dataset, mode))
                .or_insert_with(|| files(dataset, mode));
        }
    }
    let mut rows = Vec::new();
    for (dataset, query, carried) in candidates {
        let mut measured: Vec<(&str, Option<&Measured>)> = Vec::new();
        for mode in carried_modes(carried) {
            let file = &sections[&(dataset.as_str(), mode)];
            if file.is_none() && mode != TP1_SINGLE {
                continue;
            }
            let section = file
                .as_ref()
                .and_then(|file| file.get(&query.replace('_', "-")));
            measured.push((mode, section));
        }
        let tp1_failed = measured.iter().any(|(mode, section)| {
            *mode == TP1_SINGLE && matches!(section, Some(Measured::Failed(_)))
        });
        let mut timed = Vec::new();
        let mut reasons = Vec::new();
        for (mode, section) in measured {
            match section {
                Some(Measured::Timed) => timed.push(mode.to_string()),
                Some(Measured::Failed(reason)) => reasons.push(reason.clone()),
                None if tp1_failed => {}
                None => {
                    return Err(format!(
                        "{dataset} {query}: the tree holds no section at {mode}, a mode the main \
                         list carries for it"
                    ));
                }
            }
        }
        let mark = wrong.get(&(dataset.clone(), query.clone()));
        let (status, note) = match (timed.is_empty(), mark) {
            (false, None) => (CaseStatus::Ok, String::new()),
            (false, Some(ticket)) => (CaseStatus::Wrong, ticket.clone()),
            (true, None) => {
                let Some(reason) = reasons.into_iter().next() else {
                    return Err(format!(
                        "{dataset} {query}: timed at none of {modes:?}, so no row can be written"
                    ));
                };
                (CaseStatus::Skipped, reason)
            }
            (true, Some(ticket)) => {
                return Err(format!(
                    "{dataset} {query}: marked wrong {ticket} and measured in no mode; a skipped \
                     query has no answer to be wrong"
                ));
            }
        };
        rows.push(DeviceCase {
            dataset: dataset.clone(),
            sf: SF.to_string(),
            query: query.clone(),
            modes: timed,
            status,
            note,
        });
    }
    let named: Vec<(&String, &String)> = candidates.iter().map(|(d, q, _)| (d, q)).collect();
    if let Some(((dataset, query), ticket)) = wrong
        .iter()
        .find(|((dataset, query), _)| !named.contains(&(dataset, query)))
    {
        return Err(format!(
            "{dataset} {query}: marked wrong {ticket} and no candidate of the main list"
        ));
    }
    Ok(rows)
}
