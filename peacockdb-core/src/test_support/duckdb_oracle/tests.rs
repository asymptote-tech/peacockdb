//! The oracle keyword parsed, and the section comparison it names: two rendered tables in,
//! a verdict out. No dataset, no run — the doctored sections are what prove each variant
//! fails where it should.

use crate::test_support::{CellKind, DUCKDB_FLOAT_TOLERANCE, DuckdbOracle, compare_sections};

// --- the keyword -------------------------------------------------------------

#[test]
fn every_spelling_parses_and_names_itself() {
    assert_eq!(DuckdbOracle::parse("duckdb_exact"), DuckdbOracle::Exact);
    assert_eq!(DuckdbOracle::parse("duckdb_approx"), DuckdbOracle::Approx);
    assert_eq!(
        DuckdbOracle::parse("duckdb_fingerprint"),
        DuckdbOracle::Fingerprint
    );
    assert_eq!(DuckdbOracle::parse("duckdb_none"), DuckdbOracle::None);
    for name in DuckdbOracle::ALL {
        let sample = match name {
            "duckdb_divergent" => "duckdb_divergent(1)",
            other => other,
        };
        assert_eq!(DuckdbOracle::parse(sample).name(), name);
    }
}

/// `stringify!` of a parenthesized argument list puts spaces in, so the parse drops them
/// rather than every line having to be written without any.
#[test]
fn a_divergent_keeps_its_ticket_and_its_positions_through_stringifys_spacing() {
    assert_eq!(
        DuckdbOracle::parse("duckdb_divergent(243)"),
        DuckdbOracle::Divergent {
            ticket: 243,
            columns: vec![]
        }
    );
    assert_eq!(
        DuckdbOracle::parse("duckdb_divergent (243, 1, 2)"),
        DuckdbOracle::Divergent {
            ticket: 243,
            columns: vec![1, 2]
        }
    );
}

#[test]
#[should_panic(expected = "duckdb_exact|duckdb_approx")]
fn a_typo_names_the_accepted_set() {
    DuckdbOracle::parse("duckdb_exakt");
}

#[test]
#[should_panic(expected = "a ticket first")]
fn a_divergent_with_no_ticket_says_so() {
    DuckdbOracle::parse("duckdb_divergent()");
}

// --- the comparator ----------------------------------------------------------

/// Two rows, two columns, and DIFFERENT column names on the two sides: the comparison is by
/// position, since the two engines name an aggregate's output differently and a name is not
/// an answer.
const OURS: &str = "+---+-----+\n| a | b   |\n+---+-----+\n| 1 | x   |\n| 2 | 1.5 |\n+---+-----+";
const DUCK: &str = "+---+-----+\n| a | sum |\n+---+-----+\n| 1 | x   |\n| 2 | 1.5 |\n+---+-----+\n";
const KINDS: [CellKind; 2] = [CellKind::Exact, CellKind::Exact];

fn open(_: u32) -> bool {
    true
}

fn closed(_: u32) -> bool {
    false
}

fn divergent(ticket: u32, columns: Vec<usize>) -> DuckdbOracle {
    DuckdbOracle::Divergent { ticket, columns }
}

fn compare(
    oracle: &DuckdbOracle,
    ours: &str,
    duckdb: &str,
    kinds: &[CellKind],
) -> Result<(), String> {
    compare_sections(oracle, ours, duckdb, kinds, &open)
}

#[test]
fn exact_passes_by_position_whatever_the_names() {
    assert_eq!(compare(&DuckdbOracle::Exact, OURS, DUCK, &KINDS), Ok(()));
}

#[test]
fn a_wrong_row_fails_exact() {
    let bad = DUCK.replace("| 2 | 1.5 |", "| 3 | 1.5 |");
    let said = compare(&DuckdbOracle::Exact, OURS, &bad, &KINDS).expect_err("a wrong row");
    assert!(said.contains("row"), "{said}");
}

/// Row order is not an answer either: both sides sort their own rendering, at their own
/// widths, so the comparator pairs the rows itself.
#[test]
fn the_rows_are_a_multiset_and_their_order_is_not_compared() {
    let reversed = "+---+-----+\n| a | sum |\n+---+-----+\n| 2 | 1.5 |\n| 1 | x   |\n+---+-----+\n";
    assert_eq!(
        compare(&DuckdbOracle::Exact, OURS, reversed, &KINDS),
        Ok(())
    );
}

#[test]
fn approx_holds_a_float_to_the_relative_tolerance_and_exact_does_not() {
    let ours = "+-----+\n| x   |\n+-----+\n| 1.5 |\n+-----+";
    let near = "+-----------------+\n| x               |\n+-----------------+\n| 1.5000000000001 |\n+-----------------+\n";
    let far = "+--------+\n| x      |\n+--------+\n| 1.5001 |\n+--------+\n";
    assert_eq!(
        compare(&DuckdbOracle::Approx, ours, near, &[CellKind::Float]),
        Ok(())
    );
    assert!(compare(&DuckdbOracle::Approx, ours, far, &[CellKind::Float]).is_err());
    assert!(compare(&DuckdbOracle::Exact, ours, near, &[CellKind::Float]).is_err());
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
        compare(&DuckdbOracle::Approx, ours, duck, &[CellKind::Decimal(6)]),
        Ok(())
    );
    assert!(compare(&DuckdbOracle::Approx, ours, &off, &[CellKind::Decimal(6)]).is_err());
    // The same cell under the float rule would be a divergence, which is the point.
    assert!(compare(&DuckdbOracle::Approx, ours, duck, &[CellKind::Float]).is_err());
}

#[test]
fn divergent_needs_a_difference_and_an_open_ticket() {
    let bad = DUCK.replace("| 2 | 1.5 |", "| 2 | 9.5 |");
    assert_eq!(compare(&divergent(80, vec![1]), OURS, &bad, &KINDS), Ok(()));
    let stopped = compare(&divergent(80, vec![1]), OURS, DUCK, &KINDS)
        .expect_err("a divergence that is no longer one");
    assert!(stopped.contains("stopped diverging"), "{stopped}");
    let archived = compare_sections(&divergent(80, vec![1]), OURS, &bad, &KINDS, &closed)
        .expect_err("a ticket nobody can read");
    assert!(archived.contains("#80 is not open"), "{archived}");
}

#[test]
fn divergent_still_checks_the_row_count_and_the_columns_it_does_not_name() {
    let col0_wrong = DUCK.replace("| 2 | 1.5 |", "| 7 | 9.5 |");
    let said = compare(&divergent(80, vec![1]), OURS, &col0_wrong, &KINDS)
        .expect_err("a column nobody declared divergent");
    assert!(said.contains("column 0"), "{said}");

    let extra_row = "+---+-----+\n| a | sum |\n+---+-----+\n| 1 | x   |\n| 2 | 9.5 |\n| 3 | z   |\n+---+-----+\n";
    let said = compare(&divergent(80, vec![]), OURS, extra_row, &KINDS)
        .expect_err("a row-level divergence still counts the rows");
    assert!(said.contains("rows"), "{said}");
}

/// Empty positions mean the ROW SET diverges, so only the count is checked — but a line
/// that diverges nowhere at all is still a line that stopped diverging.
#[test]
fn divergent_with_no_positions_checks_the_count_and_nothing_more() {
    let every_cell_wrong =
        "+---+-----+\n| a | sum |\n+---+-----+\n| 8 | y   |\n| 9 | 2.5 |\n+---+-----+\n";
    assert_eq!(
        compare(&divergent(80, vec![]), OURS, every_cell_wrong, &KINDS),
        Ok(())
    );
    let said = compare(&divergent(80, vec![]), OURS, DUCK, &KINDS)
        .expect_err("two identical sections diverge nowhere");
    assert!(said.contains("stopped diverging"), "{said}");
}

#[test]
fn a_column_count_difference_fails_before_any_row_is_compared() {
    let wide = "+---+-----+---+\n| a | sum | c |\n+---+-----+---+\n| 1 | x   | 0 |\n| 2 | 1.5 | 0 |\n+---+-----+---+\n";
    let said = compare(&DuckdbOracle::Exact, OURS, wide, &KINDS).expect_err("a dropped column");
    assert!(said.contains("2 columns against 3"), "{said}");
}

#[test]
fn a_fingerprinted_section_needs_the_fingerprint_oracle() {
    let fp = "fingerprint: rows=2\ncol 0: nonnull=2\ncol 1: nonnull=2\nhash: 00\n";
    let said = compare(&DuckdbOracle::Exact, fp, fp, &KINDS)
        .expect_err("an over-cap section under an oracle that reads rows");
    assert!(said.contains("duckdb_fingerprint"), "{said}");
    let said = compare(&DuckdbOracle::Approx, OURS, fp, &KINDS)
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
    let ours_as_fingerprint = crate::test_support::fingerprint_of_rendered(OURS);
    assert_eq!(
        compare(
            &DuckdbOracle::Fingerprint,
            OURS,
            &ours_as_fingerprint,
            &KINDS
        ),
        Ok(())
    );
}

#[test]
fn none_over_two_sections_that_both_exist_fails() {
    let said = compare(&DuckdbOracle::None, OURS, DUCK, &KINDS)
        .expect_err("duckdb_none where both sides answer");
    assert!(said.contains("duckdb_none"), "{said}");
}

/// A NULL renders as an empty cell on both sides, and it is not a number: the tolerance
/// cannot make it equal to one.
#[test]
fn a_null_is_not_within_any_tolerance_of_a_value() {
    let ours = "+-----+\n| x   |\n+-----+\n|     |\n+-----+";
    let duck = "+-----+\n| x   |\n+-----+\n| 0.0 |\n+-----+\n";
    assert!(compare(&DuckdbOracle::Approx, ours, duck, &[CellKind::Float]).is_err());
    assert_eq!(
        compare(&DuckdbOracle::Approx, ours, ours, &[CellKind::Float]),
        Ok(())
    );
}

/// tpcds q17's shape: both sides answer zero rows, and ours renders no columns at all
/// because the cpu emits no batch to take a schema from (#205). The row set is what
/// diverges, so a line with no positions admits it — and a column-count difference under
/// any other oracle still fails before a row is read.
#[test]
fn a_row_level_divergence_admits_a_column_count_difference() {
    let ours = "++\n++\n";
    let duck = "+---+-----+\n| a | sum |\n+---+-----+\n+---+-----+\n";
    assert_eq!(compare(&divergent(205, vec![]), ours, duck, &KINDS), Ok(()));
    assert!(
        compare(&DuckdbOracle::Exact, ours, duck, &KINDS)
            .expect_err("a dropped schema under an oracle that reads rows")
            .contains("columns against")
    );
}

/// Each named column has to diverge somewhere. One that came right is a line to update and
/// a ticket to look at closing, not a column to go on excusing — and the set as a whole
/// still differing would hide it.
#[test]
fn divergent_names_each_column_and_one_that_came_right_fails() {
    let ours = "+---+---+\n| a | b |\n+---+---+\n| 1 | 2 |\n+---+---+";
    let duck = "+---+---+\n| a | b |\n+---+---+\n| 1 | 9 |\n+---+---+\n";
    let kinds = [CellKind::Exact, CellKind::Exact];
    assert_eq!(compare(&divergent(80, vec![1]), ours, duck, &kinds), Ok(()));
    let said = compare(&divergent(80, vec![0, 1]), ours, duck, &kinds)
        .expect_err("column 0 agrees and the line says it diverges");
    assert!(said.contains("column 0"), "{said}");
    assert!(said.contains("stopped diverging"), "{said}");
}

/// The ticket reader, both ways, over the committed files. An archived ticket must read as
/// closed, or a `duckdb_divergent` line goes on excusing a divergence after the fix landed —
/// which is the one thing the number on that line is there to prevent.
#[test]
fn an_archived_ticket_is_not_open_and_a_listed_one_is() {
    use crate::test_support::ticket_is_open;
    assert!(ticket_is_open(235), "#235 is in llm-wiki/tickets/");
    assert!(ticket_is_open(205), "#205 is in llm-wiki/tickets/");
    assert!(ticket_is_open(251), "#251 is in llm-wiki/tickets/");
    assert!(
        !ticket_is_open(191),
        "#191 is in llm-wiki/archive/archived-tickets.md, which is not a list of what is broken"
    );
    assert!(!ticket_is_open(99_999), "no file holds this number");
}
