//! What `cpu_backend/mod.rs`'s `has_finish_pass` asks a join: whether it keeps probe keys
//! and answers at done, rather than being one call and nothing else. That entry point
//! carries the question to `wire/tests.rs`, which holds the two readers of the rule
//! (`JoinCapability::answers_in_one_call`) to one answer. An inherent `impl` here because
//! `Calls::finish` is private to `join`, which only this module and its children see.

use super::CpuJoin;

impl CpuJoin {
    pub(crate) fn has_finish_pass(&self) -> bool {
        self.calls.finish.is_some()
    }
}
