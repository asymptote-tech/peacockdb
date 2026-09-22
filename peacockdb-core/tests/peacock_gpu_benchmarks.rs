//! Benchmark run over the corpus: every query
//! [`corpus_benchmark_cases.inc`](common/corpus_benchmark_cases.inc) names, at the modes
//! it names. Asserts nothing about an answer — correctness is `test_gpu_corpus`.
//!
//! The `bench_` cases need a GPU and the sf40 dataset and take tens of minutes, so CI runs
//! the binary with `--skip bench_` and gets the assertions below; a measuring run is
//! `build-test-shadgpu.sh --build-benchmarks --run-benchmarks`, which builds it at
//! `--release`. On the host directly it is `--nocapture --test-threads=1`, and the threads
//! are not optional — cuDF and rmm share one process-wide pool.
#![cfg(not(feature = "rust-only"))]

use peacockdb_core::test_support::{BenchmarkCase, NOT_TIMED, benchmark_case, declared_for};

/// A benchmark declaration's reading: one test per enabled mode, and no test at all for
/// `none` — a query written down and deliberately not timed.
///
/// Both arms register, and that asymmetry is the point: `none` is a record rather than an
/// omission, so every mode's file carries a marker instead of an unexplained absence.
///
/// The mode arrives as its macro spelling, not a resolved `Mode`: a `&'static Mode` in
/// a generated body would need the table indexed at expansion time, and `mode_named`'s
/// panic is a better failure than a subscript.
macro_rules! corpus_query_benchmark {
    ($dataset:ident, $sf:expr, $query:ident, none) => {
        inventory::submit! {
            BenchmarkCase {
                dataset: stringify!($dataset),
                sf: stringify!($sf),
                query: stringify!($query),
                mode: NOT_TIMED,
            }
        }
    };
    ($dataset:ident, $sf:expr, $query:ident, $($mode:ident)|+) => {
        $(
            paste::paste! {
                #[tokio::test]
                async fn [<bench_ $dataset _sf $sf _ $query _ $mode>]() {
                    benchmark_case(
                        stringify!($dataset),
                        stringify!($sf),
                        &stringify!($query).replace('_', "-"),
                        stringify!($mode),
                    )
                    .await;
                }
            }
            inventory::submit! {
                BenchmarkCase {
                    dataset: stringify!($dataset),
                    sf: stringify!($sf),
                    query: stringify!($query),
                    mode: stringify!($mode),
                }
            }
        )+
    };
}

include!("common/corpus_benchmark_cases.inc");

/// The declarations reach the inventory, and the file each mode is to hold is derived from
/// them rather than from the case list read as text.
///
/// Runs on this binary because `inventory` collects per linked binary: the same assertion
/// in another target would sweep an empty list and pass for the wrong reason. It needs no
/// device — what it checks is the expansion, not a run.
#[test]
fn every_declared_mode_names_the_queries_its_file_will_hold() {
    let modes: std::collections::BTreeSet<(&str, &str, &str)> = inventory::iter::<BenchmarkCase>
        .into_iter()
        .filter(|case| case.mode != NOT_TIMED)
        .map(|case| (case.dataset, case.sf, case.mode))
        .collect();
    assert!(
        !modes.is_empty(),
        "the case list declares at least one mode"
    );

    for (dataset, sf, mode) in modes {
        let declared = declared_for(dataset, sf, mode);
        assert!(
            !declared.is_empty(),
            "{dataset}/sf{sf} at {mode} names no query, so its file would have no sections"
        );
        let mut names: Vec<&str> = declared.iter().map(|(name, _)| name.as_str()).collect();
        let before = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(
            names.len(),
            before,
            "{dataset}/sf{sf} at {mode} names a query twice, and a section cannot be two"
        );
    }
}

/// Numeric order within the prefix, so a file reads `q2` before `q10` rather than in the
/// order a linker happened to lay the declarations down.
#[test]
fn the_sections_of_a_file_are_ordered_numerically() {
    let declared = declared_for("tpch", "40", "tp1_single");
    let names: Vec<&str> = declared.iter().map(|(name, _)| name.as_str()).collect();
    assert_eq!(names, ["q6", "q19"], "6 before 19, not '19' before '6'");
}

/// Where a mode's results land: one file per (dataset, mode), stemmed by the mode.
#[test]
fn a_modes_results_go_to_one_file_per_dataset_and_mode() {
    use peacockdb_core::test_support::results_file;

    let path = results_file("tpch", "40", "tp1_single");
    assert!(
        path.ends_with("benchmark-results/tpch.sf40/tp1_single.benchmark.txt"),
        "{}",
        path.display()
    );
}

/// A run that produced one query's section leaves every other query's alone — the property
/// the whole merge exists for, asserted on the declarations this binary actually carries.
///
/// On `merged_text` rather than on the file: what is in question is the merge's semantics
/// under `Sections`, and writing into `testdata/` to ask about them would leave the tree
/// dirty for the answer.
#[test]
fn a_filtered_run_keeps_the_sections_it_did_not_produce() {
    use peacockdb_core::test_support::{Regeneration, merged_text};

    let declared = declared_for("tpch", "40", "tp1_single");
    let after_six = merged_text("", &declared, "q6", "six\n", Regeneration::Sections);
    let after_both = merged_text(
        &after_six,
        &declared,
        "q19",
        "nineteen\n",
        Regeneration::Sections,
    );
    assert!(after_both.contains("six\n"), "q6 survived q19's write");
    assert!(after_both.contains("nineteen\n"), "q19 was written");
    assert!(
        after_both.find("six").unwrap() < after_both.find("nineteen").unwrap(),
        "sections keep declaration order, not write order"
    );
}

/// The record's switch: the variable names the file, and its absence is what makes an
/// ordinary run a measurement rather than a collection.
///
/// Both branches, because only the pair has teeth: an assertion that no file appeared
/// passes just as well when the writer is broken and writes nothing ever.
///
/// Asserted on the writer rather than by running a case — what is in question is the
/// switch, and a device run to ask about it would cost minutes for one branch. The
/// variable is process-wide and this binary runs `--test-threads=1`, which is what makes
/// setting it here safe.
#[test]
fn the_record_is_written_only_when_a_path_is_named() {
    use peacockdb_core::test_support::{
        COLUMNS, Capture, RECORD_PATH_ENV, RunMeta, append_records,
    };

    let meta = RunMeta {
        dataset: "tpch",
        sf: "40",
        query: "q6",
        mode: "tp1-single",
        allocator: "none",
        capture: Capture::None,
    };
    // A row of the width the file actually holds, so what reaches the file is a record
    // line and not a fixture the reader would have to know to allow for.
    let row = vec!["0"; COLUMNS.len()].join("\t");
    let dir = std::env::temp_dir().join(format!("peacock-record-{}", std::process::id()));
    let path = dir.join("records.tsv");
    let restore = std::env::var_os(RECORD_PATH_ENV);
    let _ = std::fs::remove_dir_all(&dir);

    unsafe { std::env::remove_var(RECORD_PATH_ENV) };
    append_records(std::slice::from_ref(&row), &meta);
    assert!(
        !path.exists(),
        "no path was named, so nothing should have been written"
    );

    unsafe { std::env::set_var(RECORD_PATH_ENV, &path) };
    append_records(std::slice::from_ref(&row), &meta);
    let written = std::fs::read_to_string(&path).expect("a named path is written to");
    assert!(written.contains(&row), "the row reached the file");
    assert!(
        written.lines().any(|line| line.starts_with("# run: ")),
        "a fresh file carries the run's conditions: {written}"
    );

    let _ = std::fs::remove_dir_all(&dir);
    match restore {
        Some(path) => unsafe { std::env::set_var(RECORD_PATH_ENV, path) },
        None => unsafe { std::env::remove_var(RECORD_PATH_ENV) },
    }
}

/// The record is checked against the plan on every case; here is what that check refuses.
///
/// Both branches, for the reason the switch test above has both: an assertion that valid
/// rows pass is satisfied just as well by a checker that accepts everything, and the two
/// shapes below are exactly the ones this record can take while still looking well formed.
///
/// On literal rows rather than on a run: what is in question is the checker, and the
/// pairing it rejects is one no correct run produces.
#[test]
fn the_record_is_checked_against_what_the_plan_declares() {
    use std::collections::{BTreeMap, BTreeSet};

    use peacockdb_core::test_support::{COLUMNS, rows_match_the_recipes};

    let at = |column: &str| COLUMNS.iter().position(|name| *name == column).unwrap();
    // One execution's rows, so `run_index` is 0 throughout — a batch spanning two is its
    // own failure and is asserted last.
    let row = |node: usize, seq: u32, call: u64| {
        let mut fields = vec![String::new(); COLUMNS.len()];
        fields[at("node_seq")] = node.to_string();
        fields[at("recipe_seq")] = seq.to_string();
        fields[at("call_index")] = call.to_string();
        fields[at("run_index")] = "0".to_string();
        fields.join("\t")
    };
    let of_run = |run: u64, node: usize, seq: u32, call: u64| {
        let mut fields: Vec<String> = row(node, seq, call)
            .split('\t')
            .map(str::to_string)
            .collect();
        fields[at("run_index")] = run.to_string();
        fields.join("\t")
    };
    // Node 0 publishes one step, node 1 two — one plan node with two calls, which is why
    // `node_seq` and `recipe_seq` are separate columns in the first place.
    let declared: BTreeMap<usize, BTreeSet<u32>> =
        BTreeMap::from([(0, BTreeSet::from([0])), (1, BTreeSet::from([1, 2]))]);

    let good = [row(0, 0, 0), row(0, 0, 1), row(1, 1, 0), row(1, 2, 0)];
    assert_eq!(rows_match_the_recipes(&good, &declared), Ok(()));

    // The pre-order confusion: node 0 exists, step #1 exists, and only the pair is wrong.
    let crossed = [row(0, 1, 0)];
    assert!(
        rows_match_the_recipes(&crossed, &declared).is_err(),
        "a step belonging to another node's recipe is not this node's"
    );

    // A dropped row leaves the rest well formed and the totals merely smaller.
    let gap = [row(0, 0, 0), row(0, 0, 2)];
    assert!(
        rows_match_the_recipes(&gap, &declared).is_err(),
        "call 1 was made and never written down"
    );

    // Ten executions appended together, which is what the file holds and what this check
    // must therefore be given one at a time. Caught by `run_index` and named as that,
    // rather than surfacing as the step-called-twice it would otherwise look like.
    let two_runs = [row(0, 0, 0), of_run(1, 0, 0, 0)];
    let mixed = rows_match_the_recipes(&two_runs, &declared).unwrap_err();
    assert!(
        mixed.contains("executions 0 and 1"),
        "two executions at once should be named as that, not as a repeated call: {mixed}"
    );
}

/// One environment variable held for the length of a case and restored however the case
/// ends. The refusals below are panics, and a panic that left `PEACOCK_RECORD_PATH` pointing
/// into a temp dir would send every `bench_` case after it there instead of the record.
struct EnvLoan {
    name: &'static str,
    was: Option<std::ffi::OsString>,
}

impl EnvLoan {
    fn set(name: &'static str, value: impl AsRef<std::ffi::OsStr>) -> Self {
        let was = std::env::var_os(name);
        unsafe { std::env::set_var(name, value) };
        Self { name, was }
    }
}

impl Drop for EnvLoan {
    fn drop(&mut self) {
        match self.was.take() {
            Some(value) => unsafe { std::env::set_var(self.name, value) },
            None => unsafe { std::env::remove_var(self.name) },
        }
    }
}

/// A file is one run: an append whose `# run:` lines differ from the heading already
/// there is refused rather than merged, since the microseconds would not be comparable.
#[test]
#[should_panic(expected = "was written by a different run")]
fn an_append_under_a_different_heading_is_refused() {
    use peacockdb_core::test_support::{
        COLUMNS, Capture, RECORD_PATH_ENV, RunMeta, append_records,
    };

    let dir = std::env::temp_dir().join(format!("peacock-record-heading-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let _loan = EnvLoan::set(RECORD_PATH_ENV, dir.join("records.tsv"));
    let row = vec!["0"; COLUMNS.len()].join("\t");
    let mut meta = RunMeta {
        dataset: "tpch",
        sf: "40",
        query: "q6",
        mode: "tp1-single",
        allocator: "rmm-pool",
        capture: Capture::None,
    };
    append_records(std::slice::from_ref(&row), &meta);
    meta.allocator = "rmm-default";
    let refused = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        append_records(std::slice::from_ref(&row), &meta)
    }));
    let _ = std::fs::remove_dir_all(&dir);
    std::panic::resume_unwind(refused.expect_err("a second heading is refused"));
}

/// `PEACOCK_BENCHMARK_CAPTURE` names one of two passes or nothing; any other value is
/// refused naming the two, rather than read as a plain run whose times may be published.
#[test]
#[should_panic(expected = "names no Nsight pass")]
fn a_capture_variable_naming_no_pass_is_refused() {
    use peacockdb_core::test_support::{CAPTURE_ENV, Capture};

    let _loan = EnvLoan::set(CAPTURE_ENV, "sideways");
    let _ = Capture::from_env();
}
