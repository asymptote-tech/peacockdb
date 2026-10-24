//! The oracle spelling parsed, and the section comparison it names: two rendered tables in,
//! a verdict out. No dataset, no run — the doctored sections are what prove each variant
//! fails where it should.

use std::collections::BTreeSet;

use datafusion::arrow::array::StringArray;
use datafusion::arrow::compute::cast;
use datafusion::arrow::datatypes::{DataType, TimeUnit};
use datafusion::arrow::util::display::array_value_to_string;

use super::tight;
use crate::test_support::{
    CellKind, DUCKDB_FLOAT_TOLERANCE, DuckdbOracle, RowOrder, Values, compare_sections,
};

fn compare_on(order: RowOrder, values: Values) -> DuckdbOracle {
    DuckdbOracle::Compare {
        order,
        values,
        dates: vec![],
    }
}

fn exact() -> DuckdbOracle {
    compare_on(RowOrder::Sort, Values::Exact)
}

fn approx() -> DuckdbOracle {
    compare_on(RowOrder::Sort, Values::Approx)
}

fn rel(tolerance: f64, columns: &[&str]) -> DuckdbOracle {
    compare_on(
        RowOrder::Sort,
        Values::Rel {
            tolerance,
            columns: columns.iter().map(|c| c.to_string()).collect(),
        },
    )
}

fn dated(columns: &[&str]) -> DuckdbOracle {
    DuckdbOracle::Compare {
        order: RowOrder::Sort,
        values: Values::Exact,
        dates: columns.iter().map(|c| c.to_string()).collect(),
    }
}

// --- the spelling ------------------------------------------------------------

#[test]
fn the_grammar_parses_each_order_and_value() {
    assert_eq!(DuckdbOracle::parse("duckdb(sort, exact)"), exact());
    assert_eq!(
        DuckdbOracle::parse("duckdb (no_sort , approx)"),
        compare_on(RowOrder::NoSort, Values::Approx)
    );
    assert_eq!(
        DuckdbOracle::parse("duckdb(sort, exact, date(\"next_month\"))"),
        dated(&["next_month"])
    );
    assert_eq!(
        DuckdbOracle::parse("duckdb_fingerprint"),
        DuckdbOracle::Fingerprint
    );
    assert_eq!(DuckdbOracle::parse("duckdb_none"), DuckdbOracle::None);
}

/// q61's header: spaces, dots and parentheses inside the quotes, none of which may split the
/// name or lose a space.
#[test]
fn rel_takes_a_name_with_spaces_dots_and_parentheses() {
    let name = "promotional_sales.promotions / all_sales.total * Int64(100)";
    assert_eq!(
        DuckdbOracle::parse(&format!("duckdb(no_sort, rel(1e-5, \"{name}\"))")),
        compare_on(
            RowOrder::NoSort,
            Values::Rel {
                tolerance: 1e-5,
                columns: vec![name.into()]
            }
        )
    );
}

#[test]
#[should_panic(expected = "names columns by name")]
fn rel_refuses_a_position() {
    DuckdbOracle::parse("duckdb(sort, rel(1e-5, 2))");
}

#[test]
#[should_panic(expected = "names columns by name")]
fn date_refuses_a_position() {
    DuckdbOracle::parse("duckdb(sort, exact, date(3))");
}

#[test]
#[should_panic(expected = "unknown duckdb_oracle")]
fn the_old_spelling_is_refused() {
    DuckdbOracle::parse("duckdb_exact");
}

#[test]
#[should_panic(expected = "unknown duckdb_oracle")]
fn divergent_is_gone() {
    DuckdbOracle::parse("duckdb_divergent(251, 2)");
}

#[test]
#[should_panic(expected = "sort|no_sort")]
fn an_unknown_order_names_the_two() {
    DuckdbOracle::parse("duckdb(sorted, exact)");
}

#[test]
fn tight_keeps_the_spaces_inside_quotes_only() {
    assert_eq!(
        tight("duckdb (no_sort , rel(1e-5, \"a / b * c\"))"),
        "duckdb(no_sort,rel(1e-5,\"a / b * c\"))"
    );
}

#[test]
fn each_spelling_names_its_facets() {
    assert_eq!(
        DuckdbOracle::parse("duckdb(no_sort, rel(1e-5, \"a\"), date(\"d\"))").facets(),
        ["no_sort", "rel", "date"]
    );
    assert_eq!(exact().facets(), ["sort", "exact"]);
    assert_eq!(DuckdbOracle::Fingerprint.facets(), ["fingerprint"]);
    assert_eq!(DuckdbOracle::None.row_order(), RowOrder::Sort);
}

// --- the comparator ----------------------------------------------------------

/// Two rows, two columns, and DIFFERENT column names on the two sides: the comparison is by
/// position, since the two engines name an aggregate's output differently and a name is not
/// an answer.
const OURS: &str = "+---+-----+\n| a | b   |\n+---+-----+\n| 1 | x   |\n| 2 | 1.5 |\n+---+-----+";
const DUCK: &str = "+---+-----+\n| a | sum |\n+---+-----+\n| 1 | x   |\n| 2 | 1.5 |\n+---+-----+\n";
const KINDS: [CellKind; 2] = [CellKind::Exact, CellKind::Exact];

fn compare(
    oracle: &DuckdbOracle,
    ours: &str,
    duckdb: &str,
    kinds: &[CellKind],
) -> Result<(), String> {
    compare_sections(oracle, ours, duckdb, kinds)
}

/// A table with the header `a | b | d`, one row per `(a, b, d)`, its borders sized to the
/// widest cell as a rendering's are.
fn abd(rows: &[(&str, &str, &str)]) -> String {
    let all: Vec<[&str; 3]> = std::iter::once(["a", "b", "d"])
        .chain(rows.iter().map(|(a, b, d)| [*a, *b, *d]))
        .collect();
    let width = |i: usize| all.iter().map(|r| r[i].len()).max().unwrap_or(1);
    let rule = format!(
        "+{}+{}+{}+",
        "-".repeat(width(0) + 2),
        "-".repeat(width(1) + 2),
        "-".repeat(width(2) + 2)
    );
    let line = |r: &[&str; 3]| {
        format!(
            "| {:w0$} | {:w1$} | {:w2$} |",
            r[0],
            r[1],
            r[2],
            w0 = width(0),
            w1 = width(1),
            w2 = width(2)
        )
    };
    let mut out = vec![rule.clone(), line(&all[0]), rule.clone()];
    out.extend(all[1..].iter().map(line));
    out.push(rule);
    out.join("\n")
}

const ABD_KINDS: [CellKind; 3] = [CellKind::Decimal(6), CellKind::Decimal(2), CellKind::Exact];

/// DuckDB's rendering of a timestamp, parsed and printed by arrow-rs — the format
/// `duckdb_result.py`'s `cell()` writes, `%Y-%m-%dT%H:%M:%S` — rather than typed by hand.
fn duckdb_timestamp(date: &str, time: &str) -> String {
    let text = StringArray::from(vec![format!("{date} {time}")]);
    let parsed = cast(&text, &DataType::Timestamp(TimeUnit::Second, None)).expect("a timestamp");
    array_value_to_string(&parsed, 0).expect("a rendered timestamp")
}

#[test]
fn exact_passes_by_position_whatever_the_names() {
    assert_eq!(compare(&exact(), OURS, DUCK, &KINDS), Ok(()));
}

#[test]
fn a_wrong_row_fails_exact() {
    let bad = DUCK.replace("| 2 | 1.5 |", "| 3 | 1.5 |");
    let said = compare(&exact(), OURS, &bad, &KINDS).expect_err("a wrong row");
    assert!(said.contains("row"), "{said}");
}

/// Under `sort` row order is not an answer: both sides sort their own rendering, at their own
/// widths, so the comparator pairs the rows itself.
#[test]
fn under_sort_the_rows_are_a_multiset_and_their_order_is_not_compared() {
    let reversed = "+---+-----+\n| a | sum |\n+---+-----+\n| 2 | 1.5 |\n| 1 | x   |\n+---+-----+\n";
    assert_eq!(compare(&exact(), OURS, reversed, &KINDS), Ok(()));
}

#[test]
fn approx_holds_a_float_to_the_relative_tolerance_and_exact_does_not() {
    let ours = "+-----+\n| x   |\n+-----+\n| 1.5 |\n+-----+";
    let near = "+-----------------+\n| x               |\n+-----------------+\n| 1.5000000000001 |\n+-----------------+\n";
    let far = "+--------+\n| x      |\n+--------+\n| 1.5001 |\n+--------+\n";
    assert_eq!(compare(&approx(), ours, near, &[CellKind::Float]), Ok(()));
    assert!(compare(&approx(), ours, far, &[CellKind::Float]).is_err());
    assert!(compare(&exact(), ours, near, &[CellKind::Float]).is_err());
    assert!(DUCKDB_FLOAT_TOLERANCE < 1e-10);
}

/// tpch q1's `avg_qty`: ours truncates at scale 6 and DuckDB answers a double, so the two
/// differ by up to one unit in our last place — 1.2e-5 relative, far past any float
/// tolerance, which is why a decimal's bound is absolute and read off its scale.
#[test]
fn approx_holds_a_decimal_to_one_unit_in_our_last_place() {
    let ours = "+-----------+\n| avg_qty   |\n+-----------+\n| 25.522005 |\n+-----------+";
    let duck = "+--------------------+\n| avg_qty            |\n+--------------------+\n| 25.522005853257337 |\n+--------------------+\n";
    let off = duck.replace("25.522005853257337", "25.522007");
    assert_eq!(
        compare(&approx(), ours, duck, &[CellKind::Decimal(6)]),
        Ok(())
    );
    assert!(compare(&approx(), ours, &off, &[CellKind::Decimal(6)]).is_err());
    // The same cell under the float rule would be a divergence, which is the point.
    assert!(compare(&approx(), ours, duck, &[CellKind::Float]).is_err());
}

/// A NEGATIVE declared scale does not widen the tolerance. `10^-s` with `s = -2` is a
/// hundred, so a cell off by 50 would have passed; arrow allows the scale and the corpus has
/// never produced one, which is why this is a case rather than a divergence somebody saw.
#[test]
fn a_negative_decimal_scale_does_not_buy_a_wider_tolerance() {
    let ours = "+-------+\n| x     |\n+-------+\n| 100.0 |\n+-------+";
    let off = "+-------+\n| x     |\n+-------+\n| 150.0 |\n+-------+\n";
    assert!(compare(&approx(), ours, off, &[CellKind::Decimal(-2)]).is_err());
}

/// A zero-column answer has a header with no pipes in it, and `split_cells` reads a pipeless
/// line as one cell. An answer that lost its columns renders `++`, and the width it reports is
/// in the message a width difference prints.
#[test]
fn a_zero_column_answer_reports_no_columns() {
    let none = "mode=tp4-sized\n++\n++\n";
    let fifteen = format!(
        "+{}\n|{}\n+{}\n",
        "---+".repeat(15),
        " a |".repeat(15),
        "---+".repeat(15)
    );
    let said = compare(&exact(), none, &fifteen, &[]).expect_err("no columns against fifteen");
    assert!(said.contains("0 columns against 15"), "{said}");
}

/// q58's `ss_dev`: our truncated quotient and DuckDB's double, 6.2e-7 relative apart.
#[test]
fn rel_passes_inside_its_tolerance() {
    let ours = abd(&[
        ("103.719200", "1.00", "2024-01-01"),
        ("1.000000", "2.00", "2024-01-02"),
    ]);
    let duck = abd(&[
        ("103.71926462", "1.00", "2024-01-01"),
        ("1.0", "2.00", "2024-01-02"),
    ]);
    assert_eq!(
        compare(&rel(1e-5, &["a"]), &ours, &duck, &ABD_KINDS),
        Ok(())
    );
    assert!(
        compare(&approx(), &ours, &duck, &ABD_KINDS).is_err(),
        "the gap is past approx's last-place unit, which is why the line needs rel"
    );
}

/// The bound is relative to the larger magnitude: 2e-5 of 1.00002 is 2.00004e-5, so a gap
/// of exactly 2e-5 is inside it and one of 2.00005e-5 is not.
#[test]
fn rel_fails_just_outside_its_tolerance() {
    let ours = abd(&[("1.000000", "1.00", "2024-01-01")]);
    let inside = abd(&[("1.00002", "1.00", "2024-01-01")]);
    let outside = abd(&[("1.0000200005", "1.00", "2024-01-01")]);
    assert_eq!(
        compare(&rel(2e-5, &["a"]), &ours, &inside, &ABD_KINDS),
        Ok(())
    );
    let said = compare(&rel(2e-5, &["a"]), &ours, &outside, &ABD_KINDS)
        .expect_err("2.00005e-5 apart under a 2e-5 tolerance");
    assert!(said.contains("(a)"), "{said}");
}

#[test]
fn an_unnamed_column_stays_at_approx() {
    let ours = abd(&[("1.000000", "1.00", "2024-01-01")]);
    let duck = abd(&[("1.0", "1.02", "2024-01-01")]);
    let said = compare(&rel(1e-5, &["a"]), &ours, &duck, &ABD_KINDS)
        .expect_err("b is two last-place units off and rel does not name it");
    assert!(said.contains("(b)"), "{said}");
}

#[test]
fn rel_and_date_fail_on_a_column_the_header_does_not_have() {
    let table = abd(&[("1.000000", "1.00", "2024-01-01")]);
    for oracle in [rel(1e-5, &["zz"]), dated(&["zz"])] {
        let said = compare(&oracle, &table, &table, &ABD_KINDS).expect_err("no column zz");
        assert!(said.contains("\"zz\""), "{said}");
        assert!(said.contains("a, b, d"), "the header's names: {said}");
    }
}

/// A name the header carries twice is refused rather than read at its first position.
#[test]
fn rel_fails_on_a_name_the_header_has_twice() {
    let table = "+---+---+\n| a | a |\n+---+---+\n| 1 | 2 |\n+---+---+";
    let kinds = [CellKind::Exact, CellKind::Exact];
    let said = compare(&rel(1e-5, &["a"]), table, table, &kinds).expect_err("two columns a");
    assert!(said.contains("2 times"), "{said}");
}

/// `date ± interval`: DuckDB answers a timestamp where DataFusion answers a date, so a named
/// column's DuckDB cell compares by its date part, whatever its time.
#[test]
fn date_compares_the_date_part() {
    let ours = abd(&[("1.000000", "1.00", "2024-02-29")]);
    for time in ["00:00:00", "12:00:00"] {
        let duck = abd(&[("1.000000", "1.00", &duckdb_timestamp("2024-02-29", time))]);
        assert_eq!(compare(&dated(&["d"]), &ours, &duck, &ABD_KINDS), Ok(()));
        assert!(compare(&exact(), &ours, &duck, &ABD_KINDS).is_err());
    }
    assert_eq!(
        duckdb_timestamp("2024-02-29", "00:00:00"),
        "2024-02-29T00:00:00"
    );
}

#[test]
fn date_fails_on_another_day() {
    let ours = abd(&[("1.000000", "1.00", "2024-03-10")]);
    let duck = abd(&[(
        "1.000000",
        "1.00",
        &duckdb_timestamp("2024-03-09", "12:00:00"),
    )]);
    let said = compare(&dated(&["d"]), &ours, &duck, &ABD_KINDS).expect_err("a day apart");
    assert!(said.contains("(d)"), "{said}");
}

/// Two rows whose dates differ only in DuckDB's time part sort and pair by their dates: the
/// time cannot reorder DuckDB's rows against ours.
#[test]
fn date_pairs_rows_under_sort() {
    let ours = abd(&[
        ("1.000000", "1.00", "2024-01-02"),
        ("2.000000", "1.00", "2024-01-01"),
    ]);
    let duck = abd(&[
        (
            "2.000000",
            "1.00",
            &duckdb_timestamp("2024-01-01", "23:00:00"),
        ),
        (
            "1.000000",
            "1.00",
            &duckdb_timestamp("2024-01-02", "00:00:00"),
        ),
    ]);
    assert_eq!(compare(&dated(&["d"]), &ours, &duck, &ABD_KINDS), Ok(()));
}

/// `no_sort` pairs rows by position: the same two rows swapped is an answer in the wrong
/// order, and it fails at the first row out of place.
#[test]
fn no_sort_fails_on_two_swapped_rows() {
    let swapped = "+---+-----+\n| a | sum |\n+---+-----+\n| 2 | 1.5 |\n| 1 | x   |\n+---+-----+\n";
    let no_sort = compare_on(RowOrder::NoSort, Values::Exact);
    let said = compare(&no_sort, OURS, swapped, &KINDS).expect_err("two rows out of order");
    assert!(said.starts_with("row 0,"), "{said}");
    assert_eq!(compare(&exact(), OURS, swapped, &KINDS), Ok(()));
}

#[test]
fn no_sort_passes_rows_in_the_same_order() {
    for values in [Values::Exact, Values::Approx] {
        let no_sort = compare_on(RowOrder::NoSort, values);
        assert_eq!(compare(&no_sort, OURS, DUCK, &KINDS), Ok(()));
    }
}

#[test]
fn a_column_count_difference_fails_before_any_row_is_compared() {
    let wide = "+---+-----+---+\n| a | sum | c |\n+---+-----+---+\n| 1 | x   | 0 |\n| 2 | 1.5 | 0 |\n+---+-----+---+\n";
    let said = compare(&exact(), OURS, wide, &KINDS).expect_err("a dropped column");
    assert!(said.contains("2 columns against 3"), "{said}");
}

#[test]
fn a_fingerprinted_section_needs_the_fingerprint_oracle() {
    let fp = "fingerprint: rows=2\ncol 0: nonnull=2\ncol 1: nonnull=2\nhash: 00\n";
    let said = compare(&exact(), fp, fp, &KINDS)
        .expect_err("an over-cap section under an oracle that reads rows");
    assert!(said.contains("duckdb_fingerprint"), "{said}");
    let said = compare(&approx(), OURS, fp, &KINDS)
        .expect_err("one side over the cap and the line says otherwise");
    assert!(said.contains("duckdb_fingerprint"), "{said}");
}

#[test]
fn the_fingerprint_oracle_needs_a_fingerprinted_section() {
    let said = compare(&DuckdbOracle::Fingerprint, OURS, DUCK, &KINDS)
        .expect_err("duckdb_fingerprint over two sections that hold their rows");
    assert!(said.contains("not fingerprinted"), "{said}");
}

/// Near the cap one writer renders what the other fingerprints — DuckDB's `repr` prints
/// longer floats than arrow-rs. The comparator fingerprints the rendered side itself rather
/// than failing to parse the other.
#[test]
fn one_side_fingerprinted_near_the_cap_is_fingerprinted_here_too() {
    let as_fingerprint = crate::test_support::fingerprint_of_rendered(OURS, &[false, false]);
    assert_eq!(
        compare(&DuckdbOracle::Fingerprint, OURS, &as_fingerprint, &KINDS),
        Ok(())
    );
}

#[test]
fn none_over_two_sections_that_both_exist_fails() {
    let said = compare(&DuckdbOracle::None, OURS, DUCK, &KINDS)
        .expect_err("duckdb_none where both sides answer");
    assert!(said.contains("duckdb_none"), "{said}");
}

/// A NULL renders as an empty cell on both sides, and it is not a number: no tolerance can
/// make it equal to one.
#[test]
fn a_null_is_not_within_any_tolerance_of_a_value() {
    let ours = "+-----+\n| x   |\n+-----+\n|     |\n+-----+";
    let duck = "+-----+\n| x   |\n+-----+\n| 0.0 |\n+-----+\n";
    assert!(compare(&approx(), ours, duck, &[CellKind::Float]).is_err());
    assert!(compare(&rel(1.0, &["x"]), ours, duck, &[CellKind::Float]).is_err());
    assert_eq!(compare(&approx(), ours, ours, &[CellKind::Float]), Ok(()));
}

/// A column-count difference fails under every oracle that reads rows: an engine that dropped
/// a column is wrong whatever the rows do. Both sides here answer zero rows, the one shape
/// where the width is all there is to compare.
#[test]
fn a_column_count_difference_fails_under_every_oracle() {
    let ours = "++\n++\n";
    let duck = "+---+-----+\n| a | sum |\n+---+-----+\n+---+-----+\n";
    for oracle in [
        exact(),
        approx(),
        compare_on(RowOrder::NoSort, Values::Exact),
    ] {
        let said = compare(&oracle, ours, duck, &KINDS)
            .expect_err("a dropped schema under an oracle that reads rows");
        assert!(said.contains("0 columns against 2"), "{oracle:?}: {said}");
    }
}

// --- the gpu-result file's coverage ------------------------------------------

/// Every enabled device cell has its section and no section is any other cell's, in both
/// directions, and an ABSENT file is a failure only where some cell is enabled.
///
/// The degenerate case is the one a hardcoded dataset list got wrong: a dataset whose device
/// cells are all off has no file at all, because the writer runs from a device case and zero
/// enabled cells write zero sections. pbench lands that way.
#[test]
fn a_dataset_with_no_enabled_device_cell_has_no_file_and_that_is_not_a_failure() {
    use super::gpu_result_coverage;
    let path = std::path::Path::new("testdata/goldens/pbench.sf1/gpu-result.txt");
    let none: BTreeSet<(String, String)> = BTreeSet::new();
    assert_eq!(gpu_result_coverage(path, &none, None), Ok(()));
    let said = gpu_result_coverage(path, &none, Some("== q1 mode=tp1-single\nan answer\n"))
        .expect_err("a section for a cell nothing enables");
    assert!(said.contains("not an enabled cell"), "{said}");
}

/// The two directions over a file that exists, and the absence that IS a failure: a dataset
/// with enabled cells and no file is a cycle nobody ran since the cells moved.
#[test]
fn an_enabled_device_cell_without_its_section_names_the_cycle_to_run() {
    use super::gpu_result_coverage;
    let path = std::path::Path::new("testdata/goldens/tpch.sf1/gpu-result.txt");
    let cell = |query: &str, mode: &str| (query.to_string(), mode.to_string());
    let enabled: BTreeSet<(String, String)> =
        [cell("q6", "tp1-single"), cell("q6", "tp4-sized")].into();
    let whole = "cudf=25.02\n== q6 mode=tp1-single\na\n== q6 mode=tp4-sized\nb\n";
    assert_eq!(gpu_result_coverage(path, &enabled, Some(whole)), Ok(()));
    let said = gpu_result_coverage(
        path,
        &enabled,
        Some("cudf=25.02\n== q6 mode=tp1-single\na\n"),
    )
    .expect_err("one of the two cells has no section");
    assert!(
        said.contains("missing") && said.contains("tp4-sized"),
        "{said}"
    );
    let said = gpu_result_coverage(path, &enabled, None).expect_err("enabled cells and no file");
    assert!(
        said.contains("does not exist") && said.contains("PCK_WRITE_GPU_RESULT=1"),
        "{said}"
    );
}
