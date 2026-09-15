//! The accounting, on a real plan.
//!
//! Eight `Executor` impls report a residency and a transient, and until this ran on a real
//! plan the only things reading them were two unit cases: every end-to-end run in the
//! query matrix passes `None`, which is the accountant watching rather than enforcing.

use datafusion::execution::context::SessionContext;

use crate::executor::{CpuBackend, RunError, RunReport, When, run};
use crate::plan::GpuNode;
use crate::planner;
use crate::test_support::{MODES, Mode, data_dir_for, queries_dir_for};
use crate::tests::injection::{
    Drain, Injected, InjectedContext, Injection, Rebatch, SEED, apply, planned_mode,
};

// The budget path, on a query rather than on a mock. The budget is searched for rather
// than taken from the watching run's peak, and the reason is the finding: a pre-call check
// tests the MODELLED transient, which can exceed what the run was ever observed holding —
// this query peaks at 8,222 bytes and the smallest budget it fits in is 9,556, which is
// what its nested-loop join asks for before a call. A budget equal to the observed peak
// therefore trips, which is the accounting working rather than failing.
//
// So the claim is that a boundary exists and is one byte wide: the smallest budget that
// completes, and the byte below it that does not.
/// Ignored under [#182](../../../../llm-wiki/tasks/active-tickets.md) rather than deleted, so it stays
/// compiled and listed: pricing a `CpuBatch` from the plan's schema moved the accounting under
/// it and the budget stopped being a boundary. The bar for T18 is results and node stats
/// consistent between the engines; memory accounting is deferred.
#[tokio::test]
#[ignore = "#182: pricing a batch from its schema moved the accounting under this — 10058 fits and so does 10057, so the budget is no longer a boundary"]
async fn a_query_has_a_smallest_budget_that_fits_and_trips_a_byte_below_it() {
    let data_dir = data_dir_for("tpch", "1");
    let sql = std::fs::read_to_string(queries_dir_for("tpch").join("nested-loop-join.sql"))
        .expect("the query text");
    let mode = &MODES[3];
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

    let watching = run::<CpuBackend>(tree.as_ref(), &ctx.task_ctx(), None)
        .expect("the unbudgeted run finishes");
    assert!(
        watching.peak_bytes > 0,
        "an unbudgeted run observed nothing"
    );

    // Why the two figures differ, since a reader who tries to remove the double count
    // finds this test red: a probing join's `resident_bytes` is its build side plus its
    // accumulated keys, and its `scratch_bytes` adds the build side AGAIN, because the
    // call is about to read it. The spec has scratch consult `self`, so it is deliberate.
    // Measured here: peak 8,222, smallest fitting budget 9,556, and region — this join's
    // build side — is 920 bytes on its own. So the gap is that side plus whatever else was
    // resident at the instant the check ran, not that side alone.
    let fits =
        |budget: usize| run::<CpuBackend>(tree.as_ref(), &ctx.task_ctx(), Some(budget)).is_ok();
    // The observed peak is a floor for the search and not the answer, per the block above.
    let (mut low, mut high) = (watching.peak_bytes, watching.peak_bytes * 8);
    assert!(fits(high), "the query does not run at eight times its peak");
    while low + 1 < high {
        let middle = low + (high - low) / 2;
        if fits(middle) {
            high = middle
        } else {
            low = middle
        }
    }
    assert!(fits(high), "the smallest fitting budget does not fit");
    // Asserted as a shape rather than as a number, so the case survives a fixture change:
    // the excess is a real quantity of this plan's — one side of one join and the rest of
    // what was live — rather than slack.
    let excess = high - watching.peak_bytes;
    assert!(
        excess > 0 && excess < watching.peak_bytes,
        "the smallest fitting budget is {high} against a peak of {}, and the gap should be \
         one side of one join rather than a multiple of the run",
        watching.peak_bytes
    );

    match run::<CpuBackend>(tree.as_ref(), &ctx.task_ctx(), Some(high - 1)) {
        Err(RunError::BudgetExceeded { when, message }) => {
            assert_eq!(when, When::PreCall, "{message}");
            assert!(
                message.contains("GpuNestedLoopJoin"),
                "the failure names the node it happened at: {message}"
            );
        }
        other => panic!("a byte below {high} should not fit, got {other:?}"),
    }
}

/// What a call measured, against what its executor said it would take.
///
/// `CallStats::scratch_bytes` is the measured half of the accounting, and every CPU
/// executor returned `None` for it until the exec nodes began reporting — so
/// `underestimates` was empty on every query for want of an input rather than for want of
/// an underestimate.
///
/// So `measured_calls` is asserted first and the empty list second, in that order: an
/// empty list means the model held only where something measured. Red with
/// `CallStats::default()` back in `CpuExec::exec` — which is the regression this exists to
/// catch, and which `calls > 0` could not see, since a call counts whether it measured or
/// not.
#[tokio::test]
async fn the_model_is_compared_against_what_the_calls_measured() {
    let data_dir = data_dir_for("tpch", "1");
    let sql = std::fs::read_to_string(queries_dir_for("tpch").join("filter-project.sql"))
        .expect("the query text");
    let mode = &MODES[3];
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
        .unwrap_or_else(|error| panic!("filter-project at {name}: {error}"));
    let report = run::<CpuBackend>(tree.as_ref(), &ctx.task_ctx(), None).expect("the run finishes");
    assert!(
        report.measured_calls > 0,
        "no call reported a measured transient, so there was nothing to compare the model \
         against"
    );
    assert!(
        report.underestimates.is_empty(),
        "an exec node needed more than its model allowed: {:?}",
        report.underestimates
    );
}

/// A query planned at one mode, so the runs a boundary search takes share one plan.
struct PlannedQuery {
    ctx: SessionContext,
    tree: Box<dyn GpuNode>,
    /// Read off the tree, since the small-table rule can leave a query at one lane whatever
    /// `target_partitions` asked for — and a one-lane query is one a drain cannot reach.
    lanes: usize,
}

impl PlannedQuery {
    async fn plan(dataset: &str, query: &str, mode: &Mode) -> Self {
        let name = mode.name;
        let sql = std::fs::read_to_string(queries_dir_for(dataset).join(format!("{query}.sql")))
            .expect("the query text");
        let ctx = crate::register_tables_for(
            crate::build_session_state(mode.target_partitions),
            &data_dir_for(dataset, "1"),
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
        let lanes = planned_mode(name, tree.as_ref()).lanes;
        Self { ctx, tree, lanes }
    }

    fn run(&self, injection: Injection, budget: Option<usize>) -> Result<RunReport, RunError> {
        let injected = apply(self.tree.as_ref(), injection, SEED);
        let context = InjectedContext::new(self.ctx.task_ctx(), injection, SEED);
        run::<Injected>(injected.as_ref(), &context, budget)
    }

    /// The smallest budget a shape completes at, and the peak it was seen holding. Searched
    /// from that peak rather than taken from it: a pre-call check tests the MODELLED
    /// transient, which can exceed anything the run was seen holding, so the peak is a
    /// floor rather than the answer.
    fn boundary(&self, injection: Injection) -> (usize, usize) {
        let peak = self
            .run(injection, None)
            .expect("the unbudgeted run finishes")
            .peak_bytes;
        let (mut low, mut high) = (peak, peak * 8);
        assert!(
            self.run(injection, Some(high)).is_ok(),
            "{} does not run at eight times its peak",
            injection.label()
        );
        while low + 1 < high {
            let middle = low + (high - low) / 2;
            if self.run(injection, Some(middle)).is_ok() {
                high = middle
            } else {
                low = middle
            }
        }
        (peak, high)
    }
}

/// The `(peak, smallest fitting budget)` pair of one query as planned and under one
/// injected shape. Asserts what both callers claim: the shape moved what the query holds,
/// and the injected plan's budget is a boundary — it completes at that budget and trips a
/// byte below, at the phase and the node named here. Whether the budget moved with the
/// peak is the caller's claim rather than this one's, so the pairs are returned.
fn boundaries_under(
    planned: &PlannedQuery,
    injection: Injection,
    trip: (When, &str),
) -> ((usize, usize), (usize, usize)) {
    let (as_planned, injected) = (
        planned.boundary(Injection::NONE),
        planned.boundary(injection),
    );
    assert_ne!(
        as_planned.0,
        injected.0,
        "{} did not move what the query holds, so this proves nothing about the accounting \
         under injection",
        injection.label()
    );
    match planned.run(injection, Some(injected.1 - 1)) {
        Err(RunError::BudgetExceeded { when, message }) => {
            assert_eq!(when, trip.0, "{message}");
            assert!(
                message.contains(trip.1),
                "the failure names the node it happened at: {message}"
            );
        }
        other => panic!("a byte below {} should not fit, got {other:?}", injected.1),
    }
    (as_planned, injected)
}

// The accounting under an injected shape, which is the half no other case reaches.
//
// Every other injected run passes no budget, so the accountant watches rather than
// enforces, and the two rewrites reach it by different halves: a drained lane moves row
// groups between lanes, a rebatcher moves the batch sizes the accountant prices.
// `q16` at four lanes peaks at 104.7 MB as planned and 77.9 MB with lane 0 drained, and
// its budget follows, 131.5 MB against 104.7 MB. `nested-loop-join` at tp4-rowgroup
// peaks at 8,222 bytes and 8,540 under a rebatcher, and its budget does not move: the
// pre-call check tests the join's own transient, which merging a lane's batches leaves
// alone.
/// Ignored under [#182](../../../../llm-wiki/tasks/active-tickets.md), the same change: the peak stopped
/// depending on the batch shape, since logical bytes are a function of rows and var-length
/// content alone, so `rebatch=sources` moves nothing and this case's premise is gone. Whether a
/// peak that ignores batch shape is right is the ticket's question.
#[tokio::test]
#[ignore = "#182: with a batch priced from its schema the peak no longer depends on the batch shape, so rebatch=sources moves nothing and this case's premise is gone"]
async fn an_injected_shape_moves_what_the_query_holds() {
    // Batching off, so the small-table rule leaves the sources at four lanes and there is a
    // lane whose row groups can move.
    let q16 = PlannedQuery::plan("tpcds", "q16", &MODES[2]).await;
    let (as_planned, drained) = boundaries_under(
        &q16,
        Injection {
            drain: Drain::FirstLane,
            ..Injection::NONE
        },
        (When::PreCall, "GpuEmitPartitions"),
    );
    assert_ne!(
        as_planned.1, drained.1,
        "the peak moved and the budget it needs did not"
    );

    // The drain's complement: a query whose residency only a rebatcher reaches, because a
    // lane it could move row groups out of is what this one does not have.
    let nested_loop = PlannedQuery::plan("tpch", "nested-loop-join", &MODES[3]).await;
    assert_eq!(
        nested_loop.lanes, 1,
        "nested-loop-join at {} planned more than one lane, so a drain reaches it too",
        MODES[3].name
    );
    let (as_planned, rebatched) = boundaries_under(
        &nested_loop,
        Injection {
            rebatch: Rebatch::AboveSources,
            ..Injection::NONE
        },
        (When::PreCall, "GpuNestedLoopJoin"),
    );
    assert_eq!(
        as_planned.1, rebatched.1,
        "the budget this query needs is its join's transient, which a rebatcher above the \
         sources does not reach"
    );
}
