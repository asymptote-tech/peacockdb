//! A column as the schema catalog compares it: name and type, with the two things the
//! production exporter rewrites set aside. Pure arrow, so it sits at the rust rung and the
//! device walk (`gpu_tests/walk.rs`) reaches it as `super::tests::columns`.

use datafusion::arrow::datatypes::{
    DECIMAL128_MAX_PRECISION, DECIMAL256_MAX_PRECISION, DataType, Field, Schema as ArrowSchema,
};

/// Nullability is not carried — the exporter derives the flag from `has_nulls()` — and a
/// decimal reads at its maximum precision on both sides, the value the exporter writes
/// whatever cuDF held, so only its scale can differ.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Column {
    pub(crate) name: String,
    pub(crate) data_type: DataType,
}

pub(crate) fn columns(schema: &ArrowSchema) -> Vec<Column> {
    schema
        .fields()
        .iter()
        .map(|field| Column {
            name: field.name().clone(),
            data_type: match field.data_type() {
                DataType::Decimal128(_, scale) => {
                    DataType::Decimal128(DECIMAL128_MAX_PRECISION, *scale)
                }
                DataType::Decimal256(_, scale) => {
                    DataType::Decimal256(DECIMAL256_MAX_PRECISION, *scale)
                }
                other => other.clone(),
            },
        })
        .collect()
}

/// Two schemas that differ only in what the exporter rewrites — decimal precision, which
/// it writes as 38 whatever cuDF held, and nullability, which it derives from the data —
/// compare equal; a scale, a type or a name that differs still shows.
#[test]
fn columns_set_precision_and_nullability_aside() {
    let declared = ArrowSchema::new(vec![
        Field::new("a", DataType::Decimal128(15, 2), false),
        Field::new("b", DataType::Utf8, true),
    ]);
    let exported = ArrowSchema::new(vec![
        Field::new("a", DataType::Decimal128(38, 2), true),
        Field::new("b", DataType::Utf8, false),
    ]);
    assert_eq!(columns(&declared), columns(&exported));

    let one = |name: &str, data_type: DataType| {
        columns(&ArrowSchema::new(vec![Field::new(name, data_type, true)]))
    };
    assert_ne!(
        one("a", DataType::Decimal128(15, 2)),
        one("a", DataType::Decimal128(38, 3))
    );
    assert_ne!(one("b", DataType::Utf8View), one("b", DataType::Utf8));
    assert_ne!(one("b", DataType::Utf8), one("c", DataType::Utf8));
}
