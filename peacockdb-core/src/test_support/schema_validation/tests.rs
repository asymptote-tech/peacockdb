//! The null-count half of the validator's comparison (#227), over declarations and counts
//! rather than over a run.
//!
//! A count rather than an array, because that is the shape a device read gives: a cuDF column
//! holds a null count and no validity labels, so the rule the gpu flavour will call once
//! something reads them cannot be written over arrow arrays.

use datafusion::arrow::datatypes::{DataType, Field, Schema as ArrowSchema};

use super::nulls_where_none_declared;

/// Int32 columns under the given names and nullability — the type is not an input here.
fn declared(fields: &[(&str, bool)]) -> ArrowSchema {
    ArrowSchema::new(
        fields
            .iter()
            .map(|(name, nullable)| Field::new(*name, DataType::Int32, *nullable))
            .collect::<Vec<_>>(),
    )
}

#[test]
fn a_null_in_a_column_declared_non_nullable_is_named_with_its_position_and_count() {
    let said = nulls_where_none_declared(&declared(&[("a", true), ("b", false)]), &[3, 7])
        .expect("b holds NULLs and declares none");
    assert!(said.contains("1 b: 7 NULL(s)"), "{said}");
    assert!(said.contains("#227"), "{said}");
    assert!(!said.contains(" a:"), "a declares its NULLs: {said}");
}

#[test]
fn a_column_declaring_nulls_and_one_holding_none_are_both_silent() {
    let schema = declared(&[("a", true), ("b", false)]);
    assert_eq!(nulls_where_none_declared(&schema, &[9, 0]), None);
}

#[test]
fn every_violating_column_is_named_rather_than_the_first() {
    let schema = declared(&[("a", false), ("b", true), ("c", false)]);
    let said = nulls_where_none_declared(&schema, &[1, 2, 4]).expect("a and c both violate");
    assert!(said.contains("0 a: 1 NULL(s)"), "{said}");
    assert!(said.contains("2 c: 4 NULL(s)"), "{said}");
}
