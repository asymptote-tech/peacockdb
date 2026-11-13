//! One benchmark case's section in its mode's file: kept when this run already measured it,
//! written when the case ran, and a record of the failure when it panicked.

use std::panic::AssertUnwindSafe;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::Duration;

use datafusion::arrow::array::RecordBatch;
use futures::FutureExt;

use super::benchmark_trailer::TRAILER_KEYS;
use super::benchmark_tree::parse_benchmark_section;
use super::provenance::fields;
use super::{
    CASE_TIMEOUT_ENV, CaseOutcome, FAILED, Provenance, Regeneration, Resume, data_dir_for,
    merge_section, ordered_sections,
};

#[cfg(test)]
mod tests;

/// `before_section` takes what the case returned beside its body, and runs before the section is
/// written: the section is what a resumed run keeps, so it must come last. A panic there is the
/// case's failure, like one in the case.
///
/// Written through `merge_section`, which locks the directory, reads inside the lock and
/// publishes by rename: a run killed mid-case leaves the file as the last finished case wrote
/// it. Always `Sections`, never `Whole`: a run can be filtered with nothing in it to say so,
/// and pruning what it did not produce would delete a measurement nobody asked to lose.
pub(crate) async fn measure_into<T>(
    path: &Path,
    declared: &[(String, Option<String>)],
    query: &str,
    provenance: &Provenance,
    resume: Resume,
    case: impl Future<Output = (String, T)>,
    before_section: impl FnOnce(T),
) -> CaseOutcome {
    if resume == Resume::KeepMeasured && holds_measured(path, query, provenance) {
        return CaseOutcome::Held;
    }
    // Unwind safety is not in question: a case that panicked is abandoned whole, and what
    // outlives it is the file, written only below.
    let measured = AssertUnwindSafe(case)
        .catch_unwind()
        .await
        .and_then(|(body, beside)| {
            std::panic::catch_unwind(AssertUnwindSafe(|| before_section(beside))).map(|()| body)
        });
    let (body, outcome) = match measured {
        Ok(body) => (body, CaseOutcome::Measured),
        Err(panic) => {
            let reason = one_line(&panic_message(panic.as_ref()));
            (
                failed_record(&reason, provenance),
                CaseOutcome::Failed(reason),
            )
        }
    };
    merge_section(path, declared, query, &body, Regeneration::Sections);
    outcome
}

/// A benchmark case measured into `path` through [`measure_into`], with what must precede its
/// section run as `before_section`: the record's rows, then the chosen run's answer. A resumed
/// run keeps the section, so neither may come after it, and a refusal of either is the case's
/// failure, recorded in place of the section.
#[cfg_attr(feature = "rust-only", allow(dead_code))]
#[allow(clippy::too_many_arguments)]
pub(crate) async fn measure_case(
    path: &Path,
    declared: &[(String, Option<String>)],
    query: &str,
    provenance: &Provenance,
    resume: Resume,
    case: impl Future<Output = (String, (Vec<String>, Vec<RecordBatch>))>,
    rows: impl FnOnce(&[String]),
    answer: impl FnOnce(&[RecordBatch]),
) -> CaseOutcome {
    let before_section = |(record, batches): (Vec<String>, Vec<RecordBatch>)| {
        rows(&record);
        answer(&batches);
    };
    measure_into(
        path,
        declared,
        query,
        provenance,
        resume,
        case,
        before_section,
    )
    .await
}

/// A failed case's section: `FAILED` and the one-line reason, then the five.
fn failed_record(reason: &str, provenance: &Provenance) -> String {
    let mut body = format!("{FAILED}{reason}\n");
    for (key, value) in fields(provenance) {
        body.push_str(&format!("{key}={value}\n"));
    }
    body
}

/// The section a case still running at its timeout leaves.
#[cfg_attr(feature = "rust-only", allow(dead_code))]
pub(crate) fn timeout_record(seconds: u64, provenance: &Provenance) -> String {
    failed_record(
        &format!("timeout: the case ran past {seconds} s"),
        provenance,
    )
}

/// [`CASE_TIMEOUT_ENV`] read as given: whole seconds, or `None` unset or empty (the scripts pass
/// it along empty when the operator set none). Anything else panics naming it: a typo read as no
/// limit would let a hung query hold the card until the host's watcher stops the process.
pub(crate) fn case_timeout_asked(value: Option<&str>) -> Option<u64> {
    match value {
        None | Some("") => None,
        Some(text) => match text.parse::<u64>() {
            Ok(seconds) if seconds > 0 => Some(seconds),
            _ => panic!("{CASE_TIMEOUT_ENV}={text} is not a whole number of seconds above 0"),
        },
    }
}

/// [`case_timeout_asked`] over the process environment.
#[cfg_attr(feature = "rust-only", allow(dead_code))]
pub(crate) fn case_timeout() -> Option<u64> {
    case_timeout_asked(std::env::var(CASE_TIMEOUT_ENV).ok().as_deref())
}

/// A running case's watch: dropping it ends the watch with nothing written.
#[cfg_attr(feature = "rust-only", allow(dead_code))]
pub(crate) struct CaseWatch {
    _cancel: mpsc::Sender<()>,
}

/// Watch one case for `seconds`. Past them, a thread writes the case's section as
/// [`timeout_record`] through `merge_section` and then calls `expire`, which in a benchmark run
/// aborts the process, with no destructors: a hung case cannot be unwound, and a section saying
/// why is all a rerun needs to skip it. Everything is held by value, since the case's own frame
/// is the one that hangs.
#[cfg_attr(feature = "rust-only", allow(dead_code))]
pub(crate) fn watch_case(
    seconds: u64,
    path: PathBuf,
    declared: Vec<(String, Option<String>)>,
    query: String,
    provenance: Provenance,
    expire: impl FnOnce() + Send + 'static,
) -> CaseWatch {
    let (cancel, cancelled) = mpsc::channel::<()>();
    std::thread::spawn(move || {
        if cancelled.recv_timeout(Duration::from_secs(seconds)) != Err(RecvTimeoutError::Timeout) {
            return;
        }
        let body = timeout_record(seconds, &provenance);
        merge_section(&path, &declared, &query, &body, Regeneration::Sections);
        eprintln!(
            "{query}: ran past {seconds} s ({CASE_TIMEOUT_ENV}), recorded failed in {}",
            path.display()
        );
        expire();
    });
    CaseWatch { _cancel: cancel }
}

/// Whether `query`'s section was measured by this harness under `provenance`: a timed section
/// whose trailer is `TRAILER_KEYS` in order, the five with this run's values, and whose every
/// node carries its batches. A failed case's record is run again, and so is a section another
/// card, driver, CUDA, cuDF or host measured, or an older binary wrote under this one.
fn holds_measured(path: &Path, query: &str, provenance: &Provenance) -> bool {
    let Ok(text) = std::fs::read_to_string(path) else {
        return false;
    };
    ordered_sections(&text)
        .into_iter()
        .filter(|(name, _)| name == query)
        .any(|(_, body)| {
            parse_benchmark_section(&body).is_some_and(|section| {
                let keys: Vec<&str> = section.trailer.iter().map(|(key, _)| *key).collect();
                let five = &section.trailer[section.trailer.len().saturating_sub(5)..];
                keys == TRAILER_KEYS
                    && five == fields(provenance)
                    && section
                        .nodes
                        .iter()
                        .all(|node| node.batch_rows.is_some() && node.batch_bytes.is_some())
            })
        })
}

fn panic_message(panic: &(dyn std::any::Any + Send)) -> String {
    match (panic.downcast_ref::<&str>(), panic.downcast_ref::<String>()) {
        (Some(message), _) => message.to_string(),
        (_, Some(message)) => message.clone(),
        _ => "a panic carrying no message".to_string(),
    }
}

/// A section's lines are its structure, so the reason is one: every run of whitespace, line
/// breaks among it, one space.
fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub(crate) fn assert_dataset_present(dataset: &str, sf: &str, what: &str) {
    let dir = data_dir_for(dataset, sf);
    assert!(
        dir.is_dir(),
        "{what}: no dataset at {} — every {dataset} sf{sf} case would fail at plan time, so the \
         run is refused before any of them is recorded",
        dir.display()
    );
}
