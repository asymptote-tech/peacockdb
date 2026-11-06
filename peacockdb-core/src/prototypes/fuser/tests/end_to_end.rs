//! Real plans fused at sf1 and run on the CPU backend: each root answers what its query
//! answers alone, whichever way the held readers were resolved, and a batch of the benchmark's
//! own list answers the corpus goldens, under a cap of one root at a time as well.

use std::num::NonZeroUsize;

use datafusion::arrow::array::RecordBatch;
use datafusion::execution::context::SessionContext;

use crate::executor::{CpuBackend, CpuBatch, run, run_roots};
use crate::plan::{Batching, GpuNode, validate_dag};
use crate::prototypes::fuser::tests::{buffer_every_hold_lowest, re_read_every_hold};
use crate::prototypes::fuser::{Fused, HeldReader, HoldDecision, Query, fuse};
use crate::prototypes::fuser::{batches, ordered_queries, resolve_cap_holds};
use crate::test_support::{MODES, SKIPPED, batches_to_sorted_str, load_csv, queries_dir_for};
use crate::test_support::{line_row_order, result_golden, result_matches, section_of, stem};
use crate::tests::share::planned;

fn record_batches(batches: &[CpuBatch]) -> Vec<RecordBatch> {
    batches
        .iter()
        .map(|batch| batch.record_batch().clone())
        .collect()
}

fn sorted(batches: &[CpuBatch]) -> String {
    batches_to_sorted_str(&record_batches(batches))
}

/// `names` of `dataset` planned at sf1 tp1_single in one session.
async fn planned_at_sf1(
    dataset: &str,
    names: &[String],
) -> (SessionContext, Vec<Box<dyn GpuNode>>) {
    let texts: Vec<String> = names
        .iter()
        .map(|name| {
            let path = queries_dir_for(dataset).join(format!("{name}.sql"));
            std::fs::read_to_string(&path).unwrap_or_else(|why| panic!("{}: {why}", path.display()))
        })
        .collect();
    let sql: Vec<&str> = texts.iter().map(String::as_str).collect();
    planned(dataset, &sql, &MODES[0]).await
}

fn queries_of<'t>(trees: &'t [Box<dyn GpuNode>], names: &[String]) -> Vec<Query<'t>> {
    trees
        .iter()
        .zip(names)
        .map(|(tree, name)| Query {
            name: name.clone(),
            tree: tree.as_ref(),
        })
        .collect()
}

fn refs(roots: &[Box<dyn GpuNode>]) -> Vec<&dyn GpuNode> {
    roots.iter().map(|root| root.as_ref()).collect()
}

fn alone(tree: &dyn GpuNode, ctx: &SessionContext, what: &str) -> String {
    let report = run::<CpuBackend>(tree, &ctx.task_ctx(), None)
        .unwrap_or_else(|why| panic!("{what} alone: {why}"));
    sorted(&report.batches)
}

#[tokio::test]
async fn tpch_q3_q5_q10_fused_answer_as_alone() {
    let names: Vec<String> = ["q3", "q5", "q10"].map(String::from).to_vec();
    let (ctx, trees) = planned_at_sf1("tpch", &names).await;
    let task = ctx.task_ctx();
    let alone: Vec<String> = trees
        .iter()
        .zip(&names)
        .map(|(tree, name)| alone(tree.as_ref(), &ctx, name))
        .collect();
    let queries = queries_of(&trees, &names);
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
        let run = run_roots::<CpuBackend>(&refs(&fused.roots), &task, None, None)
            .unwrap_or_else(|why| panic!("{what}: the fused plan does not run: {why}"));
        for (root, query) in queries.iter().enumerate() {
            let name = &query.name;
            assert_eq!(sorted(&run.answers[root]), alone[root], "{what}: {name}");
        }
    }
}

/// The `first10` batch of `dataset`'s benchmark list, planned at sf1 tp1_single and fused with
/// every held reader buffered on its lowest edge.
async fn first10(dataset: &str) -> (Vec<String>, SessionContext, Vec<Box<dyn GpuNode>>, Fused) {
    let list = ordered_queries(dataset);
    let (_, batch) = batches(&list)
        .into_iter()
        .find(|(label, _)| *label == "first10")
        .expect("a first10 batch");
    let names = batch.unwrap_or_else(|why| panic!("{dataset} first10: {why}"));
    let (ctx, trees) = planned_at_sf1(dataset, &names).await;
    let fused = fuse(
        &queries_of(&trees, &names),
        Batching::Off,
        &mut buffer_every_hold_lowest,
    )
    .unwrap_or_else(|why| panic!("{dataset} first10 does not fuse: {why}"));
    (names, ctx, trees, fused)
}

/// Each answer against its query's `.result.txt` section, in the row order the CPU corpus
/// tier writes it in and under the mode that wrote it. A section that is a `skipped:` marker
/// holds no answer, so that query is held to its own unfused run; those queries come back.
fn meet_the_goldens(
    dataset: &str,
    names: &[String],
    answers: &[Vec<CpuBatch>],
    trees: &[Box<dyn GpuNode>],
    ctx: &SessionContext,
) -> Vec<String> {
    let golden = result_golden(dataset, "1");
    let registry = load_csv();
    let mut against_alone = Vec::new();
    for ((name, answer), tree) in names.iter().zip(answers).zip(trees) {
        let what = format!("{dataset}/{name} fused");
        let section = section_of(&golden, name);
        if section.starts_with(SKIPPED) {
            assert_eq!(sorted(answer), alone(tree.as_ref(), ctx, &what), "{what}");
            against_alone.push(name.clone());
            continue;
        }
        let author = section
            .lines()
            .find_map(|line| line.strip_prefix("mode="))
            .and_then(|spelled| MODES.iter().find(|mode| mode.name == spelled))
            .unwrap_or_else(|| panic!("{what}: the section names no mode"));
        let row = registry
            .iter()
            .find(|row| row.dataset == dataset && row.sf == "1" && stem(&row.query) == *name)
            .unwrap_or_else(|| panic!("{what}: no registry row"));
        let order = line_row_order(&row.duckdb_oracle, &row.gpu_oracle);
        result_matches(&section, author, order, &record_batches(answer))
            .unwrap_or_else(|moved| panic!("{what}: the answer is not the golden's: {moved}"));
    }
    against_alone
}

#[tokio::test]
async fn fused_batches_answer_the_corpus_goldens_on_the_cpu() {
    for dataset in ["tpch", "tpcds"] {
        let (names, ctx, trees, fused) = first10(dataset).await;
        let run = run_roots::<CpuBackend>(&refs(&fused.roots), &ctx.task_ctx(), None, None)
            .unwrap_or_else(|why| panic!("{dataset} first10 does not run: {why}"));
        let against_alone = meet_the_goldens(dataset, &names, &run.answers, &trees, &ctx);
        // No first10 section at sf1 is a marker today; one that becomes one is named here.
        assert!(
            against_alone.is_empty(),
            "{dataset}: held to their own runs, their sections being markers: {against_alone:?}"
        );
    }
}

#[tokio::test]
async fn a_capped_first10_answers_the_corpus_goldens_on_the_cpu() {
    let one = NonZeroUsize::new(1).expect("one");
    for dataset in ["tpch", "tpcds"] {
        let (names, ctx, trees, fused) = first10(dataset).await;
        let task = ctx.task_ctx();
        let uncapped = run_roots::<CpuBackend>(&refs(&fused.roots), &task, None, None)
            .unwrap_or_else(|why| panic!("{dataset} first10 does not run: {why}"));
        let capped = resolve_cap_holds(fused, one, &mut buffer_every_hold_lowest)
            .unwrap_or_else(|why| panic!("{dataset} first10 under a cap of one: {why}"));
        validate_dag(&refs(&capped.roots), Some(one))
            .unwrap_or_else(|why| panic!("{dataset} first10 under a cap of one: {why}"));
        let run = run_roots::<CpuBackend>(&refs(&capped.roots), &task, None, Some(one))
            .unwrap_or_else(|why| panic!("{dataset} capped first10 does not run: {why}"));
        for (root, name) in names.iter().enumerate() {
            let (got, want) = (&run.answers[root], &uncapped.answers[root]);
            assert_eq!(sorted(got), sorted(want), "{dataset}/{name} capped");
        }
        meet_the_goldens(dataset, &names, &run.answers, &trees, &ctx);
    }
}
