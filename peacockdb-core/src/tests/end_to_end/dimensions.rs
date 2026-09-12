//! That each injected dimension does something.
//!
//! Every injected run in the query matrix asserts the same answer, which is exactly what a
//! dimension that did nothing would also produce. So each is read off the trace against
//! the same plan uninjected, and the one dimension the engine refuses is refused by name.

use crate::executor::run;
use crate::planner;
use crate::test_support::{MODES, data_dir_for, queries_dir_for};
use crate::tests::injection::{Injected, InjectedContext, Injection, SEED, apply};

/// Three of the four dimensions, made visible in the calls rather than in the rows.
///
/// Every injected run asserts the same answer, which is exactly what a dimension that did
/// nothing would also produce: a setting that never fired would pass every injected query and
/// prove nothing. So each is read off the trace against the same plan uninjected.
#[tokio::test]
async fn an_injected_run_makes_different_calls_from_the_plan_it_came_from() {
    use crate::executor::CallKind;
    use crate::plan::{NodeRef, as_node_ref};
    use crate::tests::injection::{Drain, Empties, Rebatch};

    let data_dir = data_dir_for("tpch", "1");
    let sql = std::fs::read_to_string(queries_dir_for("tpch").join("nested-loop-join.sql"))
        .expect("the query text");
    // The one mode with batching off, so the small-table rule leaves every source at four
    // lanes and there is a lane to drain.
    let mode = &MODES[2];
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
        .unwrap_or_else(|error| panic!("nested-loop-join at {name}: {error}"));

    let run = |injection: Injection| {
        let injected = apply(tree.as_ref(), injection, SEED);
        let context = InjectedContext::new(ctx.task_ctx(), injection, SEED);
        let report = run::<Injected>(injected.as_ref(), &context, None)
            .unwrap_or_else(|error| panic!("nested-loop-join at {}: {error}", injection.label()));
        let pulls = report
            .trace
            .iter()
            .filter(|event| event.call == CallKind::NextBatch)
            .count();
        let lane_pulls = |lane: u32| {
            report
                .trace
                .iter()
                .filter(|event| event.call == CallKind::NextBatch && event.lane == lane)
                .count()
        };
        (report.lanes_of.len(), pulls, lane_pulls(0))
    };

    let (nodes, pulls, first_lane) = run(Injection::NONE);
    assert!(
        first_lane > 0,
        "lane 0 read nothing before anything was injected, so draining it proves nothing"
    );

    // A rebatcher is a node the plan did not ask for, so the tree it runs is a longer one.
    let (rebatched, _, _) = run(Injection {
        rebatch: Rebatch::AboveSources,
        ..Injection::NONE
    });
    let sources = sources_in(tree.as_ref());
    assert_eq!(
        rebatched,
        nodes + sources,
        "a rebatcher above each of the {sources} sources added {} nodes",
        rebatched - nodes
    );

    // A drained lane is live and reads nothing; its row groups went to its neighbour, so
    // the pulls are the same count from one lane fewer.
    let (_, drained_pulls, drained_first) = run(Injection {
        drain: Drain::FirstLane,
        ..Injection::NONE
    });
    assert_eq!(
        drained_first, 0,
        "the drained lane read {drained_first} times"
    );
    assert_eq!(
        drained_pulls, pulls,
        "draining a lane changed how many batches were read, so rows moved rather than \
         being re-lanes"
    );

    // An empty batch is a call that produced no rows, so the sources are pulled more often
    // for the same rows.
    let (_, empty_pulls, _) = run(Injection {
        empties: Empties::Sometimes(50),
        ..Injection::NONE
    });
    assert!(
        empty_pulls > pulls,
        "the empty-batch setting fired on none of the {pulls} pulls"
    );

    fn sources_in(node: &dyn crate::plan::GpuNode) -> usize {
        usize::from(matches!(as_node_ref(node), NodeRef::LoadParquet(_)))
            + node.children().into_iter().map(sources_in).sum::<usize>()
    }
}

/// The shape the degenerate hash was not run against until #175's fix: every key in lane
/// 0 leaves the other lanes' build sides empty, and Right, Full and RightAnti owe their
/// probe side over that. The driver keeps the scatter's zero-row table for those, and
/// what the CPU's join computes over it is checked against DataFusion on the same SQL —
/// `Right` is the build-side pad, `RightAnti` is every probe row. Each asserts on the
/// calls too, so a run that never reached an empty build lane cannot pass for the wrong
/// reason.
async fn an_empty_build_lane_answers_like_the_oracle(dataset: &str, query: &str) {
    use super::{Oracle, run_and_check, sorted_rows};
    use crate::executor::{CallKind, PlanIndex};
    use crate::plan::{ExecutorCategory, NodeRef, as_node_ref, empty_build_answers_nothing};
    use crate::tests::injection::{Hash, planned_mode};

    let data_dir = data_dir_for(dataset, "1");
    let sql = std::fs::read_to_string(queries_dir_for(dataset).join(format!("{query}.sql")))
        .expect("the query text");
    let oracle_ctx = crate::register_tables_for(crate::build_session_state(1), &data_dir)
        .await
        .expect("register the tables");
    let expected = oracle_ctx
        .sql(&sql)
        .await
        .expect("the oracle plans the query")
        .collect()
        .await
        .expect("the oracle runs the query");

    let mode = &MODES[2];
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
        .unwrap_or_else(|error| panic!("{query} at {name}: {error}"));
    assert!(
        planned_mode(name, tree.as_ref()).owes_probe_when_empty,
        "{query} is here because a join in it owes its probe side, and this plan has none"
    );

    let injection = Injection {
        hash: Hash::Degenerate,
        ..Injection::NONE
    };
    let injected = apply(tree.as_ref(), injection, SEED);
    let report = run_and_check(
        injected.as_ref(),
        &ctx.task_ctx(),
        injection,
        &Oracle {
            batches: &expected,
            rows: sorted_rows(&expected),
            tolerance: None,
        },
        &format!("{dataset}/{query} at {name} with a degenerate hash"),
    );

    // The lanes the answer was made over: a scatter feeding an owing build side emitted
    // a zero-row table on some lane, and no lane of an owing join was left to ask what
    // it owed. The plan's other joins owe nothing and still take that route.
    let index = PlanIndex::build(injected.as_ref()).expect("the plan indexes");
    let empty_build_lanes: usize = (0..index.len())
        .filter(|node| {
            index.nodes[*node].category == ExecutorCategory::PartitionEmitter
                && index.feeds_owing_build(*node)
        })
        .map(|node| {
            report.emitted[node]
                .iter()
                .filter(|lane| !lane.is_empty() && lane.iter().all(|batch| batch.rows == 0))
                .count()
        })
        .sum();
    assert!(
        empty_build_lanes > 0,
        "{query}: the degenerate hash left no build lane empty, so nothing here was tested"
    );
    let owing_join = |node: usize| match as_node_ref(index.nodes[node].node) {
        NodeRef::Join(join) => !empty_build_answers_nothing(join.join_type),
        _ => false,
    };
    assert_eq!(
        report
            .trace
            .iter()
            .filter(|event| event.call == CallKind::NoBuild && owing_join(event.node as usize))
            .count(),
        0,
        "{query}: a lane of an owing join reached NoBuild, the route this fix takes away"
    );
}

/// `Right` is left_join(probe, build) with left_policy = NULLIFY, which *is* the
/// build-side pad — computed by `operators/join.cpp:299` on the device and by DataFusion's
/// join on the CPU, and checked here against the oracle rather than argued.
#[tokio::test]
async fn a_right_join_with_an_empty_build_pads_every_probe_row() {
    an_empty_build_lane_answers_like_the_oracle("tpcds", "q93").await;
}

/// `RightAnti` is left_anti_join over empty keys, which returns every probe row.
#[tokio::test]
async fn a_right_anti_join_with_an_empty_build_returns_every_probe_row() {
    an_empty_build_lane_answers_like_the_oracle("tpch", "anti-join").await;
}
