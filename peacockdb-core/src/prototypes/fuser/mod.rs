//! query-fuser: N queries' tp1_single plans fused into one DAG on dag-driver, in which each
//! parquet file is loaded once and feeds every query that reads it.
//!
//! Only sources are merged. A scan stays unshared where sharing could change its query's
//! answer: a limit cut it to a prefix, its columns would gain NULLs, or it reads rows only
//! beside scans that read columns. Every reader of a shared scan that sits inside a hold is
//! buffered or reads the table again, as a caller-supplied chooser decides. It runs at sf40
//! only: the benchmark's chooser reads the sf40 baseline, and nothing else may supply figures.
//! `llm-wiki/tasks/query-fuser-detail.md` records the decisions.

mod baseline;
mod fuse;
mod lists;
mod merge;
mod pipeline;
#[cfg(test)]
mod tests;

use std::num::NonZeroUsize;

use crate::plan::{Batching, GpuNode, PlanError};
use crate::test_support::{BenchSection, DeviceCase};

/// One query of a batch: its name, and its own planned tree.
pub(crate) struct Query<'a> {
    pub(crate) name: String,
    pub(crate) tree: &'a dyn GpuNode,
}

/// Why a scan of a file other scans read stayed unshared. `Display`: `limit-cut`,
/// `nulls:<column>`, `rows-only`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Unshared {
    /// A limit kept only the covering prefix of its row groups, and no filter rejects the rest.
    LimitCut,
    /// The union's row groups can hold a NULL in this column, and the scan's own could not:
    /// its plan's null handling was planned from its own groups.
    Nulls { column: String },
    /// It reads rows only, beside scans that read columns: a column selection cannot make the
    /// row marker.
    RowsOnly,
}

/// The scans of one file that read one shared load.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ShareGroup {
    pub(crate) table: String,
    /// The query ordinal of each member still reading the shared load; a query that reads the
    /// file twice is here twice.
    pub(crate) consumers: Vec<usize>,
    /// Each such member's own load, where it sits in its query's tree.
    pub(crate) members: Vec<Origin>,
}

/// One edge where a shared source enters a hold from outside, as dag-driver's validation names
/// it. Indices are `PlanDag::of(&fused.roots)`'s.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HeldReader {
    /// The root ordinal of the query the reader belongs to.
    pub(crate) query: usize,
    /// The `ShareGroup` ordinal of the source it reads.
    pub(crate) group: usize,
    /// The node whose hold it enters: a join, or a root waiting for the cap.
    pub(crate) hold: usize,
    /// Edges `(parent, slot)`, lowest first: from the edge above the source, or above the
    /// project inserted to select from it, up to the hold's entry — `(join, probe)` or
    /// `(root, 0)`.
    pub(crate) path: Vec<(usize, usize)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Choice {
    /// The reader loads its own scan again, once the hold lifts.
    ReRead,
    /// A `GpuHoldBuffer` on `edge`, one of the reader's `path`.
    Buffer { edge: (usize, usize) },
}

/// What a chooser decided for one held reader, with the figures it decided on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HoldDecision {
    pub(crate) choice: Choice,
    /// What a buffer would keep.
    pub(crate) bytes: u64,
    /// What a re-read would cost in load time.
    pub(crate) reread_us: u64,
    pub(crate) why: String,
}

/// One held reader resolved, in the order they were.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HoldRecord {
    pub(crate) query: usize,
    pub(crate) table: String,
    /// The hold's node, by name.
    pub(crate) hold: String,
    /// For a buffer, the pre-order index in the query's own tree of the node it sits above.
    pub(crate) above: Option<usize>,
    pub(crate) decision: HoldDecision,
}

/// A node of a query's own tree: the query's ordinal and the node's pre-order index there.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct Origin {
    pub(crate) query: usize,
    pub(crate) node: usize,
}

/// N plans fused into one DAG, and what the fusing decided.
pub(crate) struct Fused {
    /// One root per query, in input order.
    pub(crate) roots: Vec<Box<dyn GpuNode>>,
    /// In the order each group's first member appears.
    pub(crate) shared: Vec<ShareGroup>,
    /// `(query ordinal, table, why)` per scan left unshared beside others of its file.
    pub(crate) unshared: Vec<(usize, String, Unshared)>,
    pub(crate) holds: Vec<HoldRecord>,
    /// Per table, in order of first appearance: the distinct loads of it in the DAG.
    pub(crate) reads: Vec<(String, usize)>,
    /// By the fused DAG's pre-order index: the query nodes each fused node stands for. A shared
    /// load has one per member; an inserted project or buffer has none.
    pub(crate) origins: Vec<Vec<Origin>>,
    /// The queries' own trees, which `origins` index and a re-read loads from.
    originals: Vec<Box<dyn GpuNode>>,
}

/// Decides one held reader. It sees the plan as fused so far.
pub(crate) type Chooser<'c> = dyn FnMut(&Fused, &HeldReader) -> HoldDecision + 'c;

/// `queries` fused: every scan of a file that can share reads one load of the union of their
/// columns and row groups, and every reader of it inside a join's hold is resolved by `choose`.
/// Sized batching is refused: the fuser plans tp1_single.
pub(crate) fn fuse(
    queries: &[Query<'_>],
    batching: Batching,
    choose: &mut Chooser<'_>,
) -> Result<Fused, PlanError> {
    fuse::fuse(queries, batching, choose)
}

/// `fused` under a concurrency cap: every reader of a shared source in a root that waits for
/// the cap is resolved by `choose`, as a join's held readers are.
pub(crate) fn resolve_cap_holds(
    fused: Fused,
    cap: NonZeroUsize,
    choose: &mut Chooser<'_>,
) -> Result<Fused, PlanError> {
    fuse::resolve_cap_holds(fused, cap, choose)
}

/// The batch sizes, each a prefix of the ordered list; `None` is the whole list.
pub(crate) const BATCH_SIZES: [(&str, Option<usize>); 4] = [
    ("first3", Some(3)),
    ("first10", Some(10)),
    ("first20", Some(20)),
    ("all", None),
];

/// `dataset`'s queries a batch may hold, in file order: its `ok` and `wrong` rows at
/// tp1_single, and for tpch only q1-q22. Panics naming the dataset when none is left.
pub(crate) fn batch_set(dataset: &str, cases: &[DeviceCase]) -> Vec<String> {
    lists::batch_set(dataset, cases)
}

/// [`batch_set`] over the H100 tree's `cases.csv`, read now. Panics naming the file when it is
/// missing. The one entry point: the lib's tests and the fused benchmark both call it.
pub(crate) fn ordered_queries(dataset: &str) -> Vec<String> {
    lists::ordered_queries(dataset)
}

/// Each of [`BATCH_SIZES`] over `list`: its prefix, or, for a size the list does not exceed
/// (`first3` aside, which always runs), a marker saying it is the same set as `all`.
pub(crate) fn batches(list: &[String]) -> Vec<(&'static str, Result<Vec<String>, String>)> {
    lists::batches(list)
}

/// The sf40 baseline of a batch, one entry per query in batch order.
pub(crate) struct Baseline {
    pub(crate) per_query: Vec<QueryTimes>,
}

/// One query's `.benchmark.txt` section, every vector by the node's pre-order index in the
/// query's own tree, which is what an [`Origin`] indexes.
pub(crate) struct QueryTimes {
    pub(crate) names: Vec<String>,
    pub(crate) children: Vec<Vec<usize>>,
    pub(crate) total_us: Vec<u64>,
    /// Σ over lanes and batches.
    pub(crate) batch_rows: Vec<u64>,
    /// Σ over lanes and batches.
    pub(crate) batch_bytes: Vec<u64>,
    pub(crate) load_us: u64,
    pub(crate) execute_us: u64,
    pub(crate) peak_bytes: u64,
}

impl Baseline {
    /// One [`QueryTimes`] per section. Panics naming the query and node when a section lacks
    /// `batch_rows` or `batch_bytes`, or its trailer `load_us`, `execute_us` or `peak_bytes`.
    pub(crate) fn of(sections: &[BenchSection<'_>]) -> Baseline {
        baseline::of(sections)
    }

    /// `Ok` when each query's own tree, in pre-order, has its section's node names; else the
    /// first mismatch, naming the query and the node index.
    pub(crate) fn check(&self, queries: &[Query<'_>]) -> Result<(), String> {
        baseline::check(self, queries)
    }
}

/// One identical subtree several queries compute, and what merging it would save.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MergeEntry {
    /// The subtree root's plan line.
    pub(crate) node: String,
    /// Each copy's subtree root, as a node of its own query.
    pub(crate) members: Vec<Origin>,
    pub(crate) saved_us: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MergeHint {
    /// Largest saving first.
    pub(crate) entries: Vec<MergeEntry>,
    pub(crate) saved_us: u64,
}

/// What merging the identical subtrees above the shared sources would save. A report, not an
/// execution. The model, by the fused DAG's nodes:
/// - Each node's key is its plan line and its children's keys; a shared source's is its
///   sharing key, and a `GpuHoldBuffer`'s its child's, since it computes nothing.
/// - A group is two or more nodes of one key whose subtree reaches a shared source, each
///   standing for a node of its query's own tree. A root, the crossing to the host, is in none.
/// - An entry is a group no other group of the same size contains member for member (each
///   member's parent, through any buffer, in it). A copy's time is Σ baseline `total_us` over
///   its subtree, loads, inserted projects and buffers excluded; the entry saves Σ − max: one
///   copy stays, the dearest. Entries come largest saving first.
/// - The batch saves Σ over every group of (Σ own `total_us` − max own `total_us`): each node is
///   in one group, so nothing is counted twice.
pub(crate) fn merge_hint(fused: &Fused, baseline: &Baseline) -> MergeHint {
    merge::merge_hint(fused, baseline)
}

/// What one root needs before it can run: its baseline execute time, and the sources it
/// reads, as ordinals into the load times beside it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RootNeeds {
    pub(crate) execute_us: u64,
    pub(crate) sources: Vec<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PipelineHint {
    /// Source ordinals, in the order the model loads them.
    pub(crate) load_order: Vec<usize>,
    /// Σ loads + Σ executes: the run with nothing overlapped.
    pub(crate) serial_us: u64,
    pub(crate) makespan_us: u64,
    /// `serial_us − makespan_us`.
    pub(crate) saved_us: u64,
}

/// What overlapping loads with execution would save. A report, not an execution. The model:
/// - One loader and one executor, each sequential, running at the same time.
/// - A root starts once its last source is loaded and the executor is free; roots execute in
///   the order they become ready, ties by root order.
/// - Loads go greedily: next is the root whose missing sources take the least load time
///   (ties by root order), and its missing sources are appended in ordinal order.
/// - Against a measured fused run, the estimate is `(load_us + execute_us) − saved_us`.
pub(crate) fn pipeline_hint(load_us: &[u64], roots: &[RootNeeds]) -> PipelineHint {
    pipeline::pipeline_hint(load_us, roots)
}

/// [`pipeline_hint`]'s inputs off a fused plan. A source is every distinct `GpuLoadParquet` in
/// the fused DAG, ordinals in its pre-order, so a re-read is a source of its own. A shared
/// source loads in the **largest** baseline `total_us` among the loads it stands for: the union
/// reads at least as much as any of them, so this is a lower bound. A root executes in its
/// baseline trailer's `execute_us`.
pub(crate) fn pipeline_inputs(fused: &Fused, baseline: &Baseline) -> (Vec<u64>, Vec<RootNeeds>) {
    pipeline::pipeline_inputs(fused, baseline)
}
