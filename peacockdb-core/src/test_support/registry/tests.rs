//! The registry's CSV split and its `stored` column, over strings.

use super::{Stored, split_csv_line, stored_cell, stored_mismatch, stored_spelling_is_valid};

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
