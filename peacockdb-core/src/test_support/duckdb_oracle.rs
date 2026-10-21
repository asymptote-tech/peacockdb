//! What a corpus line asks of DuckDB's answer (#235): the keyword parsed, and the section
//! comparison it names.

#[cfg(test)]
mod tests;

use std::collections::BTreeSet;

use super::{
    CellKind, DUCKDB_FLOAT_TOLERANCE, DuckdbOracle, RowOrder, Values, corpus, corpus_golden,
    fingerprint, result_text,
};

/// A spelling with its whitespace removed outside double quotes, so `stringify!`'s spacing
/// (`duckdb (sort , exact)`) and the registry's compare equal while a quoted column name keeps
/// its spaces.
pub(crate) fn tight(spelled: &str) -> String {
    let mut quoted = false;
    spelled
        .chars()
        .filter(|&c| {
            quoted ^= c == '"';
            quoted || c == '"' || !c.is_whitespace()
        })
        .collect()
}

/// What `parse` accepts, for its refusals.
const GRAMMAR: &str = "duckdb(sort|no_sort, exact|approx|rel(<tol>, \"<column>\", …)\
                       [, date(\"<column>\", …)])|duckdb_fingerprint|duckdb_none";

/// The spelling a `corpus_query!` line writes, as `stringify!` hands it over — whitespace
/// included, which [`tight`] takes out everywhere but inside a quoted column name.
pub(crate) fn parse(spelled: &str) -> DuckdbOracle {
    let unknown =
        || -> ! { panic!("corpus_query!: unknown duckdb_oracle '{spelled}' (expected {GRAMMAR})") };
    let compact = tight(spelled);
    match compact.as_str() {
        "duckdb_fingerprint" => return DuckdbOracle::Fingerprint,
        "duckdb_none" => return DuckdbOracle::None,
        _ => {}
    }
    let Some(args) = call_arguments(&compact, "duckdb") else {
        unknown()
    };
    let (order, values, rest) = match args.as_slice() {
        [order, values, rest @ ..] if rest.len() <= 1 => (*order, *values, rest),
        _ => unknown(),
    };
    let order = match order {
        "sort" => RowOrder::Sort,
        "no_sort" => RowOrder::NoSort,
        _ => panic!("{spelled}: the first argument is the row order, sort|no_sort"),
    };
    let values = match values {
        "exact" => Values::Exact,
        "approx" => Values::Approx,
        _ => match call_arguments(values, "rel") {
            Some(rel) if rel.len() >= 2 => Values::Rel {
                tolerance: rel[0].parse().unwrap_or_else(|_| {
                    panic!("{spelled}: rel's tolerance `{}` is not a number", rel[0])
                }),
                columns: column_names(spelled, &rel[1..]),
            },
            _ => unknown(),
        },
    };
    let dates = match rest {
        [] => Vec::new(),
        [date] => match call_arguments(date, "date") {
            Some(columns) if !columns.is_empty() => column_names(spelled, &columns),
            _ => unknown(),
        },
        _ => unreachable!("at most one argument after the values"),
    };
    DuckdbOracle::Compare {
        order,
        values,
        dates,
    }
}

/// Each argument a double-quoted column name, the quotes taken off. A position is refused:
/// columns are named as our result header prints them, which a reader can check.
fn column_names(spelled: &str, arguments: &[&str]) -> Vec<String> {
    arguments
        .iter()
        .map(|argument| {
            argument
                .strip_prefix('"')
                .and_then(|name| name.strip_suffix('"'))
                .filter(|name| !name.is_empty())
                .unwrap_or_else(|| {
                    panic!(
                        "{spelled}: `{argument}` — rel and date names columns by name, a string \
                         literal as our result header prints it"
                    )
                })
                .to_string()
        })
        .collect()
}

/// The arguments of `prefix(a, b, …)` in tightened text, split at the commas outside
/// parentheses and quotes, or `None` when the text is not that call.
fn call_arguments<'a>(compact: &'a str, prefix: &str) -> Option<Vec<&'a str>> {
    let inner = compact
        .strip_prefix(prefix)?
        .strip_prefix('(')?
        .strip_suffix(')')?;
    let mut parts = Vec::new();
    let (mut depth, mut quoted, mut start) = (0usize, false, 0);
    for (at, c) in inner.char_indices() {
        match c {
            '"' => quoted = !quoted,
            '(' if !quoted => depth += 1,
            ')' if !quoted => depth = depth.checked_sub(1)?,
            ',' if !quoted && depth == 0 => {
                parts.push(&inner[start..at]);
                start = at + 1;
            }
            _ => {}
        }
    }
    parts.push(&inner[start..]);
    Some(parts)
}

/// How one column's cells are held: as text, by our declared type ([`cell_equal`]), or within
/// a relative tolerance whatever the type.
#[derive(Clone, Copy)]
enum Rule {
    Kind(CellKind),
    Rel(f64),
}

impl Rule {
    /// Whether the pairing key reads this column as a number, so a cell inside the tolerance
    /// cannot reorder the rows.
    fn is_numeric(self) -> bool {
        !matches!(self, Rule::Kind(CellKind::Exact))
    }

    fn holds(self, ours: &str, duck: &str) -> bool {
        match self {
            Rule::Kind(kind) => cell_equal(kind, ours, duck),
            Rule::Rel(tolerance) => {
                ours == duck
                    || matches!(
                        (ours.parse::<f64>(), duck.parse::<f64>()),
                        (Ok(a), Ok(b)) if (a - b).abs() <= tolerance * a.abs().max(b.abs())
                    )
            }
        }
    }
}

/// Two rendered sections held to what the line's oracle asks of them, by column position.
///
/// `ours` is a `mini.result.txt` or `gpu-result.txt` body with its `mode=` line taken off,
/// `duckdb` the `duckdb-result.txt` body, `kinds[i]` column `i`'s class from our declared
/// output schema. `rel` and `date` find their columns by name in OUR header. An `Err` rather
/// than a panic, so the negative cases can show each comparison failing.
pub(crate) fn compare_sections(
    oracle: &DuckdbOracle,
    ours: &str,
    duckdb: &str,
    kinds: &[CellKind],
) -> Result<(), String> {
    let (ours_fp, duck_fp) = (
        fingerprint::is_fingerprint(ours),
        fingerprint::is_fingerprint(duckdb),
    );
    let (order, values, dates) = match oracle {
        DuckdbOracle::Fingerprint => {
            if !ours_fp && !duck_fp {
                return Err(
                    "duckdb_fingerprint over a section that is not fingerprinted — both sides \
                     hold their rows, so compare them"
                        .to_string(),
                );
            }
            return fingerprint::compare_over_cap(ours, duckdb, DUCKDB_FLOAT_TOLERANCE);
        }
        DuckdbOracle::None => {
            return Err(
                "duckdb_none over two sections that both exist — one of them is an answer the \
                 line says nobody has"
                    .to_string(),
            );
        }
        DuckdbOracle::Compare { .. } if ours_fp || duck_fp => {
            return Err(
                "the section is fingerprinted (over the cap) — declare duckdb_fingerprint"
                    .to_string(),
            );
        }
        DuckdbOracle::Compare {
            order,
            values,
            dates,
        } => (*order, values, dates),
    };

    let (ours, mut duck) = (table(ours), table(duckdb));
    // A width difference is an engine that dropped a column, under every oracle.
    if ours.width != duck.width {
        return Err(format!(
            "ours has {} columns against {} in DuckDB's",
            ours.width, duck.width
        ));
    }
    if ours.rows.len() != duck.rows.len() {
        return Err(format!(
            "ours has {} rows, DuckDB {}",
            ours.rows.len(),
            duck.rows.len()
        ));
    }
    let mut rules: Vec<Rule> = match values {
        Values::Exact => vec![Rule::Kind(CellKind::Exact); ours.width],
        Values::Approx | Values::Rel { .. } => kinds.iter().copied().map(Rule::Kind).collect(),
    };
    if let Values::Rel { tolerance, columns } = values {
        for at in ours.positions(columns, "rel")? {
            rules[at] = Rule::Rel(*tolerance);
        }
    }
    // DuckDB's cell cut to its date part before anything reads it, so the time can neither
    // fail a cell nor reorder the rows the pairing sorts.
    for at in ours.positions(dates, "date")? {
        for row in &mut duck.rows {
            if let Some((date, _time)) = row[at].split_once('T') {
                row[at] = date.to_string();
            }
        }
    }
    match order {
        RowOrder::Sort => same_multiset(&ours, &duck, &rules),
        RowOrder::NoSort => same_sequence(&ours, &duck, &rules),
    }
}

/// The two row lists paired by position, each column compared under its [`Rule`].
///
/// Row for row with no tie rule: on 2026-10-20 no corpus answer compared this way had rows
/// tying on its sort keys (tpch q11 does, and is a fingerprint).
fn same_sequence(ours: &Table, duck: &Table, rules: &[Rule]) -> Result<(), String> {
    for (at, (o, d)) in ours.rows.iter().zip(&duck.rows).enumerate() {
        same_row(at, o, d, rules, &ours.header)?;
    }
    Ok(())
}

/// A rendered table: its header's names, its width and its data rows as cells.
struct Table {
    header: Vec<String>,
    width: usize,
    rows: Vec<Vec<String>>,
}

impl Table {
    /// The position of each named column in this header, or an error naming the one that is
    /// not there, or there twice, and the names that are.
    fn positions(&self, names: &[String], what: &str) -> Result<Vec<usize>, String> {
        names
            .iter()
            .map(|name| {
                let at: Vec<usize> = (0..self.header.len())
                    .filter(|&i| &self.header[i] == name)
                    .collect();
                match at.as_slice() {
                    [one] => Ok(*one),
                    _ => Err(format!(
                        "{what} names column \"{name}\", which our header has {} times: {}",
                        at.len(),
                        self.header.join(", ")
                    )),
                }
            })
            .collect()
    }
}

fn table(rendered: &str) -> Table {
    let lines: Vec<&str> = rendered.lines().collect();
    let rows: Vec<Vec<String>> = match lines.len() > 4 {
        true => lines[3..lines.len() - 1]
            .iter()
            .map(|line| result_text::split_cells(line))
            .collect(),
        false => Vec::new(),
    };
    let width = result_text::rendered_width(rendered);
    let header = match width {
        0 => Vec::new(),
        _ => lines
            .get(1)
            .map(|line| result_text::split_cells(line))
            .unwrap_or_default(),
    };
    Table {
        header,
        width,
        rows,
    }
}

/// The two row lists paired, each column compared under its [`Rule`].
///
/// Paired by sorting, since neither side's rendering fixes an order the other shares: the
/// key is the text columns as text and then the numeric ones as numbers, so a cell within
/// the tolerance cannot reorder the rows and break the pairing a looser comparison exists to
/// allow.
fn same_multiset(ours: &Table, duck: &Table, rules: &[Rule]) -> Result<(), String> {
    let mut ours_keyed = keyed(&ours.rows, rules);
    let mut duck_keyed = keyed(&duck.rows, rules);
    ours_keyed.sort_by(compare_keys);
    duck_keyed.sort_by(compare_keys);
    for (at, (o, d)) in ours_keyed.iter().zip(&duck_keyed).enumerate() {
        same_row(at, o.row, d.row, rules, &ours.header)?;
    }
    Ok(())
}

/// One pair of rows under the column rules, the first cell that differs named by its
/// position and its name in our header.
fn same_row(
    at: usize,
    ours: &[String],
    duck: &[String],
    rules: &[Rule],
    header: &[String],
) -> Result<(), String> {
    for (column, rule) in rules.iter().enumerate() {
        let (a, b) = (&ours[column], &duck[column]);
        if !rule.holds(a, b) {
            let name = header.get(column).map_or("", String::as_str);
            return Err(format!(
                "row {at}, column {column} ({name}): ours `{a}`, DuckDB `{b}`"
            ));
        }
    }
    Ok(())
}

/// One row with the sort key the pairing uses.
struct Keyed<'a> {
    text: String,
    numbers: Vec<f64>,
    row: &'a [String],
}

fn keyed<'a>(rows: &'a [Vec<String>], rules: &[Rule]) -> Vec<Keyed<'a>> {
    rows.iter()
        .map(|row| {
            let mut text = String::new();
            let mut numbers = Vec::new();
            for (cell, rule) in row.iter().zip(rules) {
                match (rule.is_numeric(), cell.parse::<f64>()) {
                    (true, Ok(value)) => numbers.push(value),
                    _ => {
                        text.push_str(cell);
                        text.push('\u{1}');
                    }
                }
            }
            Keyed { text, numbers, row }
        })
        .collect()
}

fn compare_keys(a: &Keyed<'_>, b: &Keyed<'_>) -> std::cmp::Ordering {
    a.text.cmp(&b.text).then_with(|| {
        for (p, q) in a.numbers.iter().zip(&b.numbers) {
            match p.partial_cmp(q) {
                Some(std::cmp::Ordering::Equal) | None => continue,
                Some(order) => return order,
            }
        }
        std::cmp::Ordering::Equal
    })
}

/// One cell's tolerance: its text exactly, [`DUCKDB_FLOAT_TOLERANCE`] relative for a float,
/// and one unit in the last place our scale renders for a decimal — where ours truncates and
/// DuckDB answers a double, so the gap is absolute and as wide as our own last digit.
fn cell_equal(kind: CellKind, ours: &str, duck: &str) -> bool {
    if ours == duck {
        return true;
    }
    let (Ok(a), Ok(b)) = (ours.parse::<f64>(), duck.parse::<f64>()) else {
        return false;
    };
    match kind {
        CellKind::Exact => false,
        CellKind::Float => {
            (a.is_nan() && b.is_nan())
                || (a - b).abs() <= DUCKDB_FLOAT_TOLERANCE * a.abs().max(b.abs())
        }
        // The last place itself, with a hair of room: the comparison is between a truncation
        // and a rounding, so the gap reaches the whole unit and must not fail at it. A
        // negative scale is clamped — `10^|s|` is a tolerance, not a last place.
        CellKind::Decimal(scale) => {
            (a - b).abs() <= 10f64.powi(-(scale.max(0) as i32)) * (1.0 + 1e-12)
        }
    }
}

/// The cpu's committed answer against DuckDB's, under the oracle the line names.
pub(crate) async fn duckdb_case(dataset: &str, sf: &str, query: &str, oracle: &str) {
    let ours = section(&corpus_golden::result_golden(dataset, sf), query)
        .filter(|section| {
            super::section_holds_rows(section) || fingerprint::is_fingerprint(section)
        })
        .map(|section| without_mode_line(&section));
    judge(dataset, sf, query, oracle, ours, None).await
}

/// The DEVICE's recorded answer at one mode against DuckDB's. `gpu-result.txt` is keyed by
/// (query, mode), so every enabled device cell meets DuckDB rather than only the last mode —
/// a mode-dependent device answer shows nowhere else.
///
/// An absent file is a failure and not a pass: it means the device cycle that writes it has
/// not been run since the cells moved, which is a gap in the branch rather than a case with
/// nothing to do.
pub(crate) async fn duckdb_gpu_case(
    dataset: &str,
    sf: &str,
    query: &str,
    mode: &str,
    oracle: &str,
) {
    let version = std::env::var("PCK_GPU_RESULT_VERSION").ok();
    let path = corpus_golden::gpu_result_golden(dataset, sf, version.as_deref());
    let mode = super::mode_named(mode).name;
    let knob = corpus_golden::recording_knob(version.as_deref());
    let text = std::fs::read_to_string(&path).unwrap_or_else(|_| {
        panic!(
            "{dataset}/{query} at {mode}: {} does not exist, so no device answer is recorded \
             for this cell. Run a cycle with {knob} and bring it home with --pull-results.",
            path.display()
        )
    });
    corpus_golden::gpu_result_cudf_matches_path(&text, version.as_deref())
        .unwrap_or_else(|said| panic!("{}: {said}", path.display()));
    let ours = gpu_result_sections(&text)
        .into_iter()
        .find(|section| section.query == query && section.mode == mode)
        .map(|section| section.body)
        .unwrap_or_else(|| {
            panic!(
                "{dataset}/{query} at {mode}: {} holds no section for this cell. Regenerate it \
                 with {knob} and --pull-results.",
                path.display()
            )
        });
    judge(dataset, sf, query, oracle, Some(ours), Some(mode)).await
}

/// The verdict both cases reach, with DuckDB's section read here so neither has to.
///
/// `ours` is `None` where our side does not answer at all — the section is absent, or a
/// `skipped:` marker. `mode` is `Some` only for a device cell, and names the side as well as
/// the mode: the cpu's answer has no mode, the cpu being the one that authors the section.
async fn judge(
    dataset: &str,
    sf: &str,
    query: &str,
    oracle: &str,
    ours: Option<String>,
    mode: Option<&str>,
) {
    let spelled = oracle;
    let oracle = DuckdbOracle::parse(spelled);
    let duckdb = section(&corpus_golden::duckdb_golden(dataset, sf), query)
        .filter(|section| !section.starts_with("failed:") && !section.starts_with(super::SKIPPED));
    let said = match (&ours, &duckdb) {
        (Some(ours), Some(duckdb)) => {
            let kinds = corpus::output_kinds(dataset, sf, query).await;
            compare_sections(&oracle, ours, duckdb, &kinds)
        }
        // One side does not answer, which is what `duckdb_none` is for. Under any other
        // oracle it is the line that is wrong, and the line says which answer it expected.
        _ if oracle == DuckdbOracle::None => Ok(()),
        (None, _) => Err(format!(
            "we do not answer this query and the line says {} — move it to duckdb_none, or \
             enable a mode",
            tight(spelled)
        )),
        (_, None) => Err(format!(
            "DuckDB does not answer this query and the line says {} — move it to duckdb_none",
            tight(spelled)
        )),
    };
    if let Err(said) = said {
        let at = match mode {
            Some(mode) => format!("the device at {mode}"),
            None => "the cpu".to_string(),
        };
        panic!("{dataset}/{query} ({at}): {said}");
    }
}

/// One section of a golden, or `None` where the file or the section is absent. Unlike
/// `section_of`, absence is an answer here: a query DuckDB alone answers has no section on
/// our side, and that is what `duckdb_none` records.
fn section(path: &std::path::Path, query: &str) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    super::ordered_sections(&text)
        .into_iter()
        .find(|(name, _)| name == query)
        .map(|(_, body)| body)
}

/// A result section without the `mode=` line naming its author — first for a table, last
/// under a fingerprint, so the line is found wherever it sits rather than assumed.
fn without_mode_line(section: &str) -> String {
    let kept: Vec<&str> = section
        .lines()
        .filter(|line| !line.starts_with("mode="))
        .collect();
    kept.join("\n")
}

/// One `== <query> mode=<mode>` section of `gpu-result.txt`.
pub(crate) struct GpuResultSection {
    pub(crate) query: String,
    pub(crate) mode: String,
    pub(crate) body: String,
}

/// Every enabled device cell against the sections a `gpu-result` file holds, in both
/// directions: a cell with no section and a section that is no enabled cell's both fail, and
/// regenerating is what clears either. `text` is `None` where the file does not exist.
///
/// An absent file is a failure only where some cell is enabled. A dataset whose device cells
/// are all off has no file at all — the writer runs from a device case, so zero enabled cells
/// write zero sections — and a dataset landing with its cells off is the ordinary way one
/// arrives.
pub(crate) fn gpu_result_coverage(
    path: &std::path::Path,
    enabled: &BTreeSet<(String, String)>,
    text: Option<&str>,
) -> Result<(), String> {
    let Some(text) = text else {
        return match enabled.is_empty() {
            true => Ok(()),
            false => Err(format!(
                "{} does not exist, so no device answer is recorded for any cell. Run a cycle \
                 with {} and bring it home with --pull-results.",
                path.display(),
                corpus_golden::recording_knob(None)
            )),
        };
    };
    let written: BTreeSet<(String, String)> = gpu_result_sections(text)
        .into_iter()
        .map(|section| (section.query, section.mode))
        .collect();
    let missing: Vec<&(String, String)> = enabled.difference(&written).collect();
    let extra: Vec<&(String, String)> = written.difference(enabled).collect();
    match missing.is_empty() && extra.is_empty() {
        true => Ok(()),
        false => Err(format!(
            "regenerate {} ({}, --pull-results) — missing {missing:?}, not an enabled cell \
             {extra:?}",
            path.display(),
            corpus_golden::recording_knob(None)
        )),
    }
}

/// Every section of `gpu-result.txt`, by the two fields its header carries. A header the
/// reader does not understand panics rather than being skipped: a section silently dropped
/// is a device cell silently uncompared.
pub(crate) fn gpu_result_sections(text: &str) -> Vec<GpuResultSection> {
    super::ordered_sections(text)
        .into_iter()
        .map(|(header, body)| {
            let (query, mode) = header.split_once(" mode=").unwrap_or_else(|| {
                panic!("gpu-result.txt: `== {header}` names no mode — it is keyed by (query, mode)")
            });
            GpuResultSection {
                query: query.to_string(),
                mode: mode.to_string(),
                body,
            }
        })
        .collect()
}
