//! The benchmark tree, read back against its own redundancy, with no device and no run: the
//! tree's and the trailer's arithmetic, the release build, the fields a section must carry, the
//! timed set against the device-enabled set, and the device trees' layout. Each tree against its
//! case list is `benchmark_lists`.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use super::benchmark_checks::{fused_problem, rows_problem, section_problem};
use peacockdb_core::prototypes::{fused_report_tables, parse_fused_section};
use peacockdb_core::test_support::{
    BUILD, FAILED, MODES, Mode, SKIPPED, benchmark_report_path, benchmarks_csv_path, device_slug,
    line_difference, load_csv, ordered_sections, parse_benchmark_section, render_benchmark_report,
    render_benchmarks_csv, rewrite_source_file, testdata_root, write_benchmark_report,
    write_benchmarks_csv,
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

/// `key=value` lines in order, split at the first `=`: a failed case's record, which is no
/// timed section and so not `parse_benchmark_section`'s.
fn key_values<'a>(lines: impl Iterator<Item = &'a str>, at: &str) -> Vec<(&'a str, &'a str)> {
    lines
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            line.split_once('=')
                .unwrap_or_else(|| panic!("{at}: record line {line:?} is not `key=value`"))
        })
        .collect()
}

/// Every timed section in the committed trees passes `section_problem`: its `total_us`, its
/// `device_us`, its load, execute and peak where it carries them, and its runs.
///
/// Checkable without a device and without a second oracle: the renderer writes them all from
/// one measurement, so a file where they disagree is a file it got wrong.
#[test]
fn every_total_us_is_the_sum_of_the_time_us_beside_it() {
    let mut checked = 0;
    for path in benchmark_files() {
        let text = std::fs::read_to_string(&path).expect("a benchmark file");
        for (query, body) in ordered_sections(&text) {
            // A query declared with no mode to time it at writes a marker and no tree, and a
            // failed case its record.
            let Some(section) = parse_benchmark_section(&body) else {
                continue;
            };
            if let Err(problem) = section_problem(&section, &format!("{} {query}", path.display()))
            {
                panic!("{problem}");
            }
            checked += section.nodes.len();
        }
    }
    assert!(
        checked > 0,
        "not one timing line in the committed tree: the format moved and this check is \
         reading past it"
    );
}

/// Every committed `*.fused.txt` section passes `fused_problem`; with none committed yet, there
/// is nothing to check.
#[test]
fn every_fused_section_adds_up() {
    let mut files = Vec::new();
    fn walk(dir: &Path, into: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            let path = entry.path();
            match path.is_dir() {
                true => walk(&path, into),
                false if path.to_string_lossy().ends_with(".fused.txt") => into.push(path),
                false => {}
            }
        }
    }
    walk(&testdata_root().join("benchmark-results"), &mut files);
    for path in files {
        let text = std::fs::read_to_string(&path).expect("a fused file");
        for (batch, body) in ordered_sections(&text) {
            if let Some(section) = parse_fused_section(&body) {
                let at = format!("{} {batch}", path.display());
                fused_problem(&section, &batch, &at).unwrap_or_else(|problem| panic!("{problem}"));
            }
        }
    }
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
            let Some(section) = parse_benchmark_section(&body) else {
                continue;
            };
            let at = format!("{} {query}", path.display());
            assert_eq!(section.field("build"), Some(BUILD), "{at}");
        }
    }
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

/// The benchmark case list, compiled in: no core test reads the checkout.
const CASE_LIST: &str = include_str!("../common/corpus_benchmark_cases.inc");

/// The datasets the benchmark times. Their registry rows are sf1, where correctness runs; the
/// list times the same cells at `BENCHMARK_SF`.
const BENCHMARK_DATASETS: [&str; 2] = ["tpch", "tpcds"];
const BENCHMARK_SF: &str = "40";

/// One `corpus_query_benchmark!` line's arguments as written: dataset, sf, query, modes.
pub(crate) type Case = [String; 4];

/// The compiled-in list's lines.
pub(crate) fn listed_cases() -> Vec<Case> {
    inventory::iter::<TimedDeclaration>
        .into_iter()
        .map(|case| [case.dataset, case.sf, case.query, case.modes].map(str::to_string))
        .collect()
}

/// `(dataset, query, modes)` for every tpch and tpcds row with an enabled device cell, in the
/// registry's row order, the modes in `MODES` order.
pub(crate) fn registry_cases() -> Vec<(String, String, Vec<String>)> {
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
// The benchmark corpus, one line per query, included only by
// `tests/peacock_gpu_benchmarks.rs` — a statement about timing rather than about coverage.
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
/// `UPDATE_CANONICAL=1` the source list is rewritten from the registry, published by rename, and
/// the test passes: the next build compiles the new list in.
#[test]
fn the_benchmark_cases_follow_the_registry() {
    let cases = registry_cases();
    assert!(
        !cases.is_empty(),
        "the registry enables no tpch or tpcds device cell"
    );
    let want = case_list_text(&cases);
    if std::env::var("UPDATE_CANONICAL").is_ok() {
        rewrite_source_file("tests/common/corpus_benchmark_cases.inc", &want);
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
    let mut listed = BTreeSet::new();
    for args in listed_cases() {
        for mode in modes(&args[3]) {
            listed.insert((args[0].clone(), args[1].clone(), args[2].clone(), mode));
        }
    }
    let listed: BTreeSet<(String, String, String)> = listed
        .into_iter()
        .filter(|(_, sf, _, _)| sf == BENCHMARK_SF)
        .map(|(dataset, _, query, mode)| (dataset, query, mode))
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

/// The committed page and `benchmarks.csv` are, byte for byte, what the trees under
/// `testdata/benchmark-results/` render through `parse_benchmark_section`, the main list's order
/// giving the row order: the registry's, which writes the list
/// (`the_benchmark_cases_follow_the_registry`). The fused files under it add a table each to the
/// page, through `fused_report_tables`, and nothing to the CSV. Under `UPDATE_CANONICAL=1` both
/// are rewritten and the test passes. A commit that changes a tree and not both files fails here,
/// naming the first line that moved.
#[test]
fn the_benchmark_report_matches_the_trees() {
    let order: Vec<(String, String)> = registry_cases()
        .into_iter()
        .map(|(dataset, query, _)| (dataset, query))
        .collect();
    let root = testdata_root().join("benchmark-results");
    let page = render_benchmark_report(&root, &order, &fused_report_tables(&root));
    let csv = render_benchmarks_csv(&root, &order);
    if std::env::var("UPDATE_CANONICAL").is_ok() {
        write_benchmark_report(&page);
        write_benchmarks_csv(&csv);
        return;
    }
    let rewrite = "UPDATE_CANONICAL=1 cargo test --features rust-only -p peacockdb-core --test \
                   test_corpus_goldens -- the_benchmark_report_matches_the_trees";
    for (path, want) in [
        (benchmark_report_path(), page),
        (benchmarks_csv_path(), csv),
    ] {
        let have = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("{}: {e} — write it with {rewrite}", path.display()));
        if have != want {
            panic!(
                "{} is not what the trees under testdata/benchmark-results/ render: {}. Rewrite \
                 it with {rewrite}",
                path.display(),
                line_difference(&have, &want)
            );
        }
    }
}

/// The five keys a trailer ends with, in order.
const PROVENANCE_KEYS: [&str; 5] = ["device", "driver", "cuda", "cudf", "host"];

/// The device trees written before the harness wrote provenance, whose trailers may carry
/// none of it. Only the H200's: both hosts that ran the benchmark before then, shad-gpu and
/// verda-gpu, are H200s. A tree copied or written under any other name is held to the five.
pub(crate) const PREDATES_PROVENANCE: &[&str] = &["h200"];

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
pub(crate) fn synthesized_tree(case: &str, device: &str, measured_on: Option<&str>) -> PathBuf {
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

/// Every timed section outside `PREDATES_ROWS` carries its nodes' batches and the trailer's
/// load, execute, peak, cache and prefill keys.
#[test]
fn every_section_outside_h200_carries_batches_and_the_load() {
    let root = testdata_root().join("benchmark-results");
    for path in benchmark_files() {
        let device = path.strip_prefix(&root).unwrap().iter().next().unwrap();
        let text = std::fs::read_to_string(&path).expect("a benchmark file");
        for (query, body) in ordered_sections(&text) {
            let at = format!("{} {query}", path.display());
            if let Err(problem) = rows_problem(device.to_str().unwrap(), &body, &at) {
                panic!("{problem}");
            }
        }
    }
}

/// The committed H200 section, which has none of the fields, under another device's name: an
/// old binary's output, refused naming the first field it lacks.
#[test]
fn a_section_without_batches_outside_h200_is_refused() {
    let text = std::fs::read_to_string(
        testdata_root().join("benchmark-results/h200/tpch.sf40/tp1_single.benchmark.txt"),
    )
    .expect("the committed file");
    let (_, body) = ordered_sections(&text)
        .into_iter()
        .find(|(query, _)| query == "aggregate-groupby")
        .expect("the committed aggregate-groupby section");
    let problem = rows_problem("h100-80gb-hbm3", &body, "x").expect_err("h200's bytes passed");
    assert!(problem.contains("carries no batch_rows"), "{problem}");
    assert_eq!(rows_problem("h200", &body, "x"), Ok(()));
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
            let trailer = match parse_benchmark_section(&body) {
                Some(section) => section.trailer,
                None => key_values(body.lines().skip(1), &at),
            };
            let keys: Vec<&str> = trailer.iter().map(|(key, _)| *key).collect();
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
            // The first `device=` is the one: the check above holds each of the five once.
            let (_, named) = trailer.iter().find(|(key, _)| *key == "device").unwrap();
            if device_slug(named) != device {
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
pub(crate) fn modes(argument: &str) -> BTreeSet<String> {
    match argument.trim() {
        "none" => BTreeSet::new(),
        "all_modes" => MODES.iter().map(Mode::ident).collect(),
        named => named.split('|').map(|m| m.trim().to_string()).collect(),
    }
}
