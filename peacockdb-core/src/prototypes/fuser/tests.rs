//! The fuser's test choosers, which its unit tests and the sf1 answer checks pass in place of
//! the sf40 hold rule, and those answer checks.

mod end_to_end;

use super::{Choice, Fused, HeldReader, HoldDecision};

/// Every held reader reads its table again.
pub(crate) fn re_read_every_hold(_: &Fused, _: &HeldReader) -> HoldDecision {
    HoldDecision {
        choice: Choice::ReRead,
        bytes: 0,
        reread_us: 0,
        why: "test: re-read".to_string(),
    }
}

/// Every held reader is buffered on the lowest edge of its path.
pub(crate) fn buffer_every_hold_lowest(_: &Fused, held: &HeldReader) -> HoldDecision {
    HoldDecision {
        choice: Choice::Buffer { edge: held.path[0] },
        bytes: 0,
        reread_us: 0,
        why: "test: lowest edge".to_string(),
    }
}
