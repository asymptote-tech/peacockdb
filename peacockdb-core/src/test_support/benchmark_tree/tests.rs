//! The benchmark tree's one reader, over literal sections: a section written now, one written
//! before bench-rows, and the shapes it refuses.

use crate::test_support::{FAILED, SKIPPED, parse_benchmark_section};

/// Two loaders under a join, with every key a section written now carries.
const NEW: &str = "\
GpuUnload
  time_us=[[5]] total_us=5 batch_rows=[[10]] batch_bytes=[[80]]
  GpuHashJoin: join_type=Inner, on=[(a@0, b@0)], lanes=1, batches=multiple
    time_us=[[0,7]] total_us=7 batch_rows=[[10]] batch_bytes=[[80]]
    GpuLoadParquet: table=a, projections=[a@0], partition_groups=[[[0]]], lanes=1, batches=multiple
      time_us=[[40]] total_us=40 batch_rows=[[5]] batch_bytes=[[40]]
    GpuLoadParquet: table=b, projections=[b@0], partition_groups=[[[0,1]]], lanes=1, batches=multiple
      time_us=[[12,13]] total_us=25 batch_rows=[[6,4]] batch_bytes=[[48,32]]
--- run ---
run_us=90
device_us=77
load_us=65
execute_us=12
peak_bytes=4096
runs=[95,90,91,92,93,94,96,97,98,89]
build=release
allocator=rmm-pool initial=69.0GiB max=69.0GiB of 79.1GiB free on a discrete device
pinned_cache=on cap=136.9GiB resident=0.0GiB files=2 fills=2 fill_us=300 fallbacks=0 evictions=0 reloads=0 source=default
prefill_files=2
prefill_bytes=1000
prefill_us=300
device=NVIDIA H100 80GB HBM3
driver=580.95.05
cuda=12.9
cudf=25.02.02
host=nebius-h100
";

/// The nested-loop-join section of `h200/tpch.sf40/tp1_single`, verbatim.
const LEGACY: &str = "\
GpuUnload
  time_us=[[149]] total_us=149
  GpuNestedLoopJoin: join_type=Inner, filter=n_regionkey@probe:2 > r_regionkey@build:0, lanes=1, batches=multiple
    time_us=[[0,557,0]] total_us=557
    GpuCoalesceAllBatches: lanes=1, batches=single
      time_us=[[0,117]] total_us=117
      GpuLoadParquet: table=region, projections=[r_regionkey@0, r_name@1, r_comment@2], partition_groups=[[[0]]], lanes=1, batches=multiple
        time_us=[[766]] total_us=766
    GpuLoadParquet: table=nation, projections=[n_nationkey@0, n_name@1, n_regionkey@2, n_comment@3], partition_groups=[[[0]]], lanes=1, batches=multiple
      time_us=[[849]] total_us=849
--- run ---
run_us=2651
device_us=2438
runs=[2844,2750,2725,2701,2675,2702,2666,2651,2666,2636]
build=release
allocator=rmm-pool initial=69.0GiB max=69.0GiB of 139.2GiB free on a discrete device
";

#[test]
fn a_new_section_with_its_provenance_last_is_read() {
    let s = parse_benchmark_section(NEW).expect("timed");
    assert_eq!(s.nodes.len(), 4);
    assert_eq!(s.nodes[3].batch_rows, Some(vec![vec![6, 4]]));
    assert_eq!(s.nodes[3].batch_bytes, Some(vec![vec![48, 32]]));
    assert_eq!(s.nodes[3].time_us, vec![vec![12, 13]]);
    assert_eq!(s.nodes[0].children, [1]);
    assert_eq!(s.nodes[1].children, [2, 3]);
    assert_eq!(s.count("load_us"), Some(s.tree_load_us()));
    assert_eq!(
        s.count("execute_us"),
        Some(s.tree_device_us() - s.tree_load_us())
    );
    assert_eq!(s.count("peak_bytes"), Some(4096));
    assert_eq!(
        s.field("allocator"),
        Some("rmm-pool initial=69.0GiB max=69.0GiB of 79.1GiB free on a discrete device")
    );
    assert_eq!(s.trailer.last(), Some(&("host", "nebius-h100")));
}

#[test]
fn a_legacy_section_reads_with_no_batches_and_no_load() {
    let s = parse_benchmark_section(LEGACY).expect("timed");
    assert_eq!(s.nodes.len(), 5);
    assert!(
        s.nodes
            .iter()
            .all(|n| n.batch_rows.is_none() && n.batch_bytes.is_none())
    );
    assert_eq!((s.count("load_us"), s.count("peak_bytes")), (None, None));
    assert_eq!(s.tree_load_us(), 766 + 849);
    assert_eq!(s.count("device_us"), Some(s.tree_device_us()));
}

#[test]
fn a_tree_with_no_loader_loads_nothing() {
    let body = "\
GpuUnload
  time_us=[[3]] total_us=3 batch_rows=[[0]] batch_bytes=[[0]]
  GpuEmpty: lanes=1, batches=single
    time_us=[[1]] total_us=1 batch_rows=[[]] batch_bytes=[[]]
--- run ---
run_us=9
device_us=4
load_us=0
execute_us=4
";
    let s = parse_benchmark_section(body).expect("timed");
    assert_eq!(s.tree_load_us(), 0);
    assert_eq!(s.count("load_us"), Some(0));
    assert_eq!(s.nodes[1].batch_rows, Some(vec![vec![]]));
}

#[test]
fn a_marker_and_a_failure_are_not_timed() {
    assert!(parse_benchmark_section(&format!("{SKIPPED}not enabled at this mode\n")).is_none());
    let failed =
        format!("{FAILED}timeout: x\ndevice=NVIDIA H200\ndriver=1\ncuda=1\ncudf=1\nhost=h\n");
    assert!(parse_benchmark_section(&failed).is_none());
}

#[test]
#[should_panic(expected = "unknown")]
fn an_unknown_token_beneath_a_node_is_refused() {
    parse_benchmark_section("GpuUnload\n  time_us=[[1]] total_us=1 rows=[[1]]\n--- run ---\n");
}

#[test]
#[should_panic(expected = "twice")]
fn a_trailer_key_read_twice_is_refused() {
    let body = format!("{LEGACY}device_us=1\n");
    parse_benchmark_section(&body)
        .expect("timed")
        .field("device_us");
}
