//! The projection against cuDF's interop table, and the comparator against itself: no
//! handle, no device.

use datafusion::arrow::datatypes::{DataType, Field, IntervalUnit, Schema, TimeUnit};
use datafusion::arrow::ipc::writer::StreamWriter;

use super::from_ipc;
use crate::test_support::{
    DeviceSchema, DeviceType, TypeId, device_divergence, device_schema_of, device_type_of,
};

fn plain(id: TypeId) -> DeviceType {
    DeviceType { id, scale: None }
}

fn decimal(scale: i32) -> DeviceType {
    DeviceType {
        id: TypeId::Decimal128,
        scale: Some(scale),
    }
}

#[test]
fn utf8_and_large_utf8_are_one_string() {
    assert_eq!(device_type_of(&DataType::Utf8), plain(TypeId::String));
    assert_eq!(device_type_of(&DataType::LargeUtf8), plain(TypeId::String));
}

#[test]
fn a_decimal_keeps_its_scale_and_loses_its_precision() {
    assert_eq!(device_type_of(&DataType::Decimal128(15, 2)), decimal(2));
    assert_eq!(device_type_of(&DataType::Decimal128(38, 2)), decimal(2));
    assert_ne!(device_type_of(&DataType::Decimal128(15, 2)), decimal(4));
}

#[test]
fn date32_is_timestamp_days() {
    assert_eq!(
        device_type_of(&DataType::Date32),
        plain(TypeId::TimestampDays)
    );
}

#[test]
fn timestamps_keep_the_unit_and_lose_the_zone() {
    let zoned = |unit| DataType::Timestamp(unit, Some("UTC".into()));
    let naive = |unit| DataType::Timestamp(unit, None);
    for (unit, id) in [
        (TimeUnit::Second, TypeId::TimestampSeconds),
        (TimeUnit::Millisecond, TypeId::TimestampMilliseconds),
        (TimeUnit::Microsecond, TypeId::TimestampMicroseconds),
        (TimeUnit::Nanosecond, TypeId::TimestampNanoseconds),
    ] {
        assert_eq!(device_type_of(&zoned(unit)), plain(id));
        assert_eq!(device_type_of(&naive(unit)), plain(id));
    }
}

#[test]
fn integers_and_floats_by_width() {
    for (arrow, id) in [
        (DataType::Int8, TypeId::Int8),
        (DataType::Int16, TypeId::Int16),
        (DataType::Int32, TypeId::Int32),
        (DataType::Int64, TypeId::Int64),
        (DataType::UInt8, TypeId::UInt8),
        (DataType::UInt16, TypeId::UInt16),
        (DataType::UInt32, TypeId::UInt32),
        (DataType::UInt64, TypeId::UInt64),
        (DataType::Float32, TypeId::Float32),
        (DataType::Float64, TypeId::Float64),
    ] {
        assert_eq!(device_type_of(&arrow), plain(id), "{arrow}");
    }
}

#[test]
fn boolean_is_bool8() {
    assert_eq!(device_type_of(&DataType::Boolean), plain(TypeId::Bool8));
}

#[test]
fn null_is_empty() {
    assert_eq!(device_type_of(&DataType::Null), plain(TypeId::Empty));
}

#[test]
#[should_panic(expected = "Interval(DayTime)")]
fn an_unmapped_type_panics_by_name() {
    device_type_of(&DataType::Interval(IntervalUnit::DayTime));
}

#[test]
fn a_device_type_prints_in_the_sinks_spelling() {
    assert_eq!(plain(TypeId::Int16).to_string(), "INT16");
    assert_eq!(plain(TypeId::String).to_string(), "STRING");
    assert_eq!(plain(TypeId::TimestampDays).to_string(), "TIMESTAMP_DAYS");
    assert_eq!(plain(TypeId::Bool8).to_string(), "BOOL8");
    assert_eq!(decimal(2).to_string(), "DECIMAL128 scale 2");
}

fn declared() -> Schema {
    Schema::new(vec![
        Field::new("a", DataType::Int32, false),
        Field::new("b", DataType::Utf8, true),
        Field::new("c", DataType::Decimal128(15, 2), true),
    ])
}

#[test]
fn a_declared_schema_projects_column_by_column() {
    assert_eq!(
        device_schema_of(&declared()),
        DeviceSchema(vec![
            ("a".into(), plain(TypeId::Int32)),
            ("b".into(), plain(TypeId::String)),
            ("c".into(), decimal(2)),
        ])
    );
}

#[test]
fn divergence_names_every_differing_column_in_the_sinks_spelling() {
    let actual = DeviceSchema(vec![
        ("a".into(), plain(TypeId::Int16)),
        ("b".into(), plain(TypeId::String)),
        ("c".into(), decimal(4)),
    ]);
    assert_eq!(
        device_divergence(&declared(), &actual),
        Some("0 a: Int32 vs INT16; 2 c: Decimal128(15, 2) vs DECIMAL128 scale 4".to_string())
    );
}

#[test]
fn a_matching_schema_diverges_nowhere() {
    assert_eq!(
        device_divergence(&declared(), &device_schema_of(&declared())),
        None
    );
}

// Precision and nullability are labels the export carries, not facts the device holds.
#[test]
fn precision_and_nullability_are_outside_the_comparison() {
    let relabelled = Schema::new(vec![
        Field::new("a", DataType::Int32, true),
        Field::new("b", DataType::Utf8, false),
        Field::new("c", DataType::Decimal128(38, 2), false),
    ]);
    assert_eq!(
        device_divergence(&declared(), &device_schema_of(&relabelled)),
        None
    );
}

#[test]
fn a_renamed_column_is_a_divergence() {
    let actual = DeviceSchema(vec![
        ("a".into(), plain(TypeId::Int32)),
        ("bee".into(), plain(TypeId::String)),
        ("c".into(), decimal(2)),
    ]);
    assert_eq!(
        device_divergence(&declared(), &actual),
        Some("1 b: Utf8 vs bee STRING".to_string())
    );
}

#[test]
fn a_column_count_mismatch_is_a_divergence() {
    let actual = DeviceSchema(vec![
        ("a".into(), plain(TypeId::Int32)),
        ("b".into(), plain(TypeId::String)),
    ]);
    assert_eq!(
        device_divergence(&declared(), &actual),
        Some("3 columns declared, 2 held".to_string())
    );
    let actual = DeviceSchema(vec![
        ("a".into(), plain(TypeId::Int16)),
        ("b".into(), plain(TypeId::String)),
        ("c".into(), decimal(2)),
        ("d".into(), plain(TypeId::Bool8)),
    ]);
    assert_eq!(
        device_divergence(&declared(), &actual),
        Some("3 columns declared, 4 held; 0 a: Int32 vs INT16".to_string())
    );
}

// What `peacock_handle_schema` hands back: an IPC stream carrying the schema message and
// no batch, a `DECIMAL128` at `decimal128(38, s)` and every string `utf8`.
#[test]
fn a_device_schema_reads_off_a_schema_only_ipc_stream() {
    let exported = Schema::new(vec![
        Field::new("a", DataType::Int32, true),
        Field::new("s", DataType::Utf8, true),
        Field::new("c", DataType::Decimal128(38, 2), true),
        Field::new("d", DataType::Date32, true),
    ]);
    let mut bytes = Vec::new();
    let mut writer = StreamWriter::try_new(&mut bytes, &exported).expect("a stream");
    writer.finish().expect("a schema-only stream");
    drop(writer);
    assert_eq!(
        from_ipc(&bytes),
        DeviceSchema(vec![
            ("a".into(), plain(TypeId::Int32)),
            ("s".into(), plain(TypeId::String)),
            ("c".into(), decimal(2)),
            ("d".into(), plain(TypeId::TimestampDays)),
        ])
    );
    assert_eq!(device_divergence(&exported, &from_ipc(&bytes)), None);
}

// 25.02's `to_arrow_schema` reports a narrow width as `decimal128` at that width's maximum
// precision, 9 or 18, so at the handle the precision is the width — unlike a declaration.
#[test]
fn a_narrow_precision_at_the_handle_is_the_narrow_width_the_device_holds() {
    let held = Schema::new(vec![
        Field::new("n", DataType::Decimal128(9, 1), true),
        Field::new("w", DataType::Decimal128(18, 2), true),
    ]);
    let mut bytes = Vec::new();
    let mut writer = StreamWriter::try_new(&mut bytes, &held).expect("a stream");
    writer.finish().expect("a schema-only stream");
    drop(writer);
    let narrow = |id, scale| DeviceType {
        id,
        scale: Some(scale),
    };
    assert_eq!(
        from_ipc(&bytes),
        DeviceSchema(vec![
            ("n".into(), narrow(TypeId::Decimal32, 1)),
            ("w".into(), narrow(TypeId::Decimal64, 2)),
        ])
    );
    let declared = Schema::new(vec![
        Field::new("n", DataType::Decimal128(9, 1), true),
        Field::new("w", DataType::Decimal128(18, 2), true),
    ]);
    assert_eq!(
        device_divergence(&declared, &from_ipc(&bytes)),
        Some(
            "0 n: Decimal128(9, 1) vs DECIMAL32 scale 1; 1 w: Decimal128(18, 2) vs DECIMAL64 scale 2"
                .to_string()
        )
    );
}
