//! Each reader, over the shape that would make it read nothing.

use std::path::Path;

use crate::privacy::{
    component_imports, component_type_in, component_types_on_the_surface, private_module_imports,
    private_modules, private_name_in, pub_declarations, pub_fields, signature_only, type_aliases,
};
use crate::test_code::{
    declared_item, declares_mod, doc_above, mod_declarations, names_cfg_feature, names_cfg_test,
    test_gates,
};
use crate::visibility::{
    bare_pub_name, gated_pub_mods, is_bare_pub_item, pub_mod_declarations, visible_mod_declarations,
};
use crate::walls::{super_chains, supers_that_stay_inside, uses_module};

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

    // rustfmt keeps a short attribute on the declaration's own line, so a reader that wants
    // the line to start with `pub` sees neither `#[cfg(feature = "gpu")] pub mod x;` nor
    // `#[allow(dead_code)] pub fn x()`; and a `pub(crate) mod` opens a wall while matching
    // neither reader, so the visibility reader sees every spelling.
    assert_eq!(
        pub_mod_declarations("#[cfg(feature = \"gpu\")] pub mod gpu_backend;"),
        ["gpu_backend"]
    );
    assert!(is_bare_pub_item("#[allow(dead_code)] pub fn x() {}"));
    assert_eq!(
        bare_pub_name("#[allow(dead_code)] pub fn x() {}").as_deref(),
        Some("x")
    );
    assert_eq!(
        visible_mod_declarations(
            "mod a;\npub mod b;\npub(crate) mod c;\n#[cfg(test)] pub(in crate::x) mod d;\n"
        ),
        [
            ("pub".to_string(), "b".to_string()),
            ("pub(crate)".to_string(), "c".to_string()),
            ("pub(in crate::x)".to_string(), "d".to_string()),
        ]
    );
    assert_eq!(
        pub_fields("#[serde(skip)] pub rows: u64,\npub(crate) bytes: u64,\n"),
        [(0, "#[serde(skip)] pub rows: u64,".to_string())]
    );
    assert_eq!(
        bare_pub_name("    pub batches: Vec<CpuBatch>,").as_deref(),
        Some("batches")
    );

    // `pub(crate)` and a `pub` field are not items the facade has to declare.
    assert!(is_bare_pub_item("pub fn run() {}"));
    assert!(is_bare_pub_item("    pub struct Held<T> {"));
    assert!(!is_bare_pub_item("pub(crate) fn run() {}"));
    assert!(!is_bare_pub_item("    pub batches: Vec<CpuBatch>,"));

    // The surface matches by name, so the name has to be the one after every keyword a
    // declaration can carry, and a method's name, not `self`.
    assert_eq!(
        bare_pub_name("pub async fn register_tables_for(").as_deref(),
        Some("register_tables_for")
    );
    assert_eq!(
        bare_pub_name("    pub fn record_batch(&self) -> &RecordBatch {").as_deref(),
        Some("record_batch")
    );
    assert_eq!(
        bare_pub_name("pub const SMALL_TABLE_BYTES: u64 = 1;").as_deref(),
        Some("SMALL_TABLE_BYTES")
    );
    assert_eq!(
        bare_pub_name("pub unsafe extern \"C\" fn abi() {}").as_deref(),
        Some("abi")
    );
    assert_eq!(
        bare_pub_name("pub trait Backend: Sized {").as_deref(),
        Some("Backend")
    );
    assert_eq!(bare_pub_name("pub(crate) fn run() {}"), None);

    // A gate is the line above or the same line, and only a `cfg` is one: a doc comment or
    // an attribute that is not `cfg` leaves the declaration unconditional.
    let (plain, gated) = gated_pub_mods(
        "pub mod plan;\n#[cfg(feature = \"test-support\")]\npub mod test_support;\n\
         /// docs\npub mod wire;\n#[allow(dead_code)]\npub mod x;\n#[cfg(test)] pub mod y;\n",
    );
    assert_eq!(plain, ["plan", "wire", "x"]);
    assert_eq!(
        gated,
        [
            (
                "test_support".to_string(),
                "feature = \"test-support\"".to_string()
            ),
            ("y".to_string(), "test".to_string()),
        ]
    );

    // A private module's types reach a signature three ways: the module's path, an alias out
    // of it (`fb::PlanNodeKind` carries no hint of `generated`), and a bare name a `use`
    // bound — which is the spelling every `mod.rs` in the crate actually writes.
    let m = "mod generated;\nmod read;\nmod accounting;\npub mod driver;\n\
             use generated::peacock::plan as fb;\nuse self::accounting::{Trip, trip_of};\n\
             use read::Reader as R;\nuse driver::Step;\nuse std::fmt::Debug;\n";
    let modules = private_modules(m);
    assert_eq!(
        modules,
        ["accounting", "generated", "read"]
            .map(String::from)
            .to_vec(),
        "a `pub mod` is not private, so what comes out of it is not this rule's business"
    );
    let mut names = private_module_imports(m, &modules);
    names.extend(modules);
    assert_eq!(
        names,
        [
            "fb",
            "Trip",
            "trip_of",
            "R",
            "accounting",
            "generated",
            "read"
        ]
        .map(String::from)
        .to_vec(),
        "an alias, a group member and a `self::` path each bind a name; `driver` and std do not"
    );
    assert_eq!(
        private_name_in("pub fn kind(&self) -> fb::PlanNodeKind {", &names),
        Some("fb".to_string())
    );
    assert_eq!(
        private_name_in("pub fn step(trip: Trip) -> Trip {", &names),
        Some("Trip".to_string()),
        "the bare imported type is the spelling that passed before"
    );
    assert_eq!(
        private_name_in("pub fn step(trip: Tripwire) -> Tripwire {", &names),
        None,
        "a longer identifier is a different name"
    );
    assert_eq!(
        private_name_in("pub fn read(read: usize, accounting: u8) -> Step {", &names),
        None,
        "a parameter named like a module is not a path into it"
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
    // A brace in a doc comment is prose. Counted, a `}` there closed the body early and the
    // variants below it went unread — silently, since a short capture reports nothing.
    let prose =
        "pub enum E {\n    /// like `}` in prose\n    A(Box<dyn crate::plan::GpuNode>),\n}\n";
    assert_eq!(
        component_types_on_the_surface(prose, &comps)
            .into_iter()
            .map(|(n, _, name)| (n, name))
            .collect::<Vec<_>>(),
        vec![(0, "crate::plan::GpuNode".to_string())],
        "the body runs to the brace that is code"
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
    // One chain is one climb, however long: resuming the scan one `super::` in reported
    // `super::super::x` twice, once at two and once at one.
    assert_eq!(super_chains("use super::super::x;"), vec![2]);
    assert_eq!(
        super_chains("super::a::f(super::b)"),
        vec![1, 1],
        "two chains on a line are two"
    );
    assert!(super_chains("use crate::plan::GpuNode;").is_empty());

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
}
