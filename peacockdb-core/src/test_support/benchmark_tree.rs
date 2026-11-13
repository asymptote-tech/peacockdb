//! The one reader of a `.benchmark.txt` section: the tree's checks, the report page, and the
//! later tasks that size or split a run all read it here. build-test.md, "Corpus benchmarks",
//! **The tree**, defines the format.
//!
//! Modelled on `parse_run_section`: a section is our own renderer's, so a line the format does
//! not allow panics naming it rather than being skipped.

#[cfg(test)]
mod tests;

use super::golden_text::{nested_numbers, parse_node_line};
use super::{BenchNode, BenchSection, FAILED, SKIPPED};

/// The line that ends the tree and opens the trailer.
const TRAILER: &str = "--- run ---";

pub(crate) fn parse_benchmark_section(body: &str) -> Option<BenchSection<'_>> {
    if body.starts_with(SKIPPED) || body.starts_with(FAILED) {
        return None;
    }
    let mut lines = body.lines();
    let mut nodes: Vec<BenchNode<'_>> = Vec::new();
    let mut timed: Vec<bool> = Vec::new();
    let mut open: Vec<usize> = Vec::new();
    for line in lines.by_ref() {
        if line == TRAILER {
            break;
        }
        if line.trim().is_empty() {
            continue;
        }
        if let Some(node) = parse_node_line(line) {
            open.truncate(node.depth);
            if let Some(parent) = open.last() {
                let at = nodes.len();
                nodes[*parent].children.push(at);
            }
            open.push(nodes.len());
            nodes.push(BenchNode {
                line: node,
                time_us: Vec::new(),
                total_us: 0,
                batch_rows: None,
                batch_bytes: None,
                children: Vec::new(),
            });
            timed.push(false);
            continue;
        }
        let at = nodes
            .len()
            .checked_sub(1)
            .unwrap_or_else(|| panic!("a benchmark section's line under no node: {line:?}"));
        if std::mem::replace(&mut timed[at], true) {
            panic!(
                "{} has its time line twice, the second {line:?}",
                nodes[at].line.name
            );
        }
        read_time_line(&mut nodes[at], line);
    }
    if let Some(at) = timed.iter().position(|timed| !timed) {
        panic!("{} has no time line beneath it", nodes[at].line.name);
    }
    let trailer = lines
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            line.split_once('=')
                .unwrap_or_else(|| panic!("trailer line {line:?} is not `key=value`"))
        })
        .collect();
    Some(BenchSection { nodes, trailer })
}

/// `time_us=… total_us=… [batch_rows=… batch_bytes=…]`, each token `key=value`.
fn read_time_line(node: &mut BenchNode<'_>, line: &str) {
    let (mut times, mut total) = (None, None);
    for token in line.split_whitespace() {
        let (key, value) = token
            .split_once('=')
            .unwrap_or_else(|| panic!("a time line's token {token:?} is not `key=value`"));
        match key {
            "time_us" => times = Some(nested_numbers(value)),
            "total_us" => total = Some(value.parse().expect("total_us is a number")),
            "batch_rows" => node.batch_rows = Some(nested_numbers(value)),
            "batch_bytes" => node.batch_bytes = Some(nested_numbers(value)),
            other => panic!("unknown key `{other}` beneath a node, in: {line}"),
        }
    }
    node.time_us = times.unwrap_or_else(|| panic!("no time_us on {line:?}"));
    node.total_us = total.unwrap_or_else(|| panic!("no total_us on {line:?}"));
}
