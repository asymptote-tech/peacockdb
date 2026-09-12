use super::node_text::quoted;
use super::*;
use crate::plan::Batching;
use crate::plan::RowInterval;
use crate::plan::{GpuLimit, GpuMergePartitions, GpuUnload};
use crate::planner::translate;
use crate::wire::{Payloads, attach_recipes, render_plan_recipes};

async fn rendered(sql: &str, target_partitions: usize) -> String {
    render_plan(planned(sql, target_partitions).await.as_ref())
}

async fn planned(sql: &str, target_partitions: usize) -> Box<dyn GpuNode> {
    let data = crate::test_support::testdata_minimal_dir();
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
    translate(
        target_partitions,
        Batching::Sized {
            target_batch_bytes: 1 << 20,
        },
        &plan,
    )
    .expect("translate the plan")
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

// ── the declared section of the payload golden ──────────────────────────────

/// One line per call under its node, in the recipe section's order. Three shapes a reader
/// must tell apart: a declared schema, a call no arm has declared (`undeclared`, spelled
/// out so an absent line cannot pass for one), and a node that makes no call at all.
#[test]
fn the_declared_section_prints_one_line_per_call_under_its_node() {
    let tree = GpuUnload::new(
        Box::new(GpuLimit::new(
            Box::new(GpuMergePartitions::new(crate::tests::rebuild::source(None))),
            RowInterval {
                skip: 1,
                fetch: Some(2),
            },
        )),
        None,
    );
    let plan = attach_recipes(&tree).expect("writable");
    assert_eq!(
        render_declared_schemas(&tree, &plan),
        "GpuUnload:\n\
         \x20 result_from_handle: schema=[k:Int64, v:Int64]\n\
         \x20 GpuLimit:\n\
         \x20   slice_handle: undeclared\n\
         \x20   GpuMergePartitions: no calls\n\
         \x20     GpuLoadParquet:\n\
         \x20       #0 CudfScan: schema=[k:Int64, v:Int64]\n"
    );
}

/// Through `plan_text`'s renderer and not the wire's: the same name in `wire/fb_text.rs`
/// prints a bare `Decimal128`, and the digits are what a declaration is for.
#[tokio::test]
async fn the_declared_section_keeps_a_decimals_precision_and_scale() {
    let tree = planned("SELECT c_acctbal FROM customer", 1).await;
    let plan = attach_recipes(tree.as_ref()).expect("writable");
    let text = render_declared_schemas(tree.as_ref(), &plan);
    assert!(
        text.contains("result_from_handle: schema=[c_acctbal:Decimal128(15,2)]"),
        "{text}"
    );
}

/// Both payload sections walk the tree post-order and index the recipe plan by position.
/// Nothing in the types ties the two walks together, so a kind whose `children()` order
/// moved would give the two sections different plans. Compared per node: its line, with its
/// indentation, and the `#seq Kind` labels under it — the seqs are what would move if
/// section B took a node's position before or after recursing where section A does not.
#[tokio::test]
async fn the_two_payload_sections_number_the_same_nodes() {
    for sql in [
        "SELECT c_acctbal FROM customer ORDER BY c_acctbal LIMIT 3",
        "SELECT n_name, count(*) FROM nation GROUP BY n_name",
        "SELECT n_name FROM nation JOIN region ON n_regionkey = r_regionkey",
    ] {
        let tree = planned(sql, 4).await;
        let plan = attach_recipes(tree.as_ref()).expect("writable");
        let recipes = render_plan_recipes(tree.as_ref(), &plan, Payloads::Omitted);
        let declared = render_declared_schemas(tree.as_ref(), &plan);
        assert_eq!(
            nodes_with_seqs(&recipes),
            nodes_with_seqs(&declared),
            "{sql}"
        );
        assert!(
            nodes_with_seqs(&recipes)
                .iter()
                .any(|(_, seqs)| !seqs.is_empty()),
            "{sql}: no seq label was read, so nothing was compared"
        );
    }

    /// Each node line cut at its `:`, with every `#seq Kind` label that follows it before
    /// the next node line — section A has them inside `execute_node(…)`, section B one per
    /// line beneath the node.
    fn nodes_with_seqs(text: &str) -> Vec<(String, Vec<String>)> {
        let mut nodes: Vec<(String, Vec<String>)> = Vec::new();
        for line in text.lines() {
            if line.trim_start().starts_with("Gpu") {
                let node = line.split(':').next().expect("a node line").to_string();
                nodes.push((node, Vec::new()));
            }
            let Some((_, seqs)) = nodes.last_mut() else {
                continue;
            };
            let mut rest = line;
            while let Some(at) = rest.find('#') {
                let label = &rest[at..];
                let end = label
                    .find(|c: char| c == ',' || c == ')' || c == ':')
                    .map_or(label.len(), |end| {
                        // A brace holds its own commas: `CudfRepartition{Hash, 1→4}`.
                        match label.find('{') {
                            Some(open) if open < end => label.find('}').map_or(end, |c| c + 1),
                            _ => end,
                        }
                    });
                seqs.push(label[..end].to_string());
                rest = &label[end..];
            }
        }
        nodes
    }
}
