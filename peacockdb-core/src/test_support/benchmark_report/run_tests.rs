//! The run clock beside the total: peacockdb's `run_us` and Sirius's execution window in the main
//! table, the run ratio, and `run_ms` in the CSV. The trees and readers are `tests.rs`'s.

use super::tests::{
    CSV_HEADER, TIMED, duckdb, failed, list, main_part, modes_row, order, page, put, row, sirius,
    tree,
};
use crate::test_support::{render_benchmarks_csv, section_href};

const SINGLE: &str = "h100-80gb-hbm3/tpch.sf40/tp1_single.benchmark.txt";

/// TIMED without its `run_us` line.
fn without_run_us() -> String {
    TIMED
        .lines()
        .filter(|line| !line.starts_with("run_us="))
        .map(|line| format!("{line}\n"))
        .collect()
}

#[test]
fn the_main_table_shows_peacockdb_s_run_after_its_total() {
    let root = tree("run-peacockdb");
    put(&root, SINGLE, &[("q3", TIMED), ("q6", &failed("boom"))]);
    let html = page(root, &[("tpch", "q3"), ("tpch", "q6")]);
    assert_eq!(row(&html, "q3"), ["q3", "1.0", "0.5", "1.5", "1.7", "2.00"]);
    let main = main_part(&html);
    assert!(
        main.contains("<th colspan=\"5\">h100-80gb-hbm3 · tp1_single</th>"),
        "{html}"
    );
    assert!(
        main.contains("<th>total (ms)</th><th>run (ms)</th><th title="),
        "{html}"
    );
    // Linked to the section's header, as the total is.
    let href = section_href(&format!("benchmark-results/{SINGLE}"), 1);
    assert!(
        main.contains(&format!("<td class=\"n\"><a href=\"{href}\">1.7</a></td>")),
        "{html}"
    );
    assert!(main.contains("<td colspan=\"5\" class=\"off\">"), "{html}");
}

#[test]
fn a_sirius_group_shows_run_then_total() {
    let root = tree("run-sirius-group");
    put(
        &root,
        SINGLE,
        &[("q3", TIMED), ("q6", TIMED), ("q9", TIMED)],
    );
    sirius(
        &root,
        "h100-80gb-hbm3",
        "tpch",
        "q3\t300.0\t300.0,301.0\t180.0\t180.0,181.0\tok\t-\n\
         q6\t100.0\t100.0,101.0\t60.0\t60.0,61.0\twrong\t#7\n\
         q9\t50.0\t50.0,51.0\t30.0\t30.0,31.0\tunverified\t-\n",
    );
    duckdb(&root, "duckdb-cpu128", "tpch", "nebius-cpu128");
    let html = page(root, &[("tpch", "q3"), ("tpch", "q6"), ("tpch", "q9")]);
    assert_eq!(
        row(&html, "q3"),
        [
            "q3", "1.0", "0.5", "1.5", "1.7", "2.00", "180.0", "300.0", "412.0"
        ]
    );
    // The mark follows the run; the total is the wall alone.
    assert_eq!(row(&html, "q6")[6..8], ["60.0 wrong answer #7", "100.0"]);
    assert_eq!(row(&html, "q9")[6..8], ["30.0 unverified", "50.0"]);
    let main = main_part(&html);
    assert!(
        main.contains("<th colspan=\"2\">sirius · h100-80gb-hbm3</th>"),
        "{html}"
    );
    assert!(
        main.contains("<th>run (ms)</th><th>total (ms)</th><th>warm (ms)</th>"),
        "{html}"
    );
    let href = section_href("benchmark-results/h100-80gb-hbm3/tpch.sf40/sirius.tsv", 3);
    assert!(
        main.contains(&format!(
            "<td class=\"n\"><a href=\"{href}\">180.0</a></td><td class=\"n\"><a \
             href=\"{href}\">300.0</a></td>"
        )),
        "{html}"
    );
}

#[test]
fn a_failed_sirius_row_spans_both_cells() {
    let root = tree("run-sirius-failed");
    put(
        &root,
        SINGLE,
        &[("q3", TIMED), ("q6", TIMED), ("q9", TIMED)],
    );
    sirius(
        &root,
        "h100-80gb-hbm3",
        "tpch",
        "q3\t-\t-\t-\t-\tfailed\tboom\nq6\t-\t-\t-\t-\tskipped\tnot run\n",
    );
    let html = page(root, &[("tpch", "q3"), ("tpch", "q6"), ("tpch", "q9")]);
    assert_eq!(row(&html, "q3")[6..], ["failed: boom"]);
    assert_eq!(row(&html, "q6")[6..], ["skipped: not run"]);
    // No row: a dash under each.
    assert_eq!(row(&html, "q9")[6..], ["—", "—"]);
    let main = main_part(&html);
    assert!(
        main.contains("<td colspan=\"2\" class=\"off\"><a href="),
        "{html}"
    );
    assert!(
        main.contains("<td colspan=\"2\" class=\"skip\"><a href="),
        "{html}"
    );
}

/// q3 and q15 count. q6 is Sirius's `wrong`, q9 Sirius's `failed`, q11 Sirius's `unverified`, and
/// q13 peacockdb's `wrong`. The run ratio is (3.4 + 5.1) / (1.7 + 1.7), h100's tp1_single alone:
/// not another device's, nor another mode's; the total line follows it.
#[test]
fn the_run_ratio_counts_only_rows_both_ran() {
    let root = tree("run-ratio");
    let queries = ["q3", "q6", "q9", "q11", "q13", "q15"];
    let sections: Vec<(&str, &str)> = queries.iter().map(|query| (*query, TIMED)).collect();
    put(&root, SINGLE, &sections);
    let slower = TIMED.replace("run_us=1700", "run_us=9000");
    put(
        &root,
        "h100-80gb-hbm3/tpch.sf40/tp4_sized.benchmark.txt",
        &[("q3", &slower)],
    );
    put(
        &root,
        "a100/tpch.sf40/tp1_single.benchmark.txt",
        &[("q3", &slower), ("q15", &slower)],
    );
    list(&root, "h100-80gb-hbm3", "tpch,40,q13,tp1_single,wrong,#7\n");
    sirius(
        &root,
        "h100-80gb-hbm3",
        "tpch",
        "q3\t9.0\t9.0\t3.4\t3.4\tok\t-\nq6\t9.0\t9.0\t1.0\t1.0\twrong\t-\n\
         q9\t-\t-\t-\t-\tfailed\tboom\nq11\t9.0\t9.0\t1.0\t1.0\tunverified\t-\n\
         q13\t9.0\t9.0\t1.0\t1.0\tok\t-\nq15\t9.0\t9.0\t5.1\t5.1\tok\t-\n",
    );
    let pairs: Vec<(&str, &str)> = queries.iter().map(|query| ("tpch", *query)).collect();
    let html = page(root, &pairs);
    assert!(
        html.contains(
            "<h2 id=\"tpch-sf40\">tpch sf40</h2>\n<p>Σ run Sirius / Σ run peacockdb = 2.50 over 2 \
             of 6 queries</p>\n<p>Σ total Sirius / Σ total DuckDB = — over 0 of 6 \
             queries</p>\n<table>"
        ),
        "{html}"
    );
}

#[test]
fn a_section_without_run_us_shows_a_dash_and_stays_out_of_the_ratio() {
    let root = tree("run-no-run-us");
    let bare = without_run_us();
    put(&root, SINGLE, &[("q3", TIMED), ("q6", &bare)]);
    sirius(
        &root,
        "h100-80gb-hbm3",
        "tpch",
        "q3\t9.0\t9.0\t3.4\t3.4\tok\t-\nq6\t9.0\t9.0\t5.1\t5.1\tok\t-\n",
    );
    let pairs = [("tpch", "q3"), ("tpch", "q6")];
    let csv = render_benchmarks_csv(&root, &order(&pairs));
    let html = page(root, &pairs);
    assert_eq!(row(&html, "q6")[1..6], ["1.0", "0.5", "1.5", "—", "2.00"]);
    assert!(
        html.contains("<p>Σ run Sirius / Σ run peacockdb = 2.00 over 1 of 2 queries</p>"),
        "{html}"
    );
    assert!(
        csv.contains("\ntpch,40,q6,peacockdb,h100-80gb-hbm3,tp1_single,1.0,0.5,1.5,,2.00,ok\n"),
        "{csv}"
    );
}

/// A row timed before the windows were recorded: no run, and a `wrong` row's mark stays on the page.
#[test]
fn a_sirius_row_without_run_times_shows_a_dash_and_stays_out_of_the_ratio() {
    let root = tree("run-sirius-no-run");
    put(
        &root,
        SINGLE,
        &[("q3", TIMED), ("q6", TIMED), ("q9", TIMED)],
    );
    sirius(
        &root,
        "h100-80gb-hbm3",
        "tpch",
        "q3\t300.0\t300.0\t-\t-\tok\t-\nq6\t9.0\t9.0\t3.4\t3.4\tok\t-\n\
         q9\t100.0\t100.0\t-\t-\twrong\t#7\n",
    );
    let pairs = [("tpch", "q3"), ("tpch", "q6"), ("tpch", "q9")];
    let csv = render_benchmarks_csv(&root, &order(&pairs));
    let html = page(root, &pairs);
    assert_eq!(row(&html, "q3")[6..8], ["—", "300.0"]);
    assert_eq!(row(&html, "q9")[6..8], ["— wrong answer #7", "100.0"]);
    assert!(
        html.contains("<p>Σ run Sirius / Σ run peacockdb = 2.00 over 1 of 3 queries</p>"),
        "{html}"
    );
    assert!(
        csv.contains("\ntpch,40,q3,sirius,h100-80gb-hbm3,tp1_single,,,300.0,,,ok\n"),
        "{csv}"
    );
}

#[test]
fn the_csv_has_run_ms_after_total_ms() {
    let root = tree("run-csv");
    put(&root, SINGLE, &[("q3", TIMED)]);
    sirius(
        &root,
        "h100-80gb-hbm3",
        "tpch",
        "q3\t300.0\t300.0\t180.0\t180.0\tok\t-\n",
    );
    duckdb(&root, "duckdb-cpu128", "tpch", "nebius-cpu128");
    let csv = render_benchmarks_csv(&root, &order(&[("tpch", "q3")]));
    let _ = std::fs::remove_dir_all(&root);
    assert_eq!(
        csv,
        format!(
            "{CSV_HEADER}\n\
             tpch,40,q3,peacockdb,h100-80gb-hbm3,tp1_single,1.0,0.5,1.5,1.7,2.00,ok\n\
             tpch,40,q3,sirius,h100-80gb-hbm3,tp1_single,,,300.0,180.0,,ok\n\
             tpch,40,q3,duckdb,duckdb-cpu128,,,,412.0,,,ok\n\
             tpch,40,q6,duckdb,duckdb-cpu128,,,,,,,failed\n"
        )
    );
    assert!(
        CSV_HEADER.ends_with(",total_ms,run_ms,peak_gib,status"),
        "{CSV_HEADER}"
    );
}

#[test]
fn the_modes_tables_do_not_change() {
    let root = tree("run-modes");
    put(&root, SINGLE, &[("q3", TIMED)]);
    put(
        &root,
        "h100-80gb-hbm3/tpch.sf40/tp4_sized.benchmark.txt",
        &[("q3", TIMED)],
    );
    sirius(
        &root,
        "h100-80gb-hbm3",
        "tpch",
        "q3\t300.0\t300.0\t180.0\t180.0\tok\t-\n",
    );
    let html = page(root, &[("tpch", "q3")]);
    assert_eq!(
        modes_row(&html, "h100-80gb-hbm3", "q3"),
        [
            "q3", "1.0", "1.0", "0.5", "0.5", "1.5", "1.5", "2.00", "2.00"
        ]
    );
    let modes = &html[html.find("<details>").expect("a Modes section")..];
    assert!(!modes.contains("run (ms)"), "{html}");
    assert!(!modes.contains("sirius"), "{html}");
}

/// The runner notes a rebuilt plan on a timed row: beside its run, after any mark.
#[test]
fn a_sirius_row_s_note_shows_on_its_run() {
    let root = tree("run-sirius-note");
    put(&root, SINGLE, &[("q3", TIMED), ("q6", TIMED)]);
    sirius(
        &root,
        "h100-80gb-hbm3",
        "tpch",
        "q3\t300.0\t300.0,301.0\t180.0\t180.0,181.0\tok\tplan rebuilt in 1 of 2 runs\n\
         q6\t100.0\t100.0\t-\t-\tunverified\tplan rebuilt in 1 of 1 runs\n",
    );
    let html = page(root, &[("tpch", "q3"), ("tpch", "q6")]);
    assert_eq!(
        row(&html, "q3")[6..8],
        ["180.0 plan rebuilt in 1 of 2 runs", "300.0"]
    );
    assert_eq!(
        row(&html, "q6")[6..8],
        ["— unverified plan rebuilt in 1 of 1 runs", "100.0"]
    );
    assert!(
        html.contains("</a> <span class=\"source\">plan rebuilt in 1 of 2 runs</span></td>"),
        "{html}"
    );
}
