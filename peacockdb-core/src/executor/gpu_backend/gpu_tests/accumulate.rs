//! The accumulators on a device: three batches into each, and what it emits at done.
//!
//! The source is mapped one batch per row group here, so a lane genuinely has several
//! batches to accumulate — the one shape the exec cases never see.

use super::*;

use super::super::{GpuAccumulator, GpuPartitionAccumulator};
use crate::executor::{AbiCall, AbiTarget, LaneEvent};
use crate::plan::AggFunc;
use crate::plan::AggStateColumns;
use crate::plan::RowInterval;
use crate::plan::{
    GpuAccumulateBatchesAndSort, GpuAggregateBatches, GpuCoalesceAllBatches, GpuLimit,
    GpuMergeSortedPartitions,
};

/// Held bytes at which a compaction runs. Large enough that nothing here compacts before
/// done, since what these cases are about is the answer rather than the schedule.
const NO_COMPACTION: usize = 1 << 30;

/// One lane of three batches through one accumulator, and what it answered with.
fn accumulate(session: &Session, accumulator: GpuAccumulator, out: &ArrowSchema) -> Vec<CpuBatch> {
    let mut accumulator = accumulator;
    let mut emitted = Vec::new();
    for group in ROW_GROUPS {
        let (produced, _) = accumulator
            .accumulate_and_fetch(session.scan(&[group]))
            .expect("the arrival is accepted");
        emitted.extend(produced);
    }
    let (last, _) = accumulator.mark_done_and_fetch().expect("done is accepted");
    emitted.extend(last);
    emitted
        .into_iter()
        .map(|batch| {
            session
                .export(out)
                .unload(batch, RowRange::WHOLE)
                .expect("the rows cross the boundary")
                .0
        })
        .collect()
}

#[test]
fn a_coalesce_answers_with_one_batch_holding_every_row_of_the_lane() {
    let tree: Box<dyn GpuNode> = Box::new(GpuCoalesceAllBatches::new(source_per_row_group()));
    let session = Session::open(tree.as_ref());
    let out = columns();
    let accumulator = GpuAccumulator::coalesce(session.site(), session.recipe(1), &out)
        .expect("the coalesce builds");
    let answered = accumulate(&session, accumulator, &out);
    assert_eq!(answered.len(), 1, "a coalesce emits nothing until done");
    assert_eq!(
        rows(&answered[0])
            .into_iter()
            .map(|row| row[1].clone())
            .collect::<Vec<ScalarValue>>(),
        values()
            .iter()
            .map(|v| ScalarValue::Int64(Some(*v)))
            .collect::<Vec<ScalarValue>>(),
        "the three batches in the order they arrived"
    );
}

fn accumulating_sort(ascending: bool, fetch: Option<usize>) -> Box<dyn GpuNode> {
    Box::new(GpuAccumulateBatchesAndSort::new(
        source_per_row_group(),
        vec![ColumnOrder {
            column: 1,
            ascending,
            nulls_first: false,
        }],
        fetch,
    ))
}

/// The stream, not each batch: the runs are sorted as they arrive and merged at done.
#[test]
fn an_accumulating_sort_orders_the_whole_lane() {
    let tree = accumulating_sort(true, None);
    let session = Session::open(tree.as_ref());
    let out = columns();
    let accumulator =
        GpuAccumulator::sorted(session.site(), session.recipe(1), &out).expect("the sort builds");
    let answered = accumulate(&session, accumulator, &out);
    assert_eq!(
        rows(&answered[0])
            .into_iter()
            .map(|row| row[1].clone())
            .collect::<Vec<ScalarValue>>(),
        (1..=6)
            .map(|v| ScalarValue::Int64(Some(v)))
            .collect::<Vec<ScalarValue>>()
    );
}

/// The fetch rides the merge, so it is a top-N over the lane rather than over each batch.
#[test]
fn an_accumulating_sort_with_a_fetch_keeps_the_top_of_the_lane() {
    let tree = accumulating_sort(false, Some(2));
    let session = Session::open(tree.as_ref());
    let out = columns();
    let accumulator =
        GpuAccumulator::sorted(session.site(), session.recipe(1), &out).expect("the sort builds");
    let answered = accumulate(&session, accumulator, &out);
    assert_eq!(
        rows(&answered[0])
            .into_iter()
            .map(|row| row[1].clone())
            .collect::<Vec<ScalarValue>>(),
        vec![ScalarValue::Int64(Some(6)), ScalarValue::Int64(Some(5))],
        "the two largest of all six"
    );
}

/// A merge reads state, so the init that builds it runs first: the same two nodes the
/// planner stacks, driven by hand. Each lane batch becomes a partial, and the merge folds
/// the three of them.
#[test]
fn a_merge_folds_the_partials_its_init_produced() {
    let state = Schema::new(Arc::new(schema_of(&[
        ("k", DataType::Utf8),
        ("sum(v)", DataType::Int64),
    ])));
    // The init sums the source's `v`, and the merge the state's `sum(v)`: both at ordinal 1.
    let summing = |name: &str| AggregateBody {
        group_by: vec![Expr::column(0, "k")],
        grouping_sets: Vec::new(),
        null_exprs: Vec::new(),
        aggs: vec![AggCall {
            func: PlanAgg::Sum,
            args: vec![Expr::column(1, name)],
            outputs: vec![Field::new("sum(v)", DataType::Int64, true)],
        }],
        finalize: None,
    };
    let init = GpuAggregate::new(
        source_per_row_group(),
        summing("v"),
        state.clone(),
        state.clone(),
    );
    let tree: Box<dyn GpuNode> = Box::new(GpuAggregateBatches::new(
        Box::new(init),
        summing("sum(v)"),
        state.clone(),
        state.clone(),
    ));
    let session = Session::open(tree.as_ref());
    let out = schema_of(&[("k", DataType::Utf8), ("sum(v)", DataType::Int64)]);
    let mut partial = session.exec(1, &out);
    let mut merge =
        GpuAccumulator::aggregate(session.site(), session.recipe(2), &out, &out, NO_COMPACTION)
            .expect("the merge builds");
    for group in ROW_GROUPS {
        let (state, _) = partial
            .exec(session.scan(&[group]))
            .expect("the init runs on this batch");
        merge
            .accumulate_and_fetch(state)
            .expect("the arrival is accepted");
    }
    let (emitted, _) = merge.mark_done_and_fetch().expect("done is accepted");
    let [merged] = <[GpuBatch; 1]>::try_from(emitted).expect("a merge emits one batch");
    let (answer, _) = session
        .export(&out)
        .unload(merged, RowRange::WHOLE)
        .expect("the rows cross the boundary");
    assert_eq!(
        by_key(&answer),
        vec![
            vec![string("a"), ScalarValue::Int64(Some(12))],
            vec![string("b"), ScalarValue::Int64(Some(9))],
        ],
        "2 + 4 + 6 under a and 1 + 3 + 5 under b, across three partials"
    );
}

/// The three things a limit does to a batch: the first two rows are before the interval,
/// the middle batch is wholly inside it, and the third straddles its end.
#[test]
fn a_limit_drops_forwards_and_slices_by_where_the_batch_falls() {
    let tree: Box<dyn GpuNode> = Box::new(GpuLimit::new(
        source_per_row_group(),
        RowInterval {
            skip: 2,
            fetch: Some(3),
        },
    ));
    let session = Session::open(tree.as_ref());
    let out = columns();
    let accumulator = GpuAccumulator::limit(
        session.site(),
        session.recipe(1),
        RowInterval {
            skip: 2,
            fetch: Some(3),
        },
        &out,
    )
    .expect("the limit builds");
    let answered = accumulate(&session, accumulator, &out);
    let kept: Vec<Vec<ScalarValue>> = answered
        .iter()
        .map(|batch| rows(batch).into_iter().map(|row| row[1].clone()).collect())
        .collect();
    assert_eq!(
        kept,
        vec![
            vec![ScalarValue::Int64(Some(4)), ScalarValue::Int64(Some(3))],
            vec![ScalarValue::Int64(Some(6))],
        ],
        "the second batch whole and one row of the third; nothing for the first"
    );
}

/// A slice and an export carry no seq of their own, so both sides charge them to the node
/// whose output they handled — and `call_index` counts all four entry points together.
///
/// Asserted against the device rather than against the journal alone: the two counters are
/// independent, and a rule only one side follows joins to nothing. The scans here are
/// driven by hand and journal nothing, which is why the journal holds three entries and
/// the device six.
#[test]
fn a_slice_and_an_export_are_charged_to_the_node_that_produced_the_handle() {
    let interval = RowInterval {
        skip: 2,
        fetch: Some(3),
    };
    let tree: Box<dyn GpuNode> = Box::new(GpuLimit::new(source_per_row_group(), interval));
    let session = Session::open(tree.as_ref());
    let out = columns();
    let _measured = Measuring::on();
    let mut accumulator = GpuAccumulator::limit(session.site(), session.recipe(1), interval, &out)
        .expect("the limit builds");

    let mut journalled: Vec<AbiCall> = Vec::new();
    let mut kept = Vec::new();
    for group in ROW_GROUPS {
        let (produced, stats) = accumulator
            .accumulate_and_fetch(session.scan(&[group]))
            .expect("the arrival is accepted");
        journalled.extend(made(&stats));
        kept.extend(produced);
    }
    for batch in kept {
        let (_, stats) = session
            .export(&out)
            .unload(batch, RowRange::WHOLE)
            .expect("the rows cross the boundary");
        journalled.extend(made(&stats));
    }

    let seq = session.scan_seq();
    let regions = session.regions();
    assert_eq!(
        regions
            .iter()
            .map(|r| (r.seq, r.call_index))
            .collect::<Vec<_>>(),
        (0..6).map(|index| (seq, index)).collect::<Vec<_>>(),
        "three scans, the slice of the straddling batch and two exports, counted as one \
         run of calls against the scan's seq"
    );
    assert_eq!(
        journalled
            .iter()
            .map(|call| (call.seq, call.target))
            .collect::<Vec<_>>(),
        vec![
            (seq, AbiTarget::Bare(AbiSymbol::SliceHandle)),
            (seq, AbiTarget::Bare(AbiSymbol::ResultFromHandle)),
            (seq, AbiTarget::Bare(AbiSymbol::ResultFromHandle)),
        ],
        "the three calls this side made name the node the device charged them to"
    );
}

/// A lane that received nothing emits nothing, here and on the CPU alike.
#[test]
fn a_lane_that_received_nothing_emits_no_batch() {
    let tree: Box<dyn GpuNode> = Box::new(GpuCoalesceAllBatches::new(source_per_row_group()));
    let session = Session::open(tree.as_ref());
    let out = columns();
    let accumulator = GpuAccumulator::coalesce(session.site(), session.recipe(1), &out)
        .expect("the coalesce builds");
    let (emitted, _) = accumulator
        .mark_done_and_fetch()
        .expect("done is accepted whatever arrived");
    assert!(
        emitted.is_empty(),
        "a lane that received nothing emits nothing, on both backends"
    );
}

/// Two lanes, each sorted by the per-batch sort above it, merged into one at the last
/// lane's done. The handles enter the merge in lane order, which is what a k-way merge
/// over partitions means.
#[test]
fn every_lanes_sorted_run_is_merged_at_the_last_done() {
    let keys = vec![ColumnOrder {
        column: 1,
        ascending: true,
        nulls_first: false,
    }];
    let sorted = GpuSort::new(source_two_lanes(), keys.clone(), None);
    let tree: Box<dyn GpuNode> =
        Box::new(GpuMergeSortedPartitions::new(Box::new(sorted), keys, None));
    let session = Session::open(tree.as_ref());
    let out = columns();
    let mut per_batch = session.exec(1, &out);
    let mut merge =
        GpuPartitionAccumulator::merge_sorted(session.site(), session.recipe(2), 2, &out)
            .expect("the merge builds");
    let lanes: [(usize, &[u32]); 3] = [(0, &[0]), (1, &[1]), (1, &[2])];
    for (lane, groups) in lanes {
        let (run, _) = per_batch
            .exec(session.scan(groups))
            .expect("the sort runs on this batch");
        merge
            .accumulate_and_fetch(lane, LaneEvent::Batch(run))
            .expect("the arrival is accepted");
    }
    let mut emitted = Vec::new();
    for lane in [0, 1] {
        let (produced, _) = merge
            .accumulate_and_fetch(lane, LaneEvent::Done)
            .expect("the done is accepted");
        emitted.extend(produced);
    }
    let [merged] = <[GpuBatch; 1]>::try_from(emitted).expect("one batch, at the last done");
    let (answer, _) = session
        .export(&out)
        .unload(merged, RowRange::WHOLE)
        .expect("the rows cross the boundary");
    assert_eq!(
        rows(&answer)
            .into_iter()
            .map(|row| row[1].clone())
            .collect::<Vec<ScalarValue>>(),
        (1..=6)
            .map(|v| ScalarValue::Int64(Some(v)))
            .collect::<Vec<ScalarValue>>(),
        "both lanes' rows in one order"
    );
}

/// And the arm it does not call is a refusal now (#173). A collapse of no handles has no
/// schema to answer with — it used to hand back a table of no columns, which reads as a
/// batch until something asks for a column. Constructed here through the ABI, since no
/// executor makes this call any more.
#[test]
fn a_collapse_with_no_input_handles_is_refused_by_the_device() {
    let tree: Box<dyn GpuNode> = Box::new(GpuCoalesceAllBatches::new(source_per_row_group()));
    let session = Session::open(tree.as_ref());
    let (seq, _) = session.recipe(1).calls[0]
        .target
        .expect("the collapse addresses its own node");
    let counts = [0u64];
    let mut handles = [0u64];
    let mut stats = [PeacockNodeStats::default()];
    let mut produced = 0u64;
    let rc = unsafe {
        peacock_executor_execute_node(
            session.executor,
            seq as u64,
            std::ptr::null(),
            counts.as_ptr(),
            counts.len() as u64,
            handles.as_mut_ptr(),
            handles.len() as u64,
            &mut produced,
            stats.as_mut_ptr(),
        )
    };
    assert_ne!(rc, 0, "a collapse of nothing cannot answer");
    let message = error_of(session.executor);
    assert!(
        message.contains("no input handles"),
        "the refusal has to say what it was asked for: {message}"
    );
}

/// The Welford merge on a device, and the merge where #103 actually bit. Three lane
/// batches become three partials and one state: for `a` the values are 2, 4 and 6, so a
/// count of 3, a mean of 4 and an m2 of 8, computed here rather than by either engine.
/// The count is `Int64`, as the plan declares every count and as cuDF returns it.
#[test]
fn a_welford_triple_merges_as_one_aggregate_on_the_device() {
    let state = Schema {
        fields: Arc::new(schema_of(&[
            ("k", DataType::Utf8),
            ("stddev(v)$count", DataType::Int64),
            ("stddev(v)$mean", DataType::Float64),
            ("stddev(v)$m2", DataType::Float64),
        ])),
        group_keys: vec![0],
        agg_state: vec![AggStateColumns {
            output: "stddev(v)".to_string(),
            func: AggFunc::Stddev,
            ddof: 1,
            positions: vec![1, 2, 3],
        }],
    };
    let welford = |args: Vec<Expr>, funcs: [PlanAgg; 3]| AggregateBody {
        group_by: vec![Expr::column(0, "k")],
        grouping_sets: Vec::new(),
        null_exprs: Vec::new(),
        aggs: funcs
            .iter()
            .enumerate()
            .map(|(position, func)| AggCall {
                func: *func,
                args: args.clone(),
                outputs: vec![state.fields.field(position + 1).clone()],
            })
            .collect(),
        finalize: None,
    };
    let init = GpuAggregate::new(
        source_per_row_group(),
        welford(
            vec![Expr::column(1, "v")],
            [PlanAgg::Count, PlanAgg::Mean, PlanAgg::M2],
        ),
        state.clone(),
        state.clone(),
    );
    // The merge reads the three state columns as one call, which is what makes it one
    // aggregate on the wire rather than three.
    let mut merge_body = welford(
        vec![
            Expr::column(1, "stddev(v)$count"),
            Expr::column(2, "stddev(v)$mean"),
            Expr::column(3, "stddev(v)$m2"),
        ],
        [PlanAgg::MergeM2, PlanAgg::MergeM2, PlanAgg::MergeM2],
    );
    merge_body.aggs.truncate(1);
    merge_body.aggs[0].outputs = state.fields.fields()[1..]
        .iter()
        .map(|field| field.as_ref().clone())
        .collect();
    let tree: Box<dyn GpuNode> = Box::new(GpuAggregateBatches::new(
        Box::new(init),
        merge_body,
        state.clone(),
        state.clone(),
    ));
    let session = Session::open(tree.as_ref());
    let out = state.fields.as_ref().clone();
    let mut partial = session.exec(1, &out);
    let mut merge =
        GpuAccumulator::aggregate(session.site(), session.recipe(2), &out, &out, NO_COMPACTION)
            .expect("the merge builds");
    for group in ROW_GROUPS {
        let (partials, _) = partial
            .exec(session.scan(&[group]))
            .expect("the init runs on this batch");
        merge
            .accumulate_and_fetch(partials)
            .expect("the arrival is accepted");
    }
    let (emitted, _) = merge.mark_done_and_fetch().expect("done is accepted");
    let [merged] = <[GpuBatch; 1]>::try_from(emitted).expect("a merge emits one batch");
    let (answer, _) = session
        .export(&out)
        .unload(merged, RowRange::WHOLE)
        .expect("the rows cross the boundary");
    assert_eq!(
        by_key(&answer),
        vec![
            vec![
                string("a"),
                ScalarValue::Int64(Some(3)),
                ScalarValue::Float64(Some(4.0)),
                ScalarValue::Float64(Some(8.0))
            ],
            vec![
                string("b"),
                ScalarValue::Int64(Some(3)),
                ScalarValue::Float64(Some(3.0)),
                ScalarValue::Float64(Some(8.0))
            ],
        ],
        "the count-weighted mean and the cross term, across three partials"
    );
}

/// The same rule as the collapse's, in the two places it also applies: a merge of no runs
/// and a merge of no lanes are both the collapse of nothing under another name, and the
/// device refuses that. Neither may call.
#[test]
fn a_sort_and_a_partition_merge_that_received_nothing_emit_nothing() {
    let keys = vec![ColumnOrder {
        column: 1,
        ascending: true,
        nulls_first: false,
    }];
    let sorted = GpuAccumulateBatchesAndSort::new(source_per_row_group(), keys.clone(), None);
    let tree: Box<dyn GpuNode> = Box::new(sorted);
    let session = Session::open(tree.as_ref());
    let out = columns();
    let accumulator =
        GpuAccumulator::sorted(session.site(), session.recipe(1), &out).expect("the sort builds");
    let (emitted, _) = accumulator
        .mark_done_and_fetch()
        .expect("done is accepted whatever arrived");
    assert!(emitted.is_empty(), "a merge of no runs makes no call");

    let across = GpuSort::new(source_two_lanes(), keys.clone(), None);
    let tree: Box<dyn GpuNode> =
        Box::new(GpuMergeSortedPartitions::new(Box::new(across), keys, None));
    let session = Session::open(tree.as_ref());
    let mut merge =
        GpuPartitionAccumulator::merge_sorted(session.site(), session.recipe(2), 2, &out)
            .expect("the merge builds");
    let mut emitted = Vec::new();
    for lane in [0, 1] {
        let (produced, _) = merge
            .accumulate_and_fetch(lane, LaneEvent::Done)
            .expect("the done is accepted");
        emitted.extend(produced);
    }
    assert!(
        emitted.is_empty(),
        "a merge of no lanes makes no call either"
    );
}

/// A keyless `count(v), sum(v)` over `input`, and its state.
fn keyless_count_sum(input: Box<dyn GpuNode>) -> (Box<dyn GpuNode>, ArrowSchema) {
    let state = schema_of(&[("count(v)", DataType::Int64), ("sum(v)", DataType::Int64)]);
    let value = |func, out: &str| AggCall {
        func,
        args: vec![Expr::column(1, "v")],
        outputs: vec![Field::new(out, DataType::Int64, true)],
    };
    let node = GpuAggregate::new(
        input,
        AggregateBody {
            group_by: Vec::new(),
            grouping_sets: Vec::new(),
            null_exprs: Vec::new(),
            aggs: vec![
                value(PlanAgg::Count, "count(v)"),
                value(PlanAgg::Sum, "sum(v)"),
            ],
            finalize: None,
        },
        Schema::new(Arc::new(state.clone())),
        Schema::new(Arc::new(state.clone())),
    );
    (Box::new(node), state)
}

/// Over a filter that keeps nothing: the zero-row batch is released with no call, and done —
/// nothing having gone out — calls the init's seq with no input, answering count 0, sum NULL.
#[test]
fn a_keyless_init_releases_a_zero_row_batch_and_answers_its_identity_row_at_done() {
    let filter = GpuFilter::new(
        source(),
        greater_than(100),
        None,
        Schema::new(Arc::new(columns())),
    );
    let (tree, state) = keyless_count_sum(Box::new(filter));
    let session = Session::open(tree.as_ref());
    let _measuring = Measuring::on();
    let (nothing, _) = session
        .exec(1, &columns())
        .exec(session.scan(&ROW_GROUPS))
        .expect("the filter runs");
    assert_eq!(nothing.num_rows(), 0, "the filter kept nothing");
    let mut init = session.init(2, &state, &state);
    let (out, stats) = init.accumulate_and_fetch(nothing).expect("accepted");
    assert!(out.is_empty());
    assert_eq!(made(&stats).len(), 0, "released with no call");
    let (out, stats) = init.mark_done_and_fetch().expect("done is accepted");
    assert_eq!(made(&stats).len(), 1, "the init's seq, handed nothing");
    let [identity] = <[GpuBatch; 1]>::try_from(out).expect("one batch at done");
    let row = export_of(&session, identity, &state);
    assert_eq!(
        rows(&row),
        vec![vec![ScalarValue::Int64(Some(0)), ScalarValue::Int64(None)]]
    );
}

#[test]
fn a_keyless_init_that_answered_rows_makes_no_done_call() {
    let (tree, state) = keyless_count_sum(source());
    let session = Session::open(tree.as_ref());
    let _measuring = Measuring::on();
    let mut init = session.init(1, &state, &state);
    let (out, _) = init
        .accumulate_and_fetch(session.scan(&ROW_GROUPS))
        .expect("accepted");
    assert_eq!(out.len(), 1);
    let (out, stats) = init.mark_done_and_fetch().expect("done is accepted");
    assert!(out.is_empty());
    assert_eq!(made(&stats).len(), 0);
}

/// An init built from a recipe without its done calls is refused rather than built: driven, it
/// would answer nothing for a lane where nothing went out.
#[test]
fn an_init_whose_recipe_lacks_its_done_calls_is_refused() {
    let (tree, state) = keyless_count_sum(source());
    let session = Session::open(tree.as_ref());
    let per_batch = session.recipe(1).calls_under(CallPattern::PerBatch);
    let Err(refused) = GpuAccumulator::aggregate_init(session.site(), &per_batch, &state, &state)
    else {
        panic!("an init built from its per-batch calls alone");
    };
    assert!(
        refused
            .to_string()
            .contains("the same calls at done if nothing went out"),
        "{refused}"
    );
}
