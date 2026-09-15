//! The wall the compiler cannot build: a subcomponent is its parent's alone.

use std::path::Path;

use crate::tree::{code_only, component_of, read, sources};

/// Directories that are a test module rather than a subcomponent: they have a `mod.rs`, and
/// none of the rules about walls apply to them.
///
/// All three rung names, before the first module moves. With only `tests` here the first
/// `executor/ffi_tests/` reads as a subcomponent of `executor`, and the wall rules fire on a
/// directory they were never about.
pub(crate) const TEST_DIRS: &[&str] = &["tests", "ffi_tests", "gpu_tests"];

/// **The one rule rustc explicitly cannot enforce.** A subcomponent is meant to be its
/// parent's alone, and Rust's visibility is "the module and its descendants" — so
/// `pub(super)`, `pub(crate)` and `pub(in path)` all give a subcomponent's *siblings* the
/// same access its parent has. There is no level meaning "my parent but not my siblings".
///
/// So `planner/memory_estimation/` can write `use super::translator::Translator;` and it
/// compiles. The design's claim is that no such edge exists; this is the only thing that can
/// keep it true.
#[test]
fn no_subcomponent_reaches_a_sibling() {
    let mut found = Vec::new();
    for (parent, kids) in subcomponents_by_parent() {
        for a in &kids {
            for b in &kids {
                if a == b {
                    continue;
                }
                let root = format!("{parent}/{a}");
                let absolute = format!("crate::{}::{b}::", parent.replace('/', "::"));
                for rel in sources() {
                    let s = rel.to_string_lossy().replace('\\', "/");
                    if !s.starts_with(&format!("{root}/")) {
                        continue;
                    }
                    let text = read(&rel);
                    // `super::` from `<a>/mod.rs` is already the parent; every level deeper
                    // inside `<a>` takes one more. Derived from the file's own depth rather
                    // than fixed at two, since nesting is three deep and `scan_mapping/`
                    // reaching `memory_estimation` takes three.
                    let mut needles = vec![absolute.clone()];
                    let depth = rel.components().count();
                    for climbs in 1..=depth {
                        needles.push(format!("{}{b}::", "super::".repeat(climbs)));
                    }
                    // The longest match only: `super::x::` is a substring of
                    // `super::super::x::`, and reporting both names one edge twice.
                    if let Some(needle) = needles
                        .iter()
                        .filter(|n| text.contains(*n))
                        .max_by_key(|n| n.len())
                    {
                        found.push(format!("  {} names `{needle}`", rel.display()));
                    }
                }
            }
        }
    }
    assert!(
        found.is_empty(),
        "a subcomponent reaches a sibling, which nothing in rustc refuses:\n{}\n\nWhat a \
         sibling needs is declared in the parent's own mod.rs — that is what the type moves \
         in the layout are for.",
        found.join("\n")
    );
}

/// Directory -> its subcomponent children. The crate root is not in it: components are meant
/// to reach each other, and a `tests` directory is a test module rather than a wall.
fn subcomponents_by_parent() -> Vec<(String, Vec<String>)> {
    let mut out: Vec<(String, Vec<String>)> = Vec::new();
    for rel in sources() {
        if rel.file_name().is_none_or(|n| n != "mod.rs") {
            continue;
        }
        let Some(dir) = rel.parent() else { continue };
        let Some(parent) = dir.parent() else { continue };
        let name = dir
            .file_name()
            .expect("a directory name")
            .to_string_lossy()
            .to_string();
        let parent = parent.to_string_lossy().replace('\\', "/");
        if parent.is_empty() || TEST_DIRS.contains(&name.as_str()) {
            continue;
        }
        match out.iter_mut().find(|(p, _)| *p == parent) {
            Some((_, kids)) => kids.push(name),
            None => out.push((parent, vec![name])),
        }
    }
    out.retain(|(_, kids)| kids.len() > 1);
    out
}

/// Every subcomponent under `src/`, as its path from the crate root.
///
/// Derived from the tree rather than listed: a directory with a `mod.rs` that is neither a
/// component nor a test module is a subcomponent, at whatever depth it sits.
fn subcomponent_paths() -> Vec<String> {
    let mut out = Vec::new();
    for rel in sources() {
        if rel.file_name().is_none_or(|n| n != "mod.rs") {
            continue;
        }
        let Some(dir) = rel.parent() else { continue };
        let name = dir
            .file_name()
            .expect("a directory name")
            .to_string_lossy()
            .to_string();
        if dir.parent().is_none_or(|p| p.as_os_str().is_empty())
            || TEST_DIRS.contains(&name.as_str())
        {
            continue;
        }
        out.push(dir.to_string_lossy().replace('\\', "/"));
    }
    out
}

/// The text with the whitespace after `::` removed, so a wrapped path is one string again.
///
/// A line ending in `::` is always a continued path in valid Rust, so nothing else is joined.
/// Whitespace *before* `::` is left alone: `crate::executor\n    ::cpu_backend` is a spelling
/// nothing here writes and rustfmt does not produce.
fn tight_paths(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut after_colons = false;
    for c in text.chars() {
        if after_colons && c.is_whitespace() {
            continue;
        }
        out.push(c);
        after_colons = out.ends_with("::");
    }
    out
}

/// The identifier this text starts with, or the empty string.
fn first_segment(text: &str) -> &str {
    let end = text
        .find(|c: char| !c.is_alphanumeric() && c != '_')
        .unwrap_or(text.len());
    &text[..end]
}

/// Does this text use `<parent>::<name>` — as a path, a module import, an alias, or a member of
/// a brace group?
///
/// Every spelling, not just the fully-qualified one. `use crate::executor::cpu_backend;` and
/// `... as cb;` name the module with no `::` after it, and on `cpu_backend` and `gpu_backend`
/// this reader is the only defence there is — rustc refuses a reach into any other subcomponent
/// with `E0603` whatever the spelling. One level of brace group is enough: a nested group's own
/// first segment is what the split yields.
pub(crate) fn uses_module(text: &str, parent: &str, name: &str) -> bool {
    let text = tight_paths(text);
    let needle = format!("{parent}::");
    text.match_indices(&needle).any(|(i, _)| {
        let tail = &text[i + needle.len()..];
        match tail.strip_prefix('{') {
            Some(group) => group
                .split(['}', ';'])
                .next()
                .is_some_and(|g| g.split(',').any(|m| first_segment(m.trim_start()) == name)),
            None => first_segment(tail) == name,
        }
    })
}

/// Every place a component names another component's subcomponent, as (file, subcomponent).
fn cross_component_reaches() -> Vec<(String, String)> {
    let subs = subcomponent_paths();
    let mut found = Vec::new();
    for rel in sources() {
        let Some(component) = component_of(&rel) else {
            continue;
        };
        let text = code_only(&read(&rel));
        let hits: Vec<&String> = subs
            .iter()
            .filter(|sub| !sub.starts_with(&format!("{component}/")))
            .filter(|sub| {
                let (parent, name) = sub.rsplit_once('/').expect("a subcomponent has a parent");
                uses_module(
                    &text,
                    &format!("crate::{}", parent.replace('/', "::")),
                    name,
                )
            })
            .collect();
        for sub in &hits {
            // The deepest path only: `crate::planner::translator::scan_mapping::` contains
            // `crate::planner::translator::`, and reporting both names one line twice.
            if hits
                .iter()
                .any(|other| other.starts_with(&format!("{sub}/")))
            {
                continue;
            }
            found.push((rel.to_string_lossy().replace('\\', "/"), (*sub).clone()));
        }
    }
    found
}

/// **Only the parent component's own code may use a subcomponent.** rustc enforces it wherever
/// the subcomponent is `mod`, and every subcomponent is `mod` now — so this holds the claim
/// where a `pub mod` would open it, and says where, rather than leaving an `E0603` on a line
/// nobody wrote down for the day a wall goes up.
#[test]
fn only_the_parent_component_names_a_subcomponent() {
    let found: Vec<String> = cross_component_reaches()
        .iter()
        .map(|(file, path)| format!("  {file} names {path}, which belongs to another component"))
        .collect();
    assert!(
        found.is_empty(),
        "a subcomponent is its parent's alone:\n{}\n\nWhat another component needs is \
         declared in the component's own mod.rs.",
        found.join("\n")
    );
}

/// How many `super::`s a file can use before it leaves its own component.
///
/// A `mod.rs` **is** its directory rather than a file inside it, so one `super::` there is
/// already the parent — `wire/mod.rs`'s first `super::` is the crate root. Counting path
/// components and subtracting one gives every `mod.rs` a free climb, and that free climb is
/// exactly the one that leaves the component: the reader was blind on the thirteen files most
/// likely to import across a boundary while passing on every file that could not.
/// Saturating, not `- 1`: a `mod.rs` at depth 0 would subtract twice and underflow a `usize`
/// into a cap of 18 quintillion, which is the same as no rule at all. No such file exists
/// today, and the reader should not depend on that.
pub(crate) fn supers_that_stay_inside(rel: &Path) -> usize {
    let levels = rel.components().count().saturating_sub(1);
    if rel.file_name().is_some_and(|n| n == "mod.rs") {
        levels.saturating_sub(1)
    } else {
        levels
    }
}

/// How far each `super::` chain on this line climbs, one entry per chain.
pub(crate) fn super_chains(line: &str) -> Vec<usize> {
    let mut out = Vec::new();
    let mut rest = line;
    while let Some(i) = rest.find("super::") {
        let tail = &rest[i..];
        let climbs = tail
            .as_bytes()
            .chunks(7)
            .take_while(|c| *c == b"super::")
            .count();
        out.push(climbs);
        rest = &rest[i + 7 * climbs..];
    }
    out
}

/// `super::` is for inside a component; crossing one takes an absolute `crate::` path. A
/// `super::` chain that climbs past its component's root has crossed a boundary while looking
/// like it did not, which is how a component quietly acquires a dependency nobody declared.
#[test]
fn no_super_path_climbs_out_of_its_component() {
    let mut found = Vec::new();
    for rel in sources() {
        let Some(_) = component_of(&rel) else {
            continue;
        };
        let depth = supers_that_stay_inside(&rel);
        let text = read(&rel);
        for (n, line) in text.lines().enumerate() {
            for climbs in super_chains(line).into_iter().filter(|c| *c > depth) {
                found.push(format!(
                    "  {}:{}: {} climbs {climbs} from depth {depth}",
                    rel.display(),
                    n + 1,
                    line.trim()
                ));
            }
        }
    }
    assert!(
        found.is_empty(),
        "these `super::` chains leave their own component:\n{}\n\nAcross a component \
         boundary the path is `crate::<component>::…`.",
        found.join("\n")
    );
}
