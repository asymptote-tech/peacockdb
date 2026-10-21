//! Test registry: the link-time inventory behind `testdata/cost-registry.csv`.
//!
//! Every unified test macro submits one [`RegistryEntry`] at its invocation site, so
//! the cost widget's CSV can be checked against what the suite ACTUALLY declares
//! rather than against a textual scrape of one suite file (which could only ever see
//! one file, and only by parsing comments).
//!
//! # Why this is verified per test binary
//!
//! `inventory` collects per LINKED BINARY, and the corpus expands in two of them — the
//! cpu tier and the device tier — so no single test can see all registrations. Instead
//! each binary asserts the CSV columns IT owns, in both directions, and together they
//! cover every column.
//! [`assert_registry_matches_csv`] takes those owned columns explicitly rather than
//! inferring them: inferring "columns this binary registered something for" would
//! silently pass a binary whose entire column vanished.
//!
//! # States
//!
//! A cell is `enabled | skip | disabled | na`. Only `enabled` and `skip` produce a
//! test (and therefore a registration); `disabled` (commented-out invocation) and
//! `na` (mode never applied to this query) must be ABSENT from the inventory. That
//! asymmetry is what makes the reverse direction meaningful.

use std::collections::{BTreeMap, BTreeSet};

#[cfg(test)]
mod tests;

use super::corpus::CpuOracle;
use super::device_answer::GpuResultMode;
use super::{CsvRow, RegistryEntry};

/// The CSV's per-mode columns, in file order.
///
/// Three groups, one per thing that can be enabled independently. The five bare mode columns
/// are plan enablement, and they are the one group no test macro registers: the plan goldens
/// are one file per mode rather than one file per query, so what declares a cell is the
/// golden's section for that query, and
/// `planner/tests/plan_goldens.rs` is what holds the two to each other in both directions.
/// The `cpu_` and `gpu_` columns are execution, declared by `corpus_query!` through
/// this inventory — one per engine because a query can be correct at five modes on the cpu
/// and at two on a device.
///
/// Flat rather than a repeated group: the file has two independent readers, `registry.rs`
/// and `cost-report`, and both parse by header name. A repeated group would need the two to
/// agree on a decoding convention as well, which is one more place to drift.
pub(crate) const COLUMNS: [&str; 15] = [
    "tp1_single",
    "tp1_rowgroup",
    "tp4_single",
    "tp4_rowgroup",
    "tp4_sized",
    "cpu_tp1_single",
    "cpu_tp1_rowgroup",
    "cpu_tp4_single",
    "cpu_tp4_rowgroup",
    "cpu_tp4_sized",
    "gpu_tp1_single",
    "gpu_tp1_rowgroup",
    "gpu_tp4_single",
    "gpu_tp4_rowgroup",
    "gpu_tp4_sized",
];

/// Map a registration to its CSV column: the engine it ran on and the mode it ran at,
/// composed rather than parsed off a label.
pub(crate) fn column_for(kind: &str, device: &str) -> Option<&'static str> {
    match kind {
        "cpu" | "gpu" => mode_column(kind, device),
        _ => None,
    }
}

/// A corpus registration's column: its engine and the mode it ran at. The mode really is
/// carried per registration — one macro invocation expands to a case per enabled mode — so
/// this composes rather than looks up, and the mode set is checked exhaustively: an
/// unlisted one is `None` and the registration is reported unmappable, rather than being
/// silently binned into whichever column a prefix match reached first.
fn mode_column(kind: &str, mode: &str) -> Option<&'static str> {
    let known = super::MODES.iter().any(|m| m.ident() == mode);
    let suffix = known.then_some(mode)?;
    COLUMNS
        .iter()
        .find(|column| **column == format!("{kind}_{suffix}"))
        .copied()
}

pub(crate) fn stem(query: &str) -> String {
    query.replace('_', "-")
}

/// The 15 hand-assigned feature codes. Not derived from SQL and not asserted
/// against it — but the SET is closed, so a typo'd code fails rather than silently
/// creating a new one-off category that renders as an unknown chip in the widget.
pub(crate) const FEATURE_CODES: [&str; 15] = [
    "window_functions",
    "rollup",
    "grouping_sets",
    "anti_join",
    "semi_join",
    "cross_join",
    "nested_loop_join",
    "corr_subquery",
    "stddev_var",
    "avg",
    "count_distinct",
    "string_like",
    "top_n",
    "outer_join",
    "limit_offset",
];

/// The last four columns: what a query's `corpus_query!` line declares besides its modes, each
/// its full spelling. The test binary that expands the lines holds them to the CSV, both
/// directions (`the_registry_carries_each_lines_oracles_and_schema_validation`).
pub(crate) const DECLARATION_COLUMNS: [&str; 4] = [
    "duckdb_oracle",
    "cpu_oracle",
    "gpu_oracle",
    "schema_validation",
];

/// What one side's result section holds; a side with no section, or a `skipped:` or
/// `failed:` marker, has none.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Stored {
    Result,
    Fingerprint,
}

impl Stored {
    fn word(self) -> &'static str {
        match self {
            Stored::Result => "result",
            Stored::Fingerprint => "fingerprint",
        }
    }
}

/// The `stored` cell for one query: one word where every side with a section holds that
/// format, else `cpu|duckdb|gpu` with `—` for a side with no section. `gpu` has one entry per
/// device mode with a section, and reads `mixed` where they disagree.
pub(crate) fn stored_cell(cpu: Option<Stored>, duckdb: Option<Stored>, gpu: &[Stored]) -> String {
    let gpu_part = match gpu {
        [] => None,
        [first, rest @ ..] if rest.iter().all(|mode| mode == first) => Some(first.word()),
        _ => Some("mixed"),
    };
    let parts = [cpu.map(Stored::word), duckdb.map(Stored::word), gpu_part];
    let present: BTreeSet<&str> = parts.iter().flatten().copied().collect();
    match present.len() {
        0 => panic!("stored_cell: no section on any side — a corpus line nobody answers"),
        1 => present.into_iter().next().expect("one format").to_string(),
        _ => parts.map(|part| part.unwrap_or("—")).join("|"),
    }
}

/// The three sides a `stored` cell names, in its order.
const STORED_SIDES: [&str; 3] = ["cpu", "duckdb", "gpu"];

/// A registry `stored` cell against what the goldens say, or `None` where they agree. The
/// mismatch names the first side whose part differs, a one-word cell read as that word thrice.
pub(crate) fn stored_mismatch(dataset: &str, query: &str, got: &str, want: &str) -> Option<String> {
    if got == want {
        return None;
    }
    let parts = |cell: &str| -> Vec<String> {
        match cell.split('|').collect::<Vec<_>>().as_slice() {
            [word] => vec![word.to_string(); 3],
            many => many.iter().map(|part| part.to_string()).collect(),
        }
    };
    let (got_parts, want_parts) = (parts(got), parts(want));
    // Equal parts and unequal text: three parts spelled where one word serves.
    let side = match (0..3).find(|&at| got_parts.get(at) != want_parts.get(at)) {
        Some(at) => format!("{} differs", STORED_SIDES[at]),
        None => format!("write it as {want}"),
    };
    Some(format!(
        "{dataset}/{query}: the registry says {got}, the goldens say {want} ({side})"
    ))
}

/// A `stored` cell's grammar: `na` exactly where the row has no corpus line, else one word or
/// three parts, each `result`, `fingerprint` or `—`, the gpu part also `mixed`.
pub(crate) fn stored_spelling_is_valid(stored: &str, has_line: bool) -> Result<(), String> {
    let format = |part: &str| matches!(part, "result" | "fingerprint");
    let valid = match (has_line, stored.split('|').collect::<Vec<_>>().as_slice()) {
        (false, _) => stored == "na",
        (true, [word]) => format(word),
        (true, [cpu, duckdb, gpu]) => {
            (format(cpu) || *cpu == "—")
                && (format(duckdb) || *duckdb == "—")
                && (format(gpu) || matches!(*gpu, "—" | "mixed"))
        }
        _ => false,
    };
    match valid {
        true => Ok(()),
        false => Err(format!(
            "stored is {stored:?}: a row with a corpus line says result, fingerprint or              cpu|duckdb|gpu (each result, fingerprint or —, the gpu part also mixed), and a              row without one says na"
        )),
    }
}

/// One CSV line's fields, RFC 4180: a field may be quoted, and `""` inside quotes is one `"`.
/// A writer quotes a field only where it holds a comma or a quote — an oracle spelling with
/// arguments — and the three other readers (`cost-report`, `ticket_widget.py`,
/// `duckdb_result.py`) split by the same rule.
pub(crate) fn split_csv_line(line: &str) -> Vec<String> {
    let mut fields = vec![String::new()];
    let mut quoted = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        let field = fields.last_mut().expect("a field under construction");
        match (c, quoted) {
            ('"', true) if chars.peek() == Some(&'"') => {
                field.push('"');
                chars.next();
            }
            ('"', _) => quoted = !quoted,
            (',', false) => fields.push(String::new()),
            _ => field.push(c),
        }
    }
    fields
}

pub(crate) fn registry_csv_path() -> std::path::PathBuf {
    super::testdata_root().join("cost-registry.csv")
}

/// Parse the committed registry CSV. Panics with a precise message on malformed
/// input — this is a committed fixture, so a parse failure is a bug to fix, not a
/// condition to tolerate.
pub(crate) fn load_csv() -> Vec<CsvRow> {
    let path = registry_csv_path();
    // Name the PROVISIONING requirement, not just the io error: a bare "No such
    // file or directory" on a remote host reads as "the registry check is broken"
    // when it actually means "this host was not given the fixture". Provisioning
    // paths name their shipped files by hand, so point straight at them.
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "cannot read the cost-registry fixture: {e}\n\
             Expected at: {}\n\
             This is a PROVISIONING gap, not a test failure: testdata/cost-registry.csv \
             is a committed fixture that must be shipped to whatever host runs this \
             suite. Each provisioning path names the files it ships, so a NEW path (or \
             a new fixture) has to be added to it explicitly: see the rsync steps in \
             scripts/build-test-shadgpu.sh and pipeline.yml's gpu-tests job. Set \
             PEACOCK_TESTDATA_DIR if the fixture lives elsewhere.",
            path.display()
        )
    });
    let mut lines = text.lines();
    let header = split_csv_line(lines.next().expect("registry CSV is empty"));
    let expect: Vec<&str> = ["dataset", "sf", "query"]
        .into_iter()
        .chain(COLUMNS)
        .chain(["plan_status", "features", "tickets", "stored"])
        .chain(DECLARATION_COLUMNS)
        .collect();
    assert_eq!(
        header, expect,
        "registry CSV header changed; update COLUMNS to match"
    );

    let mut rows = Vec::new();
    for (i, line) in lines.enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let fields = split_csv_line(line);
        let f: Vec<&str> = fields.iter().map(String::as_str).collect();
        assert_eq!(
            f.len(),
            expect.len(),
            "{}:{}: expected {} fields, got {}: {line}",
            path.display(),
            i + 2,
            expect.len(),
            f.len()
        );
        let mut states = BTreeMap::new();
        for (c, col) in COLUMNS.iter().enumerate() {
            let s = f[3 + c];
            assert!(
                matches!(s, "enabled" | "skip" | "disabled" | "na"),
                "{}:{}: column {col} has invalid state {s:?} (expected enabled|skip|disabled|na)",
                path.display(),
                i + 2
            );
            states.insert(col.to_string(), s.to_string());
        }
        // plan_status: does create_physical_plan SUCCEED for this query? Distinct
        // from the `plan` COLUMN, which records whether a query_plan_test exists.
        // A query can have no plan test and still plan fine, and the widget needs
        // the distinction to decide whether a row renders "plan ✓" or "plan ✗".
        // Indices derive from COLUMNS so adding a mode column can't silently shift
        // plan_status/features/tickets into the wrong field.
        let plan_status = f[3 + COLUMNS.len()];
        assert!(
            matches!(plan_status, "ok" | "fail"),
            "{}:{}: plan_status must be ok|fail, got {plan_status:?}",
            path.display(),
            i + 2
        );
        // A query that cannot be physically planned cannot execute in any mode, and
        // cannot have a passing plan test. Both hold today; asserting keeps a future
        // edit from producing a row the widget would render incoherently.
        if plan_status == "fail" {
            for (col, s) in &states {
                assert!(
                    matches!(s.as_str(), "disabled" | "na"),
                    "{}:{}: plan_status=fail but {col}={s} (a query that fails to plan \
                     cannot execute)",
                    path.display(),
                    i + 2
                );
            }
        }
        let features: Vec<String> = f[4 + COLUMNS.len()]
            .split_whitespace()
            .map(str::to_string)
            .collect();
        for feat in &features {
            assert!(
                FEATURE_CODES.contains(&feat.as_str()),
                "{}:{}: unknown feature code {feat:?} — must be one of {FEATURE_CODES:?}",
                path.display(),
                i + 2
            );
        }
        let tickets: Vec<String> = f[5 + COLUMNS.len()]
            .split_whitespace()
            .map(str::to_string)
            .collect();
        for t in &tickets {
            assert!(
                t.chars().all(|c| c.is_ascii_digit()),
                "{}:{}: ticket {t:?} is not a bare issue number",
                path.display(),
                i + 2
            );
        }
        // A cell turned off names the ticket that explains it. Bulk disablement is what a
        // rollout does, and a row that says only "not here" is one nobody can act on or
        // close — the ticket is what makes it findable when the blocker clears. `na` counts
        // as off: it is what a row whose case is commented out writes, so reading `disabled`
        // alone let a whole row go unexplained.
        let off = COLUMNS
            .iter()
            .filter(|col| {
                states
                    .get(**col)
                    .is_some_and(|state| state == "disabled" || state == "na")
            })
            .count();
        assert!(
            off == 0 || !tickets.is_empty(),
            "{}:{}: {off} cells off and no ticket — name the one that explains them",
            path.display(),
            i + 2
        );
        // What the query's `corpus_query!` line says, copied for the widget. A query with
        // no line — it does not plan — has `na` in all four, and only such a query does.
        let declared: Vec<&str> = f[7 + COLUMNS.len()..].to_vec();
        let (duckdb_oracle, cpu_oracle, gpu_oracle, schema_validation) =
            (declared[0], declared[1], declared[2], declared[3]);
        // The accepted keywords are the ones `cpu_oracle_mode` and `gpu_result_mode` decode.
        let cpu_oracles: Vec<&str> = CpuOracle::ALL.iter().map(|o| o.keyword()).collect();
        let gpu_oracles: Vec<&str> = GpuResultMode::ALL.iter().map(|m| m.keyword()).collect();
        let all_na = declared.iter().all(|v| *v == "na");
        assert!(
            all_na
                || (cpu_oracles.contains(&cpu_oracle)
                    && gpu_oracles.contains(&gpu_oracle)
                    && matches!(schema_validation, "enabled" | "disabled")),
            "{}:{}: cpu_oracle/gpu_oracle/schema_validation must be all `na` or one of \
             {cpu_oracles:?} / {gpu_oracles:?} / enabled|disabled, got {declared:?}",
            path.display(),
            i + 2
        );
        // Decoded, so a misspelling fails here naming the accepted set rather than reaching
        // the widget as a keyword nothing compares by.
        if !all_na {
            super::DuckdbOracle::parse(duckdb_oracle);
        }
        let stored = f[6 + COLUMNS.len()];
        if let Err(said) = stored_spelling_is_valid(stored, !all_na) {
            panic!("{}:{}: {said}", path.display(), i + 2);
        }
        rows.push(CsvRow {
            dataset: f[0].to_string(),
            sf: f[1].to_string(),
            query: f[2].to_string(),
            states,
            plan_status: plan_status.to_string(),
            features,
            tickets,
            stored: stored.to_string(),
            duckdb_oracle: duckdb_oracle.to_string(),
            cpu_oracle: cpu_oracle.to_string(),
            gpu_oracle: gpu_oracle.to_string(),
            schema_validation: schema_validation.to_string(),
        });
    }
    assert!(!rows.is_empty(), "registry CSV has no rows");
    rows
}

/// Assert this binary's inventory agrees with the CSV, in BOTH directions, for the
/// columns this binary owns.
///
/// Forward: every registration must match its CSV cell (a test that exists but is
/// recorded `disabled`/`na` fails). Reverse: every CSV cell marked `enabled`/`skip`
/// in an owned column must have a registration (a CSV row with no backing test
/// fails). Without the reverse direction the CSV could claim coverage that no test
/// provides — which is the exact failure mode this registry replaces.
///
/// Every owned column must be registered entirely within this binary. A column split
/// across binaries would need its cells listed as exceptions here, not a reverse check
/// weakened to "only what this binary happens to register".
pub(crate) fn assert_registry_matches_csv(owned_columns: &[&str]) {
    for c in owned_columns {
        assert!(COLUMNS.contains(c), "unknown column {c:?}");
    }
    let rows = load_csv();
    let csv: BTreeMap<(String, String, String), &CsvRow> = rows
        .iter()
        .map(|r| ((r.dataset.clone(), r.sf.clone(), r.query.clone()), r))
        .collect();

    let mut problems: Vec<String> = Vec::new();
    let mut registered: BTreeSet<(String, String, String, String)> = BTreeSet::new();

    // --- forward: inventory -> CSV
    for e in inventory::iter::<RegistryEntry> {
        let Some(col) = column_for(e.kind, e.device) else {
            problems.push(format!(
                "registration with unmappable kind/device: {}/{}",
                e.kind, e.device
            ));
            continue;
        };
        let key = (e.dataset.to_string(), e.sf.to_string(), e.query.to_string());
        // The FORWARD check applies to every registration this binary can see, even
        // for a column another binary owns the reverse check for: if a test exists,
        // its CSV cell must describe it correctly, full stop. Only the reverse
        // direction is ownership-scoped (this binary cannot know whether a cell it
        // does not own is backed by a test somewhere else).
        if owned_columns.contains(&col) {
            registered.insert((key.0.clone(), key.1.clone(), key.2.clone(), col.to_string()));
        }
        match csv.get(&key) {
            None => problems.push(format!(
                "test exists but NO CSV row: {} sf{} {} [{col}] — add the row to {}",
                e.dataset,
                e.sf,
                e.query,
                registry_csv_path().display()
            )),
            Some(row) => {
                let got = row.states.get(col).map(String::as_str).unwrap_or("na");
                if got != e.state {
                    problems.push(format!(
                        "state mismatch: {} sf{} {} [{col}] — test says {:?}, CSV says {got:?}",
                        e.dataset, e.sf, e.query, e.state
                    ));
                }
            }
        }
    }

    // --- reverse: CSV -> inventory
    for row in &rows {
        for col in owned_columns {
            let state = row.states.get(*col).map(String::as_str).unwrap_or("na");
            if state != "enabled" && state != "skip" {
                continue;
            }
            let key = (
                row.dataset.clone(),
                row.sf.clone(),
                row.query.clone(),
                col.to_string(),
            );
            if registered.contains(&key) {
                continue;
            }
            problems.push(format!(
                "CSV claims {state:?} but NO test registers it: {} sf{} {} [{col}] — \
                 either add the test or set the cell to disabled/na",
                row.dataset, row.sf, row.query
            ));
        }
    }

    assert!(
        problems.is_empty(),
        "registry/CSV disagreement ({} problem(s)):\n{}",
        problems.len(),
        problems.join("\n")
    );
}
