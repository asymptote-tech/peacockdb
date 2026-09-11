//! Each reader, over the shape that would make it read nothing.

use std::path::Path;

use crate::privacy::{
    component_imports, component_type_in, component_types_on_the_surface, private_module_aliases,
    pub_declarations, pub_fields, signature_only, type_aliases,
};
use crate::repo_root;
use crate::test_code::{
    declared_item, declares_mod, doc_above, mod_declarations, names_cfg_feature, names_cfg_test,
    test_gates,
};
use crate::visibility::{files_naming, is_bare_pub_item, names_the_module, pub_mod_declarations};
use crate::walls::{supers_that_stay_inside, uses_module};

/// Each reader in the sibling modules, over the shape that would make it read nothing.
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

    // The harness's signature rule reads the names a `use crate::<component>` binds — wrapped,
    // aliased, grouped at the crate root — and a signature naming one of them or an inline path.
    // A field counts and a `pub(crate)` does not; a const's initializer is not its signature.
    let comps: Vec<String> = ["plan", "planner"].map(String::from).to_vec();
    assert_eq!(
        component_imports(
            "use crate::planner::{\n    BatchSizing, PlanKnobs as Knobs,\n};\nuse crate::plan;\n\
             use crate::{plan::Expr, wire::Recipe};\nuse crate::test_support::Mode;\n\
             use std::path::PathBuf;\n",
            &comps
        ),
        ["BatchSizing", "Knobs", "plan", "Expr"]
            .map(String::from)
            .to_vec(),
        "an alias is the name a signature uses, and a crate-root group is read member by member"
    );
    assert_eq!(
        component_imports("use crate::plan::*;\n", &comps),
        vec!["*".to_string()],
        "a glob binds names the reader cannot enumerate, and says so"
    );
    let knobs = vec!["PlanKnobs".to_string()];
    assert_eq!(
        component_type_in("pub fn tree() -> crate::plan::Expr {", &comps, &[]),
        Some("crate::plan::Expr".to_string())
    );
    assert_eq!(
        component_type_in("pub fn knobs(&self) -> PlanKnobs {", &comps, &knobs),
        Some("PlanKnobs".to_string())
    );
    assert_eq!(
        component_type_in("pub fn knobs(&self) -> PlanKnobsExt {", &comps, &knobs),
        None,
        "a longer identifier is a different name"
    );
    assert_eq!(
        component_type_in("pub fn root() -> PathBuf {", &comps, &knobs),
        None
    );
    assert_eq!(
        pub_fields(
            "pub struct Mode {\n    pub name: &'static str,\n    pub(crate) sizing: BatchSizing,\n}\n"
        ),
        vec![(1, "    pub name: &'static str,".to_string())],
        "a `pub` field is a surface, a `pub(crate)` one is not, and the struct line is an item"
    );
    assert_eq!(
        signature_only("pub const MODES: [Mode; 5] = [Mode { sizing: BatchSizing::Budgeted }];"),
        "pub const MODES: [Mode; 5] "
    );
    assert_eq!(
        signature_only("pub type Knobs = crate::planner::PlanKnobs;"),
        "pub type Knobs = crate::planner::PlanKnobs;",
        "an alias's right-hand side is exactly the type it puts on the surface"
    );
    assert_eq!(
        signature_only("pub const fn bytes(self) -> usize {"),
        "pub const fn bytes(self) -> usize {"
    );
    // `{self, …}` binds the module, not the word: read as `self`, a violation spelled
    // `planner::PlanKnobs` was reported as "names `self`" against every `&self` receiver.
    let module_and_member = ["planner", "BatchSizing"].map(String::from).to_vec();
    assert_eq!(
        component_imports("use crate::planner::{self, BatchSizing};\n", &comps),
        module_and_member,
        "`self` in a group is the module it sits under"
    );
    assert_eq!(
        component_imports("use crate::planner::{self as p, BatchSizing};\n", &comps),
        ["p", "BatchSizing"].map(String::from).to_vec()
    );
    assert_eq!(
        component_type_in(
            "pub fn knobs(&self) -> planner::PlanKnobs {",
            &comps,
            &module_and_member
        ),
        Some("planner".to_string()),
        "the module-qualified spelling is attributed to the module"
    );
    assert_eq!(
        component_type_in("pub fn knobs(&self) -> usize {", &comps, &module_and_member),
        None,
        "a receiver is not the module"
    );

    // A signature is not only what sits before the first `{`. A variant's payload and a trait's
    // method signatures live inside the braces, and an alias moves the type out of the `pub`
    // line altogether; each was green until the reader followed it.
    let variants = pub_declarations(
        "pub enum Outcome {\n    Planned(Box<dyn crate::plan::GpuNode>),\n    Skipped,\n}\n\
         pub fn after() -> u8 {",
    );
    assert_eq!(
        variants.len(),
        2,
        "the body ends at its own brace: {variants:?}"
    );
    assert!(
        variants[0]
            .1
            .contains("Planned(Box<dyn crate::plan::GpuNode>)"),
        "a variant's payload is part of the enum's surface"
    );
    let methods = pub_declarations(
        "pub trait Probe {\n    fn tree(&self) -> Box<dyn crate::plan::GpuNode>;\n}\n\
         pub fn after() -> u8 {",
    );
    assert_eq!(
        methods.len(),
        2,
        "a `;` inside the body must not end it: {methods:?}"
    );
    assert!(
        methods[0]
            .1
            .contains("fn tree(&self) -> Box<dyn crate::plan::GpuNode>;")
    );
    assert_eq!(
        type_aliases(
            "type Tree = Box<dyn crate::plan::GpuNode>;\npub(crate) type Rows =\n    Vec<u8>;\n\
             pub type Knobs = usize;\nlet t: Tree = tree();\n"
        ),
        vec![
            (
                0,
                "type Tree = Box<dyn crate::plan::GpuNode>;\n".to_string()
            ),
            (1, "pub(crate) type Rows =\n    Vec<u8>;\n".to_string()),
        ],
        "every alias below bare `pub`, wrapped or not; a `pub type` is already a declaration"
    );

    // The composition, over the spec's probe and the three spellings that passed it, and over
    // the same shapes carrying only std and harness types.
    let violations = "use crate::plan::GpuNode;\n\
        pub fn tree() -> Box<dyn GpuNode> {\n    unimplemented!()\n}\n\
        pub enum Outcome {\n    Planned(Box<dyn crate::plan::GpuNode>),\n}\n\
        pub trait Probe {\n    fn tree(&self) -> Box<dyn crate::plan::GpuNode>;\n}\n\
        type Tree = Box<dyn crate::plan::GpuNode>;\n\
        pub fn laundered() -> Tree {\n    unimplemented!()\n}\n";
    let found: Vec<(usize, String)> = component_types_on_the_surface(violations, &comps)
        .into_iter()
        .map(|(n, _, name)| (n, name))
        .collect();
    assert_eq!(
        found,
        vec![
            (1, "GpuNode".to_string()),
            (4, "crate::plan::GpuNode".to_string()),
            (7, "crate::plan::GpuNode".to_string()),
            (10, "crate::plan::GpuNode".to_string()),
        ],
        "each spelling is reported once, at the line that declares it"
    );
    let near_misses = "use std::collections::HashMap;\n\
        use crate::planner::{self, BatchSizing};\n\
        pub fn root() -> PathBuf {\n    unimplemented!()\n}\n\
        pub enum Outcome {\n    Rows(Vec<RecordBatch>),\n}\n\
        pub trait Probe {\n    fn mode(&self) -> Mode;\n}\n\
        type Rows = HashMap<String, usize>;\n\
        pub fn take(rows: &mut Rows) -> u64 {\n    unimplemented!()\n}\n\
        pub struct Mode {\n    pub name: &'static str,\n    pub(crate) sizing: BatchSizing,\n}\n\
        pub const MODES: [Mode; 1] = [Mode { sizing: BatchSizing::Budgeted }];\n";
    assert_eq!(
        component_types_on_the_surface(near_misses, &comps),
        vec![],
        "std, arrow and the harness's own types are what a signature may carry"
    );

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
    // A gate on the same line as the declaration, which rustfmt leaves alone. Skipped, the
    // module is invisible to both rung rules — a `gpu_tests` gated `#[cfg(test)]` this way
    // was green under both.
    assert_eq!(
        declares_mod("#[cfg(test)] mod gpu_tests;"),
        Some(("gpu_tests".into(), false)),
        "an attribute on the line must not hide the declaration"
    );
    let one_line = mod_declarations("#[cfg(test)] mod gpu_tests;\n");
    assert_eq!(one_line.len(), 1);
    assert_eq!(
        one_line[0].gate.as_deref(),
        Some("#[cfg(test)]"),
        "the gate on the declaration's own line is the module's"
    );
    assert_eq!(
        one_line[0].name, "gpu_tests",
        "the name is read past the attribute"
    );

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
        test_gates("#[cfg(test)] fn hops() {}\nmod tests;\n")[0].sits_on,
        "fn hops() {}",
        "a gate sits on the rest of its own line before it sits on the next"
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
