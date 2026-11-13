//! query-fuser: N queries' tp1_single plans fused into one DAG on dag-driver, in which each
//! parquet file is loaded once and feeds every query that reads it.
//!
//! Only sources are merged. A scan stays unshared where sharing could change its query's
//! answer: a limit cut it to a prefix, its columns would gain NULLs, or it reads rows only
//! beside scans that read columns. Every reader of a shared scan that sits inside a hold is
//! buffered or reads the table again, as a caller-supplied chooser decides. It runs at sf40
//! only: the benchmark's chooser reads the sf40 baseline, and nothing else may supply figures.
//! `llm-wiki/designs/query-fuser.md` holds its rules and format.

mod baseline;
mod benchmark;
mod cap;
mod fuse;
mod holds;
mod lists;
mod loads;
mod merge;
mod pipeline;
mod report;
mod section;
#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::path::Path;

use std::path::PathBuf;

use crate::executor::RunError;
use crate::plan::{Batching, GpuNode, PlanDag, PlanError};
use crate::prototypes::FusedSection;
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
    /// It is alone on its side of the rows-only split: it reads rows only beside scans that
    /// read columns, or columns beside scans that read rows only. A column selection cannot
    /// make the row marker, and the label names the split, not the scan.
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
    /// The reader's own load, the member of the group it stands in for: what a re-read loads.
    pub(crate) member: Origin,
    /// Its root waits for the cap being resolved, so no buffer of it may keep state below.
    pub(crate) waits: bool,
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

impl Fused {
    /// The node of a query's own tree that `origin` names.
    pub(crate) fn original(&self, origin: Origin) -> &dyn GpuNode {
        fuse::original(self, origin)
    }
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

/// The card the fused runs are measured on: its case list orders the batches, and its sf40
/// baseline and `loads.tsv` price them.
pub(crate) const DEVICE: &str = "h100-80gb-hbm3";

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

/// What merging the identical subtrees above the shared sources would save: a report, not an
/// execution. A node's key is its plan line and its children's keys (a shared source's is its
/// sharing key, a buffer's its child's). A group is two or more nodes of one key, from two or
/// more queries, whose subtree reaches a shared source; no root is in one. An entry is a group
/// no same-size group contains member for member (each member's parent, through any buffer, in
/// it). It saves Σ − max of its copies' Σ baseline `total_us` over their subtrees, loads,
/// inserted projects and buffers excluded; one copy stays. Entries come largest saving first.
/// The batch saves Σ over every group of (Σ own `total_us` − max own `total_us`), each node
/// being in one group.
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

/// One case's `loads.tsv` rows: a load's time by the query and its `CudfScan` recipe seq.
pub(crate) struct LoadTimes {
    by_key: HashMap<(String, u64), u64>,
}

impl LoadTimes {
    /// The load's `load_us`. Panics naming the key where `loads.tsv` has no row: no fallback.
    pub(crate) fn load_us(&self, query: &str, recipe_seq: u64) -> u64 {
        loads::load_us(self, query, recipe_seq)
    }
}

/// The rows of `path` (pinned-loads' `loads.tsv`) for one case: `dataset`, `sf` and `mode` as
/// the file spells them. The `#` preamble is skipped.
pub(crate) fn read_load_times(path: &Path, dataset: &str, sf: &str, mode: &str) -> LoadTimes {
    loads::read_load_times(path, dataset, sf, mode)
}

/// One edge of a held reader's path, as the hold rule prices it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EdgeCost {
    pub(crate) edge: (usize, usize),
    /// What a buffer here would keep, the builds of the joins it runs early included.
    pub(crate) bytes: u64,
    pub(crate) allowed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HoldInputs {
    pub(crate) reread_us: u64,
    /// What a re-read adds to its root: the reader's own load's recorded `batch_bytes`, which
    /// its root is charged again once the load is not shared.
    pub(crate) reread_bytes: u64,
    /// In path order, lowest first; the edge right above the shared load is not among them.
    pub(crate) edges: Vec<EdgeCost>,
}

/// The allowed edge whose recorded bytes are smallest; a tie goes to the lower edge, which runs
/// less early.
pub(crate) fn place_buffer(edges: &[EdgeCost]) -> Option<&EdgeCost> {
    holds::place_buffer(edges)
}

/// What the budget leaves for a buffer: bytes to spare, or none, the charged bytes being over
/// it already.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Headroom {
    Left(u64),
    Over,
}

/// The hold rule's choice, with a buffer at [`place_buffer`]'s edge where one exists. Within
/// the budget, the buffer where its bytes fit the headroom, else a re-read. Over it, the
/// buffer where its bytes are below `reread_bytes`, what a re-read adds to its root, else a
/// re-read. A re-read costs `reread_us`, recorded either way. `why`: `fits: {b} <= {h}`,
/// `too large: {b} > {h}`, `over budget: {b} < re-read {r}`, `over budget: {b} >= re-read
/// {r}`, or `no edge: …` with the reason none was allowed.
pub(crate) fn decide(inputs: &HoldInputs, headroom: Headroom) -> HoldDecision {
    holds::decide(inputs, headroom)
}

/// The first node at or under `at` that keeps state: what a waiting root may not have below
/// its buffer (`validate_cap`).
pub(crate) fn state_under(dag: &PlanDag<'_>, at: usize) -> Option<usize> {
    holds::state_under(dag, at)
}

/// The hold rule's numbers for one held reader, all the sf40 baseline's, per origin node. An
/// edge's bytes are its child's Σ `batch_bytes` (an inserted project stands for the reader's
/// own load, an inserted buffer for its child; the edge right above the shared load is left
/// out), plus the build child's for each join below it on the path, which a buffer runs early.
/// An edge is allowed where no such join recorded more rows than its child on the path and,
/// where the reader's root waits (`held.waits`), where nothing at or under it keeps state. A
/// re-read costs the reader's own load's `loads.tsv` `load_us` (by `names` and its `CudfScan`
/// seq in `seqs`) and adds its `batch_bytes` to its root. In full:
/// `llm-wiki/designs/query-fuser.md`, "The hold rule".
pub(crate) fn hold_inputs(
    fused: &Fused,
    held: &HeldReader,
    baseline: &Baseline,
    loads: &LoadTimes,
    seqs: &[Vec<Option<u64>>],
    names: &[String],
) -> HoldInputs {
    holds::hold_inputs(fused, held, baseline, loads, seqs, names)
}

/// The benchmark's chooser: [`decide`] against the headroom left, spending it as it buffers.
pub(crate) struct HoldLedger<'b> {
    pub(crate) pool_bytes: u64,
    baseline: &'b Baseline,
    loads: &'b LoadTimes,
    seqs: &'b [Vec<Option<u64>>],
    names: &'b [String],
}

impl<'b> HoldLedger<'b> {
    pub(crate) fn new(
        pool_bytes: u64,
        baseline: &'b Baseline,
        loads: &'b LoadTimes,
        seqs: &'b [Vec<Option<u64>>],
        names: &'b [String],
    ) -> Self {
        Self {
            pool_bytes,
            baseline,
            loads,
            seqs,
            names,
        }
    }

    /// [`decide`] with headroom `budget − (S + B + max root_bytes)`, saturating, all three as
    /// [`cap_inputs`] charges `fused` at this call. So readers are decided in the order
    /// validation names them, each buffer spends what it keeps, a buffer taken out spends
    /// nothing, and a group a re-read dissolved is charged as it was rebuilt.
    pub(crate) fn choose(&mut self, fused: &Fused, held: &HeldReader) -> HoldDecision {
        holds::choose(self, fused, held)
    }
}

/// The share of the pool the cap rule keeps back for what the accountant's peak leaves out.
pub(crate) const MARGIN_PERCENT: u64 = 10;

pub(crate) struct CapInputs {
    pub(crate) pool_bytes: u64,
    pub(crate) shared_bytes: Vec<u64>,
    pub(crate) buffer_bytes: Vec<u64>,
    pub(crate) root_bytes: Vec<u64>,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct CapDecision {
    pub(crate) cap: NonZeroUsize,
    pub(crate) budget_bytes: u64,
    pub(crate) estimate_bytes: u64,
    pub(crate) over_budget: bool,
}

/// `pool_bytes − pool_bytes × MARGIN_PERCENT / 100`: what the cap rule may spend.
pub(crate) fn budget_of(pool_bytes: u64) -> u64 {
    cap::budget_of(pool_bytes)
}

/// The concurrency cap. The pool is the rmm pool the run installed, the device memory the
/// engine may hold; the margin covers what the accountant's peak (a lower bound) leaves out.
/// - `S = Σ shared_bytes` and `B = Σ buffer_bytes` are charged whatever the cap: the cap bounds
///   executor state only, and a buffer keeps its batches until its hold lifts.
/// - Roots are taken largest first; `cap` is the largest k with `S + B + Σ top k ≤ budget`, and
///   at least 1. `estimate_bytes = S + B + Σ top cap`; `over_budget = S + B + max > budget`, and
///   the run goes ahead regardless.
pub(crate) fn cap_for(inputs: &CapInputs) -> CapDecision {
    cap::cap_for(inputs)
}

/// The cap rule's inputs, from the sf40 baseline. Per shared load: its member with the largest
/// recorded load `batch_bytes`, scaled from that member's planned bytes to the union's. Per
/// buffer: its decision's bytes. Per root: its baseline `peak_bytes` less the recorded bytes of
/// its loads that are now shared, saturating; a re-read load is not shared, so it stays.
pub(crate) fn cap_inputs(fused: &Fused, baseline: &Baseline, pool_bytes: u64) -> CapInputs {
    cap::cap_inputs(fused, baseline, pool_bytes)
}

/// `measured × plan_to / plan_from` without overflow; `measured` itself where `plan_from` is 0.
pub(crate) fn scaled_bytes(measured: u64, plan_from: u64, plan_to: u64) -> u64 {
    cap::scaled_bytes(measured, plan_from, plan_to)
}

/// `fused` under the cap its numbers give: a cap below the root count makes roots wait, a hold
/// too, so their readers of shared loads are decided by `ledger` ([`resolve_cap_holds`]) and
/// the cap recomputed, until it no longer falls.
pub(crate) fn settle(
    fused: Fused,
    baseline: &Baseline,
    ledger: &mut HoldLedger<'_>,
) -> Result<(Fused, CapDecision), PlanError> {
    cap::settle(fused, baseline, ledger)
}

/// How a batch ended, and what only a measured run has.
pub(crate) enum Outcome<'r> {
    Measured {
        /// `render_timings_of_roots` of the chosen run.
        tree: &'r str,
        /// The chosen run's `load_us + execute_us`.
        measured_us: u64,
        /// `run_section` of the chosen run, `--- run ---` first.
        run: &'r str,
    },
    OutOfMemory {
        reason: &'r str,
        /// `--- run ---` and the five provenance keys.
        run: &'r str,
    },
}

/// One batch's section of the fused format (`llm-wiki/designs/query-fuser.md`, "The fused
/// format"): `--- batch ---`, `--- merge ---`, `--- pipeline ---`, and for a measured batch its
/// tree.
/// `queries` names the roots in order; `pool_bytes` is the pool the cap was computed against.
pub(crate) fn render_fused_section(
    queries: &[String],
    fused: &Fused,
    decision: &CapDecision,
    pool_bytes: u64,
    merge: &MergeHint,
    pipeline: &PipelineHint,
    outcome: Outcome<'_>,
) -> String {
    section::render_fused_section(
        queries, fused, decision, pool_bytes, merge, pipeline, outcome,
    )
}

/// One fused section read back, or `None` for a marker or a failed batch's record. Panics on
/// a line the format does not allow, naming it.
pub(crate) fn parse_fused_section(body: &str) -> Option<FusedSection<'_>> {
    section::parse_fused_section(body)
}

/// `batch` of `dataset`'s list fused, timed and checked on the device, into
/// [`fused_results_file`] and [`fused_record_path`]. Panics unless `sf` is 40.
#[cfg(not(feature = "rust-only"))]
pub(crate) async fn benchmark_fused(dataset: &str, sf: &str, batch: &str) {
    benchmark::benchmark_fused(dataset, sf, batch).await
}

/// One table per fused file under `root` (`benchmark-results/`), for bench-rows' report page.
pub(crate) fn fused_report_tables(root: &std::path::Path) -> Vec<crate::test_support::ReportTable> {
    report::fused_report_tables(root)
}

/// Whether a run failed for want of device memory: rmm's `bad_alloc`/`out_of_memory`, or a
/// CUDA `out of memory`. Such a batch is a recorded outcome, not a failure.
pub(crate) fn is_out_of_memory(error: &RunError) -> bool {
    benchmark::is_out_of_memory(error)
}

/// `benchmark-results/<device>/fused/<dataset>.sf<sf>/tp1_single.fused.txt`.
pub(crate) fn fused_results_file(device: &str, dataset: &str, sf: &str) -> PathBuf {
    benchmark::fused_results_file(device, dataset, sf)
}

/// `calibration/fused/<dataset>.sf<sf>/<batch>.records.tsv`: a batch's clean record, one file
/// per batch since one process runs one batch, and the metrics pass joins against it.
pub(crate) fn fused_record_path(dataset: &str, sf: &str, batch: &str) -> PathBuf {
    benchmark::fused_record_path(dataset, sf, batch)
}
