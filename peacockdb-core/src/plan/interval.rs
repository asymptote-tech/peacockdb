//! [`RowInterval`](super::RowInterval)'s range, which is three statements rather than one.

use super::RowInterval;
use crate::executor::RowRange;

pub(crate) fn range_of(interval: &RowInterval, seen: u64, n_rows: u64) -> Option<RowRange> {
    let start = interval.skip.saturating_sub(seen);
    let stop = match interval.stop() {
        Some(stop) => n_rows.min(stop.saturating_sub(seen)),
        None => n_rows,
    };
    // `then`, not `then_some`: the subtraction is the answer only when it is in range,
    // and an eager argument underflows on every batch of the skip prefix.
    (start < stop).then(|| crate::executor::RowRange {
        offset: start,
        length: stop - start,
    })
}
