use super::node_text::quoted;
use super::*;
use crate::batch_partitioned::partitioner::Batching;
use crate::batch_partitioned::translate::Translator;
use std::path::PathBuf;

async fn rendered(sql: &str, target_partitions: usize) -> String {
    let data = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../testdata/tpch.minimal");
    let ctx = crate::register_tables_for(crate::build_session_state(target_partitions), &data)
        .await
        .expect("register the minimal tables");
    let plan = ctx
        .sql(sql)
        .await
        .expect("plan the query")
        .create_physical_plan()
        .await
        .expect("physical plan");
    let tree = Translator::new(
        target_partitions,
        Batching::Sized {
            target_batch_bytes: 1 << 20,
        },
    )
    .translate(&plan)
    .expect("translate the plan");
    render_plan(tree.as_ref())
}

fn line_with<'a>(text: &'a str, node: &str) -> &'a str {
    text.lines()
        .find(|line| line.trim_start().starts_with(node))
        .unwrap_or_else(|| panic!("no {node} line in:\n{text}"))
}

#[tokio::test]
async fn every_column_reference_renders_name_at_ordinal() {
    let text = rendered("SELECT c_name FROM customer WHERE c_nationkey > 1", 1).await;
    // The ordinal is authoritative and the name is the declared schema's at that
    // position, so a name disagreeing with its ordinal is visible on the line.
    assert!(
        line_with(&text, "GpuFilter").contains("predicate=c_nationkey@1 > 1"),
        "{text}"
    );
    // The scan's projections carry the file ordinal each column came from.
    assert!(
        line_with(&text, "GpuLoadParquet").contains("projections=[c_name@1, c_nationkey@3]"),
        "{text}"
    );
}

#[tokio::test]
async fn the_layout_replaces_the_lane_count() {
    let text = rendered(
        "SELECT c_nationkey, count(*) FROM customer GROUP BY c_nationkey",
        4,
    )
    .await;
    // Lane count and batch layout on every node; a hash or an order only where one is
    // declared. Three of these four are properties a plan line has nowhere else to say.
    assert!(
        line_with(&text, "GpuLoadParquet").contains("lanes=4, batches=multiple"),
        "{text}"
    );
    assert!(
        line_with(&text, "GpuEmitPartitions")
            .contains("lanes=4, batches=multiple, hashed_on=[c_nationkey@0]"),
        "{text}"
    );
    assert!(
        line_with(&text, "GpuCoalesceAllBatches").contains("lanes=1, batches=single"),
        "{text}"
    );
}

#[tokio::test]
async fn every_node_carrying_a_fetch_prints_it() {
    let text = rendered(
        "SELECT c_name FROM customer ORDER BY c_name LIMIT 5 OFFSET 2",
        4,
    )
    .await;
    // A merge that turns 40 rows into 7 says so on its own line: the fetch rides the
    // merge, and a reader who found it only on the sort beneath would misread which
    // node truncates.
    assert!(line_with(&text, "GpuSort").contains("fetch=7"), "{text}");
    assert!(
        line_with(&text, "GpuMergeSortedPartitions").contains("fetch=7"),
        "{text}"
    );
    // The interval rides the boundary crossing, and prints there.
    assert!(
        line_with(&text, "GpuUnload").contains("skip=2, fetch=5"),
        "{text}"
    );
}

#[tokio::test]
async fn a_mid_plan_limit_prints_its_interval_on_its_own_node() {
    let text = rendered(
        "SELECT count(*) FROM (SELECT * FROM customer WHERE c_nationkey > 1 LIMIT 3) t",
        1,
    )
    .await;
    assert!(
        line_with(&text, "GpuLimit").contains("skip=0, fetch=3"),
        "{text}"
    );
}

#[test]
fn a_name_that_is_not_a_token_is_backquoted_and_an_ordinary_one_is_not() {
    assert_eq!(quoted("c_name"), "c_name");
    assert_eq!(
        quoted("sum(lineitem.l_quantity)"),
        "sum(lineitem.l_quantity)"
    );
    assert_eq!(quoted("order count"), "`order count`");
    assert_eq!(quoted("a,b"), "`a,b`");
    assert_eq!(quoted("x@1"), "`x@1`");
    assert_eq!(quoted("a`b"), "`a``b`");
    assert_eq!(quoted(""), "``");
}

#[tokio::test]
async fn a_spaced_column_name_prints_as_one_token_everywhere_it_appears() {
    // Both halves of `… as name` and the schema entry, which is where a reader
    // resolves an ordinal: unquoted, the name's own space reads as the separator.
    let text = rendered(
        "SELECT \"nat key\", count(*) FROM \
         (SELECT c_nationkey AS \"nat key\" FROM customer) t GROUP BY \"nat key\"",
        1,
    )
    .await;
    assert!(text.contains("as `nat key`"), "{text}");
    assert!(text.contains("schema=[`nat key`:"), "{text}");
    assert!(text.contains("group_by=[`nat key`@0]"), "{text}");
}

#[tokio::test]
async fn a_filter_that_projects_declares_and_prints_what_it_keeps() {
    // DataFusion's filter drops columns as well as rows. A node that declared its
    // child's schema here would emit an extra column and shift every ordinal above it.
    let text = rendered(
        "SELECT c_name FROM customer WHERE c_nationkey > 1 ORDER BY c_name",
        4,
    )
    .await;
    let filter = line_with(&text, "GpuFilter");
    assert!(filter.contains("projection=[c_name@0]"), "{text}");
    assert!(filter.contains("schema=[c_name:Utf8View]"), "{text}");
}

#[tokio::test]
async fn the_declared_schema_prints_name_and_type_per_column() {
    let text = rendered("SELECT c_acctbal FROM customer", 1).await;
    // Precision and scale stay: an explicit cast's target is unreadable without the
    // state column's declared scale beside it.
    assert!(
        line_with(&text, "GpuLoadParquet").contains("schema=[c_acctbal:Decimal128(15,2)]"),
        "{text}"
    );
}

#[tokio::test]
async fn a_source_prints_the_partitioners_mapping_verbatim() {
    let text = rendered("SELECT c_name FROM customer", 4).await;
    // Partitions outermost, batches within them, row groups innermost — and a lane
    // the mapping left empty renders as one, because it is one.
    assert!(
        line_with(&text, "GpuLoadParquet").contains("partition_groups=[[[0]],[[1]],[],[]]"),
        "{text}"
    );
}

#[tokio::test]
async fn node_names_carry_no_exec_suffix() {
    let text = rendered("SELECT c_name FROM customer ORDER BY c_name", 4).await;
    assert!(!text.contains("Exec"), "{text}");
    assert!(text.contains("GpuMergeSortedPartitions"), "{text}");
}

#[tokio::test]
async fn an_aggregate_prints_its_aggregators_and_its_final_list() {
    let text = rendered("SELECT stddev(c_acctbal) FROM customer", 1).await;
    let merge = line_with(&text, "GpuAggregateBatches");
    // merge_m2 returns its three state columns together, and the line spells them out.
    assert!(merge.contains("merge_m2("), "{text}");
    assert!(
        merge.contains("$count, stddev(customer.c_acctbal)$mean, stddev(customer.c_acctbal)$m2]"),
        "{text}"
    );
    assert!(merge.contains("final=[CASE WHEN"), "{text}");
    // The init node runs the three Welford aggregators over raw rows.
    assert!(
        line_with(&text, "GpuAggregate:").contains("m2(c_acctbal@0)"),
        "{text}"
    );
}

#[tokio::test]
async fn a_join_prints_its_keys_and_its_projection_by_name() {
    let text = rendered(
        "SELECT c.c_name, s.s_name FROM customer c JOIN supplier s ON c.c_nationkey = s.s_nationkey",
        4,
    )
    .await;
    let join = line_with(&text, "GpuHashJoin");
    assert!(
        join.contains("on=[(s_nationkey@1, c_nationkey@1)]"),
        "{text}"
    );
    // A projection is ordinals into the joined table, so it is named from both sides
    // rather than printed as bare positions.
    assert!(join.contains("projection=[s_name@0, c_name@2]"), "{text}");
}

#[tokio::test]
async fn a_join_filter_resolves_each_reference_onto_the_side_it_came_from() {
    // The filter's ordinals index a table of its own, which appears on no line. Both
    // sides carry their key at ordinal 0 here, so a side mix-up changes the text — the
    // case that caught the same slip when the validator went red at T4.
    let text = rendered(
        "SELECT * FROM nation n, region r WHERE n.n_nationkey < r.r_regionkey",
        1,
    )
    .await;
    let join = line_with(&text, "GpuNestedLoopJoin");
    // region is the build side — DataFusion put the smaller table there and flipped
    // the predicate — and both sides' ordinal 0 is a different column, which is what
    // makes a mix-up visible.
    assert!(
        join.contains("filter=r_regionkey@build:0 > n_nationkey@probe:0"),
        "{text}"
    );
}
