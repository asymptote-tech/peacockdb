//! No peacockdb-core test reads the checkout beyond testdata.
//!
//! peacockdb-core's test binaries are staged to remote hosts with the testdata and nothing
//! else, so a test that builds a path from `CARGO_MANIFEST_DIR` into `src/`, `tests/`,
//! `llm-wiki/` or `.github/` passes locally and fails where it is shipped (#252). The one
//! sanctioned read is `test_support/testdata.rs`, whose `PEACOCK_TESTDATA_DIR` wins on a
//! remote host, and `build.rs`, which runs where the checkout is.

use std::path::Path;

use crate::repo_root;

/// The files allowed to name `CARGO_MANIFEST_DIR`, relative to `peacockdb-core/`.
const ALLOWED: [&str; 2] = ["src/test_support/testdata.rs", "build.rs"];

/// Every line of `text` that reads `CARGO_MANIFEST_DIR`, as `<rel>:<line>: <text>`, unless
/// `rel` is one of [`ALLOWED`].
fn checkout_reads(rel: &str, text: &str) -> Vec<String> {
    if ALLOWED.contains(&rel) {
        return Vec::new();
    }
    text.lines()
        .enumerate()
        .filter(|(_, line)| line.contains("CARGO_MANIFEST_DIR"))
        .map(|(at, line)| format!("{rel}:{}: {}", at + 1, line.trim()))
        .collect()
}

#[test]
fn no_core_test_reads_the_checkout() {
    let core = repo_root().join("peacockdb-core");
    let mut files = Vec::new();
    for dir in ["src", "tests"] {
        walk(&core.join(dir), &mut files);
    }
    assert!(files.len() > 50, "only {} sources were read", files.len());
    let mut found = Vec::new();
    for path in &files {
        let rel = path.strip_prefix(&core).expect("under peacockdb-core");
        let text = std::fs::read_to_string(path).expect("a source file");
        found.extend(checkout_reads(&rel.display().to_string(), &text));
    }
    assert!(
        found.is_empty(),
        "a peacockdb-core test reads the checkout, which a staged binary's host does not have. \
         Compile the input in (`include!`), read it through testdata, or move the test to \
         cost-report:\n{}",
        found.join("\n")
    );
}

/// The scanner's own cases: a doctored read is found by file and line, and the sanctioned
/// file is not.
#[test]
fn a_doctored_read_of_the_checkout_is_found() {
    let doctored = "fn cases() {\n    let dir = \
                    Path::new(env!(\"CARGO_MANIFEST_DIR\")).join(\"tests/common\");\n}\n";
    let found = checkout_reads("tests/test_corpus_goldens/benchmark.rs", doctored);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(
        found[0].starts_with("tests/test_corpus_goldens/benchmark.rs:2: "),
        "{found:?}"
    );
    assert!(checkout_reads("src/test_support/testdata.rs", doctored).is_empty());
}

fn walk(dir: &Path, into: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        match path.is_dir() {
            true => walk(&path, into),
            false if path.extension().is_some_and(|e| e == "rs") => into.push(path),
            false => {}
        }
    }
}
