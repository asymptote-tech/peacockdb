//! One batch every operator case starts from: eight typed columns, a null in each but the
//! id, values a reader can predict from the row number, and floats that are dyadic so a
//! sum of them is exact on both engines. Decimals are a second batch, because the device
//! exports every decimal at precision 38 (#187) and one column would make every case that
//! ticket's `bug_` test.

use std::sync::Arc;

use datafusion::arrow::array::{
    ArrayRef, BooleanArray, Date32Array, Decimal128Array, Float64Array, Int32Array, Int64Array,
    StringArray,
};
use datafusion::arrow::datatypes::{DataType, Field, Schema as ArrowSchema};
use datafusion::arrow::record_batch::RecordBatch;

/// Column order is the ordinal every case's expressions use, so it is stated once.
pub(crate) fn schema() -> Arc<ArrowSchema> {
    Arc::new(ArrowSchema::new(vec![
        Field::new("id", DataType::Int64, false),
        Field::new("key", DataType::Int32, true),
        Field::new("i32", DataType::Int32, true),
        Field::new("i64", DataType::Int64, true),
        Field::new("f64", DataType::Float64, true),
        Field::new("s", DataType::Utf8, true),
        Field::new("d", DataType::Date32, true),
        Field::new("b", DataType::Boolean, true),
    ]))
}

/// splitmix64: a seed and a row number give one value, with no crate behind it.
fn mix(seed: u64, row: u64) -> u64 {
    let mut z = seed.wrapping_add(row.wrapping_mul(0x9E37_79B9_7F4A_7C15));
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// `rows` rows from `seed`. `id` is `0..rows` and never null; every other column is null
/// where `row % stride == stride - 1`, one stride per column, so no row is all null and no
/// two columns share their null rows. `key` takes seven values, so it has duplicates for a
/// group or a join and every lane of a 4-way scatter receives something.
pub(crate) fn synthetic(rows: usize, seed: u64) -> RecordBatch {
    let at = |row: usize| mix(seed, row as u64);
    let null_at = |row: usize, stride: usize| row % stride == stride - 1;
    let id: ArrayRef = Arc::new(Int64Array::from_iter_values((0..rows).map(|r| r as i64)));
    let key: ArrayRef = Arc::new(Int32Array::from_iter(
        (0..rows).map(|r| (!null_at(r, 11)).then(|| (at(r) % 7) as i32)),
    ));
    let i32s: ArrayRef = Arc::new(Int32Array::from_iter(
        (0..rows).map(|r| (!null_at(r, 5)).then(|| (at(r) % 1000) as i32 - 500)),
    ));
    let i64s: ArrayRef =
        Arc::new(Int64Array::from_iter((0..rows).map(|r| {
            (!null_at(r, 7)).then(|| (at(r) % 1_000_000) as i64 - 500_000)
        })));
    let f64s: ArrayRef =
        Arc::new(Float64Array::from_iter((0..rows).map(|r| {
            (!null_at(r, 13)).then(|| ((at(r) % 4096) as f64 - 2048.0) / 8.0)
        })));
    let words = ["alpha", "beta", "gamma", "delta", "", "epsilon"];
    let strings: ArrayRef = Arc::new(StringArray::from_iter((0..rows).map(|r| {
        (!null_at(r, 3)).then(|| words[(at(r) % words.len() as u64) as usize].to_string())
    })));
    let dates: ArrayRef = Arc::new(Date32Array::from_iter(
        (0..rows).map(|r| (!null_at(r, 19)).then(|| (at(r) % 20_000) as i32)),
    ));
    let bools: ArrayRef = Arc::new(BooleanArray::from_iter(
        (0..rows).map(|r| (!null_at(r, 23)).then(|| at(r) % 2 == 0)),
    ));
    RecordBatch::try_new(
        schema(),
        vec![id, key, i32s, i64s, f64s, strings, dates, bools],
    )
    .expect("eight arrays of one length under the schema that declares them")
}

/// `[id, dec]` from the same generator, for the decimal cases alone.
pub(crate) fn decimals(rows: usize, seed: u64) -> RecordBatch {
    let at = |row: usize| mix(seed, row as u64);
    let id: ArrayRef = Arc::new(Int64Array::from_iter_values((0..rows).map(|r| r as i64)));
    let decs: ArrayRef = Arc::new(
        Decimal128Array::from_iter(
            (0..rows).map(|r| (r % 17 != 16).then(|| (at(r) % 10_000_000) as i128 - 5_000_000)),
        )
        .with_precision_and_scale(18, 2)
        .expect("18,2 holds every value above"),
    );
    RecordBatch::try_new(
        Arc::new(ArrowSchema::new(vec![
            Field::new("id", DataType::Int64, false),
            Field::new("dec", DataType::Decimal128(18, 2), true),
        ])),
        vec![id, decs],
    )
    .expect("two arrays of one length")
}

/// The same columns under `prefix`-ed names — a join's two sides over one batch shape
/// need distinct names, and a join's declared output cannot carry `id` twice.
pub(crate) fn prefixed(batch: &RecordBatch, prefix: &str) -> RecordBatch {
    let fields: Vec<Field> = batch
        .schema()
        .fields()
        .iter()
        .map(|f| {
            Field::new(
                format!("{prefix}{}", f.name()),
                f.data_type().clone(),
                f.is_nullable(),
            )
        })
        .collect();
    RecordBatch::try_new(Arc::new(ArrowSchema::new(fields)), batch.columns().to_vec())
        .expect("a rename keeps every array under its type")
}

#[test]
fn a_synthetic_batch_is_the_same_batch_twice() {
    assert_eq!(synthetic(50, 7), synthetic(50, 7));
    assert_ne!(synthetic(50, 7), synthetic(50, 8));
}

#[test]
fn every_column_but_id_carries_a_null_and_id_carries_none() {
    let batch = synthetic(64, 1);
    for (index, field) in batch.schema().fields().iter().enumerate() {
        let nulls = batch.column(index).null_count();
        if field.name() == "id" {
            assert_eq!(nulls, 0, "id is the tie-breaker and is never null");
        } else {
            assert!(nulls > 0, "{} has no null in 64 rows", field.name());
        }
    }
}

#[test]
fn zero_rows_is_a_batch_with_the_schema_and_nothing_else() {
    let batch = synthetic(0, 1);
    assert_eq!(batch.num_rows(), 0);
    assert_eq!(batch.schema().fields().len(), 8);
}

#[test]
fn decimals_carry_an_id_and_a_decimal_with_nulls() {
    let batch = decimals(64, 1);
    assert_eq!(
        batch.schema().field(1).data_type(),
        &DataType::Decimal128(18, 2)
    );
    assert_eq!(batch.column(0).null_count(), 0);
    assert!(batch.column(1).null_count() > 0);
}

#[test]
fn prefixing_renames_every_column_and_touches_no_value() {
    let batch = synthetic(5, 3);
    let renamed = prefixed(&batch, "r_");
    assert_eq!(renamed.schema().field(0).name(), "r_id");
    assert_eq!(renamed.columns(), batch.columns());
}
