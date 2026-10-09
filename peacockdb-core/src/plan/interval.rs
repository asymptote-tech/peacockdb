//! [`RowInterval`](super::RowInterval)'s arithmetic: which rows of a batch it wants, and one
//! interval over another's output.

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

pub(crate) fn over(outer: &RowInterval, inner: &RowInterval) -> RowInterval {
    // What the inner cut leaves once the outer skip is spent in it; `None` is unbounded.
    let left = inner.fetch.map(|fetch| fetch.saturating_sub(outer.skip));
    RowInterval {
        skip: inner.skip + outer.skip,
        fetch: match (outer.fetch, left) {
            (Some(outer), Some(left)) => Some(outer.min(left)),
            (outer, left) => outer.or(left),
        },
    }
}
