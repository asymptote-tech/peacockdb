use super::schema_divergence;
use datafusion::arrow::datatypes::{DataType, Field, Schema};

fn schema(fields: &[(&str, DataType, bool)]) -> Schema {
    Schema::new(
        fields
            .iter()
            .map(|(name, data_type, nullable)| Field::new(*name, data_type.clone(), *nullable))
            .collect::<Vec<_>>(),
    )
}

/// The two things `RecordBatch::try_new`'s own message lacks are the column's name and
/// the index of the column beside it; the types it already names are repeated so one
/// clause reads on its own.
#[test]
fn a_diverging_column_is_named_with_both_types() {
    let declared = schema(&[("l_extendedprice", DataType::Decimal128(15, 2), false)]);
    let exported = schema(&[("l_extendedprice", DataType::Decimal128(38, 2), false)]);
    assert_eq!(
        schema_divergence(&declared, &exported),
        "0 l_extendedprice: Decimal128(15, 2) vs Decimal128(38, 2)"
    );
}

/// `try_new` stops at the first mismatch; a sink carrying a string and a narrow decimal
/// is two findings, and the column between them that matches is not one.
#[test]
fn every_diverging_column_is_a_clause() {
    let declared = schema(&[
        ("l_comment", DataType::Utf8View, false),
        ("l_orderkey", DataType::Int64, false),
        ("l_extendedprice", DataType::Decimal128(15, 2), false),
    ]);
    let exported = schema(&[
        ("l_comment", DataType::Utf8, false),
        ("l_orderkey", DataType::Int64, false),
        ("l_extendedprice", DataType::Decimal128(38, 2), false),
    ]);
    assert_eq!(
        schema_divergence(&declared, &exported),
        "0 l_comment: Utf8View vs Utf8; 2 l_extendedprice: Decimal128(15, 2) vs Decimal128(38, 2)"
    );
}

/// `try_new` does not check nullability, so that difference never reaches the sink, and a
/// clause for it would put a class in the survey's report that does not occur.
#[test]
fn nullability_alone_is_no_divergence() {
    let declared = schema(&[("l_orderkey", DataType::Int64, false)]);
    let exported = schema(&[("l_orderkey", DataType::Int64, true)]);
    assert_eq!(schema_divergence(&declared, &exported), "");
}
