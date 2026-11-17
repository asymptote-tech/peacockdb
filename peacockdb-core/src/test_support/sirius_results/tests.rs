//! Sirius's results file over the shared fixture and over literal texts the format refuses.

use crate::test_support::{
    AnswerVerdict, SiriusResults, SiriusRow, SiriusStatus, parse_sirius_results, sirius_status_for,
    testdata_root, write_sirius_results,
};

const HEAD: &str = "# sirius=8d17afe duckdb=1.5.6 libcudf=26.08.01 cuda=13.0 driver=580.173 \
                    device=NVIDIA_H100_80GB_HBM3 host=h\n\
                    query\tms_2nd_min\truns_ms\trun_ms_2nd_min\trun_runs_ms\tstatus\tnote\n";

fn fixture() -> String {
    let path = testdata_root().join("fixtures/sirius-results.tsv");
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn preamble(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
    pairs
        .iter()
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect()
}

/// `testdata/fixtures/sirius-results.tsv`, which the runner's tests hold its output to.
#[test]
fn reads_the_fixture() {
    assert_eq!(
        parse_sirius_results(&fixture()),
        SiriusResults {
            preamble: preamble(&[
                ("sirius", "8d17afe"),
                ("duckdb", "1.5.6"),
                ("libcudf", "26.08.01"),
                ("cuda", "13.0"),
                ("driver", "580.173"),
                ("device", "NVIDIA_H100_80GB_HBM3"),
                ("host", "nebius-h100"),
            ]),
            rows: vec![
                SiriusRow {
                    query: "q1".to_string(),
                    ms_2nd_min: Some(412.0),
                    runs_ms: vec![
                        431.0, 412.0, 409.0, 415.0, 420.0, 413.0, 418.0, 425.0, 414.0, 416.0,
                    ],
                    run_ms_2nd_min: Some(301.0),
                    run_runs_ms: vec![
                        305.0, 301.0, 299.0, 303.0, 308.0, 302.0, 306.0, 304.0, 307.0, 309.0,
                    ],
                    status: SiriusStatus::Ok,
                    note: String::new(),
                },
                SiriusRow {
                    query: "q2".to_string(),
                    ms_2nd_min: None,
                    runs_ms: Vec::new(),
                    run_ms_2nd_min: None,
                    run_runs_ms: Vec::new(),
                    status: SiriusStatus::Failed,
                    note: "NotImplemented: INTERSECT".to_string(),
                },
                SiriusRow {
                    query: "q3".to_string(),
                    ms_2nd_min: None,
                    runs_ms: Vec::new(),
                    run_ms_2nd_min: None,
                    run_runs_ms: Vec::new(),
                    status: SiriusStatus::Skipped,
                    note: "accumulator budget exceeded".to_string(),
                },
            ],
        }
    );
}

#[test]
fn the_fixture_reads_its_run_columns() {
    let results = parse_sirius_results(&fixture());
    let q1 = &results.rows[0];
    assert_eq!(q1.query, "q1");
    assert_eq!(q1.run_ms_2nd_min, Some(301.0));
    assert_eq!(
        q1.run_runs_ms,
        [
            305.0, 301.0, 299.0, 303.0, 308.0, 302.0, 306.0, 304.0, 307.0, 309.0
        ]
    );
}

/// A row timed before the execution windows were recorded.
#[test]
fn a_timed_row_without_run_times_reads_as_none() {
    let text = format!("{HEAD}q6\t17.0\t17.0,16.0\t-\t-\tok\t-\n");
    let results = parse_sirius_results(&text);
    assert_eq!(results.rows[0].ms_2nd_min, Some(17.0));
    assert_eq!(results.rows[0].run_ms_2nd_min, None);
    assert!(results.rows[0].run_runs_ms.is_empty());
    assert_eq!(write_sirius_results(&results), text);
}

#[test]
#[should_panic(expected = "1 run_runs_ms for 2 runs_ms")]
fn a_run_count_unlike_the_wall_count_is_refused() {
    parse_sirius_results(&format!("{HEAD}q6\t17.0\t17.0,16.0\t15.0\t15.0\tok\t-\n"));
}

#[test]
#[should_panic(expected = "both set or both")]
fn a_run_time_without_its_runs_is_refused() {
    parse_sirius_results(&format!("{HEAD}q6\t17.0\t17.0,16.0\t15.0\t-\tok\t-\n"));
}

#[test]
#[should_panic(expected = "both set or both")]
fn runs_without_their_run_time_are_refused() {
    parse_sirius_results(&format!("{HEAD}q6\t17.0\t17.0,16.0\t-\t15.0,16.0\tok\t-\n"));
}

#[test]
#[should_panic(expected = "a failed or skipped row has no run time")]
fn a_failed_row_with_a_run_time_is_refused() {
    parse_sirius_results(&format!("{HEAD}q6\t-\t-\t15.0\t15.0,16.0\tfailed\tboom\n"));
}

#[test]
#[should_panic(expected = "a failed or skipped row has no run time")]
fn a_skipped_row_with_a_run_time_is_refused() {
    parse_sirius_results(&format!(
        "{HEAD}q6\t-\t-\t15.0\t15.0,16.0\tskipped\tbudget\n"
    ));
}

#[test]
fn writes_what_it_reads() {
    let text = fixture();
    assert_eq!(write_sirius_results(&parse_sirius_results(&text)), text);
}

#[test]
fn a_wrong_and_an_unverified_row_keep_their_times() {
    let text = format!(
        "{HEAD}q6\t100.0\t101.5,100.0,99.0\t-\t-\twrong\t-\n\
         aggregate-groupby\t50.0\t50.0,49.0\t48.0\t48.0,47.0\tunverified\t-\n"
    );
    let results = parse_sirius_results(&text);
    assert_eq!(
        results.rows[1],
        SiriusRow {
            query: "aggregate-groupby".to_string(),
            ms_2nd_min: Some(50.0),
            runs_ms: vec![50.0, 49.0],
            run_ms_2nd_min: Some(48.0),
            run_runs_ms: vec![48.0, 47.0],
            status: SiriusStatus::Unverified,
            note: String::new(),
        }
    );
    assert_eq!(results.rows[0].status, SiriusStatus::Wrong);
    assert_eq!(write_sirius_results(&results), text);
}

#[test]
#[should_panic(expected = "sirius results line 1")]
fn a_missing_preamble_is_refused() {
    parse_sirius_results(&HEAD[HEAD.find('\n').unwrap() + 1..]);
}

#[test]
#[should_panic(expected = "sirius results line 1")]
fn a_preamble_out_of_order_is_refused() {
    parse_sirius_results(
        "# duckdb=1.5.6 sirius=8d17afe libcudf=26.08.01 cuda=13.0 driver=580.173 device=d host=h\n\
         query\tms_2nd_min\truns_ms\trun_ms_2nd_min\trun_runs_ms\tstatus\tnote\n",
    );
}

#[test]
#[should_panic(expected = "sirius results line 2")]
fn a_wrong_column_line_is_refused() {
    parse_sirius_results(
        &HEAD.replace("run_ms_2nd_min\trun_runs_ms", "run_runs_ms\trun_ms_2nd_min"),
    );
}

#[test]
#[should_panic(expected = "sirius results line 3")]
fn eight_fields_are_refused() {
    parse_sirius_results(&format!("{HEAD}q6\t17.0\t17.0,16.0\t-\t-\tok\t-\textra\n"));
}

#[test]
#[should_panic(expected = "sirius results line 4")]
fn a_query_listed_twice_is_refused() {
    parse_sirius_results(&format!(
        "{HEAD}q6\t17.0\t17.0,16.0\t-\t-\tok\t-\nq6\t18.0\t18.0,19.0\t-\t-\tok\t-\n"
    ));
}

#[test]
#[should_panic(expected = "sirius results line 3")]
fn an_ok_row_without_a_time_is_refused() {
    parse_sirius_results(&format!("{HEAD}q6\t-\t-\t-\t-\tok\t-\n"));
}

#[test]
#[should_panic(expected = "sirius results line 3")]
fn a_wrong_row_without_a_time_is_refused() {
    parse_sirius_results(&format!("{HEAD}q6\t-\t-\t-\t-\twrong\t-\n"));
}

#[test]
#[should_panic(expected = "sirius results line 3")]
fn an_unverified_row_without_a_time_is_refused() {
    parse_sirius_results(&format!("{HEAD}q6\t-\t-\t-\t-\tunverified\t-\n"));
}

#[test]
#[should_panic(expected = "sirius results line 3")]
fn a_failed_row_with_a_time_is_refused() {
    parse_sirius_results(&format!("{HEAD}q6\t17.0\t17.0,16.0\t-\t-\tfailed\tboom\n"));
}

#[test]
#[should_panic(expected = "sirius results line 3")]
fn a_skipped_row_with_a_time_is_refused() {
    parse_sirius_results(&format!(
        "{HEAD}q6\t17.0\t17.0,16.0\t-\t-\tskipped\tbudget\n"
    ));
}

#[test]
#[should_panic(expected = "sirius results line 3")]
fn an_unknown_status_is_refused() {
    parse_sirius_results(&format!("{HEAD}q6\t17.0\t17.0,16.0\t-\t-\tdone\t-\n"));
}

#[test]
fn failed_and_skipped_rows_keep_their_status() {
    assert_eq!(
        sirius_status_for(SiriusStatus::Failed, None),
        Ok(SiriusStatus::Failed)
    );
    assert_eq!(
        sirius_status_for(SiriusStatus::Skipped, None),
        Ok(SiriusStatus::Skipped)
    );
}

#[test]
fn a_timed_row_takes_its_status_from_its_answer() {
    let differs = AnswerVerdict::Differs("row 1 differs".to_string());
    assert_eq!(
        sirius_status_for(SiriusStatus::Wrong, Some(AnswerVerdict::Meets)),
        Ok(SiriusStatus::Ok)
    );
    assert_eq!(
        sirius_status_for(SiriusStatus::Ok, Some(differs)),
        Ok(SiriusStatus::Wrong)
    );
    assert_eq!(
        sirius_status_for(SiriusStatus::Ok, Some(AnswerVerdict::NoReference)),
        Ok(SiriusStatus::Unverified)
    );
}

/// No Sirius section, or DuckDB's `failed:`: no status can be set, and the verdict's text says
/// how to record the missing answer.
#[test]
fn a_timed_row_whose_answer_is_missing_is_refused() {
    for said in [
        "sirius-result.txt holds no answer for q6",
        "DuckDB does not answer this query",
    ] {
        let verdict = AnswerVerdict::Unanswered(said.to_string());
        let refused = sirius_status_for(SiriusStatus::Ok, Some(verdict)).expect_err("no answer");
        assert!(refused.contains(said), "{refused}");
    }
}
