//! One order a golden's writer cannot have wrong: the device corpus tier records its answer
//! before it compares that answer with the cpu's.

use std::path::Path;

use crate::tree::{code_only, read};

/// `gpu_case`'s body, comments dropped. Extracted by name so the extraction's own failure — a
/// renamed function, a body that no longer closes at column 0 — is one message rather than a
/// rule that quietly stops reading anything.
fn device_case_body() -> String {
    let text = code_only(&read(Path::new("test_support/corpus_gpu.rs")));
    let opens: Vec<usize> = text
        .match_indices("async fn gpu_case(")
        .map(|(at, _)| at)
        .collect();
    assert_eq!(
        opens.len(),
        1,
        "corpus_gpu.rs declares `gpu_case` {} times",
        opens.len()
    );
    let from = &text[opens[0]..];
    let end = from
        .find("\n}\n")
        .expect("gpu_case's body closes with a brace at column 0");
    from[..end].to_string()
}

/// The device tier records `gpu-result.txt` BEFORE either comparison against the cpu, both of
/// which panic. The divergence DuckDB exists to settle is the one that makes them panic —
/// #243's lane split moves an aggregate's group count, which `.cpu.txt` carries — so an answer
/// recorded after them is missing in exactly the case that needs it.
///
/// What this reads is the order of three calls in one function body, comments dropped. It
/// cannot see a panic reached through something called earlier, and no test here can run the
/// device: the end-to-end proof is the first recording cycle, where a cell whose `.cpu.txt`
/// section moved must still have its section in the file that comes home.
#[test]
fn the_device_case_records_its_answer_before_it_compares_with_the_cpu() {
    let body = device_case_body();
    let at = |call: &str| -> usize {
        let found: Vec<usize> = body.match_indices(call).map(|(at, _)| at).collect();
        assert_eq!(
            found.len(),
            1,
            "gpu_case calls `{call}` {} times — the order below is stated for one of each",
            found.len()
        );
        found[0]
    };
    let record = at("record_gpu_result(");
    for compared in ["corpus_golden::assert_section(", "assert_result("] {
        assert!(
            record < at(compared),
            "gpu_case calls `{compared}` before `record_gpu_result(`, so a device answer the \
             cpu rejects is never recorded — which is the one case the recording is for"
        );
    }
}
