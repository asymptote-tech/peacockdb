//! The benchmark report page over temp trees built from literal sections: what a cell reads
//! for each kind of section, where the device's list overrides it, and the page's layout: the
//! main table at tp1_single, then the collapsed Modes tables, one per device.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::test_support::{
    DEVICE_CASES_HEADER, FAILED, ReportCell, ReportTable, SKIPPED, render_benchmark_report,
    render_benchmarks_csv, section_href,
};

/// A section written now: two loaders (1000 µs) and an unload (500 µs), read from the pinned
/// host cache.
const TIMED: &str = "\
GpuUnload
  time_us=[[500]] total_us=500 batch_rows=[[10]] batch_bytes=[[80]]
  GpuLoadParquet: table=a, lanes=1, batches=multiple
    time_us=[[600]] total_us=600 batch_rows=[[5]] batch_bytes=[[40]]
  GpuLoadParquet: table=b, lanes=1, batches=multiple
    time_us=[[400]] total_us=400 batch_rows=[[5]] batch_bytes=[[40]]
--- run ---
run_us=1700
device_us=1500
load_us=1000
execute_us=500
peak_bytes=2147483648
runs=[1700,1700,1700,1700,1700,1700,1700,1700,1700,1700]
build=release
allocator=rmm-pool
pinned_cache=on cap=147.4GiB resident=1.0GiB files=2 fills=2 fill_us=900 fallbacks=0 evictions=0 reloads=0 source=default
prefill_files=2
prefill_bytes=1073741824
prefill_us=900
device=NVIDIA H100 80GB HBM3
driver=580.95.05
cuda=12.9
cudf=25.02.02
host=nebius-h100
";

/// A section without batches, load or peak, as the `h200/` tree's are.
const UNBATCHED: &str = "\
GpuUnload
  time_us=[[149]] total_us=149
  GpuNestedLoopJoin: join_type=Inner, lanes=1, batches=multiple
    time_us=[[0,557,0]] total_us=557
    GpuLoadParquet: table=region, lanes=1, batches=multiple
      time_us=[[766]] total_us=766
    GpuLoadParquet: table=nation, lanes=1, batches=multiple
      time_us=[[849]] total_us=849
--- run ---
run_us=2651
device_us=2321
runs=[2844,2750,2725,2701,2675,2702,2666,2651,2666,2636]
build=release
allocator=rmm-pool
";

/// A temp `benchmark-results/` of its own per test. A name is taken once per process: two tests
/// sharing one would delete each other's tree when the tier runs in parallel.
fn tree(name: &str) -> PathBuf {
    static TAKEN: Mutex<BTreeSet<String>> = Mutex::new(BTreeSet::new());
    assert!(
        TAKEN.lock().unwrap().insert(name.to_string()),
        "tree({name:?}) is already another test's"
    );
    let root = std::env::temp_dir().join(format!(
        "peacock-benchmark-report-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    root
}

/// `sections` as one file at `rel` under `root`, each under its `== ` header.
fn put(root: &Path, rel: &str, sections: &[(&str, &str)]) {
    let path = root.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let text: String = sections
        .iter()
        .map(|(query, body)| format!("== {query}\n{body}"))
        .collect();
    std::fs::write(path, text).unwrap();
}

fn failed(reason: &str) -> String {
    format!("{FAILED}{reason}\ndevice=NVIDIA H100 80GB HBM3\ndriver=1\ncuda=1\ncudf=1\nhost=h\n")
}

fn list(root: &Path, device: &str, rows: &str) {
    std::fs::write(
        root.join(device).join("cases.csv"),
        format!("{DEVICE_CASES_HEADER}\n{rows}"),
    )
    .unwrap();
}

fn order(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
    pairs
        .iter()
        .map(|(dataset, query)| (dataset.to_string(), query.to_string()))
        .collect()
}

/// The page over `root`, the tree removed.
fn page(root: PathBuf, pairs: &[(&str, &str)]) -> String {
    let html = render_benchmark_report(&root, &order(pairs), &[]);
    let _ = std::fs::remove_dir_all(&root);
    html
}

/// Each row's `<td>` texts, tags stripped, in page order.
fn rows(html: &str) -> Vec<Vec<String>> {
    html.lines()
        .filter(|line| line.starts_with("<tr><td"))
        .map(|line| {
            line.split("<td")
                .skip(1)
                .map(|cell| strip_tags(&format!("<td{cell}")))
                .collect()
        })
        .collect()
}

fn strip_tags(html: &str) -> String {
    let mut text = String::new();
    let mut inside = false;
    for c in html.chars() {
        match c {
            '<' => inside = true,
            '>' => inside = false,
            _ if !inside => text.push(c),
            _ => {}
        }
    }
    text
}

/// The rows of the first table after `marker`.
fn table_rows(html: &str, marker: &str) -> Vec<Vec<String>> {
    let at = html
        .find(marker)
        .unwrap_or_else(|| panic!("no {marker} in:\n{html}"));
    let rest = &html[at..];
    rows(&rest[..rest.find("</table>").expect("a table after the marker")])
}

/// tpch sf40's main table, its rows.
fn main_rows(html: &str) -> Vec<Vec<String>> {
    table_rows(html, "<h2 id=\"tpch-sf40\">")
}

/// `device`'s Modes table, its rows.
fn modes_rows(html: &str, device: &str) -> Vec<Vec<String>> {
    table_rows(html, &format!("<h3>{device}</h3>"))
}

fn find(rows: Vec<Vec<String>>, query: &str, html: &str) -> Vec<String> {
    rows.into_iter()
        .find(|cells| cells[0] == query)
        .unwrap_or_else(|| panic!("no row {query} in:\n{html}"))
}

/// The main table's row for `query`.
fn row(html: &str, query: &str) -> Vec<String> {
    find(main_rows(html), query, html)
}

/// `device`'s Modes row for `query`.
fn modes_row(html: &str, device: &str, query: &str) -> Vec<String> {
    find(modes_rows(html, device), query, html)
}

/// tpch sf40's main table, from its heading up to its Modes section.
fn main_part(html: &str) -> &str {
    let from = html
        .find("<h2 id=\"tpch-sf40\">")
        .expect("a tpch sf40 table");
    &html[from..html.find("<details>").expect("a Modes section")]
}

#[test]
fn a_timed_section_gives_its_load_execute_total_and_peak() {
    let root = tree("timed");
    put(
        &root,
        "h100-80gb-hbm3/tpch.sf40/tp1_single.benchmark.txt",
        &[("q1", UNBATCHED), ("q3", TIMED)],
    );
    let html = page(root, &[("tpch", "q1"), ("tpch", "q3")]);
    assert_eq!(row(&html, "q3"), ["q3", "1.0", "0.5", "1.5", "2.00"]);
    // The total links to the section's header: line 1 is q1's, then UNBATCHED's 14 lines.
    let href = section_href(
        "benchmark-results/h100-80gb-hbm3/tpch.sf40/tp1_single.benchmark.txt",
        16,
    );
    assert!(
        html.contains(&format!("<a href=\"{href}\">1.5</a>")),
        "{html}"
    );
}

#[test]
fn an_h200_section_derives_load_and_shows_no_peak() {
    let root = tree("h200-shape");
    put(
        &root,
        "h200/tpch.sf40/tp1_single.benchmark.txt",
        &[("q1", UNBATCHED)],
    );
    let html = page(root, &[("tpch", "q1")]);
    // load (766 + 849) µs, execute 2321 − 1615. No cell has a peak, so the table has no peak
    // columns; every load predates the pinned cache, so the heading says it once.
    assert_eq!(modes_row(&html, "h200", "q1"), ["q1", "1.6", "0.7", "2.3"]);
    assert!(
        html.contains("<h3>h200</h3>\n<p class=\"source\">Every load in this table is unpinned"),
        "{html}"
    );
    assert!(
        !html.contains("peak ≥ (GiB)</th><th class=\"g\">tp1"),
        "{html}"
    );
}

#[test]
fn a_load_without_the_pinned_cache_is_marked_unpinned() {
    let root = tree("unpinned");
    let off = TIMED
        .lines()
        .map(|line| match line.starts_with("pinned_cache=") {
            true => "pinned_cache=off source=env\n".to_string(),
            false => format!("{line}\n"),
        })
        .collect::<String>();
    put(
        &root,
        "h100-80gb-hbm3/tpch.sf40/tp1_single.benchmark.txt",
        &[("q1", UNBATCHED), ("q3", &off), ("q6", TIMED)],
    );
    let html = page(root, &[("tpch", "q1"), ("tpch", "q3"), ("tpch", "q6")]);
    // No `pinned_cache` line: a section from before the cache.
    assert_eq!(row(&html, "q1")[1], "1.6 unpinned");
    assert_eq!(row(&html, "q3")[1], "1.0 unpinned");
    assert_eq!(row(&html, "q6")[1], "1.0");
    // A Modes table that mixes the two marks each unpinned load.
    let device = "h100-80gb-hbm3";
    assert_eq!(modes_row(&html, device, "q1")[1], "1.6 unpinned");
    assert_eq!(modes_row(&html, device, "q6")[1], "1.0");
    assert!(!html.contains("Every load in this table"), "{html}");
    assert!(
        html.contains("A load marked unpinned was read without the pinned host cache"),
        "{html}"
    );
}

#[test]
fn a_failed_case_shows_its_reason_and_the_list_wins() {
    let root = tree("failed");
    let rel = "h100-80gb-hbm3/tpch.sf40/tp1_single.benchmark.txt";
    put(&root, rel, &[("q3", &failed("timeout: x"))]);
    let html = render_benchmark_report(&root, &order(&[("tpch", "q3")]), &[]);
    assert_eq!(row(&html, "q3"), ["q3", "failed: timeout: x"]);
    assert!(html.contains("<td colspan=\"4\" class=\"off\">"), "{html}");
    // In a Modes table the reason stands in the total, the other measures `—`.
    assert_eq!(
        modes_row(&html, "h100-80gb-hbm3", "q3"),
        ["q3", "—", "—", "failed: timeout: x"]
    );

    list(&root, "h100-80gb-hbm3", "tpch,40,q3,,skipped,oom: y\n");
    let html = page(root, &[("tpch", "q3")]);
    assert_eq!(row(&html, "q3"), ["q3", "skipped: oom: y"]);
    assert!(html.contains("<td colspan=\"4\" class=\"skip\">"), "{html}");
}

#[test]
fn a_marker_shows_its_reason() {
    let root = tree("marker");
    let marker = format!("{SKIPPED}not enabled at this mode\n");
    put(
        &root,
        "h200/tpch.sf40/tp1_single.benchmark.txt",
        &[("q2", &marker)],
    );
    let html = page(root, &[("tpch", "q2")]);
    assert_eq!(
        modes_row(&html, "h200", "q2"),
        ["q2", "—", "—", "skipped: not enabled at this mode"]
    );
}

#[test]
fn a_wrong_row_marks_its_cells() {
    let root = tree("wrong");
    put(
        &root,
        "h100-80gb-hbm3/tpch.sf40/tp1_single.benchmark.txt",
        &[("q3", TIMED)],
    );
    list(&root, "h100-80gb-hbm3", "tpch,40,q3,tp1_single,wrong,#7\n");
    let html = page(root, &[("tpch", "q3")]);
    assert_eq!(row(&html, "q3")[3], "1.5 wrong answer #7", "{html}");
}

/// The main table is tp1_single on every device but h200; h200 has a Modes table only, without
/// the queries it has nothing for.
#[test]
fn h200_sits_under_modes_only_and_an_empty_query_is_left_out() {
    let root = tree("side-by-side");
    put(
        &root,
        "h100-80gb-hbm3/tpch.sf40/tp1_single.benchmark.txt",
        &[("q3", TIMED), ("q6", TIMED)],
    );
    put(
        &root,
        "h200/tpch.sf40/tp1_single.benchmark.txt",
        &[("q6", UNBATCHED)],
    );
    let html = page(root, &[("tpch", "q3"), ("tpch", "q6")]);
    let main = main_part(&html);
    assert!(main.contains(">h100-80gb-hbm3 · tp1_single</th>"), "{html}");
    assert!(!main.contains("h200"), "{html}");
    assert_eq!(row(&html, "q6"), ["q6", "1.0", "0.5", "1.5", "2.00"]);
    let h100 = html
        .find("<h3>h100-80gb-hbm3</h3>")
        .expect("an h100 Modes table");
    let h200 = html.find("<h3>h200</h3>").expect("an h200 Modes table");
    assert!(h100 < h200, "devices by name");
    assert_eq!(modes_rows(&html, "h200"), [["q6", "1.6", "0.7", "2.3"]]);
    assert!(
        html.contains("<details><summary>Modes, tpch sf40</summary>"),
        "collapsed: {html}"
    );
}

#[test]
fn a_directory_without_benchmark_files_is_no_column() {
    let root = tree("no-column");
    put(
        &root,
        "h200/tpch.sf40/tp1_single.benchmark.txt",
        &[("q1", UNBATCHED)],
    );
    std::fs::create_dir_all(root.join("notes/tpch.sf40")).unwrap();
    std::fs::write(root.join("notes/tpch.sf40/notes.txt"), "q1\t1\n").unwrap();
    let html = page(root, &[("tpch", "q1")]);
    assert!(!html.contains("notes"), "{html}");
    assert!(!html.contains("<h3>notes</h3>"), "{html}");
    assert_eq!(modes_row(&html, "h200", "q1").len(), 4);
}

/// DuckDB's results in the format `duckdb_minimal.sh --results` writes: q3 timed, q6 failed.
fn duckdb(root: &Path, tree: &str, dataset: &str, host: &str) {
    let dir = root.join(tree).join(format!("{dataset}.sf40"));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("duckdb.tsv"),
        format!(
            "# duckdb=1.5.4 threads=128 host={host} cpu=x cores=128 memory_gib=1007.7 db=/d \
             resident_pct=100.0 floor_ms=29\n\
             query\tms_2nd_min\truns_ms\tworkset_bytes\tdisk_read_bytes\twarm\tstatus\n\
             q3\t412\t431,412\t100\t0\tyes\tok\n\
             q6\t-\t-\t100\t-\t-\tfailed\n"
        ),
    )
    .unwrap();
}

#[test]
fn a_duckdb_tree_is_a_column_of_its_own() {
    let root = tree("duckdb-column");
    put(
        &root,
        "h100-80gb-hbm3/tpch.sf40/tp1_single.benchmark.txt",
        &[("q3", TIMED), ("q6", TIMED), ("q9", TIMED)],
    );
    put(
        &root,
        "h100-80gb-hbm3/tpch.sf40/tp4_sized.benchmark.txt",
        &[("q3", TIMED)],
    );
    duckdb(&root, "duckdb-cpu128", "tpch", "nebius-cpu128");
    let html = page(root, &[("tpch", "q3"), ("tpch", "q6"), ("tpch", "q9")]);
    assert!(
        !html.contains("<th colspan=\"4\">duckdb-cpu128</th>"),
        "not a device: {html}"
    );
    let h100 = html.find(">h100-80gb-hbm3 · tp1_single</th>").unwrap();
    let duckdb = html.find("<th>duckdb-cpu128</th>").expect("a column");
    assert!(h100 < duckdb, "after the devices: {html}");
    assert!(html.contains("<th>warm (ms)</th>"), "{html}");
    assert_eq!(
        row(&html, "q3"),
        ["q3", "1.0", "0.5", "1.5", "2.00", "412.0"]
    );
    assert_eq!(row(&html, "q6")[5], "failed");
    assert_eq!(row(&html, "q9")[5], "—");
    // Modes side by side under each measure, DuckDB last.
    assert!(
        html.contains(
            "<th colspan=\"2\" class=\"g\">load (ms)</th><th colspan=\"2\" \
             class=\"g\">execute (ms)</th>"
        ),
        "{html}"
    );
    assert!(
        html.contains("<th class=\"g\">tp1<br>single</th><th>tp4<br>sized</th>"),
        "{html}"
    );
    assert_eq!(
        modes_row(&html, "h100-80gb-hbm3", "q3"),
        [
            "q3", "1.0", "1.0", "0.5", "0.5", "1.5", "1.5", "2.00", "2.00", "412.0"
        ]
    );
    assert_eq!(
        modes_row(&html, "h100-80gb-hbm3", "q6"),
        [
            "q6", "1.0", "—", "0.5", "—", "1.5", "—", "2.00", "—", "failed"
        ]
    );
    assert!(html.contains("<td class=\"off\">failed</td>"), "{html}");
    assert!(html.contains("<td class=\"na\">—</td>"), "{html}");
    let href = section_href("benchmark-results/duckdb-cpu128/tpch.sf40/duckdb.tsv", 3);
    assert!(
        html.contains(&format!(
            "<td class=\"n\"><a href=\"{href}\">412.0</a></td>"
        )),
        "{html}"
    );
}

/// A time taken with the database not resident in the page cache is not the warm protocol's: its
/// cell says what the verdict was.
#[test]
fn a_duckdb_time_not_warm_says_so() {
    let root = tree("duckdb-not-warm");
    put(
        &root,
        "h100-80gb-hbm3/tpch.sf40/tp1_single.benchmark.txt",
        &[("q3", TIMED), ("q6", TIMED)],
    );
    duckdb(&root, "duckdb-cpu128", "tpch", "nebius-cpu128");
    let file = root.join("duckdb-cpu128/tpch.sf40/duckdb.tsv");
    let text = std::fs::read_to_string(&file).unwrap();
    std::fs::write(&file, text.replace("0\tyes\tok", "0\tpartial\tok")).unwrap();
    let html = page(root, &[("tpch", "q3"), ("tpch", "q6")]);
    assert_eq!(row(&html, "q3")[5], "412.0 not warm: partial");
    assert_eq!(row(&html, "q6")[5], "failed");
}

#[test]
fn the_duckdb_column_names_its_provenance() {
    let root = tree("duckdb-provenance");
    put(
        &root,
        "h100-80gb-hbm3/tpch.sf40/tp1_single.benchmark.txt",
        &[("q3", TIMED)],
    );
    duckdb(&root, "duckdb-cpu128", "tpch", "nebius-cpu128");
    let html = page(root, &[("tpch", "q3")]);
    assert!(
        html.contains(
            "duckdb-cpu128, tpch sf40: DuckDB 1.5.4, 128 threads, host nebius-cpu128, floor 29 ms"
        ),
        "{html}"
    );
    // What one time holds: a whole CLI process, so its floor is named for a reader to subtract.
    assert!(
        html.contains(
            "one DuckDB CLI process's wall time: the process started, the database opened"
        ),
        "{html}"
    );
}

#[test]
fn rows_follow_the_main_list_and_modes_their_order() {
    let root = tree("order");
    put(
        &root,
        "h100-80gb-hbm3/tpch.sf40/tp4_sized.benchmark.txt",
        &[("q1", TIMED), ("q7", TIMED)],
    );
    put(
        &root,
        "h100-80gb-hbm3/tpch.sf40/tp1_single.benchmark.txt",
        &[
            ("aggregate-groupby", TIMED),
            ("q1", TIMED),
            ("q6", TIMED),
            ("unlisted", TIMED),
        ],
    );
    let html = page(
        root,
        &[
            ("tpch", "q6"),
            ("tpch", "aggregate_groupby"),
            ("tpch", "q1"),
            ("tpch", "q7"),
        ],
    );
    let queries = |rows: Vec<Vec<String>>| -> Vec<String> {
        rows.into_iter().map(|cells| cells[0].clone()).collect()
    };
    // q7 has no tp1_single section: it is in the Modes table only.
    assert_eq!(
        queries(main_rows(&html)),
        ["q6", "aggregate-groupby", "q1", "unlisted"]
    );
    assert_eq!(
        queries(modes_rows(&html, "h100-80gb-hbm3")),
        ["q6", "aggregate-groupby", "q1", "q7", "unlisted"]
    );
    let single = html.find("tp1<br>single").unwrap();
    let sized = html.find("tp4<br>sized").unwrap();
    assert!(single < sized, "modes in MODES order: {html}");
    assert!(html.contains("<h2 id=\"tpch-sf40\">"), "{html}");
    assert!(html.contains("tpch sf40</a> — 5 queries"), "{html}");
}

#[test]
fn text_is_escaped() {
    let root = tree("escaped");
    put(
        &root,
        "h100-80gb-hbm3/tpch.sf40/tp1_single.benchmark.txt",
        &[("q3", &failed("a < b & c"))],
    );
    let html = page(root, &[("tpch", "q3")]);
    assert!(html.contains("failed: a &lt; b &amp; c"), "{html}");
    assert!(!html.contains("a < b"), "{html}");
}

#[test]
fn the_page_is_the_same_twice() {
    let root = tree("twice");
    put(
        &root,
        "h100-80gb-hbm3/tpch.sf40/tp1_single.benchmark.txt",
        &[("q3", TIMED)],
    );
    put(
        &root,
        "h200/tpch.sf40/tp1_single.benchmark.txt",
        &[("q3", UNBATCHED)],
    );
    let pairs = order(&[("tpch", "q3")]);
    let first = render_benchmark_report(&root, &pairs, &[]);
    let second = render_benchmark_report(&root, &pairs, &[]);
    let _ = std::fs::remove_dir_all(&root);
    assert_eq!(first, second);
    let temp = std::env::temp_dir();
    assert!(!first.contains(temp.to_str().unwrap()), "{first}");
    assert!(first.ends_with("</html>\n"));
    assert!(!first.contains('\r'));
}

#[test]
fn an_extra_table_follows_the_datasets() {
    let root = tree("extra");
    put(
        &root,
        "h200/tpch.sf40/tp1_single.benchmark.txt",
        &[("q1", UNBATCHED)],
    );
    let extra = ReportTable {
        id: "fused".to_string(),
        heading: "Fused batches".to_string(),
        columns: vec!["batch".to_string(), "total (ms)".to_string()],
        rows: vec![vec![
            ReportCell {
                text: "q1, q6\nq7 & q8".to_string(),
                href: None,
                class: None,
            },
            ReportCell {
                text: "2.0".to_string(),
                href: Some("https://example.com/x".to_string()),
                class: Some("off"),
            },
        ]],
    };
    let html = render_benchmark_report(&root, &order(&[("tpch", "q1")]), &[extra]);
    let _ = std::fs::remove_dir_all(&root);
    let dataset = html.find("<h2 id=\"tpch-sf40\">").unwrap();
    let fused = html
        .find("<h2 id=\"fused\">Fused batches</h2>")
        .expect("the extra table");
    assert!(dataset < fused);
    assert!(html.contains("<th>batch</th><th>total (ms)</th>"), "{html}");
    assert!(html.contains("<td>q1, q6<br>q7 &amp; q8</td>"), "{html}");
    assert!(
        html.contains("<td class=\"off\"><a href=\"https://example.com/x\">2.0</a></td>"),
        "{html}"
    );
}

#[test]
fn section_href_names_the_line() {
    assert_eq!(
        section_href(
            "benchmark-results/h200/tpch.sf40/tp1_single.benchmark.txt",
            29
        ),
        "https://github.com/asymptote-tech/peacockdb/blob/master/testdata/benchmark-results/\
         h200/tpch.sf40/tp1_single.benchmark.txt#L29"
    );
}

/// One query twice in one file is refused naming both lines, never shown as whichever is last.
#[test]
fn a_query_with_two_sections_in_one_file_is_refused() {
    let root = tree("two-sections");
    let rel = "h100-80gb-hbm3/tpch.sf40/tp1_single.benchmark.txt";
    put(&root, rel, &[("q3", TIMED), ("q3", TIMED)]);
    let render = std::panic::catch_unwind(|| render_benchmark_report(&root, &[], &[]));
    let _ = std::fs::remove_dir_all(&root);
    let panic = render.expect_err("a file holding q3 twice rendered");
    let message = panic
        .downcast_ref::<String>()
        .expect("a formatted panic message");
    let second = 2 + TIMED.lines().count();
    assert!(
        message.contains(&format!("q3 has two sections, at lines 1 and {second}")),
        "{message}"
    );
}

/// Two tests on one temp tree race when the tier runs in parallel, so a name is taken once.
#[test]
fn a_tree_name_taken_twice_panics() {
    let root = tree("taken-twice");
    let again = std::panic::catch_unwind(|| tree("taken-twice"));
    let _ = std::fs::remove_dir_all(&root);
    assert!(again.is_err(), "tree(\"taken-twice\") was handed out twice");
}

/// A `sirius.tsv` in `device`'s tree for `dataset` at sf40, `rows` after its column line.
fn sirius(root: &Path, device: &str, dataset: &str, rows: &str) {
    let dir = root.join(device).join(format!("{dataset}.sf40"));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("sirius.tsv"),
        format!(
            "# sirius=8d17afe duckdb=1.5.6 libcudf=26.08.01 cuda=13.0 driver=580.173 \
             device=NVIDIA_H100_80GB_HBM3 host=nebius-h100\n\
             query\tms_2nd_min\truns_ms\tstatus\tnote\n{rows}"
        ),
    )
    .unwrap();
}

#[test]
fn a_sirius_file_adds_a_group_before_duckdb() {
    let root = tree("sirius-group");
    put(
        &root,
        "h100-80gb-hbm3/tpch.sf40/tp1_single.benchmark.txt",
        &[("q3", TIMED)],
    );
    sirius(
        &root,
        "h100-80gb-hbm3",
        "tpch",
        "q3\t300.0\t300.0,301.0\tok\t-\n",
    );
    duckdb(&root, "duckdb-cpu128", "tpch", "nebius-cpu128");
    let html = page(root, &[("tpch", "q3")]);
    assert_eq!(
        row(&html, "q3"),
        [
            "q3", "1.0", "0.5", "1.5", "2.00", "—", "—", "300.0", "412.0"
        ]
    );
    let main = main_part(&html);
    let h100 = main.find(">h100-80gb-hbm3 · tp1_single</th>").unwrap();
    let group = main
        .find("<th colspan=\"3\">sirius · h100-80gb-hbm3</th>")
        .expect("a Sirius group");
    let duckdb = main.find("<th>duckdb-cpu128</th>").unwrap();
    assert!(h100 < group && group < duckdb, "{html}");
    assert!(
        main.contains(
            "≥ (GiB)</th><th>load (ms)</th><th>execute (ms)</th><th>total (ms)</th><th>warm"
        ),
        "{html}"
    );
    assert!(
        main.contains("<td class=\"na\">—</td><td class=\"na\">—</td>"),
        "{html}"
    );
    let href = section_href("benchmark-results/h100-80gb-hbm3/tpch.sf40/sirius.tsv", 3);
    assert!(
        main.contains(&format!(
            "<td class=\"n\"><a href=\"{href}\">300.0</a></td>"
        )),
        "{html}"
    );
    // The Modes tables do not change.
    assert_eq!(
        modes_row(&html, "h100-80gb-hbm3", "q3"),
        ["q3", "1.0", "0.5", "1.5", "2.00", "412.0"]
    );
}

#[test]
fn a_failed_sirius_row_shows_its_reason() {
    let root = tree("sirius-failed");
    put(
        &root,
        "h100-80gb-hbm3/tpch.sf40/tp1_single.benchmark.txt",
        &[("q3", TIMED), ("q6", TIMED)],
    );
    sirius(
        &root,
        "h100-80gb-hbm3",
        "tpch",
        "q3\t-\t-\tfailed\tNotImplemented: INTERSECT\nq6\t-\t-\tskipped\tnot run\n",
    );
    let html = page(root, &[("tpch", "q3"), ("tpch", "q6")]);
    assert_eq!(row(&html, "q3")[7], "failed: NotImplemented: INTERSECT");
    assert_eq!(row(&html, "q6")[7], "skipped: not run");
    let href = section_href("benchmark-results/h100-80gb-hbm3/tpch.sf40/sirius.tsv", 3);
    assert!(
        html.contains(&format!(
            "<td class=\"off\"><a href=\"{href}\">failed: NotImplemented: INTERSECT</a></td>"
        )),
        "{html}"
    );
    assert!(html.contains("<td class=\"skip\"><a href="), "{html}");
}

/// A `wrong` Sirius row's note follows `wrong answer` as a peacockdb row's does; without one the
/// mark stands alone.
#[test]
fn a_wrong_sirius_row_shows_its_note() {
    let root = tree("sirius-wrong-note");
    put(
        &root,
        "h100-80gb-hbm3/tpch.sf40/tp1_single.benchmark.txt",
        &[("q3", TIMED), ("q6", TIMED)],
    );
    sirius(
        &root,
        "h100-80gb-hbm3",
        "tpch",
        "q3\t300.0\t300.0\twrong\t#306 <ties>\nq6\t100.0\t100.0\twrong\t-\n",
    );
    let html = page(root, &[("tpch", "q3"), ("tpch", "q6")]);
    // `row` strips the tags and leaves the escapes.
    assert_eq!(row(&html, "q3")[7], "300.0 wrong answer #306 &lt;ties&gt;");
    assert!(
        html.contains("</a> <span class=\"missing\">wrong answer #306 &lt;ties&gt;</span></td>"),
        "{html}"
    );
    assert!(
        html.contains("</a> <span class=\"missing\">wrong answer</span></td>"),
        "{html}"
    );
}

/// Only q3 is `ok` on both sides: q6 is Sirius's `wrong`, q9 Sirius's `failed`, and q11 has no
/// DuckDB row. The ratio is q3's alone, over the main table's four rows.
#[test]
fn the_ratio_counts_rows_both_ok() {
    let root = tree("sirius-ratio");
    put(
        &root,
        "h100-80gb-hbm3/tpch.sf40/tp1_single.benchmark.txt",
        &[("q3", TIMED), ("q6", TIMED), ("q9", TIMED), ("q11", TIMED)],
    );
    sirius(
        &root,
        "h100-80gb-hbm3",
        "tpch",
        "q3\t300.0\t300.0\tok\t-\nq6\t100.0\t100.0\twrong\t-\nq9\t-\t-\tfailed\tboom\n\
         q11\t50.0\t50.0\tunverified\t-\n",
    );
    let dir = root.join("duckdb-cpu128/tpch.sf40");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("duckdb.tsv"),
        "# duckdb=1.5.4 threads=128 host=h\n\
         query\tms_2nd_min\truns_ms\tworkset_bytes\tdisk_read_bytes\twarm\tstatus\n\
         q3\t400\t400\t1\t0\tyes\tok\n\
         q6\t200\t200\t1\t0\tyes\tok\n\
         q9\t70\t70\t1\t0\tyes\tok\n",
    )
    .unwrap();
    let html = page(
        root,
        &[
            ("tpch", "q3"),
            ("tpch", "q6"),
            ("tpch", "q9"),
            ("tpch", "q11"),
        ],
    );
    assert!(
        html.contains(
            "<h2 id=\"tpch-sf40\">tpch sf40</h2>\n<p>Σ total Sirius / Σ total DuckDB = 0.75 over \
             1 of 4 queries</p>\n<table>"
        ),
        "{html}"
    );
    assert_eq!(row(&html, "q6")[7], "100.0 wrong answer");
    assert_eq!(row(&html, "q11")[7], "50.0 unverified");
    assert!(
        html.contains(
            "<span class=\"source\" title=\"no DuckDB answer to check it against\">unverified</span>"
        ),
        "{html}"
    );
}

/// tpcds measured on Sirius, tpch not yet: tpch's Sirius cells are `—` and it has no ratio line.
#[test]
fn no_sirius_file_no_line_and_dashes() {
    let root = tree("sirius-one-dataset");
    put(
        &root,
        "h100-80gb-hbm3/tpch.sf40/tp1_single.benchmark.txt",
        &[("q3", TIMED)],
    );
    put(
        &root,
        "h100-80gb-hbm3/tpcds.sf40/tp1_single.benchmark.txt",
        &[("q1", TIMED)],
    );
    sirius(&root, "h100-80gb-hbm3", "tpcds", "q1\t10.0\t10.0\tok\t-\n");
    duckdb(&root, "duckdb-cpu128", "tpch", "nebius-cpu128");
    let html = page(root, &[("tpch", "q3"), ("tpcds", "q1")]);
    assert_eq!(
        row(&html, "q3"),
        ["q3", "1.0", "0.5", "1.5", "2.00", "—", "—", "—", "412.0"]
    );
    assert!(!main_part(&html).contains("Σ total Sirius"), "{html}");
    let tpcds = &html[html.find("<h2 id=\"tpcds-sf40\">").unwrap()..];
    assert!(
        tpcds.contains("<p>Σ total Sirius / Σ total DuckDB = — over 0 of 1 queries</p>"),
        "{html}"
    );
}

/// No `sirius.tsv` in any tree: the page has no Sirius group, as before Sirius was measured.
#[test]
fn no_sirius_file_anywhere_no_group() {
    let root = tree("sirius-none");
    put(
        &root,
        "h100-80gb-hbm3/tpch.sf40/tp1_single.benchmark.txt",
        &[("q3", TIMED)],
    );
    let html = page(root, &[("tpch", "q3")]);
    assert!(!html.contains("sirius"), "{html}");
    assert!(!html.contains("Sirius"), "{html}");
    assert_eq!(row(&html, "q3"), ["q3", "1.0", "0.5", "1.5", "2.00"]);
}

/// The intro says device-side times are peacockdb's; Sirius's paragraph says what its total is, where
/// it read from, what each file was measured with, and that some rows read outside the pin.
#[test]
fn the_sirius_paragraph_names_its_total_and_provenance() {
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
        "q3\t300.0\t300.0,301.0\tok\t-\n",
    );
    duckdb(&root, "duckdb-cpu128", "tpch", "nebius-cpu128");
    let html = page(root, &[("tpch", "q3")]);
    assert!(
        html.contains("PeacockDB's times are device-side, in ms: the total is load plus execute."),
        "{html}"
    );
    let at = html.find("<p>Sirius's total is").expect("a Sirius paragraph");
    let paragraph = &html[at..at + html[at..].find("</p>").unwrap()];
    for said in [
        "the wall time around the statement and an Arrow fetch of its result",
        "the second-smallest of the timed runs after one warm-up",
        "parquet bytes pinned in Sirius's host cache",
        "reads outside the pin (likely bloom filters) inside the timed runs",
        "h100-80gb-hbm3, tpch sf40: Sirius 8d17afe, DuckDB 1.5.6, libcudf 26.08.01, driver 580.173, \
         device NVIDIA H100 80GB HBM3",
    ] {
        assert!(paragraph.contains(said), "{said:?} not in {paragraph}");
    }
    // Beside DuckDB's, before it, as the groups stand in the table.
    assert!(at < html.find("<p>DuckDB's time is").unwrap(), "{html}");
}

const CSV_HEADER: &str =
    "dataset,sf,query,engine,device,mode,load_ms,exec_ms,total_ms,peak_gib,status";

/// Every result of every engine, one row each: two modes of one device, the list's `wrong` at one
/// of them, a device without peaks, a failure on each engine, DuckDB and Sirius without load,
/// execute or peak, and DuckDB without a mode.
#[test]
fn the_csv_holds_every_result_in_the_pages_order() {
    let root = tree("csv");
    put(
        &root,
        "h100-80gb-hbm3/tpch.sf40/tp1_single.benchmark.txt",
        &[("q6", &failed("timeout: x")), ("q3", TIMED)],
    );
    put(
        &root,
        "h100-80gb-hbm3/tpch.sf40/tp4_sized.benchmark.txt",
        &[("q3", TIMED)],
    );
    list(&root, "h100-80gb-hbm3", "tpch,40,q3,tp1_single,wrong,#7\n");
    put(
        &root,
        "h200/tpch.sf40/tp1_single.benchmark.txt",
        &[("q3", UNBATCHED)],
    );
    sirius(
        &root,
        "h100-80gb-hbm3",
        "tpch",
        "q6\t-\t-\tfailed\tboom\nq3\t300.0\t300.0\tok\t-\n",
    );
    duckdb(&root, "duckdb-cpu128", "tpch", "nebius-cpu128");
    let csv = render_benchmarks_csv(&root, &order(&[("tpch", "q3"), ("tpch", "q6")]));
    let _ = std::fs::remove_dir_all(&root);
    assert_eq!(
        csv,
        format!(
            "{CSV_HEADER}\n\
             tpch,40,q3,peacockdb,h100-80gb-hbm3,tp1_single,1.0,0.5,1.5,2.00,wrong\n\
             tpch,40,q3,peacockdb,h100-80gb-hbm3,tp4_sized,1.0,0.5,1.5,2.00,ok\n\
             tpch,40,q3,peacockdb,h200,tp1_single,1.6,0.7,2.3,,ok\n\
             tpch,40,q3,sirius,h100-80gb-hbm3,tp1_single,,,300.0,,ok\n\
             tpch,40,q3,duckdb,duckdb-cpu128,,,,412.0,,ok\n\
             tpch,40,q6,peacockdb,h100-80gb-hbm3,tp1_single,,,,,failed\n\
             tpch,40,q6,sirius,h100-80gb-hbm3,tp1_single,,,,,failed\n\
             tpch,40,q6,duckdb,duckdb-cpu128,,,,,,failed\n"
        )
    );
}

/// The device list's `skipped` stands over a failed section, as on the page, and leaves no time.
#[test]
fn the_csv_takes_the_lists_skipped_over_the_section() {
    let root = tree("csv-skipped");
    put(
        &root,
        "h100-80gb-hbm3/tpch.sf40/tp1_single.benchmark.txt",
        &[("q3", &failed("timeout: x"))],
    );
    list(&root, "h100-80gb-hbm3", "tpch,40,q3,,skipped,oom: y\n");
    let csv = render_benchmarks_csv(&root, &order(&[("tpch", "q3")]));
    let _ = std::fs::remove_dir_all(&root);
    assert_eq!(
        csv,
        format!("{CSV_HEADER}\ntpch,40,q3,peacockdb,h100-80gb-hbm3,tp1_single,,,,,skipped\n")
    );
}

/// UPDATE_CANONICAL flips Sirius's q6 to `wrong`: the page's cell, the ratio's count and the CSV's
/// status all follow from the one file.
#[test]
fn a_status_flip_moves_page_ratio_and_csv_together() {
    let root = tree("csv-flip");
    put(
        &root,
        "h100-80gb-hbm3/tpch.sf40/tp1_single.benchmark.txt",
        &[("q3", TIMED), ("q6", TIMED)],
    );
    let dir = root.join("duckdb-cpu128/tpch.sf40");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("duckdb.tsv"),
        "# duckdb=1.5.4 threads=128 host=h\n\
         query\tms_2nd_min\truns_ms\tworkset_bytes\tdisk_read_bytes\twarm\tstatus\n\
         q3\t400\t400\t1\t0\tyes\tok\n\
         q6\t200\t200\t1\t0\tyes\tok\n",
    )
    .unwrap();
    let pairs = order(&[("tpch", "q3"), ("tpch", "q6")]);
    let q6 =
        |status: &str| format!("tpch,40,q6,sirius,h100-80gb-hbm3,tp1_single,,,100.0,,{status}\n");
    let mut seen = Vec::new();
    for status in ["ok", "wrong"] {
        sirius(
            &root,
            "h100-80gb-hbm3",
            "tpch",
            &format!("q3\t300.0\t300.0\tok\t-\nq6\t100.0\t100.0\t{status}\t-\n"),
        );
        let html = render_benchmark_report(&root, &pairs, &[]);
        let csv = render_benchmarks_csv(&root, &pairs);
        seen.push((html, csv));
    }
    let _ = std::fs::remove_dir_all(&root);
    let (before, after) = (&seen[0], &seen[1]);
    assert!(
        before.0.contains("DuckDB = 0.67 over 2 of 2 queries"),
        "{}",
        before.0
    );
    assert!(
        after.0.contains("DuckDB = 0.75 over 1 of 2 queries"),
        "{}",
        after.0
    );
    assert_eq!(row(&before.0, "q6")[7], "100.0");
    assert_eq!(row(&after.0, "q6")[7], "100.0 wrong answer");
    assert!(before.1.contains(&q6("ok")), "{}", before.1);
    assert!(after.1.contains(&q6("wrong")), "{}", after.1);
}
