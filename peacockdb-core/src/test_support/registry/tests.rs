//! The registry's CSV split and its `stored` column, over strings.

use super::{
    Stored, gpu_fingerprint_mismatch, split_csv_line, stored_cell, stored_format, stored_mismatch,
    stored_spelling_is_valid,
};

/// A q61 spelling: a quoted field holding commas, parentheses and doubled quotes reads back
/// as one field, and the fields around it keep their places.
#[test]
fn a_quoted_field_keeps_its_commas_and_quotes() {
    let name = "promotional_sales.promotions / all_sales.total * Int64(100)";
    let line = format!(
        "tpcds,1,q61,,\"duckdb(no_sort, rel(1e-5, \"\"{name}\"\"))\",\"data_fusion(sort, exact)\",\
         golden(sort; exact),enabled"
    );
    assert_eq!(
        split_csv_line(&line),
        [
            "tpcds".to_string(),
            "1".into(),
            "q61".into(),
            "".into(),
            format!("duckdb(no_sort, rel(1e-5, \"{name}\"))"),
            "data_fusion(sort, exact)".into(),
            "golden(sort; exact)".into(),
            "enabled".into(),
        ]
    );
}

#[test]
fn an_unquoted_line_splits_at_every_comma() {
    assert_eq!(split_csv_line("a,,b"), ["a", "", "b"]);
}

const R: Stored = Stored::Result;
const F: Stored = Stored::Fingerprint;

/// A `skipped:` or `failed:` marker stands in for no section, so it never makes a cell three
/// parts; a fingerprint and a table are the two formats.
#[test]
fn a_marker_is_no_section_and_a_section_is_its_format() {
    assert_eq!(stored_format("skipped: not enabled at this mode\n"), None);
    assert_eq!(stored_format("failed: the device refused q2\n"), None);
    assert_eq!(stored_format("fingerprint: rows=2513\ncols=8\n"), Some(F));
    assert_eq!(
        stored_format("mode=tp4-sized\n+---+\n| a |\n+---+\n| 1 |\n+---+\n"),
        Some(R)
    );
}

#[test]
fn stored_cell_names_each_shape() {
    assert_eq!(stored_cell(Some(R), Some(R), &[R; 5]), "result");
    assert_eq!(stored_cell(Some(R), Some(R), &[]), "result");
    assert_eq!(stored_cell(Some(F), Some(F), &[F; 5]), "fingerprint");
    assert_eq!(
        stored_cell(Some(R), Some(R), &[F; 5]),
        "result|result|fingerprint"
    );
    assert_eq!(stored_cell(Some(F), Some(R), &[]), "fingerprint|result|—");
    assert_eq!(
        stored_cell(Some(R), Some(R), &[R, F]),
        "result|result|mixed"
    );
}

/// A side with no section is left out of the one-word rule: `duckdb_none`'s pbench line has
/// no cpu section and reads as DuckDB's format alone.
#[test]
fn a_side_with_no_section_leaves_the_cell_one_word() {
    assert_eq!(stored_cell(None, Some(R), &[]), "result");
    assert_eq!(stored_cell(None, Some(F), &[R]), "—|fingerprint|result");
}

#[test]
#[should_panic(expected = "no section on any side")]
fn stored_cell_refuses_a_query_with_no_section() {
    stored_cell(None, None, &[]);
}

#[test]
fn a_wrong_stored_cell_names_its_side() {
    let said = stored_mismatch(
        "tpcds",
        "q2",
        "result|result|result",
        "result|result|fingerprint",
    )
    .expect("a mismatch");
    assert!(said.contains("tpcds/q2"), "{said}");
    assert!(
        said.contains("the registry says result|result|result"),
        "{said}"
    );
    assert!(said.contains("(gpu differs)"), "{said}");
    let said = stored_mismatch("tpch", "q1", "result", "fingerprint").expect("a mismatch");
    assert!(said.contains("(cpu differs)"), "{said}");
    let said = stored_mismatch(
        "tpch",
        "q1",
        "result|fingerprint|result",
        "result|result|result",
    )
    .expect("a mismatch");
    assert!(said.contains("(duckdb differs)"), "{said}");
    assert_eq!(
        stored_mismatch(
            "tpcds",
            "q2",
            "result|result|fingerprint",
            "result|result|fingerprint"
        ),
        None
    );
}

/// A line declaring `gpu: fingerprint` with its device cells off has no device section, so
/// nothing on the device side would ever read the part: its `stored` gpu part must say
/// `fingerprint`, and one that says anything else names the part to drop.
#[test]
fn a_declared_gpu_fingerprint_must_be_stored() {
    let declared = "duckdb(no_sort, exact, gpu: fingerprint)";
    assert_eq!(
        gpu_fingerprint_mismatch("tpcds", "q2", declared, "result|result|fingerprint"),
        None
    );
    for stored in ["result", "result|result|—", "result|result|mixed"] {
        let said = gpu_fingerprint_mismatch("tpcds", "q2", declared, stored).expect(stored);
        assert!(said.contains("tpcds/q2"), "{said}");
        assert!(said.contains(&format!("stores {stored}")), "{said}");
        assert!(said.contains("gpu: fingerprint"), "{said}");
    }
    for (spelled, stored) in [
        ("duckdb(no_sort, exact)", "result"),
        ("duckdb(no_sort, exact)", "result|result|fingerprint"),
        ("duckdb_fingerprint", "fingerprint"),
        ("duckdb_none", "—|result|—"),
    ] {
        assert_eq!(
            gpu_fingerprint_mismatch("tpcds", "q2", spelled, stored),
            None,
            "{spelled}"
        );
    }
}

#[test]
fn a_stored_spelling_is_one_word_three_parts_or_na() {
    for good in [
        "result",
        "fingerprint",
        "result|result|fingerprint",
        "—|result|mixed",
    ] {
        assert_eq!(stored_spelling_is_valid(good, true), Ok(()), "{good}");
    }
    assert_eq!(stored_spelling_is_valid("na", false), Ok(()));
    let said = stored_spelling_is_valid("rows", true).expect_err("rows");
    assert!(!said.contains("  "), "one space between words: {said}");
    for bad in [
        "",
        "rows",
        "result|result",
        "mixed|result|result",
        "result|—|—|—",
    ] {
        stored_spelling_is_valid(bad, true).expect_err(bad);
    }
    stored_spelling_is_valid("na", true).expect_err("na on a corpus line");
    stored_spelling_is_valid("result", false).expect_err("a format on a row with no line");
}
