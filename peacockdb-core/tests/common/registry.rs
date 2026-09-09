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

/// One test-macro invocation, submitted at the invocation site.
///
/// `kind` + `device` determine the CSV column (see [`column_for`]); keeping them
/// separate rather than baking the column name into each macro means the mapping
/// lives in exactly one place and can be unit-tested.
#[derive(Debug)]
pub struct RegistryEntry {
    /// "bp_cpu" | "bp_gpu" — which engine ran the case.
    pub kind: &'static str,
    pub dataset: &'static str,
    pub sf: &'static str,
    /// Underscore form, as written in the macro (`shuffle_stddev`, `q12`).
    pub query: &'static str,
    /// The mode's ident, as `bp_mode` spells it (`bp_tp4_sized`).
    pub device: &'static str,
    /// "enabled" | "skip"
    pub state: &'static str,
}

inventory::collect!(RegistryEntry);

/// One `corpus_query!` line as declared, whatever it expanded to. Separate from
/// [`RegistryEntry`], which is per enabled (query, mode): this is per QUERY, and it is
/// submitted by the `none` arm too, so a declaration with no cases is still readable.
///
/// What reads it is the pairing between the two oracles, which is a property of the line
/// rather than of a run.
#[derive(Debug)]
pub struct CorpusDeclaration {
    pub dataset: &'static str,
    pub sf: &'static str,
    pub query: &'static str,
    pub cpu_oracle: &'static str,
    pub gpu_oracle: &'static str,
}

inventory::collect!(CorpusDeclaration);

/// The CSV's per-mode columns, in file order.
///
/// Three groups, one per thing that can be enabled independently. The five `bp_` columns
/// are plan enablement, and they are the one group no test macro registers: that mode's plan goldens are one file per mode rather than one
/// file per query, so what declares a cell is the golden's section for that query, and
/// `test_batch_partitioned_plans` is what holds the two to each other in both directions.
/// The `bp_cpu_` and `bp_gpu_` columns are execution, declared by `corpus_query!` through
/// this inventory — one per engine because a query can be correct at five modes on the cpu
/// and at two on a device.
///
/// Flat rather than a repeated group: the file has two independent readers, `registry.rs`
/// and `cost-report`, and both parse by header name. A repeated group would need the two to
/// agree on a decoding convention as well, which is one more place to drift.
pub const COLUMNS: [&str; 15] = [
    "bp_tp1_single",
    "bp_tp1_rowgroup",
    "bp_tp4_single",
    "bp_tp4_rowgroup",
    "bp_tp4_sized",
    "bp_cpu_tp1_single",
    "bp_cpu_tp1_rowgroup",
    "bp_cpu_tp4_single",
    "bp_cpu_tp4_rowgroup",
    "bp_cpu_tp4_sized",
    "bp_gpu_tp1_single",
    "bp_gpu_tp1_rowgroup",
    "bp_gpu_tp4_single",
    "bp_gpu_tp4_rowgroup",
    "bp_gpu_tp4_sized",
];

/// Map a registration to its CSV column: the engine it ran on and the mode it ran at,
/// composed rather than parsed off a label.
pub fn column_for(kind: &str, device: &str) -> Option<&'static str> {
    match kind {
        "bp_cpu" | "bp_gpu" => bp_column(kind, device),
        _ => None,
    }
}

/// A corpus registration's column: its engine and the mode it ran at. The mode really is
/// carried per registration — one macro invocation expands to a case per enabled mode — so
/// this composes rather than looks up, and the mode set is checked exhaustively: an
/// unlisted one is `None` and the registration is reported unmappable, rather than being
/// silently binned into whichever column a prefix match reached first.
fn bp_column(kind: &str, mode: &str) -> Option<&'static str> {
    let known = super::bp_mode::BP_MODES.iter().any(|m| m.ident() == mode);
    let suffix = known.then(|| mode.trim_start_matches("bp_"))?;
    COLUMNS
        .iter()
        .find(|column| **column == format!("{kind}_{suffix}"))
        .copied()
}

/// A parsed CSV row.
#[derive(Debug, Clone)]
pub struct CsvRow {
    pub dataset: String,
    pub sf: String,
    pub query: String,
    /// column -> state
    pub states: BTreeMap<String, String>,
    /// "ok" | "fail" — whether create_physical_plan succeeds for this query.
    pub plan_status: String,
    pub features: Vec<String>,
    pub tickets: Vec<String>,
}

/// The spelling everything but the CSV uses. The CSV's query column is a Rust identifier —
/// the macro takes it as one — so a query is `scan_limit` there and `scan-limit` in every
/// golden section, query file and case name. The two are compared in enough places that a
/// hyphenated query matching no row reads as absent rather than as wrong: `authoritative_mode`
/// returned None for the whole of T19's first batch and nothing went red. This is the one
/// conversion, called at every point the two names meet.
pub fn stem(query: &str) -> String {
    query.replace('_', "-")
}

/// The 15 hand-assigned feature codes. Not derived from SQL and not asserted
/// against it — but the SET is closed, so a typo'd code fails rather than silently
/// creating a new one-off category that renders as an unknown chip in the widget.
pub const FEATURE_CODES: [&str; 15] = [
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

pub fn registry_csv_path() -> std::path::PathBuf {
    super::testdata_root().join("cost-registry.csv")
}

/// Parse the committed registry CSV. Panics with a precise message on malformed
/// input — this is a committed fixture, so a parse failure is a bug to fix, not a
/// condition to tolerate.
pub fn load_csv() -> Vec<CsvRow> {
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
    let header: Vec<&str> = lines.next().expect("registry CSV is empty").split(',').collect();
    let expect: Vec<&str> = ["dataset", "sf", "query"]
        .into_iter()
        .chain(COLUMNS)
        .chain(["plan_status", "features", "tickets"])
        .collect();
    assert_eq!(header, expect, "registry CSV header changed; update COLUMNS to match");

    let mut rows = Vec::new();
    for (i, line) in lines.enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let f: Vec<&str> = line.split(',').collect();
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
        let features: Vec<String> =
            f[4 + COLUMNS.len()].split_whitespace().map(str::to_string).collect();
        for feat in &features {
            assert!(
                FEATURE_CODES.contains(&feat.as_str()),
                "{}:{}: unknown feature code {feat:?} — must be one of {FEATURE_CODES:?}",
                path.display(),
                i + 2
            );
        }
        let tickets: Vec<String> = f[5 + COLUMNS.len()].split_whitespace().map(str::to_string).collect();
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
        // close — the ticket is what makes it findable when the blocker clears.
        let off = COLUMNS
            .iter()
            .filter(|col| states.get(**col).is_some_and(|state| state == "disabled"))
            .count();
        assert!(
            off == 0 || !tickets.is_empty(),
            "{}:{}: {off} disabled cells and no ticket — name the one that explains them",
            path.display(),
            i + 2
        );
        rows.push(CsvRow {
            dataset: f[0].to_string(),
            sf: f[1].to_string(),
            query: f[2].to_string(),
            states,
            plan_status: plan_status.to_string(),
            features,
            tickets,
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
/// `elsewhere` lists cells `(dataset, sf, query, column)` that belong to an owned
/// column but are registered in a DIFFERENT test binary, so the reverse direction
/// must not demand them here.
///
/// No binary needs `elsewhere` today: after the split by execution mode, every
/// column is registered entirely within the one binary that owns it. The parameter
/// stays because listing exceptions explicitly — rather than weakening the reverse
/// check to "only verify what this binary happens to register" — is what keeps the
/// check meaningful when a column IS split again: a whole column going missing must
/// still fail. Stale entries are rejected below, so an empty list cannot rot.
pub fn assert_registry_matches_csv(owned_columns: &[&str], elsewhere: &[(&str, &str, &str, &str)]) {
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
            if elsewhere.iter().any(|(d, s, q, c)| {
                *d == row.dataset && *s == row.sf && *q == row.query && *c == *col
            }) {
                continue; // registered by another test binary
            }
            problems.push(format!(
                "CSV claims {state:?} but NO test registers it: {} sf{} {} [{col}] — \
                 either add the test or set the cell to disabled/na",
                row.dataset, row.sf, row.query
            ));
        }
    }

    // Keep `elsewhere` honest: an entry naming a cell that is NOT enabled/skip, or
    // that this binary actually does register, is stale and would mask a real gap.
    for (d, s, q, c) in elsewhere {
        let key = (d.to_string(), s.to_string(), q.to_string());
        let Some(row) = csv.get(&key) else {
            problems.push(format!("`elsewhere` names a query with no CSV row: {d} sf{s} {q}"));
            continue;
        };
        let state = row.states.get(*c).map(String::as_str).unwrap_or("na");
        if state != "enabled" && state != "skip" {
            problems.push(format!(
                "stale `elsewhere` entry: {d} sf{s} {q} [{c}] is {state:?}, so the reverse \
                 check would not demand it anyway — remove it"
            ));
        }
        if registered.contains(&(d.to_string(), s.to_string(), q.to_string(), c.to_string())) {
            problems.push(format!(
                "stale `elsewhere` entry: {d} sf{s} {q} [{c}] IS registered in this binary"
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
