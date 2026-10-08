//! The corpus on the CPU backend: every query in
//! [`corpus_cases.inc`](common/corpus_cases.inc) at every mode it declares.
//!
//! This file holds the macro and nothing else — the declarations are the include, shared
//! with the device binary so one line carries a query's coverage on both engines. What a
//! case does lives in `peacockdb_core::test_support`, reached through `cpu_case` alone.

use std::collections::BTreeSet;

use peacockdb_core::test_support::{
    CorpusDeclaration, DuckdbOracle, MODES, Mode, RegistryEntry, assert_registry_matches_csv,
    authoritative_mode, corpus_lines, cpu_case, duckdb_case, duckdb_gpu_case, gpu_result_coverage,
    gpu_result_cudf_matches_path, gpu_result_golden, load_csv, result_golden, section_holds_rows,
    section_of, stem,
};

/// `corpus_query!(dataset, sf, query, cpu_modes, gpu_modes, duckdb_oracle, cpu_oracle,
/// gpu_oracle, schema_validation)` — one test and one registration per enabled cpu mode.
/// The modes read as a bitwise or and are matched as idents, which is what lets the expansion
/// produce a case per mode rather than one that decides at run time whether it is one: a
/// disabled mode has no test to name and no registration to explain. `all_modes` is the five
/// spelled out, expanded by two arms that recurse, so the sugar is a rewrite of the line.
///
/// The device's three arguments are consumed and dropped here. That is the point of one list:
/// this binary cannot silently disagree with the other about which query exists.
macro_rules! corpus_query {
    ($dataset:ident, $sf:expr, $query:ident, all_modes, $($rest:tt)*) => {
        corpus_query!($dataset, $sf, $query,
            tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, $($rest)*);
    };
    ($dataset:ident, $sf:expr, $query:ident, $($cpu:ident)|+, all_modes, $($rest:tt)*) => {
        corpus_query!($dataset, $sf, $query, $($cpu)|+,
            tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, $($rest)*);
    };
    ($dataset:ident, $sf:expr, $query:ident, none, $($gpu:ident)|+, $duck:ident $(($($duck_arg:literal),*))?, $cpu_oracle:ident, $gpu_oracle:ident, $validation:ident) => {
        declare_corpus_query!($dataset, $sf, $query, stringify!($duck $(($($duck_arg),*))?), $cpu_oracle, $gpu_oracle);
        duckdb_device_cases!($dataset, $sf, $query, $($gpu)|+, stringify!($duck $(($($duck_arg),*))?));
    };
    ($dataset:ident, $sf:expr, $query:ident, $($cpu:ident)|+, $($gpu:ident)|+, $duck:ident $(($($duck_arg:literal),*))?, $cpu_oracle:ident, $gpu_oracle:ident, $validation:ident) => {
        declare_corpus_query!($dataset, $sf, $query, stringify!($duck $(($($duck_arg),*))?), $cpu_oracle, $gpu_oracle);
        duckdb_device_cases!($dataset, $sf, $query, $($gpu)|+, stringify!($duck $(($($duck_arg),*))?));
        $(
            paste::paste! {
                #[tokio::test]
                async fn [<cpu_ $dataset _ $query _ $cpu>]() {
                    cpu_case(
                        stringify!($dataset),
                        stringify!($sf),
                        &stringify!($query).replace('_', "-"),
                        stringify!($cpu),
                        stringify!($cpu_oracle),
                    )
                    .await;
                }
            }
            inventory::submit! {
                RegistryEntry {
                    kind: "cpu",
                    dataset: stringify!($dataset),
                    sf: stringify!($sf),
                    query: stringify!($query),
                    device: stringify!($cpu),
                    state: "enabled",
                }
            }
        )+
    };
}

/// The line itself and its DuckDB case, submitted by both arms — a query with no enabled
/// mode still declares three oracles, the pairing between them is a property of the line
/// rather than of a run, and DuckDB answers whether or not we do.
macro_rules! declare_corpus_query {
    ($dataset:ident, $sf:expr, $query:ident, $duck:expr, $cpu_oracle:ident, $gpu_oracle:ident) => {
        inventory::submit! {
            CorpusDeclaration {
                dataset: stringify!($dataset),
                sf: stringify!($sf),
                query: stringify!($query),
                duckdb_oracle: $duck,
                cpu_oracle: stringify!($cpu_oracle),
                gpu_oracle: stringify!($gpu_oracle),
            }
        }
        paste::paste! {
            #[tokio::test]
            async fn [<duckdb_ $dataset _ $query>]() {
                duckdb_case(
                    stringify!($dataset),
                    stringify!($sf),
                    &stringify!($query).replace('_', "-"),
                    $duck,
                )
                .await;
            }
        }
    };
}

/// One device case per ENABLED gpu mode, and none at all for `none`: `gpu-result.txt` is
/// keyed by (query, mode), so every device cell meets DuckDB rather than only the last
/// mode — a mode-dependent device answer shows nowhere else.
macro_rules! duckdb_device_cases {
    ($dataset:ident, $sf:expr, $query:ident, none, $duck:expr) => {};
    ($dataset:ident, $sf:expr, $query:ident, $($gpu:ident)|+, $duck:expr) => {
        $(
            paste::paste! {
                #[tokio::test]
                async fn [<duckdb_gpu_ $dataset _ $query _ $gpu>]() {
                    duckdb_gpu_case(
                        stringify!($dataset),
                        stringify!($sf),
                        &stringify!($query).replace('_', "-"),
                        stringify!($gpu),
                        $duck,
                    )
                    .await;
                }
            }
        )+
    };
}

include!("common/corpus_cases.inc");

/// A device cell exists only where the cpu has one at the same mode.
///
/// The device tier asserts read-only against the section the cpu authored AT THAT MODE, so a
/// gpu cell whose cpu twin is off compares against a section holding no rows and passes
/// having checked nothing. Today it holds because a few device cells were hand-chosen, not
/// because of a rule, and the moment [#152] clears somebody enables device modes in bulk.
///
/// Read off the registry rather than the declarations: `CorpusDeclaration` carries the
/// oracles and not the modes, and a disabled mode submits no registration to compare.
#[test]
fn every_device_cell_has_a_cpu_cell_at_the_same_mode() {
    let mut wrong: Vec<String> = Vec::new();
    let mut checked = 0;
    for row in load_csv() {
        for mode in &MODES {
            let suffix = mode.ident();
            let live = |prefix: &str| {
                row.states
                    .get(&format!("{prefix}{suffix}"))
                    .is_some_and(|s| s == "enabled" || s == "skip")
            };
            checked += 1;
            if live("gpu_") && !live("cpu_") {
                wrong.push(format!("{}/{} at {}", row.dataset, row.query, mode.name));
            }
        }
    }
    assert!(
        wrong.is_empty(),
        "these device cells have no cpu cell at the same mode, so each compares against a \
         marker and passes having checked nothing: {wrong:?}"
    );
    assert_eq!(checked, load_csv().len() * MODES.len());
}

/// The two oracles of one line have to suit each other, and both directions are asserted
/// rather than trusted.
///
/// A `golden_exact` where no committed section can serve fails on correct behaviour: the
/// result is over the cap and holds a fingerprint instead of its rows, or its rows are not
/// determined across modes and one mode's answer cannot be the authority for five. A
/// `live_cpu` where a section does serve spends a device-side cpu run on a comparison a
/// committed file makes faster and harder. Derivable is why a CHECK can exist here, never
/// why either value would be absent from the line; read off the declaration and the golden,
/// it needs no run, which is what catches the first `live_cpu` query before its rollout.
#[test]
fn each_declarations_two_oracles_suit_each_other() {
    let mut wrong: Vec<String> = Vec::new();
    for declared in inventory::iter::<CorpusDeclaration> {
        let query = stem(declared.query);
        let authority = authoritative_mode(declared.dataset, declared.sf, &query);
        // A query with no enabled mode has no result section to reason about, and its
        // oracles are inert until one is enabled.
        if authority.is_none() {
            continue;
        }
        let section = section_of(&result_golden(declared.dataset, declared.sf), &query);
        // The two conditions the entry names, read off what is committed and off the line.
        // The guard above excludes the query no mode enables, so a section standing in for
        // its rows here stands in for them because the cap kept them out.
        let over_cap = !section_holds_rows(&section);
        // `data_fusion_subset` says the ENGINE is free to answer a different row set per run,
        // which is why the device needs a live cpu. Its DuckDB oracle is a separate question —
        // see `an_undetermined_lines_rows_are_compared_only_against_committed_files`.
        let undetermined = declared.cpu_oracle == "data_fusion_subset";
        let needs_live = over_cap || undetermined;
        let says_live = declared.gpu_oracle == "live_cpu";
        if needs_live && !says_live {
            wrong.push(format!(
                "{}/{query}: gpu_oracle is {} where no committed section can serve it ({}), \
                 so it fails on correct behaviour",
                declared.dataset,
                declared.gpu_oracle,
                match (over_cap, undetermined) {
                    (true, true) => "the result is over the cap AND its rows are undetermined",
                    (true, false) => "the result is over the cap",
                    _ => "cpu_oracle is data_fusion_subset, so the modes need not agree",
                }
            ));
        }
        if says_live && !needs_live {
            wrong.push(format!(
                "{}/{query}: gpu_oracle is live_cpu and `.result.txt` holds this query's rows \
                 — a device-side cpu run per mode for what the committed section already says",
                declared.dataset
            ));
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// A line whose rows are undetermined compares them against two COMMITTED files, and only
/// there.
///
/// `data_fusion_subset` says the engine may answer a different ten rows per run, so
/// `duckdb_exact` over it looks like a latent flake and is not one — in the `duckdb_<q>`
/// case, which reads the committed `mini.result.txt` authority against the committed
/// `duckdb-result.txt`, both holding `lineitem`'s first ten rows in file order. Only a
/// regeneration moves that. `duckdb_gpu_<q>_<mode>` is the other case and has no such
/// footing: it applies the same oracle to the device's RECORDED answer, where ten unordered
/// rows need not be the cpu's ten. So this holds such a line's device cells off, and the day
/// [#186] turns them on it is what says to weaken the device-side comparison instead.
#[test]
fn an_undetermined_lines_rows_are_compared_only_against_committed_files() {
    let rows = load_csv();
    let mut checked = 0;
    for declared in inventory::iter::<CorpusDeclaration> {
        if declared.cpu_oracle != "data_fusion_subset" {
            continue;
        }
        checked += 1;
        // Weakening is not an option a reader should reach for: of the five oracles
        // `duckdb_approx` still wants the same multiset, `duckdb_divergent` wants an open
        // ticket and columns that really differ, `duckdb_fingerprint` wants an over-cap
        // section, and `duckdb_none` fails by construction while both sides answer.
        assert!(
            matches!(declared.duckdb_oracle, "duckdb_exact" | "duckdb_approx"),
            "{}/{}: its rows are undetermined and its duckdb_oracle is {} — the comparison is \
             against two committed files, so it compares rows or it compares nothing",
            declared.dataset,
            declared.query,
            declared.duckdb_oracle
        );
        // Off the registry, as the loop above reads it: a `CorpusDeclaration` carries the
        // oracles and not the modes, and the `gpu_` columns are what `duckdb_device_cases!`
        // expands from — held to that macro by the gpu binary's own registry check.
        let row = rows
            .iter()
            .find(|r| {
                r.dataset == declared.dataset && r.sf == declared.sf && r.query == declared.query
            })
            .unwrap_or_else(|| {
                panic!(
                    "{}/{}: not in the registry",
                    declared.dataset,
                    stem(declared.query)
                )
            });
        let live: Vec<&str> = MODES
            .iter()
            .filter(|mode| {
                row.states
                    .get(&format!("gpu_{}", mode.ident()))
                    .is_some_and(|state| state == "enabled" || state == "skip")
            })
            .map(|mode| mode.name)
            .collect();
        assert!(
            live.is_empty(),
            "{}/{}: its rows are undetermined and its device cells at {live:?} are on, so \
             duckdb_gpu_* holds the device's RECORDED answer to {} — ten unordered rows that \
             need not be the ten the cpu committed. Compare the row count alone on the device \
             side of such a line, or leave the cells off.",
            declared.dataset,
            stem(declared.query),
            declared.duckdb_oracle
        );
    }
    assert_eq!(checked, 1, "tpch scan-limit is the only undetermined line");
}

/// A hyphenated query resolves its authority, and that authority is what silence would cost.
///
/// The CSV spells a query as an identifier and every golden section spells it with hyphens, so
/// a reader comparing the two directly finds no row and answers None — and None is a legal
/// answer here, meaning "no mode is enabled". So nothing writes the query's `.result.txt`
/// section, nothing reads it, and the test above excuses the query at its `authority.is_none()`
/// guard. T18's twenty are all `qNN` and cannot reach it; T19's first batch is five queries that
/// can, `scan-limit` among them — which is the query that test was written for.
#[test]
fn a_hyphenated_query_resolves_its_authority_and_has_its_result_section() {
    let rows = load_csv();
    let mut checked = 0;
    for declared in inventory::iter::<CorpusDeclaration> {
        let query = stem(declared.query);
        if query == declared.query {
            continue;
        }
        let row = rows
            .iter()
            .find(|r| {
                r.dataset == declared.dataset && r.sf == declared.sf && r.query == declared.query
            })
            .unwrap_or_else(|| {
                panic!(
                    "{}/{query}: declared and not in the registry",
                    declared.dataset
                )
            });
        // A query enabled at no mode has no authority to resolve, which is the same None for
        // an entirely different reason — the one this test exists to tell apart.
        if !row
            .states
            .iter()
            .any(|(col, state)| col.starts_with("cpu_") && state == "enabled")
        {
            continue;
        }
        let authority = authoritative_mode(declared.dataset, declared.sf, &query);
        assert!(
            authority.is_some(),
            "{}/{query} is enabled and resolves no authoritative mode. The registry spells it \
             {} and this asked for {query}, so the row was never found — no result section is \
             written, nothing checks it, and the oracle pairing above excuses the query.",
            declared.dataset,
            declared.query
        );
        section_of(&result_golden(declared.dataset, declared.sf), &query);
        checked += 1;
    }
    // The exact count rather than a floor: a floor of one passes on the day all but one
    // hyphenated query stops being checked, and the set is derivable from the same two
    // sources the loop reads.
    let expected = inventory::iter::<CorpusDeclaration>
        .into_iter()
        .filter(|d| stem(d.query) != d.query)
        .filter(|d| {
            rows.iter()
                .find(|r| r.dataset == d.dataset && r.sf == d.sf && r.query == d.query)
                .is_some_and(|r| {
                    r.states
                        .iter()
                        .any(|(col, state)| col.starts_with("cpu_") && state == "enabled")
                })
        })
        .count();
    assert_eq!(
        checked, expected,
        "every enabled hyphenated query is checked, and only those"
    );
}

/// `all_modes` expands to the same cells as the five spelled out, in either position — and
/// a line whose five cells are on says `all_modes` rather than respelling them.
///
/// Three places read the sugar: this binary's macro, the device binary's, and
/// `benchmark.rs::modes()`, which parses the include as text. None of them writes
/// `cost-registry.csv`, so the CSV is what the claim is checked against. For the cpu
/// position the inventory is a third witness —
/// `the_registry_matches_the_cpu_corpus_in_both_directions` ties what this binary actually
/// expanded to the same CSV — and the gpu position is read off the CSV alone here, since
/// `inventory` collects per linked binary and the device registrations are in the other one.
#[test]
fn all_modes_expands_to_the_five_in_either_position() {
    let five: BTreeSet<String> = MODES.iter().map(Mode::ident).collect();
    let rows = load_csv();
    let mut checked = 0;
    for line in corpus_lines() {
        let (dataset, query) = (&line[0], &line[2]);
        let row = rows
            .iter()
            .find(|row| &row.dataset == dataset && &row.query == query)
            .unwrap_or_else(|| panic!("{dataset}/{query} is declared and not in the registry"));
        for (position, prefix) in [(3, "cpu_"), (4, "gpu_")] {
            let argument = &line[position];
            let declared: BTreeSet<String> = match argument.as_str() {
                "none" => BTreeSet::new(),
                "all_modes" => five.clone(),
                named => named.split('|').map(|m| m.trim().to_string()).collect(),
            };
            let live: BTreeSet<String> = MODES
                .iter()
                .map(Mode::ident)
                .filter(|mode| {
                    row.states
                        .get(&format!("{prefix}{mode}"))
                        .is_some_and(|state| state == "enabled" || state == "skip")
                })
                .collect();
            assert_eq!(
                declared, live,
                "{dataset}/{query}: the line's {prefix}modes are {argument} and the registry \
                 says {live:?}"
            );
            assert!(
                live != five || argument == "all_modes",
                "{dataset}/{query}: the line spells all five {prefix}modes out — say all_modes"
            );
            checked += 1;
        }
    }
    assert_eq!(
        checked,
        corpus_lines().len() * 2,
        "both positions of every line"
    );
}

/// Every `DuckdbOracle` variant is named by some line, so a variant the corpus does not use
/// is deleted rather than carried. The three oracle enums are each held to the lines this
/// way — `every_oracle_variant_is_named_by_some_line` does the other two.
#[test]
fn every_duckdb_oracle_is_named_by_some_line() {
    for variant in DuckdbOracle::ALL {
        let name = variant.name();
        assert!(
            inventory::iter::<CorpusDeclaration>
                .into_iter()
                .any(|declared| DuckdbOracle::parse(declared.duckdb_oracle).name() == name),
            "{name} is named by no corpus_query! line — delete it rather than keep it"
        );
    }
}

/// Every enabled device cell has its `gpu-result.txt` section and no section is any other
/// cell's, in both directions, over the datasets the registry holds.
///
/// A task that turns a device cell on or off without running the cycle that records its
/// answer fails here, which is the whole reason the file is compared with DuckDB by (query,
/// mode) rather than by query. An ABSENT file fails where some cell is enabled — no cycle has
/// written it since the cells moved — but a dataset whose device cells are all off has no file
/// and wants none, the writer running only from a device case. Regenerating clears either
/// failure both ways: the writer keeps what the registry enables and drops what it does not
/// (`merged_cells`).
#[test]
fn every_enabled_device_cell_has_its_gpu_result_section_and_no_other() {
    for (dataset, sf) in registry_datasets() {
        let path = gpu_result_golden(&dataset, &sf, None);
        let text = std::fs::read_to_string(&path).ok();
        let enabled: BTreeSet<(String, String)> = load_csv()
            .iter()
            .filter(|row| row.dataset == dataset && row.sf == sf)
            .flat_map(|row| {
                MODES.iter().filter_map(move |mode| {
                    let state = row.states.get(&format!("gpu_{}", mode.ident()));
                    state
                        .is_some_and(|state| state == "enabled" || state == "skip")
                        .then(|| (stem(&row.query), mode.name.to_string()))
                })
            })
            .collect();
        if let Err(said) = gpu_result_coverage(&path, &enabled, text.as_deref()) {
            panic!("{dataset}: {said}");
        }
    }
}

/// Every (dataset, sf) the registry holds, so a dataset a later task adds is covered by the
/// guards above without an edit here. It landing with its device cells off is the ordinary
/// case, and then it has no `gpu-result.txt` and wants none.
fn registry_datasets() -> BTreeSet<(String, String)> {
    load_csv()
        .iter()
        .map(|row| (row.dataset.clone(), row.sf.clone()))
        .collect()
}

/// The committed `gpu-result.txt` holds cuDF 25.02's answers — shad-gpu's — and carries the
/// version it was recorded under on its first line, so a 26.02 cycle that recorded over it is
/// a test failure rather than a diff nobody read. Step 4's "one version per file".
///
/// A file that does not exist is not this test's failure: the coverage guard above owns
/// absence, and two cases red for one reason buy nothing. The rule itself is held by
/// `a_recorded_file_must_carry_the_cudf_its_name_promises` in the rust-only tier, over text a
/// test can doctor today.
#[test]
fn every_committed_gpu_result_file_carries_the_committed_cudfs_stamp() {
    for (dataset, sf) in registry_datasets() {
        let path = gpu_result_golden(&dataset, &sf, None);
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        gpu_result_cudf_matches_path(&text, None)
            .unwrap_or_else(|said| panic!("{}: {said}", path.display()));
    }
}

/// The five `cpu_` columns against what this binary declares, in both directions: a
/// registration whose cell says otherwise, and a cell no case backs, both fail. The device
/// half is checked in the device binary, because `inventory` collects per linked binary.
#[test]
fn the_registry_matches_the_cpu_corpus_in_both_directions() {
    assert_registry_matches_csv(&[
        "cpu_tp1_single",
        "cpu_tp1_rowgroup",
        "cpu_tp4_single",
        "cpu_tp4_rowgroup",
        "cpu_tp4_sized",
    ]);
}
