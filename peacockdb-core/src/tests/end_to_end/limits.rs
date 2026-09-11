//! What a limit costs, in calls rather than in rows.
//!
//! The claims a result comparison cannot make: the same rows come back whether the plan
//! read the whole table or stopped, so what a limit buys is only visible as calls not made.

use crate::executor::{CpuBackend, run};
use crate::planner;
use crate::test_support::{MODES, data_dir_for, queries_dir_for};

/// The claims a result comparison cannot make: the same rows come back whether the plan
/// read the whole table or stopped, so what a limit buys is only visible as calls not made.
///
/// `nested-limits` carries both lowerings on one root-to-leaf path — the root-adjacent one
/// as the unload's skip/fetch, and the mid-plan one as a `GpuLimit` over the scan — so one
/// query holds every count below.
#[tokio::test]
async fn a_limit_slices_at_most_two_batches_and_stops_the_scan() {
    use crate::executor::CallKind;
    use crate::plan::{NodeRef, as_node_ref};

    let data_dir = data_dir_for("tpch", "1");
    let sql = std::fs::read_to_string(queries_dir_for("tpch").join("nested-limits.sql"))
        .expect("the query text");
    let mut most_offered = 0;
    for mode in &MODES {
        let name = mode.name;
        let ctx = crate::register_tables_for(
            crate::build_session_state(mode.target_partitions),
            &data_dir,
        )
        .await
        .expect("register the tables");
        let plan = ctx
            .sql(&sql)
            .await
            .expect("the query plans")
            .create_physical_plan()
            .await
            .expect("the query has a physical plan");
        let (tree, _memory) = planner::plan(&plan, mode.knobs())
            .unwrap_or_else(|error| panic!("nested-limits at {name}: {error}"));
        let offered = batches_offered(tree.as_ref());
        let report = run::<CpuBackend>(tree.as_ref(), &ctx.task_ctx(), None)
            .unwrap_or_else(|error| panic!("nested-limits at {name}: {error}"));
        let calls = |kind: CallKind| report.trace.iter().filter(|e| e.call == kind).count();

        // An interval has two ends, so at most two batches can straddle it — and a batch
        // wholly inside is forwarded untouched rather than sliced.
        assert!(
            calls(CallKind::UnloadRange) <= 2,
            "nested-limits at {name}: {} batches sliced at the boundary",
            calls(CallKind::UnloadRange)
        );
        // Two sources, and each is satisfied by its first batch: the mid-plan limit wants
        // 28 rows and a row group holds far more, so the scan under it never reads a
        // second one whatever the batching.
        let pulled = calls(CallKind::NextBatch);
        assert_eq!(
            pulled, 2,
            "nested-limits at {name}: the sources were pulled {pulled} times, and one \
             batch each is what their limits need"
        );
        assert!(
            !report.satisfied.is_empty(),
            "nested-limits at {name}: the run drained rather than stopping at its limit"
        );
        // The mid-plan limit holds nothing whatever its offset, which is what the slice
        // symbol buys: its queue never carries more than the one batch it was handed. The
        // executor's own residency is a unit case; this is the claim the driver makes.
        let limit = limit_node(tree.as_ref());
        assert!(
            report.peak_queued[limit] <= 1,
            "nested-limits at {name}: the limit queued {} batches",
            report.peak_queued[limit]
        );
        most_offered = most_offered.max(offered);
    }
    // Without this the count above is a claim about a plan that had nothing to stop: at
    // the single-batch modes a mapping offers one batch per lane and stopping is free.
    assert!(
        most_offered > 2,
        "no mode offered more batches than were pulled, so nothing here was stopped"
    );

    /// The mid-plan limit's index in the driver's pre-order numbering, which is the tree
    /// walked children-after-self.
    fn limit_node(root: &dyn crate::plan::GpuNode) -> usize {
        fn walk(node: &dyn crate::plan::GpuNode, next: &mut usize) -> Option<usize> {
            let here = *next;
            *next += 1;
            if matches!(as_node_ref(node), NodeRef::Limit(_)) {
                return Some(here);
            }
            node.children()
                .into_iter()
                .find_map(|child| walk(child, next))
        }
        walk(root, &mut 0).expect("nested-limits carries a mid-plan limit")
    }

    /// How many batches every source in the plan could produce — the mapping's own count,
    /// which is what a scan that ran to the end would have read.
    fn batches_offered(node: &dyn crate::plan::GpuNode) -> usize {
        let here = match as_node_ref(node) {
            NodeRef::LoadParquet(load) => load.partition_groups.iter().map(Vec::len).sum(),
            _ => 0,
        };
        here + node
            .children()
            .into_iter()
            .map(batches_offered)
            .sum::<usize>()
    }
}
