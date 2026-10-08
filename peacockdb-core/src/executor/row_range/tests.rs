use super::RowRange;
use crate::test_support::testdata_root;

/// One line of `testdata/fixtures/row-range-clamp.txt`.
struct Case {
    line: usize,
    range: RowRange,
    rows: u64,
    span: (u64, u64),
}

/// Digits only, as the C++ reader takes them: `u64::from_str` accepts a leading `+`.
fn count(field: &str, line: usize) -> u64 {
    if field.is_empty() || !field.bytes().all(|b| b.is_ascii_digit()) {
        panic!("row-range-clamp.txt:{line}: `{field}` is not a count");
    }
    field
        .parse()
        .unwrap_or_else(|_| panic!("row-range-clamp.txt:{line}: `{field}` does not fit a u64"))
}

/// `max` is the to-the-end sentinel, allowed in offset and length only.
fn bound(field: &str, line: usize) -> u64 {
    if field == "max" {
        u64::MAX
    } else {
        count(field, line)
    }
}

fn cases() -> Vec<Case> {
    let path = testdata_root().join("fixtures/row-range-clamp.txt");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
    let mut cases = Vec::new();
    for (index, raw) in text.lines().enumerate() {
        let line = index + 1;
        let trimmed = raw.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let fields: Vec<&str> = trimmed.split_whitespace().collect();
        let [offset, length, rows, "->", begin, end] = fields.as_slice() else {
            panic!(
                "row-range-clamp.txt:{line}: expected `offset length rows -> begin end`, got \
                 `{trimmed}`"
            );
        };
        cases.push(Case {
            line,
            range: RowRange {
                offset: bound(offset, line),
                length: bound(length, line),
            },
            rows: count(rows, line),
            span: (count(begin, line), count(end, line)),
        });
    }
    assert!(!cases.is_empty(), "{} holds no case", path.display());
    cases
}

/// The clamp answers `(offset, length)`; the table speaks the half-open span C++ answers,
/// so the length is added back here and nowhere else.
#[test]
fn the_clamp_answers_every_case_in_the_shared_table() {
    for case in cases() {
        let (offset, length) = case.range.clamp(case.rows);
        assert_eq!(
            (offset, offset + length),
            case.span,
            "row-range-clamp.txt:{}: {:?} over {} rows",
            case.line,
            (case.range.offset, case.range.length),
            case.rows
        );
    }
}
