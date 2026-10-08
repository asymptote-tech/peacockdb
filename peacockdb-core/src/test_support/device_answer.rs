//! The device's answer against the section the cpu wrote, under the `gpu_oracle` a
//! `corpus_query!` line names.
//!
//! Pure: strings and batches, no device and no file, so the comparison a device cell rests
//! on is a function the rust-only tier can show failing. `corpus_gpu.rs` reads the section
//! and calls in here; the one oracle this cannot serve is `live_cpu`, which compares against
//! a cpu run at the same mode rather than against anything committed.

use std::collections::HashMap;

use datafusion::arrow::array::RecordBatch;

use super::{GpuRecording, batches_to_sorted_str, result_text};

#[cfg(test)]
mod tests;

/// `golden_approx_std`'s tolerance: the Welford merge reassociates its sums and drifts a few
/// ULP from a single-partition pass.
const STDDEV_TOLERANCE: f64 = 1e-11;

/// What a line's `gpu_oracle` holds the device's answer to.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum GpuResultMode {
    /// The committed section, text for text — one section serving every mode.
    GoldenExact,
    /// The committed section within [`STDDEV_TOLERANCE`] on every numeric cell.
    GoldenApproxStddev,
    /// A cpu run at the SAME mode, for a query whose section cannot serve five modes.
    LiveCpu,
}

impl GpuResultMode {
    /// The three, with the keyword each is written as below. One table:
    /// `every_oracle_variant_is_named_by_some_line` (`corpus/tests.rs`) holds it to the
    /// lines, and `gpu_result_mode` decodes a keyword through it rather than spelling the
    /// set a second time.
    pub(crate) const ALL: [GpuResultMode; 3] = [
        GpuResultMode::GoldenExact,
        GpuResultMode::GoldenApproxStddev,
        GpuResultMode::LiveCpu,
    ];

    /// The keyword a `corpus_query!` line writes for this oracle.
    pub(crate) fn keyword(self) -> &'static str {
        match self {
            GpuResultMode::GoldenExact => "golden_exact",
            GpuResultMode::GoldenApproxStddev => "golden_approx_std",
            GpuResultMode::LiveCpu => "live_cpu",
        }
    }
}

/// Map a `corpus_query!` `gpu_oracle` keyword to its [`GpuResultMode`]. Exhaustive over
/// [`GpuResultMode::ALL`]: a misspelling names the accepted set rather than running under
/// whichever oracle a fallback reached first.
pub(crate) fn gpu_result_mode(keyword: &str) -> GpuResultMode {
    GpuResultMode::ALL
        .into_iter()
        .find(|mode| mode.keyword() == keyword)
        .unwrap_or_else(|| {
            let known: Vec<&str> = GpuResultMode::ALL.iter().map(|m| m.keyword()).collect();
            panic!(
                "corpus_query!: unknown gpu_oracle '{keyword}' (expected {})",
                known.join("|")
            )
        })
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
    let actual = batches_to_sorted_str(batches);
    match gpu_result_mode(gpu_oracle) {
        GpuResultMode::GoldenExact => match actual.trim_end() == rows.trim_end() {
            true => Ok(()),
            false => Err(format!(
                "the device's answer differs from the result golden, written at {author}"
            )),
        },
        GpuResultMode::GoldenApproxStddev => {
            sorted_str_approx(rows.trim_end(), actual.trim_end(), STDDEV_TOLERANCE)
        }
        GpuResultMode::LiveCpu => Err(
            "live_cpu compares against a cpu run at the same mode, not against a \
                 section"
                .to_string(),
        ),
    }
}

/// Float-tolerant comparison of two `batches_to_sorted_str` renderings.
///
/// The data rows are grouped by their NON-numeric cells, so a ULP difference in a numeric
/// cell cannot reorder the sorted lines and break the pairing — the same idea as
/// `assert_results_match`'s float path. Column NAMES and the data rows as cells, never the
/// borders: Arrow sizes each column to its widest printed cell, so the ascii art encodes the
/// values this comparator exists not to compare bit for bit, and one digit more in a float
/// moved a border by a dash and failed a line before the tolerance was reached.
fn sorted_str_approx(golden: &str, actual: &str, tol: f64) -> Result<(), String> {
    fn parse(text: &str) -> (Vec<String>, Vec<Vec<String>>) {
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

    let (golden_header, golden_rows) = parse(golden);
    let (actual_header, actual_rows) = parse(actual);
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
                match result_text::nan_settles(*want, *got) {
                    Some(true) => continue,
                    Some(false) => {
                        return Err(format!("one side is NaN (golden={want}, the device={got})"));
                    }
                    None => {}
                }
                let difference = (want - got).abs();
                let relative = match *want != 0.0 {
                    true => difference / want.abs(),
                    false => difference,
                };
                if relative > tol {
                    return Err(format!(
                        "cell relative difference {relative:.3e} > tolerance {tol:.0e} \
                         (golden={want}, the device={got})"
                    ));
                }
            }
        }
    }
    Ok(())
}
