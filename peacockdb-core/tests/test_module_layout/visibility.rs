//! Where `pub` may appear, and the surface the crate keeps.

use std::path::Path;

use crate::tree::{read, sources};

/// The one file that legitimately carries `pub` outside a `mod.rs`: the crate root, whose
/// two entry points are the CLI's.
///
/// A relative path, not a file name. `plan/common.rs` and `planner/translator/common.rs` are
/// ordinary implementation modules that happen to share a name with the crate-level one, and
/// a `file_name()` match exempted both.
const PUB_OUTSIDE_A_MOD_RS: &[&str] = &["lib.rs"];

/// The six components, `pub mod` in `lib.rs` unconditionally. Named rather than counted: a
/// count passes when one is dropped and another added.
const COMPONENTS: &[&str] = &["common", "executor", "plan", "plan_text", "planner", "wire"];

/// The crate's API, by file and name: what the CLI calls — `build_session_state`,
/// `register_tables_for`, `plan` and its knobs, `run` and `CpuBackend` — and the closure their
/// signatures force, hop by hop. `plan` returns `GpuNode`, `MemoryModel` and `PlanError`, and
/// a trait's methods are as public as the trait. `run<B: Backend>` makes the trait family, the
/// category traits' method types, `NodeExecutors`' payloads and the CPU backend's associated
/// types as public as `Backend`, and `RunReport`'s field types as public as the report. A new
/// row needs the receipt: `cargo build -p peacockdb` failing without it, or
/// `private_interfaces` firing on a row already here.
const SURFACE: &[(&str, &[&str])] = &[
    ("lib.rs", &["build_session_state", "register_tables_for"]),
    (
        "plan/mod.rs",
        &[
            "GpuNode",
            "NodeKind",
            "PartitionLayout",
            "PlanError",
            "RowInterval",
            "Schema",
        ],
    ),
    (
        "planner/mod.rs",
        &[
            "BatchSizing",
            "MemoryModel",
            "PlanKnobs",
            "SMALL_TABLE_BYTES",
            "plan",
        ],
    ),
    (
        "executor/mod.rs",
        &[
            "Backend",
            "BackendError",
            "Batch",
            "BatchAccumulatorExecutor",
            "CallStats",
            "CpuBackend",
            "CpuBatch",
            "EmittedBatch",
            "ExecExecutor",
            "Executor",
            "Forwarder",
            "JoinExecutor",
            "LaneEvent",
            "NodeExecutors",
            "PartitionAccumulatorExecutor",
            "PartitionEmitterExecutor",
            "ProbingJoin",
            "RowRange",
            "RunError",
            "RunReport",
            "SourceExecutor",
            "SourceStep",
            "TraceEvent",
            "Underestimate",
            "UnloadExecutor",
            "When",
            "record_batch",
            "run",
        ],
    ),
    (
        "executor/cpu_backend/mod.rs",
        &[
            "CpuAccumulator",
            "CpuEmitter",
            "CpuExec",
            "CpuJoin",
            "CpuPartitionAccumulator",
            "CpuProbingJoin",
            "CpuSource",
            "CpuUnload",
        ],
    ),
];

/// A `pub mod` declares a component. Anything else is a subcomponent, and a subcomponent
/// declared `pub mod` has a wall that exists only on paper — `planner::translator::Translator`
/// becomes nameable crate-wide and every rule below it stops meaning anything.
///
/// rustc cannot ask this: `pub mod` is legal everywhere, and the module it exposes is used, so
/// no lint fires. The only reader is this test. `lib.rs` is pinned by name: the six
/// components unconditionally, and `test_support` behind its feature and nothing else.
#[test]
fn pub_mod_declares_a_component_and_nothing_else() {
    let mut found = Vec::new();
    for rel in sources() {
        if rel == Path::new("lib.rs") {
            continue;
        }
        for name in pub_mod_declarations(&read(&rel)) {
            found.push(format!("  {} declares `pub mod {name};`", rel.display()));
        }
    }
    assert!(
        found.is_empty(),
        "`pub mod` outside lib.rs makes a subcomponent nameable crate-wide, and the wall it \
         was given then exists only on paper:\n{}\n\nDeclare it `mod`, and put what a sibling \
         needs in the parent's own mod.rs. There is no sanctioned form.",
        found.join("\n")
    );
    let (unconditional, gated) = gated_pub_mods(&read(Path::new("lib.rs")));
    assert_eq!(
        unconditional, COMPONENTS,
        "lib.rs declares these components `pub mod` unconditionally, and the rules name six"
    );
    assert_eq!(
        gated,
        vec![(
            "test_support".to_string(),
            "feature = \"test-support\"".to_string()
        )],
        "the harness is the one `pub mod` behind a feature, and it is behind its own"
    );
}

/// `lib.rs`'s `pub mod` declarations, split by whether the line above gates them: the
/// unconditional names, and each gated name with the `cfg` predicate over it.
pub(crate) fn gated_pub_mods(text: &str) -> (Vec<String>, Vec<(String, String)>) {
    let (mut plain, mut gated) = (Vec::new(), Vec::new());
    let mut above = "";
    for line in text.lines() {
        let trimmed = line.trim();
        if let [name] = pub_mod_declarations(line).as_slice() {
            match above
                .strip_prefix("#[cfg(")
                .and_then(|r| r.strip_suffix(")]"))
            {
                Some(pred) => gated.push((name.clone(), pred.to_string())),
                None => plain.push(name.clone()),
            }
        }
        above = trimmed;
    }
    (plain, gated)
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

/// Bare `pub` in `src/` means "the binary calls this", and the claim is checked both ways: a
/// `pub` item outside the table is a missed demotion or a claim without a receipt, and a
/// table row that is not `pub` in its file is a deletion or a demotion the table did not
/// follow. Names per file rather than a count, so a dropped-and-added pair cannot pass.
/// `test_support/` is the feature-gated exception, checked by signature elsewhere.
#[test]
fn bare_pub_is_the_surface_and_nothing_else() {
    let mut found: Vec<(String, Vec<String>)> = Vec::new();
    for rel in sources() {
        let path = rel.to_string_lossy().replace('\\', "/");
        if path.starts_with("test_support/") {
            continue;
        }
        let names: Vec<String> = read(&rel)
            .lines()
            .filter(|l| is_bare_pub_item(l))
            .filter_map(bare_pub_name)
            .filter(|n| !n.starts_with("mod "))
            .collect();
        if !names.is_empty() {
            found.push((path, names));
        }
    }
    let mut stale = Vec::new();
    for (file, names) in &found {
        let table = SURFACE.iter().find(|(f, _)| f == file).map(|(_, n)| *n);
        for name in names {
            if !table.is_some_and(|t| t.contains(&name.as_str())) {
                stale.push(format!(
                    "  {file}: `{name}` is pub and the surface does not list it"
                ));
            }
        }
    }
    for (file, names) in SURFACE {
        let present = found.iter().find(|(f, _)| f == file).map(|(_, n)| n);
        for name in *names {
            if !present.is_some_and(|p| p.iter().any(|n| n == name)) {
                stale.push(format!(
                    "  {file}: the surface lists `{name}` and it is not pub there"
                ));
            }
        }
    }
    assert!(
        stale.is_empty(),
        "the crate's surface is not the table in SURFACE:\n{}\n\nA bare `pub` claims the CLI \
         calls it; the receipt is `cargo build -p peacockdb` failing without it, or \
         `private_interfaces` on a row already listed. Otherwise it is `pub(crate)`.",
        stale.join("\n")
    );
}

/// The name a bare `pub` item line declares: the identifier after the kind keywords.
pub(crate) fn bare_pub_name(line: &str) -> Option<String> {
    let rest = line.trim_start().strip_prefix("pub ")?;
    if rest.starts_with("mod ") {
        return Some(rest.to_string());
    }
    let mut words = rest.split(|c: char| !c.is_alphanumeric() && c != '_');
    let name = words.find(|w| {
        !matches!(
            *w,
            "" | "fn"
                | "struct"
                | "enum"
                | "trait"
                | "union"
                | "type"
                | "const"
                | "static"
                | "unsafe"
                | "async"
                | "extern"
                | "C"
        )
    })?;
    Some(name.to_string())
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
