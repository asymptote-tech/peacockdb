//! The per-device case list's format, over strings: what it reads back, and what it refuses.

use crate::test_support::{
    CaseStatus, DEVICE_CASES_HEADER, DeviceCase, parse_device_cases, render_device_cases,
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
