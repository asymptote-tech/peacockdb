//! What a corpus line asks of DuckDB's answer (#235): the keyword parsed, and the section
//! comparison it names.

#[cfg(test)]
mod tests;

use std::collections::BTreeSet;
use std::sync::OnceLock;

use super::{
    CellKind, DUCKDB_FLOAT_TOLERANCE, DuckdbOracle, corpus, corpus_golden, fingerprint, result_text,
};

/// The keyword a `corpus_query!` line writes, as `stringify!` hands it over — whitespace
/// included, since `stringify!(duckdb_divergent(243, 1))` spaces its arguments out.
pub(crate) fn parse(spelled: &str) -> DuckdbOracle {
    let tight: String = spelled.chars().filter(|c| !c.is_whitespace()).collect();
    // Decoded through `ALL`, as `cpu_oracle_mode` and `gpu_result_mode` decode theirs: the
    // accepted set and the list those tests hold to the corpus are then one list.
    for variant in DuckdbOracle::ALL {
        let name = variant.name();
        let parameterized = matches!(variant, DuckdbOracle::Divergent { .. });
        match (parameterized, arguments(&tight, name), tight == name) {
            // The bare name reaches `divergent` too, for its "takes a ticket first".
            (true, Some(args), _) => return divergent(spelled, &args),
            (true, None, true) => return divergent(spelled, &[]),
            (false, _, true) => return variant,
            _ => {}
        }
    }
    let known: Vec<&str> = DuckdbOracle::ALL.iter().map(|o| o.name()).collect();
    panic!(
        "corpus_query!: unknown duckdb_oracle '{spelled}' (expected {}; duckdb_divergent \
         takes (<ticket>[, <positions>]))",
        known.join("|")
    )
}

fn divergent(spelled: &str, args: &[&str]) -> DuckdbOracle {
    assert!(
        !args.is_empty() && !args[0].is_empty(),
        "{spelled}: duckdb_divergent takes a ticket first"
    );
    DuckdbOracle::Divergent {
        ticket: number(spelled, args[0]) as u32,
        columns: args[1..].iter().map(|a| number(spelled, a)).collect(),
    }
}

/// The arguments of `prefix(a, b, …)` in already-tightened text, or `None` when the text is
/// not that call at all. An empty list is `Some(&[""])`, which the caller rejects by name.
fn arguments<'a>(tight: &'a str, prefix: &str) -> Option<Vec<&'a str>> {
    let inner = tight
        .strip_prefix(prefix)?
        .strip_prefix('(')?
        .strip_suffix(')')?;
    Some(inner.split(',').collect())
}

fn number(spelled: &str, argument: &str) -> usize {
    argument
        .parse()
        .unwrap_or_else(|_| panic!("{spelled}: `{argument}` is not a number"))
}

/// Two rendered sections held to what the line's oracle asks of them, by column position.
///
/// `ours` is a `mini.result.txt` or `gpu-result.txt` body with its `mode=` line taken off,
/// `duckdb` the `duckdb-result.txt` body, `kinds[i]` column `i`'s class from our declared
/// output schema, and `tickets_open` what decides whether a declared divergence still has a
/// ticket somebody can read. An `Err` rather than a panic, so `Divergent` can use the same
/// comparison as a predicate.
pub(crate) fn compare_sections(
    oracle: &DuckdbOracle,
    ours: &str,
    duckdb: &str,
    kinds: &[CellKind],
    tickets_open: &dyn Fn(u32) -> bool,
) -> Result<(), String> {
    let (ours_fp, duck_fp) = (
        fingerprint::is_fingerprint(ours),
        fingerprint::is_fingerprint(duckdb),
    );
    match oracle {
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
        _ if ours_fp || duck_fp => {
            return Err(format!(
                "{}: the section is fingerprinted (over the cap) — declare duckdb_fingerprint",
                oracle.name()
            ));
        }
        _ => {}
    }

    let (ours, duck) = (table(ours), table(duckdb));
    // A ROW-LEVEL divergence — `duckdb_divergent(<ticket>)`, no positions — says the two
    // answer different row sets, and the width is part of what differs: tpcds q17 renders no
    // columns where DuckDB renders fifteen, both over zero rows (#205). Under every other
    // oracle a width difference is an engine that dropped a column.
    let row_level = matches!(oracle, DuckdbOracle::Divergent { columns, .. } if columns.is_empty());
    let same_width = ours.width == duck.width;
    if !row_level && !same_width {
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
    let every: Vec<usize> = (0..ours.width).collect();
    if let DuckdbOracle::Divergent { ticket, columns } = oracle
        && let Some(past) = columns.iter().find(|column| **column >= ours.width)
    {
        return Err(format!(
            "duckdb_divergent(#{ticket}) names column {past} and the answer has {} — the \
             positions are 0-based and this line's are out of range",
            ours.width
        ));
    }
    let stopped = |what: String| {
        Err(format!(
            "declared divergent on #{} and {what} stopped diverging — the line is now \
             duckdb_approx, and the ticket may be closeable",
            match oracle {
                DuckdbOracle::Divergent { ticket, .. } => *ticket,
                _ => unreachable!("only a divergence stops diverging"),
            }
        ))
    };
    match oracle {
        DuckdbOracle::Exact => same_multiset(&ours, &duck, &every, None),
        DuckdbOracle::Approx => same_multiset(&ours, &duck, &every, Some(kinds)),
        DuckdbOracle::Divergent { ticket, columns } if columns.is_empty() => {
            if same_width && same_multiset(&ours, &duck, &every, Some(kinds)).is_ok() {
                return stopped("the row set".to_string());
            }
            open_ticket(*ticket, tickets_open)
        }
        DuckdbOracle::Divergent { ticket, columns } => {
            let undeclared: Vec<usize> = every
                .iter()
                .copied()
                .filter(|c| !columns.contains(c))
                .collect();
            same_multiset(&ours, &duck, &undeclared, Some(kinds))?;
            // EACH named column, not the set: a column that came right while its neighbour
            // still diverges is a line to narrow, and the set as a whole would hide it.
            for &column in columns {
                if same_multiset(&ours, &duck, &[column], Some(kinds)).is_ok() {
                    return stopped(format!("column {column}"));
                }
            }
            open_ticket(*ticket, tickets_open)
        }
        DuckdbOracle::Fingerprint | DuckdbOracle::None => unreachable!("returned above"),
    }
}

/// A divergence is declared against a ticket somebody is reading, or it is declared against
/// nothing: an archived number excuses a defect no list holds.
fn open_ticket(ticket: u32, tickets_open: &dyn Fn(u32) -> bool) -> Result<(), String> {
    match tickets_open(ticket) {
        true => Ok(()),
        false => Err(format!(
            "#{ticket} is not open, so this divergence is declared against a ticket nobody is \
             reading"
        )),
    }
}

/// A rendered table's width and its data rows as cells.
struct Table {
    width: usize,
    rows: Vec<Vec<String>>,
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
    Table {
        width: result_text::rendered_width(rendered),
        rows,
    }
}

/// The two row lists projected onto `columns` and paired, each kept column compared with
/// [`cell_equal`]. `kinds = None` holds every cell to its text.
///
/// Paired by sorting, since neither side's rendering fixes an order the other shares: the
/// key is the EXACT cells as text and then the approximate ones as numbers, so a cell within
/// the tolerance cannot reorder the rows and break the pairing a looser comparison exists to
/// allow.
fn same_multiset(
    ours: &Table,
    duck: &Table,
    columns: &[usize],
    kinds: Option<&[CellKind]>,
) -> Result<(), String> {
    let mut ours = keyed(&ours.rows, columns, kinds);
    let mut duck = keyed(&duck.rows, columns, kinds);
    ours.sort_by(compare_keys);
    duck.sort_by(compare_keys);
    for (at, (ours, duck)) in ours.iter().zip(&duck).enumerate() {
        for &column in columns {
            let kind = kinds.map_or(CellKind::Exact, |kinds| kinds[column]);
            let (a, b) = (&ours.row[column], &duck.row[column]);
            if !cell_equal(kind, a, b) {
                return Err(format!(
                    "row {at}, column {column}: ours `{a}`, DuckDB `{b}`"
                ));
            }
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

fn keyed<'a>(
    rows: &'a [Vec<String>],
    columns: &[usize],
    kinds: Option<&[CellKind]>,
) -> Vec<Keyed<'a>> {
    rows.iter()
        .map(|row| {
            let mut text = String::new();
            let mut numbers = Vec::new();
            for &column in columns {
                let cell = &row[column];
                let approximate = kinds.is_some_and(|kinds| kinds[column] != CellKind::Exact);
                match (approximate, cell.parse::<f64>()) {
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
    let text = std::fs::read_to_string(&path).unwrap_or_else(|_| {
        panic!(
            "{dataset}/{query} at {mode}: {} does not exist, so no device answer is recorded \
             for this cell. Run a cycle with PCK_WRITE_GPU_RESULT=1 and bring it home with \
             --pull-results.",
            path.display()
        )
    });
    let ours = gpu_result_sections(&text)
        .into_iter()
        .find(|section| section.query == query && section.mode == mode)
        .map(|section| section.body)
        .unwrap_or_else(|| {
            panic!(
                "{dataset}/{query} at {mode}: {} holds no section for this cell. Regenerate it \
                 with PCK_WRITE_GPU_RESULT=1 and --pull-results.",
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
    let oracle = DuckdbOracle::parse(oracle);
    let duckdb = section(&corpus_golden::duckdb_golden(dataset, sf), query)
        .filter(|section| !section.starts_with("failed:") && !section.starts_with(super::SKIPPED));
    let said = match (&ours, &duckdb) {
        (Some(ours), Some(duckdb)) => {
            let kinds = corpus::output_kinds(dataset, sf, query).await;
            compare_sections(&oracle, ours, duckdb, &kinds, &ticket_is_open)
        }
        // One side does not answer, which is what `duckdb_none` is for. Under any other
        // oracle it is the line that is wrong, and the line says which answer it expected.
        _ if oracle == DuckdbOracle::None => Ok(()),
        (None, _) => Err(format!(
            "we do not answer this query and the line says {} — move it to duckdb_none, or \
             enable a mode",
            oracle.name()
        )),
        (_, None) => Err(format!(
            "DuckDB does not answer this query and the line says {} — move it to duckdb_none",
            oracle.name()
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

/// Whether `llm-wiki/tickets/` still holds this ticket.
///
/// An archived one lives in `llm-wiki/archive/archived-tickets.md`, which is NOT read: a
/// divergence declared against a ticket somebody closed is a divergence nobody is tracking,
/// and the line should have moved when the fix landed.
pub(crate) fn ticket_is_open(number: u32) -> bool {
    static OPEN: OnceLock<BTreeSet<u32>> = OnceLock::new();
    OPEN.get_or_init(|| {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../llm-wiki/tickets");
        let mut open = BTreeSet::new();
        let entries = std::fs::read_dir(&dir)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", dir.display()));
        for entry in entries {
            let path = entry.expect("a ticket file").path();
            if path.extension().is_some_and(|e| e == "md") {
                let text = std::fs::read_to_string(&path)
                    .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
                open.extend(anchors(&text));
            }
        }
        assert!(!open.is_empty(), "{}: no ticket anchors", dir.display());
        open
    })
    .contains(&number)
}

/// Every `<a id="tNN">` anchor in a ticket file. Every match rather than the first: a file
/// holds dozens, and the anchor is what the cost widget links a ticket by.
fn anchors(text: &str) -> Vec<u32> {
    text.lines()
        .filter_map(|line| {
            line.trim()
                .strip_prefix("<a id=\"t")?
                .split_once('"')
                .and_then(|(number, _)| number.parse().ok())
        })
        .collect()
}
