//! The tree every rule reads: the sources under `src/`, their components, and the text
//! with its comments dropped.

use std::path::{Path, PathBuf};

use crate::src_root;
use crate::visibility::pub_mod_declarations;

/// Every `.rs` under `src/`, as a path relative to it.
pub(crate) fn sources() -> Vec<PathBuf> {
    fn walk(dir: &Path, root: &Path, out: &mut Vec<PathBuf>) {
        let mut entries: Vec<_> = std::fs::read_dir(dir)
            .unwrap_or_else(|e| panic!("read {}: {e}", dir.display()))
            .map(|e| e.expect("a directory entry").path())
            .collect();
        entries.sort();
        for path in entries {
            if path.is_dir() {
                walk(&path, root, out);
            } else if path.extension().is_some_and(|e| e == "rs") {
                out.push(path.strip_prefix(root).expect("under src").to_path_buf());
            }
        }
    }
    let root = src_root();
    let mut out = Vec::new();
    walk(&root, &root, &mut out);
    assert!(!out.is_empty(), "no sources found under {}", root.display());
    out
}

pub(crate) fn read(rel: &Path) -> String {
    let path = src_root().join(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

/// The components, read from `lib.rs` rather than listed here.
///
/// `lib.rs` declares them `pub mod` and nothing else in the crate may, which
/// `pub_mod_declares_a_component_and_nothing_else` keeps true — so that file is the source of
/// truth and a seventh component cannot arrive without every reader seeing it. A hardcoded six
/// silently exempted anything outside it from two rules, and `src/test_support/` is scheduled.
pub(crate) fn components() -> Vec<String> {
    let text = std::fs::read_to_string(src_root().join("lib.rs")).expect("read lib.rs");
    let out = pub_mod_declarations(&text);
    assert!(
        !out.is_empty(),
        "no `pub mod` components parsed from lib.rs"
    );
    out
}

/// The component a file belongs to, or `None` for the crate root's own files.
pub(crate) fn component_of(rel: &Path) -> Option<String> {
    let first = rel
        .components()
        .next()?
        .as_os_str()
        .to_string_lossy()
        .to_string();
    let name = first.strip_suffix(".rs").unwrap_or(&first).to_string();
    components().contains(&name).then_some(name)
}

/// The text with line comments dropped, since prose is not a use of anything.
///
/// Both expiries read whole files, and both are two-directional, so a commented-out `use` would
/// hold an exemption open from one side and a sentence about one would satisfy it from the
/// other. Line comments only: this crate writes no block comments, and a `//` inside a string
/// can at worst hide a later match on that line, which is the direction that under-reports.
pub(crate) fn code_only(text: &str) -> String {
    text.lines()
        .map(|l| match l.find("//") {
            Some(i) => &l[..i],
            None => l,
        })
        .collect::<Vec<_>>()
        .join("\n")
}
