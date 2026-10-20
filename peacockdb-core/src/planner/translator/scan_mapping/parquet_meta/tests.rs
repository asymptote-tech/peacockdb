use super::*;
use datafusion::arrow::datatypes::Schema as ArrowSchema;
use datafusion::execution::context::SessionContext;
use datafusion::physical_plan::ExecutionPlan;
use datafusion::prelude::ParquetReadOptions;
use std::path::PathBuf;
use std::sync::Arc;

fn minimal() -> PathBuf {
    crate::test_support::testdata_minimal_dir()
}

/// The scan under a query, which is what carries the projection and the pruning
/// predicate this file reads.
async fn scan_of(sql: &str) -> Arc<dyn ExecutionPlan> {
    let ctx = crate::register_tables_for(crate::build_session_state(1), &minimal())
        .await
        .expect("register the minimal tables");
    let plan = ctx
        .sql(sql)
        .await
        .expect("plan the query")
        .create_physical_plan()
        .await
        .expect("physical plan");
    find_scan(&plan).expect("a scan")
}

fn find_scan(plan: &Arc<dyn ExecutionPlan>) -> Option<Arc<dyn ExecutionPlan>> {
    if plan.as_any().is::<ParquetExec>() {
        return Some(plan.clone());
    }
    plan.children()
        .into_iter()
        .find_map(|child| find_scan(&child.clone()))
}

async fn read(sql: &str) -> ScanMetadata {
    let scan = scan_of(sql).await;
    survivor_metadata(scan.as_any().downcast_ref::<ParquetExec>().unwrap())
        .expect("read the metadata")
}

async fn survivors(sql: &str) -> Vec<RowGroupMeta> {
    read(sql).await.groups
}

/// The file's own numbers, read independently of the code under test.
fn chunk_bytes(table: &str, columns: &[usize]) -> u64 {
    let file = std::fs::File::open(minimal().join(format!("{table}.parquet"))).unwrap();
    let reader = SerializedFileReader::new(file).unwrap();
    reader
        .metadata()
        .row_groups()
        .iter()
        .flat_map(|group| columns.iter().map(|c| group.column(*c).uncompressed_size()))
        .sum::<i64>() as u64
}

#[tokio::test]
async fn bytes_are_the_column_chunk_totals_of_the_projected_columns() {
    // n_name is field 1 of the file schema, and its chunks are what a scan projecting
    // it reads — not a width derived from the type, which is the whole point of
    // measuring the file.
    let measured: u64 = survivors("SELECT n_name FROM nation")
        .await
        .iter()
        .map(|g| g.bytes)
        .sum();
    assert_eq!(measured, chunk_bytes("nation", &[1]));
}

#[tokio::test]
async fn a_narrower_projection_reads_fewer_bytes() {
    let one: u64 = survivors("SELECT n_name FROM nation")
        .await
        .iter()
        .map(|g| g.bytes)
        .sum();
    let all: u64 = survivors("SELECT * FROM nation")
        .await
        .iter()
        .map(|g| g.bytes)
        .sum();
    assert_eq!(all, chunk_bytes("nation", &[0, 1, 2, 3]));
    assert!(one < all, "{one} is not less than {all}");
}

#[tokio::test]
async fn rows_are_the_row_groups_own_counts() {
    let measured = survivors("SELECT * FROM customer").await;
    // customer is two row groups at this scale, the second one short.
    assert_eq!(measured.len(), 2);
    assert_eq!(measured.iter().map(|g| g.rows).sum::<u64>(), 150_000);
    assert_eq!(measured[0].index, 0);
    assert_eq!(measured[1].index, 1);
}

#[tokio::test]
async fn pruning_leaves_only_the_row_groups_that_survived_it() {
    // c_custkey rises with the row groups, so a predicate past the first group's
    // maximum leaves the second — and the index it keeps is the file's, not a position
    // in the survivor list.
    let kept = survivors("SELECT c_custkey FROM customer WHERE c_custkey > 140000").await;
    assert_eq!(kept.len(), 1, "{kept:?}");
    assert_eq!(kept[0].index, 1);
    assert_eq!(
        kept[0].bytes,
        chunk_bytes("customer", &[0]) - customer_group_zero_bytes()
    );
}

fn customer_group_zero_bytes() -> u64 {
    let file = std::fs::File::open(minimal().join("customer.parquet")).unwrap();
    let reader = SerializedFileReader::new(file).unwrap();
    reader.metadata().row_group(0).column(0).uncompressed_size() as u64
}

#[tokio::test]
async fn a_column_can_be_null_only_where_a_surviving_row_group_holds_one() {
    // tpch.minimal has no NULLs anywhere, so every column reads as not-nullable here
    // however it is declared — which is the point: the declaration says nothing.
    let scan = read("SELECT n_name, n_regionkey FROM nation").await;
    assert_eq!(scan.can_be_null, vec![false, false]);
    assert!(
        scan_of("SELECT n_name FROM nation")
            .await
            .schema()
            .field(0)
            .is_nullable(),
        "the column is declared nullable, which is exactly what cannot be trusted"
    );
}

#[tokio::test]
async fn a_scan_over_several_files_is_refused_rather_than_measured_from_one() {
    // The mapping addresses one file's row groups, so a second file would be sized
    // from the first's — a wrong answer where an error belongs.
    let dir = std::env::temp_dir().join(format!("peacockdb-multifile-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for name in ["nation-a.parquet", "nation-b.parquet"] {
        std::fs::copy(minimal().join("nation.parquet"), dir.join(name)).unwrap();
    }
    let ctx = SessionContext::new();
    ctx.register_parquet(
        "nations",
        dir.to_str().unwrap(),
        ParquetReadOptions::default(),
    )
    .await
    .expect("register the directory");
    let plan = ctx
        .sql("SELECT n_name FROM nations")
        .await
        .unwrap()
        .create_physical_plan()
        .await
        .unwrap();
    let scan = find_scan(&plan).expect("a scan");
    let err = survivor_metadata(scan.as_any().downcast_ref::<ParquetExec>().unwrap())
        .expect_err("a multi-file scan has no single mapping");
    assert!(
        matches!(&err, PlanError::Unsupported(what) if what.contains("2 files")),
        "{err}"
    );
    std::fs::remove_dir_all(&dir).unwrap();
}

/// nation registered under its own schema with `rename` applied to every field name, then
/// `sql` planned over it, the scan's metadata read.
async fn metadata_over(
    rename: impl Fn(&str) -> String,
    sql: &str,
) -> Result<ScanMetadata, PlanError> {
    let path = minimal().join("nation.parquet");
    let path = path.to_str().unwrap();
    let own = SessionContext::new();
    own.register_parquet("nation", path, ParquetReadOptions::default())
        .await
        .expect("the file's own schema");
    let file = own
        .table("nation")
        .await
        .expect("registered")
        .schema()
        .as_arrow()
        .clone();
    let declared = ArrowSchema::new(
        file.fields()
            .iter()
            .map(|f| f.as_ref().clone().with_name(rename(f.name())))
            .collect::<Vec<_>>(),
    );
    let ctx = SessionContext::new();
    ctx.register_parquet(
        "declared",
        path,
        ParquetReadOptions::default().schema(&declared),
    )
    .await
    .expect("registered under the declared schema");
    let plan = ctx
        .sql(sql)
        .await
        .expect("the query parses")
        .create_physical_plan()
        .await
        .expect("the query plans");
    let scan = find_scan(&plan).expect("a scan");
    survivor_metadata(scan.as_any().downcast_ref::<ParquetExec>().unwrap())
}

#[tokio::test]
async fn a_declared_schema_that_renames_a_read_column_is_refused_naming_both() {
    // The device reads n_name by the declared name and finds none; the cpu reads ordinal 1.
    let refused = metadata_over(
        |n| {
            if n == "n_name" {
                "nation_name".into()
            } else {
                n.into()
            }
        },
        "SELECT * FROM declared",
    )
    .await
    .expect_err("a renamed column is read by different names on the two engines");
    let message = refused.to_string();
    assert!(matches!(refused, PlanError::Unsupported(_)), "{message}");
    assert!(
        message.contains("ordinal 1")
            && message.contains("`nation_name`")
            && message.contains("`n_name`"),
        "{message}"
    );
}

#[tokio::test]
async fn a_declared_schema_that_swaps_two_columns_of_one_type_is_refused() {
    // Same type, so nothing downstream would notice: the device would read one key as the other.
    let refused = metadata_over(
        |n| match n {
            "n_nationkey" => "n_regionkey".into(),
            "n_regionkey" => "n_nationkey".into(),
            other => other.into(),
        },
        "SELECT * FROM declared",
    )
    .await
    .expect_err("a swap reads each key as the other on the device");
    let message = refused.to_string();
    assert!(
        message.contains("ordinal 0")
            && message.contains("`n_regionkey`")
            && message.contains("`n_nationkey`"),
        "{message}"
    );
}

#[tokio::test]
async fn a_rename_of_a_column_the_scan_does_not_read_plans() {
    // Only the projected ordinals are read, and n_comment is not one of them here.
    metadata_over(
        |n| {
            if n == "n_comment" {
                "remark".into()
            } else {
                n.into()
            }
        },
        "SELECT n_name FROM declared",
    )
    .await
    .expect("an unread column's name is nobody's to check");
}
