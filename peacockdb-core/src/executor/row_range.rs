//! [`RowRange`](super::RowRange)'s clamp, which C++'s `clamp_row_range` is the twin of.

use super::RowRange;

/// The rows of a batch this range actually names, as `(offset, length)`. The twin of
/// C++'s `clamp_row_range` (`node_session.cpp`), which the export and the slice share
/// so that the two cannot disagree — this is the same rule for the backend that never
/// crosses the ABI, and the two answering differently would be a divergence no test
/// of either one alone could see.
pub(crate) fn clamp(range: &RowRange, n_rows: u64) -> (u64, u64) {
    let offset = range.offset.min(n_rows);
    (offset, range.length.min(n_rows - offset))
}

#[cfg(test)]
mod tests {
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
}
