//! Where `pub` may appear, and the register of what forces one.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use crate::repo_root;
use crate::tree::{code_only, read, sources};

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

const PUB_MODULES: &[PubModule] = &[];

/// Files that legitimately carry `pub` outside a `mod.rs`: the crate root, and the shared
/// formula module the rules name alongside `mod.rs`.
///
/// Relative paths, not file names. `plan/common.rs` and `planner/translator/common.rs` are
/// ordinary implementation modules that happen to share a name with the crate-level one, and
/// a `file_name()` match exempted both.
const PUB_OUTSIDE_A_MOD_RS: &[&str] = &["lib.rs", "common.rs"];

/// Is this file the `mod.rs` or the module file of an exempt module path?
///
/// The module itself, not what is under it: an entry for `executor/gpu_backend/accumulate`
/// would exempt `accumulate.rs` and not `backend.rs` beside it.
fn is_an_exempt_module(rel: &Path) -> bool {
    let s = rel.to_string_lossy().replace('\\', "/");
    PUB_MODULES
        .iter()
        .any(|e| s == format!("{}.rs", e.path) || s == format!("{}/mod.rs", e.path))
}

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
pub(crate) fn pub_mod_declarations(text: &str) -> Vec<String> {
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
pub(crate) fn is_bare_pub_item(line: &str) -> bool {
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
/// cost is a stuck red — when the one file naming `executor/gpu_backend` stops, the forward
/// half goes red and no child-naming file can re-justify the entry.
///
/// Every workspace member's `src` and `tests`: what forces an exemption is any code outside the
/// crate that names the module. Members come from `Cargo.toml`, as `test_ci_coverage.rs` reads.
pub(crate) fn files_naming(module: &str) -> Vec<String> {
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
    // This target spells whole module paths as string literals (`near_miss.rs`), so the sweep
    // matches it: without the exclusion the guard reports itself as forcing the exemption it
    // polices. Its own directory and root file, from `file!()` rather than a written path, so
    // a rename cannot leave the exclusion pointing at nothing; asserted, not assumed.
    let own_dir = Path::new(file!())
        .parent()
        .expect("file!() names a file in a directory")
        .to_string_lossy()
        .replace('\\', "/");
    let own_root = format!("{own_dir}.rs");
    assert!(
        root.join(&own_dir).is_dir() && root.join(&own_root).is_file(),
        "file!() does not resolve from the repo root: {own_dir}"
    );
    out.retain(|p| *p != own_root && !p.starts_with(&format!("{own_dir}/")));
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

/// Does this text name the module — imported plain or under an alias — or something in it,
/// rather than a module below it?
///
/// The test is "not another `::`", not "starts with a capital". A free function or a `pub
/// const` is lowercase, and a reader that wanted an uppercase letter drops a file that forces
/// the exemption. Nothing outside the crate names a lowercase item in the registered modules
/// today, so the fixtures in `each_reader_sees_the_violation_and_not_its_near_miss` are what
/// keeps this half honest. `{` is a brace group of several names, `*` a glob. The attribution
/// differs from `uses_module`, which a deeper path satisfies too, so the two stay separate.
pub(crate) fn names_the_module(text: &str, needle: &str) -> bool {
    let text = &code_only(text);
    let module = needle.trim_end_matches("::");
    text.match_indices(module).any(|(i, _)| {
        let tail = &text[i + module.len()..];
        let Some(tail) = tail.strip_prefix("::") else {
            return tail.starts_with(';') || tail.trim_start().starts_with("as ");
        };
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
