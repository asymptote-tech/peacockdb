//! The page's fused-batches tables over temp trees: a measured batch, one out of memory, a
//! marker, a failed batch, and no fused file at all. Each tree's baseline holds q1..q12.

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

/// A baseline section: one query run alone, its loads one per table named.
fn alone(load_us: u64, execute_us: u64, tables: &[&str]) -> String {
    let mut text = String::from("GpuUnload\n  time_us=[[1]] total_us=1\n");
    for table in tables {
        text.push_str(&format!(
            "  GpuLoadParquet: table={table}, lanes=1, batches=multiple\n    time_us=[[1]] \
             total_us=1\n"
        ));
    }
    text.push_str(&format!(
        "--- run ---\nrun_us=1\ndevice_us=1\nload_us={load_us}\nexecute_us={execute_us}\n\
         peak_bytes=1\n"
    ));
    text
}

/// The baseline every tree holds: q1 reads lineitem and orders, q2 lineitem twice (a
/// self-join), q3..q12 lineitem.
fn baseline() -> String {
    let mut text = format!("== q1\n{}", alone(2000, 1000, &["lineitem", "orders"]));
    text.push_str(&format!(
        "== q2\n{}",
        alone(1500, 500, &["lineitem", "lineitem"])
    ));
    for n in 3..=12 {
        text.push_str(&format!("== q{n}\n{}", alone(500, 500, &["lineitem"])));
    }
    text
}

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
        let base = self.0.join(device).join(set);
        std::fs::create_dir_all(&base).expect("the baseline dir");
        std::fs::write(base.join("tp1_single.benchmark.txt"), baseline()).expect("the baseline");
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
            "nonfused total load (ms)",
            "fused load (ms)",
            "nonfused total execute (ms)",
            "fused execute (ms)",
            "accountant's peak ≥ (GiB)",
            "cap",
            "reads (fused/nonfused)"
        ]
    );
    let row = &table.rows[0];
    assert_eq!(
        texts(row),
        [
            "first3",
            "q1, q2",
            "measured",
            "3.5",
            "1.0",
            "1.5",
            "0.5",
            "1.50",
            "2",
            "lineitem:1/3\norders:1/1"
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
            "q1, q2, q3",
            "out of memory (cap 1)",
            "4.0",
            "—",
            "2.0",
            "—",
            "—",
            "1",
            "lineitem:1/4"
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
fn queries_go_ten_to_a_line() {
    let root = Root::new("wrap");
    let queries: Vec<String> = (1..=12).map(|n| format!("q{n}")).collect();
    let batch = MEASURED.replace(
        "queries=[q1,q2]",
        &format!("queries=[{}]", queries.join(",")),
    );
    root.write("h100-80gb-hbm3", "tpch.sf40", &[("all", &batch)]);
    let tables = fused_report_tables(root.path());
    assert_eq!(
        only(&tables).rows[0][1].text,
        "q1, q2, q3, q4, q5, q6, q7, q8, q9, q10\nq11, q12"
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
