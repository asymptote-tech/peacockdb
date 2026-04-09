//! Parameterized tests that canonize GPU execution plans for TPC-H queries.
//!
//! Each test reads a SQL file from `testdata/tpch-queries/<name>.sql`, plans it
//! against the SF-1 dataset, and asserts the plan matches the canonical file in
//! `testdata/plans.sf1/<name>.txt`.
//!
//! To generate or update canonical plans, run with:
//!   UPDATE_CANONICAL=1 cargo test -p peacockdb-core --test test_queries

use std::path::PathBuf;
use std::sync::Arc;

use datafusion::physical_plan::display::DisplayableExecutionPlan;
use datafusion::physical_plan::ExecutionPlan;

use peacockdb_core::{build_session_state, build_session_state_with_gpu_rule};
use peacockdb_core::register_tables_for;
use peacockdb_core::cpu_executor::strip_gpu_tree;

const TARGET_PARTITIONS: usize = 8;
// const GPU_MEMORY_BUDGET: usize = 2 * 1024 * 1024 * 1024; // 2 GiB

fn testdata_dir() -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../testdata/tpch.sf1");
    if !dir.exists() {
        let script = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../testdata/generate_testdata.sh");
        let status = std::process::Command::new("bash")
            .arg(&script)
            .status()
            .expect("failed to run generate_testdata.sh");
        assert!(status.success(), "generate_testdata.sh exited with {}", status);
    }
    dir
}

fn queries_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../testdata/tpch-queries")
}

fn queries_full_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../testdata/tpch-queries-full")
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

async fn compare_plans_with_query(name: &str, sql: &str) {
    let data_dir = testdata_dir();
    //build_session_state_with_gpu_rule
    let gpu_ctx = register_tables_for(build_session_state_with_gpu_rule(TARGET_PARTITIONS), &data_dir).await.unwrap();
    
    let gpu_plan = gpu_ctx.sql(sql).await.unwrap().create_physical_plan().await.unwrap();
    let actual = plan_str(&strip_gpu_tree(gpu_plan).unwrap());

    let df_ctx = register_tables_for(build_session_state(TARGET_PARTITIONS), &data_dir)
        .await
        .unwrap();
    let df_plan = df_ctx.sql(sql).await.unwrap().create_physical_plan().await.unwrap();
    let expected = plan_str(&df_plan);

    let out_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../testdata/compare").join(name);
    std::fs::create_dir_all(&out_dir).unwrap();
    std::fs::write(out_dir.join("result.txt"), format!("EXPECTED:\n{expected}\n\nGOT:\n{actual}")).unwrap();

    assert_eq!(actual, expected, "GPU-stripped plan does not match DataFusion plan for '{name}'");
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

async fn run_tpch_full_test(name: &str) {
    let data_dir = testdata_dir();
    let sql_path = queries_full_dir().join(format!("{name}.sql"));
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

macro_rules! tpch_full_test {
    ($func_name:ident, $query_name:literal) => {
        #[tokio::test]
        async fn $func_name() {
            run_tpch_full_test($query_name).await;
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

tpch_full_test!(tpch_q1,  "q1");
tpch_full_test!(tpch_q2,  "q2");
tpch_full_test!(tpch_q3,  "q3");
tpch_full_test!(tpch_q4,  "q4");
tpch_full_test!(tpch_q5,  "q5");
tpch_full_test!(tpch_q6,  "q6");
tpch_full_test!(tpch_q7,  "q7");
tpch_full_test!(tpch_q8,  "q8");
tpch_full_test!(tpch_q9,  "q9");
tpch_full_test!(tpch_q10, "q10");
tpch_full_test!(tpch_q11, "q11");
tpch_full_test!(tpch_q12, "q12");
tpch_full_test!(tpch_q13, "q13");
tpch_full_test!(tpch_q14, "q14");
tpch_full_test!(tpch_q15, "q15");
tpch_full_test!(tpch_q16, "q16");
tpch_full_test!(tpch_q17, "q17");
tpch_full_test!(tpch_q18, "q18");
tpch_full_test!(tpch_q19, "q19");
tpch_full_test!(tpch_q20, "q20");
tpch_full_test!(tpch_q21, "q21");
tpch_full_test!(tpch_q22, "q22");