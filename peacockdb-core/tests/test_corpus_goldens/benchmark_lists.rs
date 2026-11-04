//! Each device tree against its case list, with no device and no run: a tree holding its own
//! `cases.csv` answers to that list's rows, and those rows to the main list; a tree without
//! answers to the whole main list. Other engines' trees beside the devices are no device.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use peacockdb_core::test_support::{
    DEVICE_CASES_HEADER, ordered_sections, parse_device_cases, testdata_root,
};

use super::benchmark::{Case, PREDATES_PROVENANCE, listed_cases, modes, synthesized_tree};

/// The `(dataset, sf, mode)` files a case list declares, each with the sections it is to hold.
type Files = BTreeMap<(String, String, String), BTreeSet<String>>;

/// The files `cases` declare. A query declared `none` is a marker section in every file of its
/// dataset, as `declared_for` writes it.
fn declared_files(cases: &[Case]) -> Files {
    let mut files = Files::new();
    for args in cases {
        for mode in modes(&args[3]) {
            files
                .entry((args[0].clone(), args[1].clone(), mode))
                .or_default()
                .insert(args[2].replace('_', "-"));
        }
    }
    for args in cases.iter().filter(|args| args[3] == "none") {
        for ((dataset, sf, _), queries) in files.iter_mut() {
            if (dataset, sf) == (&args[0], &args[1]) {
                queries.insert(args[2].replace('_', "-"));
            }
        }
    }
    assert!(!files.is_empty(), "a case list declares no mode: {cases:?}");
    files
}

/// The list a device tree is held to when the tree holds one (`<device>/cases.csv`); a tree
/// without is held to the whole main list.
const DEVICE_LIST: &str = "cases.csv";

/// `ok` and `wrong` rows, each mode → its `(dataset, sf, mode)` file → the query, `_` → `-`, as
/// `declared_files` keys them. A `skipped` row declares no section: its modes are empty.
fn device_list_files(path: &Path) -> Files {
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let mut files = Files::new();
    for case in parse_device_cases(&text) {
        for mode in case.modes {
            files
                .entry((case.dataset.clone(), case.sf.clone(), mode))
                .or_default()
                .insert(case.query.replace('_', "-"));
        }
    }
    files
}

/// Every case the list declares has its section in each device's committed tree. The exception is
/// a tree that holds its own `DEVICE_LIST`, which is held to that list instead.
///
/// The tree accumulates across runs and nothing else compares it with the list, so a case
/// added and never measured would leave its file silently short.
#[test]
fn every_declared_case_has_a_section_in_the_tree() {
    let root = testdata_root().join("benchmark-results");
    if let Err(problem) = missing_case_problem(&root, &declared_files(&listed_cases())) {
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
    let problem = missing_case_problem(&tree, &declared_files(&listed_cases()));
    let _ = std::fs::remove_dir_all(&tree);
    let problem = problem.expect_err("a tree of one file passed against the whole list");
    assert!(problem.contains("h100-80gb-hbm3"), "{problem}");
}

/// A tree that holds a `DEVICE_LIST` answers to its rows and not to the main list: a case it
/// lists `ok` and lacks a section for is refused by name, and the same case `skipped` is not.
#[test]
fn a_tree_with_its_own_list_is_held_to_it() {
    let tree = synthesized_tree("own-list", "h100-80gb-hbm3", Some("NVIDIA H100 80GB HBM3"));
    let main = declared_files(&listed_cases());
    let held = |rows: &str| {
        let list = tree.join("h100-80gb-hbm3").join(DEVICE_LIST);
        std::fs::write(&list, format!("{DEVICE_CASES_HEADER}\n{rows}")).unwrap();
        missing_case_problem(&tree, &main)
    };
    let one = held("tpch,40,q1,tp1_single,ok,\n");
    let unmeasured = held("tpch,40,q1,tp1_single,ok,\ntpch,40,q2,tp1_single,ok,\n");
    let skipped = held("tpch,40,q1,tp1_single,ok,\ntpch,40,q2,,skipped,not measured\n");
    let _ = std::fs::remove_dir_all(&tree);
    assert_eq!(one, Ok(()), "the tree's one file holds q1");
    let problem = unmeasured.expect_err("q2 is listed ok and has no section");
    assert!(problem.contains(r#"no section for ["q2"]"#), "{problem}");
    assert_eq!(skipped, Ok(()), "a skipped case declares no section");
}

/// A device list's rows are main-list cases: a query the main list lacks is refused naming it and
/// the device, though the tree holds its section.
#[test]
fn a_device_list_beyond_the_case_list_is_refused() {
    let tree = synthesized_tree(
        "beyond-list",
        "h100-80gb-hbm3",
        Some("NVIDIA H100 80GB HBM3"),
    );
    let device = tree.join("h100-80gb-hbm3");
    let file = device.join("tpch.sf40/tp1_single.benchmark.txt");
    let text = std::fs::read_to_string(&file).unwrap();
    std::fs::write(&file, text.replace("== q1\n", "== q99\n")).unwrap();
    std::fs::write(
        device.join(DEVICE_LIST),
        format!("{DEVICE_CASES_HEADER}\ntpch,40,q99,tp1_single,ok,\n"),
    )
    .unwrap();
    let problem = missing_case_problem(&tree, &declared_files(&listed_cases()));
    let _ = std::fs::remove_dir_all(&tree);
    let problem = problem.expect_err("a list naming q99, which the main list lacks, passed");
    assert!(
        problem.contains("q99") && problem.contains("h100-80gb-hbm3/cases.csv"),
        "{problem}"
    );
}

/// A row at a mode the main list's line does not carry is refused naming the mode. Every line of
/// the committed list carries all five modes, so the main list here is one line at `tp1_single`.
#[test]
fn a_device_list_at_a_mode_the_case_list_lacks_is_refused() {
    let tree = synthesized_tree(
        "beyond-mode",
        "h100-80gb-hbm3",
        Some("NVIDIA H100 80GB HBM3"),
    );
    let device = tree.join("h100-80gb-hbm3");
    std::fs::copy(
        device.join("tpch.sf40/tp1_single.benchmark.txt"),
        device.join("tpch.sf40/tp4_single.benchmark.txt"),
    )
    .unwrap();
    std::fs::write(
        device.join(DEVICE_LIST),
        format!("{DEVICE_CASES_HEADER}\ntpch,40,q1,tp1_single|tp4_single,ok,\n"),
    )
    .unwrap();
    let main = Files::from([(
        (
            "tpch".to_string(),
            "40".to_string(),
            "tp1_single".to_string(),
        ),
        BTreeSet::from(["q1".to_string()]),
    )]);
    let problem = missing_case_problem(&tree, &main);
    let _ = std::fs::remove_dir_all(&tree);
    let problem = problem.expect_err("q1 at tp4_single, which its line lacks, passed");
    assert!(
        problem.contains("q1") && problem.contains("at tp4_single"),
        "{problem}"
    );
}

/// Another engine's results beside the devices are no device tree: not listed, and not held to
/// the main list.
#[test]
fn the_duckdb_tree_is_no_device_tree() {
    let root = std::env::temp_dir().join(format!("peacock-other-engine-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    copy_tree(
        &testdata_root().join("benchmark-results/h200"),
        &root.join("h200"),
    );
    let tsv = root.join("duckdb-cpu128/tpch.sf40/duckdb.tsv");
    std::fs::create_dir_all(tsv.parent().unwrap()).unwrap();
    std::fs::write(&tsv, "# duckdb=1.5.4 threads=128 host=nebius-cpu128\n").unwrap();
    let devices = device_directories(&root);
    let problem = missing_case_problem(&root, &declared_files(&listed_cases()));
    let _ = std::fs::remove_dir_all(&root);
    assert_eq!(devices, [root.join("h200")]);
    assert_eq!(problem, Ok(()));
}

/// `from` and everything under it, as `to`.
fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap().flatten() {
        let path = entry.path();
        let target = to.join(entry.file_name());
        match path.is_dir() {
            true => copy_tree(&path, &target),
            false => {
                std::fs::copy(&path, &target).unwrap();
            }
        }
    }
}

/// Every committed device list reads under the format's rules, and the H200's is committed.
#[test]
fn every_device_list_parses() {
    let root = testdata_root().join("benchmark-results");
    let lists: Vec<PathBuf> = device_directories(&root)
        .into_iter()
        .map(|device| device.join(DEVICE_LIST))
        .filter(|list| list.is_file())
        .collect();
    assert!(
        lists.contains(&root.join("h200").join(DEVICE_LIST)),
        "{} holds no h200/{DEVICE_LIST}",
        root.display()
    );
    for list in lists {
        let text = std::fs::read_to_string(&list).unwrap();
        assert!(
            !parse_device_cases(&text).is_empty(),
            "{} lists no case",
            list.display()
        );
    }
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
    let problem = missing_case_problem(&tree, &one_file);
    let _ = std::fs::remove_dir_all(&tree);
    let problem = problem.expect_err("q2 is in no section of the file and the file passed");
    assert!(problem.contains(r#"no section for ["q2"]"#), "{problem}");
}

/// The first declared case a device tree under `root` has no section for. A failed case's
/// record is its section.
///
/// A tree that holds a `DEVICE_LIST` is held to it, and one without to `files`: the file's
/// presence decides, not the device's name. The list's own rows are held to `files` first.
fn missing_case_problem(root: &Path, files: &Files) -> Result<(), String> {
    for device in device_directories(root) {
        let list = device.join(DEVICE_LIST);
        let own;
        let held_to = match list.is_file() {
            true => {
                own = device_list_files(&list);
                let name = device.file_name().unwrap().to_str().unwrap();
                device_list_problem(name, &own, files)?;
                &own
            }
            false => files,
        };
        for ((dataset, sf, mode), declared) in held_to {
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

/// A device list's `ok` and `wrong` rows are cases of the main list, at modes the main list
/// carries for them. A tree in `PREDATES_PROVENANCE` was measured from an older list: exempt.
fn device_list_problem(device: &str, own: &Files, list: &Files) -> Result<(), String> {
    if PREDATES_PROVENANCE.contains(&device) {
        return Ok(());
    }
    for ((dataset, sf, mode), queries) in own {
        let at_mode = list.get(&(dataset.clone(), sf.clone(), mode.clone()));
        for query in queries {
            if at_mode.is_some_and(|listed| listed.contains(query)) {
                continue;
            }
            let listed = list
                .iter()
                .any(|((d, s, _), listed)| (d, s) == (dataset, sf) && listed.contains(query));
            return Err(match listed {
                true => format!(
                    "{device}/{DEVICE_LIST}: {dataset} sf{sf} {query} at {mode}, a mode the main \
                     case list does not carry for it"
                ),
                false => format!(
                    "{device}/{DEVICE_LIST}: {dataset} sf{sf} {query} is no case of the main case \
                     list"
                ),
            });
        }
    }
    Ok(())
}

/// Other engines' result trees under `benchmark-results/`, read by their own tests: no device.
const OTHER_ENGINES: &[&str] = &["duckdb-cpu128"];

/// Every directory directly under `root` but `OTHER_ENGINES`, each one device's tree.
fn device_directories(root: &Path) -> Vec<PathBuf> {
    let mut devices: Vec<PathBuf> = std::fs::read_dir(root)
        .unwrap_or_else(|e| panic!("{}: {e}", root.display()))
        .flatten()
        .filter(|entry| !OTHER_ENGINES.contains(&entry.file_name().to_str().unwrap()))
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect();
    devices.sort();
    assert!(!devices.is_empty(), "{} holds no device", root.display());
    devices
}
