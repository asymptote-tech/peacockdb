//! DuckDB's results file over the shared fixture and over literal texts the format refuses.

use crate::test_support::{
    DuckdbResults, DuckdbRow, DuckdbStatus, parse_duckdb_results, testdata_root,
};

const HEAD: &str = "# duckdb=1.5.4 threads=8 host=h\n\
                    query\tms_2nd_min\truns_ms\tworkset_bytes\tdisk_read_bytes\twarm\tstatus\n";

fn preamble(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
    pairs
        .iter()
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect()
}

/// `testdata/fixtures/duckdb-results.tsv`, which `benchmarks/tests/test_duckdb_minimal.py` holds
/// the script's output to, and which a script reading the format is tested against.
#[test]
fn the_shared_fixture_reads_as_expected() {
    let path = testdata_root().join("fixtures/duckdb-results.tsv");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    assert_eq!(
        parse_duckdb_results(&text),
        DuckdbResults {
            preamble: preamble(&[
                ("duckdb", "1.5.4"),
                ("threads", "128"),
                ("host", "nebius-cpu128"),
                ("cpu", "AMD_EPYC_9654_96-Core_Processor"),
                ("cores", "128"),
                ("memory_gib", "1007.7"),
                ("db", "/var/lib/peacock/tpch.sf40.duckdb"),
                ("resident_pct", "100.0"),
                ("floor_ms", "31"),
            ]),
            rows: vec![
                DuckdbRow {
                    query: "q1".to_string(),
                    ms_2nd_min: Some(412.0),
                    runs_ms: vec![431.0, 412.0, 409.0, 415.0, 420.0, 413.0],
                    workset_bytes: Some(2147483648),
                    disk_read_bytes: Some(0),
                    warm: "yes".to_string(),
                    status: DuckdbStatus::Ok,
                },
                DuckdbRow {
                    query: "aggregate_groupby".to_string(),
                    ms_2nd_min: None,
                    runs_ms: Vec::new(),
                    workset_bytes: Some(1073741824),
                    disk_read_bytes: None,
                    warm: "-".to_string(),
                    status: DuckdbStatus::Failed,
                },
            ],
        }
    );
}

#[test]
fn a_working_set_it_could_not_size_reads_as_none() {
    let results = parse_duckdb_results(&format!("{HEAD}q6\t17\t17,16\t-\t0\tyes\tok\n"));
    assert_eq!(results.rows[0].workset_bytes, None);
    assert_eq!(
        results.preamble[1],
        ("threads".to_string(), "8".to_string())
    );
}

#[test]
#[should_panic(expected = "duckdb results line 3")]
fn a_short_row_is_refused() {
    parse_duckdb_results(&format!("{HEAD}q6\t17\t17,16\t0\tyes\tok\n"));
}

#[test]
#[should_panic(expected = "unknown status \"skipped\"")]
fn an_unknown_status_is_refused() {
    parse_duckdb_results(&format!("{HEAD}q6\t17\t17,16\t0\t0\tyes\tskipped\n"));
}

#[test]
#[should_panic(expected = "a failed row has no time")]
fn a_failed_row_with_a_time_is_refused() {
    parse_duckdb_results(&format!("{HEAD}q6\t17\t17,16\t0\t0\tyes\tfailed\n"));
}

#[test]
#[should_panic(expected = "duckdb results line 3")]
fn an_ok_row_without_its_time_is_refused() {
    parse_duckdb_results(&format!("{HEAD}q6\t-\t-\t0\t0\tyes\tok\n"));
}

#[test]
#[should_panic(expected = "duckdb results line 1")]
fn a_missing_preamble_is_refused() {
    parse_duckdb_results(&HEAD[HEAD.find('\n').unwrap() + 1..]);
}

#[test]
#[should_panic(expected = "duckdb results line 2")]
fn a_header_out_of_order_is_refused() {
    parse_duckdb_results(
        "# duckdb=1.5.4\nquery\truns_ms\tms_2nd_min\tworkset_bytes\tdisk_read_bytes\twarm\tstatus\n",
    );
}

#[test]
#[should_panic(expected = "listed twice")]
fn a_query_listed_twice_is_refused() {
    parse_duckdb_results(&format!(
        "{HEAD}q6\t17\t17,16\t0\t0\tyes\tok\nq6\t18\t18,19\t0\t0\tyes\tok\n"
    ));
}
