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
/// as the unload's skip/fetch, and the mid-plan one as a `GpuLimit` over the part scan, with
/// each scan's pushed cut a `GpuLimit` beneath — so one query holds every count below.
#[tokio::test]
async fn a_limit_slices_at_most_two_batches_and_stops_the_scan() {
    use crate::executor::CallKind;
    use crate::plan::{NodeRef, as_node_ref};

    let data_dir = data_dir_for("tpch", "1");
    let sql = std::fs::read_to_string(queries_dir_for("tpch").join("nested-limits.sql"))
        .expect("the query text");
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
        // 28 rows and a row group holds far more, and the scan under it maps no second one.
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
        // No limit holds anything whatever its offset, which is what the slice symbol buys:
        // its queue never carries more than the one batch it was handed. Three of them: the
        // cut over each scan, and the offset above part's.
        let limits = limit_nodes(tree.as_ref());
        assert_eq!(
            limits.len(),
            3,
            "nested-limits at {name}: limits at {limits:?}"
        );
        for limit in limits {
            assert!(
                report.peak_queued[limit] <= 1,
                "nested-limits at {name}: the limit at {limit} queued {} batches",
                report.peak_queued[limit]
            );
        }
        // Each scan maps only the row groups its cut needs, one apiece, so the plan offers
        // exactly the batches pulled at every mode: the read is bounded by the mapping.
        // Stopping a scan that offers more is the driver's mock cases' to prove.
        assert_eq!(
            offered, pulled,
            "nested-limits at {name}: the scans map {offered} batches for {pulled} pulls"
        );
    }

    /// Every limit's index in the driver's pre-order numbering, which is the tree walked
    /// children-after-self.
    fn limit_nodes(root: &dyn crate::plan::GpuNode) -> Vec<usize> {
        fn walk(node: &dyn crate::plan::GpuNode, next: &mut usize, found: &mut Vec<usize>) {
            if matches!(as_node_ref(node), NodeRef::Limit(_)) {
                found.push(*next);
            }
            *next += 1;
            for child in node.children() {
                walk(child, next, found);
            }
        }
        let mut found = Vec::new();
        walk(root, &mut 0, &mut found);
        found
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

/// A limit DataFusion pushed into the scan answers its count at every mode, from one row
/// group: the scan maps only the prefix its cut needs, so the single-batch modes no longer
/// read all forty-nine. At the tp1 modes the cpu reader ignored the scan's limit and
/// answered all 6,001,215 rows (#186).
#[tokio::test]
async fn a_scan_limit_answers_its_count_from_one_read_at_every_mode() {
    use crate::executor::CallKind;
    use crate::plan::{NodeRef, as_node_ref};

    let data_dir = data_dir_for("tpch", "1");
    for mode in &MODES {
        let name = mode.name;
        let ctx = crate::register_tables_for(
            crate::build_session_state(mode.target_partitions),
            &data_dir,
        )
        .await
        .expect("register the tables");
        let plan = ctx
            .sql("SELECT * FROM lineitem LIMIT 10")
            .await
            .expect("the query plans")
            .create_physical_plan()
            .await
            .expect("the query has a physical plan");
        let (tree, _memory) = planner::plan(&plan, mode.knobs())
            .unwrap_or_else(|error| panic!("scan-limit at {name}: {error}"));
        let NodeRef::LoadParquet(load) = as_node_ref(tree.children()[0]) else {
            panic!("scan-limit at {name}: the unload reads a loader");
        };
        assert_eq!(
            load.partition_groups,
            vec![vec![vec![0]]],
            "scan-limit at {name}: ten rows need lineitem's first row group alone"
        );
        let report = run::<CpuBackend>(tree.as_ref(), &ctx.task_ctx(), None)
            .unwrap_or_else(|error| panic!("scan-limit at {name}: {error}"));
        let rows: usize = report
            .batches
            .iter()
            .map(|batch| batch.record_batch().num_rows())
            .sum();
        assert_eq!(
            rows, 10,
            "scan-limit at {name} answered {rows} rows for LIMIT 10"
        );
        let pulled = report
            .trace
            .iter()
            .filter(|e| e.call == CallKind::NextBatch)
            .count();
        assert_eq!(
            pulled, 1,
            "scan-limit at {name}: the scan was pulled {pulled} times, and its first batch \
             holds the ten rows"
        );
        assert_eq!(
            report.in_flight_bytes, 0,
            "scan-limit at {name} ended holding batches"
        );
    }
}

/// A limited subquery under an aggregate. DataFusion's pushdown erases the limit node and
/// leaves the cut in the scan alone, so before #186 no reader applied it and every mode
/// counted all 25 rows. `count(*)` would not show it: statistics answer that, and the
/// translator refuses the plan (#158).
#[tokio::test]
async fn a_limited_subquery_under_an_aggregate_counts_only_its_cut_at_every_mode() {
    super::sql_answers_match_datafusion(
        "tpch",
        "limited subquery",
        "SELECT count(n_name) FROM (SELECT * FROM nation LIMIT 3)",
        None,
        super::Coverage::ModesOnly,
    )
    .await;
}

/// The hold's own effect, at a scan whose mapping cannot stand in for it. DataFusion pushes
/// no limit through a `FilterExec`, so the scan carries no cut, `source()`'s prefix trim has
/// nothing to trim, and the loader maps all 49 of lineitem's row groups at every mode. An
/// aggregate above the limit makes it a node rather than the unload's interval, so what
/// bounds the read is the driver's hold on the satisfied limit's subtree and nothing else.
#[tokio::test]
async fn a_limit_stops_a_scan_that_maps_every_row_group_at_every_mode() {
    use crate::executor::CallKind;
    use crate::plan::{NodeRef, as_node_ref};
    use crate::planner::BatchSizing;
    use datafusion::arrow::array::AsArray;
    use datafusion::arrow::datatypes::Int64Type;

    const SQL: &str =
        "SELECT count(l_quantity) FROM (SELECT * FROM lineitem WHERE l_quantity > 0 LIMIT 10)";

    let data_dir = data_dir_for("tpch", "1");
    for mode in &MODES {
        let name = mode.name;
        let ctx = crate::register_tables_for(
            crate::build_session_state(mode.target_partitions),
            &data_dir,
        )
        .await
        .expect("register the tables");
        let plan = ctx
            .sql(SQL)
            .await
            .expect("the query plans")
            .create_physical_plan()
            .await
            .expect("the query has a physical plan");
        let (tree, _memory) = planner::plan(&plan, mode.knobs())
            .unwrap_or_else(|error| panic!("filtered limit at {name}: {error}"));
        let mapping = loader_mapping(tree.as_ref());
        assert_eq!(
            mapping.groups, 49,
            "filtered limit at {name}: the scan carries no cut, so nothing trims its 49 \
             row groups"
        );
        let report = run::<CpuBackend>(tree.as_ref(), &ctx.task_ctx(), None)
            .unwrap_or_else(|error| panic!("filtered limit at {name}: {error}"));
        let pulled = report
            .trace
            .iter()
            .filter(|e| e.call == CallKind::NextBatch)
            .count();
        assert!(
            !report.satisfied.is_empty(),
            "filtered limit at {name}: the run drained rather than stopping at its limit"
        );
        // A lane's first batch already holds more rows than the cut wants, so no lane is
        // pulled twice however many batches its mapping offers.
        assert!(
            pulled <= mode.target_partitions,
            "filtered limit at {name}: {pulled} pulls for {} lanes, and one batch a lane \
             is all the cut needs",
            mode.target_partitions
        );
        // Where the mapping is one batch per row group it offers all 49 and the hold is the
        // whole of the bound. The other three modes map one batch a lane, so there is
        // nothing left there for a hold to save.
        if matches!(mode.sizing, BatchSizing::OneBatchPerRowGroup) {
            assert_eq!(
                (mapping.batches, pulled),
                (49, 1),
                "filtered limit at {name}: {} batches offered and {pulled} pulled",
                mapping.batches
            );
        }
        let answer: Vec<i64> = report
            .batches
            .iter()
            .flat_map(|batch| {
                batch
                    .record_batch()
                    .column(0)
                    .as_primitive::<Int64Type>()
                    .values()
                    .to_vec()
            })
            .collect();
        assert_eq!(
            answer,
            vec![10],
            "filtered limit at {name}: the cut keeps ten of lineitem's rows"
        );
        assert_eq!(
            report.in_flight_bytes, 0,
            "filtered limit at {name} ended holding batches"
        );
    }

    /// The one loader's mapping, in the two units the hold is argued in: the row groups it
    /// covers, and the batches it offers over them.
    fn loader_mapping(node: &dyn crate::plan::GpuNode) -> Mapping {
        if let NodeRef::LoadParquet(load) = as_node_ref(node) {
            return Mapping {
                groups: load.partition_groups.iter().flatten().flatten().count(),
                batches: load.partition_groups.iter().map(Vec::len).sum(),
            };
        }
        node.children()
            .into_iter()
            .map(loader_mapping)
            .fold(Mapping::default(), |total, found| Mapping {
                groups: total.groups + found.groups,
                batches: total.batches + found.batches,
            })
    }

    #[derive(Default)]
    struct Mapping {
        groups: usize,
        batches: usize,
    }
}
