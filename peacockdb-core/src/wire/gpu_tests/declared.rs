//! The schema catalog: what each call of a planned query declares against what the device
//! exported for its firing, one named test per query, at the mode the spec names
//! (`declared-schemas.md`, "The queries"). A `bug_` test asserts the observed value with
//! its ticket above it, one per divergence class rather than per node, and is deleted by
//! the change that fixes it. Precision and nullability are the exporter's own rewrites —
//! every decimal exports at 38 and the flag is `has_nulls()` — so the cases that touch
//! them record a limitation of the instrument, not a divergence. No case loops over every
//! firing asserting agreement: that assertion cannot be green beside the `bug_` cases.
//!
//! The two shapes that never reach a device are `wire/tests/refusals.rs`.

use std::collections::BTreeSet;

use datafusion::arrow::datatypes::{DECIMAL128_MAX_PRECISION, DataType, TimeUnit};

use super::super::FbKind;
use super::walk::{Firing, walk};
use super::{ONE_LANE, TWO_LANES};
use crate::planner::{BatchSizing, PlanKnobs, SMALL_TABLE_BYTES};
use crate::test_support::{GPU_BUDGET, total_rows};

/// `tp1-rowgroup`: one lane, one batch per row group — the mode that fires one call
/// several times, which `tp1-single` cannot.
const ROWGROUP: PlanKnobs = PlanKnobs {
    target_partitions: 1,
    sizing: BatchSizing::OneBatchPerRowGroup,
    budget: GPU_BUDGET as u64,
    small_table_bytes: SMALL_TABLE_BYTES,
};

/// The sort arm's shape, shared with `mod.rs`'s guard over the kinds a device has run.
pub(crate) const SORTED_KEYS: &str = "SELECT n_nationkey FROM nation ORDER BY n_nationkey";

async fn firings(sql: &str) -> Vec<Firing> {
    walk(sql, ONE_LANE).await.firings
}

/// For every column any firing declared as `declared`: the firing's label and the type the
/// device exported at that position. A refused export panics naming the firing — a
/// capability gap is its own ignored test, never a silent skip here.
fn exported_for(firings: &[Firing], declared: &DataType) -> Vec<(String, DataType)> {
    firings
        .iter()
        .flat_map(|firing| {
            let exported = firing
                .exported
                .as_ref()
                .unwrap_or_else(|error| panic!("{}: {error}", firing.label()));
            assert_eq!(
                exported.fields().len(),
                firing.declared.fields().len(),
                "{}: arity",
                firing.label()
            );
            firing
                .declared
                .fields()
                .iter()
                .zip(exported.fields())
                .filter(|(field, _)| field.data_type() == declared)
                .map(|(_, field)| (firing.label(), field.data_type().clone()))
                .collect::<Vec<_>>()
        })
        .collect()
}

/// The exported type is `exported` wherever `declared` was declared, and somewhere was:
/// without the second half a plan that stopped declaring the type would pass asserting
/// nothing, which is the failure a `bug_` test is most prone to.
fn crosses_as(firings: &[Firing], declared: &DataType, exported: &DataType) {
    let seen = exported_for(firings, declared);
    assert!(
        !seen.is_empty(),
        "no call declared {declared}, so the query no longer reaches the class — the plan \
         changed, not the device"
    );
    for (label, got) in seen {
        assert_eq!(&got, exported, "{label}: declared {declared}");
    }
}

/// The one declared firing of `kind` — the arm under test — with its declaration and the
/// export agreeing by name and type. Exactly one, so a shape that fired the kind twice or
/// not at all fails here rather than passing on the wrong firing.
fn the_one_firing_of(firings: &[Firing], kind: FbKind) -> &Firing {
    let of_kind: Vec<&Firing> = firings
        .iter()
        .filter(|firing| firing.target.is_some_and(|(_, made)| made == kind))
        .collect();
    let [firing] = of_kind.as_slice() else {
        panic!("{} declared firings of {kind}, expected one", of_kind.len())
    };
    let (declared, exported) = firing
        .declared_vs_exported()
        .unwrap_or_else(|| panic!("{} refused the export", firing.label()));
    assert_eq!(declared, exported, "{}", firing.label());
    firing
}

/// Every firing's names, in order, are the declared ones.
fn names_cross(firings: &[Firing]) {
    for firing in firings {
        let exported = firing
            .exported
            .as_ref()
            .unwrap_or_else(|error| panic!("{}: {error}", firing.label()));
        let names = |fields: &datafusion::arrow::datatypes::Fields| -> Vec<String> {
            fields.iter().map(|field| field.name().clone()).collect()
        };
        assert_eq!(
            names(exported.fields()),
            names(firing.declared.fields()),
            "{}",
            firing.label()
        );
    }
}

// Query 1. [#183](../../../../llm-wiki/tasks/active-tickets.md#t183): cuDF has one string
// type, so a declared `Utf8View` comes back `Utf8`. It enters at the scan and every node
// above inherits it, so this is one test for the class, asserted at every one of the six
// declared node kinds — three shapes reach all of them. Delete this test in the change
// that fixes #183.
#[tokio::test]
async fn bug_a_declared_utf8view_is_exported_as_utf8() {
    let mut seen = firings("SELECT n_name FROM nation").await;
    assert!(
        seen[0].label().ends_with("CudfScan"),
        "the class is expected to enter at the scan: {}",
        seen[0].label()
    );
    seen.extend(
        firings("SELECT n_name AS label FROM nation WHERE n_nationkey > 3 ORDER BY n_name").await,
    );
    seen.extend(
        walk(
            "SELECT l_returnflag, sum(l_quantity) FROM lineitem GROUP BY l_returnflag",
            TWO_LANES,
        )
        .await
        .firings,
    );
    crosses_as(&seen, &DataType::Utf8View, &DataType::Utf8);
    let kinds: BTreeSet<String> = exported_for(&seen, &DataType::Utf8View)
        .into_iter()
        .map(|(label, _)| {
            label
                .split_once(' ')
                .map_or(label.clone(), |(_, kind)| kind.into())
        })
        .collect();
    let every_declared_kind = [
        "CudfScan",
        "CudfFilter",
        "CudfProject",
        "CudfSort",
        "CudfCoalescePartitions",
        "result_from_handle",
    ];
    for kind in every_declared_kind {
        assert!(
            kinds.contains(kind),
            "no {kind} firing declared a Utf8View: {kinds:?}"
        );
    }
}

// Query 2. [#187](../../../../llm-wiki/tasks/active-tickets.md#t187): a `Decimal128(15,2)`
// exports as `(38,2)`. The cause is our exporter's, not cuDF's: a cuDF decimal carries a scale
// and no precision, the exporter's `column_metadata` has nowhere to put one, so
// `to_arrow_schema` writes `max_precision` for every decimal. What precision cuDF holds has
// no answer through this export. Delete this test in the change that fixes #187.
#[tokio::test]
async fn bug_a_narrow_decimal_is_exported_at_precision_38() {
    let firings = firings("SELECT l_extendedprice FROM lineitem WHERE l_orderkey = 1").await;
    crosses_as(
        &firings,
        &DataType::Decimal128(15, 2),
        &DataType::Decimal128(DECIMAL128_MAX_PRECISION, 2),
    );
    // With precision set aside, the scale and everything else cross.
    for firing in &firings {
        let (declared, exported) = firing.declared_vs_exported().expect("exported");
        assert_eq!(declared, exported, "{}", firing.label());
    }
}

// Query 3. A decimal already at 38 agrees with its export, and for the wrong reason: 38 is
// what the exporter writes for any decimal (query 2), so this row is no evidence that the
// device kept the declared precision. It exists so the catalog says so.
#[tokio::test]
async fn a_decimal_declared_at_max_precision_agrees_for_the_exporters_reason() {
    let firings =
        firings("SELECT CAST(l_extendedprice AS DECIMAL(38,4)) FROM lineitem WHERE l_orderkey = 1")
            .await;
    crosses_as(
        &firings,
        &DataType::Decimal128(38, 4),
        &DataType::Decimal128(38, 4),
    );
}

// Query 4. `Date32` goes to `TIMESTAMP_DAYS` and back.
#[tokio::test]
async fn a_date32_survives_the_crossing() {
    let firings = firings("SELECT l_shipdate FROM lineitem WHERE l_orderkey = 1").await;
    crosses_as(&firings, &DataType::Date32, &DataType::Date32);
}

// Query 5. Integer identity. This parquet's `l_linenumber` is `Int64`, so `Int32` identity
// is what queries 7, 9 and 10 pin on `n_nationkey`.
#[tokio::test]
async fn an_int64_survives_the_crossing() {
    let firings =
        firings("SELECT l_orderkey, l_linenumber FROM lineitem WHERE l_orderkey = 1").await;
    crosses_as(&firings, &DataType::Int64, &DataType::Int64);
}

// Query 6. The zero-row batch: an empty string column exports under the type a populated
// one does. The predicate is arithmetic because `n_nationkey < 0` prunes every row group
// and the planner refuses the scan ([#209](../../../../llm-wiki/tickets.md#t209)).
#[tokio::test]
async fn a_zero_row_batch_types_its_string_column_as_a_populated_one() {
    let empty = walk(
        "SELECT n_name FROM nation WHERE n_nationkey + 100 < 0",
        ONE_LANE,
    )
    .await;
    assert_eq!(total_rows(&empty.batches), 0);
    let populated = walk("SELECT n_name FROM nation", ONE_LANE).await;
    let at_sink = |firings: &[Firing]| {
        let sink = firings.last().expect("the export fired");
        assert_eq!(sink.label(), "result_from_handle");
        sink.exported.clone().expect("exported")
    };
    assert_eq!(at_sink(&empty.firings), at_sink(&populated.firings));
}

// Query 7. Names survive the crossing. Declared against device only: the cpu relabels
// names to the declaration, so it has no opinion to compare against.
#[tokio::test]
async fn column_names_survive_the_crossing() {
    let firings = firings("SELECT n_name AS label, n_nationkey AS id FROM nation").await;
    names_cross(&firings);
    assert!(firings.iter().any(|firing| {
        firing
            .declared
            .fields()
            .iter()
            .map(|f| f.name().as_str())
            .eq(["label", "id"])
    }));
}

// Query 8. Nullability, and what the export can say about it: the flag is `has_nulls()`,
// so a column declared nullable exports non-nullable when the batch happens to hold no
// null. The limitation is the flag, not the data — and `RecordBatch::try_new` refuses null
// values under a non-nullable field, so the opposite direction is what would be caught.
#[tokio::test]
async fn nullability_is_read_off_the_data_rather_than_the_declaration() {
    let firings = firings(
        "SELECT n_nationkey, CASE WHEN n_nationkey > 10 THEN n_name END AS maybe FROM nation",
    )
    .await;
    let sink = firings.last().expect("the export fired");
    let exported = sink.exported.as_ref().expect("exported");
    let declared = &sink.declared;
    assert!(declared.field(0).is_nullable() && declared.field(1).is_nullable());
    assert!(
        !exported.field(0).is_nullable(),
        "no key is null in the data"
    );
    assert!(
        exported.field(1).is_nullable(),
        "fourteen of twenty-five rows are null"
    );
}

// Query 9. Order and arity: the project reorders the two keys the scan reads, and the
// export answers with the same two in the same order at every node.
#[tokio::test]
async fn column_order_and_arity_survive_the_crossing() {
    let firings = firings("SELECT n_regionkey, n_nationkey FROM nation").await;
    names_cross(&firings);
    let sink = firings.last().expect("the export fired");
    assert!(
        sink.declared
            .fields()
            .iter()
            .map(|f| f.name().as_str())
            .eq(["n_regionkey", "n_nationkey"])
    );
}

// Query 10. Fixed-width cast targets. Aliased because DataFusion refuses two casts of one
// column under one generated name.
#[tokio::test]
async fn fixed_width_cast_targets_survive_the_crossing() {
    let firings = firings(
        "SELECT CAST(n_nationkey AS BIGINT) AS wide, CAST(n_nationkey AS DOUBLE) AS real FROM nation",
    )
    .await;
    crosses_as(&firings, &DataType::Int64, &DataType::Int64);
    crosses_as(&firings, &DataType::Float64, &DataType::Float64);
    crosses_as(&firings, &DataType::Int32, &DataType::Int32);
}

// The sort arm, under the accumulator every `ORDER BY` plans above it: `GpuSort`'s own
// `CudfSort` fires per batch and is declared; the accumulator's is not. An integer key, so
// the identity is measured outside #183's class.
#[tokio::test]
async fn a_sort_survives_the_crossing() {
    let firings = firings(SORTED_KEYS).await;
    the_one_firing_of(&firings, FbKind::Sort);
    crosses_as(&firings, &DataType::Int32, &DataType::Int32);
}

// The coalesce-all arm, at the two lanes a shuffle needs: the collapse below the
// repartition is declared, the aggregate's own concats are not. An integer key for the
// same reason; the sum's `Decimal128(25,2)` crosses at 38, which `columns` sets aside.
#[tokio::test]
async fn a_coalesce_all_survives_the_crossing() {
    let walked = walk(
        "SELECT l_linenumber, sum(l_quantity) FROM lineitem GROUP BY l_linenumber",
        TWO_LANES,
    )
    .await;
    let firing = the_one_firing_of(&walked.firings, FbKind::CoalescePartitions);
    assert!(
        firing
            .declared
            .fields()
            .iter()
            .any(|f| f.data_type() == &DataType::Int64),
        "the key crosses the collapse: {:?}",
        firing.declared
    );
}

// Query 11. [#203](../../../../llm-wiki/tickets.md#t203): the device refuses the cast to
// text at the project, so no handle ever reaches the export and the class cannot be
// measured. A capability gap, not a divergence — ignored rather than `bug_`.
#[tokio::test]
#[ignore = "#203: the device refuses a cast to text at execute_node, so nothing is exported"]
async fn a_cast_to_text_cannot_be_measured_until_the_device_answers_it() {
    let firings = firings("SELECT CAST(n_nationkey AS VARCHAR) FROM nation").await;
    crosses_as(&firings, &DataType::Utf8, &DataType::Utf8);
}

// Query 12. [#191](../../../../llm-wiki/tasks/active-tickets.md#t191): `extract(year)` is
// declared `Int32` and the device answers `Int16` — cuDF's `extract_year` output — from the
// project on. Delete this test in the change that fixes #191.
#[tokio::test]
async fn bug_an_extracted_year_declared_int32_is_exported_as_int16() {
    let firings = firings("SELECT extract(year FROM o_orderdate) FROM orders").await;
    crosses_as(&firings, &DataType::Int32, &DataType::Int16);
    crosses_as(&firings, &DataType::Date32, &DataType::Date32);
}

// Query 13. [#200](../../../../llm-wiki/tickets.md#t200): a `Date64` goes to
// `TIMESTAMP_MILLISECONDS` and comes back `Timestamp(ms)`, a type the wire cannot name —
// neither cast nor refused. No corpus column is a `Date64`, so the cast makes one. Delete
// this test in the change that fixes #200.
#[tokio::test]
async fn bug_a_date64_is_exported_as_a_millisecond_timestamp() {
    let firings =
        firings("SELECT arrow_cast(l_shipdate, 'Date64') FROM lineitem WHERE l_orderkey = 1").await;
    crosses_as(
        &firings,
        &DataType::Date64,
        &DataType::Timestamp(TimeUnit::Millisecond, None),
    );
}

// Query 15 (14 is a plan-time refusal, `wire/tests/refusals.rs`). The firings of one call
// agree with each other: at `tp1-rowgroup` orders is one batch per surviving row group —
// eleven under this predicate — so the scan, the filter and the export each fire that many
// times, and every firing of a call exports the schema its first did.
#[tokio::test]
async fn the_firings_of_one_call_export_one_schema() {
    let walked = walk(
        "SELECT o_orderkey, o_totalprice FROM orders WHERE o_totalprice > 500000",
        ROWGROUP,
    )
    .await;
    let labels: BTreeSet<String> = walked.firings.iter().map(Firing::label).collect();
    assert_eq!(
        labels.len(),
        3,
        "a scan, a filter and the export: {labels:?}"
    );
    for label in labels {
        let of_call: Vec<_> = walked
            .firings
            .iter()
            .filter(|f| f.label() == label)
            .map(|f| {
                f.declared_vs_exported()
                    .unwrap_or_else(|| panic!("{label} refused the export"))
            })
            .collect();
        assert!(
            of_call.len() > 1,
            "{label} fired once; the mode should give it a batch per row group"
        );
        // By name and type, so a row group that happens to hold a null does not read as a
        // firing disagreeing with its call: the flag is the exporter's, not the device's.
        for (declared, exported) in &of_call {
            assert_eq!(declared, exported, "{label}");
            assert_eq!(exported, &of_call[0].1, "{label}: the firings disagree");
        }
    }
}
