//! The validator's two inputs that need no run: the null-count half of its comparison (#227),
//! over declarations and counts, and the last `corpus_query!` argument decoded with and without
//! a mode mask.
//!
//! A count rather than an array, because that is the shape a device read gives: a cuDF column
//! holds a null count and no validity labels, so the rule the gpu flavour will call once
//! something reads them cannot be written over arrow arrays. A mask is what lets one line keep
//! the hook at the modes where it is green, so each wrong spelling of one has to name the row
//! rather than run a cell unvalidated.

use datafusion::arrow::datatypes::{DataType, Field, Schema as ArrowSchema};

use super::{is_validated_at, nulls_where_none_declared};
use crate::test_support::{MODES, Mode};

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

fn mode(ident: &str) -> &'static Mode {
    MODES
        .iter()
        .find(|mode| mode.ident() == ident)
        .expect("one of the five")
}

#[test]
fn enabled_holds_every_batch_at_every_mode() {
    for m in &MODES {
        assert!(is_validated_at("schema_validation_enabled", m, "what"));
    }
}

#[test]
fn a_bare_disabled_holds_nothing_at_any_mode() {
    for m in &MODES {
        assert!(!is_validated_at("schema_validation_disabled", m, "what"));
    }
}

#[test]
fn a_masked_disabled_holds_every_mode_the_mask_does_not_name() {
    let declared = "schema_validation_disabled(tp4_single | tp4_rowgroup | tp4_sized)";
    assert!(is_validated_at(declared, mode("tp1_single"), "what"));
    assert!(is_validated_at(declared, mode("tp1_rowgroup"), "what"));
    assert!(!is_validated_at(declared, mode("tp4_single"), "what"));
    assert!(!is_validated_at(declared, mode("tp4_rowgroup"), "what"));
    assert!(!is_validated_at(declared, mode("tp4_sized"), "what"));
}

#[test]
#[should_panic(expected = "names no mode of the five")]
fn a_mask_misspelling_a_mode_is_refused() {
    is_validated_at(
        "schema_validation_disabled(tp4_singel)",
        mode("tp4_single"),
        "what",
    );
}

#[test]
#[should_panic(expected = "takes no mode mask")]
fn a_mask_on_the_enabled_form_is_refused() {
    is_validated_at(
        "schema_validation_enabled(tp4_single)",
        mode("tp4_single"),
        "what",
    );
}

#[test]
#[should_panic(expected = "unknown schema validation")]
fn a_misspelled_keyword_is_refused() {
    is_validated_at("schema_validation_disbled", mode("tp1_single"), "what");
}
