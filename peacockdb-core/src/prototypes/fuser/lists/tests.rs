//! The batch list: which rows of a device case list are batched, in what order, and the
//! prefixes each batch size takes.

use super::ordered_queries_in;
use crate::prototypes::fuser::{BATCH_SIZES, batch_set, batches, ordered_queries};
use crate::test_support::{CaseStatus, DeviceCase};

fn case(dataset: &str, query: &str, modes: &[&str], status: CaseStatus, note: &str) -> DeviceCase {
    DeviceCase {
        dataset: dataset.to_string(),
        sf: "40".to_string(),
        query: query.to_string(),
        modes: modes.iter().map(|mode| mode.to_string()).collect(),
        status,
        note: note.to_string(),
    }
}

fn names(n: usize) -> Vec<String> {
    (1..=n).map(|at| format!("q{at}")).collect()
}

#[test]
fn batch_set_takes_tp1_single_rows_in_file_order() {
    let cases = [
        case("tpch", "q2", &["tp1_single"], CaseStatus::Ok, ""),
        case(
            "tpch",
            "aggregate_groupby",
            &["tp1_single"],
            CaseStatus::Ok,
            "",
        ),
        case(
            "tpch",
            "q1",
            &["tp1_single", "tp4_sized"],
            CaseStatus::Ok,
            "",
        ),
        case("tpch", "q5", &["tp4_sized"], CaseStatus::Ok, ""),
        case("tpch", "q9", &[], CaseStatus::Skipped, "a reason"),
        case("tpch", "q13", &["tp1_single"], CaseStatus::Wrong, "#7"),
        case("tpcds", "q3", &["tp1_single"], CaseStatus::Ok, ""),
    ];
    assert_eq!(batch_set("tpch", &cases), ["q2", "q1", "q13"]);
    assert_eq!(batch_set("tpcds", &cases), ["q3"]);
}

#[test]
fn tpch_batches_only_its_twenty_two_queries() {
    let cases: Vec<DeviceCase> = ["q0", "q1", "q01", "q22", "q23", "q10x"]
        .iter()
        .map(|query| case("tpch", query, &["tp1_single"], CaseStatus::Ok, ""))
        .collect();
    assert_eq!(batch_set("tpch", &cases), ["q1", "q22"]);
}

#[test]
#[should_panic(expected = "pbench")]
fn an_empty_batch_set_panics_naming_the_dataset() {
    batch_set(
        "pbench",
        &[case("tpch", "q1", &["tp1_single"], CaseStatus::Ok, "")],
    );
}

#[test]
#[should_panic(expected = "h100-80gb-hbm3/cases.csv")]
fn ordered_queries_without_the_list_panic_naming_it() {
    let empty = std::env::temp_dir().join(format!("fuser_no_list_{}", std::process::id()));
    ordered_queries_in(
        &empty.join("benchmark-results/h100-80gb-hbm3/cases.csv"),
        "tpch",
    );
}

#[test]
fn ordered_queries_read_the_h100_list() {
    assert_eq!(ordered_queries("tpch")[..3], ["q1", "q2", "q3"]);
    assert!(ordered_queries("tpcds").len() > 20);
}

#[test]
fn batches_take_prefixes_of_the_list() {
    let list = names(25);
    let made = batches(&list);
    let labels: Vec<&str> = made.iter().map(|(label, _)| *label).collect();
    let sizes: Vec<&str> = BATCH_SIZES.iter().map(|(label, _)| *label).collect();
    assert_eq!(labels, sizes);
    let lengths: Vec<usize> = made
        .iter()
        .map(|(label, batch)| {
            batch
                .as_ref()
                .unwrap_or_else(|why| panic!("{label}: {why}"))
                .len()
        })
        .collect();
    assert_eq!(lengths, [3, 10, 20, 25]);
    assert_eq!(made[1].1.as_ref().unwrap()[..], list[..10]);
}

#[test]
fn a_short_list_marks_first20_as_all() {
    let made = batches(&names(18));
    let first20 = &made[2];
    assert_eq!(first20.0, "first20");
    let marker = first20.1.as_ref().expect_err("first20 is the whole list");
    assert!(marker.contains("18"), "{marker}");
    assert_eq!(made[1].1.as_ref().map(Vec::len), Ok(10));
    assert_eq!(made[3].1.as_ref().map(Vec::len), Ok(18));
}

#[test]
fn first3_runs_whatever_the_list() {
    let made = batches(&names(3));
    assert_eq!(made[0].1.as_ref().map(Vec::len), Ok(3));
    assert!(made[1].1.is_err());
}
