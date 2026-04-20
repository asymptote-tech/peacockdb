//! Parameterized tests that canonize GPU execution plans for TPC-H queries.
//!
//! Each test reads a SQL file from `testdata/tpch-queries/<name>.sql`, plans it
//! against the SF-1 dataset, strips GPU wrappers, and compares the result against
//! the canonical plan stored in `tests/canondata/<name>.txt`.
//!
//! # Canonizing
//!
//! To write (or overwrite) canonical files from the current actual output, run with
//! `CANONIZE=1` or pass `-Z` to the test binary:
//!
//!   CANONIZE=1 cargo test --test test_queries
//!
//! Each canonical file contains the normalized, GPU-stripped physical plan for one
//! TPC-H query. ParquetExec lines are normalized to `ParquetExec: table=<stem>` so
//! the files are path-independent.

use std::path::PathBuf;
use std::sync::Arc;

use datafusion::arrow::array::Int64Array;
use datafusion::execution::context::SessionContext;
use datafusion::physical_plan::display::DisplayableExecutionPlan;
use datafusion::physical_plan::ExecutionPlan;

use peacockdb_core::build_session_state_with_gpu_rules;
use peacockdb_core::register_tables_for;
use peacockdb_core::cpu_executor::strip_gpu_tree;
use peacockdb_core::create_context_with_tables;
use peacockdb_core::gpu_rule::GpuScanExec;
use peacockdb_core::plan_serializer;
use peacockdb_core::CpuExecutor;

const TARGET_PARTITIONS: usize = 8;
const TEST_TARGET_PARTITIONS: usize = 8;
const TEST_GPU_MEMORY_BUDGET: usize = 2 * 1024 * 1024 * 1024; // 2 GiB

fn testdata_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../testdata/tpch.sf1")
}

fn testdata_minimal_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../testdata/tpch.minimal")
}

fn queries_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../testdata/tpch-queries")
}

fn canondata_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../testdata/plans.sf1")
}

fn plans_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../testdata/plans")
}

fn test_ctx(data_dir: &std::path::Path) -> impl std::future::Future<Output = datafusion::error::Result<SessionContext>> + '_ {
    create_context_with_tables(data_dir, TEST_TARGET_PARTITIONS, TEST_GPU_MEMORY_BUDGET)
}

async fn count(ctx: &SessionContext, query: &str) -> i64 {
    let batches = ctx.sql(query).await.unwrap().collect().await.unwrap();
    batches[0]
        .column(0)
        .as_any()
        .downcast_ref::<Int64Array>()
        .unwrap()
        .value(0)
}

/// Activated by:
/// - setting the `UPDATE_CANONICAL` environment variable (any value), e.g. `UPDATE_CANONICAL=1 cargo test`
fn is_canonize_mode() -> bool {
    std::env::var("UPDATE_CANONICAL").is_ok()
}

/// Render the plan to a string, normalizing ParquetExec lines to be path-independent.
fn plan_str(plan: &Arc<dyn ExecutionPlan>) -> String {
    let raw = DisplayableExecutionPlan::new(plan.as_ref())
        .indent(false)
        .to_string();
    raw.lines()
        .filter(|l| !l.is_empty())
        .map(|line| {
            if line.trim_start().starts_with("ParquetExec:") {
                let indent = line.len() - line.trim_start().len();
                let table = line
                    .find(".parquet")
                    .and_then(|end| line[..end].rfind('/').map(|sep| &line[sep + 1..end]))
                    .unwrap_or("unknown");
                format!("{}ParquetExec: table={table}", &line[..indent])
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn assert_plan_matches_canonical(plan: &Arc<dyn ExecutionPlan>, name: &str) {
    let canonical_path = plans_dir().join(format!("{name}.txt"));
    let actual = plan_str(plan);

    if std::env::var("UPDATE_CANONICAL").is_ok() {
        std::fs::create_dir_all(plans_dir()).unwrap();
        std::fs::write(&canonical_path, &actual).unwrap();
        eprintln!("Updated canonical plan: {}", canonical_path.display());
        return;
    }

    let canonical = std::fs::read_to_string(&canonical_path).unwrap_or_else(|_| {
        panic!(
            "canonical file not found: {}\nRun with UPDATE_CANONICAL=1 to generate it.",
            canonical_path.display()
        )
    });
    assert_eq!(
        actual,
        canonical.trim_end(),
        "plan for '{name}' does not match {}",
        canonical_path.display()
    );

    // Flatbuffer roundtrip — skip if the plan contains unsupported nodes/expressions.
    match plan_serializer::serialize_plan(plan) {
        Ok(bytes) => {
            match plan_serializer::deserialize_plan(&bytes) {
                Ok(reconstructed) => {
                    let roundtripped = plan_str(&reconstructed);
                    assert_eq!(
                        roundtripped, actual,
                        "flatbuffer roundtrip mismatch for '{name}'"
                    );
                }
                Err(e) if e.contains("not supported") || e.contains("unsupported") => {
                    eprintln!("Skipping flatbuffer roundtrip for '{name}': {e}");
                }
                Err(e) => panic!("flatbuffer deserialization failed for '{name}': {e}"),
            }
        }
        Err(e) if e.contains("unsupported") => {
            eprintln!("Skipping flatbuffer roundtrip for '{name}': {e}");
        }
        Err(e) => panic!("flatbuffer serialization failed for '{name}': {e}"),
    }
}

async fn compare_plans_with_query(name: &str, sql: &str) {
    let data_dir = testdata_dir();
    let gpu_ctx = register_tables_for(build_session_state_with_gpu_rules(TARGET_PARTITIONS, TEST_GPU_MEMORY_BUDGET), &data_dir)
        .await
        .unwrap();

    let gpu_plan = gpu_ctx.sql(sql).await.unwrap().create_physical_plan().await.unwrap();
    let actual = plan_str(&strip_gpu_tree(gpu_plan).unwrap());

    let canon_path = canondata_dir().join(format!("{name}.txt"));

    if is_canonize_mode() {
        std::fs::create_dir_all(canondata_dir()).unwrap();
        std::fs::write(&canon_path, &actual).unwrap();
        println!("canonized: {}", canon_path.display());
        return;
    }

    let expected = std::fs::read_to_string(&canon_path)
        .unwrap_or_else(|_| panic!(
            "canonical file not found: {}. Run with UPDATE_CANONICAL=1 to create it.",
            canon_path.display()
        ));
    let expected = expected.trim_end().to_string();

    assert_eq!(actual, expected, "GPU-stripped plan does not match canonical for '{name}'");
}

async fn run_query_test(name: &str) {
    let data_dir = testdata_dir();
    if !data_dir.exists() {
        panic!(
            "SF-1 dataset not found at {}. Run testdata/generate_testdata.sh first.",
            data_dir.display()
        );
    }

    let sql_path = queries_dir().join(format!("{name}.sql"));
    let sql = std::fs::read_to_string(&sql_path)
        .unwrap_or_else(|_| panic!("query file not found: {}", sql_path.display()));

    compare_plans_with_query(name, &sql).await;
}

macro_rules! query_test {
    ($func_name:ident, $query_name:literal) => {
        #[tokio::test]
        async fn $func_name() {
            run_query_test($query_name).await;
        }
    };
}

query_test!(test_scan_limit, "scan-limit");
query_test!(test_filter_project, "filter-project");
query_test!(test_aggregate_groupby, "aggregate-groupby");
query_test!(test_hash_join, "hash-join");
query_test!(test_left_join, "left-join");
query_test!(test_semi_join, "semi-join");
query_test!(test_anti_join, "anti-join");
query_test!(test_nested_loop_join, "nested-loop-join");
query_test!(test_mixed_join, "mixed-join");
query_test!(test_cross_join, "cross-join");


query_test!(tpch_q1,  "q1");
query_test!(tpch_q2,  "q2");
query_test!(tpch_q3,  "q3");
query_test!(tpch_q4,  "q4");
query_test!(tpch_q5,  "q5");
query_test!(tpch_q6,  "q6");
query_test!(tpch_q7,  "q7");
query_test!(tpch_q8,  "q8");
query_test!(tpch_q9,  "q9");
query_test!(tpch_q10, "q10");
query_test!(tpch_q11, "q11");
query_test!(tpch_q12, "q12");
query_test!(tpch_q13, "q13");
query_test!(tpch_q14, "q14");
// query_test!(tpch_q15, "q15");
query_test!(tpch_q16, "q16");
query_test!(tpch_q17, "q17");
query_test!(tpch_q18, "q18");
query_test!(tpch_q19, "q19");
query_test!(tpch_q20, "q20");
query_test!(tpch_q21, "q21");
query_test!(tpch_q22, "q22");

// ── CpuExecutor integration tests ────────────────────────────────────────

/// Full end-to-end example showing the idiomatic usage:
///   1. CpuExecutor::new  — builds a SessionContext with GPU rules
///   2. exec.execute(sql) — SQL → GPU plan → CPU execution → RecordBatches
#[tokio::test]
async fn test_cpu_executor_simple_query() {
    let exec = CpuExecutor::new(&testdata_minimal_dir(), 1, 2 * 1024 * 1024 * 1024)
        .await
        .unwrap();

    let batches = exec
        .execute("SELECT count(*) FROM nation WHERE n_regionkey >= 0")
        .await
        .unwrap();

    let count = batches[0]
        .column(0)
        .as_any()
        .downcast_ref::<Int64Array>()
        .unwrap()
        .value(0);

    assert_eq!(count, 25);
}

/// execute_instrumented returns both results and per-node stats in one call.
#[tokio::test]
async fn test_cpu_executor_instrumented() {
    let exec = CpuExecutor::new(&testdata_minimal_dir(), 1, 2 * 1024 * 1024 * 1024)
        .await
        .unwrap();

    let (batches, stats) = exec
        .execute_instrumented("SELECT count(*) FROM nation WHERE n_regionkey >= 0")
        .await
        .unwrap();

    let count = batches[0]
        .column(0)
        .as_any()
        .downcast_ref::<Int64Array>()
        .unwrap()
        .value(0);
    assert_eq!(count, 25);

    // Every stat entry must name a CPU node, never a GPU wrapper.
    for s in &stats {
        assert!(
            !s.node_name.starts_with("Gpu"),
            "GPU node '{}' leaked into stats",
            s.node_name
        );
    }
    assert!(!stats.is_empty());
}

// ── Basic correctness ────────────────────────────────────────────────────

#[tokio::test]
async fn test_nation_row_count() {
    let ctx = test_ctx(&testdata_minimal_dir()).await.unwrap();
    assert_eq!(count(&ctx, "SELECT count(*) FROM nation").await, 25);
}

#[tokio::test]
async fn test_region_nation_join() {
    let ctx = test_ctx(&testdata_minimal_dir()).await.unwrap();
    let n = count(
        &ctx,
        "SELECT count(*) FROM nation JOIN region ON nation.n_regionkey = region.r_regionkey",
    )
    .await;
    assert_eq!(n, 25);
}

// ── GPU plan node tests ──────────────────────────────────────────────────

/// Filter + aggregate: SELECT count(*) FROM customer WHERE c_acctbal > 0
/// Expected GPU nodes: GpuAggregateExec (partial + final), GpuFilterExec
#[tokio::test]
async fn test_gpu_nodes_filter_agg() {
    let ctx = test_ctx(&testdata_minimal_dir()).await.unwrap();
    let query = "SELECT count(*) FROM customer WHERE c_acctbal > 0";

    let plan = ctx.sql(query).await.unwrap().create_physical_plan().await.unwrap();
    assert_plan_matches_canonical(&plan, "filter_agg");

    let n = count(&ctx, query).await;
    assert!(n > 0 && n <= 150_000, "unexpected count {n}");
}

/// Hash join + sort: nations joined with their region, sorted by name.
/// Expected GPU nodes: GpuSortExec, GpuHashJoinExec
#[tokio::test]
async fn test_gpu_nodes_join_sort() {
    let ctx = test_ctx(&testdata_minimal_dir()).await.unwrap();
    let query = "
        SELECT n.n_name, r.r_name
        FROM nation n JOIN region r ON n.n_regionkey = r.r_regionkey
        ORDER BY n.n_name";

    let plan = ctx.sql(query).await.unwrap().create_physical_plan().await.unwrap();
    assert_plan_matches_canonical(&plan, "join_sort");

    // Result: 25 rows (every nation has exactly one region)
    let batches = ctx.sql(query).await.unwrap().collect().await.unwrap();
    let total_rows: usize = batches.iter().map(|b| b.num_rows()).sum();
    assert_eq!(total_rows, 25);
}

/// Group by + join + sort: nations per region, sorted descending by count.
/// Expected GPU nodes: GpuSortExec, GpuAggregateExec, GpuHashJoinExec
#[tokio::test]
async fn test_gpu_nodes_group_join_sort() {
    let ctx = test_ctx(&testdata_minimal_dir()).await.unwrap();
    let query = "
        SELECT r.r_name, count(*) AS nation_count
        FROM nation n JOIN region r ON n.n_regionkey = r.r_regionkey
        GROUP BY r.r_name
        ORDER BY nation_count DESC, r.r_name";

    let plan = ctx.sql(query).await.unwrap().create_physical_plan().await.unwrap();
    assert_plan_matches_canonical(&plan, "group_join_sort");

    // Result: 5 regions, each with exactly 5 nations.
    let batches = ctx.sql(query).await.unwrap().collect().await.unwrap();
    let total_rows: usize = batches.iter().map(|b| b.num_rows()).sum();
    assert_eq!(total_rows, 5);
    let counts = batches[0].column(1).as_any().downcast_ref::<Int64Array>().unwrap();
    for i in 0..counts.len() {
        assert_eq!(counts.value(i), 5, "region {} has {} nations, expected 5", i, counts.value(i));
    }
}

// ── Memory budget tests ──────────────────────────────────────────────────

/// Find all GpuScanExec nodes and return their batch sizes.
fn scan_batch_sizes(plan: &Arc<dyn ExecutionPlan>) -> Vec<usize> {
    let mut sizes = Vec::new();
    if let Some(scan) = plan.as_any().downcast_ref::<GpuScanExec>() {
        sizes.push(scan.gpu_batch_size);
    }
    for child in plan.children() {
        sizes.extend(scan_batch_sizes(child));
    }
    sizes
}

/// With a tight GPU memory budget, the batch size should be reduced below
/// the default 8192. Results must still be correct.
#[tokio::test]
async fn test_memory_budget_reduces_batch_size() {
    // 10 KiB budget → should force a very small batch size.
    let ctx =
        create_context_with_tables(&testdata_minimal_dir(), TEST_TARGET_PARTITIONS, 10 * 1024).await.unwrap();
    let query = "
        SELECT n.n_name, r.r_name
        FROM nation n JOIN region r ON n.n_regionkey = r.r_regionkey
        ORDER BY n.n_name";

    let plan = ctx.sql(query).await.unwrap().create_physical_plan().await.unwrap();
    let sizes = scan_batch_sizes(&plan);
    assert!(!sizes.is_empty(), "expected GpuScanExec nodes in plan");
    for &bs in &sizes {
        assert!(bs < 8192, "expected batch_size < 8192 with 10KiB budget, got {bs}");
        assert!(bs >= 1, "batch_size must be at least 1");
    }

    // Results must still be correct despite smaller batches.
    let batches = ctx.sql(query).await.unwrap().collect().await.unwrap();
    let total_rows: usize = batches.iter().map(|b| b.num_rows()).sum();
    assert_eq!(total_rows, 25);
}
