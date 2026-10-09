//! One batch every operator case starts from: eight typed columns, a null in each but the
//! id, values a reader can predict from the row number, and floats that are dyadic so a
//! sum of them is exact on both engines. Decimals are a second batch, so a case is about a
//! decimal only when it says so.
//!
//! `key_types` is a third: one column per hashable key type, for the cases that are about
//! which lane a key lands in rather than about an operator's arithmetic.

use std::sync::Arc;

use datafusion::arrow::array::{
    Array, ArrayRef, AsArray, BooleanArray, Date32Array, Decimal128Array, Float32Array,
    Float64Array, Int8Array, Int16Array, Int32Array, Int64Array, StringArray,
    TimestampMicrosecondArray, TimestampMillisecondArray, TimestampNanosecondArray,
    TimestampSecondArray, UInt8Array, UInt16Array, UInt32Array, UInt64Array,
};
use datafusion::arrow::datatypes::{
    DataType, Decimal128Type, Field, Float64Type, Schema as ArrowSchema, TimeUnit, UInt32Type,
    UInt64Type,
};
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

/// One column per hashable key type, in the ordinal order every key-type case names. The
/// unsigned four are last so the others keep their ordinals as the set grows.
pub(crate) fn key_types_schema() -> Arc<ArrowSchema> {
    Arc::new(ArrowSchema::new(vec![
        Field::new("id", DataType::Int64, false),
        Field::new("k8", DataType::Int8, true),
        Field::new("k16", DataType::Int16, true),
        Field::new("k32", DataType::Int32, true),
        Field::new("k64", DataType::Int64, true),
        Field::new("f32", DataType::Float32, true),
        Field::new("f64", DataType::Float64, true),
        Field::new("b", DataType::Boolean, true),
        Field::new("d", DataType::Date32, true),
        Field::new("ts_s", DataType::Timestamp(TimeUnit::Second, None), true),
        Field::new(
            "ts_ms",
            DataType::Timestamp(TimeUnit::Millisecond, None),
            true,
        ),
        Field::new(
            "ts_us",
            DataType::Timestamp(TimeUnit::Microsecond, None),
            true,
        ),
        Field::new(
            "ts_ns",
            DataType::Timestamp(TimeUnit::Nanosecond, None),
            true,
        ),
        Field::new("dec15", DataType::Decimal128(15, 2), true),
        Field::new("dec38", DataType::Decimal128(38, 4), true),
        Field::new("s", DataType::Utf8, true),
        Field::new("u8", DataType::UInt8, true),
        Field::new("u16", DataType::UInt16, true),
        Field::new("u32", DataType::UInt32, true),
        Field::new("u64", DataType::UInt64, true),
    ]))
}

/// The float specials, in the order the generator cycles them. `-NaN` is x86's `0/0`, so it
/// reaches a real column rather than only a synthetic one.
fn float_specials_f64() -> [f64; 4] {
    [
        0.0,
        -0.0,
        f64::NAN,
        f64::from_bits(f64::NAN.to_bits() | (1 << 63)),
    ]
}

/// `rows` rows from `seed`, one column per hashable key type. `id` is `0..rows` and never
/// null; every other column is null at its own stride. Every fifth row of `f32`/`f64` is one
/// of the four specials, so a float case meets `-0.0` and both NaN signs without constructing
/// a column by hand. `u32` and `u64` carry values past their signed maximum, and `dec38`
/// values past `i64`, because those are the widths the two engines have to agree on.
pub(crate) fn key_types(rows: usize, seed: u64) -> RecordBatch {
    let at = |row: usize| mix(seed, row as u64);
    let null_at = |row: usize, stride: usize| row % stride == stride - 1;
    let specials = float_specials_f64();
    let special_f64 = |r: usize| specials[(r / 5) % 4];

    let id: ArrayRef = Arc::new(Int64Array::from_iter_values((0..rows).map(|r| r as i64)));
    let k8: ArrayRef = Arc::new(Int8Array::from_iter(
        (0..rows).map(|r| (!null_at(r, 11)).then(|| (at(r) % 251) as i8)),
    ));
    let k16: ArrayRef = Arc::new(Int16Array::from_iter(
        (0..rows).map(|r| (!null_at(r, 7)).then(|| (at(r) % 60_000) as i16)),
    ));
    let k32: ArrayRef = Arc::new(Int32Array::from_iter(
        (0..rows).map(|r| (!null_at(r, 5)).then(|| (at(r) % 1000) as i32 - 500)),
    ));
    let k64: ArrayRef =
        Arc::new(Int64Array::from_iter((0..rows).map(|r| {
            (!null_at(r, 13)).then(|| (at(r) % 1_000_000) as i64 - 500_000)
        })));
    let f32s: ArrayRef = Arc::new(Float32Array::from_iter((0..rows).map(|r| {
        (!null_at(r, 17)).then(|| {
            if r % 5 == 0 {
                special_f64(r) as f32
            } else {
                ((at(r) % 4096) as f32 - 2048.0) / 8.0
            }
        })
    })));
    let f64s: ArrayRef = Arc::new(Float64Array::from_iter((0..rows).map(|r| {
        (!null_at(r, 19)).then(|| {
            if r % 5 == 0 {
                special_f64(r)
            } else {
                ((at(r) % 4096) as f64 - 2048.0) / 8.0
            }
        })
    })));
    let bools: ArrayRef = Arc::new(BooleanArray::from_iter(
        (0..rows).map(|r| (!null_at(r, 23)).then(|| at(r) % 2 == 0)),
    ));
    let dates: ArrayRef =
        Arc::new(Date32Array::from_iter((0..rows).map(|r| {
            (!null_at(r, 29)).then(|| (at(r) % 20_000) as i32 - 10_000)
        })));
    let ts = |stride: usize| -> Vec<Option<i64>> {
        (0..rows)
            .map(|r| (!null_at(r, stride)).then(|| (at(r) % 2_000_000_000) as i64 - 1_000_000_000))
            .collect()
    };
    let ts_s: ArrayRef = Arc::new(TimestampSecondArray::from(ts(31)));
    let ts_ms: ArrayRef = Arc::new(TimestampMillisecondArray::from(ts(37)));
    let ts_us: ArrayRef = Arc::new(TimestampMicrosecondArray::from(ts(41)));
    let ts_ns: ArrayRef = Arc::new(TimestampNanosecondArray::from(ts(43)));
    let dec15: ArrayRef = Arc::new(
        Decimal128Array::from_iter(
            (0..rows).map(|r| (!null_at(r, 3)).then(|| (at(r) % 10_000_000) as i128 - 5_000_000)),
        )
        .with_precision_and_scale(15, 2)
        .expect("(15, 2) holds every value above"),
    );
    let dec38: ArrayRef = Arc::new(
        Decimal128Array::from_iter((0..rows).map(|r| {
            (!null_at(r, 47)).then(|| {
                // past i64 both ways, which is what p > 18 is for
                (at(r) % 1_000_000) as i128 * 10i128.pow(31) - 5 * 10i128.pow(36)
            })
        }))
        .with_precision_and_scale(38, 4)
        .expect("(38, 4) holds every value above"),
    );
    let words = ["alpha", "beta", "gamma", "delta", "", "epsilon"];
    let strings: ArrayRef = Arc::new(StringArray::from_iter((0..rows).map(|r| {
        (!null_at(r, 53)).then(|| words[(at(r) % words.len() as u64) as usize].to_string())
    })));
    let u8s: ArrayRef = Arc::new(UInt8Array::from_iter((0..rows).map(|r| {
        (!null_at(r, 59)).then(|| {
            if r % 4 == 0 {
                u8::MAX - (r % 3) as u8
            } else {
                (at(r) % 256) as u8
            }
        })
    })));
    let u16s: ArrayRef = Arc::new(UInt16Array::from_iter((0..rows).map(|r| {
        (!null_at(r, 61)).then(|| {
            if r % 4 == 1 {
                u16::MAX - (r % 3) as u16
            } else {
                (at(r) % 65_536) as u16
            }
        })
    })));
    let u32s: ArrayRef = Arc::new(UInt32Array::from_iter((0..rows).map(|r| {
        (!null_at(r, 67)).then(|| {
            if r % 4 == 2 {
                u32::MAX - (r % 3) as u32
            } else {
                (1u32 << 31) + (at(r) % 1000) as u32
            }
        })
    })));
    let u64s: ArrayRef = Arc::new(UInt64Array::from_iter((0..rows).map(|r| {
        (!null_at(r, 71)).then(|| {
            if r % 4 == 3 {
                u64::MAX - (r % 3) as u64
            } else {
                (1u64 << 63) + at(r) % 1000
            }
        })
    })));
    RecordBatch::try_new(
        key_types_schema(),
        vec![
            id, k8, k16, k32, k64, f32s, f64s, bools, dates, ts_s, ts_ms, ts_us, ts_ns, dec15,
            dec38, strings, u8s, u16s, u32s, u64s,
        ],
    )
    .expect("twenty arrays of one length under the schema that declares them")
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
fn key_types_is_the_same_batch_twice() {
    assert_eq!(key_types(50, 7), key_types(50, 7));
    assert_ne!(key_types(50, 7), key_types(50, 8));
}

#[test]
fn key_types_carries_a_null_in_every_column_but_id() {
    let batch = key_types(96, 5);
    assert_eq!(batch.schema().fields().len(), 20);
    for (index, field) in batch.schema().fields().iter().enumerate() {
        let nulls = batch.column(index).null_count();
        if field.name() == "id" {
            assert_eq!(nulls, 0, "id is the tie-breaker and is never null");
        } else {
            assert!(nulls > 0, "{} has no null in 96 rows", field.name());
        }
    }
}

#[test]
fn key_types_floats_carry_both_zeros_and_both_nan_signs() {
    let batch = key_types(96, 5);
    let f64s = batch.column(6).as_primitive::<Float64Type>();
    let seen: Vec<u64> = (0..batch.num_rows())
        .filter(|r| f64s.is_valid(*r))
        .map(|r| f64s.value(r).to_bits())
        .collect();
    for bits in [
        0.0f64.to_bits(),
        (-0.0f64).to_bits(),
        f64::NAN.to_bits(),
        f64::NAN.to_bits() | (1 << 63),
    ] {
        assert!(seen.contains(&bits), "f64 is missing {bits:#x}");
    }
}

#[test]
fn key_types_unsigned_columns_pass_their_signed_maximum() {
    let batch = key_types(96, 5);
    let u32s = batch.column(18).as_primitive::<UInt32Type>();
    let u64s = batch.column(19).as_primitive::<UInt64Type>();
    assert!(
        (0..batch.num_rows()).any(|r| u32s.is_valid(r) && u32s.value(r) > i32::MAX as u32),
        "a u32 past i32::MAX is the case the widening is for"
    );
    assert!(
        (0..batch.num_rows()).any(|r| u64s.is_valid(r) && u64s.value(r) > i64::MAX as u64),
        "a u64 past i64::MAX is the case the bit reinterpretation is for"
    );
}

#[test]
fn key_types_wide_decimals_pass_i64() {
    let batch = key_types(96, 5);
    let dec = batch.column(14).as_primitive::<Decimal128Type>();
    assert!(
        (0..batch.num_rows()).any(|r| dec.is_valid(r) && dec.value(r).abs() > i64::MAX as i128),
        "dec38 must hold a value no i64 can, which is what p > 18 is for"
    );
}

#[test]
fn prefixing_renames_every_column_and_touches_no_value() {
    let batch = synthetic(5, 3);
    let renamed = prefixed(&batch, "r_");
    assert_eq!(renamed.schema().field(0).name(), "r_id");
    assert_eq!(renamed.columns(), batch.columns());
}
