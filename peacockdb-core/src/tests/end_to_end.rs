//! SQL in, rows out: the engine end to end on the CPU backend, at the five modes and at
//! the injected shapes.
//!
//! Every other test of the engine proves one layer against a fixture of the last one's
//! shape. This one starts at a query's text and ends at its rows, so what it tests is the
//! join between the pieces — and the oracle is DataFusion on the same SQL rather than our
//! own legacy executor, which would agree with us wherever we are consistently wrong.
//!
//! Two axes vary: the five modes, which are plans the planner would have chosen, and the
//! injected shapes, which are ones it never would. `small_table_bytes` is constant across
//! both, so a join's co-partitioning is the knob neither turns.
//!
//! The children make the claims a result comparison cannot: `limits` and `dimensions` read
//! the calls off the trace, `accounting` the budget a run needs.

use datafusion::arrow::array::RecordBatch;
use datafusion::arrow::datatypes::DataType;

use crate::executor::run;
use crate::planner;

use super::injection::{
    CAP, Dimensions, Empties, Injected, InjectedContext, Injection, PlannedMode, Rebatch, SEED,
    apply, node_count, planned_mode, select,
};
use crate::test_support::MODES;
use crate::test_support::{
    assert_results_match, batches_to_sorted_str, data_dir_for, queries_dir_for,
};

mod accounting;
mod dimensions;
mod limits;

/// Where a Welford merge is the only divergence: the engine decomposes the aggregate into
/// an init, two merges and a finalize, and DataFusion computes it in one pass, so the last
/// digits differ by reassociation. The legacy GPU tier uses the same figure for the same
/// reason (`golden_approx_std`).
const WELFORD_TOLERANCE: f64 = 1e-11;

/// One query at every mode, against DataFusion on the same text.
/// Whether a query also runs the injected shapes. An enum rather than a flag, because
/// which of the two a call gets is the difference between five runs and up to thirty-five.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Coverage {
    ModesOnly,
    ModesAndInjection,
}

async fn answers_match_datafusion(
    dataset: &str,
    query: &str,
    tolerance: Option<f64>,
    coverage: Coverage,
) {
    let sql_path = queries_dir_for(dataset).join(format!("{query}.sql"));
    let sql = std::fs::read_to_string(&sql_path)
        .unwrap_or_else(|_| panic!("query file not found: {}", sql_path.display()));
    sql_answers_match_datafusion(dataset, query, &sql, tolerance, coverage).await;
}

async fn sql_answers_match_datafusion(
    dataset: &str,
    query: &str,
    sql: &str,
    tolerance: Option<f64>,
    coverage: Coverage,
) {
    let data_dir = data_dir_for(dataset, "1");

    let oracle_ctx = crate::register_tables_for(crate::build_session_state(1), &data_dir)
        .await
        .expect("register the tables");
    let expected = oracle_ctx
        .sql(sql)
        .await
        .expect("the oracle plans the query")
        .collect()
        .await
        .expect("the oracle runs the query");

    // Encoded once: `assert_results_match` renders both sides per call, so one oracle
    // against thirty runs is rendered thirty times, and on a million-row result that is
    // most of the tier. The tolerance path indexes rather than renders and keeps its own.
    let expected_rows = tolerance
        .is_none()
        .then(|| sorted_rows(&expected))
        .flatten();
    let mut planned = Vec::new();
    for mode in &MODES {
        let name = mode.name;
        let ctx = crate::register_tables_for(
            crate::build_session_state(mode.target_partitions),
            &data_dir,
        )
        .await
        .expect("register the tables");
        let plan = ctx
            .sql(sql)
            .await
            .expect("the query plans")
            .create_physical_plan()
            .await
            .expect("the query has a physical plan");
        let (tree, _memory) = planner::plan(&plan, mode.knobs())
            .unwrap_or_else(|error| panic!("{dataset}/{query} at {name}: {error}"));
        run_and_check(
            tree.as_ref(),
            &ctx.task_ctx(),
            Injection::NONE,
            &Oracle {
                batches: &expected,
                rows: expected_rows.clone(),
                tolerance,
            },
            &format!("{dataset}/{query} at {name}"),
        );
        planned.push((planned_mode(name, tree.as_ref()), tree, ctx));
    }
    if coverage == Coverage::ModesOnly {
        return;
    }

    // One oracle for every shape below, computed above: it is the expensive half and it is
    // invariant, which is what makes a layout that changes the answer a defect rather than
    // a disagreement between two shapes of ours.
    let modes: Vec<PlannedMode> = planned.iter().map(|(mode, _, _)| *mode).collect();
    // Beside the times, because what an injected run costs is the size of the answer it
    // has to compare rather than the rows it scanned.
    eprintln!(
        "[injection] {dataset}/{query} oracle rows={}",
        expected.iter().map(RecordBatch::num_rows).sum::<usize>()
    );
    for candidate in select(&modes, &Dimensions::default(), CAP, SEED) {
        let (_, tree, ctx) = &planned[candidate.mode];
        let injected = apply(tree.as_ref(), candidate.injection, SEED);
        let what = format!("{dataset}/{query} at {}", candidate.label(&modes));
        // A rebatcher that found no edge to take injects nothing, and the run would then
        // be the as-planned one under a label saying otherwise — the same shape as a
        // dimension whose carrier was quietly dropped.
        if candidate.injection.rebatch != Rebatch::None {
            assert!(
                node_count(injected.as_ref()) > node_count(tree.as_ref()),
                "{what}: the rebatcher found no edge to take"
            );
        }
        let started = std::time::Instant::now();
        run_and_check(
            injected.as_ref(),
            &ctx.task_ctx(),
            candidate.injection,
            &Oracle {
                batches: &expected,
                rows: expected_rows.clone(),
                tolerance,
            },
            &what,
        );
        // Swallowed by libtest unless the run is a failing one or `--nocapture` is passed,
        // which is how the timing table in llm-wiki/reports/ was taken.
        eprintln!("[injection] {what} {}us", started.elapsed().as_micros());
    }
}

/// The answer every shape of one query is measured against: the rows, and — where the
/// comparison is exact — their arrow row encoding, sorted, held rather than rebuilt per
/// run.
struct Oracle<'a> {
    batches: &'a [RecordBatch],
    rows: Option<Vec<Vec<u8>>>,
    tolerance: Option<f64>,
}

/// The columns an answer declares. Names are not an input to the row encoding, and the
/// rendered comparison it replaced carried them in its header — so they are asserted here
/// rather than dropped. The engine's own defect class is what makes it worth an assert: a
/// finalize project emitting the right values under the wrong names ([#163]), which the
/// oracle cannot get wrong the same way.
///
/// [#163]: ../../../llm-wiki/tickets.md#t163
fn columns_of(batches: &[RecordBatch]) -> Vec<(String, DataType)> {
    batches
        .first()
        .map(|batch| {
            batch
                .schema()
                .fields()
                .iter()
                .map(|field| (field.name().clone(), field.data_type().clone()))
                .collect()
        })
        .unwrap_or_default()
}

/// Every row as its arrow row encoding, sorted — a multiset of rows, compared in bytes.
///
/// Values and types only — a column's NAME is not an input to the encoding, so
/// [`columns_of`] is asserted beside this rather than folded into it. On values it is not
/// looser than the rendered text it replaced: arrow renders a float at round-trip
/// precision, so two values that render alike are the same bits. What it drops is the
/// rendering, which one oracle against thirty runs otherwise pays for thirty times —
/// `SELECT *` over a million rows renders for seconds. A mismatch falls back to the
/// rendered form, so the message still says how they differ.
fn sorted_rows(batches: &[RecordBatch]) -> Option<Vec<Vec<u8>>> {
    use datafusion::arrow::row::{RowConverter, SortField};

    let schema = batches.first()?.schema();
    let fields: Vec<SortField> = schema
        .fields()
        .iter()
        .map(|field| SortField::new(field.data_type().clone()))
        .collect();
    let converter = RowConverter::new(fields).ok()?;
    let mut rows = Vec::new();
    for batch in batches {
        let encoded = converter.convert_columns(batch.columns()).ok()?;
        rows.extend(encoded.iter().map(|row| row.as_ref().to_vec()));
    }
    rows.sort_unstable();
    Some(rows)
}

/// One run of one shape, against the one oracle. The two accounting assertions ride here
/// rather than at the call sites: an injected run leaks exactly as visibly as a planned
/// one, and a batch held and never released shows in neither's rows. The report comes
/// back for the cases that read the calls a correct answer was made with.
fn run_and_check(
    tree: &dyn crate::plan::GpuNode,
    task: &std::sync::Arc<datafusion::execution::TaskContext>,
    injection: Injection,
    oracle: &Oracle<'_>,
    what: &str,
) -> crate::executor::RunReport {
    let ctx = InjectedContext::new(task.clone(), injection, SEED);
    // The check the planner made, made again: an injected tree is one no planner emitted,
    // and the driver asks only for canonical form. Without this a rewrite that broke a
    // node's requirements would run and answer, which is the failure this whole tier is
    // about.
    crate::plan::validate(tree).unwrap_or_else(|error| panic!("{what} is not a plan: {error}"));
    let report =
        run::<Injected>(tree, &ctx, None).unwrap_or_else(|error| panic!("{what}: {error}"));
    let actual: Vec<RecordBatch> = report
        .batches
        .iter()
        .map(|batch| batch.record_batch().clone())
        .collect();
    assert_eq!(
        columns_of(&actual),
        columns_of(oracle.batches),
        "{what} answers different columns from the oracle"
    );
    match &oracle.rows {
        // The rendered comparison is what says *how* they differ, so a mismatch pays for it
        // once rather than every run paying in case one does.
        Some(rows) if sorted_rows(&actual).as_ref() != Some(rows) => assert_eq!(
            batches_to_sorted_str(&actual),
            batches_to_sorted_str(oracle.batches),
            "result for {what} differs from oracle (exact compare)"
        ),
        Some(_) => {}
        None => assert_results_match(oracle.batches, &actual, oracle.tolerance, what),
    }
    assert_eq!(report.in_flight_bytes, 0, "{what} ended holding batches");
    assert_eq!(
        report.holds, report.releases,
        "{what} held {} batches and released {}",
        report.holds, report.releases
    );
    // Counted where it happens rather than inferred from the answer, which is unchanged
    // either way: a seed under which no call fires would leave the setting carried in the
    // label and in nothing else.
    if injection.empties != Empties::Never {
        assert!(
            ctx.empty_batches() > 0,
            "{what}: no source call emitted an empty batch"
        );
    }
    report
}

/// `end_to_end!(dataset, query)` — one test per query, so a failure names it. Two optional
/// arguments follow: a tolerance, for a query whose answer is compared approximately, and
/// the coverage, which decides whether the query runs the five modes alone or the injected
/// shapes as well. `injected_queries!` is the only caller passing the second.
macro_rules! end_to_end {
    ($dataset:ident, $query:ident) => {
        end_to_end!($dataset, $query, None);
    };
    ($dataset:ident, $query:ident, $tolerance:expr) => {
        end_to_end!($dataset, $query, $tolerance, Coverage::ModesOnly);
    };
    ($dataset:ident, $query:ident, $tolerance:expr, $coverage:expr) => {
        paste::paste! {
            #[tokio::test]
            async fn [<$dataset _ $query>]() {
                answers_match_datafusion(
                    stringify!($dataset),
                    &stringify!($query).replace('_', "-"),
                    $tolerance,
                    $coverage,
                )
                .await;
            }
        }
    };
}

/// The injected set, and the tests for it, from one list — so a query leaves the set only
/// by leaving the list, which `the_injected_set_keeps_the_shapes_only_one_query_has` reads.
/// The two forms differ by one word otherwise, and turning one back is invisible.
macro_rules! injected_queries {
    ($($dataset:ident / $query:ident),+ $(,)?) => {
        /// The queries that run the injected shapes as well as the five modes. Four of
        /// them carry a shape no other query here has, and a trim that drops one of those
        /// cost coverage rather than runs: `tpcds/q33` is the only four-lane interleave,
        /// `tpch/nested_loop_join` and `tpch/nested_loop_left_join` are the two nested-loop
        /// forms, and `tpch/nested_limits` carries both row-interval lowerings.
        const INJECTED: &[&str] = &[$(concat!(
            stringify!($dataset),
            "/",
            stringify!($query)
        )),+];
        $(end_to_end!($dataset, $query, None, Coverage::ModesAndInjection);)+
    };
}

// ── the join capability matrix ──────────────────────────────────────────────
// Chosen by cover over the matrix rather than by taste, off the mode goldens: every join
// type the engine claims, crossed with a residual filter, null_equals_null and multi-key.
// Eleven of the seventeen carry a cell no other query here does.
//
// nested-loop Inner, and the smallest plan in the corpus that carries a join at all.
// injected: tpch/nested-loop-join
// nested-loop Left: the one mode whose probe side is a single batch, so its call takes the
// build side rather than a copy of it.
// injected: tpch/nested-loop-left-join
// RightAnti — the probe-side semi family, answered per batch with no finish pass.
// injected: tpch/anti-join
// Left outer: the finish pass, and the only cell with no device path at all (#152).
end_to_end!(tpch, left_join);
// Inner, multi-key Inner, Inner with a residual filter, LeftSemi and RightSemi in one plan.
end_to_end!(tpch, q20);
// LeftAnti with a residual filter, which the matrix answers in one legacy call over a
// probe side the planner made single-batch.
end_to_end!(tpch, q21);
// Full outer: Right's per-batch call and Left's finish, in one node.
// injected: tpcds/q97
// LeftAnti, and a LeftSemi carrying a residual filter.
// injected: tpcds/q16
// LeftMark, which scatters a boolean into an all-false column.
// injected: tpcds/q45
// LeftSemi with null_equals_null — a set operation lowered to a join, where NULL = NULL.
// injected: tpcds/q8
// RightSemi with null_equals_null.
end_to_end!(tpcds, q38);
// RightAnti with null_equals_null.
end_to_end!(tpcds, q87);
// Right outer, and its keys are composite — the only multi-key outer in either corpus.
// injected: tpcds/q93

// ── the shapes a join cover does not reach ──────────────────────────────────
// A union executed as an INTERLEAVE at four lanes: output lane p is built from lane p of
// each branch, which is the whole claim and is invisible at one lane.
// injected: tpcds/q33
// A union that cannot interleave: its two unions carry branches that disagree on lane
// count, which is what makes them unions rather than interleaves, and the lanes above are
// their sum. q77 is the shape this claim was written for — 4+1+4 — and it is not here
// yet: one of its Right outers gets a lane whose build side is empty, and its registry
// cells carry [#175](../../../llm-wiki/tickets.md#t175) until the goldens that prove the
// pad are read and moved with them.
// injected: tpcds/q2
// Both row-interval lowerings on one root-to-leaf path — the root-adjacent one becoming
// the unload's skip/fetch and the mid-plan one a limit over the scan — and the only
// OFFSETs in either corpus. It is also where the matrix's cross join is covered: what
// connects the two intervals has to be one, so the cell has no query of its own here.
// injected: tpch/nested-limits
// A merge over state worth merging: the Welford init, both merges and the finalize
// project. Every other aggregate here merges a sum.
end_to_end!(tpch, shuffle_stddev, Some(WELFORD_TOLERANCE));

// ── the injected set ────────────────────────────────────────────────────────

// Eleven queries, four of which carry a shape no other query here has — see INJECTED.
injected_queries!(
    tpch / nested_loop_join,
    tpch / nested_loop_left_join,
    tpch / anti_join,
    tpcds / q97,
    tpcds / q16,
    tpcds / q45,
    tpcds / q8,
    tpcds / q93,
    tpcds / q33,
    tpcds / q2,
    tpch / nested_limits
);

// ── the aggregate that stops aggregating ────────────────────────────────────

/// Groups nearly as many as rows, over more than 100,000 of them: the shape that makes
/// DataFusion's partial aggregate give up on grouping.
///
/// `AggregateExec` in Partial mode probes its own aggregation ratio and, where grouping is
/// not paying, passes its input through as state — sound in a DataFusion plan because a
/// Final stage regroups downstream, and wrong here, where the init emits state and the
/// merge is a Partial too. The duplicate keys reach the finalize and come out as extra
/// rows: 2,797,913 groups against 2,764,744 before the fix, and tpcds q97 counted them.
///
/// One key does not reach it — the ratio is far below the threshold — which is why this
/// case has two.
#[tokio::test]
async fn a_two_key_group_by_over_many_rows_does_not_emit_a_group_twice() {
    sql_answers_match_datafusion(
        "tpcds",
        "two-key group by",
        "SELECT count(*) FROM (SELECT ss_customer_sk, ss_item_sk FROM store_sales GROUP BY 1, 2)",
        None,
        Coverage::ModesOnly,
    )
    .await;
}

/// The hole the row encoding leaves, and the assert that covers it.
///
/// Two batches of the same values under different column names encode identically — names
/// are not an input to the encoding — so the rendered fallback, which does carry them in
/// its header, is never reached on a names-only divergence. Constructed rather than read
/// off a query: no query answers with the wrong names today, which is exactly why the
/// substitution could weaken this without anything going red.
#[test]
fn an_answer_under_the_wrong_column_names_is_not_the_same_answer() {
    use datafusion::arrow::array::{ArrayRef, Int64Array};

    let values: ArrayRef = std::sync::Arc::new(Int64Array::from(vec![1i64, 2, 3]));
    let expected = RecordBatch::try_from_iter_with_nullable([("sum(v)", values.clone(), true)])
        .expect("the oracle's answer");
    let renamed = RecordBatch::try_from_iter_with_nullable([("v", values, true)])
        .expect("the same values, misnamed");

    assert_eq!(
        sorted_rows(std::slice::from_ref(&expected)),
        sorted_rows(std::slice::from_ref(&renamed)),
        "the encoding is supposed to be blind to names, and this case exists because it is"
    );
    assert_ne!(
        columns_of(std::slice::from_ref(&expected)),
        columns_of(std::slice::from_ref(&renamed)),
        "the column check does not see a rename, so nothing does"
    );
}

/// The injected set is a list, and four of its entries are the only carriers of a shape.
///
/// `end_to_end!` and the injected form differ by one word, so a query leaving the set
/// leaves it silently: `tpcds_q33` would still pass, the tier's test count would not
/// move, and the four-lane interleave — the one operator whose correctness IS a lane
/// correspondence — would stop being injected at all. The list generates the fixtures, so
/// leaving the set means leaving the list, and this reads the list.
#[test]
fn the_injected_set_keeps_the_shapes_only_one_query_has() {
    for carrier in [
        "tpcds/q33",
        "tpch/nested_loop_join",
        "tpch/nested_loop_left_join",
        "tpch/nested_limits",
    ] {
        assert!(
            INJECTED.contains(&carrier),
            "{carrier} carries a shape no other injected query has: {INJECTED:?}"
        );
    }
    assert_eq!(
        INJECTED.len(),
        11,
        "the injected set is eleven queries: {INJECTED:?}"
    );
}
