//! Where `pub` may appear, and the surface the crate keeps.

use std::path::Path;

use crate::privacy::pub_fields;
use crate::test_code::{declares_mod, split_attributes};
use crate::tree::{read, sources};
use crate::walls::TEST_DIRS;

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

/// One surface file: its bare `pub` items and its `pub` fields, by name.
struct Surface {
    file: &'static str,
    items: &'static [&'static str],
    fields: &'static [&'static str],
}

/// The crate's API, by file and name: what the CLI calls — `build_session_state`,
/// `register_tables_for`, `plan` and its knobs, `run` and `CpuBackend` — and the closure their
/// signatures force, hop by hop. `plan` returns `GpuNode`, `MemoryModel` and `PlanError`, and
/// a trait's methods are as public as the trait. `run<B: Backend>` makes the trait family, the
/// category traits' method types, `NodeExecutors`' payloads and the CPU backend's associated
/// types as public as `Backend`. Fields are surface too: the CLI writes `PlanKnobs` as a literal
/// and reads `RunReport.batches`. A new row needs the receipt: `cargo build -p peacockdb`
/// failing without it, or `private_interfaces` firing on a row already here.
const SURFACE: &[Surface] = &[
    Surface {
        file: "lib.rs",
        items: &["build_session_state", "register_tables_for"],
        fields: &[],
    },
    Surface {
        file: "plan/mod.rs",
        items: &[
            "GpuNode",
            "NodeKind",
            "PartitionLayout",
            "PlanError",
            "RowInterval",
            "Schema",
        ],
        fields: &[],
    },
    Surface {
        file: "planner/mod.rs",
        items: &[
            "BatchSizing",
            "MemoryModel",
            "PlanKnobs",
            "SMALL_TABLE_BYTES",
            "plan",
        ],
        fields: &["target_partitions", "sizing", "budget", "small_table_bytes"],
    },
    Surface {
        file: "executor/mod.rs",
        items: &[
            "Backend",
            "BackendError",
            "Batch",
            "BatchAccumulatorExecutor",
            "CallStats",
            "CpuBackend",
            "CpuBatch",
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
            "UnloadExecutor",
            "When",
            "record_batch",
            "run",
        ],
        fields: &["batches"],
    },
    Surface {
        file: "executor/cpu_backend/mod.rs",
        items: &[
            "CpuAccumulator",
            "CpuEmitter",
            "CpuExec",
            "CpuJoin",
            "CpuPartitionAccumulator",
            "CpuProbingJoin",
            "CpuSource",
            "CpuUnload",
        ],
        fields: &[],
    },
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
        // A test module's `pub(crate) mod` is a test crate's own business (`src/tests/`,
        // `driver/tests/`); anywhere else any visibility on a `mod` opens the wall.
        let in_a_test_dir = rel
            .iter()
            .any(|part| TEST_DIRS.contains(&part.to_string_lossy().as_ref()));
        for (vis, name) in visible_mod_declarations(&read(&rel)) {
            if vis == "pub" || !in_a_test_dir {
                found.push(format!("  {} declares `{vis} mod {name};`", rel.display()));
            }
        }
    }
    assert!(
        found.is_empty(),
        "a visible `mod` outside lib.rs makes a subcomponent nameable beyond its parent, and \
         the wall it was given then exists only on paper:\n{}\n\nDeclare it `mod`, and put \
         what a sibling needs in the parent's own mod.rs. There is no sanctioned form.",
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
            let same_line = split_attributes(line).0;
            let cfg = same_line
                .iter()
                .copied()
                .chain(std::iter::once(above))
                .find_map(|a| a.strip_prefix("#[cfg(").and_then(|r| r.strip_suffix(")]")));
            match cfg {
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
        let Some(rest) = split_attributes(line).1.strip_prefix("pub mod ") else {
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

/// Every `mod` declaration carrying a visibility, as (visibility, name): `pub mod x;`,
/// `pub(crate) mod x;`, `pub(in …) mod x;`. Attributes on the line are skipped first.
pub(crate) fn visible_mod_declarations(text: &str) -> Vec<(String, String)> {
    text.lines()
        .filter_map(|line| {
            let code = split_attributes(line).1;
            let (name, _) = declares_mod(code)?;
            let after_pub = code.strip_prefix("pub")?;
            let i = after_pub.find("mod ")?;
            Some((format!("pub{}", after_pub[..i].trim_end()), name))
        })
        .collect()
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
/// `pub` item or field outside the table is a missed demotion or a claim without a receipt,
/// and a table row that is not `pub` in its file is a deletion or a demotion the table did not
/// follow. Names per file rather than a count, so a dropped-and-added pair cannot pass.
/// `test_support/` is the feature-gated exception, checked by signature elsewhere.
#[test]
fn bare_pub_is_the_surface_and_nothing_else() {
    let mut found: Vec<(String, Vec<String>, Vec<String>)> = Vec::new();
    for rel in sources() {
        let path = rel.to_string_lossy().replace('\\', "/");
        if path.starts_with("test_support/") {
            continue;
        }
        let text = read(&rel);
        let items: Vec<String> = text
            .lines()
            .filter(|l| is_bare_pub_item(l))
            .filter_map(bare_pub_name)
            .filter(|n| !n.starts_with("mod "))
            .collect();
        let fields: Vec<String> = pub_fields(&text)
            .iter()
            .filter_map(|(_, line)| bare_pub_name(line))
            .collect();
        if !items.is_empty() || !fields.is_empty() {
            found.push((path, items, fields));
        }
    }
    let mut stale = Vec::new();
    for (file, items, fields) in &found {
        let table = SURFACE.iter().find(|s| s.file == file);
        for name in items {
            if !table.is_some_and(|t| t.items.contains(&name.as_str())) {
                stale.push(format!(
                    "  {file}: `{name}` is pub and the surface does not list it"
                ));
            }
        }
        // A field is reachable only through a reachable type, and `pub` types exist only in
        // the surface files — so a `pub` field elsewhere is a reader's convention, not surface.
        for name in fields {
            if table.is_some_and(|t| !t.fields.contains(&name.as_str())) {
                stale.push(format!(
                    "  {file}: field `{name}` is pub and the surface does not list it"
                ));
            }
        }
    }
    for entry in SURFACE {
        let present = found.iter().find(|(f, _, _)| f == entry.file);
        for name in entry.items {
            if !present.is_some_and(|(_, items, _)| items.iter().any(|n| n == name)) {
                stale.push(format!(
                    "  {}: the surface lists `{name}` and it is not pub there",
                    entry.file
                ));
            }
        }
        for name in entry.fields {
            if !present.is_some_and(|(_, _, fields)| fields.iter().any(|n| n == name)) {
                stale.push(format!(
                    "  {}: the surface lists field `{name}` and it is not pub there",
                    entry.file
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

/// The name a bare `pub` line declares: an item's identifier after its kind keywords, or a
/// field's before its colon.
pub(crate) fn bare_pub_name(line: &str) -> Option<String> {
    let rest = split_attributes(line).1.strip_prefix("pub ")?;
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
    let Some(rest) = split_attributes(line).1.strip_prefix("pub ") else {
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
