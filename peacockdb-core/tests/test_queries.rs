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

use peacockdb_core::create_context_with_tables;
use peacockdb_core::plan_serializer;

const TARGET_PARTITIONS: usize = 8;
const GPU_MEMORY_BUDGET: usize = 2 * 1024 * 1024 * 1024; // 2 GiB

fn testdata_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../testdata/tpch.sf1")
}

fn queries_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../testdata/tpch-queries")
}

fn plans_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../testdata/plans.sf1")
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

    let ctx = create_context_with_tables(&data_dir, TARGET_PARTITIONS, GPU_MEMORY_BUDGET)
        .await
        .unwrap();

    let plan = ctx
        .sql(sql.trim())
        .await
        .unwrap()
        .create_physical_plan()
        .await
        .unwrap();

    assert_plan_matches_canonical(&plan, name);
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
