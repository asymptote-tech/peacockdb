//! The fingerprint a result section holds instead of its rows when it is over the cap.
//!
//! Both result writers produce it — this one and `testdata/duckdb_result.py` — so an answer
//! too large to commit is still an answer DuckDB can be held to. A column goes into the hash
//! when it renders identically on both engines and into an approximate triple when it does
//! not, and only a FLOAT is the second. Each side classes from its own DECLARED type and
//! never from the cells ([`is_approximate`] here, `is_approximate` there): a rendered `31.00`
//! does not say whether it was a decimal or a double, and a column holding no value says
//! nothing at all. Two declarations that disagree are incomparable, and
//! [`compare_fingerprints`] names the column and the remedy.

use datafusion::arrow::array::RecordBatch;
use datafusion::arrow::datatypes::DataType;
use datafusion::arrow::util::display::{ArrayFormatter, FormatOptions};
use sha2::{Digest, Sha256};

use super::{SKIPPED, result_text};

/// What a fingerprinted section opens with. First position, as `SKIPPED` keeps: a reader
/// deciding whether a section holds rows looks at the first line.
pub(crate) const FINGERPRINT: &str = "fingerprint: ";

/// What separates two cells inside one hashed row, written the same by both writers.
///
/// `\u{1}`, as [`result_text`]'s row rendering and the tolerance arm's key already use. A
/// separator that occurs in data makes `("a|b","c")` and `("a","b|c")` hash alike — two
/// different answers agreeing, on the comparison that has no second opinion behind it — and
/// `o_comment` is 1.5M hashed rows in anti-join and semi-join.
const CELL_SEPARATOR: char = '\u{1}';

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
    let approximate: Vec<bool> = batches.first().map_or_else(Vec::new, |batch| {
        batch
            .schema()
            .fields()
            .iter()
            .map(|field| is_approximate(field.data_type()))
            .collect()
    });
    fingerprint(&approximate, &|visit| rows_of_batches(batches, visit))
}

/// A column whose cells do NOT render identically on the two engines, so it leaves the hash
/// and compares by its triple instead: a float, which each side prints to its own precision.
///
/// A decimal renders at its declared scale on both sides, and so do an integer, a string, a
/// date, a boolean and a timestamp — all hashed row by row, which is what makes an over-cap
/// join of them checked row for row rather than by its sums. Where the two sides declare
/// different types for one column, a decimal here against a double there, their classes
/// disagree and [`compare_fingerprints`] names it.
fn is_approximate(declared: &DataType) -> bool {
    matches!(
        declared,
        DataType::Float16 | DataType::Float32 | DataType::Float64
    )
}

/// The same fingerprint over a rendered table, for the side that stayed under the cap while
/// the other crossed it. The cells are the ones the table prints, trimmed of its padding; the
/// classes come from the fingerprinted side, because a rendering no longer says whether a
/// numeric column was a decimal or a double.
///
/// Trimmed is why it agrees with [`fingerprint_of`], which takes its cells straight from
/// `ArrayFormatter`: equal only for cells with no surrounding whitespace of their own, which
/// is every cell either writer renders — the padding here is the table's, not the value's.
pub(crate) fn fingerprint_of_rendered(table: &str, approximate: &[bool]) -> String {
    fingerprint(approximate, &|visit| {
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

/// The section text, in one pass: the classes are decided before it starts, so every cell
/// goes straight to its column's value list or to the row's hashed text.
fn fingerprint(approximate: &[bool], each_row: &dyn Fn(&mut dyn FnMut(&[String]))) -> String {
    let width = approximate.len();
    let mut nonnull = vec![0usize; width];
    let mut values: Vec<Vec<f64>> = vec![Vec::new(); width];
    let mut hashed: Vec<String> = Vec::new();
    let mut rows = 0usize;
    each_row(&mut |row| {
        rows += 1;
        assert_eq!(
            row.len(),
            width,
            "a row of {} cells under {width} classes",
            row.len()
        );
        let mut exact = String::new();
        for (at, cell) in row.iter().enumerate() {
            if !cell.is_empty() {
                nonnull[at] += 1;
            }
            match approximate[at] {
                true if !cell.is_empty() => values[at].push(
                    cell.parse()
                        .unwrap_or_else(|_| panic!("column {at} is a float and renders `{cell}`")),
                ),
                true => {}
                false => {
                    exact.push_str(cell);
                    exact.push(CELL_SEPARATOR);
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

/// `(sum, min, max)` with the sum taken IN VALUE ORDER, so the two sides add in one
/// sequence and float reassociation cannot move the digits the comparison reads.
///
/// Folded from an explicit `0.0` rather than `Iterator::sum`, whose identity for `f64` is
/// `-0.0` — which renders as `-0.00000000000000000e0` and so disagreed with the Python
/// writer's `0.0` on every column that sums to zero, an all-NULL one among them.
///
/// A NaN among the values makes the WHOLE triple NaN, on both writers. Neither sort places a
/// NaN: this one's comparator calls it equal to everything and Python's `sorted` leaves it
/// where the rows put it, so min and max were a function of each side's row order — and the
/// two engines return the same rows in different sequences. `nan_settles` then reads two NaN
/// triples as the one absent value they are, and a NaN against a number as a difference.
fn triple_of(values: &mut [f64]) -> (f64, f64, f64) {
    if values.iter().any(|value| value.is_nan()) {
        return (f64::NAN, f64::NAN, f64::NAN);
    }
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let sum = values.iter().fold(0.0, |total, value| total + value);
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

/// Two over-cap sections compared, where at least one holds a fingerprint.
///
/// Near the cap one writer rendered what the other fingerprinted — DuckDB's `repr` prints
/// longer floats than arrow-rs, so its rendering crosses the cap where ours does not. The
/// FINGERPRINTED side declares the column classes and the rendered side is fingerprinted
/// under them: a rendering no longer says whether a numeric column was a decimal or a
/// double, so classing it again from its cells would call a decimal approximate here and
/// exact there, and the two would be incomparable for a reason neither side is wrong about.
pub(crate) fn compare_over_cap(ours: &str, duckdb: &str, tol: f64) -> Result<(), String> {
    let (ours, duckdb) = match (is_fingerprint(ours), is_fingerprint(duckdb)) {
        (true, true) => (ours.to_string(), duckdb.to_string()),
        (true, false) => (ours.to_string(), under_the_classes_of(duckdb, ours)?),
        (false, true) => (under_the_classes_of(ours, duckdb)?, duckdb.to_string()),
        (false, false) => return Err("neither side is fingerprinted".to_string()),
    };
    compare_fingerprints(&ours, &duckdb, tol)
}

/// A rendered table fingerprinted under the classes `fingerprinted` declares.
fn under_the_classes_of(table: &str, fingerprinted: &str) -> Result<String, String> {
    let classes: Vec<bool> = parse(fingerprinted)?
        .columns
        .iter()
        .map(|column| column.triple.is_some())
        .collect();
    let width = result_text::rendered_width(table);
    if width != classes.len() {
        return Err(format!(
            "the rendered side has {width} columns against {} in the fingerprinted one's",
            classes.len()
        ));
    }
    Ok(fingerprint_of_rendered(table, &classes))
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
                    let agree = result_text::nan_settles(a, b)
                        .unwrap_or_else(|| (a - b).abs() <= tol * a.abs().max(b.abs()));
                    if !agree {
                        return Err(format!("col {at}: {what} is {a} against DuckDB's {b}"));
                    }
                }
            }
            // Neither side is wrong: the declarations themselves differ, `avg` and division
            // answering a fixed-scale decimal here and a double there. The hash is taken over
            // whatever a side called exact and a fingerprint no longer holds the rows, so
            // there is nothing to recompute it from and the remedy is at the writers.
            _ => {
                let (ours, duck) = match ours.triple.is_some() {
                    true => ("approximately", "exactly"),
                    false => ("exactly", "approximately"),
                };
                return Err(format!(
                    "col {at}: ours renders it {ours} and DuckDB's {duck}, so the two hash \
                     different columns and neither hash can be recomputed from a fingerprint. \
                     Class it alike on both sides — `is_approximate` here, \
                     `duckdb_result.py`'s there; duckdb_divergent does not reach this path"
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
