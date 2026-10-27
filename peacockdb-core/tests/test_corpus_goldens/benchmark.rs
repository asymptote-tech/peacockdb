//! The benchmark tree, read back against its own redundancy, and the calibration record's
//! format, with no device and no run: the trailer's arithmetic, the release build, the
//! record's preamble against what the writer produces today, the checker's refusals, and the
//! timed set against the device-enabled set.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::PoisonError;

use peacockdb_core::test_support::{
    BUILD, COLUMNS, Capture, FAILED, MEASURED_RUNS, MODES, Mode, Provenance, RECORD_PATH_ENV,
    RunMeta, SKIPPED, append_records, device_slug, load_csv, ordered_sections, record_header,
    rows_match_the_recipes, testdata_root,
};

/// One `corpus_query_benchmark!` line: the query and its timed modes as written.
struct TimedDeclaration {
    dataset: &'static str,
    sf: &'static str,
    query: &'static str,
    modes: &'static str,
}

inventory::collect!(TimedDeclaration);

/// One `corpus_query!` line's device modes and its schema-validation mask, as written. The
/// registry cannot serve the mask: its `schema_validation` column holds only
/// `enabled|disabled`.
struct LineDeclaration {
    dataset: &'static str,
    query: &'static str,
    gpu_modes: &'static str,
    mask: &'static str,
}

inventory::collect!(LineDeclaration);

// Both lists compiled in rather than read: a staged binary runs where the checkout is not.
macro_rules! corpus_query_benchmark {
    ($dataset:ident, $sf:expr, $query:ident, $($modes:tt)+) => {
        inventory::submit! {
            TimedDeclaration {
                dataset: stringify!($dataset),
                sf: stringify!($sf),
                query: stringify!($query),
                modes: stringify!($($modes)+),
            }
        }
    };
}

macro_rules! corpus_query {
    ($dataset:ident, $sf:expr, $query:ident, $($cpu:ident)|+, $($gpu:ident)|+,
     $duck:ident $(($($duck_arg:tt)*))?, $cpu_oracle:ident $(($($cpu_arg:tt)*))?,
     $gpu_oracle:ident $(($($gpu_arg:tt)*))?, $validation:ident $(($($vmode:tt)*))?) => {
        inventory::submit! {
            LineDeclaration {
                dataset: stringify!($dataset),
                query: stringify!($query),
                gpu_modes: stringify!($($gpu)|+),
                mask: concat!("" $(, stringify!($($vmode)*))?),
            }
        }
    };
}

include!("../common/corpus_benchmark_cases.inc");
include!("../common/corpus_cases.inc");

/// Every `.benchmark.txt` the tree holds.
///
/// An empty answer is a failure wherever this is called, not a skip: the files are
/// committed, and a check that silently examines nothing is what this suite closes.
fn benchmark_files() -> Vec<PathBuf> {
    let files = files_under(&testdata_root().join("benchmark-results"));
    assert!(
        !files.is_empty(),
        "no benchmark file under testdata/benchmark-results/ — the tree is committed, so \
         its absence is a run that never published rather than a fresh checkout"
    );
    files
}

/// Every `.benchmark.txt` under `root`, sorted.
fn files_under(root: &Path) -> Vec<PathBuf> {
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
    walk(root, &mut files);
    files.sort();
    files
}

/// The `--- run ---` trailer of one section, as `key=value` in file order.
///
/// `allocator=` carries `=` inside its value, so the split is at the first one only.
fn run_trailer(body: &str, at: &str) -> Vec<(String, String)> {
    let (_, trailer) = body
        .split_once("--- run ---\n")
        .unwrap_or_else(|| panic!("{at}: the section has no `--- run ---` trailer"));
    key_values(trailer.lines(), at)
}

/// `key=value` lines in order, split at the first `=`.
fn key_values<'a>(lines: impl Iterator<Item = &'a str>, at: &str) -> Vec<(String, String)> {
    lines
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
            // A query declared with no mode to time it at writes a marker and no tree, and a
            // failed case its record.
            if !is_timed(&body) {
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
            if !is_timed(&body) {
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

/// A record's preamble, read as its readers read it, is the one `record_header` writes.
///
/// Three Python scripts and a person read the record, and none can ask this source what a
/// column means, so the preamble is the column documentation. They take every `#` line, then
/// one column line: a note that lost its `#` ends the preamble early and fails here.
///
/// Written by the harness's own writer into a temp dir, since the measured record is not in
/// git. `allocator=` is read back, as it names the host that measured and no build can know
/// it; its value carries `=` and spaces, as a pool's does. `capture=none` is not read back: a
/// captured run's rows are the distorted ones, and a published record must not be one.
#[test]
fn the_records_preamble_is_what_record_header_writes() {
    let dir = std::env::temp_dir().join(format!("peacock-corpus-{}-record", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let path = dir.join("records.tsv");
    let provenance = Provenance {
        device: "NVIDIA H200".to_string(),
        driver: "570.172.08".to_string(),
        cuda: "12.2".to_string(),
        cudf: "25.02.02".to_string(),
        host: "gpu-host".to_string(),
    };
    let run = RunMeta {
        dataset: "tpch",
        sf: "40",
        query: "q6",
        mode: "tp1-single",
        allocator: "rmm-pool initial=1.0GiB max=1.0GiB of 2.0GiB free on a discrete device",
        capture: Capture::None,
        provenance: &provenance,
    };
    let row = vec!["0"; COLUMNS.len()].join("\t");
    let restore = std::env::var_os(RECORD_PATH_ENV);
    // SAFETY: `append_records` is the variable's only reader, and this is the only case in
    // this binary that calls it.
    unsafe { std::env::set_var(RECORD_PATH_ENV, &path) };
    let wrote = std::panic::catch_unwind(|| append_records(std::slice::from_ref(&row), &run));
    match restore {
        Some(value) => unsafe { std::env::set_var(RECORD_PATH_ENV, value) },
        None => unsafe { std::env::remove_var(RECORD_PATH_ENV) },
    }
    if let Err(panic) = wrote {
        std::panic::resume_unwind(panic);
    }
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{}: {e} — the writer wrote nothing", path.display()));
    let _ = std::fs::remove_dir_all(&dir);

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
    assert_eq!(allocator, run.allocator, "the allocator line is the run's");
    let meta = RunMeta {
        dataset: "",
        sf: "",
        query: "",
        mode: "",
        allocator,
        capture: Capture::None,
        provenance: &provenance,
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
#[test]
fn every_timed_case_is_enabled_on_a_device() {
    let timed: Vec<&TimedDeclaration> = inventory::iter::<TimedDeclaration>.into_iter().collect();
    assert!(!timed.is_empty(), "the benchmark list declares nothing");
    for case in timed {
        let (dataset, query) = (case.dataset, case.query);
        let line = inventory::iter::<LineDeclaration>
            .into_iter()
            .find(|line| line.dataset == dataset && line.query == query)
            .unwrap_or_else(|| {
                panic!("{dataset}/{query} is timed and is not in the corpus at all")
            });
        // `none` means the query is enabled at no device mode, which reads as an empty set
        // rather than as a mode called none.
        let on_device = modes(line.gpu_modes);
        for mode in modes(case.modes) {
            assert!(
                on_device.contains(&mode),
                "{dataset}/{query} is timed at {mode} and corpus_cases.inc enables it on a \
                 device at {on_device:?} — it would fail at plan time, having measured nothing"
            );
        }
    }
}

/// The datasets the benchmark times. Their registry rows are sf1, where correctness runs; the
/// list times the same cells at `BENCHMARK_SF`.
const BENCHMARK_DATASETS: [&str; 2] = ["tpch", "tpcds"];
const BENCHMARK_SF: &str = "40";

/// The case list as compiled in, for the byte comparison.
const CASE_LIST: &str = include_str!("../common/corpus_benchmark_cases.inc");

/// The case list from the package root, where cargo runs a test. Only the rewrite writes it, and
/// only a checkout asks for one; a staged binary compares what it compiled in.
const CASE_LIST_PATH: &str = "tests/common/corpus_benchmark_cases.inc";

/// `(dataset, query, modes)` for every tpch and tpcds row with an enabled device cell, in the
/// registry's row order, the modes in `MODES` order.
fn registry_cases() -> Vec<(String, String, Vec<String>)> {
    let mut cases = Vec::new();
    for dataset in BENCHMARK_DATASETS {
        for row in load_csv().into_iter().filter(|row| row.dataset == dataset) {
            let modes: Vec<String> = MODES
                .iter()
                .map(Mode::ident)
                .filter(|mode| row.states[&format!("gpu_{mode}")] == "enabled")
                .collect();
            if !modes.is_empty() {
                cases.push((row.dataset, row.query, modes));
            }
        }
    }
    cases
}

/// The case list as the registry makes it, one line per query.
fn case_list_text(cases: &[(String, String, Vec<String>)]) -> String {
    let mut text = String::from(CASE_LIST_HEADER);
    for dataset in BENCHMARK_DATASETS {
        text.push_str(&format!("\n// ── {dataset}, sf{BENCHMARK_SF} ──\n"));
        for (_, query, modes) in cases.iter().filter(|(of, _, _)| of == dataset) {
            text.push_str(&format!(
                "corpus_query_benchmark!({dataset}, {BENCHMARK_SF}, {query}, {});\n",
                modes.join(" | ")
            ));
        }
    }
    text
}

const CASE_LIST_HEADER: &str = "\
// The benchmark corpus, one line per query, timed by `tests/peacock_gpu_benchmarks.rs` — a
// statement about timing rather than about coverage. `test_corpus_goldens` compiles it in too,
// to check it.
//
//   corpus_query_benchmark!(dataset, sf, query, modes)
//
// Written from testdata/cost-registry.csv, never by hand: every tpch and tpcds row's enabled
// device cells (`gpu_*`), timed at sf40. `the_benchmark_cases_follow_the_registry` in
// `test_corpus_goldens` holds it to the registry and rewrites it under UPDATE_CANONICAL=1.
// sf40 and not sf1: at sf1 a query is mostly the host prologue, below the clock's resolution.
// build-test.md's Corpus Benchmarks section carries the rest.
";

/// The case list is, byte for byte, what the registry's enabled device cells for tpch and tpcds
/// make of it at sf40; the cells that differ, mode by mode, are named. Under
/// `UPDATE_CANONICAL=1` the list is rewritten from the registry instead, published by rename;
/// this binary compiled the old one in, so the next build is what checks it.
#[test]
fn the_benchmark_cases_follow_the_registry() {
    let cases = registry_cases();
    assert!(
        !cases.is_empty(),
        "the registry enables no tpch or tpcds device cell"
    );
    let want = case_list_text(&cases);
    let rewrite = {
        let _env = crate::ENV.lock().unwrap_or_else(PoisonError::into_inner);
        std::env::var("UPDATE_CANONICAL").is_ok()
    };
    if rewrite {
        let path = Path::new(CASE_LIST_PATH);
        let staged = path.with_extension("inc.new");
        std::fs::write(&staged, &want).unwrap_or_else(|e| {
            panic!(
                "{}: {e} — the rewrite runs under cargo test, in peacockdb-core/",
                staged.display()
            )
        });
        std::fs::rename(&staged, path).expect("the case list");
        return;
    }
    if CASE_LIST == want {
        return;
    }
    let registry: BTreeSet<(String, String, String)> = cases
        .iter()
        .flat_map(|(dataset, query, modes)| {
            modes
                .iter()
                .map(|mode| (dataset.clone(), query.clone(), mode.clone()))
        })
        .collect();
    let listed: BTreeSet<(String, String, String)> = case_list()
        .into_iter()
        .filter(|[_, sf, _, _]| *sf == BENCHMARK_SF)
        .flat_map(|[dataset, _, query, timed]| {
            modes(timed)
                .into_iter()
                .map(move |mode| (dataset.to_string(), query.to_string(), mode))
        })
        .collect();
    let unlisted: Vec<_> = registry.difference(&listed).collect();
    let unregistered: Vec<_> = listed.difference(&registry).collect();
    panic!(
        "the case list is not what the registry writes: {} cells enabled and not listed at \
         sf{BENCHMARK_SF} {:?}, {} listed and not enabled {:?}, and the rest of the bytes as \
         written. Rewrite it with UPDATE_CANONICAL=1 cargo test --features rust-only -p \
         peacockdb-core --test test_corpus_goldens -- the_benchmark_cases_follow_the_registry",
        unlisted.len(),
        unlisted.iter().take(10).collect::<Vec<_>>(),
        unregistered.len(),
        unregistered.iter().take(10).collect::<Vec<_>>(),
    );
}

/// `(dataset, sf, query, modes)` for every line of the case list, as written.
fn case_list() -> Vec<[&'static str; 4]> {
    inventory::iter::<TimedDeclaration>
        .into_iter()
        .map(|case| [case.dataset, case.sf, case.query, case.modes])
        .collect()
}

/// The `(dataset, sf, mode)` files a case list declares, each with the sections it is to hold.
type Files = BTreeMap<(String, String, String), BTreeSet<String>>;

/// The files `list`'s lines declare. A query declared `none` is a marker section in every file
/// of its dataset, as `declared_for` writes it.
fn declared_files(list: &str, cases: &[[&str; 4]]) -> Files {
    let mut files = Files::new();
    for [dataset, sf, query, timed] in cases {
        for mode in modes(timed) {
            files
                .entry((dataset.to_string(), sf.to_string(), mode))
                .or_default()
                .insert(query.replace('_', "-"));
        }
    }
    for [dataset, sf, query, _] in cases.iter().filter(|[.., timed]| timed.trim() == "none") {
        for ((of, at, _), queries) in files.iter_mut() {
            if (of.as_str(), at.as_str()) == (*dataset, *sf) {
                queries.insert(query.replace('_', "-"));
            }
        }
    }
    assert!(!files.is_empty(), "{list} declares no mode");
    files
}

/// One line of `h200/cases.inc`, the list the committed H200 tree was measured from.
struct H200Declaration {
    dataset: &'static str,
    sf: &'static str,
    query: &'static str,
    modes: &'static str,
}

inventory::collect!(H200Declaration);

// The H200's list under a macro of its own, so its lines land in their own inventory.
mod h200_list {
    macro_rules! corpus_query_benchmark {
        ($dataset:ident, $sf:expr, $query:ident, $($modes:tt)+) => {
            inventory::submit! {
                super::H200Declaration {
                    dataset: stringify!($dataset),
                    sf: stringify!($sf),
                    query: stringify!($query),
                    modes: stringify!($($modes)+),
                }
            }
        };
    }

    include!("../../../testdata/benchmark-results/h200/cases.inc");
}

/// The list a tree in `PREDATES_PROVENANCE` was measured from, compiled in. A device in that
/// list with no list here is refused, never held to another device's.
fn predates_list(device: &str) -> Files {
    match device {
        "h200" => {
            let cases: Vec<[&str; 4]> = inventory::iter::<H200Declaration>
                .into_iter()
                .map(|case| [case.dataset, case.sf, case.query, case.modes])
                .collect();
            declared_files("h200/cases.inc", &cases)
        }
        other => panic!("{other} is in PREDATES_PROVENANCE and has no list it was measured from"),
    }
}

/// Every case the list declares has its section in each device's committed tree. The exception is
/// a tree in `PREDATES_PROVENANCE`, which is held to the list it was measured from instead.
///
/// The tree accumulates across runs and nothing else compares it with the list, so a case
/// added and never measured would leave its file silently short.
#[test]
fn every_declared_case_has_a_section_in_the_tree() {
    let root = testdata_root().join("benchmark-results");
    if let Err(problem) =
        missing_case_problem(&root, &declared_files("the case list", &case_list()))
    {
        panic!("{problem}");
    }
}

/// The committed H200 file under another device's name: a new tree, so held to the whole list.
#[test]
fn a_new_device_tree_is_held_to_every_declared_case() {
    let tree = synthesized_tree(
        "short-tree",
        "h100-80gb-hbm3",
        Some("NVIDIA H100 80GB HBM3"),
    );
    let problem = missing_case_problem(&tree, &declared_files("the case list", &case_list()));
    let _ = std::fs::remove_dir_all(&tree);
    let problem = problem.expect_err("a tree of one file passed against the whole list");
    assert!(problem.contains("h100-80gb-hbm3"), "{problem}");
}

/// A file that exists and lacks one declared section: named, rather than read as whole.
#[test]
fn a_file_short_of_a_declared_section_is_refused() {
    let tree = synthesized_tree(
        "short-file",
        "h100-80gb-hbm3",
        Some("NVIDIA H100 80GB HBM3"),
    );
    let one_file = Files::from([(
        (
            "tpch".to_string(),
            "40".to_string(),
            "tp1_single".to_string(),
        ),
        BTreeSet::from(["q1".to_string(), "q2".to_string()]),
    )]);
    let problem = missing_case_problem(&tree, &one_file);
    let _ = std::fs::remove_dir_all(&tree);
    let problem = problem.expect_err("q2 is in no section of the file and the file passed");
    assert!(problem.contains(r#"no section for ["q2"]"#), "{problem}");
}

/// The first declared case a device tree under `root` has no section for. A failed case's
/// record is its section.
///
/// A tree in `PREDATES_PROVENANCE` is held to the list it was measured from, its
/// `predates_list`: its card is outside the run that would complete it to `files`.
fn missing_case_problem(root: &Path, files: &Files) -> Result<(), String> {
    for device in device_directories(root) {
        let name = device.file_name().unwrap().to_str().unwrap();
        let own;
        let held_to = match PREDATES_PROVENANCE.contains(&name) {
            true => {
                own = predates_list(name);
                &own
            }
            false => files,
        };
        for ((dataset, sf, mode), declared) in held_to {
            let path = device.join(format!("{dataset}.sf{sf}/{mode}.benchmark.txt"));
            let text = std::fs::read_to_string(&path).map_err(|e| {
                format!(
                    "{}: {e} — {dataset}/sf{sf} is declared at {mode} and was never measured \
                     on this device",
                    path.display()
                )
            })?;
            let present: BTreeSet<String> = ordered_sections(&text)
                .into_iter()
                .map(|(query, _)| query)
                .collect();
            let missing: Vec<&String> = declared.difference(&present).collect();
            if !missing.is_empty() {
                return Err(format!(
                    "{} has no section for {missing:?}, which the case list declares at {mode}",
                    path.display()
                ));
            }
        }
    }
    Ok(())
}

/// Every schema-validation mask entry against its own line's device modes.
///
/// On the declaration side because the decoder cannot see this: `is_validated_at` is asked
/// only about a mode a run actually reached, so an entry written for a cell that is off is
/// never decoded, and arms itself the day that cell is enabled. This reader sees every
/// declaration either way.
#[test]
fn every_mask_entry_is_one_of_its_lines_device_modes() {
    let lines: Vec<&LineDeclaration> = inventory::iter::<LineDeclaration>.into_iter().collect();
    assert!(!lines.is_empty(), "the corpus list declares nothing");
    let dead: Vec<String> = lines
        .iter()
        .flat_map(|line| {
            dead_mask_entries(line.gpu_modes, line.mask)
                .into_iter()
                .map(move |entry| format!("  {}/{}: {entry}", line.dataset, line.query))
        })
        .collect();
    assert!(
        dead.is_empty(),
        "a schema validation mask names a mode its own line does not run on a device, so the \
         exemption is dead until that cell is enabled and then arms itself unread:\n{}",
        dead.join("\n")
    );
}

/// The two shapes that reader has to call dead, and the three it must not.
///
/// The last three are what keep it from reading nothing at all: a mask whose entries are its
/// own modes, the same under `all_modes`, and a line with no mask, which is most of the
/// corpus.
#[test]
fn a_mask_entry_its_line_never_runs_on_a_device_is_dead() {
    assert_eq!(
        dead_mask_entries("tp1_single", "tp4_single"),
        ["tp4_single"]
    );
    assert_eq!(
        dead_mask_entries("none", "tp4_single | tp1_single"),
        ["tp4_single", "tp1_single"]
    );
    assert!(dead_mask_entries("tp1_single | tp4_single", "tp4_single").is_empty());
    assert!(dead_mask_entries("all_modes", "tp4_single | tp4_rowgroup | tp4_sized").is_empty());
    assert!(dead_mask_entries("tp1_single", "").is_empty());
}

/// The mask entries a declaration names that its own device modes do not include.
///
/// No mask has none, and a `none` device column makes every entry one: there is no cell for
/// the exemption to apply to. Whether an entry is a mode at all stays `mask_names`' check, at
/// run time, where it protects a hand-built string too.
fn dead_mask_entries(gpu_modes: &str, mask: &str) -> Vec<String> {
    if mask.trim().is_empty() {
        return Vec::new();
    }
    let on_device = modes(gpu_modes);
    mask.split('|')
        .map(|entry| entry.trim().to_string())
        .filter(|entry| !on_device.contains(entry))
        .collect()
}

/// Every directory directly under `root`, each one device's tree.
fn device_directories(root: &Path) -> Vec<PathBuf> {
    let mut devices: Vec<PathBuf> = std::fs::read_dir(root)
        .unwrap_or_else(|e| panic!("{}: {e}", root.display()))
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect();
    devices.sort();
    assert!(!devices.is_empty(), "{} holds no device", root.display());
    devices
}

/// The five keys a trailer ends with, in order.
const PROVENANCE_KEYS: [&str; 5] = ["device", "driver", "cuda", "cudf", "host"];

/// The device trees written before the harness wrote provenance, whose trailers may carry
/// none of it, and before the case list followed the registry, so each is held to the list it
/// was measured from, `predates_list`. Only the H200's: both hosts that ran the benchmark
/// before then, shad-gpu and verda-gpu, are H200s. A tree copied or written under any other
/// name is held to the five and to the whole list.
const PREDATES_PROVENANCE: &[&str] = &["h200"];

/// Every file is `<device>/<dataset>.sf<sf>/<mode>.benchmark.txt`, its device directory a
/// slug, and every timed section's trailer ends with the five provenance keys, `device=`
/// slugging to that directory; a failed case's record is its status line and the five. Only a
/// tree in `PREDATES_PROVENANCE` may have timed sections without.
///
/// The directory is what keeps two cards' runs apart: a file one level up, under a name the
/// harness would not write, or without the card it names, is a run nobody can place.
#[test]
fn every_benchmark_file_lies_under_its_device() {
    benchmark_files();
    if let Err(problem) = device_tree_problem(&testdata_root().join("benchmark-results")) {
        panic!("{problem}");
    }
}

/// One committed file under `device`/ in a temp tree, its provenance lines stripped, then the
/// five appended after each trailer's `allocator=` when `measured_on` names a card. Built from
/// the committed bytes, so the tree is well formed in every other respect.
fn synthesized_tree(case: &str, device: &str, measured_on: Option<&str>) -> PathBuf {
    let root = std::env::temp_dir().join(format!("peacock-{case}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let rel = "tpch.sf40/tp1_single.benchmark.txt";
    let text = std::fs::read_to_string(testdata_root().join("benchmark-results/h200").join(rel))
        .expect("the committed file");
    let mut out = String::new();
    for line in text.lines() {
        let key = line.split_once('=').map_or("", |(key, _)| key);
        if PROVENANCE_KEYS.contains(&key) {
            continue;
        }
        out.push_str(line);
        out.push('\n');
        if let (Some(card), true) = (measured_on, line.starts_with("allocator=")) {
            out.push_str(&format!(
                "device={card}\ndriver=580.95.05\ncuda=12.9\ncudf=25.02.02\nhost=gpu-host\n"
            ));
        }
    }
    let path = root.join(device).join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, out).unwrap();
    root
}

/// The check over a synthesized tree, the tree removed whatever it answers.
fn problem_of(root: PathBuf) -> Result<(), String> {
    let problem = device_tree_problem(&root);
    let _ = std::fs::remove_dir_all(&root);
    problem
}

/// The committed H200 sections under another device's name: well formed, and saying nothing
/// of the card the directory claims.
#[test]
fn a_tree_without_provenance_under_another_device_is_refused() {
    let problem = problem_of(synthesized_tree("no-provenance", "h100-80gb-hbm3", None))
        .expect_err("h200/'s sections under h100-80gb-hbm3/ passed");
    assert!(
        problem.contains("names no device, driver, CUDA, cuDF or host"),
        "{problem}"
    );
}

/// A trailer naming a card other than its directory's: filed under the wrong device.
#[test]
fn a_trailer_naming_another_card_than_its_directory_is_refused() {
    let tree = synthesized_tree("another-card", "h100-80gb-hbm3", Some("NVIDIA L40S"));
    let problem = problem_of(tree).expect_err("an L40S's trailers under h100-80gb-hbm3/ passed");
    assert!(problem.contains("measured on"), "{problem}");
}

/// The shape a new device's run writes: the five last, `device=` its directory's card.
#[test]
fn a_trailer_naming_its_directory_s_card_passes() {
    let tree = synthesized_tree("own-card", "h100-80gb-hbm3", Some("NVIDIA H100 80GB HBM3"));
    assert_eq!(problem_of(tree), Ok(()));
}

/// A failed case's record without the five, even under `h200/`: the harness has written the
/// five into every record since it recorded failures at all.
#[test]
fn a_failed_record_without_its_provenance_is_refused() {
    let root = std::env::temp_dir().join(format!("peacock-bare-failure-{}", std::process::id()));
    let path = root.join("h200/tpch.sf40/tp1_single.benchmark.txt");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, format!("== q1\n{FAILED}out of memory\n")).unwrap();
    let problem = problem_of(root).expect_err("a bare failure record passed");
    assert!(
        problem.contains("a failed case's record holds []"),
        "{problem}"
    );
}

/// A section holding a measured tree: neither a marker nor a failed case's record.
fn is_timed(body: &str) -> bool {
    !body.starts_with(SKIPPED) && !body.starts_with(FAILED)
}

/// The first way the tree under `root` breaks the device layout, if any.
pub(crate) fn device_tree_problem(root: &Path) -> Result<(), String> {
    for path in files_under(root) {
        let relative = path
            .strip_prefix(root)
            .expect("under the root it was found in");
        let parts: Vec<&str> = relative.iter().map(|p| p.to_str().unwrap()).collect();
        let [device, dataset_sf, _file] = parts[..] else {
            return Err(format!(
                "{}: not <device>/<dataset>.sf<sf>/<mode>.benchmark.txt",
                path.display()
            ));
        };
        if device_slug(device) != device {
            return Err(format!(
                "{}: {device:?} is not a device slug",
                path.display()
            ));
        }
        let sf = dataset_sf.split_once(".sf").map(|(_, sf)| sf);
        if !sf.is_some_and(|sf| !sf.is_empty() && sf.chars().all(|c| c.is_ascii_digit())) {
            return Err(format!(
                "{}: {dataset_sf:?} is not <dataset>.sf<sf>",
                path.display()
            ));
        }
        let text = std::fs::read_to_string(&path).expect("a benchmark file");
        for (query, body) in ordered_sections(&text) {
            if body.starts_with(SKIPPED) {
                continue;
            }
            let at = format!("{} {query}", path.display());
            let failed = body.starts_with(FAILED);
            // A failed case's record is its status line and the five, which the harness
            // wrote with provenance from the start.
            let trailer = match failed {
                true => key_values(body.lines().skip(1), &at),
                false => run_trailer(&body, &at),
            };
            let keys: Vec<&str> = trailer.iter().map(|(key, _)| key.as_str()).collect();
            if failed && keys != PROVENANCE_KEYS {
                return Err(format!(
                    "{at}: a failed case's record holds {keys:?} after its status line, not \
                     device= driver= cuda= cudf= host="
                ));
            }
            // Every provenance key the trailer holds, not the first: two of one are two
            // runs' trailers in one section.
            let held = keys
                .iter()
                .filter(|key| PROVENANCE_KEYS.contains(key))
                .count();
            if held == 0 {
                if PREDATES_PROVENANCE.contains(&device) {
                    continue;
                }
                return Err(format!(
                    "{at}: the trailer names no device, driver, CUDA, cuDF or host, and only \
                     {PREDATES_PROVENANCE:?} were written before the harness wrote them"
                ));
            }
            if held != PROVENANCE_KEYS.len() || !keys.ends_with(&PROVENANCE_KEYS) {
                return Err(format!(
                    "{at}: the trailer's keys are {keys:?}, which do not end with device= \
                     driver= cuda= cudf= host=, each once"
                ));
            }
            let named = trailer_field(&trailer, "device", &at);
            if device_slug(&named) != device {
                return Err(format!(
                    "{at}: measured on {named:?}, filed under {device}/"
                ));
            }
        }
    }
    Ok(())
}

/// A `mode1 | mode2` argument as a set. `none` is the empty set, `all_modes` the five: the
/// argument is the line's text, so the macro's sugar has to be expanded here too or
/// `all_modes` reads as a mode of that name and nothing is enabled.
fn modes(argument: &str) -> BTreeSet<String> {
    match argument.trim() {
        "none" => BTreeSet::new(),
        "all_modes" => MODES.iter().map(Mode::ident).collect(),
        named => named.split('|').map(|m| m.trim().to_string()).collect(),
    }
}
