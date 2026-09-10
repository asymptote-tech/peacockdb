//! Shared test harness: where the fixtures live, how a result is rendered and compared,
//! and the golden-reading helpers every tier uses.
//!
//! Each integration-test crate includes this via `#[macro_use] mod common;`, and every
//! suite uses a subset, so dead code is fine.
#![allow(dead_code)]

pub mod memory_limit;
pub mod mode;
pub mod corpus;
pub mod corpus_golden;
#[cfg(not(feature = "rust-only"))]
pub mod corpus_gpu;
pub mod cost_model;
pub mod golden_text;
pub mod injection;
pub mod join_fixture;
pub mod rebuild;
pub mod registry;
pub mod result_text;

use std::path::PathBuf;

use datafusion::arrow::record_batch::RecordBatch;
use datafusion::arrow::util::pretty::pretty_format_batches;

use memory_limit::MemoryLimit;

/// The budget a device run is given, and the one `mode::TIER` plans against: a run
/// under a budget the plan was not priced for measures a plan nobody wrote down.
pub const GPU_BUDGET: usize = MemoryLimit::Mini.bytes();

/// Max rendered size for a committed `.result.txt` golden. Above this the golden is
/// NOT written (full-result text doesn't scale — e.g. tpch anti-join renders ~240
/// MB / 1.2M rows and trips the repo's push size guard). Large-result queries fall
/// back to the live CPU oracle in the merged GPU test.
pub const RESULT_GOLDEN_MAX_BYTES: usize = 256 * 1024;

// --- parameterized testdata layout -----------------------------------------
//   data    = <root>/<dataset>.sf<sf>/        (parquet)
//   queries = <root>/<dataset>-queries/<query>.sql
//   goldens = <root>/goldens/<dataset>.sf<sf>/<mode>-<tier>.{cpu,cost,result}.txt,
//             one section per query, plus <mode>.plans.txt for the plan tier.
// PEACOCK_TESTDATA_DIR overrides the compile-time root so a binary built on one
// machine can run on another (e.g. shad-gpu).
pub fn testdata_root() -> PathBuf {
    if let Some(d) = std::env::var_os("PEACOCK_TESTDATA_DIR") {
        return PathBuf::from(d);
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../testdata")
}

pub fn data_dir_for(dataset: &str, sf: &str) -> PathBuf {
    testdata_root().join(format!("{dataset}.sf{sf}"))
}

/// A FIXED path that stands in for the testdata root when a plan's own BYTES are the
/// thing under test.
///
/// A serialized plan legitimately embeds absolute parquet paths — the C++ side has to open
/// those files — so the bytes depend on where the repo is checked out, and a digest of them
/// would false-red in CI and on any dev box off /media/data. Substituting the path
/// afterwards does not fix it: a FlatBuffer string is [len][bytes][pad], so a different root
/// moves the length prefix, every later offset and the padding, and the result would be a
/// digest of something that is not a real buffer. So the path is held constant instead: a
/// symlink whose location is the same on every machine.
pub fn canonical_root() -> PathBuf {
    // Once per process, not once per call: the staged name below is unique per PROCESS, so
    // two threads pointing the link at the same instant collide on it — one create fails
    // EEXIST, or one remove deletes the other's before its rename.
    static LINK: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    LINK.get_or_init(point_canonical_root).clone()
}

fn point_canonical_root() -> PathBuf {
    let link = PathBuf::from("/tmp/peacock-plan-bytes-root");
    let real = testdata_root();
    // Re-pointed every run, since a stale link from another checkout would silently
    // describe the wrong tree — but swapped rather than removed and recreated. Two test
    // binaries use this path, and `cargo test` with no `--test` runs them at the same
    // time: a remove-then-create leaves a window where the path does not resolve, and the
    // other binary's parquet open fails for a reason that has nothing to do with it.
    // A rename onto the name is atomic, so the path always resolves, to one root or the
    // other — and both binaries derive the same root anyway.
    #[cfg(unix)]
    {
        let staged = link.with_extension(std::process::id().to_string());
        let _ = std::fs::remove_file(&staged);
        std::os::unix::fs::symlink(&real, &staged).unwrap_or_else(|e| {
            panic!("cannot create {} -> {}: {e}", staged.display(), real.display())
        });
        std::fs::rename(&staged, &link)
            .unwrap_or_else(|e| panic!("cannot point {} at {}: {e}", link.display(), real.display()));
    }
    link
}

/// [`canonical_root`] for one dataset.
pub fn canonical_data_dir(dataset: &str, sf: &str) -> PathBuf {
    canonical_root().join(format!("{dataset}.sf{sf}"))
}

pub fn queries_dir_for(dataset: &str) -> PathBuf {
    testdata_root().join(format!("{dataset}-queries"))
}

pub fn golden_dir_for(dataset: &str, sf: &str) -> PathBuf {
    testdata_root().join(format!("goldens/{dataset}.sf{sf}"))
}

pub fn testdata_minimal_dir() -> PathBuf {
    testdata_root().join("tpch.minimal")
}

// --- result formatting ------------------------------------------------------
/// Pretty-print batches with data rows sorted, for order-independent compares.
pub fn batches_to_sorted_str(batches: &[RecordBatch]) -> String {
    let formatted = pretty_format_batches(batches).unwrap().to_string();
    let lines: Vec<&str> = formatted.lines().collect();
    if lines.len() > 4 {
        let mut data = lines[3..lines.len() - 1].to_vec();
        data.sort_unstable();
        let mut out = lines[..3].to_vec();
        out.extend(data);
        out.push(lines[lines.len() - 1]);
        out.join("\n")
    } else {
        formatted
    }
}

pub fn total_rows(batches: &[RecordBatch]) -> usize {
    batches.iter().map(|b| b.num_rows()).sum()
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
    let Some(tol) = rel_tol else {
        // Digests rather than two rendered tables: `assert_eq!` evaluates both arguments
        // before comparing a byte, so the exact arm materialized the whole answer twice to
        // answer yes or no. The excerpt is built only where the answer is no.
        assert!(
            result_text::results_agree(expected, actual),
            "result for {query} differs from oracle (exact compare)\n{}",
            result_text::first_difference(expected, actual)
        );
        return;
    };

    use std::collections::HashMap;

    use datafusion::arrow::array::{Array, Float64Array};
    use datafusion::arrow::datatypes::DataType;
    use datafusion::arrow::util::display::{ArrayFormatter, FormatOptions};

    // key (non-float columns, formatted) -> list of the row's Float64 cells.
    fn index(batches: &[RecordBatch]) -> HashMap<String, Vec<Vec<f64>>> {
        let mut m: HashMap<String, Vec<Vec<f64>>> = HashMap::new();
        let opts = FormatOptions::default();
        for b in batches {
            let s = b.schema();
            let floats: Vec<usize> = (0..s.fields().len())
                .filter(|&i| s.field(i).data_type() == &DataType::Float64)
                .collect();
            // One formatter per keyed column, not per cell: the float columns are not
            // keyed on and are never formatted, and the rest are built once for the batch.
            let keyed: Vec<(usize, ArrayFormatter<'_>)> = (0..s.fields().len())
                .filter(|c| !floats.contains(c))
                .map(|c| (c, ArrayFormatter::try_new(b.column(c), &opts).unwrap()))
                .collect();
            for r in 0..b.num_rows() {
                let mut key = String::new();
                for (_, f) in &keyed {
                    key.push_str(&f.value(r).to_string());
                    key.push('\u{1}');
                }
                let vals = floats
                    .iter()
                    .map(|&c| {
                        let a = b.column(c).as_any().downcast_ref::<Float64Array>().unwrap();
                        if a.is_null(r) { f64::NAN } else { a.value(r) }
                    })
                    .collect();
                m.entry(key).or_default().push(vals);
            }
        }
        m
    }

    // Stable order for the float-tuples within one key group (NaN treated as equal).
    fn tuple_cmp(a: &[f64], b: &[f64]) -> std::cmp::Ordering {
        for (p, q) in a.iter().zip(b) {
            match p.partial_cmp(q) {
                Some(std::cmp::Ordering::Equal) | None => continue,
                Some(o) => return o,
            }
        }
        std::cmp::Ordering::Equal
    }

    let (mut em, am) = (index(expected), index(actual));
    assert_eq!(
        em.len(),
        am.len(),
        "approx compare: distinct non-float row keys differ for {query} (expected {}, actual {})",
        em.len(),
        am.len()
    );
    for (key, mut avs) in am {
        let mut evs = em
            .remove(&key)
            .unwrap_or_else(|| panic!("approx compare: actual row key absent from expected for {query}"));
        assert_eq!(
            evs.len(),
            avs.len(),
            "approx compare: row multiplicity differs for a key in {query}"
        );
        evs.sort_by(|a, b| tuple_cmp(a, b));
        avs.sort_by(|a, b| tuple_cmp(a, b));
        for (ev, av) in evs.iter().zip(&avs) {
            for (e, a) in ev.iter().zip(av) {
                if e.is_nan() && a.is_nan() {
                    continue;
                }
                let d = (e - a).abs();
                let rel = if *e != 0.0 { d / e.abs() } else { d };
                assert!(
                    rel <= tol,
                    "approx compare: float cell rel diff {rel:.3e} > tol {tol:.0e} for {query} (expected={e}, actual={a})"
                );
            }
        }
    }
}

/// Float-tolerant comparison of two `batches_to_sorted_str` renderings. The data
/// rows are grouped by their NON-numeric cells (so a ULP difference in a numeric
/// cell can't reorder the sorted lines and break pairing — same idea as
/// `assert_results_match`'s float path), and every numeric cell must agree within
/// `tol` relative error. Used for the result-golden approx path (q14/q39).
pub fn assert_sorted_str_approx(golden: &str, actual: &str, tol: f64, query: &str) {
    use std::collections::HashMap;

    fn split_cells(line: &str) -> Vec<String> {
        let parts: Vec<&str> = line.split('|').collect();
        if parts.len() < 2 {
            return vec![line.trim().to_string()];
        }
        parts[1..parts.len() - 1].iter().map(|c| c.trim().to_string()).collect()
    }
    // Column NAMES and the data rows as cells — never the borders. Arrow sizes each column
    // to its widest printed cell, so the ascii art encodes the values this comparator exists
    // not to compare bit-for-bit: one digit more in a float moved a border by a dash and
    // failed the test a line before the tolerance that allows it was reached.
    fn parse(s: &str) -> (Vec<String>, Vec<Vec<String>>) {
        let lines: Vec<&str> = s.lines().collect();
        if lines.len() <= 4 {
            return (lines.get(1).map(|l| split_cells(l)).unwrap_or_default(), vec![]);
        }
        let header = split_cells(lines[1]);
        let data = lines[3..lines.len() - 1].iter().map(|l| split_cells(l)).collect();
        (header, data)
    }
    // key = non-numeric cells joined; vals = the numeric cells (as f64) per row.
    fn index(rows: &[Vec<String>]) -> HashMap<String, Vec<Vec<f64>>> {
        let mut m: HashMap<String, Vec<Vec<f64>>> = HashMap::new();
        for row in rows {
            let mut key = String::new();
            let mut nums = Vec::new();
            for cell in row {
                match cell.parse::<f64>() {
                    Ok(v) => nums.push(v),
                    Err(_) => {
                        key.push_str(cell);
                        key.push('\u{1}');
                    }
                }
            }
            m.entry(key).or_default().push(nums);
        }
        m
    }
    fn tuple_cmp(a: &[f64], b: &[f64]) -> std::cmp::Ordering {
        for (p, q) in a.iter().zip(b) {
            match p.partial_cmp(q) {
                Some(std::cmp::Ordering::Equal) | None => continue,
                Some(o) => return o,
            }
        }
        std::cmp::Ordering::Equal
    }

    let (gh, gd) = parse(golden);
    let (ah, ad) = parse(actual);
    assert_eq!(gh, ah, "result header/schema for {query} differs from golden");
    let (mut gm, am) = (index(&gd), index(&ad));
    assert_eq!(
        gm.len(),
        am.len(),
        "approx result: distinct non-numeric row keys differ for {query} (golden {}, actual {})",
        gm.len(),
        am.len()
    );
    for (key, mut avs) in am {
        let mut evs = gm
            .remove(&key)
            .unwrap_or_else(|| panic!("approx result: actual row key absent from golden for {query}"));
        assert_eq!(evs.len(), avs.len(), "approx result: row multiplicity differs for a key in {query}");
        evs.sort_by(|a, b| tuple_cmp(a, b));
        avs.sort_by(|a, b| tuple_cmp(a, b));
        for (ev, av) in evs.iter().zip(&avs) {
            assert_eq!(ev.len(), av.len(), "approx result: numeric-cell count differs for {query}");
            for (e, a) in ev.iter().zip(av) {
                let d = (e - a).abs();
                let rel = if *e != 0.0 { d / e.abs() } else { d };
                assert!(
                    rel <= tol,
                    "approx result: cell rel diff {rel:.3e} > tol {tol:.0e} for {query} (golden={e}, actual={a})"
                );
            }
        }
    }
}

#[derive(Clone, Copy)]
pub enum GpuResultMode {
    GoldenExact,
    GoldenApprox,
    GoldenApproxStddev,
    LiveCpu,
    Skip,
}

/// Map a `corpus_query!` `gpu_oracle` keyword to its [`GpuResultMode`].
#[cfg(not(feature = "rust-only"))]
pub fn gpu_result_mode(s: &str) -> GpuResultMode {
    match s {
        "golden_exact" => GpuResultMode::GoldenExact,
        "golden_approx" => GpuResultMode::GoldenApprox,
        "golden_approx_std" => GpuResultMode::GoldenApproxStddev,
        "live_cpu" => GpuResultMode::LiveCpu,
        "skip" => GpuResultMode::Skip,
        other => panic!(
            "corpus_query!: unknown gpu_oracle '{other}' \
             (expected golden_exact|golden_approx|golden_approx_std|live_cpu|skip)"
        ),
    }
}
