//! The run clock beside the device time and the wall: peacockdb's `run_us` and Sirius's execution
//! window in the main table, the run ratio, each engine's times in their own CSV columns, the
//! headings' titles, and the intro's and Sirius's paragraph saying what each run, device time and
//! wall is. The trees and readers are `tests.rs`'s.

use super::sirius::{SIRIUS_RUN_MEANS, SIRIUS_WALL_MEANS};
use super::tests::{
    CSV_HEADER, TIMED, duckdb, failed, list, main_part, modes_row, order, page, put, row, sirius,
    tree,
};
use super::{
    DEVICE_MEANS, DUCKDB_WARM_MEANS, LOAD_MEANS, LOAD_UNPINNED_MEANS, PEAK_MEANS, RUN_MEANS, escape,
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
fn the_main_table_shows_peacockdb_s_run_after_its_device_time() {
    let root = tree("run-peacockdb");
    put(&root, SINGLE, &[("q3", TIMED), ("q6", &failed("boom"))]);
    let html = page(root, &[("tpch", "q3"), ("tpch", "q6")]);
    assert_eq!(row(&html, "q3"), ["q3", "1.0", "0.5", "1.5", "1.7", "2.00"]);
    let main = main_part(&html);
    assert!(
        main.contains("<th colspan=\"5\">h100-80gb-hbm3 · tp1_single</th>"),
        "{html}"
    );
    let headings = format!(
        "<th title=\"{}\">device (ms)</th><th title=\"{}\">run (ms)</th><th title=\"{}\">peak \
         ≥ (GiB)</th>",
        escape(DEVICE_MEANS),
        escape(RUN_MEANS),
        escape(PEAK_MEANS)
    );
    assert!(main.contains(&headings), "{html}");
    // Linked to the section's header, as the device time is.
    let href = section_href(&format!("benchmark-results/{SINGLE}"), 1);
    assert!(
        main.contains(&format!("<td class=\"n\"><a href=\"{href}\">1.7</a></td>")),
        "{html}"
    );
    assert!(main.contains("<td colspan=\"5\" class=\"off\">"), "{html}");
}

#[test]
fn a_sirius_group_shows_run_then_wall() {
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
    // The mark follows the run; the wall stands alone.
    assert_eq!(row(&html, "q6")[6..8], ["60.0 wrong answer #7", "100.0"]);
    assert_eq!(row(&html, "q9")[6..8], ["30.0 unverified", "50.0"]);
    let main = main_part(&html);
    assert!(
        main.contains("<th colspan=\"2\">sirius · h100-80gb-hbm3</th>"),
        "{html}"
    );
    let headings = format!(
        "<th title=\"{}\">run (ms)</th><th title=\"{}\">wall (ms)</th><th \
         title=\"{}\">warm (ms)</th>",
        escape(SIRIUS_RUN_MEANS),
        escape(SIRIUS_WALL_MEANS),
        escape(DUCKDB_WARM_MEANS)
    );
    assert!(main.contains(&headings), "{html}");
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
/// not another device's, nor another mode's; the wall line follows it.
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
             of 6 queries</p>\n<p>Σ wall Sirius / Σ warm DuckDB = — over 0 of 6 \
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
        csv.contains("\ntpch,40,q6,peacockdb,h100-80gb-hbm3,tp1_single,1.0,0.5,1.5,,,2.00,ok\n"),
        "{csv}"
    );
}

/// A timed row without run times fails the page, naming its line: every run records its windows.
#[test]
#[should_panic(expected = "sirius results line 3")]
fn a_sirius_row_without_run_times_fails_the_page() {
    let root = tree("run-sirius-no-run");
    put(&root, SINGLE, &[("q3", TIMED)]);
    sirius(
        &root,
        "h100-80gb-hbm3",
        "tpch",
        "q3\t300.0\t300.0\t-\t-\tok\t-\n",
    );
    page(root, &[("tpch", "q3")]);
}

#[test]
fn the_csv_puts_device_run_and_wall_in_their_own_columns() {
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
             tpch,40,q3,peacockdb,h100-80gb-hbm3,tp1_single,1.0,0.5,1.5,1.7,,2.00,ok\n\
             tpch,40,q3,sirius,h100-80gb-hbm3,tp1_single,,,,180.0,300.0,,ok\n\
             tpch,40,q3,duckdb,duckdb-cpu128,,,,,,412.0,,ok\n\
             tpch,40,q6,duckdb,duckdb-cpu128,,,,,,,,failed\n"
        )
    );
    assert!(
        CSV_HEADER.ends_with(",exec_ms,device_ms,run_ms,wall_ms,peak_gib,status"),
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
         q6\t100.0\t100.0\t40.0\t40.0\tunverified\tplan rebuilt in 1 of 1 runs\n",
    );
    let html = page(root, &[("tpch", "q3"), ("tpch", "q6")]);
    assert_eq!(
        row(&html, "q3")[6..8],
        ["180.0 plan rebuilt in 1 of 2 runs", "300.0"]
    );
    assert_eq!(
        row(&html, "q6")[6..8],
        ["40.0 unverified plan rebuilt in 1 of 1 runs", "100.0"]
    );
    assert!(
        html.contains("</a> <span class=\"source\">plan rebuilt in 1 of 2 runs</span></td>"),
        "{html}"
    );
}

/// The intro and Sirius's paragraph say what each engine's run, device time and wall are; Sirius's
/// where it read from, what each file was measured with, and that some rows read outside the pin.
#[test]
fn the_sirius_paragraph_names_its_run_wall_and_provenance() {
    let root = tree("sirius-intro");
    put(
        &root,
        "h100-80gb-hbm3/tpch.sf40/tp1_single.benchmark.txt",
        &[("q3", TIMED)],
    );
    sirius(
        &root,
        "h100-80gb-hbm3",
        "tpch",
        "q3\t300.0\t300.0,301.0\t180.0\t180.0,181.0\tok\t-\n",
    );
    duckdb(&root, "duckdb-cpu128", "tpch", "nebius-cpu128");
    let html = page(root, &[("tpch", "q3")]);
    let peacockdb = "PeacockDB's times are one run's, the second-smallest by wall time of its timed \
                     runs after one warm-up. Its load, execute and device times are device-side, \
                     in ms: device is load plus execute. Its run is that run's wall time end to \
                     end: after planning, the copy of its result to the host included.";
    assert!(html.contains(peacockdb), "{html}");
    let at = html.find("<p>Sirius's run is").expect("a Sirius paragraph");
    let paragraph = &html[at..at + html[at..].find("</p>").unwrap()];
    for said in [
        "its execution window after planning, from its log's [window] begin to [window] end",
        "its result materialized on the host included, its Arrow conversion not",
        "Its wall time is taken around the statement and an Arrow fetch of its result, planning \
         included",
        "A row whose plan was rebuilt inside its window says so beside its run.",
        "the second-smallest of the timed runs after one warm-up",
        "pinned in Sirius's host cache. Its config departs from Sirius's defaults in three",
        "reads outside the pin (likely bloom filters) inside the timed runs; served from tmpfs",
        "h100-80gb-hbm3, tpch sf40: Sirius 8d17afe, DuckDB 1.5.6, libcudf 26.08.01, driver 580.173, \
         device NVIDIA H100 80GB HBM3",
    ] {
        assert!(paragraph.contains(said), "{said:?} not in {paragraph}");
    }
    // Beside DuckDB's, before it, as the groups stand in the table.
    assert!(at < html.find("<p>DuckDB's time is").unwrap(), "{html}");
}

/// Each heading of a time, device time or peak says what it measures on hover, in the main table
/// and in a Modes table, and the page marks such a heading.
#[test]
fn every_time_heading_carries_a_title() {
    let root = tree("titles");
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
    duckdb(&root, "duckdb-cpu128", "tpch", "nebius-cpu128");
    let html = page(root, &[("tpch", "q3")]);
    let headings =
        |row: &str| -> Vec<String> { row.split("<th").skip(1).map(|th| th.to_string()).collect() };
    let main = main_part(&html);
    let second = main
        .lines()
        .filter(|line| line.starts_with("<tr><th"))
        .nth(1)
        .expect("the main table's second heading row");
    let times = headings(second);
    // Five for the device, two for Sirius, one for DuckDB.
    assert_eq!(times.len(), 8, "{second}");
    for th in &times {
        assert!(
            th.starts_with(" title=\"") && !th.starts_with(" title=\"\""),
            "{th}"
        );
    }
    let modes = &html[html.find("<h3>h100-80gb-hbm3</h3>").expect("a Modes table")..];
    let mut rows = modes.lines().filter(|line| line.starts_with("<tr><th"));
    let first = rows.next().expect("the Modes table's first heading row");
    // The query, the four measures, then DuckDB's tree, whose measure is in the second row.
    let measures = headings(first);
    assert_eq!(measures.len(), 6, "{first}");
    let second = rows.next().expect("the Modes table's second heading row");
    let wall = headings(second).pop().expect("DuckDB's heading");
    for th in measures[1..5].iter().chain([&wall]) {
        assert!(
            th.contains(" title=\"") && !th.contains(" title=\"\""),
            "{th}"
        );
    }
    assert!(
        html.contains("th[title] { text-decoration: underline dotted; cursor: help; }"),
        "{html}"
    );
}

/// A Modes table whose every load is unpinned titles its load heading so, the cells unmarked; a
/// pinned device's keeps the pinned title.
#[test]
fn an_all_unpinned_modes_table_titles_its_load_unpinned() {
    let root = tree("titles-unpinned");
    put(&root, SINGLE, &[("q3", TIMED)]);
    let unpinned = TIMED.replace("pinned_cache=on", "pinned_cache=off");
    put(
        &root,
        "a100/tpch.sf40/tp1_single.benchmark.txt",
        &[("q3", &unpinned)],
    );
    let html = page(root, &[("tpch", "q3")]);
    let load = |means: &str| format!(" title=\"{}\">load (ms)</th>", escape(means));
    let a100 = &html[html.find("<h3>a100</h3>").expect("a100's Modes table")..];
    let a100 = &a100[..a100.find("</table>").unwrap()];
    assert!(a100.contains(&load(LOAD_UNPINNED_MEANS)), "{html}");
    let h100 = &html[html
        .find("<h3>h100-80gb-hbm3</h3>")
        .expect("h100's Modes table")..];
    assert!(h100.contains(&load(LOAD_MEANS)), "{html}");
}
