//! Asserts the layout rules that nothing in rustc checks.
//!
//! Three of the design's claims are the compiler's: an implementation module is private, so
//! naming one from outside its component is `E0603`; a subcomponent declared `mod` is
//! unreachable from another component the same way. This file is for the rest — the claims
//! that hold today because someone wrote the tree that way and nothing would notice if the
//! next change did not.
//!
//! `llm-wiki/coding-style.md` has the rules. The reason each one needs a test rather than a
//! reviewer is recorded at the test that carries it.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// Directories that are a test module rather than a subcomponent: they have a `mod.rs`, and
/// none of the rules about walls apply to them.
///
/// All three rung names, before the first module moves. With only `tests` here the first
/// `executor/ffi_tests/` reads as a subcomponent of `executor`, and the wall rules fire on a
/// directory they were never about.
const TEST_DIRS: &[&str] = &["tests", "ffi_tests", "gpu_tests"];

/// A module inside a subcomponent that is `pub` because something outside the crate names it.
///
/// One entry per **module path**, not per subtree. A subtree exemption is the shape that
/// grows: exempting `executor/cpu_backend` wholesale put 91 `pub` items across 13 files — over
/// half the crate's remaining `pub` surface — behind an exception granted for fourteen types,
/// and two of the eleven inner `pub mod` were forced by nothing at all.
///
/// `forced_by` names the files that force it, verified rather than trusted: each must exist
/// and must still name the path. That is the expiry — `test-layout.md` moves these targets
/// into `src/`, and on that day this goes red and says the wall can go up.
struct PubModule {
    path: &'static str,
    forced_by: &'static [&'static str],
}

const PUB_MODULES: &[PubModule] = &[
    PubModule {
        path: "executor/cpu_backend",
        forced_by: &[
            "peacockdb-core/tests/common/injection.rs",
            "peacockdb-core/tests/test_cpu_executors.rs",
        ],
    },
    PubModule {
        path: "executor/cpu_backend/accumulate",
        forced_by: &[
            "peacockdb-core/tests/common/injection.rs",
            "peacockdb-core/tests/test_cpu_executors.rs",
        ],
    },
    PubModule {
        path: "executor/cpu_backend/emit",
        forced_by: &[
            "peacockdb-core/tests/common/injection.rs",
            "peacockdb-core/tests/test_cpu_executors.rs",
        ],
    },
    PubModule {
        path: "executor/cpu_backend/join",
        forced_by: &["peacockdb-core/tests/common/injection.rs"],
    },
    PubModule {
        path: "executor/cpu_backend/source",
        forced_by: &["peacockdb-core/tests/common/injection.rs"],
    },
    PubModule {
        path: "executor/gpu_backend",
        forced_by: &["peacockdb-core/tests/test_gpu_executors.rs"],
    },
    PubModule {
        path: "executor/gpu_backend/accumulate",
        forced_by: &[
            "peacockdb-core/tests/test_gpu_executors/accumulate.rs",
            "peacockdb-core/tests/test_gpu_executors/contract.rs",
        ],
    },
    PubModule {
        path: "executor/gpu_backend/emit",
        forced_by: &[
            "peacockdb-core/tests/test_gpu_executors/contract.rs",
            "peacockdb-core/tests/test_gpu_executors/join.rs",
        ],
    },
    PubModule {
        path: "executor/gpu_backend/join",
        forced_by: &["peacockdb-core/tests/test_gpu_executors/join.rs"],
    },
];

/// A file that names a subcomponent of a component that is not its own.
///
/// The layout forbids it and rustc refuses it wherever the subcomponent is declared `mod`, so
/// this can only happen behind a `PUB_MODULES` exemption. Verified in both directions like
/// `forced_by`: an entry whose line is gone is reported, and so is a reach nothing here names.
/// Without the first the register outlives its line; without the second it is decoration.
struct CrossComponentReach {
    file: &'static str,
    path: &'static str,
    why: &'static str,
}

const CROSS_COMPONENT_REACHES: &[CrossComponentReach] = &[CrossComponentReach {
    file: "wire/tests.rs",
    path: "executor/cpu_backend",
    why: "one test builds the CPU join beside the recipe it checks, and CpuJoin is a type, so \
          no one-line delegation in executor/mod.rs can carry it; it dies with the exemption",
}];

/// Files that legitimately carry `pub` outside a `mod.rs`: the crate root, and the shared
/// formula module the rules name alongside `mod.rs`.
///
/// Relative paths, not file names. `plan/common.rs` and `planner/translator/common.rs` are
/// ordinary implementation modules that happen to share a name with the crate-level one, and
/// a `file_name()` match exempted both.
const PUB_OUTSIDE_A_MOD_RS: &[&str] = &["lib.rs", "common.rs"];

/// The checkout this test reads.
///
/// Named `repo_root` deliberately: `scripts/build-test.sh` decides which targets may be
/// staged to a remote host by grepping a test's source for `repo_root` or
/// `.github/workflows`, and a target that reads the tree cannot run anywhere the tree is not.
/// Without the name this file is shipped to a CPU host where `sources()` panics on a missing
/// `src/` and the probe cannot find its rlib. One rule rather than two: match the classifier
/// rather than widening it.
fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn src_root() -> PathBuf {
    repo_root().join("peacockdb-core/src")
}

/// Every `.rs` under `src/`, as a path relative to it.
fn sources() -> Vec<PathBuf> {
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

fn read(rel: &Path) -> String {
    let path = src_root().join(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

/// Is this file the `mod.rs` or the module file of an exempt module path?
///
/// The module itself, not what is under it: `executor/cpu_backend/accumulate.rs` is exempt and
/// `executor/cpu_backend/backend.rs` beside it is not.
fn is_an_exempt_module(rel: &Path) -> bool {
    let s = rel.to_string_lossy().replace('\\', "/");
    PUB_MODULES
        .iter()
        .any(|e| s == format!("{}.rs", e.path) || s == format!("{}/mod.rs", e.path))
}

/// The components, read from `lib.rs` rather than listed here.
///
/// `lib.rs` declares them `pub mod` and nothing else in the crate may, which
/// `pub_mod_declares_a_component_and_nothing_else` keeps true — so that file is the source of
/// truth and a seventh component cannot arrive without every reader seeing it. A hardcoded six
/// silently exempted anything outside it from two rules, and `src/test_support/` is scheduled.
fn components() -> Vec<String> {
    let text = std::fs::read_to_string(src_root().join("lib.rs")).expect("read lib.rs");
    let out = pub_mod_declarations(&text);
    assert!(
        !out.is_empty(),
        "no `pub mod` components parsed from lib.rs"
    );
    out
}

/// The component a file belongs to, or `None` for the crate root's own files.
fn component_of(rel: &Path) -> Option<String> {
    let first = rel
        .components()
        .next()?
        .as_os_str()
        .to_string_lossy()
        .to_string();
    let name = first.strip_suffix(".rs").unwrap_or(&first).to_string();
    components().contains(&name).then_some(name)
}

// --- where `pub` may appear --------------------------------------------------

/// A `pub mod` declares a component. Anything else is a subcomponent, and a subcomponent
/// declared `pub mod` has a wall that exists only on paper — `planner::translator::Translator`
/// becomes nameable crate-wide and every rule below it stops meaning anything.
///
/// rustc cannot ask this: `pub mod` is legal everywhere, and the module it exposes is used, so
/// no lint fires. The only reader is this test.
#[test]
fn pub_mod_declares_a_component_and_nothing_else() {
    let exempt: BTreeSet<&str> = PUB_MODULES.iter().map(|e| e.path).collect();
    let mut found = Vec::new();
    for rel in sources() {
        let text = read(&rel);
        let dir = rel
            .parent()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();
        for name in pub_mod_declarations(&text) {
            if rel == Path::new("lib.rs") {
                continue;
            }
            let declared = if dir.is_empty() {
                name.clone()
            } else {
                format!("{dir}/{name}")
            };
            if exempt.contains(declared.as_str()) {
                continue;
            }
            found.push(format!("  {} declares `pub mod {name};`", rel.display()));
        }
    }
    assert!(
        found.is_empty(),
        "`pub mod` outside lib.rs makes a subcomponent nameable crate-wide, and the wall it \
         was given then exists only on paper:\n{}\n\nDeclare it `mod`, and put what a sibling \
         needs in the parent's own mod.rs. If a target outside the crate genuinely forces it, \
         add it to PUB_MODULES with the file that does.",
        found.join("\n")
    );
}

/// Every `pub mod <name>;` a file declares.
///
/// Word-boundary matched on purpose: `pub modelled: usize` is a field of `Underestimate` and
/// a `contains("pub mod")` reader counts it. That is not hypothetical — it is in
/// `executor/mod.rs` today.
fn pub_mod_declarations(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim_start();
        let Some(rest) = line.strip_prefix("pub mod ") else {
            continue;
        };
        let name: String = rest
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        if !name.is_empty() && rest[name.len()..].trim_start().starts_with(';') {
            out.push(name);
        }
    }
    out
}

/// A component's API is declared in its `mod.rs`. A `pub` item anywhere else is API the
/// facade does not list, which is the accident the whole layout exists to prevent — and
/// nothing catches it, because a `pub` item in a private module is perfectly legal and
/// perfectly invisible.
#[test]
fn a_components_api_is_declared_in_its_mod_rs() {
    let mut found = Vec::new();
    for rel in sources() {
        let name = rel
            .file_name()
            .expect("a file name")
            .to_string_lossy()
            .to_string();
        let path = rel.to_string_lossy().replace('\\', "/");
        if name == "mod.rs" || PUB_OUTSIDE_A_MOD_RS.contains(&path.as_str()) {
            continue;
        }
        if is_an_exempt_module(&rel) {
            continue;
        }
        for (n, line) in read(&rel).lines().enumerate() {
            if is_bare_pub_item(line) {
                found.push(format!("  {}:{}: {}", rel.display(), n + 1, line.trim()));
            }
        }
    }
    assert!(
        found.is_empty(),
        "these items are `pub` outside a mod.rs, so they are component API nothing declared:\
         \n{}\n\nDeclare them in the component's mod.rs, or make them pub(crate).",
        found.join("\n")
    );
}

/// A `pub` item declaration, at any indent — `pub(crate)`, `pub(super)` and `pub(in …)` are
/// not this, and neither is a `pub` field, which is a property of a declaration made
/// elsewhere.
fn is_bare_pub_item(line: &str) -> bool {
    let t = line.trim_start();
    let Some(rest) = t.strip_prefix("pub ") else {
        return false;
    };
    const KINDS: &[&str] = &[
        "fn ", "struct ", "enum ", "trait ", "union ", "type ", "const ", "static ", "mod ",
        "unsafe ", "async ", "extern ",
    ];
    KINDS.iter().any(|k| rest.starts_with(k))
}

/// `pub use` re-exports a child through its parent, which is the one shape that makes the
/// facade a lie: the item is declared in one place and named from another, so a reader of
/// `mod.rs` cannot see what the component offers.
#[test]
fn nothing_re_exports_with_pub_use() {
    let found = lines_matching(|l| l.trim_start().starts_with("pub use "));
    assert!(
        found.is_empty(),
        "`pub use` is not allowed — inline the declaration into mod.rs, or into common.rs \
         for what the implementation modules share:\n{}",
        found.join("\n")
    );
}

/// `pub(super)` says "my parent", which is the level the design never wants: it is exactly as
/// wide as `pub(crate)` for a module one deep and narrower in a way no rule here relies on,
/// so it reads as a promise the layout does not make.
#[test]
fn nothing_is_pub_super() {
    let found = lines_matching(|l| l.contains("pub(super)"));
    assert!(
        found.is_empty(),
        "`pub(super)` is not a level this layout uses; the module's own privacy is the \
         boundary, so `pub(crate)` is the level:\n{}",
        found.join("\n")
    );
}

fn lines_matching(pred: impl Fn(&str) -> bool) -> Vec<String> {
    let mut out = Vec::new();
    for rel in sources() {
        for (n, line) in read(&rel).lines().enumerate() {
            if pred(line) {
                out.push(format!("  {}:{}: {}", rel.display(), n + 1, line.trim()));
            }
        }
    }
    out
}

/// A `pub mod` exemption claims a file outside the crate forces it. Verify the claim rather
/// than trusting it — that is the difference between an exemption and a hole.
///
/// Both halves are checked, because half a claim expires wrong: a `forced_by` that lists one
/// of four forcing files goes green the day that one is fixed and announces the wall can go
/// up while three files still need it.
#[test]
fn every_pub_mod_exemption_is_still_forced_by_what_it_names() {
    let root = repo_root();
    let mut stale = Vec::new();
    for entry in PUB_MODULES {
        assert!(
            !entry.forced_by.is_empty(),
            "{} is exempt for no stated reason",
            entry.path
        );
        let module = entry.path.replace('/', "::");
        for forcing in entry.forced_by {
            let path = root.join(forcing);
            if !path.exists() {
                stale.push(format!(
                    "  {} names {forcing}, which no longer exists",
                    entry.path
                ));
                continue;
            }
            let text = std::fs::read_to_string(&path).expect("read a forcing file");
            if !names_the_module(&text, &format!("peacockdb_core::{module}::")) {
                stale.push(format!(
                    "  {} names {forcing}, which no longer reaches `peacockdb_core::{module}`",
                    entry.path
                ));
            }
        }
        // The other direction: a file that forces it and is not listed would keep the
        // exemption alive after every listed file was fixed.
        for rel in files_naming(&module) {
            if !entry.forced_by.contains(&rel.as_str()) {
                stale.push(format!(
                    "  {} is forced by {rel}, which forced_by does not name",
                    entry.path
                ));
            }
        }
    }
    assert!(
        stale.is_empty(),
        "these `pub mod` exemptions no longer say who forces them:\n{}\n\nWhen nothing \
         outside the crate names the path, take the entry out of PUB_MODULES and watch the \
         wall go up.",
        stale.join("\n")
    );
}

/// Every file outside `peacockdb-core/src` that names this module path, repo-root relative.
///
/// The path exactly, not as a prefix. `gpu_backend::accumulate::GpuAccumulator` does traverse
/// `gpu_backend`, so it forces that wall down too, but it counts only for the child: letting a
/// child's callers justify the parent would let one file justify an entry it never names. The
/// cost is a stuck red — when `test_gpu_executors.rs` stops naming `executor/gpu_backend`, the
/// forward half goes red and no child-naming file can re-justify the entry.
///
/// Every workspace member's `src` and `tests`: what forces an exemption is any code outside the
/// crate that names the module. Members come from `Cargo.toml`, as `test_ci_coverage.rs` reads.
fn files_naming(module: &str) -> Vec<String> {
    let root = repo_root();
    let needle = format!("peacockdb_core::{module}::");
    let mut out = Vec::new();
    for member in workspace_members() {
        for sub in ["src", "tests"] {
            // `peacockdb-core/src` is the crate itself: it reaches its own modules by
            // `crate::`, and nothing there is a reason to keep a wall down.
            if member == "peacockdb-core" && sub == "src" {
                continue;
            }
            walk_naming(&root.join(&member).join(sub), &root, &needle, &mut out);
        }
    }
    // This file spells whole module paths as string literals, so the sweep matches it: without
    // the exclusion the guard reports itself as forcing the exemption it polices. `file!()`
    // rather than a written path, so a rename cannot leave the exclusion pointing at nothing;
    // asserted rather than assumed, since a form that stops matching is a silent hole.
    let own = file!().replace('\\', "/");
    assert!(
        root.join(&own).is_file(),
        "file!() does not resolve from the repo root: {own}"
    );
    out.retain(|p| *p != own);
    out.sort();
    out
}

fn walk_naming(dir: &Path, root: &Path, needle: &str, out: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut paths: Vec<PathBuf> = entries.filter_map(|e| e.ok().map(|e| e.path())).collect();
    paths.sort();
    for path in paths {
        if path.is_dir() {
            walk_naming(&path, root, needle, out);
        } else if path.extension().is_some_and(|e| e == "rs")
            && std::fs::read_to_string(&path).is_ok_and(|t| names_the_module(&t, needle))
        {
            out.push(
                path.strip_prefix(root)
                    .expect("under the repo root")
                    .to_string_lossy()
                    .replace('\\', "/"),
            );
        }
    }
}

/// The `[workspace]` members, in manifest order.
fn workspace_members() -> Vec<String> {
    let manifest = std::fs::read_to_string(repo_root().join("Cargo.toml"))
        .expect("read the workspace Cargo.toml");
    let members: Vec<String> = manifest
        .lines()
        .skip_while(|l| !l.starts_with("[workspace]"))
        .skip(1)
        .take_while(|l| !l.starts_with('['))
        .filter_map(|l| {
            l.trim()
                .trim_end_matches(',')
                .strip_prefix('"')?
                .strip_suffix('"')
                .map(str::to_string)
        })
        .collect();
    assert!(
        !members.is_empty(),
        "no [workspace] members parsed from Cargo.toml"
    );
    members
}

/// The text with line comments dropped, since prose is not a use of anything.
///
/// Both expiries read whole files, and both are two-directional, so a commented-out `use` would
/// hold an exemption open from one side and a sentence about one would satisfy it from the
/// other. Line comments only: this crate writes no block comments, and a `//` inside a string
/// can at worst hide a later match on that line, which is the direction that under-reports.
fn code_only(text: &str) -> String {
    text.lines()
        .map(|l| match l.find("//") {
            Some(i) => &l[..i],
            None => l,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Does this text name the module and then something in it, rather than a module below it?
///
/// The test is "not another `::`", not "starts with a capital". A free function or a `pub
/// const` is lowercase, and a reader that wanted an uppercase letter drops a file that forces
/// the exemption. Nothing outside the crate names a lowercase item in these nine modules
/// today, so the fixtures in `each_reader_sees_the_violation_and_not_its_near_miss` are what
/// keeps this half honest. `{` is a brace group of several names, `*` a glob.
// The mirror of `uses_module`'s widening, not taken here: a plain
// `use peacockdb_core::executor::cpu_backend;` forces the wall and has no `::` after the path,
// so the reverse half of `forced_by` would miss it. Nothing outside the crate spells it that
// way today, and the attribution rule above differs, so the two readers stay separate.
fn names_the_module(text: &str, needle: &str) -> bool {
    let text = &code_only(text);
    text.match_indices(needle).any(|(i, _)| {
        let tail = &text[i + needle.len()..];
        let mut chars = tail.chars();
        match chars.next() {
            Some('{') | Some('*') => true,
            Some(c) if c.is_alphabetic() || c == '_' => {
                let len = tail
                    .find(|c: char| !c.is_alphanumeric() && c != '_')
                    .unwrap_or(tail.len());
                !tail[len..].starts_with("::")
            }
            _ => false,
        }
    })
}

// --- the wall the compiler cannot build --------------------------------------

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
fn uses_module(text: &str, parent: &str, name: &str) -> bool {
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
/// the subcomponent is `mod`, and stops the moment one is `pub mod` — so the nine `PUB_MODULES`
/// entries are exactly where the claim needs a test rather than a compiler.
///
/// The register is the point, not the count. A reach that is merely tolerated has no expiry, so
/// the day `forced_by` says the `cpu_backend` wall can go up, taking it up is an `E0603` on a
/// line nobody wrote down.
#[test]
fn only_the_parent_component_names_a_subcomponent() {
    let found = cross_component_reaches();
    let mut stale = Vec::new();
    for entry in CROSS_COMPONENT_REACHES {
        assert!(
            !entry.why.is_empty(),
            "{} reaches {} for no stated reason",
            entry.file,
            entry.path
        );
        if !found
            .iter()
            .any(|(f, p)| f == entry.file && p == entry.path)
        {
            stale.push(format!(
                "  {} no longer names {}, so the entry can go",
                entry.file, entry.path
            ));
        }
    }
    for (file, path) in &found {
        if !CROSS_COMPONENT_REACHES
            .iter()
            .any(|e| e.file == file && e.path == path)
        {
            stale.push(format!(
                "  {file} names {path}, which belongs to another component"
            ));
        }
    }
    assert!(
        stale.is_empty(),
        "the subcomponent wall is not where the register says it is:\n{}\n\nWhat another \
         component needs is declared in the component's own mod.rs.",
        stale.join("\n")
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
fn supers_that_stay_inside(rel: &Path) -> usize {
    let levels = rel.components().count().saturating_sub(1);
    if rel.file_name().is_some_and(|n| n == "mod.rs") {
        levels.saturating_sub(1)
    } else {
        levels
    }
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
            let mut rest = line;
            while let Some(i) = rest.find("super::") {
                let tail = &rest[i..];
                let climbs = tail
                    .as_bytes()
                    .chunks(7)
                    .take_while(|c| *c == b"super::")
                    .count();
                if climbs > depth {
                    found.push(format!(
                        "  {}:{}: {} climbs {climbs} from depth {depth}",
                        rel.display(),
                        n + 1,
                        line.trim()
                    ));
                }
                rest = &rest[i + 7..];
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

// --- what rustc cannot be asked ----------------------------------------------

/// A `pub` item whose signature names a type from the component's own private module.
///
/// **rustc does not warn here, and the reason is the point of this test.**
/// `private_interfaces` compares against a type's *nominal* visibility. flatc emits
/// `pub struct` / `pub enum`, so every type in `wire/generated.rs` is nominally public even
/// though no path outside `wire` can name it — and a `pub fn` in `wire/mod.rs` returning one
/// compiles silently. `FbKind::wire_kind` was exactly that until it was narrowed.
///
/// Deleting this test as redundant with the compiler is the mistake it exists to prevent: the
/// compiler is not asking this question and cannot be made to.
#[test]
fn no_public_signature_names_a_type_from_a_private_module() {
    let mut found = Vec::new();
    for rel in sources() {
        if rel.file_name().is_none_or(|n| n != "mod.rs") {
            continue;
        }
        let text = read(&rel);
        let private = private_module_aliases(&text);
        if private.is_empty() {
            continue;
        }
        for (n, declaration) in pub_declarations(&text) {
            for alias in &private {
                if declaration.contains(&format!("{alias}::")) {
                    let head = declaration.lines().next().unwrap_or("").trim();
                    found.push(format!("  {}:{}: {head}", rel.display(), n + 1));
                }
            }
        }
    }
    assert!(
        found.is_empty(),
        "a `pub` item names a type from a module private to its own component, so the type \
         escapes by inference even though no path can reach it:\n{}\n\nNarrow the item, or \
         declare the type in the mod.rs.",
        found.join("\n")
    );
}

/// Every `pub` item's declaration — from the `pub` to the `{` or `;` that ends the signature —
/// with the line it starts on.
///
/// The whole signature, not its first line: rustfmt wraps past 100 columns, and fourteen
/// `pub fn` in this crate's `mod.rs` files span several lines, `executor::run` among them. A
/// reader matching one line could not see a parameter or a return type.
///
/// Only `(` and `[` nest. A `<` counted as an open leaves `1 << 20` two deep, so the `;` that
/// should end a `pub const` is missed and every declaration below it is swallowed. Nothing in
/// this crate's signatures puts `;` or `{` inside `<…>`, so angle brackets buy nothing.
fn pub_declarations(text: &str) -> Vec<(usize, String)> {
    let lines: Vec<&str> = text.lines().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        if !is_bare_pub_item(lines[i]) {
            i += 1;
            continue;
        }
        let start = i;
        let mut acc = String::new();
        let mut depth = 0i32;
        loop {
            acc.push_str(lines[i]);
            acc.push('\n');
            let mut done = false;
            for c in lines[i].chars() {
                match c {
                    '(' | '[' => depth += 1,
                    ')' | ']' => depth -= 1,
                    '{' | ';' if depth <= 0 => done = true,
                    _ => {}
                }
            }
            if done || i + 1 == lines.len() {
                break;
            }
            i += 1;
        }
        out.push((start, acc));
        i += 1;
    }
    out
}

/// The names a `mod.rs` can reach a privately-declared module's types by: the module itself,
/// and any alias it imports out of it (`use generated::peacock::plan as fb;`).
fn private_module_aliases(text: &str) -> Vec<String> {
    let declared: BTreeSet<String> = text
        .lines()
        .filter_map(|l| l.trim_start().strip_prefix("mod "))
        .map(|r| r.trim_end_matches(';').trim().to_string())
        .filter(|n| !n.is_empty() && n.chars().all(|c| c.is_alphanumeric() || c == '_'))
        .collect();
    let mut out: Vec<String> = declared.iter().cloned().collect();
    for line in text.lines() {
        let t = line.trim_start();
        let Some(rest) = t.strip_prefix("use ") else {
            continue;
        };
        let Some((path, alias)) = rest.trim_end_matches(';').split_once(" as ") else {
            continue;
        };
        let head = path.split("::").next().unwrap_or("").trim();
        if declared.contains(head) {
            out.push(alias.trim().to_string());
        }
    }
    out
}

/// A component's implementation module is unreachable from outside the crate, proved by
/// compiling against the built library rather than by reading the tree.
///
/// `wire::generated` is the case worth the machinery: 7,336 lines of flatc output whose
/// privacy is the whole reason `wire/` is drawn where it is. A text check would assert that
/// `wire/mod.rs` says `mod generated;` — this asserts what that means.
///
/// The control probe is not decoration. A probe that fails for the wrong reason — the wrong
/// rlib, a missing `-L` — looks exactly like a probe that passed, so the positive case has to
/// compile before the negative one proves anything.
#[test]
fn a_private_module_is_unreachable_from_outside_the_crate() {
    let (control, err) = (
        "pub fn control() { let _ = std::any::type_name::<peacockdb_core::wire::Recipe>(); }",
        "pub fn probe() { let _ = std::any::type_name::\
         <peacockdb_core::wire::generated::peacock::plan::PlanNodeKind>(); }",
    );
    let ok = compile_against_the_library("layout_control", control);
    assert!(
        ok.status,
        "the control probe does not compile, so nothing below it proves anything — the \
         library was found but a public path through it did not resolve:\n{}",
        ok.stderr
    );
    let probe = compile_against_the_library("layout_probe", err);
    assert!(
        !probe.status,
        "`wire::generated` compiled from outside the crate. flatc's 7,336 lines are supposed \
         to be private to one component; `wire/mod.rs` must declare `mod generated;`."
    );
    assert!(
        probe.stderr.contains("E0603") && probe.stderr.contains("generated"),
        "the probe failed, but not because `generated` is private — so this test is currently \
         asserting nothing about the wall:\n{}",
        probe.stderr
    );
}

struct Compiled {
    status: bool,
    stderr: String,
}

/// Compile one snippet against the `peacockdb_core` rlib this test binary was linked with.
///
/// The deps directory is taken from the running binary rather than guessed: `cargo test` puts
/// the test binary in `<target>/debug/deps`, and this crate is built into several target dirs
/// (`target/`, `target-cudf-*`), so any hardcoded path would read the wrong build or none.
fn compile_against_the_library(name: &str, body: &str) -> Compiled {
    let exe = std::env::current_exe().expect("the running test binary");
    let deps = exe.parent().expect("a deps directory").to_path_buf();
    let mut rlibs: Vec<PathBuf> = std::fs::read_dir(&deps)
        .unwrap_or_else(|e| panic!("read {}: {e}", deps.display()))
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.file_name().is_some_and(|n| {
                let n = n.to_string_lossy();
                n.starts_with("libpeacockdb_core-") && n.ends_with(".rlib")
            })
        })
        .collect();
    assert!(
        !rlibs.is_empty(),
        "no libpeacockdb_core rlib beside the test binary in {} — the probe cannot compile \
         against a library it cannot find, and a probe that fails for that reason would read \
         as a pass",
        deps.display()
    );
    rlibs.sort_by_key(|p| {
        std::fs::metadata(p)
            .and_then(|m| m.modified())
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
    });
    let rlib = rlibs.last().expect("at least one rlib");

    let dir =
        std::env::temp_dir().join(format!("peacockdb-layout-{}-{}", name, std::process::id()));
    std::fs::create_dir_all(&dir).expect("a scratch directory");
    let file = dir.join("probe.rs");
    std::fs::write(&file, body).expect("write the probe");
    let out = std::process::Command::new(std::env::var("RUSTC").unwrap_or("rustc".into()))
        .args([
            "--edition",
            "2024",
            "--crate-type",
            "lib",
            "--emit",
            "metadata",
        ])
        .arg("--extern")
        .arg(format!("peacockdb_core={}", rlib.display()))
        .arg("-L")
        .arg(&deps)
        .arg("-o")
        .arg(dir.join("probe.rmeta"))
        .arg(&file)
        .output()
        .expect("run rustc");
    let _ = std::fs::remove_dir_all(&dir);
    Compiled {
        status: out.status.success(),
        stderr: String::from_utf8_lossy(&out.stderr).to_string(),
    }
}

// --- the readers' own guard ---------------------------------------------------

// --- where test code may live ------------------------------------------------

/// The three test-module names, each with the gate its rung requires.
///
/// The build shapes are a ladder — `rust-only` ⊂ default ⊂ `gpu` — and one CI line per rung
/// selects its modules by path. So a name and a gate that disagree either lose a case or drag
/// it onto a host that cannot run it, and neither shows up as a failing test. Both directions
/// are checked, by a test each: a rung name must carry its gate, and a rung gate must sit on
/// its name.
const RUNGS: &[(&str, &str)] = &[
    ("tests", "#[cfg(test)]"),
    (
        "ffi_tests",
        "#[cfg(all(test, not(feature = \"rust-only\")))]",
    ),
    ("gpu_tests", "#[cfg(all(test, feature = \"gpu\"))]"),
];

/// An item that keeps a `#[cfg(test)]` of its own because no test module can hold it.
///
/// Two shapes, one check. A cross-component entry point sits in a component's `mod.rs`
/// because visibility pins it there: it names what its component owns while its caller is a
/// test in another. A private-state reader sits in the file that declares the field, because
/// only that module and its children can see it.
///
/// `called_by` is verified both ways, like `PubModule::forced_by`: every file named must
/// exist and must still call the item, and the doc comment must name every one of them. A
/// comment naming a caller that is gone is how this set grows unnoticed — `planner::translate`
/// claimed "three of them, in two other components" and had one.
struct TestOnlyItem {
    file: &'static str,
    item: &'static str,
    called_by: &'static [&'static str],
}

const TEST_ONLY_ITEMS: &[TestOnlyItem] = &[
    TestOnlyItem {
        file: "executor/cpu_backend/accumulate.rs",
        item: "compactions",
        called_by: &["executor/cpu_backend/tests/accumulate.rs"],
    },
    TestOnlyItem {
        file: "executor/cpu_backend/join.rs",
        item: "makes_a_finish_pass",
        called_by: &["wire/tests.rs"],
    },
    TestOnlyItem {
        file: "executor/cpu_backend/mod.rs",
        item: "physical_expr",
        called_by: &["executor/mod.rs"],
    },
    TestOnlyItem {
        file: "executor/mod.rs",
        item: "physical_expr",
        called_by: &["plan/tests/aggregate.rs"],
    },
    TestOnlyItem {
        file: "plan/mod.rs",
        item: "state_for",
        called_by: &["planner/translator/schema_tests.rs"],
    },
    TestOnlyItem {
        file: "planner/mod.rs",
        item: "translate",
        called_by: &["plan_text/tests.rs", "planner/memory_estimation/tests.rs"],
    },
    TestOnlyItem {
        file: "planner/mod.rs",
        item: "translate_expr",
        called_by: &["executor/cpu_backend/expr_physical/tests.rs"],
    },
    TestOnlyItem {
        file: "planner/translator/mod.rs",
        item: "translate_expr",
        called_by: &["planner/mod.rs"],
    },
];

/// A `mod` declaration and the `#[cfg(…)]` directly above it.
struct ModDecl {
    line: usize,
    name: String,
    /// `mod x { … }` rather than `mod x;`.
    inline: bool,
    gate: Option<String>,
}

/// Every `mod` declaration in a file, with the attribute that gates it.
///
/// The upward scan stops at a blank line in the **original** text, not in the comment-stripped
/// text: a doc comment strips to an empty line, so a reader that stopped at either would
/// either lose the gate above a documented module or reach past a blank line and borrow an
/// unrelated one from further up.
fn mod_declarations(text: &str) -> Vec<ModDecl> {
    let raw: Vec<&str> = text.lines().collect();
    let stripped = code_only(text);
    let code: Vec<&str> = stripped.lines().collect();
    let mut out = Vec::new();
    for (i, line) in code.iter().enumerate() {
        let Some((name, inline)) = declares_mod(line) else {
            continue;
        };
        let mut gate = None;
        for j in (0..i).rev() {
            if raw[j].trim().is_empty() {
                break;
            }
            let t = code[j].trim();
            if t.is_empty() {
                continue;
            }
            if !t.starts_with("#[") {
                break;
            }
            if t.starts_with("#[cfg(") {
                gate = Some(t.to_string());
                break;
            }
        }
        out.push(ModDecl {
            line: i + 1,
            name,
            inline,
            gate,
        });
    }
    out
}

/// The module a line declares, and whether its body is inline.
///
/// Visibility is stripped first: `pub mod tests;` is still a test module, and a reader keyed
/// on a bare `mod ` would call it an item and hand it to the wrong rule.
fn declares_mod(line: &str) -> Option<(String, bool)> {
    let t = strip_visibility(line.trim_start());
    let rest = t.strip_prefix("mod ")?;
    let name: String = rest
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    if name.is_empty() {
        return None;
    }
    let tail = rest[name.len()..].trim_start();
    match tail.chars().next() {
        Some(';') => Some((name, false)),
        Some('{') => Some((name, true)),
        _ => None,
    }
}

/// `pub`, `pub(crate)`, `pub(in …)` and nothing else removed from the front of a line.
fn strip_visibility(t: &str) -> &str {
    let Some(rest) = t.strip_prefix("pub") else {
        return t;
    };
    match rest.chars().next() {
        Some('(') => match rest.find(')') {
            Some(i) => rest[i + 1..].trim_start(),
            None => t,
        },
        Some(' ') => rest.trim_start(),
        _ => t,
    }
}

/// Does this `#[cfg(…)]` name the `test` predicate?
///
/// String literals are removed before the word match. `lib.rs` carries
/// `#[cfg(feature = "test-support")]`, and a reader matching the substring would call the
/// whole `test_support` component test code and then demand a rung gate on it.
fn names_cfg_test(attr: &str) -> bool {
    outside_strings(attr)
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .any(|w| w == "test")
}

/// Does this `#[cfg(…)]` name this feature?
fn names_cfg_feature(attr: &str, feature: &str) -> bool {
    let mut squeezed = String::new();
    let mut quoted = false;
    for c in attr.chars() {
        if c == '"' {
            quoted = !quoted;
        }
        if !c.is_whitespace() {
            squeezed.push(c);
        }
    }
    squeezed.contains(&format!("feature=\"{feature}\""))
}

fn outside_strings(text: &str) -> String {
    let mut out = String::new();
    let mut quoted = false;
    for c in text.chars() {
        if c == '"' {
            quoted = !quoted;
            continue;
        }
        if !quoted {
            out.push(c);
        }
    }
    out
}

/// A test module carries the gate of the rung it is named for.
///
/// The floor is a name with no feature in its gate; `ffi_tests` and `gpu_tests` each name one.
/// Nothing in rustc relates the two — a `gpu_tests` module gated `#[cfg(test)]` compiles and
/// runs, on a host with no device, and the only symptom is a failure nobody expected on a rung
/// that was supposed to skip it.
#[test]
fn a_test_module_is_named_for_its_rung() {
    let mut wrong = Vec::new();
    for rel in sources() {
        for decl in mod_declarations(&read(&rel)) {
            let Some((_, want)) = RUNGS.iter().find(|(n, _)| *n == decl.name) else {
                continue;
            };
            if decl.gate.as_deref() != Some(*want) {
                wrong.push(format!(
                    "  {}:{}: `mod {}` is gated {} and its rung requires {}",
                    rel.display(),
                    decl.line,
                    decl.name,
                    decl.gate.as_deref().unwrap_or("(nothing)"),
                    want
                ));
            }
        }
    }
    assert!(
        wrong.is_empty(),
        "a test module is named for the lowest rung it needs, and the gate must say the \
         same:\n{}",
        wrong.join("\n")
    );
}

/// A rung's gate sits only on the module named for it.
///
/// The other direction, and it needs its own test: the first catches a name whose gate is
/// missing, this one catches a gate whose name is missing. A `mod tests` gated on `gpu`
/// satisfies neither rung's path filter and its cases simply stop running.
#[test]
fn a_rung_gate_implies_its_module_name() {
    let mut wrong = Vec::new();
    for rel in sources() {
        for decl in mod_declarations(&read(&rel)) {
            let Some(gate) = decl.gate.as_deref() else {
                continue;
            };
            if !names_cfg_test(gate) {
                continue;
            }
            let want = if names_cfg_feature(gate, "gpu") {
                "gpu_tests"
            } else if names_cfg_feature(gate, "rust-only") {
                "ffi_tests"
            } else if decl.name == "ffi_tests" || decl.name == "gpu_tests" {
                "a name below the rung its gate names"
            } else {
                continue;
            };
            if decl.name != want {
                wrong.push(format!(
                    "  {}:{}: {} sits on `mod {}` and belongs on {}",
                    rel.display(),
                    decl.line,
                    gate,
                    decl.name,
                    want
                ));
            }
        }
    }
    assert!(
        wrong.is_empty(),
        "a rung's gate names the module it belongs on:\n{}",
        wrong.join("\n")
    );
}

/// A `#[cfg(test)]` and the code line it sits on.
struct TestGate {
    line: usize,
    gate: String,
    sits_on: String,
}

/// Every `#[cfg(…)]` naming `test`, with the first code line below it.
///
/// Attributes and comment-only lines are skipped on the way down, so a gate above a
/// `#[allow(…)]` above a declaration still reports the declaration.
fn test_gates(text: &str) -> Vec<TestGate> {
    let stripped = code_only(text);
    let code: Vec<&str> = stripped.lines().collect();
    let mut out = Vec::new();
    for (i, line) in code.iter().enumerate() {
        let t = line.trim();
        if !t.starts_with("#[cfg(") || !names_cfg_test(t) {
            continue;
        }
        let sits_on = code[i + 1..]
            .iter()
            .map(|l| l.trim())
            .find(|l| !l.is_empty() && !l.starts_with("#["))
            .unwrap_or("")
            .to_string();
        out.push(TestGate {
            line: i + 1,
            gate: t.to_string(),
            sits_on,
        });
    }
    out
}

/// The name an item declaration introduces, or the empty string.
fn declared_item(line: &str) -> String {
    let mut t = strip_visibility(line.trim_start());
    // `const ` is not in this list: `const fn` is a qualifier and `const X` is the item, and
    // stripping both left `pub const X: usize = 1 << 20;` nameless.
    for lead in ["default ", "async ", "unsafe ", "extern ", "const fn"] {
        if let Some(rest) = t.strip_prefix(lead) {
            if rest.starts_with('"') {
                break;
            }
            t = if lead == "const fn" {
                &t[6..]
            } else {
                rest.trim_start()
            };
        }
    }
    for kind in [
        "fn ",
        "struct ",
        "enum ",
        "trait ",
        "union ",
        "type ",
        "const ",
        "static ",
        "macro_rules!",
    ] {
        if let Some(rest) = t.strip_prefix(kind) {
            return rest
                .trim_start()
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
        }
    }
    String::new()
}

/// The `///` block directly above a line, from the unstripped text.
fn doc_above(text: &str, line: usize) -> String {
    let raw: Vec<&str> = text.lines().collect();
    let mut doc = Vec::new();
    for j in (0..line.saturating_sub(1)).rev() {
        let t = raw[j].trim();
        if t.starts_with("///") {
            doc.push(t.to_string());
        } else if t.starts_with("#[") {
            continue;
        } else {
            break;
        }
    }
    doc.reverse();
    doc.join("\n")
}

/// `#[cfg(test)]` sits on a test-module declaration and nowhere else.
///
/// This is the rule that keeps test code out of production files: without it the attribute
/// doubles as a `dead_code` silencer, and a fixture at a time is how a production file becomes
/// half a test suite. `driver/partitioned.rs` carried four such items.
///
/// The carve-out is `TEST_ONLY_ITEMS`, and it is checked in both directions here — an entry
/// whose item or caller is gone is as much a finding as an item no entry names. A gated `use`
/// is allowed only in a file that holds an entry, since it serves the declaration below it.
#[test]
fn cfg_test_appears_only_on_a_test_module() {
    let mut stray = Vec::new();
    let mut seen: BTreeSet<(String, String)> = BTreeSet::new();
    for rel in sources() {
        let path = rel.to_string_lossy().replace('\\', "/");
        let text = read(&rel);
        let registered: Vec<&TestOnlyItem> =
            TEST_ONLY_ITEMS.iter().filter(|e| e.file == path).collect();
        for g in test_gates(&text) {
            if declares_mod(&g.sits_on).is_some() {
                continue;
            }
            if g.sits_on.starts_with("use ") && !registered.is_empty() {
                continue;
            }
            let item = declared_item(&g.sits_on);
            match registered.iter().find(|e| e.item == item) {
                Some(entry) => {
                    seen.insert((path.clone(), item.clone()));
                    let doc = doc_above(&text, g.line);
                    for caller in entry.called_by {
                        if !doc.contains(caller) {
                            stray.push(format!(
                                "  {}:{}: `{}` is kept for {}, and its doc comment does not \
                                 name it",
                                path, g.line, item, caller
                            ));
                        }
                    }
                }
                None => stray.push(format!(
                    "  {}:{}: {} on `{}` — test code in a production file",
                    path,
                    g.line,
                    g.gate,
                    g.sits_on.trim_end_matches(['{', ';']).trim()
                )),
            }
        }
    }
    for entry in TEST_ONLY_ITEMS {
        if !seen.contains(&(entry.file.to_string(), entry.item.to_string())) {
            stray.push(format!(
                "  {}: `{}` is registered as test-only and no longer carries a `#[cfg(test)]`; \
                 drop the entry",
                entry.file, entry.item
            ));
            continue;
        }
        for caller in entry.called_by {
            let called = src_root().join(caller);
            if !called.is_file() {
                stray.push(format!(
                    "  {}: `{}` names {} as its caller and that file is gone",
                    entry.file, entry.item, caller
                ));
                continue;
            }
            let text = std::fs::read_to_string(&called).expect("read a named caller");
            if !code_only(&text).contains(entry.item) {
                stray.push(format!(
                    "  {}: `{}` names {} as its caller and that file no longer calls it",
                    entry.file, entry.item, caller
                ));
            }
        }
    }
    assert!(
        stray.is_empty(),
        "`#[cfg(test)]` belongs on a test-module declaration; anything else is a carve-out \
         and must be registered in TEST_ONLY_ITEMS with the caller it exists for:\n{}",
        stray.join("\n")
    );
}

/// A test module is a file of its own, not a block inside a production file.
///
/// `#[cfg(test)] mod tests { … }` compiles to the same thing as `mod tests;` beside a
/// `tests.rs`, so nothing but this notices — and the whole point of the layout is that a
/// reader can tell test code from production code by the path.
#[test]
fn a_test_module_lives_in_its_own_file() {
    let mut inline = Vec::new();
    for rel in sources() {
        for decl in mod_declarations(&read(&rel)) {
            if decl.inline && decl.gate.as_deref().is_some_and(names_cfg_test) {
                inline.push(format!(
                    "  {}:{}: mod {}",
                    rel.display(),
                    decl.line,
                    decl.name
                ));
            }
        }
    }
    assert!(
        inline.is_empty(),
        "a test module is a declaration, not a block — move each body to its own file beside \
         the module it tests:\n{}",
        inline.join("\n")
    );
}

/// Every file a test gate reaches, closed over the directories those files own.
///
/// Three seeds: a file holding a test function, a file a gated `mod` declares, and — because
/// the gate is transitive — everything under a directory whose module file is one of those.
fn test_only_paths() -> BTreeSet<String> {
    let all: Vec<String> = sources()
        .iter()
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .collect();
    let mut out = BTreeSet::new();
    for rel in sources() {
        let path = rel.to_string_lossy().replace('\\', "/");
        let text = read(&rel);
        if code_only(&text)
            .lines()
            .any(|l| l.trim().starts_with("#[") && l.contains("test]"))
        {
            out.insert(path.clone());
        }
        let dir = match rel.file_name().and_then(|n| n.to_str()) {
            Some("mod.rs") => rel.parent().map(|p| p.to_string_lossy().replace('\\', "/")),
            _ => Some(path.trim_end_matches(".rs").to_string()),
        }
        .unwrap_or_default();
        for decl in mod_declarations(&text) {
            if !decl.gate.as_deref().is_some_and(names_cfg_test) || decl.inline {
                continue;
            }
            let stem = if dir.is_empty() {
                decl.name.clone()
            } else {
                format!("{dir}/{}", decl.name)
            };
            for candidate in [format!("{stem}.rs"), format!("{stem}/mod.rs")] {
                if all.contains(&candidate) {
                    out.insert(candidate);
                }
            }
        }
    }
    loop {
        let mut grew = false;
        for owner in out.clone() {
            let dir = match owner.strip_suffix("/mod.rs") {
                Some(d) => d.to_string(),
                None => owner.trim_end_matches(".rs").to_string(),
            };
            for path in &all {
                if path.starts_with(&format!("{dir}/")) && out.insert(path.clone()) {
                    grew = true;
                }
            }
        }
        if !grew {
            break;
        }
    }
    out
}

/// A test-only path says so in its name.
///
/// The compiler is happy either way, and a reader of a diff cannot tell `driver/mock.rs` from
/// an implementation module without opening it. The carve-out to this is `TEST_ONLY_ITEMS`,
/// which is an item in a production file rather than a file.
#[test]
fn a_test_only_path_carries_test() {
    let unnamed: Vec<String> = test_only_paths()
        .into_iter()
        .filter(|p| !p.split('/').any(|c| c.contains("test")))
        .map(|p| format!("  {p}"))
        .collect();
    assert!(
        unnamed.is_empty(),
        "these paths are compiled only under a test gate and nothing in their name says so:\
         \n{}",
        unnamed.join("\n")
    );
}

/// Each reader above, over the shape that would make it read nothing.
///
/// Every one of these is a way the test stays green while the tree stops obeying the rule,
/// which is the failure mode a layout test has: it reads text, and text has near-misses.
#[test]
fn each_reader_sees_the_violation_and_not_its_near_miss() {
    // `pub modelled: usize` is a field in executor/mod.rs, and a `contains("pub mod")` reader
    // counts it as a subcomponent declaration.
    assert!(pub_mod_declarations("    pub modelled: usize,").is_empty());
    assert_eq!(
        pub_mod_declarations("pub mod driver;"),
        vec!["driver".to_string()]
    );
    assert!(
        pub_mod_declarations("// pub mod driver;").is_empty(),
        "a comment is not a declaration"
    );

    // `pub(crate)` and a `pub` field are not items the facade has to declare.
    assert!(is_bare_pub_item("pub fn run() {}"));
    assert!(is_bare_pub_item("    pub struct Held<T> {"));
    assert!(!is_bare_pub_item("pub(crate) fn run() {}"));
    assert!(!is_bare_pub_item("    pub batches: Vec<CpuBatch>,"));

    // The alias is the form that matters: `fb::PlanNodeKind` carries no hint of `generated`.
    let m = "mod generated;\nmod read;\nuse generated::peacock::plan as fb;\n";
    let aliases = private_module_aliases(m);
    assert!(
        aliases.contains(&"fb".to_string()),
        "the alias is what a signature names"
    );
    assert!(aliases.contains(&"generated".to_string()));
    assert!(
        !private_module_aliases("pub mod generated;\nuse generated::peacock::plan as fb;")
            .contains(&"fb".to_string()),
        "a `pub mod` is not private, so an alias out of it is not this rule's business"
    );

    // A shift is not an unclosed generic. Counting `<` as an open left the terminating `;` at
    // depth 2, so the accumulator ran past it and every `pub` item below vanished from the
    // reader — the guard reporting nothing while reading nothing.
    let two = pub_declarations("pub const X: usize = 1 << 20;\npub fn y() -> fb::T {");
    assert_eq!(
        two.len(),
        2,
        "a shift must not swallow the declarations after it: {two:?}"
    );
    assert!(
        two[1].1.contains("fb::T"),
        "the second declaration is the one that names a type"
    );
    // The whole signature even when rustfmt wraps it, which is the reason this reader exists.
    let wrapped = pub_declarations("pub fn f(\n    a: fb::T,\n) -> u8 {\n    0\n}");
    assert_eq!(wrapped.len(), 1);
    assert!(wrapped[0].0 == 0 && wrapped[0].1.contains("fb::T"));

    // A `mod.rs` is its own directory, so it gets one fewer climb than a file beside it — and
    // at depth 0 the subtraction must not wrap a `usize` into an unlimited budget.
    assert_eq!(supers_that_stay_inside(Path::new("executor/mod.rs")), 0);
    assert_eq!(supers_that_stay_inside(Path::new("plan/exec_ops.rs")), 1);
    assert_eq!(
        supers_that_stay_inside(Path::new("planner/translator/mod.rs")),
        1
    );
    assert_eq!(supers_that_stay_inside(Path::new("lib.rs")), 0);
    assert_eq!(
        supers_that_stay_inside(Path::new("mod.rs")),
        0,
        "must not underflow"
    );

    // A free function is lowercase. A reader that wanted a capital called the one file that
    // forces an exemption a near-miss, which is how a bidirectional check goes green in one
    // direction for a reason that has nothing to do with the tree.
    let n = "peacockdb_core::executor::cpu_backend::";
    assert!(names_the_module(
        "use peacockdb_core::executor::cpu_backend::physical_expr;",
        n
    ));
    assert!(names_the_module(
        "use peacockdb_core::executor::cpu_backend::CpuExec;",
        n
    ));
    assert!(names_the_module(
        "use peacockdb_core::executor::cpu_backend::{a, B};",
        n
    ));
    assert!(
        !names_the_module(
            "use peacockdb_core::executor::cpu_backend::join::CpuJoin;",
            n
        ),
        "a deeper module segment forces the child, not this one"
    );
    assert!(
        !names_the_module("// use peacockdb_core::executor::cpu_backend::CpuExec;", n),
        "a commented-out use holds no wall down, so it must not hold an exemption open"
    );
    assert!(names_the_module(
        "use peacockdb_core::executor::cpu_backend::CpuExec; // why",
        n
    ));

    // Every spelling of a reach, not just the fully-qualified one. A module import and an
    // alias have no `::` after the path, and those were green on the two subcomponents where
    // this reader is the only thing standing between the tree and a broken wall.
    let (p, m) = ("crate::executor", "cpu_backend");
    assert!(uses_module("use crate::executor::cpu_backend;", p, m));
    assert!(uses_module("use crate::executor::cpu_backend as cb;", p, m));
    assert!(uses_module(
        "use crate::executor::cpu_backend::join::CpuJoin;",
        p,
        m
    ));
    assert!(uses_module(
        "use crate::executor::{cpu_backend, CpuBatch};",
        p,
        m
    ));
    assert!(uses_module(
        "use crate::executor::\n    cpu_backend::join::CpuJoin;",
        p,
        m
    ));
    assert!(!uses_module("use crate::executor::CpuBatch;", p, m));
    assert!(
        !uses_module("use crate::executor::cpu_backendish::X;", p, m),
        "an identifier this one is a prefix of is a different module"
    );
    assert!(
        !uses_module("use crate::executor::{driver, CpuBatch};", p, m),
        "a brace group naming other members is not a reach into this one"
    );

    // A feature called `test-support` is not the `test` predicate. `lib.rs` carries exactly
    // that attribute, and a substring reader would call the whole component test code.
    assert!(names_cfg_test("#[cfg(test)]"));
    assert!(names_cfg_test("#[cfg(all(test, feature = \"gpu\"))]"));
    assert!(!names_cfg_test("#[cfg(feature = \"test-support\")]"));
    assert!(!names_cfg_test(
        "#[cfg(all(feature = \"gpu\", feature = \"rust-only\"))]"
    ));
    assert!(names_cfg_feature(
        "#[cfg(all(test, feature = \"gpu\"))]",
        "gpu"
    ));
    assert!(!names_cfg_feature(
        "#[cfg(all(test, feature = \"gpu\"))]",
        "rust-only"
    ));
    assert!(names_cfg_feature(
        "#[cfg(all(test, not(feature=\"rust-only\")))]",
        "rust-only"
    ));

    // A declaration, a block and a field, which the three rules part on.
    assert_eq!(declares_mod("mod tests;"), Some(("tests".into(), false)));
    assert_eq!(declares_mod("mod tests {"), Some(("tests".into(), true)));
    assert_eq!(
        declares_mod("pub(crate) mod mock;"),
        Some(("mock".into(), false)),
        "visibility must not hide a test module from the rung rules"
    );
    assert_eq!(declares_mod("    pub modelled: usize,"), None);
    assert_eq!(declares_mod("let mode = 1;"), None);

    // The gate above a documented module is the module's; the one above a blank line is not.
    // A doc comment strips to an empty line, so a reader keyed on the stripped text alone
    // would take an unrelated attribute from further up and report the wrong rung.
    let gated = "#[cfg(all(test, feature = \"gpu\"))]\n/// why\nmod gpu_tests;\n";
    let decls = mod_declarations(gated);
    assert_eq!(decls.len(), 1);
    assert_eq!(
        decls[0].gate.as_deref(),
        Some("#[cfg(all(test, feature = \"gpu\"))]")
    );
    assert!(
        mod_declarations("#[cfg(test)]\n\nmod tests;\n")[0]
            .gate
            .is_none(),
        "a blank line ends the attribute block"
    );
    assert!(
        mod_declarations("// #[cfg(test)]\nmod tests;\n")[0]
            .gate
            .is_none(),
        "a commented-out attribute gates nothing"
    );
    assert!(mod_declarations("#[cfg(test)]\nmod tests {\n}\n")[0].inline);

    // What a gate sits on, and the name the register matches against.
    let sites = test_gates("#[cfg(test)]\n#[allow(dead_code)]\npub(crate) fn hops() {\n");
    assert_eq!(sites.len(), 1);
    assert_eq!(
        sites[0].sits_on, "pub(crate) fn hops() {",
        "an attribute between the gate and the item must not hide the item"
    );
    assert_eq!(
        declared_item("pub(crate) fn hops(&self) -> (usize, usize) {"),
        "hops"
    );
    assert_eq!(declared_item("pub const X: usize = 1 << 20;"), "X");
    assert_eq!(
        declared_item("use crate::plan::Expr;"),
        "",
        "a gated use is not an item, and the register matches items by name"
    );
    assert_eq!(
        doc_above(
            "/// names plan_text/tests.rs\n#[cfg(test)]\nfn translate() {}\n",
            2
        ),
        "/// names plan_text/tests.rs"
    );
    assert_eq!(
        doc_above(
            "// an ordinary comment\n#[cfg(test)]\nfn translate() {}\n",
            2
        ),
        "",
        "only a doc comment documents the carve-out"
    );

    // Those fixtures make this file itself a match, which is what makes the `file!()`
    // exclusion in `files_naming` load-bearing rather than decorative. Matched on the file
    // name, so a `file!()` whose form drifts away from what the walk yields fails here too.
    let own = Path::new(file!())
        .file_name()
        .expect("file!() names a file")
        .to_owned();
    let text = std::fs::read_to_string(repo_root().join(file!())).expect("read this file");
    assert!(
        names_the_module(&text, n),
        "the fixtures above must make this file a match"
    );
    assert!(
        !files_naming("executor::cpu_backend")
            .iter()
            .any(|p| Path::new(p).file_name() == Some(own.as_os_str())),
        "the guard states the rule and must not report itself as forcing the exemption"
    );
}
