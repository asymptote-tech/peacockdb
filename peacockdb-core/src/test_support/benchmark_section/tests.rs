use std::cell::Cell;
use std::path::PathBuf;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::Duration;

use super::{case_timeout_asked, measure_case, timeout_record, watch_case};
use crate::test_support::{
    CaseOutcome, FAILED, Provenance, Regeneration, Resume, merge_section, ordered_sections,
    parse_benchmark_section,
};

fn this_run() -> Provenance {
    Provenance {
        device: "NVIDIA H100 80GB HBM3".to_string(),
        driver: "580.95.05".to_string(),
        cuda: "12.9".to_string(),
        cudf: "25.02.02".to_string(),
        host: "gpu-host".to_string(),
    }
}

fn scratch(case: &str) -> (PathBuf, PathBuf) {
    let root = std::env::temp_dir().join(format!("pck-watch-{case}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let path = root.join("h100-80gb-hbm3/tpch.sf40/tp1_single.benchmark.txt");
    (root, path)
}

fn declared(queries: &[&str]) -> Vec<(String, Option<String>)> {
    queries.iter().map(|q| (q.to_string(), None)).collect()
}

/// The record a timed-out case leaves is a failed case's record: the reason on the `FAILED`
/// line, then the five, and no trailer, so the tree's reader takes it as no measurement.
#[test]
fn the_timeout_record_is_a_failed_record() {
    let body = timeout_record(1800, &this_run());
    assert_eq!(
        body,
        format!(
            "{FAILED}timeout: the case ran past 1800 s\ndevice=NVIDIA H100 80GB HBM3\n\
             driver=580.95.05\ncuda=12.9\ncudf=25.02.02\nhost=gpu-host\n"
        )
    );
    assert!(parse_benchmark_section(&body).is_none());
}

/// A case still running at its timeout: its section becomes the timeout's record, the other
/// sections of the file stay, and only then does the watch end the process (an abort in a
/// benchmark run; here, the file as `expire` finds it, sent back).
#[test]
fn a_case_past_its_timeout_is_recorded_failed() {
    let (root, path) = scratch("past");
    let both = declared(&["q1", "q6"]);
    merge_section(&path, &both, "q1", "q1's section\n", Regeneration::Sections);
    let (expired, heard) = mpsc::channel();
    let at_expiry = path.clone();
    let watch = watch_case(
        1,
        path.clone(),
        both,
        "q6".to_string(),
        this_run(),
        move || {
            let text = std::fs::read_to_string(&at_expiry).unwrap_or_default();
            expired.send(text).expect("the test listens");
        },
    );
    let heard = heard.recv_timeout(Duration::from_secs(20));
    drop(watch);
    let _ = std::fs::remove_dir_all(&root);
    let text = heard.expect("the watch never expired");
    assert_eq!(
        ordered_sections(&text),
        vec![
            ("q1".to_string(), "q1's section\n".to_string()),
            ("q6".to_string(), timeout_record(1, &this_run())),
        ]
    );
}

/// A case that ends inside its timeout drops its watch, which then writes nothing and never
/// ends the process.
#[test]
fn a_case_within_its_timeout_records_nothing() {
    let (root, path) = scratch("within");
    let (expired, heard) = mpsc::channel();
    let watch = watch_case(
        1,
        path.clone(),
        declared(&["q6"]),
        "q6".to_string(),
        this_run(),
        move || expired.send(()).expect("the test listens"),
    );
    drop(watch);
    let heard = heard.recv_timeout(Duration::from_secs(3));
    let written = path.exists();
    let _ = std::fs::remove_dir_all(&root);
    assert_eq!(heard, Err(RecvTimeoutError::Disconnected));
    assert!(
        !written,
        "a case inside its timeout has its section written by the case"
    );
}

/// `PEACOCK_BENCH_CASE_TIMEOUT_S` is whole seconds, or unset (empty reads as unset, as the other
/// superset-env variables do).
#[test]
fn a_case_timeout_is_whole_seconds_or_unset() {
    assert_eq!(case_timeout_asked(None), None);
    assert_eq!(case_timeout_asked(Some("")), None);
    assert_eq!(case_timeout_asked(Some("1800")), Some(1800));
}

#[test]
#[should_panic(expected = "PEACOCK_BENCH_CASE_TIMEOUT_S=30m")]
fn a_case_timeout_in_other_units_is_refused() {
    case_timeout_asked(Some("30m"));
}

#[test]
#[should_panic(expected = "PEACOCK_BENCH_CASE_TIMEOUT_S=0")]
fn a_zero_case_timeout_is_refused() {
    case_timeout_asked(Some("0"));
}

// --- what a measured case writes before its section ---------------------------------------

fn q6() -> Vec<(String, Option<String>)> {
    vec![("q6".to_string(), None)]
}

/// Recorded after the section, a killed or refused recording would leave a section a resumed run
/// keeps for ever, with no answer to hold to DuckDB's.
#[tokio::test]
async fn the_answer_is_recorded_before_the_section() {
    let (root, path) = scratch("answer-first");
    let section_at_answer = Cell::new(None);
    let outcome = measure_case(
        &path,
        &q6(),
        "q6",
        &this_run(),
        Resume::No,
        async { ("q6's section\n".to_string(), (vec![], vec![])) },
        |_| {},
        |_| {
            let text = std::fs::read_to_string(&path).unwrap_or_default();
            section_at_answer.set(Some(ordered_sections(&text).len()));
        },
    )
    .await;
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    let _ = std::fs::remove_dir_all(&root);
    assert!(matches!(outcome, CaseOutcome::Measured));
    assert_eq!(
        section_at_answer.get(),
        Some(0),
        "the section was written first"
    );
    assert_eq!(
        ordered_sections(&text),
        vec![("q6".to_string(), "q6's section\n".to_string())]
    );
}

#[tokio::test]
async fn a_refused_answer_is_the_case_s_failure() {
    let (root, path) = scratch("answer-refused");
    let outcome = measure_case(
        &path,
        &q6(),
        "q6",
        &this_run(),
        Resume::No,
        async { ("q6's section\n".to_string(), (vec![], vec![])) },
        |_| {},
        |_| panic!("no sf1 registry row to read its row order from"),
    )
    .await;
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    let _ = std::fs::remove_dir_all(&root);
    match outcome {
        CaseOutcome::Failed(reason) => assert!(reason.contains("no sf1 registry row"), "{reason}"),
        _ => panic!("a refused answer did not fail the case"),
    }
    let sections = ordered_sections(&text);
    assert_eq!(sections.len(), 1, "{text}");
    assert!(sections[0].1.starts_with(FAILED), "{text}");
}
