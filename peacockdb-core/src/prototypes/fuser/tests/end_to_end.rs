//! Real plans fused at sf1 and run on the CPU backend: each root answers what its query
//! answers alone, whichever way the held readers were resolved.

use datafusion::arrow::array::RecordBatch;

use crate::executor::{CpuBackend, CpuBatch, run, run_roots};
use crate::plan::{Batching, GpuNode};
use crate::prototypes::fuser::tests::{buffer_every_hold_lowest, re_read_every_hold};
use crate::prototypes::fuser::{Fused, HeldReader, HoldDecision, Query, fuse};
use crate::test_support::{MODES, batches_to_sorted_str, queries_dir_for};
use crate::tests::share::planned;

fn sorted(batches: &[CpuBatch]) -> String {
    let batches: Vec<RecordBatch> = batches
        .iter()
        .map(|batch| batch.record_batch().clone())
        .collect();
    batches_to_sorted_str(&batches)
}

#[tokio::test]
async fn tpch_q3_q5_q10_fused_answer_as_alone() {
    let names = ["q3", "q5", "q10"];
    let texts: Vec<String> = names
        .iter()
        .map(|name| {
            let path = queries_dir_for("tpch").join(format!("{name}.sql"));
            std::fs::read_to_string(&path).unwrap_or_else(|why| panic!("{}: {why}", path.display()))
        })
        .collect();
    let sql: Vec<&str> = texts.iter().map(String::as_str).collect();
    let (ctx, trees) = planned("tpch", &sql, &MODES[0]).await;
    let task = ctx.task_ctx();
    let alone: Vec<String> = trees
        .iter()
        .zip(names)
        .map(|(tree, name)| {
            let report = run::<CpuBackend>(tree.as_ref(), &task, None)
                .unwrap_or_else(|why| panic!("{name} alone: {why}"));
            sorted(&report.batches)
        })
        .collect();
    let queries: Vec<Query<'_>> = trees
        .iter()
        .zip(names)
        .map(|(tree, name)| Query {
            name: name.to_string(),
            tree: tree.as_ref(),
        })
        .collect();
    let choosers: [(&str, &mut dyn FnMut(&Fused, &HeldReader) -> HoldDecision); 2] = [
        ("re-read", &mut re_read_every_hold),
        ("buffer", &mut buffer_every_hold_lowest),
    ];
    for (what, choose) in choosers {
        let fused = fuse(&queries, Batching::Off, choose)
            .unwrap_or_else(|why| panic!("{what}: the plans do not fuse: {why}"));
        let tables: Vec<&str> = fused.shared.iter().map(|g| g.table.as_str()).collect();
        for table in ["lineitem", "orders", "customer"] {
            assert!(
                tables.contains(&table),
                "{what}: {table} is not shared: {tables:?}"
            );
        }
        assert!(!fused.holds.is_empty(), "{what}: no reader sat in a hold");
        let roots: Vec<&dyn GpuNode> = fused.roots.iter().map(|root| root.as_ref()).collect();
        let run = run_roots::<CpuBackend>(&roots, &task, None, None)
            .unwrap_or_else(|why| panic!("{what}: the fused plan does not run: {why}"));
        for (root, query) in queries.iter().enumerate() {
            let name = &query.name;
            assert_eq!(sorted(&run.answers[root]), alone[root], "{what}: {name}");
        }
    }
}
