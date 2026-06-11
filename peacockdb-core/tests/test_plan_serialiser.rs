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

// ---------------------------------------------------------------------------
// Focused synthetic round-trips for the join IR variants (I2 / Component-6).
//
// The corpus round-trip harness (test_query_plan*.rs) SILENTLY SKIPS a plan
// whose serialize/deserialize returns Err — that is exactly how LeftMark went
// untested for a while. These tests pin each new IR variant independently of
// which corpus queries happen to be enabled and independent of the skip path:
//   - a non-vacuous structural assertion that the variant is actually present
//     (so the test fails loudly if the planner stops emitting it), and
//   - the same `serialize → deserialize → serialize` bytes-equal oracle the
//     corpus harness uses.
// `serialize_query` itself `.expect()`s serialization, so an Err here fails the
// test rather than skipping it.
// ---------------------------------------------------------------------------

/// Walk the serialized PlanNode tree, collecting every node (root first).
fn collect_nodes<'a>(node: fb::PlanNode<'a>, out: &mut Vec<fb::PlanNode<'a>>) {
    use fb::PlanNodeKind as K;
    out.push(node);
    let single_input = match node.node_type() {
        K::GpuFilter => node.node_as_gpu_filter().unwrap().input(),
        K::GpuProject => node.node_as_gpu_project().unwrap().input(),
        K::GpuAggregate => node.node_as_gpu_aggregate().unwrap().input(),
        K::GpuSort => node.node_as_gpu_sort().unwrap().input(),
        K::GpuCoalesceBatches => node.node_as_gpu_coalesce_batches().unwrap().input(),
        K::GpuCoalescePartitions => node.node_as_gpu_coalesce_partitions().unwrap().input(),
        K::GpuRepartition => node.node_as_gpu_repartition().unwrap().input(),
        K::GpuSortPreservingMerge => node.node_as_gpu_sort_preserving_merge().unwrap().input(),
        K::GpuLimit => node.node_as_gpu_limit().unwrap().input(),
        K::GpuWindow => node.node_as_gpu_window().unwrap().input(),
        _ => None,
    };
    if let Some(child) = single_input {
        collect_nodes(child, out);
        return;
    }
    match node.node_type() {
        K::GpuHashJoin => {
            let j = node.node_as_gpu_hash_join().unwrap();
            if let Some(l) = j.left() {
                collect_nodes(l, out);
            }
            if let Some(r) = j.right() {
                collect_nodes(r, out);
            }
        }
        K::GpuCrossJoin => {
            let j = node.node_as_gpu_cross_join().unwrap();
            if let Some(l) = j.left() {
                collect_nodes(l, out);
            }
            if let Some(r) = j.right() {
                collect_nodes(r, out);
            }
        }
        K::GpuNestedLoopJoin => {
            let j = node.node_as_gpu_nested_loop_join().unwrap();
            if let Some(l) = j.left() {
                collect_nodes(l, out);
            }
            if let Some(r) = j.right() {
                collect_nodes(r, out);
            }
        }
        K::GpuUnion => {
            if let Some(inputs) = node.node_as_gpu_union().unwrap().inputs() {
                for i in inputs.iter() {
                    collect_nodes(i, out);
                }
            }
        }
        _ => {} // GpuScan and other leaves
    }
}

fn nodes_of(bytes: &[u8]) -> Vec<fb::PlanNode<'_>> {
    let plan = flatbuffers::root::<fb::GpuPlan>(bytes).expect("invalid FlatBuffer");
    let mut out = Vec::new();
    collect_nodes(plan.root().expect("root PlanNode present"), &mut out);
    out
}

/// The same bytes-equal oracle the corpus harness uses.
fn assert_roundtrip_bytes_equal(bytes: &[u8]) {
    let reconstructed = deserialize_plan(bytes).expect("deserialize failed");
    let reserialized = serialize_plan(&reconstructed).expect("re-serialize failed");
    assert_eq!(
        reserialized, bytes,
        "serialize → deserialize → serialize must be bytes-equal"
    );
}

#[tokio::test]
async fn test_roundtrip_gpu_cross_join() {
    // No join predicate → CrossJoinExec → GpuCrossJoin.
    let bytes = serialize_query("SELECT * FROM region, nation").await;
    let nodes = nodes_of(&bytes);
    assert!(
        nodes
            .iter()
            .any(|n| n.node_type() == fb::PlanNodeKind::GpuCrossJoin),
        "plan must contain a GpuCrossJoin node"
    );
    assert_roundtrip_bytes_equal(&bytes);
}

#[tokio::test]
async fn test_roundtrip_gpu_nested_loop_join() {
    // A non-equi predicate → NestedLoopJoinExec; selecting a strict subset of the
    // joined columns drives a projection embedded into the join node, so this
    // exercises both the filter and projection fields of the IR variant.
    let bytes = serialize_query(
        "SELECT b.n_name, b.n_comment \
         FROM region a, nation b \
         WHERE b.n_regionkey > a.r_regionkey",
    )
    .await;
    let nodes = nodes_of(&bytes);
    let nlj = nodes
        .iter()
        .find(|n| n.node_type() == fb::PlanNodeKind::GpuNestedLoopJoin)
        .expect("plan must contain a GpuNestedLoopJoin node")
        .node_as_gpu_nested_loop_join()
        .unwrap();
    assert!(nlj.filter().is_some(), "NLJ should carry its filter predicate");
    assert!(
        nlj.projection().map(|p| !p.is_empty()).unwrap_or(false),
        "NLJ should carry a non-empty output projection"
    );
    assert_roundtrip_bytes_equal(&bytes);
}

#[tokio::test]
async fn test_roundtrip_gpu_left_mark_join() {
    // EXISTS inside a disjunction can't be decorrelated to a plain semi-join, so
    // DataFusion emits a HashJoinExec with JoinType::LeftMark (one row per left
    // row + a boolean "mark"). This pins the LeftMark JoinType on the wire.
    let bytes = serialize_query(
        "SELECT n_name FROM nation n \
         WHERE EXISTS (SELECT 1 FROM region r WHERE r.r_regionkey = n.n_regionkey) \
            OR n.n_nationkey < 5",
    )
    .await;
    let nodes = nodes_of(&bytes);
    let has_left_mark = nodes
        .iter()
        .filter(|n| n.node_type() == fb::PlanNodeKind::GpuHashJoin)
        .any(|n| n.node_as_gpu_hash_join().unwrap().join_type() == fb::JoinType::LeftMark);
    assert!(
        has_left_mark,
        "plan must contain a GpuHashJoin with JoinType::LeftMark"
    );
    assert_roundtrip_bytes_equal(&bytes);
}
