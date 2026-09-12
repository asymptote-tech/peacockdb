//! What rustc cannot be asked: a public signature over a private type, and a private
//! module's reach from outside the crate, proved by compiling against it.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use crate::tree::{code_only, components, read, sources};
use crate::visibility::is_bare_pub_item;

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
        let modules = private_modules(&text);
        if modules.is_empty() {
            continue;
        }
        let mut names = private_module_imports(&text, &modules);
        names.extend(modules);
        for (n, declaration) in pub_declarations(&text) {
            let signature = signature_only(&code_only(&declaration));
            if let Some(name) = private_name_in(&signature, &names) {
                let head = declaration.lines().next().unwrap_or("").trim();
                found.push(format!(
                    "  {}:{}: {head} names `{name}`",
                    rel.display(),
                    n + 1
                ));
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

/// The first of `names` this signature reaches: as a path (`fb::PlanNodeKind`), or as a
/// whole identifier where the name is capitalised, which a type, trait or const is. A
/// lowercase name matches only as a path: `layout` is a parameter as often as the module.
pub(crate) fn private_name_in(signature: &str, names: &[String]) -> Option<String> {
    names
        .iter()
        .find(|name| {
            signature.contains(&format!("{name}::"))
                || (name.starts_with(char::is_uppercase) && names_identifier(signature, name))
        })
        .cloned()
}

/// A `pub` in `test_support/mod.rs` whose signature names a type from a component.
///
/// The harness is what the corpus binaries reach the engine through, and it is a facade only
/// while nothing it declares hands a component type across: `pub fn tree() -> Box<dyn GpuNode>`
/// puts `GpuNode` back on the surface under another name, and it compiles. One file, because
/// `a_components_api_is_declared_in_its_mod_rs` keeps every `pub` there; fields as well as
/// parameters and returns, since a `pub` field is read across the same boundary. The components
/// are read off `lib.rs` rather than listed, so an eighth is inside the rule the day it arrives.
#[test]
fn no_test_support_signature_names_a_component_type() {
    let rel = Path::new("test_support/mod.rs");
    let text = read(rel);
    let components: Vec<String> = components()
        .into_iter()
        .filter(|c| c != "test_support")
        .collect();
    let imported = component_imports(&text, &components);
    assert!(
        !imported.iter().any(|name| name == "*"),
        "test_support/mod.rs glob-imports a component, so the names a signature can use \
         cannot be read off the file; import them by name"
    );
    let found: Vec<String> = component_types_on_the_surface(&text, &components)
        .into_iter()
        .map(|(n, head, name)| format!("  {}:{}: {head} names `{name}`", rel.display(), n + 1))
        .collect();
    assert!(
        found.is_empty(),
        "a `pub` in test_support names a type from a component, so the type is on the surface \
         under the harness's name and the facade is a rename:\n{}\n\nNo parameter, return or \
         `pub` field names a component's type; std, arrow and the harness's own types are what \
         remain. The engine type stays behind a pub(crate) body.",
        found.join("\n")
    );
}

/// Every declaration in a `test_support/mod.rs` text that puts a component type on the
/// surface, as (line, first line of the declaration, the type named): each bare `pub` item
/// with its body where the body is signature — a `pub enum`'s variants, a `pub trait`'s
/// methods — each `pub` field, and each `type` alias below bare `pub`, since an alias is one
/// `pub fn` away from the surface and checking it once catches an alias of an alias at its root.
pub(crate) fn component_types_on_the_surface(
    text: &str,
    components: &[String],
) -> Vec<(usize, String, String)> {
    let imported = component_imports(text, components);
    let declared = pub_declarations(text)
        .into_iter()
        .chain(pub_fields(text))
        .chain(type_aliases(text));
    let mut found = Vec::new();
    for (n, declaration) in declared {
        let signature = signature_only(&code_only(&declaration));
        if let Some(name) = component_type_in(&signature, components, &imported) {
            let head = declaration.lines().next().unwrap_or("").trim().to_string();
            found.push((n, head, name));
        }
    }
    found.sort();
    found
}

/// The names a `use crate::<component>…;` binds in this file: each member of a brace group
/// or the single item, under its `as` alias where it has one, the module itself for a bare
/// module import, and `*` for a glob. A crate-root group (`use crate::{plan::A, planner::B}`)
/// is read member by member.
pub(crate) fn component_imports(text: &str, components: &[String]) -> Vec<String> {
    bound_from(text, &["crate::"], components)
}

/// The names a `use <module>…;` binds in this file out of one of its own private modules,
/// spelled bare or as `self::`, read the same way.
pub(crate) fn private_module_imports(text: &str, modules: &[String]) -> Vec<String> {
    bound_from(text, &["self::", ""], modules)
}

/// What every `use` under one of `prefixes` binds out of one of `roots`. Statements are
/// joined across lines first, since rustfmt wraps a long group.
fn bound_from(text: &str, prefixes: &[&str], roots: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    for statement in use_statements(text) {
        let Some(path) = prefixes.iter().find_map(|p| statement.strip_prefix(p)) else {
            continue;
        };
        let members = match path.strip_prefix('{') {
            Some(group) => split_members(group.trim_end_matches('}')),
            None => vec![path.to_string()],
        };
        for member in members {
            // A group is peeled before an alias is looked for: the ` as ` inside
            // `planner::{A, B as C}` belongs to `B`, not to the member.
            let (head, alias) = match member.split_once(" as ") {
                Some((head, alias)) if !member.contains('{') => (head.trim(), Some(alias.trim())),
                _ => (member.trim(), None),
            };
            let Some(root) = roots
                .iter()
                .find(|r| head == r.as_str() || head.starts_with(&format!("{r}::")))
            else {
                continue;
            };
            let tail = &head[root.len()..];
            if tail.is_empty() {
                out.push(alias.unwrap_or(root).to_string());
                continue;
            }
            match tail[2..].strip_prefix('{') {
                Some(group) => {
                    // `{self, …}` binds the module the group sits under, not the word.
                    for member in split_members(group.trim_end_matches('}')) {
                        let name = bound_name(&member);
                        out.push(if name == "self" { root.clone() } else { name });
                    }
                }
                None => out.push(match alias {
                    Some(alias) => alias.to_string(),
                    None => bound_name(&tail[2..]),
                }),
            }
        }
    }
    out
}

/// Every `use …;` in the text, joined across lines, without the keyword and the semicolon.
fn use_statements(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut lines = text.lines();
    while let Some(line) = lines.next() {
        let Some(rest) = line.trim().strip_prefix("use ") else {
            continue;
        };
        let mut statement = rest.trim().to_string();
        while !statement.contains(';') {
            let Some(more) = lines.next() else { break };
            statement.push(' ');
            statement.push_str(more.trim());
        }
        out.push(statement.split(';').next().unwrap_or("").trim().to_string());
    }
    out
}

/// A brace group's members, split at the commas outside any inner group.
fn split_members(group: &str) -> Vec<String> {
    let mut out = Vec::new();
    let (mut depth, mut current) = (0usize, String::new());
    for c in group.chars() {
        match c {
            '{' => depth += 1,
            '}' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                out.push(std::mem::take(&mut current));
                continue;
            }
            _ => {}
        }
        current.push(c);
    }
    out.push(current);
    out.into_iter()
        .map(|m| m.trim().to_string())
        .filter(|m| !m.is_empty())
        .collect()
}

/// What a `use` member binds: its alias, else its last segment — `*` for a glob.
fn bound_name(member: &str) -> String {
    match member.split_once(" as ") {
        Some((_, alias)) => alias.trim().to_string(),
        None => member
            .rsplit("::")
            .next()
            .unwrap_or(member)
            .trim()
            .to_string(),
    }
}

/// The first component type this signature names: an inline `crate::<component>::` path, or
/// one of the names a `use` bound, matched as a whole identifier so a longer name is not it.
pub(crate) fn component_type_in(
    signature: &str,
    components: &[String],
    imported: &[String],
) -> Option<String> {
    for component in components {
        let needle = format!("crate::{component}::");
        if let Some(i) = signature.find(&needle) {
            let path: String = signature[i..]
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == ':')
                .collect();
            return Some(path);
        }
    }
    imported
        .iter()
        .find(|name| names_identifier(signature, name))
        .cloned()
}

/// Does the signature use `name` as a whole identifier, so a longer name is not it?
fn names_identifier(signature: &str, name: &str) -> bool {
    signature.match_indices(name).any(|(i, _)| {
        let before = signature[..i].chars().next_back();
        let after = signature[i + name.len()..].chars().next();
        !before.is_some_and(|c| c.is_alphanumeric() || c == '_')
            && !after.is_some_and(|c| c.is_alphanumeric() || c == '_')
    })
}

/// Every `pub` field, with its line: a `pub` line that is not an item declaration.
pub(crate) fn pub_fields(text: &str) -> Vec<(usize, String)> {
    text.lines()
        .enumerate()
        .filter(|(_, line)| line.trim_start().starts_with("pub ") && !is_bare_pub_item(line))
        .map(|(n, line)| (n, line.to_string()))
        .collect()
}

/// A declaration without its initializer: a `pub const` or `pub static` runs to the `;`, and
/// `MODES` spells `BatchSizing::Budgeted` in its value without putting the type in its type.
pub(crate) fn signature_only(declaration: &str) -> String {
    let t = declaration.trim_start();
    let is_value = t.strip_prefix("pub ").is_some_and(|rest| {
        (rest.starts_with("const ") && !rest.starts_with("const fn "))
            || rest.starts_with("static ")
    });
    match (is_value, declaration.find('=')) {
        (true, Some(i)) => declaration[..i].to_string(),
        _ => declaration.to_string(),
    }
}

/// Every `pub` item's declaration, with the line it starts on: from the `pub` to the `{` or
/// `;` that ends the signature — or, for a `pub enum` and a `pub trait`, to the `}` closing
/// the body, since a variant's payload and a method's signature are the surface too.
///
/// The whole signature, not its first line: rustfmt wraps past 100 columns, and fourteen
/// `pub fn` in this crate's `mod.rs` files span several lines. A reader matching one line
/// could not see a parameter or a return type.
///
/// Only `(` and `[` nest. A `<` counted as an open leaves `1 << 20` two deep, so the `;` that
/// should end a `pub const` is missed and every declaration below it is swallowed.
pub(crate) fn pub_declarations(text: &str) -> Vec<(usize, String)> {
    declarations(text, is_bare_pub_item)
}

/// Every `type` alias below bare `pub` — `type` or `pub(…) type`, a `pub type` being a
/// declaration already — with the line it starts on.
pub(crate) fn type_aliases(text: &str) -> Vec<(usize, String)> {
    declarations(text, |line| {
        let t = line.trim_start();
        t.starts_with("type ")
            || t.strip_prefix("pub(")
                .and_then(|rest| rest.split_once(')'))
                .is_some_and(|(_, rest)| rest.trim_start().starts_with("type "))
    })
}

fn declarations(text: &str, starts: impl Fn(&str) -> bool) -> Vec<(usize, String)> {
    let lines: Vec<&str> = text.lines().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        if !starts(lines[i]) {
            i += 1;
            continue;
        }
        let start = i;
        let with_body = lines[i]
            .trim_start()
            .strip_prefix("pub ")
            .is_some_and(|rest| rest.starts_with("enum ") || rest.starts_with("trait "));
        let mut acc = String::new();
        let (mut depth, mut braces) = (0i32, 0i32);
        loop {
            acc.push_str(lines[i]);
            acc.push('\n');
            let mut done = false;
            for c in code_only(lines[i]).chars() {
                match c {
                    '(' | '[' => depth += 1,
                    ')' | ']' => depth -= 1,
                    '{' if with_body => braces += 1,
                    '}' if with_body => {
                        braces -= 1;
                        done |= braces == 0;
                    }
                    '{' | ';' if depth <= 0 && !with_body => done = true,
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

/// The modules a `mod.rs` declares privately: every `mod x;`.
pub(crate) fn private_modules(text: &str) -> Vec<String> {
    let declared: BTreeSet<String> = text
        .lines()
        .filter_map(|l| l.trim_start().strip_prefix("mod "))
        .map(|r| r.trim_end_matches(';').trim().to_string())
        .filter(|n| !n.is_empty() && n.chars().all(|c| c.is_alphanumeric() || c == '_'))
        .collect();
    declared.into_iter().collect()
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
        "pub fn control() { let _ = std::any::type_name::<peacockdb_core::executor::CpuBackend>(); }",
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
