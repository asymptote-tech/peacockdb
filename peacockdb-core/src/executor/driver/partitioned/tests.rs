//! What `driver/tests/` asks the driver that the driver does not otherwise answer.
//!
//! An inherent `impl` rather than a module of functions, and here rather than in
//! `driver/tests/`, because every one of these reads a private field of [`Driver`]: only
//! this module and its children can see them. The methods are `pub(crate)`, which an
//! inherent `impl` carries independently of the module it is written in, so `driver/tests/`
//! reaches them without `partitioned` exposing anything else.

use super::Driver;
use crate::executor::driver::StepError;
use crate::executor::{Backend, CallKind};

impl<B: Backend> Driver<'_, B> {
    pub(crate) fn hops(&self) -> (usize, usize) {
        self.acct.hops()
    }

    pub(crate) fn release_all(&mut self) -> Result<(), StepError> {
        self.release_in_flight()
    }

    pub(crate) fn queue_len(&self, node: usize, lane: usize) -> usize {
        self.states[node].out_queues[lane].len()
    }

    pub(crate) fn last_call(&self) -> Option<CallKind> {
        self.trace.last().map(|event| event.call)
    }
}
