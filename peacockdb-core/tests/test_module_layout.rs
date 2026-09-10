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

/// The six the crate root declares. Everything else under `src/` belongs to one of them.
const COMPONENTS: &[&str] = &["common", "executor", "plan", "plan_text", "planner", "wire"];

/// Directories that are a test module rather than a subcomponent: they have a `mod.rs`, and
/// none of the rules about walls apply to them.
const TEST_DIRS: &[&str] = &["tests"];

/// Why a `pub mod` subcomponent is tolerated.
///
/// A struct rather than a bare path, because the entry has to carry the thing a reader
/// judges: not "is this still exempt?" but "is the claim still true?". Both of these are
/// reachable only because two integration targets are separate crates that construct backend
/// executors directly. `llm-wiki/tasks/test-layout.md` moves those targets into `src/`, and
/// this list goes with them.
struct PubSubcomponent {
    path: &'static str,
    /// The targets whose existence outside the crate is the whole reason.
    forced_by: &'static [&'static str],
}

const PUB_SUBCOMPONENTS: &[PubSubcomponent] = &[
    PubSubcomponent {
        path: "executor/cpu_backend",
        forced_by: &["test_cpu_executors"],
    },
    PubSubcomponent {
        path: "executor/gpu_backend",
        forced_by: &["test_gpu_executors"],
    },
];

/// Files that legitimately carry `pub` outside a `mod.rs`: the crate root, and the shared
/// formula module the rules name alongside `mod.rs`.
const PUB_OUTSIDE_A_MOD_RS: &[&str] = &["lib.rs", "common.rs"];

fn src_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src")
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

/// Is this file inside one of the `pub mod` subcomponents?
fn inside_a_pub_subcomponent(rel: &Path) -> bool {
    let s = rel.to_string_lossy().replace('\\', "/");
    PUB_SUBCOMPONENTS
        .iter()
        .any(|e| s.starts_with(&format!("{}/", e.path)))
}

/// The component a file belongs to, or `None` for the crate root's own files.
fn component_of(rel: &Path) -> Option<String> {
    let first = rel.components().next()?.as_os_str().to_string_lossy().to_string();
    let name = first.strip_suffix(".rs").unwrap_or(&first).to_string();
    COMPONENTS.contains(&name.as_str()).then_some(name)
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
    let exempt: BTreeSet<&str> = PUB_SUBCOMPONENTS.iter().map(|e| e.path).collect();
    let mut found = Vec::new();
    for rel in sources() {
        let text = read(&rel);
        let dir = rel.parent().map(|p| p.to_string_lossy().to_string()).unwrap_or_default();
        for name in pub_mod_declarations(&text) {
            if rel == Path::new("lib.rs") {
                continue;
            }
            let declared = if dir.is_empty() { name.clone() } else { format!("{dir}/{name}") };
            // The exemption covers what is inside it too: a test naming
            // `cpu_backend::accumulate::CpuAccumulator` needs both levels reachable, and the
            // two expire together.
            if exempt.contains(declared.as_str()) || inside_a_pub_subcomponent(&rel) {
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
         add it to PUB_SUBCOMPONENTS with the target that does.",
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
        let Some(rest) = line.strip_prefix("pub mod ") else { continue };
        let name: String = rest.chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
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
        let name = rel.file_name().expect("a file name").to_string_lossy().to_string();
        if name == "mod.rs" || PUB_OUTSIDE_A_MOD_RS.contains(&name.as_str()) {
            continue;
        }
        if inside_a_pub_subcomponent(&rel) {
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
    let Some(rest) = t.strip_prefix("pub ") else { return false };
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

/// A `pub mod` exemption claims a target outside the crate forces it. Verify the claim
/// rather than trusting it — that is the difference between an exemption and a hole.
///
/// This is the exemption's expiry, and it is mechanical: `test-layout.md` moves both targets
/// into `src/`, and on the day it does, this goes red and says the wall can be raised.
#[test]
fn every_pub_mod_exemption_still_has_the_target_that_forces_it() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests");
    let mut stale = Vec::new();
    for entry in PUB_SUBCOMPONENTS {
        for target in entry.forced_by {
            if !dir.join(format!("{target}.rs")).exists() {
                stale.push(format!("  {} is exempt because of {target}, which no longer exists",
                                   entry.path));
            }
        }
        assert!(
            !entry.forced_by.is_empty(),
            "{} is exempt for no stated reason",
            entry.path
        );
    }
    assert!(
        stale.is_empty(),
        "these `pub mod` exemptions have outlived the targets that forced them:\n{}\n\nThe \
         subcomponent can be `mod` again — take the entry out of PUB_SUBCOMPONENTS and watch \
         the wall go up.",
        stale.join("\n")
    );
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
                    // `super::` from `<a>/mod.rs` is the parent, so `super::<b>::` there is
                    // the edge; from deeper inside `<a>` it takes one more level.
                    for needle in [
                        absolute.as_str(),
                        &format!("super::{b}::"),
                        &format!("super::super::{b}::"),
                    ] {
                        if text.contains(needle) {
                            found.push(format!("  {} names `{needle}`", rel.display()));
                        }
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
        let name = dir.file_name().expect("a directory name").to_string_lossy().to_string();
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

/// `super::` is for inside a component; crossing one takes an absolute `crate::` path. A
/// `super::` chain that climbs past its component's root has crossed a boundary while looking
/// like it did not, which is how a component quietly acquires a dependency nobody declared.
#[test]
fn no_super_path_climbs_out_of_its_component() {
    let mut found = Vec::new();
    for rel in sources() {
        let Some(_) = component_of(&rel) else { continue };
        // `plan/exec_ops.rs` sits one below `plan`, so one `super::` reaches the component
        // root and two would leave it. `X/mod.rs` sits at X's own depth.
        let depth = rel.components().count() - 1;
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
        for (n, line) in text.lines().enumerate() {
            if !is_bare_pub_item(line) {
                continue;
            }
            for alias in &private {
                if line.contains(&format!("{alias}::")) {
                    found.push(format!("  {}:{}: {}", rel.display(), n + 1, line.trim()));
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
        let Some(rest) = t.strip_prefix("use ") else { continue };
        let Some((path, alias)) = rest.trim_end_matches(';').split_once(" as ") else { continue };
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
        std::fs::metadata(p).and_then(|m| m.modified()).unwrap_or(std::time::SystemTime::UNIX_EPOCH)
    });
    let rlib = rlibs.last().expect("at least one rlib");

    let dir = std::env::temp_dir().join(format!("peacockdb-layout-{}-{}", name, std::process::id()));
    std::fs::create_dir_all(&dir).expect("a scratch directory");
    let file = dir.join("probe.rs");
    std::fs::write(&file, body).expect("write the probe");
    let out = std::process::Command::new(std::env::var("RUSTC").unwrap_or("rustc".into()))
        .args(["--edition", "2024", "--crate-type", "lib", "--emit", "metadata"])
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

/// Each reader above, over the shape that would make it read nothing.
///
/// Every one of these is a way the test stays green while the tree stops obeying the rule,
/// which is the failure mode a layout test has: it reads text, and text has near-misses.
#[test]
fn each_reader_sees_the_violation_and_not_its_near_miss() {
    // `pub modelled: usize` is a field in executor/mod.rs, and a `contains("pub mod")` reader
    // counts it as a subcomponent declaration.
    assert!(pub_mod_declarations("    pub modelled: usize,").is_empty());
    assert_eq!(pub_mod_declarations("pub mod driver;"), vec!["driver".to_string()]);
    assert!(pub_mod_declarations("// pub mod driver;").is_empty(), "a comment is not a declaration");

    // `pub(crate)` and a `pub` field are not items the facade has to declare.
    assert!(is_bare_pub_item("pub fn run() {}"));
    assert!(is_bare_pub_item("    pub struct Held<T> {"));
    assert!(!is_bare_pub_item("pub(crate) fn run() {}"));
    assert!(!is_bare_pub_item("    pub batches: Vec<CpuBatch>,"));

    // The alias is the form that matters: `fb::PlanNodeKind` carries no hint of `generated`.
    let m = "mod generated;\nmod read;\nuse generated::peacock::plan as fb;\n";
    let aliases = private_module_aliases(m);
    assert!(aliases.contains(&"fb".to_string()), "the alias is what a signature names");
    assert!(aliases.contains(&"generated".to_string()));
    assert!(
        !private_module_aliases("pub mod generated;\nuse generated::peacock::plan as fb;")
            .contains(&"fb".to_string()),
        "a `pub mod` is not private, so an alias out of it is not this rule's business"
    );
}
