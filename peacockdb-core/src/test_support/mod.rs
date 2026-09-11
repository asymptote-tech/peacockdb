//! The shared test harness: the testdata root, the planning modes, the golden-text reader,
//! the link-time registry, the result comparators, the corpus goldens and the corpus case.
//!
//! Behind the `test-support` feature, which the crate's dev-dependency on itself turns on —
//! so `cargo test` sees this module and `cargo build` cannot name it. Two audiences are what
//! make it a component rather than a file under `tests/`: the crate's unit tests say
//! `crate::test_support::…`, the binaries `peacockdb_core::test_support::…`, and a copy on
//! each side of that boundary is the drift #49 is about. Its API is declared here like any
//! component's, so everything below is private with `pub(crate)` items, a type that crosses
//! out is declared in this file, and no `pub` signature here names an engine type.

mod corpus;
mod corpus_golden;
#[cfg(not(feature = "rust-only"))]
mod corpus_gpu;
mod cost_model;
mod golden_text;
mod registry;
mod result_text;
mod testdata;

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

use datafusion::arrow::array::RecordBatch;

use crate::planner::{BatchSizing, PlanKnobs, SMALL_TABLE_BYTES};

// --- where the testdata is ---------------------------------------------------
//   data    = <root>/<dataset>.sf<sf>/        (parquet)
//   queries = <root>/<dataset>-queries/<query>.sql
//   goldens = <root>/goldens/<dataset>.sf<sf>/<mode>-<tier>.{cpu,cost,result}.txt,
//             one section per query, plus <mode>.plans.txt for the plan tier.

/// The one testdata root. Everything that reads the tree derives its path from here, in the
/// crate and in the binaries alike, which is what stops a second spelling of the rule.
pub fn testdata_root() -> PathBuf {
    testdata::root()
}

pub fn data_dir_for(dataset: &str, sf: &str) -> PathBuf {
    testdata_root().join(format!("{dataset}.sf{sf}"))
}

pub fn queries_dir_for(dataset: &str) -> PathBuf {
    testdata_root().join(format!("{dataset}-queries"))
}

pub fn golden_dir_for(dataset: &str, sf: &str) -> PathBuf {
    testdata_root().join(format!("goldens/{dataset}.sf{sf}"))
}

/// The small fixture the crate's own unit tests plan against.
pub fn testdata_minimal_dir() -> PathBuf {
    testdata_root().join("tpch.minimal")
}

// --- the budget a run is given ------------------------------------------------

/// Resident-memory budget tier handed to `GpuMemoryBudgetRule`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryLimit {
    /// 100 MiB — sits in the gap between the corpus' top query (tpcds q78 ≈ 135.5
    /// MB) and the next one down, so the OOM tests get a real boundary to cross.
    Micro,
    /// 2 GiB.
    Mini,
    /// 12 GiB.
    Standard,
    /// 70 GiB.
    Full,
}

impl MemoryLimit {
    pub const fn bytes(self) -> usize {
        match self {
            MemoryLimit::Micro => 100 * 1024 * 1024,
            MemoryLimit::Mini => 2 * 1024 * 1024 * 1024,
            MemoryLimit::Standard => 12 * 1024 * 1024 * 1024,
            MemoryLimit::Full => 70 * 1024 * 1024 * 1024,
        }
    }

    /// Label component of a device string.
    pub const fn label(self) -> &'static str {
        match self {
            MemoryLimit::Micro => "micro",
            MemoryLimit::Mini => "mini",
            MemoryLimit::Standard => "standard",
            MemoryLimit::Full => "full",
        }
    }

    pub fn from_label(s: &str) -> Option<Self> {
        match s {
            "micro" => Some(MemoryLimit::Micro),
            "mini" => Some(MemoryLimit::Mini),
            "standard" => Some(MemoryLimit::Standard),
            "full" => Some(MemoryLimit::Full),
            _ => None,
        }
    }
}

/// The budget a device run is given, and the one `TIER` plans against: a run under a budget
/// the plan was not priced for measures a plan nobody wrote down.
pub const GPU_BUDGET: usize = MemoryLimit::Mini.bytes();

// --- the five planning modes --------------------------------------------------
// One table, because four tiers plan the same five shapes: the plan goldens, the end-to-end
// tier and the two corpus binaries. A second copy checked against this one is not the same
// thing — an agreement test is opt-in per copy, so the next tier that spells the modes out
// needs someone to remember to write one, and nothing reddens if they do not.

/// The tier every mode is planned at. The plan goldens are written here, so a failure
/// anywhere reads against a committed plan rather than a shape nothing records. The
/// execution goldens carry its label in their names, so the budget and the filename cannot
/// name different tiers.
pub const TIER: MemoryLimit = MemoryLimit::Mini;
pub const BUDGET: u64 = TIER.bytes() as u64;

/// One planning mode: what the goldens call it, and the two knobs that make it distinct.
pub struct Mode {
    /// The golden's spelling, `tp4-sized`.
    pub name: &'static str,
    pub target_partitions: usize,
    pub(crate) sizing: BatchSizing,
}

impl Mode {
    /// The macro's spelling of the same mode, `tp4_sized` — one derivation rather than
    /// a second field, so the two cannot disagree.
    pub fn ident(&self) -> String {
        self.name.replace('-', "_")
    }

    pub(crate) fn knobs(&self) -> PlanKnobs {
        PlanKnobs {
            target_partitions: self.target_partitions,
            sizing: self.sizing,
            budget: BUDGET,
            small_table_bytes: SMALL_TABLE_BYTES,
        }
    }
}

/// The five, in the fixed sequence the widget and the `.result.txt` authority both read:
/// the last enabled one wins in each. One lane and one batch is the degenerate end,
/// row-group granularity is the finest the mapping expresses, and the sized mode is the
/// only one a budget moves.
pub const MODES: [Mode; 5] = [
    Mode {
        name: "tp1-single",
        target_partitions: 1,
        sizing: BatchSizing::OneBatchPerLane,
    },
    Mode {
        name: "tp1-rowgroup",
        target_partitions: 1,
        sizing: BatchSizing::OneBatchPerRowGroup,
    },
    Mode {
        name: "tp4-single",
        target_partitions: 4,
        sizing: BatchSizing::OneBatchPerLane,
    },
    Mode {
        name: "tp4-rowgroup",
        target_partitions: 4,
        sizing: BatchSizing::OneBatchPerRowGroup,
    },
    Mode {
        name: "tp4-sized",
        target_partitions: 4,
        sizing: BatchSizing::Budgeted,
    },
];

/// The mode a macro's ident names. Exhaustive over the table: an unlisted ident panics
/// naming the set, rather than being routed to whichever mode a prefix reached first.
pub fn mode_named(ident: &str) -> &'static Mode {
    MODES
        .iter()
        .find(|mode| mode.ident() == ident)
        .unwrap_or_else(|| {
            let known: Vec<String> = MODES.iter().map(Mode::ident).collect();
            panic!("unknown mode '{ident}' (expected one of {known:?})")
        })
}

// --- the golden text format ---------------------------------------------------

/// A node line: its name, its depth in the tree, and every field it carries.
///
/// Indentation is the tree, two spaces per level, so `depth` is what a caller pairs a node
/// with its parent by. Fields keep file order and are borrowed from the line.
pub struct NodeLine<'a> {
    pub name: &'a str,
    pub depth: usize,
    pub fields: Vec<(&'a str, &'a str)>,
}

impl<'a> NodeLine<'a> {
    pub fn field(&self, key: &str) -> Option<&'a str> {
        self.fields
            .iter()
            .find(|(name, _)| *name == key)
            .map(|(_, value)| *value)
    }

    /// The field as a count. `None` when the field is absent; panics when it is present and
    /// not a number, since that is a renderer defect rather than a line of another kind.
    pub fn count(&self, key: &str) -> Option<u64> {
        self.field(key).map(|value| {
            value.parse().unwrap_or_else(|e| {
                panic!("{}: field `{key}={value}` is not a count: {e}", self.name)
            })
        })
    }
}

/// A node as an execution golden records it: its line, and the per-batch record beneath.
///
/// `in_rows` is nested by child and then by that child's lane; `batch_rows` and
/// `batch_bytes` by this node's lane and then by batch. `abandoned` is per lane and
/// present only where a run left something behind, and `rows_skipped` is a total.
pub struct RunNode<'a> {
    pub line: NodeLine<'a>,
    pub in_rows: Vec<Vec<u64>>,
    pub batch_rows: Vec<Vec<u64>>,
    pub batch_bytes: Vec<Vec<u64>>,
    pub abandoned: Vec<u64>,
    pub rows_skipped: u64,
    /// Indices into the same vector. Depth is the tree, so the parent of a node is the
    /// nearest one above it at one less.
    pub children: Vec<usize>,
}

/// Parse one line of a golden as a node line, or `None` for a line of any other kind.
pub fn parse_node_line(line: &str) -> Option<NodeLine<'_>> {
    golden_text::parse_node_line(line)
}

/// The `== <query>` sections of a golden, in file order.
pub fn ordered_sections(text: &str) -> Vec<(String, String)> {
    golden_text::ordered_sections(text)
}

/// Every section where the two texts disagree, one line per section.
pub fn section_differences(canonical: &str, actual: &str) -> Vec<String> {
    golden_text::section_differences(canonical, actual)
}

/// The first line two texts differ on, with a window around the differing column.
pub fn line_difference(expected: &str, actual: &str) -> String {
    golden_text::line_difference(expected, actual)
}

/// One `.cpu.txt` section: the early-exit marker and every node under it, in file order.
pub fn parse_run_section(body: &str) -> (String, Vec<RunNode<'_>>) {
    golden_text::parse_run_section(body)
}

// --- the link-time registry ----------------------------------------------------

/// One test-macro invocation, submitted at the invocation site.
///
/// `kind` + `device` determine the CSV column; keeping them separate rather than baking the
/// column name into each macro means the mapping lives in exactly one place.
#[derive(Debug)]
pub struct RegistryEntry {
    /// "cpu" | "gpu" — which engine ran the case.
    pub kind: &'static str,
    pub dataset: &'static str,
    pub sf: &'static str,
    /// Underscore form, as written in the macro (`shuffle_stddev`, `q12`).
    pub query: &'static str,
    /// The mode's ident, as `Mode::ident` spells it (`tp4_sized`).
    pub device: &'static str,
    /// "enabled" | "skip"
    pub state: &'static str,
}

inventory::collect!(RegistryEntry);

/// One `corpus_query!` line as declared, whatever it expanded to. Separate from
/// [`RegistryEntry`], which is per enabled (query, mode): this is per QUERY, and it is
/// submitted by the `none` arm too, so a declaration with no cases is still readable.
///
/// What reads it is the pairing between the two oracles, which is a property of the line
/// rather than of a run.
#[derive(Debug)]
pub struct CorpusDeclaration {
    pub dataset: &'static str,
    pub sf: &'static str,
    pub query: &'static str,
    pub cpu_oracle: &'static str,
    pub gpu_oracle: &'static str,
}

inventory::collect!(CorpusDeclaration);

/// A parsed CSV row.
#[derive(Debug, Clone)]
pub struct CsvRow {
    pub dataset: String,
    pub sf: String,
    pub query: String,
    /// column -> state
    pub states: BTreeMap<String, String>,
    /// "ok" | "fail" — whether create_physical_plan succeeds for this query.
    pub plan_status: String,
    pub features: Vec<String>,
    pub tickets: Vec<String>,
}

/// The spelling everything but the CSV uses. The CSV's query column is a Rust identifier —
/// the macro takes it as one — so a query is `scan_limit` there and `scan-limit` in every
/// golden section, query file and case name. The two are compared in enough places that a
/// hyphenated query matching no row reads as absent rather than as wrong: `authoritative_mode`
/// returned None for the whole of T19's first batch and nothing went red. This is the one
/// conversion, called at every point the two names meet.
pub fn stem(query: &str) -> String {
    registry::stem(query)
}

/// Parse the committed registry CSV.
pub fn load_csv() -> Vec<CsvRow> {
    registry::load_csv()
}

/// Assert this binary's inventory agrees with the CSV, in BOTH directions, for the columns
/// it owns. `inventory` collects per linked binary, so no single test can see them all.
pub fn assert_registry_matches_csv(owned_columns: &[&str], elsewhere: &[(&str, &str, &str, &str)]) {
    registry::assert_registry_matches_csv(owned_columns, elsewhere)
}

// --- comparing an answer --------------------------------------------------------

/// An answer as what a comparison needs of it: its schema, and its rows' digests sorted.
/// Eight bytes a row, which is what makes it safe to hold — an answer held whole is the
/// thing the comparators exist to stop materializing.
#[derive(PartialEq, Eq, Clone)]
pub struct ResultDigest {
    schema: u64,
    rows: Vec<u64>,
}

impl ResultDigest {
    pub fn rows(&self) -> usize {
        self.rows.len()
    }
}

pub fn total_rows(batches: &[RecordBatch]) -> usize {
    batches.iter().map(|b| b.num_rows()).sum()
}

/// Every row rendered, one string each, in batch order.
pub fn rendered_rows(batches: &[RecordBatch]) -> Vec<String> {
    result_text::rendered_rows(batches)
}

pub fn digest_of(batches: &[RecordBatch]) -> ResultDigest {
    result_text::digest_of(batches)
}

/// Whether the two answers are the same multiset of rows under the same schema.
pub fn results_agree(expected: &[RecordBatch], actual: &[RecordBatch]) -> bool {
    result_text::results_agree(expected, actual)
}

/// What a failure prints: the first row the two disagree on, with a few either side.
pub fn first_difference(expected: &[RecordBatch], actual: &[RecordBatch]) -> String {
    result_text::first_difference(expected, actual)
}

/// The answer rendered as a table with its data rows sorted, so two runs that emit the
/// same rows in different orders compare equal.
pub fn batches_to_sorted_str(batches: &[RecordBatch]) -> String {
    result_text::batches_to_sorted_str(batches)
}

/// A lower bound on the rendered size, stopping the moment it passes `cap`.
pub fn exceeds_rendered_size(batches: &[RecordBatch], cap: usize) -> bool {
    result_text::exceeds_rendered_size(batches, cap)
}

/// Order-independent result comparison with an OPTIONAL relative tolerance on
/// `Float64` columns.
///
/// - `rel_tol = None`: exact sorted-string equality (the default).
/// - `rel_tol = Some(tol)`: rows are grouped by their NON-float columns
///   (formatted) and every `Float64` cell must agree within `tol` relative error.
///   Used only where the sole divergence from the DataFusion oracle is float
///   summation reassociation across lanes (~1 ULP), which a run at more than one lane
///   incurs and exact-string compare cannot tolerate.
pub fn assert_results_match(
    expected: &[RecordBatch],
    actual: &[RecordBatch],
    rel_tol: Option<f64>,
    query: &str,
) {
    result_text::assert_results_match(expected, actual, rel_tol, query)
}

// --- the corpus goldens ---------------------------------------------------------

/// What a section says when it holds no content. One prefix for every such reason, so a
/// reader scanning a file sees the same word wherever a section is not a run.
pub const SKIPPED: &str = "skipped: ";

/// Whether this run writes goldens, and how much of the file it owns when it does.
///
/// `UPDATE_CANONICAL`'s contract is a whole file, which a corpus file cannot honour from
/// one case: the sections belong to different cases and a filtered run has only some of
/// them. So the whole-file form is what a full run means, and `PCK_UPDATE_SECTIONS` is the
/// filtered one — the same merge, without the pruning that a whole-file rewrite implies.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Regeneration {
    /// Verify. The default, and what CI always does.
    No,
    /// Merge this section and prune sections no declaration accounts for.
    Whole,
    /// Merge this section and leave every other byte of the file alone.
    Sections,
}

/// `<mode>-<tier>.cpu.txt` — the per-node tree of every query that ran at this mode.
pub fn cpu_golden(dataset: &str, sf: &str, mode: &str) -> PathBuf {
    corpus_golden::cpu_golden(dataset, sf, mode)
}

/// `<mode>-<tier>.cost.txt`, derived per section from the `.cpu.txt` beside it.
pub fn cost_golden(dataset: &str, sf: &str, mode: &str) -> PathBuf {
    corpus_golden::cost_golden(dataset, sf, mode)
}

/// `<tier>.result.txt` — one entry per query, keyed by the query alone.
pub fn result_golden(dataset: &str, sf: &str) -> PathBuf {
    corpus_golden::result_golden(dataset, sf)
}

/// Read this query's section, or panic naming what a reader has to do next.
pub fn section_of(path: &Path, query: &str) -> String {
    corpus_golden::section_of(path, query)
}

/// Verify one section and never write, whatever the run was asked to regenerate.
pub fn assert_section(path: &Path, query: &str, body: &str) {
    corpus_golden::assert_section(path, query, body)
}

/// Merge one section into the file under an advisory lock, and publish by rename.
pub fn merge_section(
    path: &Path,
    declared: &[(String, Option<String>)],
    query: &str,
    body: &str,
    mode: Regeneration,
) {
    corpus_golden::merge_section(path, declared, query, body, mode)
}

/// The file as it will be written: every declared query in declaration order, this one's
/// section replaced, and each of the others kept as it stands.
pub fn merged_text(
    text: &str,
    declared: &[(String, Option<String>)],
    query: &str,
    body: &str,
    mode: Regeneration,
) -> String {
    corpus_golden::merged_text(text, declared, query, body, mode)
}

// --- the cost model -------------------------------------------------------------

/// One cost category: where its bytes come from and how they are weighted.
pub struct Category {
    pub name: String,
    pub multiplier: f64,
    /// Gpu node types binned into this category (empty = placeholder category).
    pub nodes: Vec<String>,
}

/// The parsed `cost_model.conf`, in file (= `.cost.txt` line) order.
pub struct CostModel {
    pub categories: Vec<Category>,
}

impl CostModel {
    /// Load + parse `cost_model.conf` from under the testdata root.
    pub fn load() -> CostModel {
        cost_model::load()
    }

    /// Derive the `.cost.txt` body from a `.cpu.txt` body; `ctx` names the case in a panic.
    pub fn cost_text_from_cpu(&self, cpu_text: &str, ctx: &str) -> String {
        cost_model::cost_text_from_cpu(self, cpu_text, ctx)
    }

    /// The same derivation over a `.cpu.txt` holding every query in `== <query>` sections.
    pub fn cost_text_from_sections(&self, cpu_text: &str, ctx: &str) -> String {
        cost_model::cost_text_from_sections(self, cpu_text, ctx)
    }
}

// --- a corpus case ----------------------------------------------------------------
// What the two corpus binaries call, and all they call: dataset, scale factor, query, the
// mode's macro spelling and the oracle keyword, as strings. The plan, the run report and
// the backend stay behind these bodies, which is what keeps the harness a facade and not a
// rename — `no_test_support_signature_names_a_component_type` holds it there.

/// The whole of a cpu corpus case: plan, run, answer, and the three goldens. `mode` is the
/// macro's ident spelling, decoded here rather than at the call site.
pub async fn cpu_case(dataset: &str, sf: &str, query: &str, mode: &str, cpu_oracle: &str) {
    corpus::cpu_case(dataset, sf, query, mode, cpu_oracle).await
}

/// The whole of a device corpus case: plan, run on the device, then the two read-only
/// assertions — the mode's `.cpu.txt` section, and the result the declaration names.
#[cfg(not(feature = "rust-only"))]
pub async fn gpu_case(dataset: &str, sf: &str, query: &str, mode: &str, gpu_oracle: &str) {
    corpus_gpu::gpu_case(dataset, sf, query, mode, gpu_oracle).await
}

/// Which mode authors `.result.txt`: the last mode the query declares, in the fixed
/// sequence of five.
pub fn authoritative_mode(dataset: &str, sf: &str, query: &str) -> Option<&'static Mode> {
    corpus::authoritative_mode(dataset, sf, query)
}

/// Why a result has no section, and which mode decided it.
pub fn over_cap(bytes: Option<usize>, mode: &Mode) -> String {
    corpus::over_cap(bytes, mode)
}

/// `max(0, min(n, |unlimited| - m))` for `LIMIT n OFFSET m`.
pub fn wanted_rows(available: u64, skip: u64, fetch: Option<u64>) -> u64 {
    corpus::wanted_rows(available, skip, fetch)
}

/// The rows a run returned, counted — what the containment check owes the oracle.
pub fn owed_rows(batches: &[RecordBatch]) -> HashMap<String, usize> {
    corpus::owed_rows(batches)
}

/// Strike off what this slice of the unlimited answer accounts for, and record which owed
/// rows it holds at all.
pub fn take_rows(
    owed: &mut HashMap<String, usize>,
    batches: &[RecordBatch],
    held_at_all: &mut HashSet<String>,
) {
    corpus::take_rows(owed, batches, held_at_all)
}

/// The query without its trailing `LIMIT n [OFFSET m]`, and the interval it carried.
pub fn without_its_limit(sql: &str, what: &str) -> (String, u64, Option<u64>) {
    corpus::without_its_limit(sql, what)
}
