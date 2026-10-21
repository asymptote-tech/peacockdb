//! The join session on a device: built once, probed per batch, finished once, released on
//! drop — and what it holds and spends between those calls, as the accountant reads it.
//! The scatter rides here too, as the other executor that answers more than a batch in.

use super::*;

use super::super::join::PairScratch;
use super::super::{GpuEmitter, GpuJoin as GpuJoinExec, GpuProbingJoin};
use crate::common::logical_size_from_schema;
use crate::executor::{AbiTarget, Backend, Executor, GpuBackend, GpuContext, NodeExecutors};
use crate::plan::{BatchLayout, GpuEmitPartitions, GpuHashJoin, JoinFilterColumn, JoinSide};
use crate::tests::given::Given;
use crate::wire::FbKind;
use crate::wire::{DEFAULT_CHUNK_BYTES, attach_recipes};
use datafusion::arrow::datatypes::TimeUnit;
use datafusion::common::JoinType;
use peacockdb_ffi::raw::peacock_join_finish;

/// Both sides' columns, the probe's nullable where a Left pads it.
fn joined(probe_padded: bool) -> ArrowSchema {
    ArrowSchema::new(vec![
        Field::new("k", DataType::Utf8, true),
        Field::new("v", DataType::Int64, true),
        Field::new("k2", DataType::Utf8, probe_padded),
        Field::new("v2", DataType::Int64, probe_padded),
    ])
}

/// A hash join of the table to itself, on `k` (`key = 0`) or on `v` (`key = 1`): `build` and
/// `probe` are the row-group lists of each side's one lane.
fn self_join(
    join_type: JoinType,
    key: u32,
    build: Vec<u32>,
    probe: Vec<Vec<u32>>,
    output: &ArrowSchema,
) -> Box<dyn GpuNode> {
    Box::new(GpuHashJoin::new(
        mapped(vec![vec![build]]),
        mapped(vec![probe]),
        join_type,
        vec![(key, key)],
        None,
        Vec::new(),
        false,
        None,
        Schema::new(Arc::new(output.clone())),
    ))
}

/// The join's executor, at post-order 2 — after its two leaves — as `executors_for` builds
/// it: keyed, no residual, the default 1 GiB chunk.
fn session_join(session: &Session, join_type: JoinType, output: &ArrowSchema) -> GpuJoinExec {
    let scratch = PairScratch::new(true, &[], &columns(), &columns(), DEFAULT_CHUNK_BYTES);
    GpuJoinExec::new(
        session.site(),
        session.recipe(2),
        join_type,
        scratch,
        0,
        output,
    )
    .expect("the recipe is a session's")
}

fn rows_of(session: &Session, batch: GpuBatch, out: &ArrowSchema) -> Vec<Vec<ScalarValue>> {
    let (back, _) = session
        .export(out)
        .unload(batch, RowRange::WHOLE)
        .expect("the rows cross the boundary");
    by_key(&back)
}

/// The defect the session ends (#152): the old recipe's second probe batch found its build
/// side erased by the first. Three batches, the third a re-read of the first, each answered
/// with its own matches — `a` and `b` meet the build's `a` and `b` once per batch.
#[test]
fn a_session_probed_three_times_answers_each() {
    let out = joined(false);
    let tree = self_join(
        JoinType::Inner,
        0,
        vec![2],
        vec![vec![0], vec![1], vec![0]],
        &out,
    );
    let session = Session::open(tree.as_ref());
    let (mut probing, _) = session_join(&session, JoinType::Inner, &out)
        .set_build(Some(session.scan(&[2])))
        .expect("the session is built");
    for (group, probed) in [(0u32, [2, 1]), (1, [4, 3]), (0, [2, 1])] {
        let (answer, _) = probing
            .probe_and_fetch(session.scan(&[group]))
            .expect("every probe batch is answered");
        let answer = answer.expect("an Inner probe answers a table");
        assert_eq!(
            rows_of(&session, answer, &out),
            vec![
                vec![
                    string("a"),
                    ScalarValue::Int64(Some(6)),
                    string("a"),
                    ScalarValue::Int64(Some(probed[0]))
                ],
                vec![
                    string("b"),
                    ScalarValue::Int64(Some(5)),
                    string("b"),
                    ScalarValue::Int64(Some(probed[1]))
                ],
            ],
            "row group {group}"
        );
    }
    let (finished, _) = probing.finish_and_fetch().expect("the finish runs");
    assert!(finished.is_none(), "an Inner join owes nothing at done");
}

/// The join's executor as `executors_for` builds it from the node, which is where its
/// prices are read off the plan.
fn backend_join(session: &Session, tree: &dyn GpuNode) -> GpuJoinExec {
    let ctx = GpuContext {
        executor: session.executor,
        recipes: attach_recipes(tree, DEFAULT_CHUNK_BYTES)
            .expect("every node's payload is writable"),
    };
    match GpuBackend::executors_for(&ctx, tree, 2, 0) {
        Ok(NodeExecutors::Join(join)) => join,
        Ok(other) => panic!("a join built a {:?} executor", other.category()),
        Err(error) => panic!("the join did not build: {error}"),
    }
}

/// The four types that answer each probe row in its own call owe nothing at done, and their
/// recipe publishes no `join_finish`. Their finish ends the session without one, so neither
/// the journal nor the device's regions hold a call past the probe.
#[test]
fn a_join_that_owes_nothing_at_done_makes_no_finish_call() {
    for join_type in [
        JoinType::Inner,
        JoinType::Right,
        JoinType::RightSemi,
        JoinType::RightAnti,
    ] {
        let out = match join_type {
            JoinType::Inner | JoinType::Right => joined(false),
            _ => columns(),
        };
        let tree = self_join(join_type, 0, vec![2], vec![vec![0]], &out);
        let session = Session::open(tree.as_ref());
        let join = session_join(&session, join_type, &out);
        let (build, probe) = (session.scan(&[2]), session.scan(&[0]));
        let _measured = Measuring::on();
        let (mut probing, _) = join.set_build(Some(build)).expect("built");
        probing.probe_and_fetch(probe).expect("probed");
        let (finished, stats) = probing.finish_and_fetch().expect("finished");
        assert!(finished.is_none(), "{join_type:?} owes nothing at done");
        assert_eq!(
            made(&stats),
            Vec::new(),
            "{join_type:?}'s finish made a call"
        );
        assert_eq!(
            session.regions().len(),
            2,
            "{join_type:?}: the build and the probe, and no finish"
        );
    }
}

/// The session is the device's to free, and `Drop` is what frees it: after the drop the
/// device no longer knows the id, which it says rather than answering.
#[test]
fn dropping_a_probing_join_releases_its_session() {
    let out = joined(true);
    let tree = self_join(JoinType::Left, 0, vec![2], vec![vec![0]], &out);
    let session = Session::open(tree.as_ref());
    let (probing, _) = session_join(&session, JoinType::Left, &out)
        .set_build(Some(session.scan(&[2])))
        .expect("the session is built");
    let id = probing.session;
    drop(probing);
    let mut handle = 0u64;
    let mut stats = PeacockNodeStats::default();
    let rc = unsafe { peacock_join_finish(session.executor, id, &mut handle, &mut stats) };
    assert_ne!(rc, 0, "the session outlived the executor that held it");
    let said = error_of(session.executor);
    assert!(said.contains(&format!("unknown join {id}")), "{said}");
}

/// A lane with no build batch owes nothing for the six types every row of which is built
/// from a build row, and its probe rows for the three that preserve them — read off the
/// type at `set_build`, the one `join_build(0)` still makes. A batch of no rows is the
/// harness's `*_over_a_zero_row_build_*` cases, which both engines drain.
#[test]
fn an_empty_build_under_inner_owes_nothing() {
    let out = joined(false);
    let owes = |join_type: JoinType| -> bool {
        let tree = self_join(join_type, 0, vec![2], vec![vec![0]], &out);
        let session = Session::open(tree.as_ref());
        let (probing, _) = session_join(&session, join_type, &out)
            .set_build(None)
            .expect("a session is built over no batch");
        !probing.owes_nothing()
    };
    assert!(!owes(JoinType::Inner));
    for join_type in [
        JoinType::Left,
        JoinType::LeftSemi,
        JoinType::LeftAnti,
        JoinType::LeftMark,
        JoinType::RightSemi,
    ] {
        assert!(!owes(join_type), "{join_type:?}");
    }
    for join_type in [JoinType::Right, JoinType::Full, JoinType::RightAnti] {
        assert!(owes(join_type), "{join_type:?}");
    }
}

/// A Left join's finish, once, after a probe that matched and one that did not: the build
/// rows nothing matched, padded. On `v`, the build `{4, 3, 6, 5}` meets `{6, 5}` and then
/// `{2, 1}`, so 4 and 3 are what the finish owes.
#[test]
fn a_build_preserving_finish_answers_once() {
    let out = joined(true);
    let tree = self_join(JoinType::Left, 1, vec![1, 2], vec![vec![2], vec![0]], &out);
    let session = Session::open(tree.as_ref());
    let (mut probing, _) = session_join(&session, JoinType::Left, &out)
        .set_build(Some(session.scan(&[1, 2])))
        .expect("the session is built");
    let (first, _) = probing.probe_and_fetch(session.scan(&[2])).expect("probed");
    assert_eq!(rows_of(&session, first.expect("a table"), &out).len(), 2);
    let (second, _) = probing.probe_and_fetch(session.scan(&[0])).expect("probed");
    assert_eq!(
        rows_of(&session, second.expect("a table, of no rows"), &out).len(),
        0,
        "nothing in {{2, 1}} meets the build"
    );
    let (finished, _) = probing.finish_and_fetch().expect("the finish runs");
    assert_eq!(
        rows_of(
            &session,
            finished.expect("Left owes its unmatched build rows"),
            &out
        ),
        vec![
            vec![
                string("a"),
                ScalarValue::Int64(Some(4)),
                ScalarValue::Utf8(None),
                ScalarValue::Int64(None)
            ],
            vec![
                string("b"),
                ScalarValue::Int64(Some(3)),
                ScalarValue::Utf8(None),
                ScalarValue::Int64(None)
            ],
        ]
    );
}

/// `k` and a microsecond timestamp: a side the fixture file does not hold, as a leaf that
/// is never read — what the session takes from it is `CudfJoin`'s schema alone.
fn stamped() -> Schema {
    Schema::new(Arc::new(ArrowSchema::new(vec![
        Field::new("k", DataType::Utf8, true),
        Field::new("ts", DataType::Timestamp(TimeUnit::Microsecond, None), true),
    ])))
}

/// The pad column of a side that sent no batch takes its type from the schema the wire
/// wrote for that side, which is the whole reason `CudfJoin` carries both: a Left join that
/// finishes before any probe batch pads `ts` as the probe declares it.
#[test]
fn a_left_join_with_a_timestamp_probe_column_finished_without_a_probe_batch_pads_it() {
    let out = ArrowSchema::new(vec![
        Field::new("k", DataType::Utf8, true),
        Field::new("v", DataType::Int64, true),
        Field::new("k2", DataType::Utf8, true),
        Field::new("ts", DataType::Timestamp(TimeUnit::Microsecond, None), true),
    ]);
    let tree: Box<dyn GpuNode> = Box::new(GpuHashJoin::new(
        mapped(vec![vec![vec![2]]]),
        Given::of(stamped(), BatchLayout::MultipleBatches),
        JoinType::Left,
        vec![(0, 0)],
        None,
        Vec::new(),
        false,
        None,
        Schema::new(Arc::new(out.clone())),
    ));
    let session = Session::open(tree.as_ref());
    let scratch = PairScratch::new(
        true,
        &[],
        &columns(),
        stamped().fields.as_ref(),
        DEFAULT_CHUNK_BYTES,
    );
    let (probing, _) = GpuJoinExec::new(
        session.site(),
        session.recipe(2),
        JoinType::Left,
        scratch,
        0,
        &out,
    )
    .expect("the recipe is a session's")
    .set_build(Some(session.scan(&[2])))
    .expect("the session is built");
    let (finished, _) = probing.finish_and_fetch().expect("the finish runs");
    let (back, _) = session
        .export(&out)
        .unload(
            finished.expect("Left owes every build row"),
            RowRange::WHOLE,
        )
        .expect("the rows cross the boundary");
    let back = back.record_batch();
    assert_eq!(back.num_rows(), 2);
    assert_eq!(
        back.column(3).data_type(),
        &DataType::Timestamp(TimeUnit::Microsecond, None)
    );
    assert_eq!(
        back.column(3).null_count(),
        2,
        "every build row is unmatched"
    );
}

/// The other side's absence (#212's device half): a Right join whose lane got no build
/// batch owes every probe row, padded with the build columns `CudfJoin` declares — the
/// timestamp among them, typed though no batch ever carried it.
#[test]
fn a_right_join_whose_lane_got_no_build_batch_pads_a_timestamp_build_column() {
    let out = ArrowSchema::new(vec![
        Field::new("k", DataType::Utf8, true),
        Field::new("ts", DataType::Timestamp(TimeUnit::Microsecond, None), true),
        Field::new("k2", DataType::Utf8, true),
        Field::new("v", DataType::Int64, true),
    ]);
    let tree: Box<dyn GpuNode> = Box::new(GpuHashJoin::new(
        Given::of(stamped(), BatchLayout::SingleBatch),
        mapped(vec![vec![vec![0]]]),
        JoinType::Right,
        vec![(0, 0)],
        None,
        Vec::new(),
        false,
        None,
        Schema::new(Arc::new(out.clone())),
    ));
    let session = Session::open(tree.as_ref());
    // The given leaf is a stub on the wire, so the scan is post-order 1 and the join 2.
    let scan = session.recipes.get(1).expect("the scan's recipe").calls[0]
        .target
        .expect("a scan addresses its node")
        .0;
    let scratch = PairScratch::new(
        true,
        &[],
        stamped().fields.as_ref(),
        &columns(),
        DEFAULT_CHUNK_BYTES,
    );
    let (mut probing, _) = GpuJoinExec::new(
        session.site(),
        session.recipe(2),
        JoinType::Right,
        scratch,
        0,
        &out,
    )
    .expect("the recipe is a session's")
    .set_build(None)
    .expect("a session is built over no batch");
    assert!(!probing.owes_nothing(), "a Right join owes its probe rows");
    let (answer, _) = probing
        .probe_and_fetch(scan_at(&session, scan, &[0]))
        .expect("the probe is answered");
    let (back, _) = session
        .export(&out)
        .unload(answer.expect("one table per probe batch"), RowRange::WHOLE)
        .expect("the rows cross the boundary");
    let back = back.record_batch();
    assert_eq!(back.num_rows(), 2, "both probe rows, padded");
    assert_eq!(
        back.column(1).data_type(),
        &DataType::Timestamp(TimeUnit::Microsecond, None)
    );
    assert_eq!(back.column(1).null_count(), 2);
}

/// A scan at a seq the caller names, for a tree whose post-order 0 is not the scan.
fn scan_at(session: &Session, seq: Seq, groups: &[u32]) -> GpuBatch {
    let mut handle = 0u64;
    let mut stats = PeacockNodeStats::default();
    let rc = unsafe {
        peacock_executor_execute_scan_rowgroups(
            session.executor,
            seq as u64,
            groups.as_ptr(),
            groups.len() as u64,
            &mut handle,
            &mut stats,
        )
    };
    assert_eq!(rc, 0, "the scan failed: {}", error_of(session.executor));
    GpuBatch::new(session.executor, handle, seq, stats.rows as usize, 0)
}

/// Every session call is one call on the `CudfJoin`'s seq, journalled here and answered by
/// one device region, counted together: build, three probes and the finish are calls 0 to
/// 4 of that seq, which is the key the driver's measurement joins the two records on.
#[test]
fn every_session_call_is_journalled_and_timed_on_the_joins_seq() {
    let out = joined(true);
    let tree = self_join(
        JoinType::Left,
        0,
        vec![2],
        vec![vec![0], vec![1], vec![0]],
        &out,
    );
    let session = Session::open(tree.as_ref());
    let join = session_join(&session, JoinType::Left, &out);
    let seq = join.seq;
    // The scans are made before timing is on, so the device's regions are the join's alone.
    let build = session.scan(&[2]);
    let probes: Vec<GpuBatch> = [0u32, 1, 0]
        .iter()
        .map(|group| session.scan(&[*group]))
        .collect();
    let _measured = Measuring::on();
    let mut journalled = Vec::new();
    let (mut probing, stats) = join.set_build(Some(build)).expect("built");
    journalled.extend(made(&stats));
    for probe in probes {
        let (_, stats) = probing.probe_and_fetch(probe).expect("probed");
        journalled.extend(made(&stats));
    }
    let (_, stats) = probing.finish_and_fetch().expect("finished");
    journalled.extend(made(&stats));
    assert_eq!(
        journalled
            .iter()
            .map(|call| (call.seq, call.target))
            .collect::<Vec<_>>(),
        vec![(seq, AbiTarget::Node(FbKind::Join)); 5]
    );
    assert_eq!(
        journalled[0].in_rows, 2,
        "the build call was handed the build batch"
    );
    assert_eq!(
        session
            .regions()
            .iter()
            .map(|region| (region.seq, region.call_index))
            .collect::<Vec<_>>(),
        (0..5).map(|index| (seq, index)).collect::<Vec<_>>(),
        "one region per call, the build first"
    );
}

/// What the accountant reads off a session: the build batch, sixteen bytes a row of hash
/// table, and for a type that finishes one byte a row of `matched` — all of it from
/// `set_build` until the drop, and none of it before.
#[test]
fn a_session_holds_its_build_its_hash_table_and_its_matched_column() {
    let held = |join_type: JoinType| -> (usize, usize) {
        let out = joined(join_type == JoinType::Left);
        let tree = self_join(join_type, 0, vec![1, 2], vec![vec![0]], &out);
        let session = Session::open(tree.as_ref());
        let join = session_join(&session, join_type, &out);
        assert_eq!(join.resident_bytes(), 0, "nothing before the build");
        let build = session.scan(&[1, 2]);
        let build_bytes = build.byte_size();
        let (probing, _): (GpuProbingJoin, _) = join.set_build(Some(build)).expect("built");
        (probing.resident_bytes(), build_bytes)
    };
    let (inner, build) = held(JoinType::Inner);
    assert_eq!(inner, build + 16 * 4, "four build rows, no matched column");
    let (left, build) = held(JoinType::Left);
    assert_eq!(
        left,
        build + 16 * 4 + 4,
        "and a byte a row saying which matched"
    );
}

/// A RightSemi session also holds its build's keys taken distinct, the copy its table views:
/// four `k`s here, a string each, at the 16 bytes a row the session prices one. Built as
/// `executors_for` builds it, so the width is read off the node.
#[test]
fn a_probe_side_semi_session_holds_its_distinct_build_keys() {
    let out = columns();
    let tree = self_join(JoinType::RightSemi, 0, vec![1, 2], vec![vec![0]], &out);
    let session = Session::open(tree.as_ref());
    let build = session.scan(&[1, 2]);
    let build_bytes = build.byte_size();
    let (probing, _) = backend_join(&session, tree.as_ref())
        .set_build(Some(build))
        .expect("built");
    assert_eq!(
        probing.resident_bytes(),
        build_bytes + 16 * 4 + 16 * 4,
        "four build rows: the hash table and the distinct keys"
    );
}

/// A LeftSemi probe takes its batch's keys distinct before matching them, so a call is
/// charged a key a probe row beside the batch, an output as long and the pairs.
#[test]
fn a_build_side_semi_probe_is_charged_its_batchs_distinct_keys() {
    let out = columns();
    let tree = self_join(JoinType::LeftSemi, 0, vec![1, 2], vec![vec![0]], &out);
    let session = Session::open(tree.as_ref());
    let (probing, _) = backend_join(&session, tree.as_ref())
        .set_build(Some(session.scan(&[1, 2])))
        .expect("built");
    let output = logical_size_from_schema(&out, 2, 0);
    assert_eq!(
        probing.scratch_bytes(2, 100),
        100 + output + 2 * 8 + 2 * 16,
        "two probe rows: the batch, the output, the pairs and the distinct keys"
    );
}

/// The pair scratch is `gpu_plan.fbs`'s formula: 8 bytes a pair for the index maps plus
/// the residual's own bytes, 16 for a string, and never more than one chunk.
#[test]
fn a_join_calls_pair_scratch_is_the_sessions_chunked_formula() {
    let filter = [
        JoinFilterColumn {
            side: JoinSide::Build,
            index: 0,
        },
        JoinFilterColumn {
            side: JoinSide::Probe,
            index: 1,
        },
    ];
    let keyed = PairScratch::new(true, &filter, &columns(), &columns(), DEFAULT_CHUNK_BYTES);
    assert_eq!(
        keyed.bytes(4, 10),
        10 * (8 + 16 + 8),
        "one match per probe row"
    );
    let keyless = PairScratch::new(false, &filter, &columns(), &columns(), 1000);
    assert_eq!(
        keyless.bytes(4, 10),
        1000,
        "40 pairs at 32 bytes, held to the chunk"
    );
    let cross = PairScratch::new(false, &[], &columns(), &columns(), DEFAULT_CHUNK_BYTES);
    assert_eq!(
        cross.bytes(4, 10),
        40 * 8,
        "no residual: the index maps alone"
    );
}

/// The scatter: one call per batch, N handles back, and the count is the contract.
#[test]
fn a_scatter_answers_with_one_handle_per_lane() {
    const LANES: usize = 3;
    let tree: Box<dyn GpuNode> = Box::new(GpuEmitPartitions::new(source(), vec![0], LANES));
    let session = Session::open(tree.as_ref());
    let out = columns();
    let mut emitter =
        GpuEmitter::new(session.site(), session.recipe(1), &out).expect("the scatter builds");
    let (lanes, _) = emitter
        .emit(session.scan(&ROW_GROUPS))
        .expect("the scatter runs");
    assert_eq!(lanes.len(), LANES);
    let scattered: usize = lanes.iter().map(Batch::num_rows).sum();
    assert_eq!(
        scattered,
        values().len(),
        "every row landed in exactly one lane"
    );
}
