//! The calibration record's format, with no device and no run: the preamble against what the
//! writer produces today, and the checker's refusals.

use std::collections::{BTreeMap, BTreeSet};

use peacockdb_core::test_support::{
    COLUMNS, Capture, Provenance, RECORD_PATH_ENV, RunMeta, append_records, record_header,
    rows_match_the_recipes,
};

/// A record's preamble, read as its readers read it, is the one `record_header` writes.
///
/// Three Python scripts and a person read the record, and none can ask this source what a
/// column means, so the preamble is the column documentation. They take every `#` line, then
/// one column line: a note that lost its `#` ends the preamble early and fails here.
///
/// Written by the harness's own writer into a temp dir, since the measured record is not in
/// git. `allocator=` is read back, as it names the host that measured and no build can know
/// it; its value carries `=` and spaces, as a pool's does. `capture=none` is not read back: a
/// captured run's rows are the distorted ones, and a published record must not be one.
#[test]
fn the_records_preamble_is_what_record_header_writes() {
    let dir = std::env::temp_dir().join(format!("peacock-corpus-{}-record", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let path = dir.join("records.tsv");
    let provenance = Provenance {
        device: "NVIDIA H200".to_string(),
        driver: "570.172.08".to_string(),
        cuda: "12.2".to_string(),
        cudf: "25.02.02".to_string(),
        host: "gpu-host".to_string(),
    };
    let run = RunMeta {
        dataset: "tpch",
        sf: "40",
        query: "q6",
        mode: "tp1-single",
        allocator: "rmm-pool initial=1.0GiB max=1.0GiB of 2.0GiB free on a discrete device",
        capture: Capture::None,
        provenance: &provenance,
    };
    let row = vec!["0"; COLUMNS.len()].join("\t");
    let restore = std::env::var_os(RECORD_PATH_ENV);
    // SAFETY: `append_records` is the variable's only reader, and this is the only case in
    // this binary that calls it.
    unsafe { std::env::set_var(RECORD_PATH_ENV, &path) };
    let wrote = std::panic::catch_unwind(|| append_records(std::slice::from_ref(&row), &run));
    match restore {
        Some(value) => unsafe { std::env::set_var(RECORD_PATH_ENV, value) },
        None => unsafe { std::env::remove_var(RECORD_PATH_ENV) },
    }
    if let Err(panic) = wrote {
        std::panic::resume_unwind(panic);
    }
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{}: {e} — the writer wrote nothing", path.display()));
    let _ = std::fs::remove_dir_all(&dir);

    let mut found: Vec<&str> = text.lines().take_while(|l| l.starts_with('#')).collect();
    let columns = text
        .lines()
        .find(|l| !l.starts_with('#'))
        .unwrap_or_else(|| panic!("{}: no column line", path.display()));
    found.push(columns);

    let allocator = found
        .iter()
        .find_map(|l| l.strip_prefix("# run: allocator="))
        .unwrap_or_else(|| panic!("{}: no `# run: allocator=` line", path.display()));
    assert_eq!(allocator, run.allocator, "the allocator line is the run's");
    let meta = RunMeta {
        dataset: "",
        sf: "",
        query: "",
        mode: "",
        allocator,
        capture: Capture::None,
        provenance: &provenance,
    };
    assert_eq!(found.join("\n"), record_header(&meta), "{}", path.display());
}

/// A row that lost a cell is refused before anything joins on it.
///
/// The one defect this record cannot survive quietly: every column is a number or a name,
/// so a row missing one still parses — each later cell moves into the column to its left,
/// and `device_us` then reads whatever `host_us` measured. Checked here rather than in the
/// benchmark binary because it needs no device.
#[test]
fn a_row_that_lost_a_cell_is_refused() {
    let at = |column: &str| COLUMNS.iter().position(|name| *name == column).unwrap();
    let mut fields = vec!["0".to_string(); COLUMNS.len()];
    fields[at("node_seq")] = "0".to_string();
    fields[at("recipe_seq")] = "0".to_string();
    let declared = BTreeMap::from([(0, BTreeSet::from([0]))]);

    let whole = [fields.join("\t")];
    assert_eq!(rows_match_the_recipes(&whole, &declared), Ok(()));

    fields.pop();
    let short = [fields.join("\t")];
    let refused = rows_match_the_recipes(&short, &declared)
        .expect_err("a row one cell short is not a row of this record");
    assert!(
        refused.contains(&format!("{} columns", COLUMNS.len())),
        "the refusal should name the width it wanted: {refused}"
    );
}

/// A bare call's row names the seq it was handed, which is never its own node's.
///
/// `result_from_handle` and `slice_handle` publish no step: the plan's recipes line for a
/// `GpuUnload` or a `GpuLimit` names no `#seq` at all. They are charged to the node that
/// made the call and carry the seq of the node whose output they were handed — below them
/// in the tree, since post-order is children first. So the pair a row of theirs carries is
/// two nodes' and the check has to say so, which is what q6 found at sf40: every case
/// panicked on the export row, one call before it could write anything.
#[test]
fn a_bare_calls_row_names_the_seq_it_was_handed() {
    let at = |column: &str| COLUMNS.iter().position(|name| *name == column).unwrap();
    let row = |node: usize, seq: usize| {
        let mut fields = vec!["0".to_string(); COLUMNS.len()];
        fields[at("node_seq")] = node.to_string();
        fields[at("recipe_seq")] = seq.to_string();
        [fields.join("\t")]
    };
    // q6's shape: a scan, a project, and an unload that publishes nothing above them.
    let declared = BTreeMap::from([
        (0, BTreeSet::from([0])),
        (1, BTreeSet::from([1])),
        (2, BTreeSet::new()),
    ]);

    assert_eq!(rows_match_the_recipes(&row(2, 1), &declared), Ok(()));

    let refused = rows_match_the_recipes(&row(2, 7), &declared)
        .expect_err("no node publishes step #7, so no handle can have come from one");
    assert!(
        refused.contains("#7"),
        "the refusal should name the step: {refused}"
    );

    let above = rows_match_the_recipes(&row(1, 0), &declared)
        .expect_err("node 1 publishes step #1, and a row of its own may not name another");
    assert!(
        above.contains("#0"),
        "the refusal should name the step: {above}"
    );
}
