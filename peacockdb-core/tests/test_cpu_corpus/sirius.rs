//! Sirius's sf40 answers against DuckDB's: each timed row of every `sirius.tsv` in the H100's tree
//! holds the status its answer sets. build-test.md, **The sf40 answer check**.

use peacockdb_core::test_support::{
    AnswerVerdict, SiriusResults, SiriusStatus, device_cases_path, parse_sirius_results,
    replace_sirius_results, sf40_sirius_verdict, sirius_results_path, sirius_status_for,
    testdata_root, write_sirius_results,
};

use super::SF40_DEVICE;

const SIRIUS_REWRITE: &str = "UPDATE_CANONICAL=1 cargo test --features rust-only -p \
                              peacockdb-core --test test_cpu_corpus -- \
                              every_sf40_sirius_status_meets_its_answer";

/// Each timed row of every `sirius.tsv` in the H100's tree holds the status its answer sets
/// against DuckDB's at sf40: `ok` where it meets, `wrong` where it differs, `unverified` where
/// DuckDB has no sf40 answer. Under `UPDATE_CANONICAL=1` the statuses are written instead, and
/// nothing else in the file moves. No `sirius.tsv` anywhere passes, reading nothing.
#[tokio::test]
async fn every_sf40_sirius_status_meets_its_answer() {
    let update = std::env::var("UPDATE_CANONICAL").is_ok();
    let mut problems = Vec::new();
    for dataset in sirius_datasets() {
        let path = sirius_results_path(SF40_DEVICE, &dataset, "40");
        let text =
            std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let recorded = parse_sirius_results(&text);
        let mut verdicts = Vec::new();
        for row in &recorded.rows {
            verdicts.push(match row.status {
                SiriusStatus::Failed | SiriusStatus::Skipped => None,
                SiriusStatus::Ok | SiriusStatus::Wrong | SiriusStatus::Unverified => {
                    Some(sf40_sirius_verdict(&dataset, &row.query).await)
                }
            });
        }
        let (set, unset) = sirius_statuses_set(&dataset, &recorded, verdicts);
        problems.extend(unset);
        match update {
            true if set != recorded => {
                replace_sirius_results(SF40_DEVICE, &dataset, "40", &write_sirius_results(&set))
            }
            true => {}
            false => problems.extend(sirius_status_moves(&dataset, &recorded, &set)),
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

/// The datasets whose sf40 directory in the H100's tree holds a `sirius.tsv`, by name.
fn sirius_datasets() -> Vec<String> {
    let list = device_cases_path(SF40_DEVICE);
    let tree = list.parent().expect("a device directory");
    let Ok(entries) = std::fs::read_dir(tree) else {
        return Vec::new();
    };
    let mut datasets: Vec<String> = entries
        .map(|entry| entry.expect("a readable device directory").file_name())
        .filter_map(|name| Some(name.to_str()?.strip_suffix(".sf40")?.to_string()))
        .filter(|dataset| sirius_results_path(SF40_DEVICE, dataset, "40").is_file())
        .collect();
    datasets.sort();
    datasets
}

/// `recorded` with each row's status set from its verdict, one per row (`None` for a row with no
/// time), and a line for each row whose status cannot be set, which keeps its own.
fn sirius_statuses_set(
    dataset: &str,
    recorded: &SiriusResults,
    verdicts: Vec<Option<AnswerVerdict>>,
) -> (SiriusResults, Vec<String>) {
    let mut set = recorded.clone();
    let mut unset = Vec::new();
    for (row, verdict) in set.rows.iter_mut().zip(verdicts) {
        match sirius_status_for(row.status, verdict) {
            Ok(status) => row.status = status,
            Err(said) => unset.push(format!("{dataset}/{} on Sirius at sf40: {said}", row.query)),
        }
    }
    (set, unset)
}

/// A line for each row whose recorded status is not the one its answer sets.
fn sirius_status_moves(
    dataset: &str,
    recorded: &SiriusResults,
    set: &SiriusResults,
) -> Vec<String> {
    recorded
        .rows
        .iter()
        .zip(&set.rows)
        .filter(|(was, now)| was.status != now.status)
        .map(|(was, now)| {
            format!(
                "{dataset}/{} is `{}` in sirius.tsv and its answer against DuckDB's makes it \
                 `{}` — rerun with {SIRIUS_REWRITE}",
                was.query,
                spelled(was.status),
                spelled(now.status)
            )
        })
        .collect()
}

/// A status as `sirius.tsv` spells it.
fn spelled(status: SiriusStatus) -> String {
    format!("{status:?}").to_lowercase()
}

/// The shared fixture with q1, whose answer meets DuckDB's, marked `wrong`: the check names q1
/// and the rewrite, and the rewrite moves q1's status and nothing else.
#[test]
fn a_sirius_row_that_meets_duckdb_marked_wrong_is_refused() {
    let path = testdata_root().join("fixtures/sirius-results.tsv");
    let fixture =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let doctored = fixture.replacen("\tok\t", "\twrong\t", 1);
    assert_ne!(doctored, fixture);
    let recorded = parse_sirius_results(&doctored);
    let verdicts = vec![Some(AnswerVerdict::Meets), None, None];
    let (set, unset) = sirius_statuses_set("tpch", &recorded, verdicts);
    assert!(unset.is_empty(), "{unset:?}");
    let moves = sirius_status_moves("tpch", &recorded, &set);
    assert_eq!(moves.len(), 1, "{moves:?}");
    assert!(
        moves[0].contains("tpch/q1 ") && moves[0].contains("UPDATE_CANONICAL=1"),
        "{}",
        moves[0]
    );
    assert_eq!(write_sirius_results(&set), fixture);
}

/// A note written by hand on a `wrong` row, a ticket number, outlives the rewrite while the row's
/// answer still differs: the rewrite sets statuses, and nothing else in the file moves.
#[test]
fn a_wrong_sirius_rows_note_survives_the_rewrite() {
    let path = testdata_root().join("fixtures/sirius-results.tsv");
    let fixture =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let noted = fixture.replacen("\tok\t-\n", "\twrong\t#306\n", 1);
    assert_ne!(noted, fixture);
    let recorded = parse_sirius_results(&noted);
    let differs = AnswerVerdict::Differs("row 1 differs".to_string());
    let (set, unset) = sirius_statuses_set("tpch", &recorded, vec![Some(differs), None, None]);
    assert!(unset.is_empty(), "{unset:?}");
    assert!(sirius_status_moves("tpch", &recorded, &set).is_empty());
    assert_eq!(write_sirius_results(&set), noted);
}

/// tpcds q17 is a `skipped` row of the H100's list, so DuckDB's sf40 run never answered it: a
/// timed Sirius row there has nothing to be held to, which is not the same as no Sirius answer.
#[tokio::test]
async fn a_query_duckdb_has_no_sf40_answer_for_has_no_reference() {
    assert_eq!(
        sf40_sirius_verdict("tpcds", "q17").await,
        AnswerVerdict::NoReference
    );
}
