//! `end_to_end::dag`'s fused plans on the device: several queries' plans as one plan in one
//! session, each shared source loaded once, and each root answering what its query answers
//! alone in a session of its own.

use crate::plan::GpuNode;
use crate::test_support::{
    MODES, Mode, answer_on_the_device, answers_of_roots_on_the_device, batches_to_sorted_str,
    queries_dir_for,
};
use crate::tests::share::{
    LINEITEM_BY_QUANTITY, LINEITEM_TOTALS, ORDERS_BY_PRIORITY, QUANTITY_BY_PRIORITY,
    buffer_unbuffered_holds, planned, share_sources,
};
use crate::wire::join_scratch_bytes;

const TP1_SINGLE: &Mode = &MODES[0];
const TP4_SIZED: &Mode = &MODES[4];

/// `queries` over `dataset` planned at `mode`, each run alone on the device, then fused —
/// `shared` sources shared and `buffers` buffers inserted — and run as one plan: each root's
/// answer, sorted, byte for byte its query's alone.
async fn answers_as_alone_on_the_device(
    dataset: &str,
    queries: &[&str],
    mode: &Mode,
    shared: usize,
    buffers: usize,
) {
    let what = format!(
        "{dataset}, {} queries at {} on a device",
        queries.len(),
        mode.name
    );
    let (_ctx, trees) = planned(dataset, queries, mode).await;
    let join_scratch = join_scratch_bytes(&mode.knobs_for(dataset));
    let alone: Vec<String> = trees
        .iter()
        .map(|tree| {
            batches_to_sorted_str(&answer_on_the_device(tree.as_ref(), join_scratch, &what))
        })
        .collect();
    let (roots, sharing) = share_sources(trees);
    assert_eq!(sharing, shared, "{what}: sources shared");
    let (roots, inserted) = buffer_unbuffered_holds(roots, None);
    assert_eq!(inserted, buffers, "{what}: buffers inserted");
    let refs: Vec<&dyn GpuNode> = roots.iter().map(|root| root.as_ref()).collect();
    let fused = answers_of_roots_on_the_device(&refs, join_scratch, &what);
    assert_eq!(fused.len(), alone.len(), "{what}: one answer per root");
    for (root, (answer, alone)) in fused.iter().zip(&alone).enumerate() {
        assert_eq!(&batches_to_sorted_str(answer), alone, "{what}: root {root}");
    }
}

const TWO_ROOTS: [&str; 2] = [LINEITEM_TOTALS, LINEITEM_BY_QUANTITY];
const THREE_ROOTS: [&str; 3] = [LINEITEM_TOTALS, QUANTITY_BY_PRIORITY, ORDERS_BY_PRIORITY];

#[tokio::test]
async fn two_roots_over_one_shared_source_answer_as_alone_on_the_device_at_tp1_single() {
    answers_as_alone_on_the_device("tpch", &TWO_ROOTS, TP1_SINGLE, 1, 0).await;
}

#[tokio::test]
async fn two_roots_over_one_shared_source_answer_as_alone_on_the_device_at_tp4_sized() {
    answers_as_alone_on_the_device("tpch", &TWO_ROOTS, TP4_SIZED, 1, 0).await;
}

#[tokio::test]
async fn three_roots_over_two_shared_sources_answer_as_alone_on_the_device_at_tp1_single() {
    answers_as_alone_on_the_device("tpch", &THREE_ROOTS, TP1_SINGLE, 2, 1).await;
}

#[tokio::test]
async fn three_roots_over_two_shared_sources_answer_as_alone_on_the_device_at_tp4_sized() {
    answers_as_alone_on_the_device("tpch", &THREE_ROOTS, TP4_SIZED, 2, 1).await;
}

#[tokio::test]
async fn q95_fused_with_buffers_answers_as_q95_alone_on_the_device() {
    let sql =
        std::fs::read_to_string(queries_dir_for("tpcds").join("q95.sql")).expect("q95's text");
    answers_as_alone_on_the_device("tpcds", &[&sql], TP1_SINGLE, 1, 4).await;
}
