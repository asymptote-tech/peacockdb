//! What `cpu_backend/tests/accumulate.rs` asks the accumulator that it does not otherwise
//! answer: the compaction count, which is what the doubling threshold is asserted on. An
//! inherent `impl` here rather than a function in `cpu_backend/tests/`, because the counter
//! is a private field of [`AggregateBatches`] and only this module and its children see it;
//! the method is `pub(crate)`, which it carries independently of where the `impl` is written.

use super::AggregateBatches;

impl AggregateBatches {
    pub(crate) fn compactions(&self) -> usize {
        self.compactions
    }
}
