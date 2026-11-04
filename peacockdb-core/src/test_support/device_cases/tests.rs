//! The per-device case list's format, over strings: what it reads back, and what it refuses;
//! and the rows a device's tree makes of the main list's candidates.

use std::collections::BTreeMap;

use crate::test_support::{
    CaseStatus, DEVICE_CASES_HEADER, DeviceCase, Measured, device_lines, parse_device_cases,
    render_device_cases, testdata_root,
};

fn case(query: &str, modes: &[&str], status: CaseStatus, note: &str) -> DeviceCase {
    DeviceCase {
        dataset: "tpch".to_string(),
        sf: "40".to_string(),
        query: query.to_string(),
        modes: modes.iter().map(|mode| mode.to_string()).collect(),
        status,
        note: note.to_string(),
    }
}

/// `rows` under the header.
fn list(rows: &str) -> String {
    format!("{DEVICE_CASES_HEADER}\n{rows}")
}

/// What `parse_device_cases` panics with over `text`; a list it accepts fails the test.
fn refusal(text: &str) -> String {
    let refused =
        std::panic::catch_unwind(|| parse_device_cases(text)).expect_err("the list was accepted");
    match refused.downcast::<String>() {
        Ok(message) => *message,
        Err(payload) => payload
            .downcast_ref::<&str>()
            .expect("a panic message")
            .to_string(),
    }
}

/// One row of each status, rendered and read back, is what it was; the text is the header and
/// one line per row, the modes `|`-joined in their order.
#[test]
fn a_list_round_trips() {
    let cases = vec![
        case("q1", &["tp1_single", "tp4_sized"], CaseStatus::Ok, ""),
        case(
            "aggregate_groupby",
            &["tp4_single"],
            CaseStatus::Wrong,
            "#7",
        ),
        case("q9", &[], CaseStatus::Skipped, "out of device memory"),
    ];
    let text = render_device_cases(&cases);
    assert_eq!(
        text,
        list(
            "tpch,40,q1,tp1_single|tp4_sized,ok,\n\
             tpch,40,aggregate_groupby,tp4_single,wrong,#7\n\
             tpch,40,q9,,skipped,out of device memory\n"
        )
    );
    assert_eq!(parse_device_cases(&text), cases);
}

/// The note is the rest of the line after the fifth comma, so an error message keeps its
/// commas and takes no column of its own.
#[test]
fn a_note_may_hold_commas() {
    let cases = parse_device_cases(&list("tpch,40,q9,,skipped,error: a, b\n"));
    assert_eq!(cases, [case("q9", &[], CaseStatus::Skipped, "error: a, b")]);
}

#[test]
fn a_skipped_row_has_no_modes_and_a_reason() {
    let timed = refusal(&list("tpch,40,q9,tp1_single,skipped,too large\n"));
    assert!(
        timed.contains("line 2") && timed.contains("a skipped row times no mode"),
        "{timed}"
    );
    let silent = refusal(&list("tpch,40,q9,,skipped,\n"));
    assert!(
        silent.contains("line 2") && silent.contains("names its reason"),
        "{silent}"
    );
}

#[test]
#[should_panic(expected = "note is its ticket, #NN")]
fn a_wrong_row_names_its_ticket() {
    parse_device_cases(&list("tpch,40,q1,tp1_single,wrong,7\n"));
}

#[test]
#[should_panic(expected = "an ok row has no note")]
fn an_ok_row_has_no_note() {
    parse_device_cases(&list("tpch,40,q1,tp1_single,ok,#7\n"));
}

#[test]
fn an_ok_or_wrong_row_names_its_modes() {
    for row in ["tpch,40,q1,,ok,\n", "tpch,40,q1,,wrong,#7\n"] {
        let refused = refusal(&list(row));
        assert!(refused.contains("names the modes it ran"), "{refused}");
    }
}

#[test]
#[should_panic(expected = "the first line is not")]
fn a_wrong_header_is_refused() {
    parse_device_cases("dataset,sf,query,modes,status\ntpch,40,q1,tp1_single,ok,\n");
}

/// The mode a row names is the main list's ident, and only the five.
#[test]
#[should_panic(expected = r#"line 3 "tpch,40,q6,tp1-single,ok,": unknown mode "tp1-single""#)]
fn an_unknown_mode_is_refused() {
    parse_device_cases(&list(
        "tpch,40,q1,tp1_single,ok,\ntpch,40,q6,tp1-single,ok,\n",
    ));
}

#[test]
#[should_panic(expected = "unknown status \"passed\"")]
fn an_unknown_status_is_refused() {
    parse_device_cases(&list("tpch,40,q1,tp1_single,passed,\n"));
}

#[test]
#[should_panic(expected = "line 3")]
fn a_row_short_of_six_fields_is_refused() {
    parse_device_cases(&list("tpch,40,q1,tp1_single,ok,\ntpch,40,q6\n"));
}

/// Two rows of one `(dataset, sf, query)` would hold the device to whichever a reader took.
#[test]
fn a_case_twice_is_refused() {
    let refused = refusal(&list(
        "tpch,40,q1,tp1_single,ok,\ntpch,40,q6,tp1_single,ok,\ntpch,40,q1,tp4_sized,ok,\n",
    ));
    assert!(
        refused.contains("line 4") && refused.contains("line 2"),
        "{refused}"
    );
}

// --- the H100 list written from the tree -------------------------------------------------

/// A candidate as the main list writes it: tpch, the query, its modes.
fn candidate(query: &str, modes: &[&str]) -> (String, String, Vec<String>) {
    (
        "tpch".to_string(),
        query.to_string(),
        modes.iter().map(|mode| mode.to_string()).collect(),
    )
}

/// `(mode, [(section, measured)])` as a tree's files: a mode not named has no file.
fn tree(
    files: Vec<(&str, Vec<(&str, Measured)>)>,
) -> impl Fn(&str, &str) -> Option<BTreeMap<String, Measured>> {
    let files: BTreeMap<String, BTreeMap<String, Measured>> = files
        .into_iter()
        .map(|(mode, sections)| {
            let sections = sections
                .into_iter()
                .map(|(query, measured)| (query.to_string(), measured))
                .collect();
            (mode.to_string(), sections)
        })
        .collect();
    move |dataset, mode| match dataset {
        "tpch" => files.get(mode).cloned(),
        _ => None,
    }
}

fn failed(reason: &str) -> Measured {
    Measured::Failed(reason.to_string())
}

const TWO_MODES: &[&str] = &["tp1_single", "tp4_sized"];

/// `device_lines` over `candidates` and the tree, with no `wrong` mark.
fn lines(
    candidates: &[(String, String, Vec<String>)],
    files: &dyn Fn(&str, &str) -> Option<BTreeMap<String, Measured>>,
) -> Result<Vec<DeviceCase>, String> {
    device_lines(candidates, TWO_MODES, files, &BTreeMap::new())
}

#[test]
fn a_timed_section_makes_an_ok_row() {
    let files = tree(vec![
        ("tp1_single", vec![("q1", Measured::Timed)]),
        ("tp4_sized", vec![("q1", Measured::Timed)]),
    ]);
    let rows = lines(&[candidate("q1", TWO_MODES)], &files);
    assert_eq!(rows, Ok(vec![case("q1", TWO_MODES, CaseStatus::Ok, "")]));
}

/// A section's name is the query as the file writes it, `_` read as `-`.
#[test]
fn a_query_is_found_under_its_section_name() {
    let files = tree(vec![(
        "tp1_single",
        vec![("aggregate-groupby", Measured::Timed)],
    )]);
    let rows = lines(&[candidate("aggregate_groupby", &["tp1_single"])], &files);
    assert_eq!(
        rows,
        Ok(vec![case(
            "aggregate_groupby",
            &["tp1_single"],
            CaseStatus::Ok,
            ""
        )])
    );
}

#[test]
fn a_query_failed_in_every_mode_is_a_skipped_row() {
    let files = tree(vec![
        (
            "tp1_single",
            vec![("q9", failed("timeout: past 1800 s, killed"))],
        ),
        ("tp4_sized", vec![("q9", failed("out of device memory"))]),
    ]);
    let rows = lines(&[candidate("q9", TWO_MODES)], &files);
    assert_eq!(
        rows,
        Ok(vec![case(
            "q9",
            &[],
            CaseStatus::Skipped,
            "timeout: past 1800 s, killed"
        )])
    );
}

#[test]
fn a_mode_failed_beside_a_timed_one_drops_from_the_row() {
    let files = tree(vec![
        ("tp1_single", vec![("q1", Measured::Timed)]),
        ("tp4_sized", vec![("q1", failed("out of device memory"))]),
    ]);
    let rows = lines(&[candidate("q1", TWO_MODES)], &files);
    assert_eq!(
        rows,
        Ok(vec![case("q1", &["tp1_single"], CaseStatus::Ok, "")])
    );
}

/// A mode whose file the tree lacks is not measured yet on this device: no row names it.
#[test]
fn a_mode_without_its_file_is_not_considered() {
    let files = tree(vec![("tp1_single", vec![("q1", Measured::Timed)])]);
    let rows = lines(&[candidate("q1", TWO_MODES)], &files);
    assert_eq!(
        rows,
        Ok(vec![case("q1", &["tp1_single"], CaseStatus::Ok, "")])
    );
}

/// A mode the main list's line does not carry is not considered, whatever the tree holds.
#[test]
fn a_mode_the_line_does_not_carry_is_not_considered() {
    let files = tree(vec![
        ("tp1_single", vec![("q1", Measured::Timed)]),
        ("tp4_sized", vec![("q1", Measured::Timed)]),
    ]);
    let rows = lines(&[candidate("q1", &["tp1_single"])], &files);
    assert_eq!(
        rows,
        Ok(vec![case("q1", &["tp1_single"], CaseStatus::Ok, "")])
    );
}

#[test]
fn a_missing_section_is_refused() {
    let files = tree(vec![
        ("tp1_single", vec![("q1", Measured::Timed)]),
        ("tp4_sized", vec![("q6", Measured::Timed)]),
    ]);
    let problem = lines(&[candidate("q1", TWO_MODES)], &files)
        .expect_err("q1 has no tp4_sized section and was written");
    assert!(
        problem.contains("tpch") && problem.contains("q1") && problem.contains("tp4_sized"),
        "{problem}"
    );
}

/// `tp1_single` is always considered: the tree without its file has not measured the query.
#[test]
fn a_missing_tp1_single_file_is_refused() {
    let problem = lines(&[candidate("q1", TWO_MODES)], &tree(vec![]))
        .expect_err("a tree with no file was written");
    assert!(
        problem.contains("q1") && problem.contains("tp1_single"),
        "{problem}"
    );
}

/// tp4-sized does not run a query whose `tp1_single` failed, so its file owes it no section.
#[test]
fn a_later_mode_of_a_query_failed_at_tp1_single_owes_no_section() {
    let files = tree(vec![
        ("tp1_single", vec![("q9", failed("out of device memory"))]),
        ("tp4_sized", vec![("q1", Measured::Timed)]),
    ]);
    let rows = lines(&[candidate("q9", TWO_MODES)], &files);
    assert_eq!(
        rows,
        Ok(vec![case(
            "q9",
            &[],
            CaseStatus::Skipped,
            "out of device memory"
        )])
    );
}

/// The tree knows no tickets: a `wrong` mark the file carried comes back on the measured row.
#[test]
fn a_rewrite_keeps_a_wrong_answer_mark() {
    let files = tree(vec![
        (
            "tp1_single",
            vec![("q1", Measured::Timed), ("q6", Measured::Timed)],
        ),
        (
            "tp4_sized",
            vec![("q1", Measured::Timed), ("q6", Measured::Timed)],
        ),
    ]);
    let wrong = BTreeMap::from([(("tpch".to_string(), "q6".to_string()), "#7".to_string())]);
    let rows = device_lines(
        &[candidate("q1", TWO_MODES), candidate("q6", TWO_MODES)],
        TWO_MODES,
        &files,
        &wrong,
    );
    assert_eq!(
        rows,
        Ok(vec![
            case("q1", TWO_MODES, CaseStatus::Ok, ""),
            case("q6", TWO_MODES, CaseStatus::Wrong, "#7"),
        ])
    );
}

#[test]
fn a_wrong_mark_on_a_skipped_query_is_refused() {
    let files = tree(vec![(
        "tp1_single",
        vec![("q9", failed("out of device memory"))],
    )]);
    let wrong = BTreeMap::from([(("tpch".to_string(), "q9".to_string()), "#7".to_string())]);
    let problem = device_lines(&[candidate("q9", TWO_MODES)], TWO_MODES, &files, &wrong)
        .expect_err("a wrong mark on a query with no measurement was kept");
    assert!(
        problem.contains("q9") && problem.contains("#7"),
        "{problem}"
    );
}

/// A mark for a query no candidate names would be dropped by the rewrite without a word.
#[test]
fn a_wrong_mark_on_no_candidate_is_refused() {
    let files = tree(vec![("tp1_single", vec![("q1", Measured::Timed)])]);
    let wrong = BTreeMap::from([(("tpch".to_string(), "q99".to_string()), "#7".to_string())]);
    let problem = device_lines(&[candidate("q1", TWO_MODES)], TWO_MODES, &files, &wrong)
        .expect_err("a wrong mark on no candidate was dropped");
    assert!(
        problem.contains("q99") && problem.contains("#7"),
        "{problem}"
    );
}

/// `testdata/fixtures/device-cases.csv`, which `scripts/calibration/device_cases.py`'s test reads
/// to the same values, field by field.
#[test]
fn the_shared_fixture_reads_as_expected() {
    let path = testdata_root().join("fixtures/device-cases.csv");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let in_tpcds = |mut case: DeviceCase| {
        case.dataset = "tpcds".to_string();
        case
    };
    assert_eq!(
        parse_device_cases(&text),
        [
            case("q1", &["tp1_single", "tp4_sized"], CaseStatus::Ok, ""),
            case(
                "aggregate_groupby",
                &["tp1_single"],
                CaseStatus::Wrong,
                "#7"
            ),
            case(
                "q9",
                &[],
                CaseStatus::Skipped,
                "timeout: the case ran past 1800 s, and was killed"
            ),
            in_tpcds(case("q37", &["tp1_single"], CaseStatus::Ok, "")),
        ]
    );
}
