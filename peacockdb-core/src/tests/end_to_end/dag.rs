//! Several queries' plans fused into one DAG and run at once on the CPU backend: every load two
//! of them share read once, a hold buffer on every edge that needs one, and each root answering
//! what its query answers alone. TPC-DS q95, which reads `web_sales` on both sides of a join, is
//! the one real plan with a shared source inside a hold.

use datafusion::arrow::array::RecordBatch;
use datafusion::execution::context::SessionContext;

use crate::executor::{CpuBackend, CpuBatch, FusedRun, RunReport, run, run_roots};
use crate::plan::{GpuNode, PlanDag, PlanError, validate_roots};
use crate::test_support::{MODES, Mode, batches_to_sorted_str, queries_dir_for};
use crate::tests::share::{
    LINEITEM_BY_QUANTITY, LINEITEM_TOTALS, ORDERS_BY_PRIORITY, QUANTITY_BY_PRIORITY,
    buffer_unbuffered_holds, planned, share_sources,
};

const TP1_SINGLE: &Mode = &MODES[0];
const TP4_SIZED: &Mode = &MODES[4];

fn refs(roots: &[Box<dyn GpuNode>]) -> Vec<&dyn GpuNode> {
    roots.iter().map(|root| root.as_ref()).collect()
}

fn record_batches(batches: &[CpuBatch]) -> Vec<RecordBatch> {
    batches
        .iter()
        .map(|batch| batch.record_batch().clone())
        .collect()
}

fn alone(tree: &dyn GpuNode, ctx: &SessionContext, what: &str) -> RunReport {
    let report = run::<CpuBackend>(tree, &ctx.task_ctx(), None)
        .unwrap_or_else(|why| panic!("{what} alone: {why}"));
    assert_eq!(report.holds, report.releases, "{what} alone");
    assert_eq!(report.in_flight_bytes, 0, "{what} alone");
    report
}

fn fused(roots: &[Box<dyn GpuNode>], ctx: &SessionContext, what: &str) -> FusedRun {
    let run = run_roots::<CpuBackend>(&refs(roots), &ctx.task_ctx(), None, None)
        .unwrap_or_else(|why| panic!("{what} fused: {why}"));
    assert_eq!(run.report.holds, run.report.releases, "{what} fused");
    assert_eq!(run.report.in_flight_bytes, 0, "{what} fused");
    run
}

/// Each answer of `fused` against what its query answers alone, sorted, byte for byte.
fn assert_answers_as_alone(fused: &FusedRun, alone: &[RunReport], what: &str) {
    assert_eq!(
        fused.answers.len(),
        alone.len(),
        "{what}: one answer per root"
    );
    for (root, (answer, alone)) in fused.answers.iter().zip(alone).enumerate() {
        assert_eq!(
            batches_to_sorted_str(&record_batches(answer)),
            batches_to_sorted_str(&record_batches(&alone.batches)),
            "{what}: root {root}"
        );
    }
}

/// `queries` planned at `mode`, fused, and run, each root's answer checked against its query
/// alone; `shared` is how many sources the fusing must share. What came back: the fused run,
/// each query's run alone, and the buffers the fixture inserted.
async fn fused_as_alone(
    queries: &[&str],
    mode: &Mode,
    shared: usize,
) -> (FusedRun, Vec<RunReport>, usize, Vec<Box<dyn GpuNode>>) {
    let what = format!("{} queries at {}", queries.len(), mode.name);
    let (ctx, trees) = planned("tpch", queries, mode).await;
    let alone: Vec<RunReport> = trees
        .iter()
        .zip(queries)
        .map(|(tree, query)| alone(tree.as_ref(), &ctx, query))
        .collect();
    let (roots, sharing) = share_sources(trees);
    assert_eq!(sharing, shared, "{what}: sources shared");
    let (roots, buffers) = buffer_unbuffered_holds(roots, None);
    let fused = fused(&roots, &ctx, &what);
    assert_answers_as_alone(&fused, &alone, &what);
    (fused, alone, buffers, roots)
}

async fn two_roots_over_one_shared_source(mode: &Mode) {
    let (_, _, buffers, _) =
        fused_as_alone(&[LINEITEM_TOTALS, LINEITEM_BY_QUANTITY], mode, 1).await;
    assert_eq!(buffers, 0, "no reader of lineitem sits in a hold");
}

#[tokio::test]
async fn two_roots_over_one_shared_source_answer_as_alone_at_tp1_single() {
    two_roots_over_one_shared_source(TP1_SINGLE).await;
}

#[tokio::test]
async fn two_roots_over_one_shared_source_answer_as_alone_at_tp4_sized() {
    two_roots_over_one_shared_source(TP4_SIZED).await;
}

const THREE_ROOTS: [&str; 3] = [LINEITEM_TOTALS, QUANTITY_BY_PRIORITY, ORDERS_BY_PRIORITY];

async fn three_roots_over_two_shared_sources(mode: &Mode) {
    let (_, _, buffers, _) = fused_as_alone(&THREE_ROOTS, mode, 2).await;
    assert_eq!(
        buffers, 1,
        "lineitem enters the join's probe hold from the first root"
    );
}

#[tokio::test]
async fn three_roots_over_two_shared_sources_answer_as_alone_at_tp1_single() {
    three_roots_over_two_shared_sources(TP1_SINGLE).await;
}

#[tokio::test]
async fn three_roots_over_two_shared_sources_answer_as_alone_at_tp4_sized() {
    three_roots_over_two_shared_sources(TP4_SIZED).await;
}

/// The roots alone hold every batch of a shared source once per reader; fused, once. A
/// buffer takes over the charge of what it keeps rather than adding one.
async fn a_fused_run_holds_each_shared_batch_once(mode: &Mode) {
    let (fused, alone, _, roots) = fused_as_alone(&THREE_ROOTS, mode, 2).await;
    let dag = PlanDag::of(&refs(&roots));
    let saved: usize = dag
        .nodes
        .iter()
        .enumerate()
        .filter(|(_, node)| node.node.sharing_key().is_some())
        .map(|(at, node)| {
            let batches: usize = fused.report.emitted[at].iter().map(Vec::len).sum();
            (node.readers.len() - 1) * batches
        })
        .sum();
    assert!(saved > 0, "the shared sources emitted nothing");
    let held_alone: usize = alone.iter().map(|report| report.holds).sum();
    assert_eq!(fused.report.holds, held_alone - saved);
}

#[tokio::test]
async fn a_fused_run_holds_each_shared_batch_once_at_tp1_single() {
    a_fused_run_holds_each_shared_batch_once(TP1_SINGLE).await;
}

#[tokio::test]
async fn a_fused_run_holds_each_shared_batch_once_at_tp4_sized() {
    a_fused_run_holds_each_shared_batch_once(TP4_SIZED).await;
}

/// q95 at tp1-single, planned from its corpus text.
async fn q95() -> (SessionContext, Box<dyn GpuNode>) {
    let sql =
        std::fs::read_to_string(queries_dir_for("tpcds").join("q95.sql")).expect("q95's text");
    let (ctx, mut trees) = planned("tpcds", &[&sql], TP1_SINGLE).await;
    (ctx, trees.pop().expect("one tree"))
}

/// q95 with its four `web_sales` loads, which read the same columns, made one shared source.
fn shared(q95: Box<dyn GpuNode>) -> Vec<Box<dyn GpuNode>> {
    let (roots, sharing) = share_sources(vec![q95]);
    assert_eq!(sharing, 1, "web_sales, read four times as one");
    roots
}

#[tokio::test]
async fn q95_shared_without_buffers_is_refused() {
    let (_, tree) = q95().await;
    match validate_roots(&refs(&shared(tree))) {
        Err(PlanError::UnbufferedHold { .. }) => {}
        other => panic!("q95 shared without buffers: {other:?}"),
    }
}

#[tokio::test]
async fn q95_fused_with_buffers_answers_as_q95_alone() {
    let (ctx, tree) = q95().await;
    let alone = alone(tree.as_ref(), &ctx, "q95");
    let (roots, buffers) = buffer_unbuffered_holds(shared(tree), None);
    assert_eq!(
        buffers, 4,
        "each self-join's probe edge, and each coalesce under a semi join's probe"
    );
    let fused = fused(&roots, &ctx, "q95");
    assert_answers_as_alone(&fused, &[alone], "q95");
}
