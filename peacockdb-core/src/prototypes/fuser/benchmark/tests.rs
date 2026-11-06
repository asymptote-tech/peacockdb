//! The fused benchmark's host-side rules: which errors are out of memory, where its files go,
//! and that a rerun replaces its own record rows.

use std::collections::BTreeSet;
use std::path::PathBuf;

use crate::executor::{RunError, nodes_as_recorded, plan_files};
use crate::plan::{Batching, GpuNode};
use crate::prototypes::fuser::tests::re_read_every_hold;
use crate::prototypes::fuser::{Baseline, ordered_queries, read_load_times, replace_case_rows};
use crate::prototypes::fuser::{Query, case_name, fuse, fused_record_path, is_out_of_memory};
use crate::prototypes::{fused_results_file, parse_fused_section};
use crate::test_support::{COLUMNS, Capture, FAILED, MODES, Provenance, RunMeta, SKIPPED};
use crate::test_support::{ordered_sections, parse_benchmark_section, provenance_fields};
use crate::test_support::{queries_dir_for, testdata_root};
use crate::tests::share::planned;
use crate::wire::join_scratch_bytes;

use super::{baseline_for, declared_batches, holds_this_run, load_times_path};
use super::{out_of_memory_run, scan_seqs, second_smallest_at};

fn failed(text: &str) -> RunError {
    RunError::CallFailed(text.to_string())
}

#[test]
fn an_out_of_memory_error_is_recognised() {
    assert!(is_out_of_memory(&failed(
        "GpuHashJoin lane 0: std::bad_alloc: out_of_memory: RMM failure at arena.hpp:123"
    )));
    assert!(is_out_of_memory(&failed(
        "GpuAggregate lane 0: CUDA error at cudf/utilities: cudaErrorMemoryAllocation out of memory"
    )));
    assert!(!is_out_of_memory(&failed("no plan loaded")));
}

#[test]
fn the_fused_file_lies_beside_the_device_tree() {
    let path = fused_results_file("h100-80gb-hbm3", "tpch", "40");
    assert!(
        path.ends_with("benchmark-results/h100-80gb-hbm3/fused/tpch.sf40/tp1_single.fused.txt"),
        "{}",
        path.display()
    );
    let record = fused_record_path("tpch", "40");
    assert!(
        record.ends_with("calibration/fused/tpch.sf40/records.tsv"),
        "{}",
        record.display()
    );
    let loads = load_times_path("tpch", "40");
    assert!(
        loads.ends_with("calibration/h100-80gb-hbm3/tpch.sf40/loads.tsv"),
        "{}",
        loads.display()
    );
}

fn provenance() -> Provenance {
    Provenance {
        device: "NVIDIA H100 80GB HBM3".to_string(),
        driver: "580".to_string(),
        cuda: "12.2".to_string(),
        cudf: "25.02.02".to_string(),
        host: "h".to_string(),
    }
}

fn meta<'a>(case: &'a str, provenance: &'a Provenance) -> RunMeta<'a> {
    RunMeta {
        dataset: "tpch",
        sf: "40",
        query: case,
        mode: "tp1-single",
        allocator: "rmm-pool",
        capture: Capture::None,
        provenance,
    }
}

/// A record row of `case`, its `node_seq` `n`, every other column 0 but the four in front.
fn row(case: &str, n: usize) -> String {
    let mut fields = vec!["tpch", "40", case, "tp1-single"]
        .into_iter()
        .map(String::from)
        .collect::<Vec<_>>();
    fields.extend((4..COLUMNS.len()).map(|at| if at == 4 { n.to_string() } else { "0".into() }));
    fields.join("\t")
}

#[test]
fn a_rerun_batch_replaces_its_record_rows() {
    let path = std::env::temp_dir().join(format!("fuser_record_{}.tsv", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let provenance = provenance();
    let (first3, first10) = (case_name("tpch", "first3"), case_name("tpch", "first10"));
    assert_eq!(first3, "bench_fused_tpch_sf40_first3_tp1_single");
    let (first3, first10) = (first3.as_str(), first10.as_str());
    replace_case_rows(
        &path,
        &[row(first3, 1), row(first3, 2)],
        &meta(first3, &provenance),
    );
    replace_case_rows(&path, &[row(first10, 7)], &meta(first10, &provenance));
    replace_case_rows(&path, &[row(first3, 3)], &meta(first3, &provenance));
    let text = std::fs::read_to_string(&path).expect("the record");
    std::fs::remove_file(&path).expect("the record is removed");
    let rows: Vec<&str> = text
        .lines()
        .filter(|line| line.starts_with("tpch\t"))
        .collect();
    assert_eq!(rows, [row(first10, 7), row(first3, 3)]);
    assert_eq!(
        text.lines().filter(|l| l.starts_with("dataset\t")).count(),
        1
    );
}

#[tokio::test]
async fn plan_files_of_a_fused_batch_is_its_queries_union() {
    let names = ["q3", "q6"];
    let texts: Vec<String> = names
        .iter()
        .map(|name| std::fs::read_to_string(queries_dir_for("tpch").join(format!("{name}.sql"))))
        .collect::<Result<_, _>>()
        .expect("the query texts");
    let sql: Vec<&str> = texts.iter().map(String::as_str).collect();
    let (_ctx, trees) = planned("tpch", &sql, &MODES[0]).await;
    let queries: Vec<Query<'_>> = trees
        .iter()
        .zip(names)
        .map(|(tree, name)| Query {
            name: name.to_string(),
            tree: tree.as_ref(),
        })
        .collect();
    let fused = fuse(&queries, Batching::Off, &mut re_read_every_hold).expect("they fuse");
    assert!(fused.shared.iter().any(|group| group.table == "lineitem"));
    let roots: Vec<&dyn GpuNode> = fused.roots.iter().map(|root| root.as_ref()).collect();
    let each: BTreeSet<PathBuf> = trees
        .iter()
        .flat_map(|tree| plan_files(&[tree.as_ref()]))
        .collect();
    assert_eq!(plan_files(&roots), each);
    assert!(each.iter().any(|file| file.ends_with("lineitem.parquet")));
}

#[tokio::test]
async fn each_load_carries_its_scan_seq() {
    let text = std::fs::read_to_string(queries_dir_for("tpch").join("q3.sql")).expect("q3");
    let (_ctx, trees) = planned("tpch", &[text.as_str()], &MODES[0]).await;
    let tree = trees[0].as_ref();
    let join_scratch = join_scratch_bytes(&MODES[0].knobs_for("tpch"));
    let seqs = scan_seqs(tree, join_scratch);
    let names: Vec<&str> = nodes_as_recorded(tree)
        .expect("q3 indexes")
        .into_iter()
        .map(|(name, _)| name)
        .collect();
    assert_eq!(seqs.len(), names.len());
    let loads: Vec<usize> = (0..names.len())
        .filter(|at| names[*at] == "GpuLoadParquet")
        .collect();
    let seqd: Vec<usize> = (0..seqs.len()).filter(|at| seqs[*at].is_some()).collect();
    assert_eq!(seqd, loads, "a seq at each load and nowhere else");
    let distinct: BTreeSet<u64> = seqs.iter().flatten().copied().collect();
    assert_eq!(distinct.len(), 3, "q3's three loads: {seqs:?}");
}

#[test]
fn a_batch_the_list_does_not_exceed_is_declared_a_marker() {
    let list: Vec<String> = (1..=15).map(|n| format!("q{n}")).collect();
    let declared = declared_batches(&list);
    let names: Vec<&str> = declared.iter().map(|(name, _)| name.as_str()).collect();
    assert_eq!(names, ["first3", "first10", "first20", "all"]);
    assert_eq!(declared[0].1, None);
    assert_eq!(declared[1].1, None);
    let marker = declared[2]
        .1
        .as_deref()
        .expect("first20 holds the whole list");
    assert!(
        marker.starts_with(SKIPPED) && marker.contains("same set as all"),
        "{marker}"
    );
    assert_eq!(declared[3].1, None);
}

/// A file holding `first3` as an out-of-memory section under `provenance`, and `first10` failed.
fn fused_file(provenance: &Provenance) -> String {
    format!(
        "== first3\n--- batch ---\nqueries=[q1,q2,q3]\noutcome=out-of-memory\nreason=bad_alloc\n\
         {}\n== first10\n{FAILED}prefill: no room\n",
        out_of_memory_run(provenance)
    )
}

#[test]
fn an_out_of_memory_run_block_is_the_five() {
    let ours = provenance();
    let section = format!(
        "--- batch ---\nqueries=[q1]\noutcome=out-of-memory\nreason=x\n{}",
        out_of_memory_run(&ours)
    );
    let parsed = parse_fused_section(&section).expect("a recorded outcome");
    let five: Vec<(&str, &str)> = provenance_fields(&ours).to_vec();
    assert_eq!(parsed.trailer, five);
}

#[test]
fn a_section_this_run_wrote_is_held() {
    let ours = provenance();
    let text = fused_file(&ours);
    assert!(holds_this_run(&text, "first3", &ours));
    assert!(
        !holds_this_run(&text, "first10", &ours),
        "a failed batch runs again"
    );
    assert!(
        !holds_this_run(&text, "first20", &ours),
        "an absent batch runs"
    );
    let elsewhere = Provenance {
        host: "another".to_string(),
        ..provenance()
    };
    assert!(!holds_this_run(&text, "first3", &elsewhere));
}

#[test]
fn the_reported_run_is_the_second_smallest() {
    assert_eq!(second_smallest_at(&[30, 10, 20]), 2);
    assert_eq!(second_smallest_at(&[10, 20]), 1);
    let tie = second_smallest_at(&[5, 9, 5]);
    assert!(tie == 0 || tie == 2, "{tie}");
}

fn h100_tree(dataset: &str) -> String {
    let path = testdata_root()
        .join("benchmark-results/h100-80gb-hbm3")
        .join(format!("{dataset}.sf40/tp1_single.benchmark.txt"));
    std::fs::read_to_string(&path).unwrap_or_else(|why| panic!("{}: {why}", path.display()))
}

#[test]
fn a_batch_baseline_takes_its_queries_sections_in_batch_order() {
    let text = h100_tree("tpch");
    let names = ["q3".to_string(), "q1".to_string()];
    let batch = baseline_for(&text, &names);
    let sections = ordered_sections(&text);
    let alone = |query: &str| {
        let (_, body) = sections
            .iter()
            .find(|(name, _)| name == query)
            .expect("the section");
        Baseline::of(&[parse_benchmark_section(body).expect("measured")])
            .per_query
            .remove(0)
    };
    assert_eq!(batch.per_query.len(), 2);
    for (at, query) in names.iter().enumerate() {
        let want = alone(query);
        assert_eq!(batch.per_query[at].names, want.names, "{query}");
        assert_eq!(batch.per_query[at].peak_bytes, want.peak_bytes, "{query}");
    }
}

#[test]
#[should_panic(expected = "q99")]
fn a_query_the_baseline_lacks_panics_naming_it() {
    baseline_for(&h100_tree("tpch"), &["q1".to_string(), "q99".to_string()]);
}

#[test]
fn every_batched_query_has_its_baseline_and_its_loads() {
    for dataset in ["tpch", "tpcds"] {
        let list = ordered_queries(dataset);
        let baseline = baseline_for(&h100_tree(dataset), &list);
        assert_eq!(baseline.per_query.len(), list.len(), "{dataset}");
        let loads = read_load_times(
            &load_times_path(dataset, "40"),
            dataset,
            "40",
            MODES[0].name,
        );
        for query in &list {
            assert!(
                loads.by_key.keys().any(|(name, _)| name == query),
                "{dataset}/{query}: no loads.tsv row"
            );
        }
    }
}
