//! The benchmark tree and the calibration record, read back against their own redundancy
//! with no device and no run: the trailer's arithmetic, the release build, the record's
//! preamble against what the writer produces today, the checker's refusals, and the timed
//! set against the device-enabled set.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use peacockdb_core::test_support::{
    BUILD, COLUMNS, Capture, MEASURED_RUNS, RunMeta, SKIPPED, ordered_sections, record_header,
    rows_match_the_recipes, testdata_root,
};

/// Every `.benchmark.txt` the tree holds.
///
/// An empty answer is a failure wherever this is called, not a skip: the files are
/// committed, and a check that silently examines nothing is what this suite closes.
fn benchmark_files() -> Vec<PathBuf> {
    fn walk(dir: &Path, into: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, into);
            } else if path.to_string_lossy().ends_with(".benchmark.txt") {
                into.push(path);
            }
        }
    }
    let mut files = Vec::new();
    walk(&testdata_root().join("benchmark-results"), &mut files);
    files.sort();
    assert!(
        !files.is_empty(),
        "no benchmark file under testdata/benchmark-results/ — the tree is committed, so \
         its absence is a run that never published rather than a fresh checkout"
    );
    files
}

/// The `--- run ---` trailer of one section, as `key=value` in file order.
///
/// `allocator=` carries `=` inside its value, so the split is at the first one only.
fn run_trailer(body: &str, at: &str) -> Vec<(String, String)> {
    let (_, trailer) = body
        .split_once("--- run ---\n")
        .unwrap_or_else(|| panic!("{at}: the section has no `--- run ---` trailer"));
    trailer
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let (key, value) = line
                .split_once('=')
                .unwrap_or_else(|| panic!("{at}: trailer line {line:?} is not `key=value`"));
            (key.to_string(), value.to_string())
        })
        .collect()
}

fn trailer_field(trailer: &[(String, String)], key: &str, at: &str) -> String {
    trailer
        .iter()
        .find(|(name, _)| name == key)
        .unwrap_or_else(|| panic!("{at}: the trailer has no {key}"))
        .1
        .clone()
}

fn number(text: &str, what: &str) -> u64 {
    text.parse()
        .unwrap_or_else(|_| panic!("{what}: {text:?} is not a number"))
}

/// Every `total_us` in a benchmark file is the sum of the `time_us` beside it, and the
/// trailer's `device_us` is the sum of those totals.
///
/// Checkable without a device and without a second oracle: the renderer writes both
/// numbers from one measurement, so a file where they disagree is a file it got wrong.
/// `run_us` is not summed into anything — it is the wall clock, and the gap between it and
/// `device_us` is what the run spent outside the calls.
///
/// Every entry is a number, so a total is a plain sum. A `0` is a call that opened no
/// region; a `1` is a region the clock rounded down — not the same digit.
#[test]
fn every_total_us_is_the_sum_of_the_time_us_beside_it() {
    let mut checked = 0;
    for path in benchmark_files() {
        let text = std::fs::read_to_string(&path).expect("a benchmark file");
        for (query, body) in ordered_sections(&text) {
            let at = format!("{} {query}", path.display());
            // A query declared with no mode to time it at writes a marker and no tree.
            if body.starts_with(SKIPPED) {
                continue;
            }
            let mut tree_us = 0;
            for (n, line) in body.lines().enumerate() {
                let Some(rest) = line.trim().strip_prefix("time_us=") else {
                    continue;
                };
                let (array, total) = rest
                    .split_once(" total_us=")
                    .unwrap_or_else(|| panic!("{at}:{}: no total_us on {line:?}", n + 1));
                // Every number between the brackets, in one pass: the nesting says which
                // lane a call was on and the sum does not care.
                let numbers: Vec<u64> = array
                    .split(|c| c == ',' || c == '[' || c == ']')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(|e| number(e, &format!("{at}:{}", n + 1)))
                    .collect();
                assert_eq!(
                    total,
                    numbers.iter().sum::<u64>().to_string(),
                    "{at}:{}: total_us disagrees with the {} entries beside it — {line:?}",
                    n + 1,
                    numbers.len()
                );
                tree_us += numbers.iter().sum::<u64>();
                checked += 1;
            }

            let trailer = run_trailer(&body, &at);
            assert_eq!(
                trailer_field(&trailer, "device_us", &at),
                tree_us.to_string(),
                "{at}: the trailer's device_us is not the sum of the tree's total_us"
            );
            let spread: Vec<u64> = trailer_field(&trailer, "runs", &at)
                .trim_matches(|c| c == '[' || c == ']')
                .split(',')
                .map(|e| number(e, &at))
                .collect();
            assert_eq!(
                spread.len(),
                MEASURED_RUNS,
                "{at}: the spread holds {} executions",
                spread.len()
            );
            // The reported run is the second-smallest of the spread, which is the whole
            // reason the spread is written: the file would otherwise state one number with
            // nothing saying which of the ten it is.
            let mut sorted = spread.clone();
            sorted.sort_unstable();
            assert_eq!(
                trailer_field(&trailer, "run_us", &at),
                sorted[1].to_string(),
                "{at}: run_us is not the second-smallest of {spread:?}"
            );
        }
    }
    assert!(
        checked > 0,
        "not one timing line in the committed tree: the format moved and this check is \
         reading past it"
    );
}

/// The committed trees were written by a release build.
///
/// The one condition of a benchmark run that the file itself can be asked about: a debug
/// build measures a host prologue that is not release's, and the numbers would describe
/// the compiler.
#[test]
fn every_committed_tree_reports_a_release_build() {
    for path in benchmark_files() {
        let text = std::fs::read_to_string(&path).expect("a benchmark file");
        for (query, body) in ordered_sections(&text) {
            if body.starts_with(SKIPPED) {
                continue;
            }
            let at = format!("{} {query}", path.display());
            assert_eq!(
                trailer_field(&run_trailer(&body, &at), "build", &at),
                BUILD,
                "{at}"
            );
        }
    }
}

/// The committed record's preamble is the one `record_header` writes today.
///
/// The record is read by three Python scripts and by a person, none of whom can ask this
/// source what a column means. So the preamble is the column documentation, and a preamble
/// describing a record the writer no longer produces is worse than none: every line of it
/// still reads as authoritative.
///
/// `allocator=` is taken from the file, since it names the host that measured and no build
/// can know it. `capture=none` is not: a captured run's rows are the distorted ones, and
/// the committed record must not be one.
#[test]
fn the_records_preamble_is_what_record_header_writes() {
    let path = testdata_root().join("calibration/records.tsv");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{}: {e} — the record is committed", path.display()));
    let mut found: Vec<&str> = text.lines().take_while(|l| l.starts_with('#')).collect();
    let columns = text
        .lines()
        .find(|l| !l.starts_with('#'))
        .unwrap_or_else(|| panic!("{}: no column line", path.display()));
    found.push(columns);

    let allocator = found
        .iter()
        .find_map(|l| l.strip_prefix("# run: allocator="))
        .unwrap_or_else(|| panic!("{}: no `# run: allocator=` line", path.display()));
    let meta = RunMeta {
        dataset: "",
        sf: "",
        query: "",
        mode: "",
        allocator,
        capture: Capture::None,
    };
    assert_eq!(found.join("\n"), record_header(&meta), "{}", path.display());
}

/// A row that lost a cell is refused before anything joins on it.
///
/// The one defect this record cannot survive quietly: every column is a number or a name,
/// so a row missing one still parses — each later cell moves into the column to its left,
/// and `device_us` then reads whatever `host_us` measured. Checked here rather than in the
/// benchmark binary because it needs no device.
#[test]
fn a_row_that_lost_a_cell_is_refused() {
    let at = |column: &str| COLUMNS.iter().position(|name| *name == column).unwrap();
    let mut fields = vec!["0".to_string(); COLUMNS.len()];
    fields[at("node_seq")] = "0".to_string();
    fields[at("recipe_seq")] = "0".to_string();
    let declared = BTreeMap::from([(0, BTreeSet::from([0]))]);

    let whole = [fields.join("\t")];
    assert_eq!(rows_match_the_recipes(&whole, &declared), Ok(()));

    fields.pop();
    let short = [fields.join("\t")];
    let refused = rows_match_the_recipes(&short, &declared)
        .expect_err("a row one cell short is not a row of this record");
    assert!(
        refused.contains(&format!("{} columns", COLUMNS.len())),
        "the refusal should name the width it wanted: {refused}"
    );
}

/// A bare call's row names the seq it was handed, which is never its own node's.
///
/// `result_from_handle` and `slice_handle` publish no step: the plan's recipes line for a
/// `GpuUnload` or a `GpuLimit` names no `#seq` at all. They are charged to the node that
/// made the call and carry the seq of the node whose output they were handed — below them
/// in the tree, since post-order is children first. So the pair a row of theirs carries is
/// two nodes' and the check has to say so, which is what q6 found at sf40: every case
/// panicked on the export row, one call before it could write anything.
#[test]
fn a_bare_calls_row_names_the_seq_it_was_handed() {
    let at = |column: &str| COLUMNS.iter().position(|name| *name == column).unwrap();
    let row = |node: usize, seq: usize| {
        let mut fields = vec!["0".to_string(); COLUMNS.len()];
        fields[at("node_seq")] = node.to_string();
        fields[at("recipe_seq")] = seq.to_string();
        [fields.join("\t")]
    };
    // q6's shape: a scan, a project, and an unload that publishes nothing above them.
    let declared = BTreeMap::from([
        (0, BTreeSet::from([0])),
        (1, BTreeSet::from([1])),
        (2, BTreeSet::new()),
    ]);

    assert_eq!(rows_match_the_recipes(&row(2, 1), &declared), Ok(()));

    let refused = rows_match_the_recipes(&row(2, 7), &declared)
        .expect_err("no node publishes step #7, so no handle can have come from one");
    assert!(
        refused.contains("#7"),
        "the refusal should name the step: {refused}"
    );

    let above = rows_match_the_recipes(&row(1, 0), &declared)
        .expect_err("node 1 publishes step #1, and a row of its own may not name another");
    assert!(
        above.contains("#0"),
        "the refusal should name the step: {above}"
    );
}

/// Every (query, mode) the benchmark list times is one the corpus enables on a device.
///
/// The two lists are separate on purpose — they disagree about sf — so nothing but this
/// compares them. A mode a query is not enabled at fails at plan time on the GPU host,
/// tens of minutes into a run, having measured nothing.
///
/// Read as text rather than through the inventories: `corpus_cases.inc` is expanded only
/// by the two corpus binaries, and a rust-only build links neither.
#[test]
fn every_timed_case_is_enabled_on_a_device() {
    let cases = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/common");
    let timed = read_cases(
        &cases.join("corpus_benchmark_cases.inc"),
        "corpus_query_benchmark!",
    );
    let corpus = read_cases(&cases.join("corpus_cases.inc"), "corpus_query!");
    assert!(!timed.is_empty(), "the benchmark list declares nothing");

    for args in &timed {
        let (dataset, query) = (args[0].as_str(), args[2].as_str());
        let enabled = corpus
            .iter()
            .find(|line| line[0] == dataset && line[2] == query)
            .unwrap_or_else(|| {
                panic!("{dataset}/{query} is timed and is not in the corpus at all")
            });
        // The fifth argument is the device column; `none` means the query is enabled at no
        // mode there, which reads as an empty set rather than as a mode called none.
        let on_device = modes(&enabled[4]);
        for mode in modes(&args[3]) {
            assert!(
                on_device.contains(&mode),
                "{dataset}/{query} is timed at {mode} and corpus_cases.inc enables it on a \
                 device at {on_device:?} — it would fail at plan time, having measured nothing"
            );
        }
    }
}

/// The arguments of every `name(…)` invocation in a case list, one vector per line.
fn read_cases(path: &Path, name: &str) -> Vec<Vec<String>> {
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let mut out = Vec::new();
    for line in text.lines() {
        let Some(rest) = line.trim().strip_prefix(name) else {
            continue;
        };
        let args = rest
            .strip_prefix('(')
            .unwrap_or_else(|| panic!("{}: {line:?} does not open", path.display()));
        // The last `)` rather than the first, with the tail asserted: an argument list here
        // carries no parentheses of its own, and that is what makes either end the same one.
        // A line may end in a comment naming its ticket, which is not a second invocation.
        let (args, tail) = args
            .rsplit_once(')')
            .unwrap_or_else(|| panic!("{}: {line:?} does not close", path.display()));
        let tail = tail.split_once("//").map_or(tail, |(before, _)| before);
        assert_eq!(
            tail.trim(),
            ";",
            "{}: {line:?} carries more than one invocation",
            path.display()
        );
        out.push(args.split(',').map(|a| a.trim().to_string()).collect());
    }
    out
}

/// A `mode1 | mode2` argument as a set. `none` is the empty set.
fn modes(argument: &str) -> BTreeSet<String> {
    match argument {
        "none" => BTreeSet::new(),
        named => named.split('|').map(|m| m.trim().to_string()).collect(),
    }
}
