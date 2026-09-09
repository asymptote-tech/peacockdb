//! Arrow-to-wire conversions the recipe writers share: a schema, a literal and the
//! type tag both carry.
//!
//! One spelling per fact — the fb `DataType` a column has, and the flags a decimal
//! carries beside it — because a second one drifts silently: the payload verifies, the
//! bytes differ, and the digest golden is the only thing that ever says so.

use datafusion::arrow::datatypes::{DataType as ArrowDataType, SchemaRef};
use datafusion::common::ScalarValue as DfScalarValue;
use flatbuffers::{FlatBufferBuilder, WIPOffset};

use crate::generated::gpu_plan_generated::peacock::plan as fb;

pub(crate) fn serialize_scalar_value<'a>(
    b: &mut FlatBufferBuilder<'a>,
    sv: &DfScalarValue,
) -> Result<WIPOffset<fb::ScalarValue<'a>>, String> {
    let mut args = fb::ScalarValueArgs::default();

    match sv {
        DfScalarValue::Null => {
            args.type_ = fb::DataType::Null;
        }
        DfScalarValue::Boolean(Some(v)) => {
            args.type_ = fb::DataType::Boolean;
            args.bool_val = *v;
        }
        DfScalarValue::Int8(Some(v)) => {
            args.type_ = fb::DataType::Int8;
            args.int_val = *v as i64;
        }
        DfScalarValue::Int16(Some(v)) => {
            args.type_ = fb::DataType::Int16;
            args.int_val = *v as i64;
        }
        DfScalarValue::Int32(Some(v)) => {
            args.type_ = fb::DataType::Int32;
            args.int_val = *v as i64;
        }
        DfScalarValue::Int64(Some(v)) => {
            args.type_ = fb::DataType::Int64;
            args.int_val = *v;
        }
        DfScalarValue::UInt8(Some(v)) => {
            args.type_ = fb::DataType::UInt8;
            args.uint_val = *v as u64;
        }
        DfScalarValue::UInt16(Some(v)) => {
            args.type_ = fb::DataType::UInt16;
            args.uint_val = *v as u64;
        }
        DfScalarValue::UInt32(Some(v)) => {
            args.type_ = fb::DataType::UInt32;
            args.uint_val = *v as u64;
        }
        DfScalarValue::UInt64(Some(v)) => {
            args.type_ = fb::DataType::UInt64;
            args.uint_val = *v;
        }
        DfScalarValue::Float32(Some(v)) => {
            args.type_ = fb::DataType::Float32;
            args.float_val = *v as f64;
        }
        DfScalarValue::Float64(Some(v)) => {
            args.type_ = fb::DataType::Float64;
            args.float_val = *v;
        }
        DfScalarValue::Utf8(Some(s)) | DfScalarValue::LargeUtf8(Some(s)) => {
            args.type_ = fb::DataType::Utf8;
            args.string_val = Some(b.create_string(s));
        }
        DfScalarValue::Utf8View(Some(s)) => {
            // Utf8View is a DataFusion 45+ optimizer rewrite of string literals;
            // cuDF doesn't distinguish view vs. owned strings. Preserve the type
            // tag for faithful roundtrip, but the wire payload is identical.
            args.type_ = fb::DataType::Utf8View;
            args.string_val = Some(b.create_string(s));
        }
        DfScalarValue::Date32(Some(d)) => {
            args.type_ = fb::DataType::Date32;
            args.int_val = *d as i64;
        }
        DfScalarValue::Decimal128(Some(v), prec, scale) => {
            args.type_ = fb::DataType::Decimal128;
            args.decimal_hi = (*v >> 64) as i64;
            args.decimal_lo = *v as u64;
            args.decimal_precision = *prec;
            args.decimal_scale = *scale as i8;
        }
        // Treat any None variant as typed null. The `is_null` flag is what
        // distinguishes it from a zero value on the wire.
        other if other.is_null() => {
            args.type_ = convert_data_type(&other.data_type())?;
            args.is_null = true;
            if let DfScalarValue::Decimal128(_, prec, scale) = other {
                args.decimal_precision = *prec;
                args.decimal_scale = *scale as i8;
            }
        }
        other => {
            return Err(format!("unsupported scalar value: {other:?}"));
        }
    }

    Ok(fb::ScalarValue::create(b, &args))
}

pub(crate) fn convert_data_type(dt: &ArrowDataType) -> Result<fb::DataType, String> {
    Ok(match dt {
        ArrowDataType::Null => fb::DataType::Null,
        ArrowDataType::Boolean => fb::DataType::Boolean,
        ArrowDataType::Int8 => fb::DataType::Int8,
        ArrowDataType::Int16 => fb::DataType::Int16,
        ArrowDataType::Int32 => fb::DataType::Int32,
        ArrowDataType::Int64 => fb::DataType::Int64,
        ArrowDataType::UInt8 => fb::DataType::UInt8,
        ArrowDataType::UInt16 => fb::DataType::UInt16,
        ArrowDataType::UInt32 => fb::DataType::UInt32,
        ArrowDataType::UInt64 => fb::DataType::UInt64,
        ArrowDataType::Float16 => fb::DataType::Float16,
        ArrowDataType::Float32 => fb::DataType::Float32,
        ArrowDataType::Float64 => fb::DataType::Float64,
        ArrowDataType::Utf8 => fb::DataType::Utf8,
        ArrowDataType::LargeUtf8 => fb::DataType::LargeUtf8,
        ArrowDataType::Binary => fb::DataType::Binary,
        ArrowDataType::LargeBinary => fb::DataType::LargeBinary,
        ArrowDataType::Date32 => fb::DataType::Date32,
        ArrowDataType::Date64 => fb::DataType::Date64,
        ArrowDataType::Decimal128(_, _) => fb::DataType::Decimal128,
        ArrowDataType::Utf8View => fb::DataType::Utf8View,
        ArrowDataType::BinaryView => fb::DataType::BinaryView,
        other => return Err(format!("unsupported Arrow data type: {other:?}")),
    })
}

pub(crate) fn serialize_schema<'a>(
    b: &mut FlatBufferBuilder<'a>,
    schema: &SchemaRef,
) -> WIPOffset<fb::Schema<'a>> {
    let fields: Vec<_> = schema
        .fields()
        .iter()
        .map(|f| {
            let name = b.create_string(f.name());
            let dt = convert_data_type(f.data_type()).unwrap_or(fb::DataType::Null);
            let (decimal_precision, decimal_scale) = match f.data_type() {
                ArrowDataType::Decimal128(p, s) => (*p, *s),
                _ => (0, 0),
            };
            fb::Field::create(
                b,
                &fb::FieldArgs {
                    name: Some(name),
                    data_type: dt,
                    nullable: f.is_nullable(),
                    decimal_precision,
                    decimal_scale,
                },
            )
        })
        .collect();
    let fields_vec = b.create_vector(&fields);
    fb::Schema::create(b, &fb::SchemaArgs { fields: Some(fields_vec) })
}
