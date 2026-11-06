//! Every case the list declares against each device's committed tree: a new device's tree is
//! held to the whole list, less the modes `UNTIMED` names for it, and `h200/` to the list it was
//! measured from.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use peacockdb_core::test_support::{ordered_sections, testdata_root};

use super::benchmark::{
    PREDATES_PROVENANCE, case_list, device_directories, files_under, modes, synthesized_tree,
};

/// The `(dataset, sf, mode)` files a case list declares, each with the sections it is to hold.
type Files = BTreeMap<(String, String, String), BTreeSet<String>>;

/// The files `list`'s lines declare. A query declared `none` is a marker section in every file
/// of its dataset, as `declared_for` writes it.
fn declared_files(list: &str, cases: &[[&str; 4]]) -> Files {
    let mut files = Files::new();
    for [dataset, sf, query, timed] in cases {
        for mode in modes(timed) {
            files
                .entry((dataset.to_string(), sf.to_string(), mode))
                .or_default()
                .insert(query.replace('_', "-"));
        }
    }
    for [dataset, sf, query, _] in cases.iter().filter(|[.., timed]| timed.trim() == "none") {
        for ((of, at, _), queries) in files.iter_mut() {
            if (of.as_str(), at.as_str()) == (*dataset, *sf) {
                queries.insert(query.replace('_', "-"));
            }
        }
    }
    assert!(!files.is_empty(), "{list} declares no mode");
    files
}

/// One line of `h200/cases.inc`, the list the committed H200 tree was measured from.
struct H200Declaration {
    dataset: &'static str,
    sf: &'static str,
    query: &'static str,
    modes: &'static str,
}

inventory::collect!(H200Declaration);

// The H200's list under a macro of its own, so its lines land in their own inventory.
mod h200_list {
    macro_rules! corpus_query_benchmark {
        ($dataset:ident, $sf:expr, $query:ident, $($modes:tt)+) => {
            inventory::submit! {
                super::H200Declaration {
                    dataset: stringify!($dataset),
                    sf: stringify!($sf),
                    query: stringify!($query),
                    modes: stringify!($($modes)+),
                }
            }
        };
    }

    include!("../../../testdata/benchmark-results/h200/cases.inc");
}

/// The list a tree in `PREDATES_PROVENANCE` was measured from, compiled in. A device in that
/// list with no list here is refused, never held to another device's.
fn predates_list(device: &str) -> Files {
    match device {
        "h200" => {
            let cases: Vec<[&str; 4]> = inventory::iter::<H200Declaration>
                .into_iter()
                .map(|case| [case.dataset, case.sf, case.query, case.modes])
                .collect();
            declared_files("h200/cases.inc", &cases)
        }
        other => panic!("{other} is in PREDATES_PROVENANCE and has no list it was measured from"),
    }
}

/// Modes a device's committed tree was not timed at, held out of the case list by name.
struct Untimed {
    device: &'static str,
    modes: &'static [&'static str],
    reason: &'static str,
}

/// The gaps the committed trees may have. An entry naming a mode the list declares nowhere, one
/// whose device has no tree, or one whose tree carries a file at one of its modes excuses nothing
/// and is refused.
const UNTIMED: &[Untimed] = &[Untimed {
    device: "h100-80gb-hbm3",
    modes: &["tp4_sized"],
    reason: "not timed on the H100: the mode's run was stopped after seven tpcds cases",
}];

/// Every case the list declares has its section in each device's committed tree. The exceptions
/// are a tree in `PREDATES_PROVENANCE`, which is held to the list it was measured from instead,
/// and the modes `UNTIMED` names for one device.
///
/// The tree accumulates across runs and nothing else compares it with the list, so a case
/// added and never measured would leave its file silently short.
#[test]
fn every_declared_case_has_a_section_in_the_tree() {
    let root = testdata_root().join("benchmark-results");
    if let Err(problem) = missing_case_problem(
        &root,
        &declared_files("the case list", &case_list()),
        UNTIMED,
    ) {
        panic!("{problem}");
    }
}

/// The committed H200 file under another device's name: a new tree, so held to the whole list.
#[test]
fn a_new_device_tree_is_held_to_every_declared_case() {
    let tree = synthesized_tree(
        "short-tree",
        "h100-80gb-hbm3",
        Some("NVIDIA H100 80GB HBM3"),
    );
    let problem = missing_case_problem(&tree, &declared_files("the case list", &case_list()), &[]);
    let _ = std::fs::remove_dir_all(&tree);
    let problem = problem.expect_err("a tree of one file passed against the whole list");
    assert!(problem.contains("h100-80gb-hbm3"), "{problem}");
}

/// A file that exists and lacks one declared section: named, rather than read as whole.
#[test]
fn a_file_short_of_a_declared_section_is_refused() {
    let tree = synthesized_tree(
        "short-file",
        "h100-80gb-hbm3",
        Some("NVIDIA H100 80GB HBM3"),
    );
    let one_file = Files::from([(
        (
            "tpch".to_string(),
            "40".to_string(),
            "tp1_single".to_string(),
        ),
        BTreeSet::from(["q1".to_string(), "q2".to_string()]),
    )]);
    let problem = missing_case_problem(&tree, &one_file, &[]);
    let _ = std::fs::remove_dir_all(&tree);
    let problem = problem.expect_err("q2 is in no section of the file and the file passed");
    assert!(problem.contains(r#"no section for ["q2"]"#), "{problem}");
}

/// The first declared case a device tree under `root` has no section for. A failed case's
/// record is its section.
///
/// A tree in `PREDATES_PROVENANCE` is held to the list it was measured from, its
/// `predates_list`: its card is outside the run that would complete it to `files`. A device's
/// modes in `untimed` are not read, once every entry is shown still to excuse something.
fn missing_case_problem(root: &Path, files: &Files, untimed: &[Untimed]) -> Result<(), String> {
    for entry in untimed {
        dead_untimed_problem(root, files, entry)?;
    }
    for device in device_directories(root) {
        let name = device.file_name().unwrap().to_str().unwrap();
        let own;
        let held_to = match PREDATES_PROVENANCE.contains(&name) {
            true => {
                own = predates_list(name);
                &own
            }
            false => files,
        };
        for ((dataset, sf, mode), declared) in held_to {
            let excused = untimed
                .iter()
                .any(|entry| entry.device == name && entry.modes.contains(&mode.as_str()));
            if excused {
                continue;
            }
            let path = device.join(format!("{dataset}.sf{sf}/{mode}.benchmark.txt"));
            let text = std::fs::read_to_string(&path).map_err(|e| {
                format!(
                    "{}: {e} — {dataset}/sf{sf} is declared at {mode} and was never measured \
                     on this device",
                    path.display()
                )
            })?;
            let present: BTreeSet<String> = ordered_sections(&text)
                .into_iter()
                .map(|(query, _)| query)
                .collect();
            let missing: Vec<&String> = declared.difference(&present).collect();
            if !missing.is_empty() {
                return Err(format!(
                    "{} has no section for {missing:?}, which the case list declares at {mode}",
                    path.display()
                ));
            }
        }
    }
    Ok(())
}

/// Why `entry` excuses nothing, if it does not: a mode it names is one no file of `files` is
/// declared at, its device has no tree, or the tree carries a file at one of its modes, which is
/// then held to the list like any other.
fn dead_untimed_problem(root: &Path, files: &Files, entry: &Untimed) -> Result<(), String> {
    let Untimed {
        device,
        modes,
        reason,
    } = entry;
    let undeclared: Vec<&str> = modes
        .iter()
        .copied()
        .filter(|mode| !files.keys().any(|(_, _, declared)| declared == mode))
        .collect();
    if !undeclared.is_empty() {
        return Err(format!(
            "`UNTIMED` excuses {device}/ from {} ({reason}), which the case list declares \
             nowhere — remove it",
            undeclared.join(", ")
        ));
    }
    let tree = root.join(device);
    if !tree.is_dir() {
        return Err(format!(
            "{}: {device}/ is no device's tree, and `UNTIMED` excuses it from {modes:?} \
             ({reason}) — remove the entry",
            root.display()
        ));
    }
    // One file at a named mode is enough to make the entry dead; the first names it.
    let timed = files_under(&tree).into_iter().find(|path| {
        modes
            .iter()
            .any(|mode| path.ends_with(format!("{mode}.benchmark.txt")))
    });
    match timed {
        Some(path) => Err(format!(
            "{}: `UNTIMED` excuses {device}/ from {modes:?} ({reason}), and the tree now \
             carries this file — remove the entry, and the tree is held to the list there",
            path.display()
        )),
        None => Ok(()),
    }
}

/// The entry the cases below hold the check to, apart from `UNTIMED` so they outlive it.
const SIZED_UNTIMED: Untimed = Untimed {
    device: "h100-80gb-hbm3",
    modes: &["tp4_sized"],
    reason: "the reason it was not timed",
};

/// tpch sf40 declaring q1 at each of `modes`.
fn q1_at(modes: &[&str]) -> Files {
    modes
        .iter()
        .map(|mode| {
            let file = ("tpch".to_string(), "40".to_string(), mode.to_string());
            (file, BTreeSet::from(["q1".to_string()]))
        })
        .collect()
}

/// The check, with `untimed`, over a temp tree holding the committed H200 tpch tp1_single file
/// at each `(device, mode)`, the tree removed whatever it answers.
fn coverage_of(
    case: &str,
    at: &[(&str, &str)],
    files: &Files,
    untimed: &[Untimed],
) -> Result<(), String> {
    let root = std::env::temp_dir().join(format!("peacock-{case}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let text = std::fs::read_to_string(
        testdata_root().join("benchmark-results/h200/tpch.sf40/tp1_single.benchmark.txt"),
    )
    .expect("the committed file");
    for (device, mode) in at {
        let path = root.join(format!("{device}/tpch.sf40/{mode}.benchmark.txt"));
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, &text).unwrap();
    }
    let problem = missing_case_problem(&root, files, untimed);
    let _ = std::fs::remove_dir_all(&root);
    problem
}

/// The device's tree without its untimed mode's file passes, and fails without the entry.
#[test]
fn an_untimed_mode_excuses_its_devices_files_at_that_mode() {
    let at = [("h100-80gb-hbm3", "tp1_single")];
    let files = q1_at(&["tp1_single", "tp4_sized"]);
    assert_eq!(
        coverage_of("untimed-excused", &at, &files, &[SIZED_UNTIMED]),
        Ok(())
    );
    let problem = coverage_of("untimed-absent", &at, &files, &[])
        .expect_err("a tree with no tp4_sized file passed with no entry excusing it");
    assert!(problem.contains("tp4_sized.benchmark.txt"), "{problem}");
}

/// A mode the entry does not name is held to the list on its device.
#[test]
fn an_untimed_mode_excuses_no_other_mode() {
    let at = [("h100-80gb-hbm3", "tp1_single")];
    let files = q1_at(&["tp1_single", "tp4_single", "tp4_sized"]);
    let problem = coverage_of("untimed-other-mode", &at, &files, &[SIZED_UNTIMED])
        .expect_err("an entry for tp4_sized excused a missing tp4_single file");
    assert!(
        problem.contains("h100-80gb-hbm3/tpch.sf40/tp4_single.benchmark.txt"),
        "{problem}"
    );
}

/// Another device's tree is held to the list at the entry's mode.
#[test]
fn an_untimed_mode_excuses_no_other_device() {
    let at = [("h100-80gb-hbm3", "tp1_single"), ("l40s", "tp1_single")];
    let files = q1_at(&["tp1_single", "tp4_sized"]);
    let problem = coverage_of("untimed-other-device", &at, &files, &[SIZED_UNTIMED])
        .expect_err("an entry for h100-80gb-hbm3 excused l40s's missing tp4_sized file");
    assert!(
        problem.contains("l40s/tpch.sf40/tp4_sized.benchmark.txt"),
        "{problem}"
    );
}

/// An entry for a device with no tree excuses nothing.
#[test]
fn an_untimed_mode_whose_device_has_no_tree_is_refused() {
    let at = [("l40s", "tp1_single")];
    let problem = coverage_of(
        "untimed-no-tree",
        &at,
        &q1_at(&["tp1_single", "tp4_sized"]),
        &[SIZED_UNTIMED],
    )
    .expect_err("an entry for a device with no tree passed");
    assert!(
        problem.contains("h100-80gb-hbm3/ is no device's tree"),
        "{problem}"
    );
}

/// An entry whose tree now carries the mode's file excuses nothing: the file is held to the list.
#[test]
fn an_untimed_mode_the_tree_now_carries_is_refused() {
    let at = [
        ("h100-80gb-hbm3", "tp1_single"),
        ("h100-80gb-hbm3", "tp4_sized"),
    ];
    let files = q1_at(&["tp1_single", "tp4_sized"]);
    let problem = coverage_of("untimed-timed", &at, &files, &[SIZED_UNTIMED])
        .expect_err("an entry for tp4_sized passed beside a tp4_sized file");
    assert!(
        problem.contains("h100-80gb-hbm3/tpch.sf40/tp4_sized.benchmark.txt")
            && problem.contains("the reason it was not timed"),
        "{problem}"
    );
}

/// An entry for a mode no declared file has excuses nothing.
#[test]
fn an_untimed_mode_the_list_does_not_declare_is_refused() {
    let at = [("h100-80gb-hbm3", "tp1_single")];
    let problem = coverage_of(
        "untimed-undeclared",
        &at,
        &q1_at(&["tp1_single"]),
        &[SIZED_UNTIMED],
    )
    .expect_err("an entry for a mode the list never declares passed");
    assert!(
        problem.contains(
            "from tp4_sized (the reason it was not timed), which the case list \
                          declares nowhere"
        ),
        "{problem}"
    );
}
