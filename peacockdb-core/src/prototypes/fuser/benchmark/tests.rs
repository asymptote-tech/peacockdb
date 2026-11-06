//! The fused benchmark's host-side rules: which errors are out of memory, where its files go,
//! and that a rerun replaces its own record rows.

use std::collections::BTreeSet;
use std::path::PathBuf;

use crate::executor::{RunError, plan_files};
use crate::plan::{Batching, GpuNode};
use crate::prototypes::fused_results_file;
use crate::prototypes::fuser::replace_case_rows;
use crate::prototypes::fuser::tests::re_read_every_hold;
use crate::prototypes::fuser::{Query, case_name, fuse, fused_record_path, is_out_of_memory};
use crate::test_support::{COLUMNS, Capture, MODES, Provenance, RunMeta, queries_dir_for};
use crate::tests::share::planned;

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
