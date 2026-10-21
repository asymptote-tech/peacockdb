//! The registry's CSV split, over strings.

use super::split_csv_line;

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
