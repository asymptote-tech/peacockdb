//! The fingerprint a result section holds instead of its rows when it is over the cap.
//!
//! Written by both result writers — this one and `testdata/duckdb_result.py` — so an answer
//! too large to commit is still an answer DuckDB can be held to. A column goes into the hash
//! when it renders identically on both engines and into an approximate triple when it does
//! not, and the class is read off the RENDERED cells rather than off either engine's
//! declared type: ours answers a decimal where DuckDB answers a double, and only the
//! rendering is common ground.
//!
//! The two passes over the rows are what keeps the memory bounded: the first classifies and
//! holds nothing, the second collects only the approximate columns' values and the exact
//! columns' text. Rendering twice costs cpu and no memory, which is the right way round for
//! a 1.2-million-row answer.

use datafusion::arrow::array::RecordBatch;
use datafusion::arrow::util::display::{ArrayFormatter, FormatOptions};
use sha2::{Digest, Sha256};

use super::{SKIPPED, result_text};

/// What a fingerprinted section opens with. First position, as `SKIPPED` keeps: a reader
/// deciding whether a section holds rows looks at the first line.
pub(crate) const FINGERPRINT: &str = "fingerprint: ";

/// One column's fingerprint: how many cells hold a value, and — where the column is
/// approximate — the triple that stands in for them.
#[derive(Debug, PartialEq)]
struct Column {
    nonnull: usize,
    /// `(sum, min, max)`, or `None` where the column is exact and in the hash instead.
    triple: Option<(f64, f64, f64)>,
}

/// A parsed fingerprint section.
#[derive(Debug)]
struct Parsed {
    rows: usize,
    columns: Vec<Column>,
    hash: String,
}

pub(crate) fn is_fingerprint(section: &str) -> bool {
    section.starts_with(FINGERPRINT)
}

/// Whether this section holds the query's rows, as against standing in for them: the
/// `skipped:` marker a query no mode enables carries, or the fingerprint an answer over the
/// cap leaves. Both keep first position for this reader — a mode line ahead of either would
/// let a `golden_exact` device case compare against nothing.
pub(crate) fn section_holds_rows(section: &str) -> bool {
    !section.starts_with(SKIPPED) && !is_fingerprint(section)
}

pub(crate) fn fingerprint_of(batches: &[RecordBatch]) -> String {
    let width = batches.first().map_or(0, RecordBatch::num_columns);
    fingerprint(width, &|visit| rows_of_batches(batches, visit))
}

/// The same fingerprint over a rendered table, for the side that stayed under the cap while
/// the other crossed it. The cells are the ones the table prints, trimmed of its padding.
pub(crate) fn fingerprint_of_rendered(table: &str) -> String {
    let width = data_lines(table)
        .next()
        .map_or(0, |line| result_text::split_cells(line).len());
    fingerprint(width, &|visit| {
        for line in data_lines(table) {
            visit(&result_text::split_cells(line));
        }
    })
}

/// A rendered table's data lines: everything between the header's border and the last one.
fn data_lines(table: &str) -> impl Iterator<Item = &str> {
    let lines: Vec<&str> = table.lines().collect();
    let body = match lines.len() > 4 {
        true => lines[3..lines.len() - 1].to_vec(),
        false => Vec::new(),
    };
    body.into_iter()
}

/// Every row of `batches` as its rendered cells, unpadded — `result_text`'s rendering, which
/// is what the golden's table prints inside its padding.
fn rows_of_batches(batches: &[RecordBatch], visit: &mut dyn FnMut(&[String])) {
    let options = FormatOptions::default();
    for batch in batches {
        let columns: Vec<ArrayFormatter<'_>> = batch
            .columns()
            .iter()
            .map(|column| ArrayFormatter::try_new(column, &options).expect("a formattable column"))
            .collect();
        let mut row: Vec<String> = vec![String::new(); columns.len()];
        for at in 0..batch.num_rows() {
            for (cell, formatter) in row.iter_mut().zip(&columns) {
                *cell = formatter.value(at).to_string();
            }
            visit(&row);
        }
    }
}

/// The section text, from two passes over the rows: classify, then collect.
fn fingerprint(width: usize, each_row: &dyn Fn(&mut dyn FnMut(&[String]))) -> String {
    let mut nonnull = vec![0usize; width];
    let mut approximate = vec![false; width];
    let mut numeric = vec![true; width];
    let mut rows = 0usize;
    each_row(&mut |row| {
        rows += 1;
        for (at, cell) in row.iter().enumerate() {
            if cell.is_empty() {
                continue;
            }
            nonnull[at] += 1;
            approximate[at] |= reads_as_inexact(cell);
            numeric[at] &= cell.parse::<f64>().is_ok();
        }
    });
    let approximate: Vec<bool> = (0..width)
        .map(|at| approximate[at] && numeric[at])
        .collect();

    let mut values: Vec<Vec<f64>> = vec![Vec::new(); width];
    let mut hashed: Vec<String> = Vec::new();
    each_row(&mut |row| {
        let mut exact = String::new();
        for (at, cell) in row.iter().enumerate() {
            match approximate[at] {
                true if !cell.is_empty() => values[at].push(cell.parse().expect("a numeric cell")),
                true => {}
                false => {
                    exact.push_str(cell);
                    exact.push('|');
                }
            }
        }
        hashed.push(exact);
    });

    let mut out = format!("{FINGERPRINT}rows={rows}\n");
    for at in 0..width {
        out.push_str(&format!("col {at}: nonnull={}", nonnull[at]));
        if approximate[at] {
            let triple = triple_of(&mut values[at]);
            out.push_str(&format!(
                " sum={} min={} max={}",
                number(triple.0),
                number(triple.1),
                number(triple.2)
            ));
        }
        out.push('\n');
    }
    out.push_str(&format!("hash: {}\n", hash_of(&mut hashed)));
    out
}

/// A cell whose rendering is the engine's own choice rather than common ground: a fraction,
/// an exponent, or a non-finite value. An integer, a string, a date and a boolean are not.
fn reads_as_inexact(cell: &str) -> bool {
    cell.contains('.')
        || cell.contains('e')
        || cell.contains('E')
        || matches!(cell, "inf" | "-inf" | "NaN" | "nan" | "Inf" | "-Inf")
}

/// `(sum, min, max)` with the sum taken IN VALUE ORDER, so the two sides add in one
/// sequence and float reassociation cannot move the digits the comparison reads.
fn triple_of(values: &mut [f64]) -> (f64, f64, f64) {
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let sum = values.iter().sum();
    let first = values.first().copied().unwrap_or(f64::NAN);
    let last = values.last().copied().unwrap_or(f64::NAN);
    (sum, first, last)
}

/// SHA-256 over the exact cells of every row, rows sorted as byte strings. Sorted rather
/// than streamed in arrival order: the two engines return the same rows in different
/// sequences, and a hash that depended on the sequence would compare nothing.
fn hash_of(rows: &mut [String]) -> String {
    rows.sort_unstable();
    let mut hasher = Sha256::new();
    for (at, row) in rows.iter().enumerate() {
        if at > 0 {
            hasher.update(b"\n");
        }
        hasher.update(row.as_bytes());
    }
    format!("{:x}", hasher.finalize())
}

/// `{:.17e}` with the exponent written plainly — what Python's `'{:.17e}'` prints once its
/// `+00` padding is taken off, so the two writers produce one text. A shortest-round-trip
/// form would not: Rust's and Python's differ in notation.
fn number(x: f64) -> String {
    if x.is_nan() {
        return "nan".to_string();
    }
    if x.is_infinite() {
        return match x > 0.0 {
            true => "inf".to_string(),
            false => "-inf".to_string(),
        };
    }
    format!("{x:.17e}")
}

/// What the two sides are held to: the row count, each column's `nonnull` and class, the
/// approximate triples within `tol` relative, and the hash exactly. A mismatch names the
/// column, since a fingerprint says nothing about which row moved.
pub(crate) fn compare_fingerprints(ours: &str, duckdb: &str, tol: f64) -> Result<(), String> {
    let (ours, duck) = (parse(ours)?, parse(duckdb)?);
    if ours.rows != duck.rows {
        return Err(format!("ours has {} rows, DuckDB {}", ours.rows, duck.rows));
    }
    if ours.columns.len() != duck.columns.len() {
        return Err(format!(
            "ours has {} columns against {} in DuckDB's",
            ours.columns.len(),
            duck.columns.len()
        ));
    }
    for (at, (ours, duck)) in ours.columns.iter().zip(&duck.columns).enumerate() {
        if ours.nonnull != duck.nonnull {
            return Err(format!(
                "col {at}: ours holds {} values, DuckDB {}",
                ours.nonnull, duck.nonnull
            ));
        }
        match (ours.triple, duck.triple) {
            (None, None) => {}
            (Some(ours), Some(duck)) => {
                for (what, a, b) in [
                    ("sum", ours.0, duck.0),
                    ("min", ours.1, duck.1),
                    ("max", ours.2, duck.2),
                ] {
                    if (a - b).abs() > tol * a.abs().max(b.abs()) {
                        return Err(format!("col {at}: {what} is {a} against DuckDB's {b}"));
                    }
                }
            }
            _ => {
                return Err(format!(
                    "col {at}: one side renders it approximately and the other exactly, so the \
                     two hash different columns"
                ));
            }
        }
    }
    match ours.hash == duck.hash {
        true => Ok(()),
        false => Err(format!(
            "hash over the exact columns differs: ours {}, DuckDB {} — the same values paired \
             into different rows look like this",
            ours.hash, duck.hash
        )),
    }
}

fn parse(section: &str) -> Result<Parsed, String> {
    let mut rows = None;
    let mut columns = Vec::new();
    let mut hash = None;
    for line in section.lines().filter(|line| !line.is_empty()) {
        if let Some(n) = line
            .strip_prefix(FINGERPRINT)
            .and_then(|r| r.strip_prefix("rows="))
        {
            rows = Some(number_of(n)?);
        } else if let Some(body) = line.strip_prefix("col ") {
            columns.push(column(body)?);
        } else if let Some(h) = line.strip_prefix("hash: ") {
            hash = Some(h.to_string());
        } else if !line.starts_with("mode=") {
            return Err(format!("a fingerprint carries no line `{line}`"));
        }
    }
    Ok(Parsed {
        rows: rows.ok_or("a fingerprint opens with its row count")?,
        columns,
        hash: hash.ok_or("a fingerprint ends with its hash")?,
    })
}

/// `<i>: nonnull=<n> [sum=<x> min=<x> max=<x>]`. The position is read and discarded: the
/// columns are in order and their position is where they sit.
fn column(body: &str) -> Result<Column, String> {
    let (_, fields) = body
        .split_once(": ")
        .ok_or_else(|| format!("a column line names its position: `col {body}`"))?;
    let mut nonnull = None;
    let mut triple = [None; 3];
    for field in fields.split_whitespace() {
        let (key, value) = field
            .split_once('=')
            .ok_or_else(|| format!("`{field}` is not a key=value"))?;
        match key {
            "nonnull" => nonnull = Some(number_of(value)?),
            "sum" => triple[0] = Some(float_of(value)?),
            "min" => triple[1] = Some(float_of(value)?),
            "max" => triple[2] = Some(float_of(value)?),
            other => return Err(format!("a column line carries no field `{other}`")),
        }
    }
    Ok(Column {
        nonnull: nonnull.ok_or("a column line carries its nonnull count")?,
        triple: match triple {
            [Some(sum), Some(min), Some(max)] => Some((sum, min, max)),
            [None, None, None] => None,
            _ => return Err("a triple is sum, min and max or none of the three".to_string()),
        },
    })
}

fn number_of(text: &str) -> Result<usize, String> {
    text.parse().map_err(|_| format!("`{text}` is not a count"))
}

fn float_of(text: &str) -> Result<f64, String> {
    match text {
        "nan" => Ok(f64::NAN),
        "inf" => Ok(f64::INFINITY),
        "-inf" => Ok(f64::NEG_INFINITY),
        _ => text
            .parse()
            .map_err(|_| format!("`{text}` is not a number")),
    }
}
