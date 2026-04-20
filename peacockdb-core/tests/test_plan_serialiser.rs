use std::path::PathBuf;

use peacockdb_core::generated::gpu_plan_generated::peacock::plan as fb;
use peacockdb_core::plan_serializer::serialize_plan;

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
