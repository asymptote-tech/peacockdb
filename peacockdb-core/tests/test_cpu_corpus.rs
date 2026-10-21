//! The corpus on the CPU backend: every query in
//! [`corpus_cases.inc`](common/corpus_cases.inc) at every mode it declares.
//!
//! This file holds the macro and nothing else — the declarations are the include, shared
//! with the device binary so one line carries a query's coverage on both engines. What a
//! case does lives in `peacockdb_core::test_support`, reached through `cpu_case` alone.

use std::collections::BTreeSet;

use peacockdb_core::test_support::{
    CorpusDeclaration, DuckdbOracle, MODES, RegistryEntry, assert_registry_matches_csv,
    authoritative_mode, cpu_case, device_order_case, duckdb_case, duckdb_golden, duckdb_gpu_case,
    ends_in_sort, golden_row_order, gpu_result_coverage, gpu_result_cudf_matches_path,
    gpu_result_golden, line_order_is_right, load_csv, ordered_sections, result_golden,
    section_holds_rows, section_of, stem, stored_mismatch, stored_of, tight,
};

/// `corpus_query!(dataset, sf, query, cpu_modes, gpu_modes, duckdb_oracle, cpu_oracle,
/// gpu_oracle, schema_validation[(modes)])` — one test and one registration per enabled cpu
/// mode.
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
    // Each oracle is stringified whole here, outside the repetition over the modes: inside
    // it, rustc refuses an argument list that repeats once against modes that repeat five
    // times. The helpers take the spellings as expressions.
    ($dataset:ident, $sf:expr, $query:ident, $($cpu:ident)|+, $($gpu:ident)|+,
     $duck:ident $(($($duck_arg:tt)*))?, $cpu_oracle:ident $(($($cpu_arg:tt)*))?,
     $gpu_oracle:ident $(($($gpu_arg:tt)*))?, $validation:ident $(($($vmode:ident)|+))?) => {
        declare_corpus_query!($dataset, $sf, $query,
            stringify!($duck $(($($duck_arg)*))?),
            stringify!($cpu_oracle $(($($cpu_arg)*))?),
            stringify!($gpu_oracle $(($($gpu_arg)*))?),
            $validation);
        device_cell_cases!($dataset, $sf, $query, $($gpu)|+, stringify!($duck $(($($duck_arg)*))?));
        cpu_cell_cases!($dataset, $sf, $query, $($cpu)|+,
            stringify!($cpu_oracle $(($($cpu_arg)*))?), stringify!($duck $(($($duck_arg)*))?),
            stringify!($gpu_oracle $(($($gpu_arg)*))?));
    };
}

/// One test and one registration per enabled cpu mode, and none at all for `none`.
macro_rules! cpu_cell_cases {
    ($dataset:ident, $sf:expr, $query:ident, none, $cpu_oracle:expr, $duck:expr, $gpu_oracle:expr) => {};
    ($dataset:ident, $sf:expr, $query:ident, $($cpu:ident)|+, $cpu_oracle:expr, $duck:expr, $gpu_oracle:expr) => {
        $(
            paste::paste! {
                #[tokio::test]
                async fn [<cpu_ $dataset _ $query _ $cpu>]() {
                    cpu_case(
                        stringify!($dataset),
                        stringify!($sf),
                        &stringify!($query).replace('_', "-"),
                        stringify!($cpu),
                        $cpu_oracle,
                        $duck,
                        $gpu_oracle,
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
/// mode still declares three oracles and a schema validation, the pairing between the
/// oracles is a property of the line rather than of a run, and DuckDB answers whether or
/// not we do.
macro_rules! declare_corpus_query {
    ($dataset:ident, $sf:expr, $query:ident, $duck:expr, $cpu_oracle:expr, $gpu_oracle:expr, $validation:ident) => {
        inventory::submit! {
            CorpusDeclaration {
                dataset: stringify!($dataset),
                sf: stringify!($sf),
                query: stringify!($query),
                duckdb_oracle: $duck,
                cpu_oracle: $cpu_oracle,
                gpu_oracle: $gpu_oracle,
                schema_validation: stringify!($validation),
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
/// mode — a mode-dependent device answer shows nowhere else. And one `device_order_` case per
/// line with a device cell, refusing a line whose answer a device join's order can move.
macro_rules! device_cell_cases {
    ($dataset:ident, $sf:expr, $query:ident, none, $duck:expr) => {};
    ($dataset:ident, $sf:expr, $query:ident, $($gpu:ident)|+, $duck:expr) => {
        paste::paste! {
            #[tokio::test]
            async fn [<device_order_ $dataset _ $query>]() {
                device_order_case(
                    stringify!($dataset),
                    stringify!($sf),
                    &stringify!($query).replace('_', "-"),
                )
                .await;
            }
        }
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
/// having checked nothing. Device cells are enabled in bulk, so this rule is what holds it.
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

/// A line that asks DataFusion nothing compares its answer with DuckDB's.
///
/// `data_fusion_disabled` makes `assert_answer` return without comparing, so the line's cpu
/// cells check the plan, the run and the goldens and no answer. What holds the answer is the
/// line's `duckdb_<q>` case, which reads the committed authority section against DuckDB's, and
/// every mode's run is held to that section. So its oracle must compare the whole answer: a
/// `duckdb_none` would give the next line to copy the keyword unchecked
/// modes and an unverified first golden, with nothing red. The exact count makes growing the
/// set a decision rather than a copy. Nothing else needs verifying: the holder is the case the
/// line itself expands to, so it cannot be renamed away from it, and its section is held by
/// `every_duckdb_result_section_has_a_registry_row_and_every_row_a_section`.
#[test]
fn a_line_asking_datafusion_nothing_compares_its_answer_with_duckdb() {
    let mut checked = 0;
    for declared in inventory::iter::<CorpusDeclaration> {
        if tight(declared.cpu_oracle) != "data_fusion_disabled" {
            continue;
        }
        checked += 1;
        assert!(
            matches!(
                DuckdbOracle::parse(declared.duckdb_oracle),
                DuckdbOracle::Compare { .. } | DuckdbOracle::Fingerprint
            ),
            "{}/{}: data_fusion_disabled compares no cpu answer, and its duckdb_oracle {} \
             compares no whole answer either, so nothing holds it",
            declared.dataset,
            stem(declared.query),
            declared.duckdb_oracle
        );
    }
    assert_eq!(
        checked, 1,
        "tpch distinct-functions is the only line DataFusion cannot answer"
    );
}

/// What the order guard finds wrong with one line: its `duckdb(…)` and its `golden(…)` each say
/// `no_sort` exactly when the query's plan ends in a sort. A fingerprinted line has no row
/// order and is skipped whole; a `live_cpu` gpu oracle has no order of its own. `duckdb_none`
/// compares no rows with DuckDB, so only its golden half is read — and not where our side is
/// the one that does not answer, the query having no plan to read.
async fn order_findings(declared: &CorpusDeclaration) -> Vec<String> {
    let duckdb = match DuckdbOracle::parse(declared.duckdb_oracle) {
        DuckdbOracle::Fingerprint => return Vec::new(),
        DuckdbOracle::None => None,
        compared => Some(compared.row_order()),
    };
    let query = stem(declared.query);
    let ends = match ends_in_sort(declared.dataset, declared.sf, &query).await {
        Ok(ends) => ends,
        Err(_) if duckdb.is_none() => return Vec::new(),
        Err(why) => return vec![format!("  {why}")],
    };
    let spelled = [
        ("duckdb", duckdb),
        ("golden", golden_row_order(declared.gpu_oracle)),
    ];
    spelled
        .into_iter()
        .filter_map(|(which, order)| {
            let why = line_order_is_right(order?, ends).err()?;
            Some(format!("  {}/{query} ({which}): {why}", declared.dataset))
        })
        .collect()
}

/// Every line whose query's plan ends in a sort compares its rows in order, against DuckDB
/// and, live on the device, against the cpu's section — and no other line does.
///
/// Read off DataFusion's plan at one partition: the root a sort, or a limit or a projection
/// over one. A line declaring `sort` there passes a wrong row order; one declaring `no_sort`
/// elsewhere fails on an order no SQL fixed.
#[tokio::test]
async fn every_line_whose_query_ends_in_a_sort_says_no_sort() {
    let mut wrong: Vec<String> = Vec::new();
    for declared in inventory::iter::<CorpusDeclaration> {
        wrong.extend(order_findings(declared).await);
    }
    assert!(
        wrong.is_empty(),
        "{} lines spell the wrong row order:\n{}",
        wrong.len(),
        wrong.join("\n")
    );
}

/// tpch q11 ends in `ORDER BY value DESC` and is over the cap, so DuckDB's side is a
/// fingerprint, which has no row order: the guard skips it rather than asking for `no_sort`.
/// The same line spelled `duckdb(sort, exact)` is one the guard would refuse.
#[tokio::test]
async fn the_order_guard_skips_a_fingerprinted_section() {
    let q11 = inventory::iter::<CorpusDeclaration>
        .into_iter()
        .find(|d| d.dataset == "tpch" && d.query == "q11")
        .expect("tpch q11 is declared");
    assert_eq!(
        DuckdbOracle::parse(q11.duckdb_oracle),
        DuckdbOracle::Fingerprint
    );
    assert_eq!(
        ends_in_sort("tpch", "1", "q11").await,
        Ok(true),
        "q11 ends in a sort"
    );
    assert!(order_findings(q11).await.is_empty());
    let compared = CorpusDeclaration {
        duckdb_oracle: "duckdb(sort, exact)",
        ..*q11
    };
    assert_eq!(order_findings(&compared).await.len(), 1);
}

/// The golden half is held on its own: tpch q3 ends in a sort, and its line spelled with
/// `golden(sort, exact)` and its DuckDB half left right is one finding, the golden one.
#[tokio::test]
async fn the_order_guard_holds_the_golden_half_on_its_own() {
    let q3 = declared("tpch", "q3");
    assert!(order_findings(q3).await.is_empty());
    let sorted = CorpusDeclaration {
        gpu_oracle: "golden(sort, exact)",
        ..*q3
    };
    let found = order_findings(&sorted).await;
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].contains("(golden)"), "{found:?}");
}

/// A `duckdb_none` line compares no rows with DuckDB, but where we answer, its device compare
/// is a golden one and its golden half is held: tpch q3 spelled `duckdb_none` with
/// `golden(sort, exact)` is one golden finding. pbench's `in-is-null`, the one real
/// `duckdb_none` line, does not plan (#250), so it has no order to hold.
#[tokio::test]
async fn the_order_guard_reads_a_duckdb_none_lines_golden_half() {
    let none = CorpusDeclaration {
        duckdb_oracle: "duckdb_none",
        gpu_oracle: "golden(sort, exact)",
        ..*declared("tpch", "q3")
    };
    let found = order_findings(&none).await;
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].contains("(golden)"), "{found:?}");
    let in_is_null = declared("pbench", "in_is_null");
    assert_eq!(
        DuckdbOracle::parse(in_is_null.duckdb_oracle),
        DuckdbOracle::None
    );
    assert!(ends_in_sort("pbench", "1", "in-is-null").await.is_err());
    assert!(order_findings(in_is_null).await.is_empty());
}

fn declared(dataset: &str, query: &str) -> &'static CorpusDeclaration {
    inventory::iter::<CorpusDeclaration>
        .into_iter()
        .find(|d| d.dataset == dataset && d.query == query)
        .unwrap_or_else(|| panic!("{dataset}/{query} is declared"))
}

/// The two oracles of one line have to suit each other, and both directions are asserted
/// rather than trusted.
///
/// A `golden(…)` where no committed section can serve fails on correct behaviour: the
/// result is over the cap and holds a fingerprint instead of its rows. A `live_cpu` where a
/// section does serve spends a device-side cpu run on a comparison a committed file makes
/// faster and harder. One question, asked both ways, and the same one `corpus_gpu`'s
/// device-side check asks — whether a section serves. Derivable is why a CHECK can exist here,
/// never why either value would be absent from the line; read off the declaration and the
/// golden, it needs no run, which is what catches the first `live_cpu` query before its rollout.
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
        // Read off what is committed, not off `cpu_oracle`. A `golden(sort, exact)` on a
        // `data_fusion_subset` line is the extra claim that our own five modes agree, which only
        // the device golden compare tests. `tpch/scan-limit`'s do: #186 cut it on one lane over
        // the covering prefix in order, so its five plans are identical. Measured 2026-10-09,
        // before chain J's master; the next device run proves it again.
        // The guard above excludes the query no mode enables, so a section standing in for
        // its rows here stands in for them because the cap kept them out.
        let needs_live = !section_holds_rows(&section);
        let says_live = tight(declared.gpu_oracle) == "live_cpu";
        if needs_live && !says_live {
            wrong.push(format!(
                "{}/{query}: gpu_oracle is {} where the committed section is a marker, \
                 so it fails on correct behaviour",
                declared.dataset, declared.gpu_oracle,
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

/// A line whose rows are undetermined compares them against COMMITTED files, and only there.
///
/// `data_fusion_subset` says DataFusion may answer a different ten rows per run, so
/// `duckdb(sort, exact)` over it looks like a latent flake and is not one — in the `duckdb_<q>`
/// case, which reads the committed `mini.result.txt` authority against the committed
/// `duckdb-result.txt`, both holding `lineitem`'s first ten rows in file order. Only a
/// regeneration moves that. `duckdb_gpu_<q>_<mode>` applies the same oracle to the device's
/// RECORDED answer, which has that footing only where the line is `golden(sort, exact)`: the device
/// case then holds the recorded rows to the committed section at every mode, so the answer
/// DuckDB judges is the committed one again. Since #186 our five modes cut scan-limit on one
/// lane over the covering prefix in order, which is what lets its device cells be on. A line
/// whose device side needed a live cpu would hand DuckDB ten rows no committed file pins.
#[test]
fn an_undetermined_lines_rows_are_compared_only_against_committed_files() {
    let rows = load_csv();
    let mut checked = 0;
    for declared in inventory::iter::<CorpusDeclaration> {
        if tight(declared.cpu_oracle) != "data_fusion_subset" {
            continue;
        }
        checked += 1;
        // Weakening is not an option a reader should reach for: `duckdb_fingerprint` wants an
        // over-cap section, and `duckdb_none` fails by construction while both sides answer.
        assert!(
            matches!(
                DuckdbOracle::parse(declared.duckdb_oracle),
                DuckdbOracle::Compare { .. }
            ),
            "{}/{}: its rows are undetermined and its duckdb_oracle is {} — the comparison is \
             against two committed files, so it compares rows or it compares nothing",
            declared.dataset,
            declared.query,
            declared.duckdb_oracle
        );
        // Off the registry, as the loop above reads it: a `CorpusDeclaration` carries the
        // oracles and not the modes, and the `gpu_` columns are what `device_cell_cases!`
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
            live.is_empty() || tight(declared.gpu_oracle) == "golden(sort,exact)",
            "{}/{}: its rows are undetermined, its device cells at {live:?} are on and its \
             gpu_oracle is {}, so duckdb_gpu_* holds the device's RECORDED answer to {} — ten \
             unordered rows no committed section pins. Hold the device to the committed \
             section with golden(sort, exact), compare the row count alone, or leave the cells \
             off.",
            declared.dataset,
            stem(declared.query),
            declared.gpu_oracle,
            declared.duckdb_oracle
        );
    }
    assert_eq!(checked, 1, "tpch scan-limit is the only undetermined line");
}

/// The registry's last four columns are a copy of each `corpus_query!` line's oracles and
/// schema validation, held to the lines in both directions: every line's row says what the
/// line says, and every row that names an oracle has a line. The widget reads the copy, so a
/// line edited without its row would show a reader an oracle no test uses.
#[test]
fn the_registry_carries_each_lines_oracles_and_schema_validation() {
    let rows = load_csv();
    let mut wrong: Vec<String> = Vec::new();
    let mut declared = std::collections::BTreeSet::new();
    for line in inventory::iter::<CorpusDeclaration> {
        let key = (line.dataset, line.sf, line.query);
        declared.insert(key);
        let validation = line
            .schema_validation
            .trim_start_matches("schema_validation_");
        match rows
            .iter()
            .find(|r| (r.dataset.as_str(), r.sf.as_str(), r.query.as_str()) == key)
        {
            None => wrong.push(format!(
                "{}/{}: a line and no registry row",
                line.dataset, line.query
            )),
            Some(row) => {
                let got = (
                    tight(&row.duckdb_oracle),
                    tight(&row.cpu_oracle),
                    tight(&row.gpu_oracle),
                    row.schema_validation.clone(),
                );
                let want = (
                    tight(line.duckdb_oracle),
                    tight(line.cpu_oracle),
                    tight(line.gpu_oracle),
                    validation.to_string(),
                );
                if got != want {
                    wrong.push(format!(
                        "{}/{}: the line says {want:?}, the row {got:?}",
                        line.dataset, line.query
                    ));
                }
            }
        }
    }
    for row in &rows {
        let named = row.cpu_oracle != "na";
        if named && !declared.contains(&(row.dataset.as_str(), row.sf.as_str(), row.query.as_str()))
        {
            wrong.push(format!(
                "{}/{}: the row names {} / {} / {} and no corpus_query! line declares it",
                row.dataset, row.query, row.duckdb_oracle, row.cpu_oracle, row.gpu_oracle
            ));
        }
    }
    assert!(!declared.is_empty(), "no corpus_query! line was collected");
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
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

/// Every facet of the DuckDB grammar is named by some line, so one the corpus does not use is
/// deleted rather than carried. Read off the registry's copy of each line's oracle, which the
/// test above holds to the lines. The three oracle grammars are each held to the lines this
/// way — `every_oracle_variant_is_named_by_some_line` does the other two.
#[test]
fn every_duckdb_oracle_is_named_by_some_line() {
    let named: BTreeSet<&str> = load_csv()
        .iter()
        .filter(|row| row.duckdb_oracle != "na")
        .flat_map(|row| DuckdbOracle::parse(&row.duckdb_oracle).facets())
        .collect();
    for facet in DuckdbOracle::FACETS {
        assert!(
            named.contains(facet),
            "{facet} is named by no corpus_query! line — delete it rather than keep it"
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

/// `duckdb-result.txt`'s sections against the registry's rows, in both directions.
///
/// The `duckdb_<ds>_<q>` cases read one section each, so a section whose query has left the
/// tree is read by nobody: it survives every run and a regeneration deletes it, which makes
/// the committed file and `duckdb_result.py`'s output differ with nothing red. The plan
/// goldens are held to the registry the same way, by
/// `the_registry_matches_the_goldens_in_both_directions`.
#[test]
fn every_duckdb_result_section_has_a_registry_row_and_every_row_a_section() {
    for (dataset, sf) in registry_datasets() {
        let path = duckdb_golden(&dataset, &sf);
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
        let sections: BTreeSet<String> = ordered_sections(&text)
            .into_iter()
            .map(|(query, _)| query)
            .collect();
        let rows: BTreeSet<String> = load_csv()
            .iter()
            .filter(|row| row.dataset == dataset && row.sf == sf)
            .map(|row| stem(&row.query))
            .collect();
        let orphans: Vec<&String> = sections.difference(&rows).collect();
        let missing: Vec<&String> = rows.difference(&sections).collect();
        assert!(
            orphans.is_empty() && missing.is_empty(),
            "{dataset}: duckdb-result.txt has sections with no registry row {orphans:?} and \
             rows with no section {missing:?} — regenerate with `python3 \
             testdata/duckdb_result.py --dataset {dataset}`"
        );
    }
}

/// The registry's `stored` column against the three result goldens, in both directions: each
/// corpus line's cell says what its cpu, DuckDB and device sections hold, a row with no line
/// says `na`, and every query with a section in any of the three files has a row. The widget
/// shows the cell, so a section that crossed the cap with the cell unchanged would tell a
/// reader the cpu is held to rows it no longer stores.
#[test]
fn the_registry_stored_column_matches_the_goldens_in_both_directions() {
    let rows = load_csv();
    let mut wrong: Vec<String> = Vec::new();
    for row in &rows {
        let query = stem(&row.query);
        if row.cpu_oracle == "na" {
            if row.stored != "na" {
                wrong.push(format!(
                    "{}/{query}: no corpus line, and the registry says {} — write na",
                    row.dataset, row.stored
                ));
            }
            continue;
        }
        let want = stored_of(&row.dataset, &row.sf, &query);
        wrong.extend(stored_mismatch(&row.dataset, &query, &row.stored, &want));
    }
    for (dataset, sf) in registry_datasets() {
        let named: BTreeSet<String> = rows
            .iter()
            .filter(|row| row.dataset == dataset && row.sf == sf)
            .map(|row| stem(&row.query))
            .collect();
        for path in [
            result_golden(&dataset, &sf),
            duckdb_golden(&dataset, &sf),
            gpu_result_golden(&dataset, &sf, None),
        ] {
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            for (header, _) in ordered_sections(&text) {
                let query = header.split(" mode=").next().unwrap_or(&header).to_string();
                if !named.contains(&query) {
                    wrong.push(format!(
                        "{}: a section for {query} and no registry row",
                        path.display()
                    ));
                }
            }
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
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

/// The committed `gpu-result.txt` holds cuDF 25.02's answers, recorded on nebius-gpu's L40S
/// while shad-gpu was down, and carries the
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
