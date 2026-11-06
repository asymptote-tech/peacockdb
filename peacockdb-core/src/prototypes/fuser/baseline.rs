//! The sf40 baseline as the fuser reads it: each query's `.benchmark.txt` section, its nodes by
//! pre-order index, with the per-node figures the hints and the memory rules are computed from.

#[cfg(test)]
mod tests;

use crate::plan::GpuNode;
use crate::test_support::BenchSection;

use super::{Baseline, Query, QueryTimes};

pub(crate) fn of(sections: &[BenchSection<'_>]) -> Baseline {
    Baseline {
        per_query: sections
            .iter()
            .enumerate()
            .map(|(query, section)| times_of(query, section))
            .collect(),
    }
}

fn times_of(query: usize, section: &BenchSection<'_>) -> QueryTimes {
    let summed = |at: usize, field: &str, lanes: &Option<Vec<Vec<u64>>>| -> u64 {
        let name = section.nodes[at].line.name;
        let lanes = lanes
            .as_ref()
            .unwrap_or_else(|| panic!("query {query}: node {at} ({name}) carries no {field}"));
        lanes.iter().flatten().sum()
    };
    let trailer = |key: &str| {
        section
            .count(key)
            .unwrap_or_else(|| panic!("query {query}: the trailer carries no {key}"))
    };
    let nodes = &section.nodes;
    QueryTimes {
        names: nodes
            .iter()
            .map(|node| node.line.name.to_string())
            .collect(),
        children: nodes.iter().map(|node| node.children.clone()).collect(),
        total_us: nodes.iter().map(|node| node.total_us).collect(),
        batch_rows: (0..nodes.len())
            .map(|at| summed(at, "batch_rows", &nodes[at].batch_rows))
            .collect(),
        batch_bytes: (0..nodes.len())
            .map(|at| summed(at, "batch_bytes", &nodes[at].batch_bytes))
            .collect(),
        load_us: trailer("load_us"),
        execute_us: trailer("execute_us"),
        peak_bytes: trailer("peak_bytes"),
    }
}

pub(crate) fn check(baseline: &Baseline, queries: &[Query<'_>]) -> Result<(), String> {
    if baseline.per_query.len() != queries.len() {
        return Err(format!(
            "the baseline holds {} queries and the batch {}",
            baseline.per_query.len(),
            queries.len()
        ));
    }
    for (query, times) in queries.iter().zip(&baseline.per_query) {
        let names = names_in_pre_order(query.tree);
        let at = (0..names.len().max(times.names.len()))
            .find(|at| names.get(*at) != times.names.get(*at));
        if let Some(at) = at {
            let said = |name: Option<&String>| name.map_or("nothing", String::as_str).to_string();
            return Err(format!(
                "{}: node {at} is {} in the plan and {} in the baseline",
                query.name,
                said(names.get(at)),
                said(times.names.get(at))
            ));
        }
    }
    Ok(())
}

fn names_in_pre_order(tree: &dyn GpuNode) -> Vec<String> {
    let mut names = vec![tree.name().to_string()];
    for child in tree.children() {
        names.extend(names_in_pre_order(child));
    }
    names
}
