//! The plan-time memory model: what each node holds, and how large a source's batches may
//! be for the whole plan to fit a budget.
//!
//! Two passes. The first walks from every source up to the nearest accumulator, because
//! that is exactly where resident stops scaling with batch size — a join's build side holds
//! a whole relation and an aggregate's state one row per group, so those come off the
//! budget as constants before anything is divided. The second spends what is left.
//!
//! Every cardinality here is the trivial estimate the planner has today (#19, #73): a
//! filter passes everything and a join is 1:1. Widths are real, from the declared schemas.

use std::collections::HashMap;

use datafusion::arrow::datatypes::{Fields, Schema as ArrowSchema};

use super::{MIN_TARGET_BATCH_BYTES, MemoryModel, SourceEstimate};
use crate::common::logical_size_from_schema;
use crate::plan::GpuNode;
use crate::plan::NodeKind;
use crate::plan::PlanError;
use crate::plan::{NodeRef, as_node_ref};

pub(crate) fn estimate(root: &dyn GpuNode, budget: u64) -> Result<MemoryModel, PlanError> {
    let tree = Tree::of(root);
    let accumulator_bytes: u64 = (0..tree.nodes.len())
        .filter_map(|seq| tree.held_by_accumulator(seq))
        .sum();

    let certain_accumulator_bytes: u64 = (0..tree.nodes.len())
        .filter(|seq| tree.rows_are_certain(*seq))
        .filter_map(|seq| tree.held_by_accumulator(seq))
        .sum();

    // The planner refuses on a constant only where the constant cannot be an overestimate.
    // A build side over a scan is its input's rows; an aggregate's state rests on the
    // cardinality estimate, which today says one row per input row, so refusing on it
    // would turn "we do not know" into "you cannot run this".
    if certain_accumulator_bytes >= budget {
        return Err(PlanError::Invalid(format!(
            "what the accumulators hold and cannot be holding less of \
             ({certain_accumulator_bytes} bytes) is already the whole budget ({budget}), so \
             no batch size makes this plan run"
        )));
    }
    // Where the estimated constants alone exhaust the budget, batches are sized against the
    // ones we know and the accountant owns the rest.
    let remainder = budget
        .checked_sub(accumulator_bytes)
        .filter(|left| *left > 0)
        .unwrap_or(budget - certain_accumulator_bytes);

    // Equal shares rather than proportional ones: a proportional split hands the most
    // budget to the source already producing the most bytes.
    let seqs = tree.sources();
    let share_per_source = remainder / seqs.len().max(1) as u64;
    let sources: Vec<SourceEstimate> = seqs
        .iter()
        .map(|seq| {
            let amplification = tree.amplification(*seq);
            let afforded = coarse((share_per_source as f64 / amplification).floor() as u64);
            SourceEstimate {
                seq: *seq,
                amplification,
                // A batch is never larger than what the source holds — a scan pruned to a
                // few row groups is where that is orders of magnitude.
                target_batch_bytes: afforded.min(tree.source_bytes(*seq)),
            }
        })
        .collect();

    let targets: HashMap<usize, u64> = sources
        .iter()
        .map(|source| (source.seq, source.target_batch_bytes))
        .collect();
    let batch_bytes = tree.batch_bytes(&targets);
    let resident = (0..tree.nodes.len())
        .map(|seq| match tree.held_by_accumulator(seq) {
            // A join holds its build side and is handed a probe batch per lane on top of
            // it; every other accumulator's held already is what it received.
            Some(held) => held + tree.streamed_into(seq, &batch_bytes),
            None => batch_bytes[seq] * tree.lanes(seq),
        })
        .collect();

    Ok(MemoryModel {
        budget,
        accumulator_bytes,
        certain_accumulator_bytes,
        share_per_source,
        resident,
        sources,
    })
}

/// Rounds down onto a coarse grid. The inputs are the optimizer's estimates, and an
/// estimate that drifts slightly should not regenerate every golden.
fn coarse(bytes: u64) -> u64 {
    if bytes < MIN_TARGET_BATCH_BYTES {
        return MIN_TARGET_BATCH_BYTES;
    }
    1 << (63 - bytes.leading_zeros() as u64)
}

/// The tree in canonical post-order — children left to right, then the node — which is the
/// order the goldens render and the order every per-node figure is indexed by.
struct Tree<'a> {
    nodes: Vec<&'a dyn GpuNode>,
    children: Vec<Vec<usize>>,
    parent: Vec<Option<usize>>,
}

impl<'a> Tree<'a> {
    fn of(root: &'a dyn GpuNode) -> Self {
        let mut tree = Tree {
            nodes: Vec::new(),
            children: Vec::new(),
            parent: Vec::new(),
        };
        tree.visit(root);
        for seq in 0..tree.nodes.len() {
            for child in tree.children[seq].clone() {
                tree.parent[child] = Some(seq);
            }
        }
        tree
    }

    fn visit(&mut self, node: &'a dyn GpuNode) -> usize {
        let children: Vec<usize> = node.children().into_iter().map(|c| self.visit(c)).collect();
        self.nodes.push(node);
        self.children.push(children);
        self.parent.push(None);
        self.nodes.len() - 1
    }

    fn sources(&self) -> Vec<usize> {
        (0..self.nodes.len())
            .filter(|seq| matches!(self.nodes[*seq].kind(), NodeKind::Source { .. }))
            .collect()
    }

    /// Whether a node's row count is a fact rather than an estimate. A filter's
    /// selectivity, an aggregate's group count and a join's cardinality are all the
    /// trivial estimates the planner has today (#19, #73), and each of them can only
    /// overstate what its subtree really produces.
    fn rows_are_certain(&self, seq: usize) -> bool {
        let guessed = matches!(
            as_node_ref(self.nodes[seq]),
            NodeRef::Filter(_)
                | NodeRef::Aggregate(_)
                | NodeRef::AggregateBatches(_)
                | NodeRef::Join(_)
                | NodeRef::CrossJoin(_)
                | NodeRef::NestedLoopJoin(_)
        );
        !guessed && self.children[seq].iter().all(|c| self.rows_are_certain(*c))
    }

    /// What a source holds in total, over the columns it projects.
    fn source_bytes(&self, seq: usize) -> u64 {
        match as_node_ref(self.nodes[seq]) {
            NodeRef::LoadParquet(load) => load.bytes().max(1),
            _ => u64::MAX,
        }
    }

    fn lanes(&self, seq: usize) -> u64 {
        match self.nodes[seq].kind().layout() {
            Some(layout) => layout.n as u64,
            // A sink holds nothing of its own; its input's lane count is that node's.
            None => 1,
        }
    }

    fn width(&self, seq: usize) -> u64 {
        match self.nodes[seq].kind().schema() {
            Some(schema) => logical_size_from_schema(&schema.fields, 1, 0) as u64,
            None => self.children[seq]
                .first()
                .map(|child| self.width(*child))
                .unwrap_or(1),
        }
        .max(1)
    }

    /// Rows a node emits. Selectivity and join cardinality are the trivial estimates the
    /// planner has today, so a filter passes everything and a join is 1:1 against its
    /// larger side; a limit is the one node that knows better.
    fn rows(&self, seq: usize) -> u64 {
        match as_node_ref(self.nodes[seq]) {
            NodeRef::LoadParquet(load) => load.rows(),
            NodeRef::Limit(limit) => {
                let input = self.rows(self.children[seq][0]);
                match limit.interval.fetch {
                    Some(fetch) => input.min(limit.interval.skip + fetch),
                    None => input.saturating_sub(limit.interval.skip),
                }
            }
            NodeRef::Union(_) | NodeRef::Interleave(_) => {
                self.children[seq].iter().map(|c| self.rows(*c)).sum()
            }
            NodeRef::Join(_) | NodeRef::CrossJoin(_) | NodeRef::NestedLoopJoin(_) => self.children
                [seq]
                .iter()
                .map(|c| self.rows(*c))
                .max()
                .unwrap_or(0),
            _ => self.children[seq]
                .first()
                .map(|child| self.rows(*child))
                .unwrap_or(0),
        }
    }

    fn bytes(&self, seq: usize) -> u64 {
        self.rows(seq) * self.width(seq)
    }

    /// What a node holds whatever the batch size — `None` for a node whose residency
    /// scales with its input batch. A mid-plan limit is a `BatchAccumulator` by category
    /// and holds nothing at all, which is why it is not one of these.
    fn held_by_accumulator(&self, seq: usize) -> Option<u64> {
        match as_node_ref(self.nodes[seq]) {
            NodeRef::CoalesceAllBatches(_) | NodeRef::MergeSortedPartitions(_) => {
                Some(self.bytes(self.children[seq][0]))
            }
            NodeRef::AccumulateBatchesAndSort(accumulator) => {
                let input = self.children[seq][0];
                let rows = match accumulator.fetch {
                    // A top-N holds n rows per lane, which is what makes it bounded.
                    Some(fetch) => self.rows(input).min(fetch as u64 * self.lanes(input)),
                    None => self.rows(input),
                };
                Some(rows * self.width(seq))
            }
            // One row per group, and with no cardinality estimate the worst case is one
            // group per input row (#19 is what would sharpen it).
            NodeRef::AggregateBatches(_) => {
                Some(self.rows(self.children[seq][0]) * self.width(seq))
            }
            NodeRef::Join(join) => {
                let build = self.bytes(self.children[seq][0]);
                // A build-preserving join on the frozen surface also holds the key columns
                // of every probe row it has seen, per lane, until the finish pass — the
                // term that decides whether such a plan fits (#136).
                let keys = match join.capability() {
                    Ok(capability) if capability.needs_finish => {
                        let probe = self.children[seq][1];
                        self.rows(probe) * self.key_width(join, probe)
                    }
                    _ => 0,
                };
                Some(build + keys)
            }
            NodeRef::CrossJoin(_) | NodeRef::NestedLoopJoin(_) => {
                Some(self.bytes(self.children[seq][0]))
            }
            _ => None,
        }
    }

    /// What arrives at a node while it holds its state: a join's probe batch, per lane.
    fn streamed_into(&self, seq: usize, batch_bytes: &[u64]) -> u64 {
        match as_node_ref(self.nodes[seq]) {
            NodeRef::Join(_) | NodeRef::CrossJoin(_) | NodeRef::NestedLoopJoin(_) => {
                batch_bytes[self.children[seq][1]] * self.lanes(seq)
            }
            _ => 0,
        }
    }

    /// The key columns a finish pass accumulates, per probe row.
    fn key_width(&self, join: &crate::plan::GpuHashJoin, probe: usize) -> u64 {
        let Some(schema) = self.nodes[probe].kind().schema() else {
            return 0;
        };
        join.keys
            .iter()
            .filter_map(|(_, ordinal)| schema.fields.fields().get(*ordinal as usize))
            .map(|field| {
                let one = ArrowSchema::new(Fields::from(vec![field.as_ref().clone()]));
                logical_size_from_schema(&one, 1, 0) as u64
            })
            .sum()
    }

    /// A source's amplification: the largest its batch gets anywhere on the way to the
    /// accumulator that ends the walk, counting the lanes live at that point. The maximum
    /// rather than either end — a batch is rarely widest where it starts, and above a
    /// merge the same batch costs one lane's worth rather than N.
    fn amplification(&self, source: usize) -> f64 {
        let (rows, width) = (self.rows(source).max(1), self.width(source));
        let mut node = source;
        let mut amplification = self.lanes(source) as f64;
        while let Some(parent) = self.parent[node] {
            if self.held_by_accumulator(parent).is_some() {
                break;
            }
            let factor = (self.rows(parent) as f64 / rows as f64)
                * (self.width(parent) as f64 / width as f64)
                * self.lanes(parent) as f64;
            amplification = amplification.max(factor);
            node = parent;
        }
        amplification.max(1.0)
    }

    /// One in-flight batch at each node, bottom up: a source emits its target, an
    /// accumulator emits what it held as one batch per lane, and everything between scales
    /// its input by the rows and width it changes.
    fn batch_bytes(&self, targets: &HashMap<usize, u64>) -> Vec<u64> {
        let _ = targets;
        let mut batch = vec![0u64; self.nodes.len()];
        for seq in 0..self.nodes.len() {
            batch[seq] = if let NodeRef::LoadParquet(load) = as_node_ref(self.nodes[seq]) {
                // What this mapping emits, which is the budget's target only where the
                // budget is what cut the batches.
                load.largest_batch_bytes()
            } else if let Some(held) = self.held_by_accumulator(seq) {
                held / self.lanes(seq).max(1)
            } else {
                self.children[seq]
                    .iter()
                    .map(|child| {
                        let child_bytes = self.bytes(*child).max(1);
                        (batch[*child] as f64 * self.bytes(seq) as f64 / child_bytes as f64) as u64
                    })
                    .max()
                    .unwrap_or(0)
            };
        }
        batch
    }
}

#[cfg(test)]
mod tests;
