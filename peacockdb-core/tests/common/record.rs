//! The cost-model calibration record: one line per cuDF call.
//!
//! Built from the two halves of a measurement: the driver's call log says where a call was
//! and what went into it, the device's regions say what it cost. Neither half is a row on
//! its own.
//!
//! TSV, with the run's conditions — timing mode, build, allocator, capture — in the `#`
//! heading rather than in every row. `hbm_bytes` is deliberately absent: it comes from
//! Nsight and is joined in later on the same tuple.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};

use peacockdb_core::executor::{Measured, Measurements, RunReport};
use peacockdb_core::executor::AbiCall;
use peacockdb_core::wire::{RecipePlan, Seq};

/// Env var naming the file rows are appended to. Unset ⇒ no record is written, which
/// is why every caller can emit unconditionally.
pub const RECORD_PATH_ENV: &str = "PEACOCK_RECORD_PATH";

/// Env var naming the Nsight pass a run is under. Unset ⇒ [`Capture::None`].
pub const CAPTURE_ENV: &str = "PEACOCK_BENCHMARK_CAPTURE";

/// Which Nsight pass this run is under — and therefore whether its microseconds may be
/// published.
///
/// A captured run is never the reported one: tracing and the memory counters each cost the
/// query several percent, so a capture writes the record and the nvtx ranges the capture
/// joins on, and leaves the `.benchmark.txt` tree alone. It is a property of how the run
/// was launched, and the case cannot see the nsys command line, so it is named here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Capture {
    None,
    Trace,
    Metrics,
}

impl Capture {
    /// Exhaustive on purpose: an unnamed value panics naming the two rather than falling
    /// back to `None`, which would publish a captured run's times into the committed tree
    /// with nothing saying they were measured under counters.
    pub fn from_env() -> Self {
        let Some(value) = std::env::var_os(CAPTURE_ENV) else {
            return Capture::None;
        };
        match value.to_str() {
            Some("trace") => Capture::Trace,
            Some("metrics") => Capture::Metrics,
            other => panic!(
                "{CAPTURE_ENV}={other:?} names no Nsight pass — it is `trace` or `metrics`, \
                 and unset for a run that publishes its times"
            ),
        }
    }

    /// As the heading spells it.
    pub fn name(self) -> &'static str {
        match self {
            Capture::None => "none",
            Capture::Trace => "trace",
            Capture::Metrics => "metrics",
        }
    }
}

/// One row per cuDF call, keyed by `(dataset, sf, query, plan node, recipe step, call)`.
///
/// Six coordinates because that is what identifies a call: one plan node publishes several
/// recipe steps, and a batched run drives each once per batch per lane.
///
/// `node_seq` is the post-order position, the space recipes are addressed in; the driver
/// numbers pre-order, so a writer that forgets to translate produces a plausible number
/// from the wrong order. `lane` is the driving lane. `run_index` is the seventh: derivable
/// from where `call_index` restarts, written anyway, since two counting rules can disagree.
pub const COLUMNS: &[&str] = &[
    "dataset",
    "sf",
    "query",
    "mode",
    "node_seq",
    "node_type",
    "lane",
    "recipe_seq",
    "recipe_kind",
    "call_index",
    "run_index",
    "in_rows",
    "in_bytes",
    "out_rows",
    "out_bytes",
    "host_us",
    "device_us",
];

/// What a row cannot be recovered from: which engine produced it, over what data, and
/// under what conditions.
///
/// The last two are constant across a run and go into the file's `#` heading rather than
/// into every row — see [`record_header`]. They are still part of this struct because the
/// heading is written from it. The other two conditions, [`TIMING_MODE`] and [`BUILD`],
/// are not fields at all: the harness refuses to measure under anything else.
pub struct RunMeta<'a> {
    pub dataset: &'a str,
    pub sf: &'a str,
    pub query: &'a str,
    /// The batch-partitioned planning mode, `tp4-sized` and the like. The same query
    /// at two modes is a different plan and a different set of calls.
    pub mode: &'a str,
    pub allocator: &'a str,
    pub capture: Capture,
}
/// One row per call the run made, in the order the driver made them.
///
/// `nodes` is [`nodes_as_recorded`](peacockdb_core::executor::nodes_as_recorded):
/// each node's type and post-order position, in the driver's pre-order — the order the
/// report is indexed by. The translation is the whole reason it is taken rather than
/// derived here; see the note on [`COLUMNS`].
pub fn record_rows(
    nodes: &[(&str, usize)],
    report: &RunReport,
    measured: &Measurements,
    meta: &RunMeta<'_>,
    run_index: usize,
) -> Vec<String> {
    let mut rows = Vec::new();
    for (node, (node_type, post_order)) in nodes.iter().enumerate() {
        for (lane, calls) in report.abi_calls[node].iter().enumerate() {
            for made in calls.iter().filter_map(|made| made.recorded()) {
                for call in made {
                    let cost = measured
                        .call(call.seq, call.call_index)
                        .expect("join_regions answered for every journalled call");
                    rows.push(row(meta, node_type, *post_order, lane, run_index, call, cost));
                }
            }
        }
    }
    rows
}

/// Each row carries its own call's measurement, looked up by `(seq, call_index)`.
///
/// Not the driver call's total. A driver call can address several seqs — an aggregate
/// concatenates and then merges — and the device measured each of them separately. Handing
/// every row the total would report a merge that produced one row as having produced the
/// six its whole entry did.
fn row(
    meta: &RunMeta<'_>,
    node_type: &str,
    post_order: usize,
    lane: usize,
    run_index: usize,
    call: &AbiCall,
    cost: Measured,
) -> String {
    [
        meta.dataset.to_string(),
        meta.sf.to_string(),
        meta.query.to_string(),
        meta.mode.to_string(),
        post_order.to_string(),
        node_type.to_string(),
        lane.to_string(),
        call.seq.to_string(),
        call.target.to_string(),
        call.call_index.to_string(),
        run_index.to_string(),
        call.in_rows.to_string(),
        call.in_bytes.to_string(),
        cost.out_rows.to_string(),
        cost.out_bytes.to_string(),
        cost.host_us.to_string(),
        cost.device_us.to_string(),
    ]
    .join("\t")
}

/// What the plan declares, in the record's own coordinates: the seqs each node's recipe
/// publishes, indexed by the post-order position rows carry in `node_seq`.
///
/// Read through the same two steps `--- recipes ---` renders from — `RecipePlan::get` at a
/// post-order position, then each call's target — so rows checked against this are rows
/// checked against that section. That the two readings really do line up is asserted on a
/// planned query in `test_plan_goldens`, where a plan can be built without a device.
pub fn declared_steps(recipes: &RecipePlan) -> BTreeMap<usize, BTreeSet<Seq>> {
    (0..recipes.nodes())
        .map(|node| {
            let seqs = recipes.get(node).map(|recipe| recipe.seqs()).unwrap_or_default();
            (node, seqs.into_iter().collect())
        })
        .collect()
}

/// One execution's rows against what its plan declares.
///
/// Three statements, and the row count follows rather than being counted: every row has a
/// cell per column, every row names a step its own node publishes, and a step's calls are
/// numbered `0..n` with no gap. The total is then what the plan predicts.
///
/// It exists for the three ways this record is wrong while looking right. A row that lost a
/// cell still parses: every column is a number or a name, so the cells after the gap each
/// move one left. A `node_seq` from the pre-order walk names a node that exists and pairs
/// with a seq that exists, so only the pair is wrong. And a dropped call leaves every
/// remaining row well formed. Rows of one execution, since `call_index` restarts at each.
pub fn rows_match_the_recipes(
    rows: &[String],
    declared: &BTreeMap<usize, BTreeSet<Seq>>,
) -> Result<(), String> {
    // Keyed by seq alone, as the driver's counter is: `call_index` counts a seq's calls
    // across the whole run, so a per-node tally would read a lane's share as a gap.
    let mut calls: BTreeMap<Seq, BTreeSet<u64>> = BTreeMap::new();
    let mut run: Option<u64> = None;
    for row in rows {
        let cells = row.split('\t').count();
        if cells != COLUMNS.len() {
            return Err(format!(
                "a row has {cells} cells and the record has {} columns — every later cell \
                 then names the column to its left, and each one still parses — {row:?}",
                COLUMNS.len()
            ));
        }
        let node = field(row, "node_seq")? as usize;
        let seq = field(row, "recipe_seq")? as Seq;
        let call = field(row, "call_index")?;
        let at = field(row, "run_index")?;
        match run {
            Some(first) if first != at => {
                return Err(format!("rows of executions {first} and {at} were checked together"));
            }
            _ => run = Some(at),
        }
        match declared.get(&node) {
            None => {
                return Err(format!(
                    "a row is at node {node}, and the plan has {} — {row:?}",
                    declared.len()
                ));
            }
            // A node that publishes no step of its own makes only bare calls — the export
            // and the slice, which are handed a handle and charged to the node that made
            // it. That node is below this one, post-order being children first, so what a
            // row here is held to is that some node the walk reached earlier publishes it.
            Some(seqs) if seqs.is_empty() => {
                let mut below = declared.range(..node).flat_map(|(_, published)| published);
                if !below.any(|published| *published == seq) {
                    return Err(format!(
                        "a row at node {node} publishes no step of its own and names #{seq}, \
                         which no node below it publishes — a bare call carries the seq of \
                         the node whose output it was handed, and that one is below"
                    ));
                }
            }
            Some(seqs) if !seqs.contains(&seq) => {
                return Err(format!(
                    "a row pairs node {node} with step #{seq}, whose recipe publishes {seqs:?} \
                     — the pair is what a join against `--- recipes ---` reads, and both \
                     halves of a wrong one exist"
                ));
            }
            Some(_) => {}
        }
        if !calls.entry(seq).or_default().insert(call) {
            return Err(format!("step #{seq} has two rows for call {call} — {row:?}"));
        }
    }
    for (seq, made) in &calls {
        let expected: BTreeSet<u64> = (0..made.len() as u64).collect();
        if *made != expected {
            return Err(format!(
                "step #{seq} has {} rows numbered {made:?} — a gap is a call that was made \
                 and never written down",
                made.len()
            ));
        }
    }
    Ok(())
}

/// One numeric column of a row, by the name [`COLUMNS`] gives it — so the checker reads
/// the record the way a consumer does, by column name, rather than by a position it would
/// have to be kept in step with.
fn field(row: &str, column: &str) -> Result<u64, String> {
    let at = COLUMNS
        .iter()
        .position(|name| *name == column)
        .expect("a column this module names");
    let text = row
        .split('\t')
        .nth(at)
        .ok_or_else(|| format!("a row has no {column} column — {row:?}"))?;
    text.parse()
        .map_err(|_| format!("{column} is {text:?}, which is not a number — {row:?}"))
}

const RUN_PREFIX: &str = "# run: ";

/// The only timing mode a record is written under: the harness sets `NodeTiming::Events`
/// before it plans. A literal because a reader cannot tell events from a host clock by
/// looking at the microseconds, and a run under anything else writes no record at all.
pub const TIMING_MODE: &str = "events";

/// How the harness that writes this was compiled, as the record and the tree both state
/// it. A literal for the same reason: the harness refuses a build with debug assertions
/// before it measures anything, so no other value can reach a written file.
pub const BUILD: &str = "release";

/// The conditions this run measured under, as heading lines. Constant across a run —
/// which is why they are here and not columns — but each one changes what the microseconds
/// mean, so a file mixing two of them is a file whose rows cannot be compared.
/// [`append_records`] refuses to write one.
fn run_conditions(meta: &RunMeta<'_>) -> Vec<String> {
    vec![
        format!("{RUN_PREFIX}timing_mode={TIMING_MODE}"),
        format!("{RUN_PREFIX}build={BUILD}"),
        format!("{RUN_PREFIX}allocator={}", meta.allocator),
        format!("{RUN_PREFIX}capture={}", meta.capture.name()),
    ]
}

/// The `#` preamble, written once per file. A record has to be readable without this
/// source, and every line below is one a reader would otherwise guess wrong.
pub fn record_header(meta: &RunMeta<'_>) -> String {
    format!("{HEADER_NOTES}\n{}\n{}", run_conditions(meta).join("\n"), COLUMNS.join("\t"))
}

const HEADER_NOTES: &str = "\
# peacockdb cost-model calibration record. One row per call — per
# (plan node, recipe step, call index), not per node and not per output partition: one
#   plan node publishes several recipe steps, and a batched run drives each of them once
#   per batch per lane. A call answering with several output partitions is still one row,
#   its cost summed over the regions it opened.
# A benchmark executes its plan several times and writes every measured execution, in
#   the order they ran; the .benchmark.txt beside it reports one chosen run instead. So
#   the same (query, mode, node_seq, recipe_seq, call_index) recurs once per execution,
#   told apart by run_index. Spread across executions is data.
# One file is one run. The `# run:` lines below hold what is constant across it, and each
#   of them changes what the microseconds mean — so appending a run that disagrees with
#   them is refused rather than merged.
# capture = which Nsight pass measured this, `none` for a plain run. Tracing and the
#   memory counters each cost the query several percent, so a captured run writes this
#   file and never the .benchmark.txt tree.
# mode = the batch-partitioned planning mode, `tp4-sized` and the like. The same query
#   at two modes is a different plan and a different set of calls.
# node_seq = the node's position in the tree, post-order — the same space recipe_seq is
#   in. The driver numbers nodes pre-order, so this is a translation and not the index a
#   report is walked by.
# lane = the lane the node was driven on. A scatter is driven on one and emits into four,
#   a cross-lane accumulator the other way round; a call belongs to the driving one.
# recipe_seq = the seq the call addressed in the FlatBuffers plan. Deliberately a different
#   index from node_seq: one plan node with two calls is one tree position and two fb ones.
# recipe_kind = what the call addressed: the fb node kind, `CudfAggregate{Partial}` and
#   the like, or the ABI symbol for the two calls that address no wire node — a slice and
#   an export. Redundant given the plan, and written so a row is readable without one.
# call_index = which call of this recipe_seq the session had reached, 0 for the first.
#   The number C++ counts to independently, and the key the two halves of a measurement
#   meet on.
# run_index = which execution of the case, 0 for the first measured one — the warm-up is
#   not written. Derivable from a repeat of call_index 0 and written anyway, so that a
#   file joined against this one (hbm.tsv) names the same execution by the same number
#   rather than by its own count of the same boundary.
# in_rows/in_bytes = what the caller handed over, summed over the call's input slots. For
#   a call in the middle of a node's chain, what the call before it answered with — which
#   is the only side that ever priced that handle.
# out_rows/out_bytes = what the call answered with, summed over its output partitions.
#   Priced on this side from the rows the ABI reported and the schema that output belongs
#   to; C++ prices nothing. The two calls no batch is built from — an aggregate before its
#   finalize, the concat before a merge — are priced with the node's intermediate schema.
# host_us = the steady clock across the whole call, summed over its regions. Not launch
#   cost: cuDF and rmm synchronize internally, so it follows device_us closely.
# device_us = between the region's CUDA events, on the one stream everything is issued
#   to. An interval of that stream, not a figure for the device as a whole.
# hbm_bytes is not here: it comes from Nsight and joins on the same tuple.";

/// Append this run's rows to `$PEACOCK_RECORD_PATH`, or do nothing if it is unset.
///
/// Appends rather than writes: a collection run is many queries in one process, and one
/// file per run is what a reader gets. The header goes in only when the file is created,
/// so concatenating runs stays a valid file.
pub fn append_records(rows: &[String], meta: &RunMeta<'_>) {
    let Ok(path) = std::env::var(RECORD_PATH_ENV) else { return };
    let path = PathBuf::from(path);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).ok();
    }
    let fresh = std::fs::metadata(&path).map(|m| m.len() == 0).unwrap_or(true);
    if !fresh {
        assert_run_conditions_match(&path, meta);
    }
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .unwrap_or_else(|e| panic!("cannot open {} for records: {e}", path.display()));
    let mut text = String::new();
    if fresh {
        text.push_str(&record_header(meta));
        text.push('\n');
    }
    for row in rows {
        text.push_str(row);
        text.push('\n');
    }
    f.write_all(text.as_bytes())
        .unwrap_or_else(|e| panic!("cannot append records to {}: {e}", path.display()));
}

/// Refuse to append to a file whose heading describes a different run.
///
/// This is the check the three dropped columns used to make possible. As columns, a file
/// mixing two runs was legal and only detectable afterwards by whoever thought to look;
/// as a heading written once, the mixing is what has to be caught, and here is the only
/// place it can be. Reads the heading, not the file: it stops at the first row.
fn assert_run_conditions_match(path: &Path, meta: &RunMeta<'_>) {
    let f = std::fs::File::open(path)
        .unwrap_or_else(|e| panic!("cannot read the heading of {}: {e}", path.display()));
    let found: Vec<String> = std::io::BufReader::new(f)
        .lines()
        .map_while(Result::ok)
        .take_while(|l| l.starts_with('#'))
        .filter(|l| l.starts_with(RUN_PREFIX))
        .collect();
    let want = run_conditions(meta);
    assert!(
        found == want,
        "{} was written by a different run and this one would not be comparable with \
         it.\n  file: {}\n  this run: {}\nA record file is one run: point \
         {RECORD_PATH_ENV} at a new path, or remove that one.",
        path.display(),
        found.join(" | "),
        want.join(" | "),
    );
}
