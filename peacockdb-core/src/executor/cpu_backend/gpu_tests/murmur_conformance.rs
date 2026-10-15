//! GPU↔CPU murmur3 hash-partition conformance (the linchpin gate).
//!
//! Both engines must put a row in the SAME lane, or the hash-repartition golden (per-node
//! rows + result) diverges. The CPU side is `rows_per_lane`, the function the emitter
//! calls; the GPU side is our own Spark-murmur3 CUDA kernel
//! `peacock::partitioning::spark_partition_ids` (seed 42, per-column left-to-right running
//! seed, Spark null-skip, UTF-8 bytes). cuDF ships only STANDARD murmur3, proven ≠ Spark's
//! by the probe, so we own the hash kernel and reuse `cudf::partition` for the scatter.
//!
//! The `*_match_rule_live` gates drive the real kernel through the FFI hook and the real
//! production rule in one process, and assert the two agree position by position. A key
//! type not covered here is unproven on the device.

use datafusion::arrow::array::{ArrayRef, StringArray};
use datafusion::arrow::datatypes::{Field, Schema as ArrowSchema};
use datafusion::arrow::record_batch::RecordBatch;
use datafusion::physical_expr::PhysicalExpr;
use datafusion::physical_expr::expressions::Column;
use datafusion_comet_spark_expr::hash_funcs::murmur3::spark_compatible_murmur3_hash;
use std::sync::Arc;

use super::super::spark_partitioning::{pmod, rows_per_lane};

/// The lane of each row under the production rule — `rows_per_lane`, the function the cpu's
/// emitter calls — inverted from per-lane row lists to one id per row, so it compares with
/// the kernel's output position by position.
fn production_partition_ids(cols: &[(Field, ArrayRef)], lanes: usize) -> Vec<i32> {
    let schema = Arc::new(ArrowSchema::new(
        cols.iter().map(|(f, _)| f.clone()).collect::<Vec<_>>(),
    ));
    let batch = RecordBatch::try_new(schema, cols.iter().map(|(_, a)| Arc::clone(a)).collect())
        .expect("the gate's columns share a length");
    let exprs: Vec<Arc<dyn PhysicalExpr>> = cols
        .iter()
        .enumerate()
        .map(|(i, (f, _))| Arc::new(Column::new(f.name(), i)) as Arc<dyn PhysicalExpr>)
        .collect();
    let per_lane = rows_per_lane(&batch, &exprs, lanes).expect("the production rule answers");
    let mut ids = vec![-1i32; batch.num_rows()];
    for (lane, rows) in per_lane.iter().enumerate() {
        for row in rows {
            ids[*row as usize] = lane as i32;
        }
    }
    assert!(ids.iter().all(|id| *id >= 0), "every row has a lane");
    ids
}

#[test]
fn step_i_comet_murmur3_public_api_compiles_and_runs() {
    use datafusion::arrow::datatypes::DataType;
    // Single-value low-level hash (UTF-8 bytes), seed 42 — the q1 chars.
    let h_a = spark_compatible_murmur3_hash(b"A", 42);
    let h_n = spark_compatible_murmur3_hash(b"N", 42);
    assert_ne!(
        h_a, h_n,
        "distinct keys should (almost surely) hash differently"
    );

    let keys: ArrayRef = Arc::new(StringArray::from(vec!["A", "N", "R", "F", "O"]));
    let ids = production_partition_ids(&[(Field::new("k", DataType::Utf8, true), keys)], 8);
    assert_eq!(ids.len(), 5);
    assert!(ids.iter().all(|p| (0..8).contains(p)), "all ids in [0,8)");
}

/// Conformance harness: drive the real GPU kernel (`peacock::partitioning::
/// spark_partition_ids` via the FFI hook) and the real production rule, in ONE process,
/// over the SAME `cols` — assert bit-exact. No hardcoded reference values. Each
/// `(Field, ArrayRef)` is a key column; they seed-chain left-to-right (composite keys).
/// Requires a GPU + the cudf-linked build.
fn assert_gpu_matches_rule_live(cols: Vec<(Field, ArrayRef)>, n_parts: i32) {
    use datafusion::arrow::array::{Array, StructArray};
    use datafusion::arrow::ffi::{FFI_ArrowArray, FFI_ArrowSchema, to_ffi};
    use std::ffi::c_void;

    let rows = cols[0].1.len();
    let rule = production_partition_ids(&cols, n_parts as usize);
    let key_count = cols.len();

    // Export the key columns as a struct array (= the table) over the Arrow C-Data
    // interface; cuDF reads the struct as a table.
    let struct_arr = StructArray::from(
        cols.into_iter()
            .map(|(f, a)| (Arc::new(f), a))
            .collect::<Vec<_>>(),
    );
    let (ffi_arr, ffi_schema) = to_ffi(&struct_arr.to_data()).unwrap();

    let key_cols: Vec<u32> = (0..key_count as u32).collect();
    let mut out = vec![0i32; rows];
    let mut got_n: u64 = 0;
    let rc = unsafe {
        peacockdb_ffi::raw::peacock_spark_partition_ids(
            &ffi_schema as *const FFI_ArrowSchema as *const c_void,
            &ffi_arr as *const FFI_ArrowArray as *const c_void,
            key_cols.as_ptr(),
            key_cols.len() as u64,
            n_parts as u32,
            42,
            out.as_mut_ptr(),
            out.len() as u64,
            &mut got_n,
        )
    };
    assert_eq!(rc, 0, "peacock_spark_partition_ids FFI returned an error");
    assert_eq!(got_n as usize, rows, "FFI returned wrong row count");
    eprintln!("LIVE production rule partition_ids = {rule:?}");
    eprintln!("LIVE GPU  FFI       partition_ids = {out:?}");
    assert_eq!(
        out, rule,
        "the GPU kernel's lanes must equal the production lane rule's, bit-exact"
    );
}

/// PERMANENT gate (STRING keys): 2 string columns (l_returnflag, l_linestatus)
/// + a NULL in each (multi-key seeding + null-skip).
#[test]
fn gpu_spark_partition_ids_match_rule_live() {
    use datafusion::arrow::datatypes::DataType;
    let rf: ArrayRef = Arc::new(StringArray::from(vec![
        Some("A"), Some("N"), Some("N"), Some("R"), None, Some("A"),
    ]));
    let ls: ArrayRef = Arc::new(StringArray::from(vec![
        Some("F"), Some("F"), Some("O"), Some("F"), Some("F"), None,
    ]));
    assert_gpu_matches_rule_live(
        vec![
            (Field::new("rf", DataType::Utf8, true), rf),
            (Field::new("ls", DataType::Utf8, true), ls),
        ],
        8,
    );
}

/// INT32 key conformance — edge values (0, -1, i32::MAX/MIN) + a NULL.
/// int32 = one 4-byte LE block, no tail; exercises the generic fixed-width kernel
/// and the negative/extreme two's-complement encodings + Spark null-skip.
#[test]
fn gpu_spark_partition_ids_int32_match_rule_live() {
    use datafusion::arrow::array::Int32Array;
    use datafusion::arrow::datatypes::DataType;
    let k: ArrayRef = Arc::new(Int32Array::from(vec![
        Some(1), Some(0), Some(-1), Some(i32::MAX), Some(i32::MIN), None, Some(42),
    ]));
    assert_gpu_matches_rule_live(vec![(Field::new("k", DataType::Int32, true), k)], 8);
}

/// INT64 key conformance — the dominant surrogate-key (*_sk) case.
/// int64 = two 4-byte LE blocks (low then high), no tail. Edge values + NULL.
#[test]
fn gpu_spark_partition_ids_int64_match_rule_live() {
    use datafusion::arrow::array::Int64Array;
    use datafusion::arrow::datatypes::DataType;
    let k: ArrayRef = Arc::new(Int64Array::from(vec![
        Some(1), Some(0), Some(-1), Some(i64::MAX), Some(i64::MIN), None, Some(1234567890123),
    ]));
    assert_gpu_matches_rule_live(vec![(Field::new("k", DataType::Int64, true), k)], 8);
}

/// (#18) INT16 key conformance — the GROUP-BY year case (cudf::extract_year emits
/// INT16, so a year-grouped query repartitions on an INT16 key). Spark widens short→int
/// (4-byte hash); the GPU casts INT16→INT32 before the fixed kernel, so this proves the
/// widened hash is bit-exact vs the rule. Edge values + NULL.
#[test]
fn gpu_spark_partition_ids_int16_match_rule_live() {
    use datafusion::arrow::array::Int16Array;
    use datafusion::arrow::datatypes::DataType;
    let k: ArrayRef = Arc::new(Int16Array::from(vec![
        Some(1), Some(0), Some(-1), Some(i16::MAX), Some(i16::MIN), None, Some(1998),
    ]));
    assert_gpu_matches_rule_live(vec![(Field::new("k", DataType::Int16, true), k)], 8);
}

/// (#18) DATE32 key conformance — the GROUP-BY date case (q3 groups by o_orderdate;
/// cuDF stores it as TIMESTAMP_DAYS = int32 days-since-epoch). Spark hashes DATE as the
/// int32 day count (4-byte); the GPU bit-casts TIMESTAMP_DAYS→INT32, so this proves the
/// days hash is bit-exact vs the rule. Epoch, real dates, pre-epoch negative, NULL.
#[test]
fn gpu_spark_partition_ids_date32_match_rule_live() {
    use datafusion::arrow::array::Date32Array;
    use datafusion::arrow::datatypes::DataType;
    let k: ArrayRef = Arc::new(Date32Array::from(vec![
        Some(0), Some(9203), Some(-1), Some(i32::MAX), None, Some(10000),
    ]));
    assert_gpu_matches_rule_live(vec![(Field::new("k", DataType::Date32, true), k)], 8);
}

/// COMPOSITE all-INT key conformance — the q17 join-key shape
/// (ss_customer_sk, ss_item_sk, ss_ticket_number are all int surrogate keys).
/// Proves the seed-chain across multiple int columns, incl per-column NULLs (a null
/// in ONE column of a row still folds the other columns — Spark skips only that col).
#[test]
fn gpu_spark_partition_ids_composite_int_match_rule_live() {
    use datafusion::arrow::array::{Int32Array, Int64Array};
    use datafusion::arrow::datatypes::DataType;
    let a: ArrayRef = Arc::new(Int32Array::from(vec![
        Some(1), Some(2), Some(-1), None, Some(i32::MIN), Some(7),
    ]));
    let b: ArrayRef = Arc::new(Int64Array::from(vec![
        Some(100), None, Some(-100), Some(i64::MAX), Some(0), Some(7),
    ]));
    assert_gpu_matches_rule_live(
        vec![
            (Field::new("a", DataType::Int32, true), a),
            (Field::new("b", DataType::Int64, true), b),
        ],
        8,
    );
}

/// COMPOSITE MIXED-type key conformance (int64 + string, nulls in each) —
/// proves the running seed chains correctly across type-heterogeneous columns (the
/// general case: an int join/group key interleaved with a string dimension key).
#[test]
fn gpu_spark_partition_ids_composite_mixed_match_rule_live() {
    use datafusion::arrow::array::Int64Array;
    use datafusion::arrow::datatypes::DataType;
    let ints: ArrayRef = Arc::new(Int64Array::from(vec![
        Some(10), Some(-5), None, Some(i64::MAX), Some(0), Some(999),
    ]));
    let strs: ArrayRef = Arc::new(StringArray::from(vec![
        Some("A"), Some("BB"), Some("C"), None, Some(""), Some("ticket"),
    ]));
    assert_gpu_matches_rule_live(
        vec![
            (Field::new("i", DataType::Int64, true), ints),
            (Field::new("s", DataType::Utf8, true), strs),
        ],
        8,
    );
}

#[test]
fn pmod_handles_negative_hashes() {
    // pmod must differ from raw % for negative hashes (the classic mismatch source).
    assert_eq!(pmod(-1, 8), 7);
    assert_eq!(pmod(-9, 8), 7);
    assert_eq!(pmod(7, 8), 7);
    assert_ne!(-1 % 8, pmod(-1, 8), "raw % would give -1, pmod gives 7");
}

/// Boolean: comet hashes `i32::from(b)`, 4 bytes. NULL skipped.
#[test]
fn gpu_spark_partition_ids_boolean_match_rule_live() {
    use datafusion::arrow::array::BooleanArray;
    use datafusion::arrow::datatypes::DataType;
    let k: ArrayRef = Arc::new(BooleanArray::from(vec![
        Some(true), Some(false), None, Some(true),
    ]));
    assert_gpu_matches_rule_live(vec![(Field::new("k", DataType::Boolean, true), k)], 8);
}

/// Int8, which the kernel widens to INT32 already and no gate proved.
#[test]
fn gpu_spark_partition_ids_int8_match_rule_live() {
    use datafusion::arrow::array::Int8Array;
    use datafusion::arrow::datatypes::DataType;
    let k: ArrayRef = Arc::new(Int8Array::from(vec![
        Some(1), Some(0), Some(-1), Some(i8::MAX), Some(i8::MIN), None,
    ]));
    assert_gpu_matches_rule_live(vec![(Field::new("k", DataType::Int8, true), k)], 8);
}

/// A zero-row input and an all-NULL key: no row at all, and every row in `pmod(seed, n)`'s
/// lane, since comet skips a null and leaves the running hash at the seed.
#[test]
fn gpu_spark_partition_ids_zero_rows_and_all_null_match_rule_live() {
    use datafusion::arrow::array::Int32Array;
    use datafusion::arrow::datatypes::DataType;
    let none: ArrayRef = Arc::new(Int32Array::from(Vec::<Option<i32>>::new()));
    assert_gpu_matches_rule_live(vec![(Field::new("k", DataType::Int32, true), none)], 8);
    let nulls: ArrayRef = Arc::new(Int32Array::from(vec![None::<i32>; 5]));
    assert_gpu_matches_rule_live(vec![(Field::new("k", DataType::Int32, true), nulls)], 8);
}

/// 0.0, -0.0, both NaN signs, a NULL and ordinary values — the shapes the float rule has to
/// agree on. `-NaN` is what x86's `0/0` yields, so it is not a synthetic case.
fn float_specials() -> Vec<Option<f64>> {
    vec![
        Some(0.0),
        Some(-0.0),
        Some(f64::NAN),
        Some(f64::from_bits(f64::NAN.to_bits() | (1 << 63))),
        None,
        Some(1.5),
        Some(f64::INFINITY),
        Some(f64::NEG_INFINITY),
        Some(-2.25),
    ]
}

/// Float64: the value's 8 bytes, -0.0 as +0.0's bits, and every NaN as one canonical NaN —
/// the cpu canonicalizes before comet and the kernel does the same, so the engines agree
/// where comet alone would split NaN from -NaN.
#[test]
fn gpu_spark_partition_ids_float64_match_rule_live() {
    use datafusion::arrow::array::Float64Array;
    use datafusion::arrow::datatypes::DataType;
    let k: ArrayRef = Arc::new(Float64Array::from(float_specials()));
    assert_gpu_matches_rule_live(vec![(Field::new("k", DataType::Float64, true), k)], 8);
}

/// Float32, 4 bytes, the same rules.
#[test]
fn gpu_spark_partition_ids_float32_match_rule_live() {
    use datafusion::arrow::array::Float32Array;
    use datafusion::arrow::datatypes::DataType;
    let v: Vec<Option<f32>> = float_specials()
        .into_iter()
        .map(|x| {
            x.map(|f| {
                if f.is_nan() {
                    f32::from_bits(if f.is_sign_negative() {
                        0xffc0_0000
                    } else {
                        0x7fc0_0000
                    })
                } else {
                    f as f32
                }
            })
        })
        .collect();
    let k: ArrayRef = Arc::new(Float32Array::from(v));
    assert_gpu_matches_rule_live(vec![(Field::new("k", DataType::Float32, true), k)], 8);
}

/// A column of nothing but specials, at four lanes: the case where a disagreement cannot
/// hide behind ordinary values that happen to land right.
#[test]
fn gpu_spark_partition_ids_float_specials_only_match_rule_live() {
    use datafusion::arrow::array::Float64Array;
    use datafusion::arrow::datatypes::DataType;
    let k: ArrayRef = Arc::new(Float64Array::from(vec![
        Some(-0.0),
        Some(f64::NAN),
        None,
        Some(-0.0),
        Some(f64::from_bits(f64::NAN.to_bits() | (1 << 63))),
    ]));
    assert_gpu_matches_rule_live(vec![(Field::new("k", DataType::Float64, true), k)], 4);
}

fn ts_values() -> Vec<Option<i64>> {
    vec![
        Some(0), Some(1_700_000_000), Some(-1), Some(i64::MAX), Some(i64::MIN), None, Some(86_400),
    ]
}

/// Every timestamp unit, and one with a zone: comet matches `Timestamp(unit, _)` whatever the
/// zone and hashes the i64, and cuDF reads the same i64 (it keeps no zone), so the lane is the
/// unit's value and nothing else.
#[test]
fn gpu_spark_partition_ids_timestamps_match_rule_live() {
    use datafusion::arrow::array::{
        TimestampMicrosecondArray, TimestampMillisecondArray, TimestampNanosecondArray,
        TimestampSecondArray,
    };
    use datafusion::arrow::datatypes::{DataType, TimeUnit};
    let cases: Vec<(DataType, ArrayRef)> = vec![
        (
            DataType::Timestamp(TimeUnit::Second, None),
            Arc::new(TimestampSecondArray::from(ts_values())),
        ),
        (
            DataType::Timestamp(TimeUnit::Millisecond, None),
            Arc::new(TimestampMillisecondArray::from(ts_values())),
        ),
        (
            DataType::Timestamp(TimeUnit::Microsecond, None),
            Arc::new(TimestampMicrosecondArray::from(ts_values())),
        ),
        (
            DataType::Timestamp(TimeUnit::Nanosecond, None),
            Arc::new(TimestampNanosecondArray::from(ts_values())),
        ),
        (
            DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
            Arc::new(TimestampMicrosecondArray::from(ts_values()).with_timezone("UTC")),
        ),
    ];
    for (ty, k) in cases {
        assert_gpu_matches_rule_live(vec![(Field::new("k", ty, true), k)], 8);
    }
}

fn decimal(values: Vec<Option<i128>>, p: u8, s: i8) -> ArrayRef {
    use datafusion::arrow::array::Decimal128Array;
    Arc::new(
        Decimal128Array::from(values)
            .with_precision_and_scale(p, s)
            .expect("the values fit the declared precision"),
    )
}

/// Decimal128 at p ≤ 18 and p > 18: both engines hash the unscaled value's 16 LE bytes — the
/// cpu by widening to (38, s) before comet, the kernel from the `int128`. Deliberately not
/// Spark's placement at p ≤ 18, where comet hashes 8 bytes: cuDF keeps no precision and the
/// loader widens every decimal, so the kernel cannot pick comet's width (decision D2).
#[test]
fn gpu_spark_partition_ids_decimals_match_rule_live() {
    use datafusion::arrow::datatypes::DataType;
    let small = vec![
        Some(0), Some(1), Some(-1), Some(12_345), Some(-99_999_999), None,
    ];
    assert_gpu_matches_rule_live(
        vec![(
            Field::new("k", DataType::Decimal128(15, 2), true),
            decimal(small, 15, 2),
        )],
        8,
    );
    let wide = vec![
        Some(0),
        Some(-1),
        Some(i64::MIN as i128 - 1),
        Some(10i128.pow(37)),
        Some(-(10i128.pow(37))),
        None,
    ];
    assert_gpu_matches_rule_live(
        vec![(
            Field::new("k", DataType::Decimal128(38, 4), true),
            decimal(wide, 38, 4),
        )],
        8,
    );
}

/// A decimal then a string: the running seed has to chain out of the 16-byte block into the
/// string's bytes, which is the one thing a per-type arm can get right on its own and still
/// break composed.
#[test]
fn gpu_spark_partition_ids_decimal_composite_match_rule_live() {
    use datafusion::arrow::datatypes::DataType;
    let d = decimal(vec![Some(150), None, Some(-3), Some(150)], 15, 2);
    let s: ArrayRef = Arc::new(StringArray::from(vec![
        Some("a"), Some("b"), None, Some("c"),
    ]));
    assert_gpu_matches_rule_live(
        vec![
            (Field::new("d", DataType::Decimal128(15, 2), true), d),
            (Field::new("s", DataType::Utf8, true), s),
        ],
        8,
    );
}

/// Unsigned keys: the cpu and the device apply the same widening before hashing — u8/u16 to
/// i32, u32 to i64 by value, u64 to i64 by its bits. Spark has no unsigned types, so there is
/// no placement to match and the rule is ours; the values past each signed maximum are the
/// point, since a value cast would overflow there.
#[test]
fn gpu_spark_partition_ids_unsigned_match_rule_live() {
    use datafusion::arrow::array::{UInt8Array, UInt16Array, UInt32Array, UInt64Array};
    use datafusion::arrow::datatypes::DataType;
    let cases: Vec<(DataType, ArrayRef)> = vec![
        (
            DataType::UInt8,
            Arc::new(UInt8Array::from(vec![
                Some(0), Some(1), Some(1 << 7), Some(u8::MAX), None,
            ])),
        ),
        (
            DataType::UInt16,
            Arc::new(UInt16Array::from(vec![
                Some(0), Some(1), Some(1 << 15), Some(u16::MAX), None,
            ])),
        ),
        (
            DataType::UInt32,
            Arc::new(UInt32Array::from(vec![
                Some(0), Some(1), Some(1 << 31), Some(u32::MAX), None,
            ])),
        ),
        (
            DataType::UInt64,
            Arc::new(UInt64Array::from(vec![
                Some(0), Some(1), Some(1 << 63), Some(u64::MAX), None,
            ])),
        ),
    ];
    for (ty, k) in cases {
        assert_gpu_matches_rule_live(vec![(Field::new("k", ty, true), k)], 8);
    }
}
