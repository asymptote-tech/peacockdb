//! Parameterized tests that canonize GPU execution plans for TPC-DS queries.
//!
//! Each test reads a SQL file from `testdata/tpcds-queries/q<N>.sql`, plans it
//! against the SF-1 dataset, and compares the result against the canonical plan
//! stored in `testdata/plans-tpcds.sf1/q<N>.txt`.
//!
//! # Canonizing
//!
//! To write (or overwrite) canonical files from the current actual output, run with
//! `UPDATE_CANONICAL=1`:
//!
//!   UPDATE_CANONICAL=1 cargo test --test test_query_plan_tpcds
//!
//! Each canonical file contains the normalized physical plan for one TPC-DS query.
//! ParquetExec lines are normalized to `ParquetExec: table=<stem>` so the files are
//! path-independent.

use std::path::PathBuf;
use std::sync::Arc;

use datafusion::physical_plan::display::DisplayableExecutionPlan;
use datafusion::physical_plan::ExecutionPlan;

use peacockdb_core::build_session_state_with_gpu_rules;
use peacockdb_core::gpu_rule::{analyze_memory, row_width};
use peacockdb_core::plan_serializer;
use peacockdb_core::register_tables_for;

const TARGET_PARTITIONS: usize = 8;
const TEST_GPU_MEMORY_BUDGET: usize = 2 * 1024 * 1024 * 1024; // 2 GiB

fn testdata_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../testdata/tpcds.sf1")
}

fn queries_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../testdata/tpcds-queries")
}

fn canondata_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../testdata/plans-tpcds.sf1")
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

fn memory_str(plan: &Arc<dyn ExecutionPlan>) -> String {
    fn walk(plan: &Arc<dyn ExecutionPlan>, indent: usize, lines: &mut Vec<String>) {
        let mem = analyze_memory(plan);
        let rw = row_width(&plan.schema());
        lines.push(format!(
            "{}{}: row_width={}, subtree_max_row_bytes={}",
            " ".repeat(indent),
            plan.name(),
            rw,
            mem.subtree_max_row_bytes
        ));
        for child in plan.children() {
            walk(child, indent + 2, lines);
        }
    }
    let mut lines = Vec::new();
    walk(plan, 0, &mut lines);
    lines.join("\n")
}

fn assert_plan_matches_canonical_at(
    plan: &Arc<dyn ExecutionPlan>,
    name: &str,
    dir: &std::path::Path,
) {
    let canonical_path = dir.join(format!("{name}.txt"));
    let actual = format!("{}\n--- memory ---\n{}", plan_str(plan), memory_str(plan));

    if std::env::var("UPDATE_CANONICAL").is_ok() {
        std::fs::create_dir_all(dir).unwrap();
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
        Ok(bytes) => match plan_serializer::deserialize_plan(&bytes) {
            Ok(reconstructed) => {
                let roundtripped = plan_str(&reconstructed);
                assert_eq!(
                    roundtripped,
                    plan_str(plan),
                    "flatbuffer roundtrip mismatch for '{name}'"
                );
            }
            Err(e) if e.contains("not supported") || e.contains("unsupported") => {
                eprintln!("Skipping flatbuffer roundtrip for '{name}': {e}");
            }
            Err(e) => panic!("flatbuffer deserialization failed for '{name}': {e}"),
        },
        Err(e) if e.contains("unsupported") => {
            eprintln!("Skipping flatbuffer roundtrip for '{name}': {e}");
        }
        Err(e) => panic!("flatbuffer serialization failed for '{name}': {e}"),
    }
}

async fn compare_plans_with_query(name: &str, sql: &str) {
    let data_dir = testdata_dir();
    let gpu_ctx = register_tables_for(
        build_session_state_with_gpu_rules(TARGET_PARTITIONS, TEST_GPU_MEMORY_BUDGET),
        &data_dir,
    )
    .await
    .unwrap();

    let plan = gpu_ctx
        .sql(sql)
        .await
        .unwrap()
        .create_physical_plan()
        .await
        .unwrap();
    assert_plan_matches_canonical_at(&plan, name, &canondata_dir());
}

async fn run_query_test(name: &str) {
    let data_dir = testdata_dir();
    if !data_dir.exists() {
        panic!(
            "TPC-DS SF-1 dataset not found at {}. Run \
             testdata/generate_testdata.sh --bench tpcds first.",
            data_dir.display()
        );
    }

    let sql_path = queries_dir().join(format!("{name}.sql"));
    let sql = std::fs::read_to_string(&sql_path)
        .unwrap_or_else(|_| panic!("query file not found: {}", sql_path.display()));

    compare_plans_with_query(name, &sql).await;
}

macro_rules! query_plan_test {
    ($func_name:ident, $query_name:literal) => {
        #[tokio::test]
        async fn $func_name() {
            run_query_test($query_name).await;
        }
    };
}

query_plan_test!(tpcds_q1, "q1");
query_plan_test!(tpcds_q2, "q2");
query_plan_test!(tpcds_q3, "q3");
// TODO(plan_serializer): same Final-aggregate-input-schema bug as TPC-H q3/q5.
// query_plan_test!(tpcds_q4, "q4");
query_plan_test!(tpcds_q5, "q5");
// TODO(plan_serializer): same Final-aggregate-input-schema bug as TPC-H q3/q5
// (avg() validates against the wrong schema and rejects Utf8View columns).
// query_plan_test!(tpcds_q6, "q6");
query_plan_test!(tpcds_q7, "q7");
// q8: flatbuffer verifier rejects on deserialize with "Nested table depth
// limit reached". q8 is the deepest TPC-DS plan and exceeds the current
// max_depth bump. See issue #12 ("Plan-serializer bugs").
// query_plan_test!(tpcds_q8, "q8");
query_plan_test!(tpcds_q9, "q9");
query_plan_test!(tpcds_q10, "q10");
query_plan_test!(tpcds_q11, "q11");
query_plan_test!(tpcds_q12, "q12");
query_plan_test!(tpcds_q13, "q13");
query_plan_test!(tpcds_q14, "q14");
query_plan_test!(tpcds_q15, "q15");
query_plan_test!(tpcds_q16, "q16");
query_plan_test!(tpcds_q17, "q17");
query_plan_test!(tpcds_q18, "q18");
query_plan_test!(tpcds_q19, "q19");
query_plan_test!(tpcds_q20, "q20");
query_plan_test!(tpcds_q21, "q21");
// q22: AggregateExec roundtrip doesn't preserve __grouping_id virtual column
// for ROLLUP/GROUPING SETS in the Final stage. See issue #12.
// query_plan_test!(tpcds_q22, "q22");
query_plan_test!(tpcds_q23, "q23");
query_plan_test!(tpcds_q24, "q24");
query_plan_test!(tpcds_q25, "q25");
query_plan_test!(tpcds_q26, "q26");
// q27: DataFusion 45 SanityCheckPlan rejects the SortPreservingMergeExec ordering
// emitted for ROLLUP. Re-enable once upstream is fixed.
// query_plan_test!(tpcds_q27, "q27");
query_plan_test!(tpcds_q28, "q28");
query_plan_test!(tpcds_q29, "q29");
query_plan_test!(tpcds_q30, "q30");
query_plan_test!(tpcds_q31, "q31");
query_plan_test!(tpcds_q32, "q32");
query_plan_test!(tpcds_q33, "q33");
query_plan_test!(tpcds_q34, "q34");
query_plan_test!(tpcds_q35, "q35");
query_plan_test!(tpcds_q36, "q36");
query_plan_test!(tpcds_q37, "q37");
query_plan_test!(tpcds_q38, "q38");
query_plan_test!(tpcds_q39, "q39");
// TODO(plan_serializer): same Final-aggregate-input-schema bug; aggregate args
// validate against the wrong schema and reject Utf8View - Decimal128.
// query_plan_test!(tpcds_q40, "q40");
query_plan_test!(tpcds_q41, "q41");
query_plan_test!(tpcds_q42, "q42");
query_plan_test!(tpcds_q43, "q43");
query_plan_test!(tpcds_q44, "q44");
query_plan_test!(tpcds_q45, "q45");
query_plan_test!(tpcds_q46, "q46");
query_plan_test!(tpcds_q47, "q47");
query_plan_test!(tpcds_q48, "q48");
query_plan_test!(tpcds_q49, "q49");
query_plan_test!(tpcds_q50, "q50");
query_plan_test!(tpcds_q51, "q51");
query_plan_test!(tpcds_q52, "q52");
query_plan_test!(tpcds_q53, "q53");
query_plan_test!(tpcds_q54, "q54");
query_plan_test!(tpcds_q55, "q55");
query_plan_test!(tpcds_q56, "q56");
query_plan_test!(tpcds_q57, "q57");
query_plan_test!(tpcds_q58, "q58");
query_plan_test!(tpcds_q59, "q59");
query_plan_test!(tpcds_q60, "q60");
query_plan_test!(tpcds_q61, "q61");
query_plan_test!(tpcds_q62, "q62");
query_plan_test!(tpcds_q63, "q63");
// TODO(plan_serializer): same Final-aggregate-input-schema bug as q4/q6/q40
// (PhysicalExpr Column 'cr_reversed_charge'@3 doesn't exist in partial-output
// schema passed to AggregateExprBuilder). See issue #12.
// query_plan_test!(tpcds_q64, "q64");
query_plan_test!(tpcds_q65, "q65");
query_plan_test!(tpcds_q66, "q66");
query_plan_test!(tpcds_q67, "q67");
query_plan_test!(tpcds_q68, "q68");
query_plan_test!(tpcds_q69, "q69");
// q70: DataFusion 45 doesn't physical-plan the GROUPING() aggregate.
// query_plan_test!(tpcds_q70, "q70");
query_plan_test!(tpcds_q71, "q71");
// q72: DataFusion 45 type-coercion can't handle Date32 + Int64 arithmetic.
// query_plan_test!(tpcds_q72, "q72");
query_plan_test!(tpcds_q73, "q73");
query_plan_test!(tpcds_q74, "q74");
query_plan_test!(tpcds_q75, "q75");
query_plan_test!(tpcds_q76, "q76");
query_plan_test!(tpcds_q77, "q77");
query_plan_test!(tpcds_q78, "q78");
query_plan_test!(tpcds_q79, "q79");
query_plan_test!(tpcds_q80, "q80");
query_plan_test!(tpcds_q81, "q81");
query_plan_test!(tpcds_q82, "q82");
query_plan_test!(tpcds_q83, "q83");
query_plan_test!(tpcds_q84, "q84");
query_plan_test!(tpcds_q85, "q85");
// q86: DataFusion 45 doesn't physical-plan the GROUPING() aggregate.
// query_plan_test!(tpcds_q86, "q86");
// TODO(plan_serializer): ProjectionExec field-name mismatch on roundtrip.
// query_plan_test!(tpcds_q87, "q87");
query_plan_test!(tpcds_q88, "q88");
query_plan_test!(tpcds_q89, "q89");
query_plan_test!(tpcds_q90, "q90");
query_plan_test!(tpcds_q91, "q91");
query_plan_test!(tpcds_q92, "q92");
query_plan_test!(tpcds_q93, "q93");
query_plan_test!(tpcds_q94, "q94");
query_plan_test!(tpcds_q95, "q95");
query_plan_test!(tpcds_q96, "q96");
query_plan_test!(tpcds_q97, "q97");
query_plan_test!(tpcds_q98, "q98");
query_plan_test!(tpcds_q99, "q99");
