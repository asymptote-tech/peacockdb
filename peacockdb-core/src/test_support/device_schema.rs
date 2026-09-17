//! What the device holds at a handle, reduced to what cuDF stores: a `type_id` and, for a
//! decimal, its scale. The declared side is cuDF's own arrow-to-cudf table
//! (`third_party/cudf/cpp/src/interop/arrow_utilities.cpp`, `arrow_to_cudf_type`) over the
//! types the wire admits, the same table in 25.02. The actual side comes back through 25.02's
//! `to_arrow_schema`, which has no narrow-decimal arrow type and so reports `DECIMAL32` and
//! `DECIMAL64` as `decimal128` at precision 9 and 18 — there, and only there, a precision is
//! a width. Otherwise precision, timezone and nullability are labels the export carries and
//! the device does not hold, so they are not here.

use std::fmt;

use datafusion::arrow::datatypes::{DataType, Field, Schema, TimeUnit};
use datafusion::arrow::ipc::reader::StreamReader;
#[cfg(not(feature = "rust-only"))]
use peacockdb_ffi::raw::{PeacockExecutor, peacock_handle_schema, peacock_result_free};

use super::{DeviceSchema, DeviceType, TypeId};

#[cfg(test)]
mod tests;

pub(crate) fn device_type_of(arrow: &DataType) -> DeviceType {
    let plain = |id| DeviceType { id, scale: None };
    match arrow {
        DataType::Null => plain(TypeId::Empty),
        DataType::Boolean => plain(TypeId::Bool8),
        DataType::Int8 => plain(TypeId::Int8),
        DataType::Int16 => plain(TypeId::Int16),
        DataType::Int32 => plain(TypeId::Int32),
        DataType::Int64 => plain(TypeId::Int64),
        DataType::UInt8 => plain(TypeId::UInt8),
        DataType::UInt16 => plain(TypeId::UInt16),
        DataType::UInt32 => plain(TypeId::UInt32),
        DataType::UInt64 => plain(TypeId::UInt64),
        DataType::Float32 => plain(TypeId::Float32),
        DataType::Float64 => plain(TypeId::Float64),
        DataType::Date32 => plain(TypeId::TimestampDays),
        DataType::Timestamp(TimeUnit::Second, _) => plain(TypeId::TimestampSeconds),
        DataType::Timestamp(TimeUnit::Millisecond, _) => plain(TypeId::TimestampMilliseconds),
        DataType::Timestamp(TimeUnit::Microsecond, _) => plain(TypeId::TimestampMicroseconds),
        DataType::Timestamp(TimeUnit::Nanosecond, _) => plain(TypeId::TimestampNanoseconds),
        DataType::Utf8 | DataType::LargeUtf8 => plain(TypeId::String),
        DataType::Decimal128(_, scale) => DeviceType {
            id: TypeId::Decimal128,
            scale: Some(i32::from(*scale)),
        },
        other => panic!("{other} has no cuDF image"),
    }
}

pub(crate) fn device_schema_of(declared: &Schema) -> DeviceSchema {
    DeviceSchema(
        declared
            .fields()
            .iter()
            .map(|field| (field.name().clone(), device_type_of(field.data_type())))
            .collect(),
    )
}

/// Every diverging column in the sink's spelling, `3 o_year: Int32 vs INT16`, a renamed one
/// carrying the device's name before its type; a width mismatch first, then the columns
/// both have.
pub(crate) fn device_divergence(declared: &Schema, actual: &DeviceSchema) -> Option<String> {
    let mut findings = Vec::new();
    if declared.fields().len() != actual.0.len() {
        findings.push(format!(
            "{} columns declared, {} held",
            declared.fields().len(),
            actual.0.len()
        ));
    }
    for (at, (field, (name, held))) in declared.fields().iter().zip(&actual.0).enumerate() {
        let expected = device_type_of(field.data_type());
        if field.name() == name && expected == *held {
            continue;
        }
        let held = match field.name() == name {
            true => held.to_string(),
            false => format!("{name} {held}"),
        };
        findings.push(format!(
            "{at} {}: {} vs {held}",
            field.name(),
            field.data_type()
        ));
    }
    (!findings.is_empty()).then(|| findings.join("; "))
}

/// The schema message of an IPC stream — what `peacock_handle_schema` hands back — projected,
/// a decimal by the width its precision names. Its one caller is the handle read below, which
/// `rust-only` compiles out; the unit tests are what keep it in that build.
#[cfg_attr(feature = "rust-only", allow(dead_code))]
pub(crate) fn from_ipc(bytes: &[u8]) -> DeviceSchema {
    let reader = StreamReader::try_new(std::io::Cursor::new(bytes), None).expect("an IPC stream");
    DeviceSchema(
        reader
            .schema()
            .fields()
            .iter()
            .map(|field| (field.name().clone(), held_type_of(field)))
            .collect(),
    )
}

fn held_type_of(field: &Field) -> DeviceType {
    match field.data_type() {
        DataType::Decimal128(precision, scale) => DeviceType {
            id: match precision {
                9 => TypeId::Decimal32,
                18 => TypeId::Decimal64,
                38 => TypeId::Decimal128,
                other => panic!(
                    "{}: a handle reports precision {other}, which names no cuDF width",
                    field.name()
                ),
            },
            scale: Some(i32::from(*scale)),
        },
        other => device_type_of(other),
    }
}

/// `peacock_handle_schema` on the handle: the schema and no rows, the handle left resident.
#[cfg(not(feature = "rust-only"))]
pub(crate) fn schema_at(executor: *mut PeacockExecutor, handle: u64) -> DeviceSchema {
    let mut ipc: *mut u8 = std::ptr::null_mut();
    let mut len = 0u64;
    let rc = unsafe { peacock_handle_schema(executor, handle, &mut ipc, &mut len) };
    assert_eq!(rc, 0, "handle_schema({handle}) failed");
    let bytes = unsafe { std::slice::from_raw_parts(ipc, len as usize) };
    let schema = from_ipc(bytes);
    unsafe { peacock_result_free(ipc) };
    schema
}

impl fmt::Display for TypeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            TypeId::Empty => "EMPTY",
            TypeId::Bool8 => "BOOL8",
            TypeId::Int8 => "INT8",
            TypeId::Int16 => "INT16",
            TypeId::Int32 => "INT32",
            TypeId::Int64 => "INT64",
            TypeId::UInt8 => "UINT8",
            TypeId::UInt16 => "UINT16",
            TypeId::UInt32 => "UINT32",
            TypeId::UInt64 => "UINT64",
            TypeId::Float32 => "FLOAT32",
            TypeId::Float64 => "FLOAT64",
            TypeId::TimestampDays => "TIMESTAMP_DAYS",
            TypeId::TimestampSeconds => "TIMESTAMP_SECONDS",
            TypeId::TimestampMilliseconds => "TIMESTAMP_MILLISECONDS",
            TypeId::TimestampMicroseconds => "TIMESTAMP_MICROSECONDS",
            TypeId::TimestampNanoseconds => "TIMESTAMP_NANOSECONDS",
            TypeId::String => "STRING",
            TypeId::Decimal32 => "DECIMAL32",
            TypeId::Decimal64 => "DECIMAL64",
            TypeId::Decimal128 => "DECIMAL128",
        })
    }
}

impl fmt::Display for DeviceType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.scale {
            Some(scale) => write!(f, "{} scale {scale}", self.id),
            None => write!(f, "{}", self.id),
        }
    }
}
