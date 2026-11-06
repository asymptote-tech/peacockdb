//! The page's fused-batches tables over temp trees: a measured batch, one out of memory, a
//! marker, a failed batch, and no fused file at all.

use std::path::{Path, PathBuf};

use crate::prototypes::fused_report_tables;
use crate::test_support::{FAILED, ReportCell, ReportTable, SKIPPED, section_href};

const MEASURED: &str = "\
--- batch ---
queries=[q1,q2]
outcome=measured
shared=[lineitem:q1+q2]
unshared=[]
reads=[lineitem:1,orders:1]
buffers=0
buffer_bytes=0
rereads=0
cap=2
pool_bytes=1000
budget_bytes=900
estimate_bytes=500
over_budget=false
answers=match
--- run ---
run_us=1600
device_us=1500
load_us=1000
execute_us=500
peak_bytes=1610612736
";

const OUT_OF_MEMORY: &str = "\
--- batch ---
queries=[q1,q2,q3]
outcome=out-of-memory
reason=std::bad_alloc: out_of_memory
shared=[lineitem:q1+q2+q3]
unshared=[]
reads=[lineitem:1]
buffers=0
buffer_bytes=0
rereads=0
cap=1
pool_bytes=1000
budget_bytes=900
estimate_bytes=950
over_budget=true
--- run ---
device=NVIDIA H100 80GB HBM3
";

/// A temp `benchmark-results` root with `device/fused/<ds>.sf<sf>/tp1_single.fused.txt` holding
/// `sections` as `(batch, body)`.
struct Root(PathBuf);

impl Root {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("fused_report_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("the temp root");
        Root(dir)
    }

    fn write(&self, device: &str, set: &str, sections: &[(&str, &str)]) {
        let dir = self.0.join(device).join("fused").join(set);
        std::fs::create_dir_all(&dir).expect("the fused dir");
        let text: String = sections
            .iter()
            .map(|(batch, body)| format!("== {batch}\n{body}"))
            .collect();
        std::fs::write(dir.join("tp1_single.fused.txt"), text).expect("the fused file");
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Root {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn texts(row: &[ReportCell]) -> Vec<&str> {
    row.iter().map(|cell| cell.text.as_str()).collect()
}

fn only(tables: &[ReportTable]) -> &ReportTable {
    assert_eq!(tables.len(), 1, "one fused file, one table");
    &tables[0]
}

#[test]
fn a_measured_batch_is_a_row() {
    let root = Root::new("measured");
    root.write("h100-80gb-hbm3", "tpch.sf40", &[("first3", MEASURED)]);
    let tables = fused_report_tables(root.path());
    let table = only(&tables);
    assert_eq!(table.id, "fused-h100-80gb-hbm3-tpch-sf40");
    assert_eq!(table.heading, "Fused batches: h100-80gb-hbm3, tpch sf40");
    assert_eq!(
        table.columns,
        [
            "batch",
            "queries",
            "outcome",
            "fused load (ms)",
            "fused execute (ms)",
            "accountant's peak ≥ (GiB)",
            "cap",
            "reads"
        ]
    );
    let row = &table.rows[0];
    assert_eq!(
        texts(row),
        [
            "first3",
            "[q1,q2]",
            "measured",
            "1.0",
            "0.5",
            "1.50",
            "2",
            "[lineitem:1,orders:1]"
        ]
    );
    let rel = "benchmark-results/h100-80gb-hbm3/fused/tpch.sf40/tp1_single.fused.txt";
    assert_eq!(row[0].href.as_deref(), Some(section_href(rel, 1).as_str()));
}

#[test]
fn an_out_of_memory_batch_shows_its_cap() {
    let root = Root::new("oom");
    root.write(
        "h100-80gb-hbm3",
        "tpcds.sf40",
        &[("first3", MEASURED), ("all", OUT_OF_MEMORY)],
    );
    let tables = fused_report_tables(root.path());
    let row = &only(&tables).rows[1];
    assert_eq!(
        texts(row),
        [
            "all",
            "[q1,q2,q3]",
            "out of memory (cap 1)",
            "—",
            "—",
            "—",
            "1",
            "[lineitem:1]"
        ]
    );
    assert_eq!(row[2].class, Some("off"));
    let line = MEASURED.lines().count() + 2;
    let rel = "benchmark-results/h100-80gb-hbm3/fused/tpcds.sf40/tp1_single.fused.txt";
    assert_eq!(
        row[0].href.as_deref(),
        Some(section_href(rel, line).as_str())
    );
}

#[test]
fn a_marker_reads_same_set_as_all() {
    let root = Root::new("marker");
    let marker = format!("{SKIPPED}same set as all: the list holds 18 queries\n");
    root.write("h100-80gb-hbm3", "tpch.sf40", &[("first20", &marker)]);
    let tables = fused_report_tables(root.path());
    let row = &only(&tables).rows[0];
    assert_eq!(row[0].text, "first20");
    assert_eq!(row[2].text, "same set as all");
    assert_eq!(row[2].class, Some("skip"));
    assert!(
        texts(row)[3..].iter().all(|cell| *cell == "—"),
        "{:?}",
        texts(row)
    );
}

#[test]
fn a_failed_batch_says_why() {
    let root = Root::new("failed");
    let failed = format!("{FAILED}prefill: needs 9 > limit 8\ndevice=x\n");
    root.write("h100-80gb-hbm3", "tpch.sf40", &[("first10", &failed)]);
    let tables = fused_report_tables(root.path());
    let row = &only(&tables).rows[0];
    assert_eq!(row[2].text, "failed: prefill: needs 9 > limit 8");
    assert_eq!(row[2].class, Some("off"));
}

#[test]
fn tables_go_by_device_then_dataset_then_scale() {
    let root = Root::new("order");
    for (device, set) in [
        ("l40s", "tpch.sf40"),
        ("h100-80gb-hbm3", "tpch.sf40"),
        ("h100-80gb-hbm3", "tpch.sf100"),
        ("h100-80gb-hbm3", "tpcds.sf40"),
    ] {
        root.write(device, set, &[("first3", MEASURED)]);
    }
    let ids: Vec<String> = fused_report_tables(root.path())
        .into_iter()
        .map(|table| table.id)
        .collect();
    assert_eq!(
        ids,
        [
            "fused-h100-80gb-hbm3-tpcds-sf40",
            "fused-h100-80gb-hbm3-tpch-sf40",
            "fused-h100-80gb-hbm3-tpch-sf100",
            "fused-l40s-tpch-sf40"
        ]
    );
}

#[test]
fn no_fused_file_means_no_table() {
    let root = Root::new("none");
    std::fs::create_dir_all(root.path().join("h100-80gb-hbm3/tpch.sf40")).expect("a tree dir");
    assert!(fused_report_tables(root.path()).is_empty());
}
