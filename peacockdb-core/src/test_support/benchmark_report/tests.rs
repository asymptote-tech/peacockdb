//! The benchmark report page over temp trees built from literal sections: what a cell reads
//! for each kind of section, where the device's list overrides it, and the page's layout.

use std::path::{Path, PathBuf};

use crate::test_support::{
    DEVICE_CASES_HEADER, FAILED, ReportCell, ReportTable, SKIPPED, render_benchmark_report,
    section_href,
};

/// A section written now: two loaders (1000 µs) and an unload (500 µs).
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
pinned_cache=off source=env
prefill_files=0
prefill_bytes=0
prefill_us=0
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

/// A temp `benchmark-results/` of its own per test.
fn tree(name: &str) -> PathBuf {
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

/// The row for `query` at `mode`.
fn row(html: &str, query: &str, mode: &str) -> Vec<String> {
    rows(html)
        .into_iter()
        .find(|cells| cells[0] == query && cells[1] == mode)
        .unwrap_or_else(|| panic!("no row {query} {mode} in:\n{html}"))
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
    assert_eq!(
        row(&html, "q3", "tp1_single"),
        ["q3", "tp1_single", "1.0", "0.5", "1.5", "2.00"]
    );
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
    // load (766 + 849) µs, execute 2321 − 1615.
    assert_eq!(
        row(&html, "q1", "tp1_single"),
        ["q1", "tp1_single", "1.6", "0.7", "2.3", "—"]
    );
}

#[test]
fn a_failed_case_shows_its_reason_and_the_list_wins() {
    let root = tree("failed");
    let rel = "h100-80gb-hbm3/tpch.sf40/tp1_single.benchmark.txt";
    put(&root, rel, &[("q3", &failed("timeout: x"))]);
    let html = render_benchmark_report(&root, &order(&[("tpch", "q3")]), &[]);
    assert_eq!(
        row(&html, "q3", "tp1_single"),
        ["q3", "tp1_single", "failed: timeout: x"]
    );
    assert!(html.contains("<td colspan=\"4\" class=\"off\">"), "{html}");

    list(&root, "h100-80gb-hbm3", "tpch,40,q3,,skipped,oom: y\n");
    let html = page(root, &[("tpch", "q3")]);
    assert_eq!(
        row(&html, "q3", "tp1_single"),
        ["q3", "tp1_single", "skipped: oom: y"]
    );
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
        row(&html, "q2", "tp1_single"),
        ["q2", "tp1_single", "skipped: not enabled at this mode"]
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
    let cells = row(&html, "q3", "tp1_single");
    assert_eq!(cells[4], "1.5 wrong answer #7", "{html}");
}

#[test]
fn devices_sit_side_by_side_and_a_case_one_lacks_is_absent() {
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
    let h100 = html.find(">h100-80gb-hbm3</th>").expect("an h100 column");
    let h200 = html.find(">h200</th>").expect("an h200 column");
    assert!(h100 < h200, "devices by name");
    assert_eq!(
        row(&html, "q3", "tp1_single"),
        ["q3", "tp1_single", "1.0", "0.5", "1.5", "2.00", "—"]
    );
    assert!(
        html.contains("<td colspan=\"4\" class=\"na\">—</td>"),
        "{html}"
    );
    assert_eq!(
        row(&html, "q6", "tp1_single"),
        [
            "q6",
            "tp1_single",
            "1.0",
            "0.5",
            "1.5",
            "2.00",
            "1.6",
            "0.7",
            "2.3",
            "—"
        ]
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
    std::fs::create_dir_all(root.join("duckdb/tpch.sf40")).unwrap();
    std::fs::write(root.join("duckdb/tpch.sf40/duckdb.tsv"), "q1\t1\n").unwrap();
    let html = page(root, &[("tpch", "q1")]);
    assert!(!html.contains("duckdb"), "{html}");
    assert_eq!(row(&html, "q1", "tp1_single").len(), 6);
}

#[test]
fn rows_follow_the_main_list_then_modes_in_order() {
    let root = tree("order");
    put(
        &root,
        "h200/tpch.sf40/tp4_sized.benchmark.txt",
        &[("q1", UNBATCHED)],
    );
    put(
        &root,
        "h200/tpch.sf40/tp1_single.benchmark.txt",
        &[
            ("aggregate-groupby", UNBATCHED),
            ("q1", UNBATCHED),
            ("q6", UNBATCHED),
            ("unlisted", UNBATCHED),
        ],
    );
    let html = page(
        root,
        &[
            ("tpch", "q6"),
            ("tpch", "aggregate_groupby"),
            ("tpch", "q1"),
        ],
    );
    let order: Vec<(String, String)> = rows(&html)
        .into_iter()
        .map(|cells| (cells[0].clone(), cells[1].clone()))
        .collect();
    let want = [
        ("q6", "tp1_single"),
        ("aggregate-groupby", "tp1_single"),
        ("q1", "tp1_single"),
        ("q1", "tp4_sized"),
        ("unlisted", "tp1_single"),
    ];
    assert_eq!(
        order,
        want.map(|(query, mode)| (query.to_string(), mode.to_string()))
    );
    assert!(html.contains("<h2 id=\"tpch-sf40\">"), "{html}");
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
                text: "q1+q6".to_string(),
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
    let root = tree("twice");
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
