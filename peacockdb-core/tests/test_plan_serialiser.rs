use std::path::PathBuf;

use peacockdb_core::generated::gpu_plan_generated::peacock::plan as fb;
use peacockdb_core::plan_serializer::{deserialize_plan, serialize_plan};

fn assert_valid_flatbuffer(bytes: &[u8]) {
    let plan = flatbuffers::root::<fb::GpuPlan>(bytes).expect("invalid FlatBuffer");
    assert!(plan.root().is_some(), "root PlanNode should be present");
}

async fn serialize_query(sql: &str) -> Vec<u8> {
    let data_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../testdata/tpch.minimal");
    let ctx = peacockdb_core::create_context_with_tables(&data_dir, 1, 2 * 1024 * 1024 * 1024)
        .await
        .unwrap();
    let plan = ctx
        .sql(sql)
        .await
        .unwrap()
        .create_physical_plan()
        .await
        .unwrap();
    serialize_plan(&plan).expect("serialization failed")
}

#[tokio::test]
async fn test_serialize_filter_agg() {
    let bytes = serialize_query("SELECT count(*) FROM customer WHERE c_acctbal > 0").await;
    assert_valid_flatbuffer(&bytes);

    let plan = flatbuffers::root::<fb::GpuPlan>(&bytes).unwrap();
    let root = plan.root().unwrap();
    assert_eq!(root.node_type(), fb::PlanNodeKind::GpuAggregate);
}

#[tokio::test]
async fn test_serialize_join_sort() {
    let bytes = serialize_query(
        "SELECT n.n_name, r.r_name \
         FROM nation n JOIN region r ON n.n_regionkey = r.r_regionkey \
         ORDER BY n.n_name",
    )
    .await;
    assert_valid_flatbuffer(&bytes);

    let plan = flatbuffers::root::<fb::GpuPlan>(&bytes).unwrap();
    let root = plan.root().unwrap();
    assert_eq!(root.node_type(), fb::PlanNodeKind::GpuSort);
}

/// Fast regression guard for the IR round-trip's bytes-equal oracle, focused on
/// decimal precision/scale (the field that the plan-test harness historically
/// couldn't see, since `plan_str`/DisplayAs doesn't print schema types).
///
/// `c_acctbal` is `Decimal128(15, 2)`, so the product is a `Decimal128(p, s)`
/// output field. If the wire `Field` (or a scalar fn's return type) drops the
/// scale, deserialize defaults to `Decimal128(38, 10)` and the re-serialized IR
/// diverges. The oracle here is `serialize → deserialize → serialize` is
/// bytes-equal, matching the plan-test harness.
#[tokio::test]
async fn test_roundtrip_decimal_field_bytes_equal() {
    use datafusion::arrow::datatypes::DataType;

    let data_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../testdata/tpch.minimal");
    let ctx = peacockdb_core::create_context_with_tables(&data_dir, 1, 2 * 1024 * 1024 * 1024)
        .await
        .unwrap();
    let plan = ctx
        .sql("SELECT c_acctbal * c_acctbal AS sq FROM customer")
        .await
        .unwrap()
        .create_physical_plan()
        .await
        .unwrap();

    // The plan must actually exercise a decimal field, else the guard is vacuous.
    let orig_types: Vec<DataType> = plan
        .schema()
        .fields()
        .iter()
        .map(|f| f.data_type().clone())
        .collect();
    assert!(
        orig_types.iter().any(|t| matches!(t, DataType::Decimal128(_, _))),
        "test query should produce a Decimal128 output field, got {orig_types:?}"
    );

    let bytes = serialize_plan(&plan).expect("serialize failed");
    let reconstructed = deserialize_plan(&bytes).expect("deserialize failed");

    // Strong, targeted check: the exact decimal precision/scale survives.
    let recon_types: Vec<DataType> = reconstructed
        .schema()
        .fields()
        .iter()
        .map(|f| f.data_type().clone())
        .collect();
    assert_eq!(
        recon_types, orig_types,
        "reconstructed schema types (incl. decimal precision/scale) must match the original"
    );

    // Bytes-equal oracle (same as the plan-test round-trip harness).
    let reserialized = serialize_plan(&reconstructed).expect("re-serialize failed");
    assert_eq!(
        reserialized, bytes,
        "serialize → deserialize → serialize must be bytes-equal"
    );
}

#[tokio::test]
async fn test_serialize_group_join_sort() {
    let bytes = serialize_query(
        "SELECT r.r_name, count(*) AS nation_count \
         FROM nation n JOIN region r ON n.n_regionkey = r.r_regionkey \
         GROUP BY r.r_name \
         ORDER BY nation_count DESC, r.r_name",
    )
    .await;
    assert_valid_flatbuffer(&bytes);
}
