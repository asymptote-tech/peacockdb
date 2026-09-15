use super::RowRange;

/// The to-the-end sentinel is the case the clamp is written around: subtracting the
/// offset from the row count rather than adding it to the length is what keeps
/// `u64::MAX` from wrapping.
#[test]
fn a_range_to_the_end_takes_every_row_after_its_offset() {
    assert_eq!(RowRange::WHOLE.clamp(4), (0, 4));
    assert_eq!(
        RowRange {
            offset: 3,
            length: u64::MAX,
        }
        .clamp(4),
        (3, 1)
    );
}

/// A fetch legitimately overruns the batch it straddles, and an offset past the end
/// names no rows rather than a negative count.
#[test]
fn a_range_past_the_end_clamps_to_what_is_there() {
    assert_eq!(
        RowRange {
            offset: 1,
            length: 100
        }
        .clamp(4),
        (1, 3)
    );
    assert_eq!(
        RowRange {
            offset: 9,
            length: 1
        }
        .clamp(4),
        (4, 0)
    );
}
