//! The seqless three, and the helper round trip — the cases whose recipe puts nothing on
//! the wire, so a wrong helper has nowhere to hide.

use datafusion::arrow::datatypes::{DataType, Field, Schema as ArrowSchema};
use datafusion::arrow::record_batch::RecordBatch;

use super::device::Device;
use super::script::{Script, each_answers, run_both};
use crate::executor::RowRange;
use crate::plan::{BatchLayout, GpuLimit, GpuUnload, RowInterval, Schema};
use crate::tests::compare::{Order, assert_same};
use crate::tests::given::Given;
use crate::tests::synthetic::synthetic;

/// Any node over a given leaf opens a session; the unload's is the emptiest plan there is.
fn empty_plan() -> GpuUnload {
    GpuUnload::new(
        Given::of(
            Schema::new(synthetic(0, 0).schema()),
            BatchLayout::MultipleBatches,
        ),
        None,
    )
}

/// The context an executor is built from carries the plan the session loaded: an unload
/// over a given leaf is one stub node, and `open` checked the device counted the same.
#[test]
fn the_session_holds_the_plan_of_one_stub_it_loaded() {
    let device = Device::open(&empty_plan());
    assert_eq!(device.ctx().recipes.wire_nodes(), 1);
    assert!(
        device.ctx().recipes.get(0).is_none(),
        "the leaf has no recipe"
    );
    assert!(device.ctx().recipes.get(1).is_some(), "the unload has one");
}

#[test]
fn an_uploaded_batch_comes_back_as_itself() {
    let device = Device::open(&empty_plan());
    let batch = synthetic(64, 1);
    let back = device
        .fetch(device.upload(&batch), RowRange::WHOLE)
        .expect("a whole export ships");
    assert_same(&[vec![batch]], &[vec![back]], Order::AsEmitted);
}

#[test]
fn a_row_range_ships_those_rows_in_order() {
    let device = Device::open(&empty_plan());
    let batch = synthetic(64, 1);
    let back = device
        .fetch(
            device.upload(&batch),
            RowRange {
                offset: 10,
                length: 5,
            },
        )
        .expect("five rows ship");
    assert_same(&[vec![batch.slice(10, 5)]], &[vec![back]], Order::AsEmitted);
}

#[test]
fn a_range_past_the_end_ships_nothing() {
    let device = Device::open(&empty_plan());
    let back = device.fetch(
        device.upload(&synthetic(8, 1)),
        RowRange {
            offset: 8,
            length: 1,
        },
    );
    assert!(back.is_none());
}

#[test]
fn a_range_over_the_end_is_clamped() {
    let device = Device::open(&empty_plan());
    let batch = synthetic(8, 1);
    let back = device
        .fetch(
            device.upload(&batch),
            RowRange {
                offset: 6,
                length: 100,
            },
        )
        .expect("two rows ship");
    assert_same(&[vec![batch.slice(6, 2)]], &[vec![back]], Order::AsEmitted);
}

#[test]
fn zero_rows_round_trip_as_zero_rows_under_the_schema() {
    let device = Device::open(&empty_plan());
    let batch = synthetic(0, 1);
    let back = device
        .fetch(device.upload(&batch), RowRange::WHOLE)
        .expect("the schema ships");
    assert_same(&[vec![batch]], &[vec![back]], Order::AsEmitted);
}

// `GpuUnload` through `executors_for` on both backends: `CpuUnload`'s slice against
// `GpuExport`'s row range, the first operator through `run_both`. The round trip above
// and the refusal below are about the harness, not a node kind, and stay plain tests.

fn unload_over(rows: usize) -> (GpuUnload, RecordBatch) {
    let batch = synthetic(rows, 2);
    let node = GpuUnload::new(
        Given::of(Schema::new(batch.schema()), BatchLayout::MultipleBatches),
        None,
    );
    (node, batch)
}

/// A batch whose exported types are not the sink's: the device holds one string layout and
/// exports it as `Utf8`, so declaring `s` as `LargeUtf8` — a type the plan may carry — is a
/// divergence the sink refuses, naming the column with its index and both types. The cpu
/// holds to the declaration and answers, so the refusal is one-sided by construction.
operator_case! {
    GpuUnload,
    fn the_sink_names_a_column_whose_exported_type_is_not_the_declared_one() {
        let batch = synthetic(8, 2);
        let fields: Vec<Field> = batch
            .schema()
            .fields()
            .iter()
            .map(|f| match f.name().as_str() {
                "s" => Field::new("s", DataType::LargeUtf8, f.is_nullable()),
                _ => f.as_ref().clone(),
            })
            .collect();
        let node = GpuUnload::new(
            Given::of(
                Schema::new(std::sync::Arc::new(ArrowSchema::new(fields))),
                BatchLayout::MultipleBatches,
            ),
            None,
        );
        let outcome = run_both(
            &node,
            Script::Unload {
                batch,
                rows: RowRange::WHOLE,
            },
        );
        let why = outcome.gpu_refuses();
        assert!(
            why.contains("the exported stream is not the sink's rows"),
            "{why}"
        );
        assert!(
            why.contains("(declared vs exported: 5 s: LargeUtf8 vs Utf8)"),
            "{why}"
        );
    }
}

#[test]
#[should_panic(expected = "the script's shape is not the node's category")]
fn a_script_of_another_shape_is_refused_before_either_backend_runs() {
    let (node, batch) = unload_over(4);
    run_both(&node, Script::Exec(vec![batch]));
}

operator_case! {
    GpuUnload,
    fn an_unload_hands_the_whole_batch_over_on_both_backends() {
        let (node, batch) = unload_over(64);
        run_both(
            &node,
            Script::Unload {
                batch,
                rows: RowRange::WHOLE,
            },
        )
        .same(Order::AsEmitted);
    }
}

operator_case! {
    GpuUnload,
    fn an_unload_over_a_range_hands_those_rows_over() {
        let (node, batch) = unload_over(64);
        run_both(
            &node,
            Script::Unload {
                batch,
                rows: RowRange {
                    offset: 20,
                    length: 7,
                },
            },
        )
        .same(Order::AsEmitted);
    }
}

operator_case! {
    GpuUnload,
    fn an_unload_clamps_a_range_over_the_end_the_same_way() {
        let (node, batch) = unload_over(64);
        run_both(
            &node,
            Script::Unload {
                batch,
                rows: RowRange {
                    offset: 60,
                    length: 100,
                },
            },
        )
        .same(Order::AsEmitted);
    }
}

operator_case! {
    GpuUnload,
    fn an_unload_of_a_range_past_the_end_is_zero_rows_on_both() {
        let (node, batch) = unload_over(8);
        run_both(
            &node,
            Script::Unload {
                batch,
                rows: RowRange {
                    offset: 8,
                    length: 4,
                },
            },
        )
        .same(Order::AsEmitted);
    }
}

// Empty inputs, each its own case.
operator_case! {
    GpuUnload,
    fn an_unload_of_a_zero_row_batch_is_zero_rows_under_the_schema_on_both() {
        let (node, batch) = unload_over(0);
        run_both(
            &node,
            Script::Unload {
                batch,
                rows: RowRange::WHOLE,
            },
        )
        .same(Order::AsEmitted);
    }
}

operator_case! {
    GpuUnload,
    fn a_range_over_a_zero_row_batch_is_zero_rows_on_both() {
        let (node, batch) = unload_over(0);
        run_both(
            &node,
            Script::Unload {
                batch,
                rows: RowRange {
                    offset: 0,
                    length: 5,
                },
            },
        )
        .same(Order::AsEmitted);
    }
}

// `GpuLimit` through `executors_for`: the mid-plan limit streams and holds nothing, so
// one slot per batch and an empty one at done — a batch outside the interval is a slot
// both sides leave empty, which is not a zero-row batch.

/// A limit over a stream of `batches` batches of `rows` rows each. The schema is the
/// fixture's, which every `synthetic` shares, so a stream of no batches is a legal one.
fn limit_over(
    skip: u64,
    fetch: Option<u64>,
    rows: usize,
    batches: usize,
) -> (GpuLimit, Vec<RecordBatch>) {
    let stream: Vec<RecordBatch> = (0..batches)
        .map(|i| synthetic(rows, 10 + i as u64))
        .collect();
    let node = GpuLimit::new(
        Given::of(
            Schema::new(synthetic(0, 0).schema()),
            BatchLayout::MultipleBatches,
        ),
        RowInterval { skip, fetch },
    );
    (node, stream)
}

operator_case! {
    GpuLimit,
    fn an_interval_inside_one_batch_slices_that_batch() {
        let (node, stream) = limit_over(3, Some(4), 16, 1);
        run_both(&node, Script::Accumulate(stream)).same(Order::AsEmitted);
    }
}

operator_case! {
    GpuLimit,
    fn an_interval_straddling_two_batches_slices_both() {
        let (node, stream) = limit_over(12, Some(8), 16, 2);
        run_both(&node, Script::Accumulate(stream)).same(Order::AsEmitted);
    }
}

operator_case! {
    GpuLimit,
    fn batches_entirely_outside_the_interval_produce_nothing() {
        let (node, stream) = limit_over(40, Some(4), 16, 4);
        run_both(&node, Script::Accumulate(stream)).same(Order::AsEmitted);
    }
}

operator_case! {
    GpuLimit,
    fn a_skip_alone_drops_the_prefix_and_keeps_the_rest() {
        let (node, stream) = limit_over(20, None, 16, 3);
        run_both(&node, Script::Accumulate(stream)).same(Order::AsEmitted);
    }
}

operator_case! {
    GpuLimit,
    fn a_stream_of_several_batches_is_cut_at_the_same_two_edges() {
        let (node, stream) = limit_over(5, Some(30), 8, 6);
        run_both(&node, Script::Accumulate(stream)).same(Order::AsEmitted);
    }
}

// Empty inputs, each its own case.
operator_case! {
    GpuLimit,
    fn a_stream_of_no_batches_answers_nothing_on_both() {
        let (node, stream) = limit_over(0, Some(4), 8, 0);
        run_both(&node, Script::Accumulate(stream)).same(Order::AsEmitted);
    }
}

// #214 — `range_of` reads a zero-row batch as outside every interval, so both sides release
// it instead of emitting it under the schema. Wanted: one zero-row batch each.
operator_case! {
    GpuLimit,
    fn bug_a_stream_of_one_zero_row_batch_is_dropped_on_both() {
        let (node, stream) = limit_over(0, Some(4), 0, 1);
        let outcome = run_both(&node, Script::Accumulate(stream));
        // One slot per batch, one for the finish; wanted: `vec![synthetic(0, 10)]` in the first.
        each_answers(&outcome, &[vec![], vec![]], &[vec![], vec![]]);
    }
}

// #214 — the same drop mid-stream: the batch counts no rows, which is right, and is not
// emitted, which is not. Wanted: the zero-row batch in slot 1 on both sides.
operator_case! {
    GpuLimit,
    fn bug_a_zero_row_batch_inside_a_stream_is_dropped_on_both() {
        let (node, mut stream) = limit_over(10, Some(10), 8, 3);
        stream.insert(1, synthetic(0, 99));
        let expected = vec![
            vec![],
            vec![], // wanted: vec![synthetic(0, 99)]
            vec![stream[2].slice(2, 6)],
            vec![stream[3].slice(0, 4)],
            vec![], // the finish
        ];
        let outcome = run_both(&node, Script::Accumulate(stream));
        each_answers(&outcome, &expected, &expected);
    }
}

// #214 — three zero-row batches, none emitted. Wanted: three zero-row batches each.
operator_case! {
    GpuLimit,
    fn bug_a_stream_of_nothing_but_zero_row_batches_is_dropped_on_both() {
        let (node, stream) = limit_over(2, Some(4), 0, 3);
        let outcome = run_both(&node, Script::Accumulate(stream));
        let expected = [vec![], vec![], vec![], vec![]]; // three batches and the finish
        each_answers(&outcome, &expected, &expected);
    }
}

operator_case! {
    GpuLimit,
    fn an_interval_no_batch_reaches_answers_nothing_on_both() {
        let (node, stream) = limit_over(1000, Some(4), 8, 3);
        run_both(&node, Script::Accumulate(stream)).same(Order::AsEmitted);
    }
}
