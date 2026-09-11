//! Where test code may live: the rung ladder, the carve-out register, and the paths.

use std::collections::BTreeSet;

use crate::src_root;
use crate::tree::{code_only, read, sources};

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
/// Two shapes: a cross-component entry point sits in a component's `mod.rs` because it names
/// what its component owns while its caller is a test in another; a private-state reader sits
/// in the file that declares the field, since only that module and its children can see it.
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
        item: "has_finish_pass",
        called_by: &["executor/cpu_backend/mod.rs"],
    },
    TestOnlyItem {
        file: "executor/cpu_backend/mod.rs",
        item: "has_finish_pass",
        called_by: &["executor/mod.rs"],
    },
    TestOnlyItem {
        file: "executor/cpu_backend/mod.rs",
        item: "physical_expr",
        called_by: &["executor/mod.rs"],
    },
    TestOnlyItem {
        file: "executor/mod.rs",
        item: "has_finish_pass",
        called_by: &["wire/tests.rs"],
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
pub(crate) struct ModDecl {
    pub(crate) line: usize,
    pub(crate) name: String,
    /// `mod x { … }` rather than `mod x;`.
    pub(crate) inline: bool,
    pub(crate) gate: Option<String>,
}

/// Every `mod` declaration in a file, with the attribute that gates it.
///
/// The upward scan stops at a blank line in the **original** text, not in the comment-stripped
/// text: a doc comment strips to an empty line, so a reader that stopped at either would
/// either lose the gate above a documented module or reach past a blank line and borrow an
/// unrelated one from further up.
pub(crate) fn mod_declarations(text: &str) -> Vec<ModDecl> {
    let raw: Vec<&str> = text.lines().collect();
    let stripped = code_only(text);
    let code: Vec<&str> = stripped.lines().collect();
    let mut out = Vec::new();
    for (i, line) in code.iter().enumerate() {
        let Some((name, inline)) = declares_mod(line) else {
            continue;
        };
        let mut gate = split_attributes(line)
            .0
            .into_iter()
            .find(|a| a.starts_with("#[cfg("))
            .map(str::to_string);
        for j in (0..i).rev() {
            if gate.is_some() {
                break;
            }
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
pub(crate) fn declares_mod(line: &str) -> Option<(String, bool)> {
    let t = strip_visibility(split_attributes(line).1);
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

/// A line's leading `#[…]` attributes, and the code after them.
///
/// rustfmt leaves `#[cfg(test)] mod tests;` on one line, so a reader that takes a line as
/// either an attribute or a declaration sees neither half of it.
fn split_attributes(line: &str) -> (Vec<&str>, &str) {
    let mut rest = line.trim_start();
    let mut attrs = Vec::new();
    while rest.starts_with("#[") {
        let Some(end) = rest.find(']') else {
            break;
        };
        attrs.push(&rest[..=end]);
        rest = rest[end + 1..].trim_start();
    }
    (attrs, rest)
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
pub(crate) fn names_cfg_test(attr: &str) -> bool {
    outside_strings(attr)
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .any(|w| w == "test")
}

/// Does this `#[cfg(…)]` name this feature?
pub(crate) fn names_cfg_feature(attr: &str, feature: &str) -> bool {
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
pub(crate) struct TestGate {
    pub(crate) line: usize,
    pub(crate) gate: String,
    pub(crate) sits_on: String,
}

/// Every `#[cfg(…)]` naming `test`, with the first code line below it.
///
/// Attributes and comment-only lines are skipped on the way down, so a gate above a
/// `#[allow(…)]` above a declaration still reports the declaration.
pub(crate) fn test_gates(text: &str) -> Vec<TestGate> {
    let stripped = code_only(text);
    let code: Vec<&str> = stripped.lines().collect();
    let mut out = Vec::new();
    for (i, line) in code.iter().enumerate() {
        let (attrs, same_line) = split_attributes(line);
        let Some(gate) = attrs
            .iter()
            .find(|a| a.starts_with("#[cfg(") && names_cfg_test(a))
        else {
            continue;
        };
        let sits_on = if same_line.is_empty() {
            code[i + 1..]
                .iter()
                .map(|l| l.trim())
                .find(|l| !l.is_empty() && !l.starts_with("#["))
                .unwrap_or("")
        } else {
            same_line.trim_end()
        }
        .to_string();
        out.push(TestGate {
            line: i + 1,
            gate: gate.to_string(),
            sits_on,
        });
    }
    out
}

/// The name an item declaration introduces, or the empty string.
pub(crate) fn declared_item(line: &str) -> String {
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
pub(crate) fn doc_above(text: &str, line: usize) -> String {
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
