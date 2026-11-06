//! `loads.tsv` read back as the hold rule needs it: one load time per (query, recipe seq) of
//! one case.

use crate::prototypes::fuser::read_load_times;
use crate::prototypes::fuser::tests::{load_times, loads_tsv};

#[test]
fn a_loads_tsv_reads_back() {
    let times = load_times(&[("q1", 0, 70), ("q3", 2, 90)]);
    assert_eq!(times.load_us("q1", 0), 70);
    assert_eq!(times.load_us("q3", 2), 90);
}

#[test]
fn only_the_named_case_is_kept() {
    let mut text = loads_tsv(&[("q1", 0, 70)]);
    text.push_str("tpch\t40\tq1\ttp4-sized\t0\t10\t1\t5\t0\n");
    text.push_str("tpcds\t40\tq1\ttp1-single\t0\t10\t1\t6\t0\n");
    let path = std::env::temp_dir().join(format!("fuser_loads_case_{}.tsv", std::process::id()));
    std::fs::write(&path, text).expect("the file writes");
    let times = read_load_times(&path, "tpch", "40", "tp1-single");
    std::fs::remove_file(&path).expect("the file is removed");
    assert_eq!(times.load_us("q1", 0), 70);
}

#[test]
#[should_panic(expected = "q9")]
fn a_missing_load_panics_naming_it() {
    load_times(&[("q1", 0, 70)]).load_us("q9", 4);
}
