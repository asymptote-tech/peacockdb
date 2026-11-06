//! Which queries a batch fuses: sf40-baseline's ordered list, the H100 tree's `cases.csv`,
//! read when the batch runs rather than compiled in, and the prefixes of it each batch takes.

#[cfg(test)]
mod tests;

use std::path::Path;

use crate::test_support::{CaseStatus, DeviceCase, device_cases_path, parse_device_cases};

use super::BATCH_SIZES;

/// The device whose case list orders the batches: the card the fused runs are measured on.
const DEVICE: &str = "h100-80gb-hbm3";

const MODE: &str = "tp1_single";

pub(crate) fn batch_set(dataset: &str, cases: &[DeviceCase]) -> Vec<String> {
    let set: Vec<String> = cases
        .iter()
        .filter(|case| case.dataset == dataset)
        .filter(|case| matches!(case.status, CaseStatus::Ok | CaseStatus::Wrong))
        .filter(|case| case.modes.iter().any(|mode| mode == MODE))
        .filter(|case| dataset != "tpch" || is_a_tpch_query(&case.query))
        .map(|case| case.query.clone())
        .collect();
    assert!(
        !set.is_empty(),
        "{dataset}: the case list holds no `ok` or `wrong` row at {MODE}, so it orders no batch"
    );
    set
}

/// `q1` to `q22`: the benchmark's own queries, not the micro queries beside them.
fn is_a_tpch_query(query: &str) -> bool {
    query
        .strip_prefix('q')
        .filter(|number| !number.starts_with('0'))
        .and_then(|number| number.parse::<u32>().ok())
        .is_some_and(|number| (1..=22).contains(&number))
}

pub(crate) fn ordered_queries(dataset: &str) -> Vec<String> {
    ordered_queries_in(&device_cases_path(DEVICE), dataset)
}

/// [`batch_set`] over the case list at `path`, which must exist: a batch never runs an empty
/// or a default list.
fn ordered_queries_in(path: &Path, dataset: &str) -> Vec<String> {
    let text = std::fs::read_to_string(path).unwrap_or_else(|why| {
        panic!(
            "{}: the fused batches are the case list's queries, and it does not read: {why}",
            path.display()
        )
    });
    batch_set(dataset, &parse_device_cases(&text))
}

pub(crate) fn batches(list: &[String]) -> Vec<(&'static str, Result<Vec<String>, String>)> {
    BATCH_SIZES
        .iter()
        .map(|(label, size)| {
            let batch = match size {
                Some(size) if *label != "first3" && list.len() <= *size => Err(format!(
                    "same set as all: the list holds {} queries",
                    list.len()
                )),
                Some(size) => Ok(list.iter().take(*size).cloned().collect()),
                None => Ok(list.to_vec()),
            };
            (*label, batch)
        })
        .collect()
}
