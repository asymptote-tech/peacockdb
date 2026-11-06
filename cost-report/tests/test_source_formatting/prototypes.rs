//! Prototype code lives apart from the engine: no file outside `prototypes/` and the tests
//! names `prototypes`, but for its declaration, and the component builds only with
//! `test-support`.
//!
//! rustc catches a reach only in a build without the feature, and only where the reaching
//! code is compiled there: an engine item behind `test-support` that names a prototype
//! compiles in every build that has one. `llm-wiki/architecture.md`, "Prototypes", has the
//! rule.

use std::path::Path;

use crate::tree::{code_only, read, sources};
use crate::walls::TEST_DIRS;

/// A reach into the prototypes component, outside comments: `prototypes` as a whole
/// identifier, so `my_prototypes` is not one and neither a `use … as` alias nor a brace list
/// hides it. The component's own declaration, `mod prototypes;`, is the one exception.
fn names_prototypes(text: &str) -> bool {
    let ident = |c: char| c.is_alphanumeric() || c == '_';
    code_only(text)
        .lines()
        .filter(|line| {
            !line
                .trim()
                .trim_start_matches("pub ")
                .starts_with("mod prototypes;")
        })
        .any(|line| {
            line.match_indices("prototypes").any(|(at, word)| {
                !line[..at].chars().next_back().is_some_and(ident)
                    && !line[at + word.len()..].chars().next().is_some_and(ident)
            })
        })
}

/// The files the rule does not cover: the prototypes themselves, and test code, which may
/// test a prototype.
fn may_name_prototypes(rel: &Path) -> bool {
    let first = rel.iter().next().map(|part| part.to_string_lossy());
    first.as_deref() == Some("prototypes")
        || rel
            .iter()
            .any(|part| TEST_DIRS.contains(&part.to_string_lossy().as_ref()))
        || rel.file_name().is_some_and(|name| name == "tests.rs")
}

#[test]
fn no_engine_file_names_prototypes() {
    let found: Vec<String> = sources()
        .into_iter()
        .filter(|rel| !may_name_prototypes(rel) && names_prototypes(&read(rel)))
        .map(|rel| format!("  {}", rel.display()))
        .collect();
    assert!(
        found.is_empty(),
        "an engine file names `prototypes`, so the engine depends on code that measures an \
         idea and runs at one scale:\n{}\n\nMove what it needs into the engine, or the caller \
         into prototypes/.",
        found.join("\n")
    );
}

#[test]
fn the_gate_catches_a_use() {
    assert!(names_prototypes("use crate::prototypes::fuser;"));
    assert!(names_prototypes("    crate::prototypes::fuser::fuse(…)"));
    assert!(!names_prototypes("// prototypes::"));
    assert!(!names_prototypes("let x = my_prototypes::y;"));
    assert!(names_prototypes("use crate::prototypes as p;"));
    assert!(names_prototypes("use crate::{plan, prototypes};"));
    assert!(!names_prototypes("pub mod prototypes;"));
    assert!(!names_prototypes("let prototypes_seen = 1;"));
    assert!(may_name_prototypes(Path::new("prototypes/fuser/fuse.rs")));
    assert!(may_name_prototypes(Path::new("executor/tests/x.rs")));
    assert!(!may_name_prototypes(Path::new("executor/mod.rs")));
    assert!(!may_name_prototypes(Path::new("test_support/mod.rs")));
}

#[test]
fn prototypes_build_only_with_test_support() {
    let lib = read(Path::new("lib.rs"));
    let lines: Vec<&str> = lib.lines().map(str::trim).collect();
    let at = lines
        .iter()
        .position(|line| *line == "pub mod prototypes;")
        .expect("lib.rs declares `pub mod prototypes;`");
    assert_eq!(
        lines[at.saturating_sub(1)],
        "#[cfg(feature = \"test-support\")]",
        "prototypes build only where the harness does, so `cargo build` cannot name them"
    );
}
