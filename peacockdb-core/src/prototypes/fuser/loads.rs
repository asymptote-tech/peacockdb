//! `loads.tsv` (pinned-loads' format) as the hold rule reads it: what a re-read of one load
//! costs, by the query and the seq of its `CudfScan` recipe. No fallback: a missing row panics.

#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::path::Path;

use super::LoadTimes;

pub(crate) fn read_load_times(path: &Path, dataset: &str, sf: &str, mode: &str) -> LoadTimes {
    let text = std::fs::read_to_string(path)
        .unwrap_or_else(|why| panic!("{}: the hold rule reads it: {why}", path.display()));
    let mut lines = text.lines().filter(|line| !line.starts_with('#'));
    let header: Vec<&str> = lines.next().unwrap_or("").split('\t').collect();
    let column = |name: &str| {
        header
            .iter()
            .position(|column| *column == name)
            .unwrap_or_else(|| panic!("{}: no `{name}` column", path.display()))
    };
    let at = ["dataset", "sf", "query", "mode", "recipe_seq", "load_us"].map(column);
    let mut by_key = HashMap::new();
    for line in lines {
        let fields: Vec<&str> = line.split('\t').collect();
        let field = |which: usize| fields[at[which]];
        if (field(0), field(1), field(3)) != (dataset, sf, mode) {
            continue;
        }
        let number = |which: usize| -> u64 {
            field(which)
                .parse()
                .unwrap_or_else(|why| panic!("{}: `{line}`: {why}", path.display()))
        };
        let key = (field(2).to_string(), number(4));
        if by_key.insert(key, number(5)).is_some() {
            panic!("{}: two rows for `{line}`'s load", path.display());
        }
    }
    LoadTimes { by_key }
}

pub(crate) fn load_us(times: &LoadTimes, query: &str, recipe_seq: u64) -> u64 {
    *times
        .by_key
        .get(&(query.to_string(), recipe_seq))
        .unwrap_or_else(|| {
            panic!("loads.tsv has no row for {query} at recipe seq {recipe_seq}: no fallback")
        })
}
