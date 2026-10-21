//! The device's answer against the section the cpu wrote, under the `gpu_oracle` a
//! `corpus_query!` line names.
//!
//! Pure: strings and batches, no device and no file, so the comparison a device cell rests
//! on is a function the rust-only tier can show failing. `corpus_gpu.rs` reads the section
//! and calls in here; the one oracle this cannot serve is `live_cpu`, which compares against
//! a cpu run at the same mode rather than against anything committed.

use std::collections::HashMap;

use datafusion::arrow::array::RecordBatch;

use super::result_text::batches_to_str;
use super::{GpuRecording, RowOrder, result_text, tight};

#[cfg(test)]
mod tests;

/// `golden(…, approx_std)`'s tolerance. Two things drift a few ULP from DataFusion's answer: the
/// Welford merge reassociates its sums against a single-partition pass, and the device's
/// `round(x, p > 0)` rounds in two steps where DataFusion rounds in one (#60).
const STDDEV_TOLERANCE: f64 = 1e-11;

/// cuDF's `version_config.hpp` as `build.rs` found it under `$CUDF_ROOT`, and empty in a build
/// that has no cuDF — a rust-only one, or a from-source one where the header is in the cmake
/// tree rather than at a path this can know.
const CUDF_VERSION_CONFIG: &str = include_str!(concat!(env!("OUT_DIR"), "/cudf-version-config.h"));

/// The cuDF version `version_config.hpp` declares, as `<major>.<minor padded to two digits>`.
///
/// The padding is the whole reason this is read rather than assembled: cuDF's `MINOR` is `2`
/// and every path, gitignore line and spec sentence in the tree says `25.02`. `None` where the
/// text declares neither field, which is what an absent cuDF header reads as.
fn cudf_version_of_config(config: &str) -> Option<String> {
    let field = |name: &str| -> Option<u32> {
        let define = format!("#define CUDF_VERSION_{name} ");
        let mut declared = config
            .lines()
            .filter_map(|line| line.trim().strip_prefix(&define))
            .map(|value| value.trim().parse::<u32>().ok());
        // Every match, and they have to agree: a field declared twice is a header this cannot
        // read, which the write path reports rather than taking whichever line came first.
        let first = declared.next()??;
        declared.all(|value| value == Some(first)).then_some(first)
    };
    Some(format!("{}.{:02}", field("MAJOR")?, field("MINOR")?))
}

/// The cuDF this binary is linked against, for the provenance line a device recording carries.
///
/// Read at build time from `$CUDF_ROOT`, not from the operator's `PCK_WRITE_GPU_RESULT` value —
/// that value is the thing the stamp exists not to trust. Not from the running library either:
/// `libcudf.so` carries no version in its soname, and the ABI's `peacock_gpu_version` answers
/// the ENGINE's `0.1.0`. verify-26.02 adds `peacock_cudf_version()`, which is the stronger
/// source this should read once it exists.
#[cfg_attr(feature = "rust-only", allow(dead_code))]
pub(crate) fn cudf_version() -> String {
    cudf_version_of_config(CUDF_VERSION_CONFIG).unwrap_or_else(|| {
        panic!(
            "cannot tell which cuDF this binary is linked against: no cudf/version_config.hpp \
             was copied out of CUDF_ROOT at build time, so a recorded device answer would \
             carry no provenance. Build with CUDF_ROOT set (scripts/cargo-cudf.sh)."
        )
    })
}

/// What a line's `gpu_oracle` holds the device's answer to: `golden(sort|no_sort,
/// exact|approx_std)` or `live_cpu`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum GpuResultMode {
    /// The committed section, one serving every mode: text for text, or within
    /// [`STDDEV_TOLERANCE`] on every numeric cell under `approx_std` — for stddev and var, and
    /// for a float `round` to `p > 0` places. `order` says whether both sides' rows are sorted
    /// first or compared row for row.
    Golden { order: RowOrder, approx_std: bool },
    /// A cpu run at the SAME mode, for a query whose section cannot serve five modes.
    LiveCpu,
}

impl GpuResultMode {
    /// The five, with the spelling each is written as below. One table:
    /// `every_oracle_variant_is_named_by_some_line` (`corpus/tests.rs`) holds it to the
    /// lines, and `gpu_result_mode` decodes a spelling through it rather than spelling the
    /// set a second time.
    pub(crate) const ALL: [GpuResultMode; 5] = [
        Self::golden(RowOrder::Sort, false),
        Self::golden(RowOrder::Sort, true),
        Self::golden(RowOrder::NoSort, false),
        Self::golden(RowOrder::NoSort, true),
        GpuResultMode::LiveCpu,
    ];

    const fn golden(order: RowOrder, approx_std: bool) -> Self {
        GpuResultMode::Golden { order, approx_std }
    }

    /// The spelling a `corpus_query!` line writes for this oracle.
    pub(crate) fn keyword(self) -> &'static str {
        match self {
            Self::Golden {
                order: RowOrder::Sort,
                approx_std: false,
            } => "golden(sort, exact)",
            Self::Golden {
                order: RowOrder::Sort,
                approx_std: true,
            } => "golden(sort, approx_std)",
            Self::Golden {
                order: RowOrder::NoSort,
                approx_std: false,
            } => "golden(no_sort, exact)",
            Self::Golden {
                order: RowOrder::NoSort,
                approx_std: true,
            } => "golden(no_sort, approx_std)",
            Self::LiveCpu => "live_cpu",
        }
    }
}

/// Map a `corpus_query!` `gpu_oracle` spelling to its [`GpuResultMode`], through
/// `stringify!`'s spacing. Exhaustive over [`GpuResultMode::ALL`]: a misspelling names the
/// accepted set rather than running under whichever oracle a fallback reached first.
pub(crate) fn gpu_result_mode(keyword: &str) -> GpuResultMode {
    GpuResultMode::ALL
        .into_iter()
        .find(|mode| tight(mode.keyword()) == tight(keyword))
        .unwrap_or_else(|| {
            let known: Vec<&str> = GpuResultMode::ALL.iter().map(|m| m.keyword()).collect();
            panic!(
                "corpus_query!: unknown gpu_oracle '{keyword}' (expected {})",
                known.join("|")
            )
        })
}

/// The order a `golden(…)` spelling compares in; `None` for `live_cpu`.
pub(crate) fn golden_row_order(keyword: &str) -> Option<RowOrder> {
    match gpu_result_mode(keyword) {
        GpuResultMode::Golden { order, .. } => Some(order),
        GpuResultMode::LiveCpu => None,
    }
}

/// What the `PCK_WRITE_GPU_RESULT` value asks for, an EMPTY one reading as absent.
///
/// `build-test-shadgpu.sh` exports the variable on every cycle, empty when the operator did
/// not ask to record — the superset-env idiom. So an empty value read as a version wrote
/// `gpu-result-.txt` on every ordinary cycle, which `--pull-results` then found and brought
/// home, leaving its "nothing came home" guard unable to fire. `cpp/src/expr.cpp`'s
/// `v && v[0]` is the same rule.
pub(crate) fn gpu_recording_asked(value: Option<&str>) -> GpuRecording {
    match value {
        None | Some("") => GpuRecording::No,
        Some("1") => GpuRecording::Committed,
        Some(version) => GpuRecording::Versioned(version.to_string()),
    }
}

/// [`gpu_recording_asked`] over the process environment.
pub(crate) fn gpu_recording() -> GpuRecording {
    gpu_recording_asked(std::env::var("PCK_WRITE_GPU_RESULT").ok().as_deref())
}

/// The device's answer against `section` under `gpu_oracle`. `section` is a
/// `mini.result.txt` body, its `mode=` line included — the author is part of what is
/// compared, since the frozen section is one mode's answer serving every mode's run.
pub(crate) fn device_answer_matches(
    section: &str,
    gpu_oracle: &str,
    batches: &[RecordBatch],
) -> Result<(), String> {
    let (author, rows) = section
        .split_once('\n')
        .ok_or("a result section names the mode that wrote it, then its rows")?;
    let author = author
        .strip_prefix("mode=")
        .ok_or("a result section opens with `mode=`")?;
    let GpuResultMode::Golden { order, approx_std } = gpu_result_mode(gpu_oracle) else {
        return Err(
            "live_cpu compares against a cpu run at the same mode, not against a section"
                .to_string(),
        );
    };
    let actual = batches_to_str(batches, order);
    let actual = actual.trim_end();
    // A `no_sort` line's section is written in engine order, so a `sort` comparison sorts
    // the section's rows as it sorts the device's.
    let golden = match order {
        RowOrder::Sort => result_text::sorted_rendering(rows.trim_end()),
        RowOrder::NoSort => rows.trim_end().to_string(),
    };
    match (order, approx_std) {
        (RowOrder::Sort, false) => match actual == golden {
            true => Ok(()),
            false => Err(format!(
                "the device's answer differs from the result golden, written at {author}"
            )),
        },
        (RowOrder::Sort, true) => sorted_str_approx(&golden, actual, STDDEV_TOLERANCE),
        (RowOrder::NoSort, approx_std) => {
            let tol = approx_std.then_some(STDDEV_TOLERANCE);
            same_sequence(&golden, actual, tol)
                .map_err(|said| format!("{said}, written at {author}"))
        }
    }
}

/// The header's names and the data rows as cells.
fn table_cells(text: &str) -> (Vec<String>, Vec<Vec<String>>) {
    let lines: Vec<&str> = text.lines().collect();
    if lines.len() <= 4 {
        return (
            lines
                .get(1)
                .map(|line| result_text::split_cells(line))
                .unwrap_or_default(),
            vec![],
        );
    }
    let header = result_text::split_cells(lines[1]);
    let data = lines[3..lines.len() - 1]
        .iter()
        .map(|line| result_text::split_cells(line))
        .collect();
    (header, data)
}

/// Two renderings row for row, by position: every cell as text, or under `tol` a numeric one
/// within it. Row for row with no tie rule: on 2026-10-20 no corpus answer compared this way
/// had rows tying on its sort keys (tpch q11 does, and is a fingerprint).
fn same_sequence(golden: &str, actual: &str, tol: Option<f64>) -> Result<(), String> {
    let (golden_header, golden_rows) = table_cells(golden);
    let (actual_header, actual_rows) = table_cells(actual);
    if golden_header != actual_header {
        return Err("the result header differs from the golden's".to_string());
    }
    if golden_rows.len() != actual_rows.len() {
        return Err(format!(
            "the device answers {} rows and the golden holds {}",
            actual_rows.len(),
            golden_rows.len()
        ));
    }
    for (at, (want, got)) in golden_rows.iter().zip(&actual_rows).enumerate() {
        for (column, (w, g)) in want.iter().zip(got).enumerate() {
            let numbers = (w.parse::<f64>(), g.parse::<f64>());
            let held = match (tol, numbers) {
                _ if w == g => Ok(()),
                (Some(tol), (Ok(w), Ok(g))) => number_within(w, g, tol),
                _ => Err(String::new()),
            };
            held.map_err(|why| {
                format!("row {at}, column {column}: the golden `{w}`, the device `{g}` {why}")
            })?;
        }
    }
    Ok(())
}

/// One numeric cell against the golden's within `tol` relative, NaN settled first.
fn number_within(want: f64, got: f64, tol: f64) -> Result<(), String> {
    match result_text::nan_settles(want, got) {
        Some(true) => return Ok(()),
        Some(false) => return Err(format!("one side is NaN (golden={want}, the device={got})")),
        None => {}
    }
    let difference = (want - got).abs();
    let relative = match want != 0.0 {
        true => difference / want.abs(),
        false => difference,
    };
    match relative > tol {
        true => Err(format!(
            "cell relative difference {relative:.3e} > tolerance {tol:.0e} (golden={want}, the \
             device={got})"
        )),
        false => Ok(()),
    }
}

/// Float-tolerant comparison of two sorted renderings.
///
/// The data rows are grouped by their NON-numeric cells, so a ULP difference in a numeric
/// cell cannot reorder the sorted lines and break the pairing — the same idea as
/// `assert_results_match`'s float path. Column NAMES and the data rows as cells, never the
/// borders: Arrow sizes each column to its widest printed cell, so the ascii art encodes the
/// values this comparator exists not to compare bit for bit, and one digit more in a float
/// moved a border by a dash and failed a line before the tolerance was reached.
fn sorted_str_approx(golden: &str, actual: &str, tol: f64) -> Result<(), String> {
    // key = the non-numeric cells joined; values = the numeric cells, as f64, per row.
    fn index(rows: &[Vec<String>]) -> HashMap<String, Vec<Vec<f64>>> {
        let mut held: HashMap<String, Vec<Vec<f64>>> = HashMap::new();
        for row in rows {
            let mut key = String::new();
            let mut numbers = Vec::new();
            for cell in row {
                match cell.parse::<f64>() {
                    Ok(value) => numbers.push(value),
                    Err(_) => {
                        key.push_str(cell);
                        key.push('\u{1}');
                    }
                }
            }
            held.entry(key).or_default().push(numbers);
        }
        held
    }
    fn tuple_cmp(a: &[f64], b: &[f64]) -> std::cmp::Ordering {
        for (p, q) in a.iter().zip(b) {
            match p.partial_cmp(q) {
                Some(std::cmp::Ordering::Equal) | None => continue,
                Some(order) => return order,
            }
        }
        std::cmp::Ordering::Equal
    }

    let (golden_header, golden_rows) = table_cells(golden);
    let (actual_header, actual_rows) = table_cells(actual);
    if golden_header != actual_header {
        return Err("the result header differs from the golden's".to_string());
    }
    let (mut expected, got) = (index(&golden_rows), index(&actual_rows));
    if expected.len() != got.len() {
        return Err(format!(
            "distinct non-numeric row keys differ (golden {}, the device {})",
            expected.len(),
            got.len()
        ));
    }
    for (key, mut got) in got {
        let mut want = expected
            .remove(&key)
            .ok_or("a row key of the device's answer is absent from the golden")?;
        if want.len() != got.len() {
            return Err("row multiplicity differs for a key".to_string());
        }
        want.sort_by(|a, b| tuple_cmp(a, b));
        got.sort_by(|a, b| tuple_cmp(a, b));
        for (want, got) in want.iter().zip(&got) {
            if want.len() != got.len() {
                return Err("the numeric-cell count differs".to_string());
            }
            for (want, got) in want.iter().zip(got) {
                number_within(*want, *got, tol)?;
            }
        }
    }
    Ok(())
}
