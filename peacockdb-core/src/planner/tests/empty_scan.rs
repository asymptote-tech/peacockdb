//! #282: a scan no row group survives — every one pruned, or a file with none — maps to one
//! lane holding no batch, at every mode, rather than being refused; a scan with survivors maps
//! as before.

use crate::plan::{GpuLoadParquet, GpuNode, NodeRef, as_node_ref};
use crate::planner;
use crate::test_support::{MODES, Mode, data_dir_for};

/// The scans of `node`'s tree, in pre-order.
fn scans(node: &dyn GpuNode) -> Vec<&GpuLoadParquet> {
    let mut found = Vec::new();
    if let NodeRef::LoadParquet(load) = as_node_ref(node) {
        found.push(load);
    }
    for child in node.children() {
        found.extend(scans(child));
    }
    found
}

/// `sql` over tpch sf1, planned at `mode` as the corpus plans it.
async fn planned(sql: &str, mode: &Mode) -> Box<dyn GpuNode> {
    let ctx = crate::register_tables_for(
        crate::build_session_state(mode.target_partitions),
        &data_dir_for("tpch", "1"),
    )
    .await
    .expect("register the tables");
    let physical = ctx
        .sql(sql)
        .await
        .expect("DataFusion plans it")
        .create_physical_plan()
        .await
        .expect("a physical plan");
    let (tree, _memory) = planner::plan(&physical, mode.knobs())
        .unwrap_or_else(|why| panic!("{sql} at {}: {why}", mode.name));
    tree
}

/// nation's one row group holds `n_nationkey` 0–24, so `< 0` prunes it. One lane, whatever
/// the mode's target partitions: no survivor has rows to balance across lanes.
#[tokio::test]
async fn a_scan_no_row_group_survives_is_one_lane_with_no_batch_at_every_mode() {
    for mode in &MODES {
        let tree = planned("select count(*) from nation where n_nationkey < 0", mode).await;
        let found = scans(tree.as_ref());
        let [scan] = found.as_slice() else {
            panic!("{}: {} scans", mode.name, found.len())
        };
        assert!(
            scan.survivors.is_empty(),
            "{}: the row group is pruned",
            mode.name
        );
        assert_eq!(
            scan.partition_groups,
            vec![Vec::<Vec<u32>>::new()],
            "{}: one lane, no batch",
            mode.name
        );
    }
}

#[tokio::test]
async fn a_scan_with_survivors_maps_as_before() {
    for mode in &MODES {
        let tree = planned("select count(*) from nation where n_nationkey >= 0", mode).await;
        let found = scans(tree.as_ref());
        let [scan] = found.as_slice() else {
            panic!("{}: {} scans", mode.name, found.len())
        };
        let read: Vec<u32> = scan
            .partition_groups
            .iter()
            .flatten()
            .flatten()
            .copied()
            .collect();
        assert_eq!(read, vec![0], "{}: the one survivor, read once", mode.name);
    }
}
