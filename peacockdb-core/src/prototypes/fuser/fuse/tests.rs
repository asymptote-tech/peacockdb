//! The fuser over hand-built plans: which scans share and which do not, what each consumer
//! reads, how a held reader is resolved, and that the fused roots answer as each plan alone.
//! The loads read real files, since the fuser re-reads the union's row-group metadata.

use std::path::PathBuf;
use std::sync::Arc;

use datafusion::arrow::array::{Int64Array, RecordBatch};
use datafusion::arrow::datatypes::{DataType, Field, Schema as ArrowSchema};
use datafusion::parquet::arrow::ArrowWriter;
use datafusion::parquet::file::properties::WriterProperties;

use crate::executor::{CpuBackend, CpuBatch, run, run_roots};
use crate::plan::{Batching, Expr, NamedExpr, NodeRef, RowInterval};
use crate::plan::{GpuLimit, GpuLoadParquet, GpuNode, GpuUnload, PlanDag, PlanError};
use crate::plan::{as_node_ref, validate_dag};
use crate::prototypes::fuser::tests::{buffer_every_hold_lowest, re_read_every_hold};
use crate::prototypes::fuser::tests::{child, filter, fused_with, held_pair, join, lineitem};
use crate::prototypes::fuser::tests::{coalesce, load, orders, queries, refs, schema_of, unload};
use crate::prototypes::fuser::{Choice, Fused, HeldReader, HoldDecision, Unshared};
use crate::prototypes::fuser::{fuse, resolve_cap_holds};
use crate::test_support::batches_to_sorted_str;

fn first_ten() -> RowInterval {
    RowInterval {
        skip: 0,
        fetch: Some(10),
    }
}

fn load_of(node: &dyn GpuNode) -> &GpuLoadParquet {
    match as_node_ref(node) {
        NodeRef::LoadParquet(load) => load,
        _ => panic!("{} is not a load", node.name()),
    }
}

/// q0's filter in its own pre-order: unload, join, coalesce, orders, filter.
const HELD_FILTER: usize = 4;

#[test]
fn two_scans_of_one_file_become_one_shared_load() {
    let trees = vec![
        unload(lineitem(&[0, 4], &[0, 1])),
        unload(lineitem(&[4, 5], &[1, 2])),
    ];
    let fused = fused_with(&trees, &mut re_read_every_hold);
    assert_eq!(fused.shared.len(), 1);
    assert_eq!(fused.shared[0].table, "lineitem");
    assert_eq!(fused.shared[0].consumers, [0, 1]);
    let roots = refs(&fused.roots);
    let dag = PlanDag::of(&roots);
    let read_twice: Vec<usize> = (0..dag.nodes.len())
        .filter(|at| dag.nodes[*at].readers.len() == 2)
        .collect();
    assert_eq!(read_twice.len(), 1, "one node has two readers");
    let shared = load_of(dag.nodes[read_twice[0]].node);
    assert_eq!(shared.projection, [0, 4, 5]);
    let kept: Vec<u32> = shared.survivors.iter().map(|group| group.index).collect();
    assert_eq!(kept, [0, 1, 2]);
    assert_eq!(shared.partition_groups, [[[0, 1, 2]]]);
    assert_eq!(fused.reads, [("lineitem".to_string(), 1)]);
}

#[test]
fn each_consumer_reads_its_own_columns() {
    let trees = vec![
        unload(lineitem(&[0, 4], &[0, 1])),
        unload(lineitem(&[4, 5], &[1, 2])),
    ];
    let fused = fused_with(&trees, &mut re_read_every_hold);
    let read = child(fused.roots[0].as_ref(), 0);
    let NodeRef::Project(project) = as_node_ref(read) else {
        panic!("q0 reads through a {}", read.name());
    };
    assert_eq!(
        project.exprs,
        [
            NamedExpr::new(Expr::column(0, "l_orderkey"), "l_orderkey"),
            NamedExpr::new(Expr::column(1, "l_quantity"), "l_quantity"),
        ]
    );
    assert_eq!(schema_of(read), schema_of(child(trees[0].as_ref(), 0)));
    assert!(
        child(read, 0).sharing_key().is_some(),
        "it selects from the source"
    );
}

#[test]
fn a_consumer_with_the_union_projection_reads_the_source_directly() {
    let trees = vec![
        unload(lineitem(&[0, 4, 5], &[0])),
        unload(lineitem(&[4, 5], &[0])),
    ];
    let fused = fused_with(&trees, &mut re_read_every_hold);
    let direct = child(fused.roots[0].as_ref(), 0);
    assert!(direct.sharing_key().is_some(), "q0 reads {}", direct.name());
    let selected = child(fused.roots[1].as_ref(), 0);
    assert!(matches!(as_node_ref(selected), NodeRef::Project(_)));
}

#[test]
fn a_limit_cut_scan_is_not_shared() {
    let trees = vec![
        Box::new(GpuUnload::new(lineitem(&[0], &[0]), Some(first_ten()))) as Box<dyn GpuNode>,
        unload(filter(Box::new(GpuLimit::new(
            lineitem(&[0], &[0]),
            first_ten(),
        )))),
    ];
    let fused = fused_with(&trees, &mut re_read_every_hold);
    assert!(fused.shared.is_empty());
    assert_eq!(
        fused.unshared,
        [
            (0, "lineitem".to_string(), Unshared::LimitCut),
            (1, "lineitem".to_string(), Unshared::LimitCut),
        ]
    );
    assert_eq!(fused.reads, [("lineitem".to_string(), 2)]);
    assert_eq!(Unshared::LimitCut.to_string(), "limit-cut");
}

/// Two row groups of two rows: `k` never NULL, `v` NULL only in the second group.
fn file_with_a_null_in_its_second_group() -> PathBuf {
    let path = std::env::temp_dir().join(format!("fuser_nulls_{}.parquet", std::process::id()));
    let schema = Arc::new(ArrowSchema::new(vec![
        Field::new("k", DataType::Int64, true),
        Field::new("v", DataType::Int64, true),
    ]));
    let batch = RecordBatch::try_new(
        schema.clone(),
        vec![
            Arc::new(Int64Array::from(vec![1, 2, 3, 4])),
            Arc::new(Int64Array::from(vec![Some(1), Some(2), None, Some(4)])),
        ],
    )
    .expect("a batch");
    let properties = WriterProperties::builder()
        .set_max_row_group_size(2)
        .build();
    let file = std::fs::File::create(&path).expect("the file is created");
    let mut writer = ArrowWriter::try_new(file, schema, Some(properties)).expect("a writer");
    writer.write(&batch).expect("the rows write");
    writer.close().expect("the file closes");
    path
}

#[test]
fn a_scan_whose_columns_gain_nulls_is_not_shared() {
    let path = file_with_a_null_in_its_second_group();
    let trees = vec![
        unload(load(&path, &[0], &[0, 1])),
        unload(load(&path, &[0, 1], &[0])),
    ];
    assert_eq!(
        load_of(child(trees[1].as_ref(), 0)).can_be_null,
        [false, false]
    );
    let fused = fused_with(&trees, &mut re_read_every_hold);
    let table = path.file_stem().unwrap().to_string_lossy().to_string();
    std::fs::remove_file(&path).expect("the file is removed");
    assert!(fused.shared.is_empty());
    let nulls = Unshared::Nulls {
        column: "v".to_string(),
    };
    assert_eq!(nulls.to_string(), "nulls:v");
    assert_eq!(fused.unshared, [(1, table, nulls)]);
}

#[test]
fn a_rows_only_scan_shares_only_with_rows_only_scans() {
    let trees = vec![
        unload(lineitem(&[], &[0])),
        unload(lineitem(&[], &[0, 1])),
        unload(lineitem(&[0], &[0])),
    ];
    let fused = fused_with(&trees, &mut re_read_every_hold);
    assert_eq!(fused.shared.len(), 1);
    assert_eq!(fused.shared[0].consumers, [0, 1]);
    assert_eq!(
        fused.unshared,
        [(2, "lineitem".to_string(), Unshared::RowsOnly)]
    );
    assert_eq!(Unshared::RowsOnly.to_string(), "rows-only");
    assert_eq!(fused.reads, [("lineitem".to_string(), 2)]);
    validate_dag(&refs(&fused.roots), None).expect("the fused roots validate");
}

#[test]
fn two_scans_of_one_file_in_one_query_share() {
    let trees = vec![unload(join(
        coalesce(lineitem(&[0], &[0])),
        lineitem(&[0, 4], &[0]),
    ))];
    let fused = fused_with(&trees, &mut buffer_every_hold_lowest);
    assert_eq!(fused.shared.len(), 1);
    assert_eq!(fused.shared[0].consumers, [0, 0]);
    assert_eq!(fused.holds.len(), 1);
    assert_eq!(fused.reads, [("lineitem".to_string(), 1)]);
    validate_dag(&refs(&fused.roots), None).expect("the fused roots validate");
}

#[test]
fn a_held_reader_is_re_read() {
    let trees = held_pair(&[0, 4], &[0, 4]);
    let fused = fused_with(&trees, &mut re_read_every_hold);
    assert_eq!(fused.holds.len(), 1);
    let record = &fused.holds[0];
    assert_eq!(record.decision.choice, Choice::ReRead);
    let figures = (record.decision.bytes, record.decision.reread_us);
    assert_eq!(
        (figures, record.decision.why.as_str()),
        ((0, 0), "test: re-read")
    );
    assert_eq!((record.query, record.table.as_str()), (0, "lineitem"));
    assert_eq!(record.hold, "GpuHashJoin");
    assert_eq!(record.above, None);
    let held_filter = child(child(fused.roots[0].as_ref(), 0), 1);
    let own = child(held_filter, 0);
    assert!(
        own.sharing_key().is_none(),
        "q0's filter reads its own load"
    );
    assert_eq!(load_of(own).projection, [0, 4]);
    assert_eq!(fused.shared[0].consumers, [1]);
    assert_eq!(
        fused.reads,
        [("orders".to_string(), 1), ("lineitem".to_string(), 2)]
    );
    validate_dag(&refs(&fused.roots), None).expect("the fused roots validate");
}

#[test]
fn a_held_reader_is_buffered_where_the_chooser_says() {
    let trees = held_pair(&[0, 4], &[0, 4]);
    let mut above_the_filter = |_: &Fused, held: &HeldReader| HoldDecision {
        choice: Choice::Buffer { edge: held.path[1] },
        bytes: 0,
        reread_us: 0,
        why: "test: above the filter".to_string(),
    };
    let fused = fused_with(&trees, &mut above_the_filter);
    let buffer = child(child(fused.roots[0].as_ref(), 0), 1);
    assert!(matches!(as_node_ref(buffer), NodeRef::HoldBuffer(_)));
    assert!(matches!(as_node_ref(child(buffer, 0)), NodeRef::Filter(_)));
    assert_eq!(fused.holds.len(), 1);
    assert_eq!(fused.holds[0].above, Some(HELD_FILTER));
    assert_eq!(fused.shared[0].consumers, [0, 1]);
    assert_eq!(
        fused.reads,
        [("orders".to_string(), 1), ("lineitem".to_string(), 1)]
    );
    validate_dag(&refs(&fused.roots), None).expect("the fused roots validate");
}

#[test]
fn the_chooser_sees_each_named_edge_once() {
    // Two lineitem readers inside the outer join's probe side, both reading the source the
    // second query reads outside any hold.
    let trees = vec![
        unload(join(
            coalesce(orders(&[0], &[0])),
            join(
                coalesce(lineitem(&[0, 4], &[0])),
                filter(lineitem(&[0, 4], &[0])),
            ),
        )),
        unload(filter(lineitem(&[0, 4], &[0]))),
    ];
    let mut paths: Vec<Vec<(usize, usize)>> = Vec::new();
    let mut counting = |fused: &Fused, held: &HeldReader| {
        paths.push(held.path.clone());
        buffer_every_hold_lowest(fused, held)
    };
    let fused = fused_with(&trees, &mut counting);
    assert_eq!(paths.len(), 2, "{paths:?}");
    assert_ne!(paths[0], paths[1]);
    assert_eq!(fused.holds.len(), 2);
    validate_dag(&refs(&fused.roots), None).expect("the fused roots validate");
}

/// The node at pre-order `at` of `tree`.
fn nth(tree: &dyn GpuNode, at: usize) -> &dyn GpuNode {
    fn walk<'t>(node: &'t dyn GpuNode, out: &mut Vec<&'t dyn GpuNode>) {
        out.push(node);
        for child in node.children() {
            walk(child, out);
        }
    }
    let mut nodes = Vec::new();
    walk(tree, &mut nodes);
    nodes[at]
}

#[test]
fn origins_map_every_fused_node_back() {
    let trees = held_pair(&[0, 4], &[0, 5]);
    let fused = fused_with(&trees, &mut buffer_every_hold_lowest);
    let roots = refs(&fused.roots);
    let dag = PlanDag::of(&roots);
    assert_eq!(fused.origins.len(), dag.nodes.len());
    let mut inserted = Vec::new();
    for (at, origins) in fused.origins.iter().enumerate() {
        let name = dag.nodes[at].node.name();
        if origins.is_empty() {
            inserted.push(name);
        }
        for origin in origins {
            let original = nth(trees[origin.query].as_ref(), origin.node);
            assert_eq!(name, original.name(), "fused node {at}, origin {origin:?}");
        }
    }
    inserted.sort();
    assert_eq!(inserted, ["GpuHoldBuffer", "GpuProject", "GpuProject"]);
    // The path starts above the project selecting q0's columns, which stands for its load.
    let buffer = child(child(child(fused.roots[0].as_ref(), 0), 1), 0);
    assert!(matches!(as_node_ref(buffer), NodeRef::HoldBuffer(_)));
    assert!(matches!(as_node_ref(child(buffer, 0)), NodeRef::Project(_)));
    assert_eq!(fused.holds[0].above, Some(HELD_FILTER + 1));
    let shared = (0..dag.nodes.len())
        .find(|at| dag.nodes[*at].node.sharing_key().is_some())
        .expect("a shared source");
    assert_eq!(fused.origins[shared].len(), 2);
}

#[test]
fn sized_batching_is_refused() {
    let trees = held_pair(&[0, 4], &[0, 4]);
    let refused = fuse(
        &queries(&trees),
        Batching::Sized {
            target_batch_bytes: 1 << 20,
        },
        &mut re_read_every_hold,
    );
    assert!(
        matches!(refused, Err(PlanError::Unsupported(_))),
        "{:?}",
        refused.err()
    );
}

#[test]
fn a_root_waiting_on_the_cap_is_resolved() {
    let trees = vec![
        unload(filter(lineitem(&[0, 4], &[0]))),
        unload(filter(lineitem(&[0, 4], &[0]))),
    ];
    let fused = fused_with(&trees, &mut buffer_every_hold_lowest);
    assert!(fused.holds.is_empty(), "no join holds a reader");
    let one = std::num::NonZeroUsize::new(1).unwrap();
    let capped = resolve_cap_holds(fused, one, &mut buffer_every_hold_lowest)
        .expect("the waiting root is resolved");
    assert_eq!(capped.holds.len(), 1);
    assert_eq!(
        (capped.holds[0].query, capped.holds[0].hold.as_str()),
        (1, "GpuUnload")
    );
    validate_dag(&refs(&capped.roots), Some(one)).expect("the capped roots validate");
}

fn answers(batches: &[CpuBatch]) -> String {
    let batches: Vec<RecordBatch> = batches
        .iter()
        .map(|batch| batch.record_batch().clone())
        .collect();
    batches_to_sorted_str(&batches)
}

#[test]
fn fused_roots_answer_as_each_alone() {
    let trees = held_pair(&[0, 4], &[0, 5]);
    let ctx = crate::build_session_state(1).task_ctx();
    let alone: Vec<String> = trees
        .iter()
        .map(|tree| {
            let report = run::<CpuBackend>(tree.as_ref(), &ctx, None).expect("it runs alone");
            answers(&report.batches)
        })
        .collect();
    assert!(alone.iter().all(|answer| !answer.is_empty()));
    let choosers: [(&str, &mut dyn FnMut(&Fused, &HeldReader) -> HoldDecision); 2] = [
        ("re-read", &mut re_read_every_hold),
        ("buffer", &mut buffer_every_hold_lowest),
    ];
    for (what, choose) in choosers {
        let fused = fused_with(&trees, choose);
        let run = run_roots::<CpuBackend>(&refs(&fused.roots), &ctx, None, None)
            .unwrap_or_else(|why| panic!("{what}: {why}"));
        for (root, answer) in run.answers.iter().enumerate() {
            assert_eq!(answers(answer), alone[root], "{what}: root {root}");
        }
    }
}
