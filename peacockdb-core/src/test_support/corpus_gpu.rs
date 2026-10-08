//! One corpus query at one mode, on the device.
//!
//! The device side reads what the CPU side wrote and never writes: per-node shape and
//! statistics against that mode's `.cpu.txt` always, and the result where the declaration
//! names a golden. It ignores the regeneration variables rather than honouring them — a
//! device that can author its own golden proves nothing against it.
//!
//! `run` is generic over the backend, so both engines walk one driver over one plan and
//! produce the same shape by construction; the evidence is the rows and the bytes under it.
//! The shape check stays because it costs nothing and goes red the day that stops holding.

use datafusion::arrow::array::RecordBatch;

use crate::executor::{GpuBackend, PlanIndex, run_with_hook};
use crate::plan_text::render_run;

use super::corpus::{self, plan_at, run_cpu};
use super::device_answer::{GpuResultMode, device_answer_matches, gpu_result_mode};
use super::gpu_session::Session;
use super::{
    Mode, assert_results_match, corpus_golden, gpu_schema_validator, mode_named, section_holds_rows,
};

/// The whole of a device corpus case: plan, run on the device with the schema validator
/// installed where the declaration asks for it, then the two read-only assertions — the
/// mode's `.cpu.txt` section, and the result the declaration names.
pub(crate) async fn gpu_case(
    dataset: &str,
    sf: &str,
    query: &str,
    mode: &str,
    gpu_oracle: &str,
    validation: &str,
) {
    let mode = mode_named(mode);
    let what = format!("{dataset}/{query} at {} on a device", mode.name);
    let validated = schema_validation(validation, &what);
    let (_ctx, tree) = plan_at(dataset, sf, query, mode).await;
    let index = PlanIndex::build(tree.as_ref()).unwrap_or_else(|e| panic!("{what}: {e}"));
    let mut session = Session::open(tree.as_ref(), &what);
    let ctx = session.context();
    let hook = validated.then(|| gpu_schema_validator(&index));
    let report = run_with_hook::<GpuBackend>(tree.as_ref(), &ctx, None, hook)
        .unwrap_or_else(|e| panic!("{what}: {e}"));
    assert_eq!(report.in_flight_bytes, 0, "{what} ended holding batches");
    assert_eq!(
        report.holds, report.releases,
        "{what} held {} batches and released {}",
        report.holds, report.releases
    );
    corpus_golden::assert_section(
        &corpus_golden::cpu_golden(dataset, sf, mode.name),
        query,
        &render_run(tree.as_ref(), &report),
    );
    let batches: Vec<RecordBatch> = report
        .batches
        .iter()
        .map(|batch| batch.record_batch().clone())
        .collect();
    // BEFORE the assertion, so an answer the cpu rejects is still recorded — which is
    // exactly where DuckDB is the one that says which engine is right (#243).
    record_gpu_result(dataset, sf, query, mode, &batches);
    assert_result(dataset, sf, query, mode, gpu_oracle, &batches).await;
}

/// The device's own answer, written beside the cpu's goldens when the cycle asks for it.
///
/// A RECORD and never an authority: the device still asserts against `mini.result.txt`, and
/// nothing compares this file with its previous version — GPU float reductions are not
/// reproducible run to run. What reads it is the DuckDB comparison in the cpu binary.
///
/// `PCK_WRITE_GPU_RESULT=1` writes the committed file; any other value writes
/// `gpu-result-<value>.txt` beside it, which `testdata/.gitignore` lists — a cuDF other than
/// shad-gpu's is a record of a different engine and must not replace it. Never under
/// `UPDATE_CANONICAL` or `PCK_UPDATE_SECTIONS` alone: those two own the cpu's goldens, and
/// `a_device_run_under_a_regeneration_writes_no_golden` sets exactly them.
fn record_gpu_result(dataset: &str, sf: &str, query: &str, mode: &Mode, batches: &[RecordBatch]) {
    let Ok(asked) = std::env::var("PCK_WRITE_GPU_RESULT") else {
        return;
    };
    let version = (asked != "1").then_some(asked);
    let (body, _over_cap) = corpus::rendered_or_fingerprint(batches);
    corpus_golden::merge_mode_section(
        &corpus_golden::gpu_result_golden(dataset, sf, version.as_deref()),
        dataset,
        sf,
        query,
        mode.name,
        &body,
    );
}

/// The two conditions that decide which authority is available, and the check that the
/// declaration named the right one. Derivable is why a CHECK can exist, never why the value
/// would be absent: a `golden_exact` where the section is a marker is a test that fails on
/// correct behaviour, and a `live_cpu` where a committed section serves spends a device-side
/// cpu run on a comparison a file makes faster and harder.
fn assert_oracle_suits_the_golden(
    dataset: &str,
    sf: &str,
    query: &str,
    gpu_oracle: &str,
    what: &str,
) {
    let section = corpus_golden::section_of(&corpus_golden::result_golden(dataset, sf), query);
    let frozen = section_holds_rows(&section);
    match gpu_result_mode(gpu_oracle) {
        GpuResultMode::LiveCpu => assert!(
            !frozen,
            "{what}: gpu_oracle is live_cpu and `.result.txt` holds this query's rows — a \
             device-side cpu run for a comparison the committed section already makes"
        ),
        _ => assert!(
            frozen,
            "{what}: gpu_oracle names a golden and `.result.txt` has none for this query — \
             the section says `{}`, so this compare fails on correct behaviour",
            section.trim_end()
        ),
    }
}

/// The device's answer against whichever authority the declaration names.
///
/// The frozen section is one mode's answer serving every mode's run. Where that cannot hold
/// the declaration says `live_cpu`, so a golden compare here is also the claim that the modes
/// agree — which is why `device_answer_matches` is handed the author line too.
async fn assert_result(
    dataset: &str,
    sf: &str,
    query: &str,
    mode: &Mode,
    gpu_oracle: &str,
    batches: &[RecordBatch],
) {
    let what = format!("{dataset}/{query} at {} on a device", mode.name);
    assert_oracle_suits_the_golden(dataset, sf, query, gpu_oracle, &what);
    // A live cpu run at the SAME mode, because where the SQL does not fix the row set,
    // another mode's answer is not an authority on this one's — which is the reason this
    // value exists rather than a frozen section serving all five.
    if gpu_result_mode(gpu_oracle) == GpuResultMode::LiveCpu {
        let run = run_cpu(dataset, sf, query, mode).await;
        assert_results_match(&run.batches, batches, None, &what);
        return;
    }
    let golden = corpus_golden::section_of(&corpus_golden::result_golden(dataset, sf), query);
    device_answer_matches(&golden, gpu_oracle, batches)
        .unwrap_or_else(|said| panic!("{what}: {said}"));
}

/// A `corpus_query!` line's last argument, decoded: whether every batch is held to its
/// node's declaration. Exhaustive, so a misspelling names the row rather than running it
/// unvalidated.
fn schema_validation(s: &str, what: &str) -> bool {
    match s {
        "schema_validation_enabled" => true,
        "schema_validation_disabled" => false,
        other => panic!(
            "{what}: corpus_query!: unknown schema validation '{other}' \
             (expected schema_validation_enabled|schema_validation_disabled)"
        ),
    }
}
