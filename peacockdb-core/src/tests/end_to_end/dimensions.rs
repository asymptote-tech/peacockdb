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

/// The one plan shape the degenerate hash is not run against, and why it is a refusal
/// rather than a defect.
///
/// A hash that puts every key in lane 0 leaves every other lane's build side empty, and
/// Right, Full and RightAnti answer an empty build side with their probe side — a call
/// over a build table that does not exist ([#175](../../../../llm-wiki/tickets.md#t175)). The
/// candidate set drops the dimension for those plans, so this is what says the drop is a
/// refusal the engine makes rather than a shape nobody tried.
#[tokio::test]
async fn a_degenerate_hash_under_a_right_outer_is_refused_by_name() {
    use crate::tests::injection::{Hash, planned_mode};

    let data_dir = data_dir_for("tpcds", "1");
    let sql =
        std::fs::read_to_string(queries_dir_for("tpcds").join("q93.sql")).expect("the query text");
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
    let (tree, _memory) =
        planner::plan(&plan, mode.knobs()).unwrap_or_else(|error| panic!("q93 at {name}: {error}"));
    assert!(
        planned_mode(name, tree.as_ref()).owes_probe_when_empty,
        "q93 is here because its Right outer owes its probe side, and this plan has none"
    );

    let injected = apply(
        tree.as_ref(),
        Injection {
            hash: Hash::Degenerate,
            ..Injection::NONE
        },
        SEED,
    );
    let context = InjectedContext::new(
        ctx.task_ctx(),
        Injection {
            hash: Hash::Degenerate,
            ..Injection::NONE
        },
        SEED,
    );
    match run::<Injected>(injected.as_ref(), &context, None) {
        Err(error) => {
            let message = error.to_string();
            assert!(
                message.contains("#175") && message.contains("build side is empty"),
                "the refusal names neither the ticket nor the cause: {message}"
            );
        }
        Ok(_) => panic!("a lane with no build side should have been refused"),
    }
}
