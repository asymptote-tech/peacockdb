//! The last `corpus_query!` argument decoded, with and without a mode mask: no golden, no
//! device. A mask is what lets one line keep the hook at the modes where it is green, so
//! each wrong spelling of one has to name the row rather than run a cell unvalidated.

use super::is_validated_at;
use crate::test_support::{MODES, Mode};

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
