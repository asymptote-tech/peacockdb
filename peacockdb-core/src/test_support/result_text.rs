//! Comparing and sizing a result without holding it whole.
//!
//! Decide, then materialize. The comparator renders a row, hashes it and drops the string,
//! so a green run holds eight bytes a row rather than the answer — `anti-join` is 1.2
//! million rows and 240 MB of `String` under the old exact arm, which `assert_eq!`
//! materialized on both sides before comparing a byte. The cap check is the same shape from
//! the other end: an answer that will never be written is never rendered whole.
//!
//! On a mismatch the rows are re-streamed and a bounded excerpt printed. That is not
//! politeness — a CI log drops a line past a thousand characters, so a half-gigabyte diff
//! spends the memory and produces something nobody can read.

use std::hash::{Hash, Hasher};

use datafusion::arrow::array::RecordBatch;
use datafusion::arrow::util::display::{ArrayFormatter, FormatOptions};
use datafusion::arrow::util::pretty::pretty_format_batches;

use super::ResultDigest;

/// How many rows either side of the first difference a failure prints.
const EXCERPT: usize = 3;

/// One formatter per column, built once for the batch. Arrow's is cheap but not free, and
/// the alternative is building one per cell — 1.2 million rows times sixteen columns.
fn formatters(batch: &RecordBatch) -> Vec<ArrayFormatter<'_>> {
    let options = FormatOptions::default();
    batch
        .columns()
        .iter()
        .map(|column| ArrayFormatter::try_new(column, &options).expect("a formattable column"))
        .collect()
}

/// One row rendered as its cells. Not the golden's padded form — padding is a function of
/// the whole answer, so it cannot be computed a row at a time — and the verdict is the same
/// either way: two answers agree cell for cell exactly when they agree padded.
///
/// The separator is `\u{1}`, as the tolerance arm's key in `mod.rs` already uses. A tab
/// occurs in data, so with one of those `("a\tb", "c")` and `("a", "b\tc")` render to one
/// string and hash to one digest — two different answers agreeing, on the comparison that
/// has no second opinion behind it.
fn render_row(columns: &[ArrayFormatter<'_>], row: usize, out: &mut String) {
    out.clear();
    for formatter in columns {
        out.push_str(&formatter.value(row).to_string());
        out.push('\u{1}');
    }
}

/// Every row as its cells, unpadded — one string per row and nothing else, so two answers
/// of different widths render the same logical row the same way. The padded form cannot do
/// this: its widths are a function of the whole answer, and it carries a header and borders
/// that are not rows at all.
pub(crate) fn rendered_rows(batches: &[RecordBatch]) -> Vec<String> {
    let mut rows = Vec::new();
    let mut rendered = String::new();
    for batch in batches {
        let columns = formatters(batch);
        for row in 0..batch.num_rows() {
            render_row(&columns, row, &mut rendered);
            rows.push(rendered.trim_end().to_string());
        }
    }
    rows
}

/// Every row's digest, paired with its position, sorted by digest — the same
/// order-independence the golden's sorted rendering has, at eight bytes a row.
fn row_digests(batches: &[RecordBatch]) -> Vec<(u64, usize)> {
    let mut digests = Vec::new();
    let mut rendered = String::new();
    let mut at = 0usize;
    for batch in batches {
        let columns = formatters(batch);
        for row in 0..batch.num_rows() {
            render_row(&columns, row, &mut rendered);
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            rendered.hash(&mut hasher);
            digests.push((hasher.finish(), at));
            at += 1;
        }
    }
    digests.sort_unstable();
    digests
}

/// The column NAMES as one digest, so a right answer under the wrong names is still wrong.
///
/// Names and not types, because the rendering this replaces carried exactly that: a header
/// of names and no types at all. Hashing the type would redden runs the string compare
/// passed — the device exports `Utf8` where DataFusion's oracle holds `Utf8View`, same
/// values, and every string-returning gpu case would fail on a difference the comparator
/// was never asked about. A type mismatch that matters is `columns_of`'s check, one tier up.
fn schema_digest(batches: &[RecordBatch]) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    if let Some(batch) = batches.first() {
        for field in batch.schema().fields() {
            field.name().hash(&mut hasher);
        }
    }
    hasher.finish()
}

pub(crate) fn digest_of(batches: &[RecordBatch]) -> ResultDigest {
    ResultDigest {
        schema: schema_digest(batches),
        rows: row_digests(batches)
            .into_iter()
            .map(|(digest, _)| digest)
            .collect(),
    }
}

/// Whether the two answers are the same multiset of rows under the same schema.
pub(crate) fn results_agree(expected: &[RecordBatch], actual: &[RecordBatch]) -> bool {
    digest_of(expected) == digest_of(actual)
}

/// What a failure prints: the first row the two disagree on, with a few either side, from a
/// second pass over the rows the digests named. Bounded on purpose.
pub(crate) fn first_difference(expected: &[RecordBatch], actual: &[RecordBatch]) -> String {
    if schema_digest(expected) != schema_digest(actual) {
        return format!(
            "the column names differ — expected {:?}, actual {:?}",
            columns_of(expected),
            columns_of(actual)
        );
    }
    let (want, got) = (row_digests(expected), row_digests(actual));
    let at = want
        .iter()
        .zip(got.iter())
        .position(|((want, _), (got, _))| want != got)
        .unwrap_or_else(|| want.len().min(got.len()));
    let mut said = format!(
        "{} rows expected, {} actual; first difference at sorted row {at}\n",
        want.len(),
        got.len()
    );
    let from = at.saturating_sub(EXCERPT);
    for (label, side) in [("expected", &want), ("actual", &got)] {
        let batches = match label {
            "expected" => expected,
            _ => actual,
        };
        for (offset, (_, row)) in side.iter().enumerate().skip(from).take(EXCERPT * 2 + 1) {
            said.push_str(&format!("  {label} [{offset}] {}\n", row_at(batches, *row)));
        }
    }
    said
}

fn columns_of(batches: &[RecordBatch]) -> Vec<String> {
    batches
        .first()
        .map(|batch| {
            batch
                .schema()
                .fields()
                .iter()
                .map(|field| field.name().to_string())
                .collect()
        })
        .unwrap_or_default()
}

/// One row by its position across the batches, rendered for a message.
fn row_at(batches: &[RecordBatch], mut at: usize) -> String {
    for batch in batches {
        if at < batch.num_rows() {
            let mut rendered = String::new();
            render_row(&formatters(batch), at, &mut rendered);
            return rendered.trim_end().to_string();
        }
        at -= batch.num_rows();
    }
    "(no such row)".to_string()
}

/// A lower bound on the rendered size: the cells alone, without the padding and borders the
/// table adds. Stops the moment it passes `cap`, so an answer far above it costs one row of
/// memory and no full rendering.
pub(crate) fn exceeds_rendered_size(batches: &[RecordBatch], cap: usize) -> bool {
    let mut total = 0usize;
    let mut rendered = String::new();
    for batch in batches {
        let columns = formatters(batch);
        for row in 0..batch.num_rows() {
            render_row(&columns, row, &mut rendered);
            total += rendered.len() + 1;
            if total >= cap {
                return true;
            }
        }
    }
    false
}

pub(crate) fn assert_results_match(
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
            results_agree(expected, actual),
            "result for {query} differs from oracle (exact compare)\n{}",
            first_difference(expected, actual)
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
        let mut evs = em.remove(&key).unwrap_or_else(|| {
            panic!("approx compare: actual row key absent from expected for {query}")
        });
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

/// Pretty-print batches with the data rows sorted, for order-independent compares. Unlike
/// the comparators above this renders the whole answer, so it is for the small ones: a
/// caller with a large result wants `results_agree`.
pub(crate) fn batches_to_sorted_str(batches: &[RecordBatch]) -> String {
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
