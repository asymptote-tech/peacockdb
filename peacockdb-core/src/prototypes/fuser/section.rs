//! The fused format: one section per batch in `tp1_single.fused.txt`, written here and read
//! back here, by the page's table and the corpus goldens' checks alike. The format is
//! `llm-wiki/designs/query-fuser.md`, "The fused format".

#[cfg(test)]
mod tests;

use std::fmt::Write as _;

use crate::plan::{GpuNode, NodeRef, PlanDag, as_node_ref};
use crate::prototypes::FusedSection;
use crate::test_support::{BenchNode, FAILED, NodeLine, SKIPPED, parse_node_line};

use super::{CapDecision, Choice, Fused, HoldRecord, MergeHint, Outcome, PipelineHint};

/// What the pipelining hint's numbers assume, written into every section.
const PIPELINE_MODEL: &str = "one loader and one executor, each sequential, side by side; a root \
     starts once its last source is loaded and the executor is free; cheapest root's loads first";

pub(crate) fn render_fused_section(
    queries: &[String],
    fused: &Fused,
    decision: &CapDecision,
    pool_bytes: u64,
    merge: &MergeHint,
    pipeline: &PipelineHint,
    outcome: Outcome<'_>,
) -> String {
    let mut text = String::from("--- batch ---\n");
    let list = |items: Vec<String>| format!("[{}]", items.join(","));
    let _ = writeln!(text, "queries={}", list(queries.to_vec()));
    match &outcome {
        Outcome::Measured { .. } => text.push_str("outcome=measured\n"),
        Outcome::OutOfMemory { reason, .. } => {
            let reason = reason.lines().collect::<Vec<_>>().join(" ");
            let _ = writeln!(text, "outcome=out-of-memory\nreason={reason}");
        }
    }
    let shared = fused
        .shared
        .iter()
        .filter(|group| !group.consumers.is_empty())
        .map(|group| {
            let readers: Vec<&str> = group
                .consumers
                .iter()
                .map(|q| queries[*q].as_str())
                .collect();
            format!("{}:{}", group.table, readers.join("+"))
        });
    let _ = writeln!(text, "shared={}", list(shared.collect()));
    let unshared = fused
        .unshared
        .iter()
        .map(|(query, table, why)| format!("{}/{table}:{why}", queries[*query]));
    let _ = writeln!(text, "unshared={}", list(unshared.collect()));
    let reads = fused.reads.iter().map(|(table, n)| format!("{table}:{n}"));
    let _ = writeln!(text, "reads={}", list(reads.collect()));
    let buffered = |record: &&HoldRecord| matches!(record.decision.choice, Choice::Buffer { .. });
    let buffers: Vec<_> = fused.holds.iter().filter(buffered).collect();
    let _ = writeln!(text, "buffers={}", buffers.len());
    let bytes: u64 = buffers.iter().map(|record| record.decision.bytes).sum();
    let _ = writeln!(text, "buffer_bytes={bytes}");
    let _ = writeln!(text, "rereads={}", fused.holds.len() - buffers.len());
    for record in &fused.holds {
        let (choice, above) = match record.decision.choice {
            Choice::Buffer { .. } => ("buffer", record.above.map_or("-".into(), |n| n.to_string())),
            Choice::ReRead => ("re-read", "-".to_string()),
        };
        let _ = writeln!(
            text,
            "hold query={} table={} hold={} choice={choice} above={above} bytes={} reread_us={} \
             why={}",
            queries[record.query],
            record.table,
            record.hold,
            record.decision.bytes,
            record.decision.reread_us,
            record.decision.why
        );
    }
    let _ = writeln!(
        text,
        "cap={}\npool_bytes={}\nbudget_bytes={}\nestimate_bytes={}\nover_budget={}",
        decision.cap,
        pool_bytes,
        decision.budget_bytes,
        decision.estimate_bytes,
        decision.over_budget
    );
    if let Outcome::Measured { .. } = outcome {
        text.push_str("answers=match\n");
    }
    let _ = writeln!(text, "--- merge ---\nsaved_us={}", merge.saved_us);
    for entry in &merge.entries {
        let members: Vec<String> = entry
            .members
            .iter()
            .map(|origin| format!("{}:{}", queries[origin.query], origin.node))
            .collect();
        let _ = writeln!(
            text,
            "entry saved_us={} members={} node={}",
            entry.saved_us,
            members.join(","),
            entry.node
        );
    }
    let tables = source_tables(fused);
    let order = pipeline.load_order.iter().map(|at| tables[*at].clone());
    let _ = writeln!(
        text,
        "--- pipeline ---\nmodel={PIPELINE_MODEL}\nload_order={}\nserial_us={}\nmakespan_us={}\n\
         saved_us={}",
        list(order.collect()),
        pipeline.serial_us,
        pipeline.makespan_us,
        pipeline.saved_us
    );
    match outcome {
        Outcome::Measured {
            tree,
            measured_us,
            run,
        } => {
            let estimate = measured_us.saturating_sub(pipeline.saved_us);
            let _ = write!(
                text,
                "fused_measured_us={measured_us}\npipelined_estimate_us={estimate}\n\
                 --- tree ---\n{tree}{run}"
            );
        }
        Outcome::OutOfMemory { run, .. } => text.push_str(run),
    }
    text
}

/// Each source of the fused DAG, by its pipelining ordinal: the table it loads.
fn source_tables(fused: &Fused) -> Vec<String> {
    let roots: Vec<&dyn GpuNode> = fused.roots.iter().map(|root| root.as_ref()).collect();
    let dag = PlanDag::of(&roots);
    dag.nodes
        .iter()
        .filter_map(|node| match as_node_ref(node.node) {
            NodeRef::LoadParquet(load) => Some(load.table.clone()),
            _ => None,
        })
        .collect()
}

/// The block a section's line belongs to, by the `--- … ---` line above it.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Block {
    Batch,
    Merge,
    Pipeline,
    Tree,
    Run,
}

pub(crate) fn parse_fused_section(body: &str) -> Option<FusedSection<'_>> {
    if body.starts_with(SKIPPED) || body.starts_with(FAILED) {
        return None;
    }
    let mut section = FusedSection {
        batch: Vec::new(),
        holds: Vec::new(),
        merge: Vec::new(),
        pipeline: Vec::new(),
        nodes: Vec::new(),
        roots: Vec::new(),
        trailer: Vec::new(),
    };
    let mut tree = Tree::default();
    let mut block = None;
    for line in body.lines().filter(|line| !line.trim().is_empty()) {
        let opened = match line {
            "--- batch ---" => Some(Block::Batch),
            "--- merge ---" => Some(Block::Merge),
            "--- pipeline ---" => Some(Block::Pipeline),
            "--- tree ---" => Some(Block::Tree),
            "--- run ---" => Some(Block::Run),
            _ => None,
        };
        if opened.is_some() {
            block = opened;
            continue;
        }
        let pair = || {
            line.split_once('=')
                .unwrap_or_else(|| panic!("a fused section's line {line:?} is not `key=value`"))
        };
        match block {
            Some(Block::Batch) if line.starts_with("hold ") => section.holds.push(line),
            Some(Block::Batch) => section.batch.push(pair()),
            Some(Block::Merge) => section.merge.push(line),
            Some(Block::Pipeline) => section.pipeline.push(pair()),
            Some(Block::Tree) => tree.read(line, &mut section),
            Some(Block::Run) => section.trailer.push(pair()),
            None => panic!("a fused section's line {line:?} lies above `--- batch ---`"),
        }
    }
    if let Some(at) = tree.timed.iter().position(|timed| !timed) {
        panic!(
            "{} has no time line beneath it",
            section.nodes[at].line.name
        );
    }
    Some(section)
}

/// The `--- tree ---` block as `render_timings_of_roots` writes it: `root <i>:` before each
/// root when there are several, a node several parents read written once as `@<k> <line>` and
/// referred to after as `@<k>` alone, and each node's time line right beneath it.
#[derive(Default)]
struct Tree {
    /// The node at each depth on the current path.
    open: Vec<usize>,
    /// By label `k − 1`, the node it names.
    labels: Vec<usize>,
    /// Whether each node has had its time line.
    timed: Vec<bool>,
    /// The depth a root's own line sits at: 1 under `root <i>:` headers, 0 without.
    base: usize,
}

impl Tree {
    fn read<'a>(&mut self, line: &'a str, section: &mut FusedSection<'a>) {
        let text = line.trim_start();
        let indent = (line.len() - text.len()) / 2;
        if text.starts_with("root ") && text.ends_with(':') {
            self.base = 1;
            self.open.clear();
            section.roots.push(section.nodes.len());
            return;
        }
        if text.starts_with("time_us=") {
            let at = section
                .nodes
                .len()
                .checked_sub(1)
                .expect("a time line beneath a node");
            if std::mem::replace(&mut self.timed[at], true) {
                panic!("{} has its time line twice", section.nodes[at].line.name);
            }
            read_time_line(&mut section.nodes[at], text);
            return;
        }
        let depth = indent
            .checked_sub(self.base)
            .unwrap_or_else(|| panic!("a fused tree's line {line:?} lies above its root"));
        self.open.truncate(depth);
        let parent = self.open.last().copied();
        let (label, rest) = split_label(text);
        if let (Some(label), None) = (label, rest) {
            let parent = parent.expect("a reference sits under a node");
            section.nodes[parent].children.push(self.labels[label - 1]);
            return;
        }
        let parsed = parse_node_line(rest.unwrap_or(text))
            .unwrap_or_else(|| panic!("a fused tree's line {line:?} is no node line"));
        let at = section.nodes.len();
        match parent {
            Some(parent) => section.nodes[parent].children.push(at),
            None if self.base == 0 => section.roots.push(at),
            None => {}
        }
        if label.is_some() {
            self.labels.push(at);
        }
        section.nodes.push(BenchNode {
            line: NodeLine {
                name: parsed.name,
                depth,
                fields: parsed.fields,
            },
            time_us: Vec::new(),
            total_us: 0,
            batch_rows: None,
            batch_bytes: None,
            children: Vec::new(),
        });
        self.timed.push(false);
        self.open.push(at);
    }
}

/// `@<k> <line>` as `(Some(k), Some(line))`, `@<k>` alone as `(Some(k), None)`, anything
/// else as `(None, None)`.
fn split_label(text: &str) -> (Option<usize>, Option<&str>) {
    let Some(rest) = text.strip_prefix('@') else {
        return (None, None);
    };
    let (number, line) = match rest.split_once(' ') {
        Some((number, line)) => (number, Some(line)),
        None => (rest, None),
    };
    match number.parse() {
        Ok(label) => (Some(label), line),
        Err(_) => (None, None),
    }
}

/// `time_us=… total_us=… batch_rows=… batch_bytes=…`, each token `key=value`.
fn read_time_line(node: &mut BenchNode<'_>, line: &str) {
    for token in line.split_whitespace() {
        let (key, value) = token
            .split_once('=')
            .unwrap_or_else(|| panic!("a time line's token {token:?} is not `key=value`"));
        match key {
            "time_us" => node.time_us = nested(value),
            "total_us" => node.total_us = value.parse().expect("total_us is a number"),
            "batch_rows" => node.batch_rows = Some(nested(value)),
            "batch_bytes" => node.batch_bytes = Some(nested(value)),
            other => panic!("unknown key `{other}` beneath a node, in: {line}"),
        }
    }
}

/// `[[1,2],[3],[]]`: lanes, then entries.
fn nested(value: &str) -> Vec<Vec<u64>> {
    let inner = value
        .strip_prefix('[')
        .and_then(|v| v.strip_suffix(']'))
        .unwrap_or_else(|| panic!("{value:?} is not a list of lists"));
    if inner.is_empty() {
        return Vec::new();
    }
    let lanes = inner
        .strip_prefix('[')
        .and_then(|v| v.strip_suffix(']'))
        .unwrap_or_else(|| panic!("{value:?} is not a list of lists"));
    lanes
        .split("],[")
        .map(|lane| {
            lane.split(',')
                .filter(|entry| !entry.is_empty())
                .map(|entry| {
                    entry
                        .parse()
                        .unwrap_or_else(|_| panic!("{entry:?} in {value}"))
                })
                .collect()
        })
        .collect()
}
