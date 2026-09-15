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
mod tests;
