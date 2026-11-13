//! What `driver/tests/` asks the driver that the driver does not otherwise answer.
//!
//! An inherent `impl` rather than a module of functions, and here rather than in
//! `driver/tests/`, because every one of these reads a private field of [`Driver`]: only
//! this module and its children can see them. The methods are `pub(crate)`, which an
//! inherent `impl` carries independently of the module it is written in, so `driver/tests/`
//! reaches them without `partitioned` exposing anything else.

use super::Driver;
use crate::executor::driver::StepError;
use crate::executor::{Backend, CallKind, OutputHook};

impl<'a, B: Backend> Driver<'a, B> {
    pub(crate) fn with_hook(mut self, hook: OutputHook<'a, B>) -> Self {
        self.hook = Some(hook);
        self
    }

    pub(crate) fn hops(&self) -> (usize, usize) {
        self.acct.hops()
    }

    pub(crate) fn release_all(&mut self) -> Result<(), StepError> {
        self.release_in_flight()
    }

    pub(crate) fn queue_len(&self, node: usize, lane: usize) -> usize {
        self.states[node].out_queues[lane].len()
    }

    pub(crate) fn node_count(&self) -> usize {
        self.index.len()
    }

    /// The most batches the node's queue on `lane` ever held at once.
    pub(crate) fn queue_high_water(&self, node: usize, lane: usize) -> usize {
        self.states[node].out_queues[lane].high_water()
    }

    /// The node the last step ran.
    pub(crate) fn last_node(&self) -> usize {
        self.trace
            .last()
            .map(|event| event.node as usize)
            .expect("a step ran")
    }

    pub(crate) fn in_flight(&self) -> usize {
        self.acct.in_flight()
    }

    /// How many batches a hold buffer's lane keeps.
    pub(crate) fn kept(&self, node: usize, lane: usize) -> usize {
        self.states[node].kept[lane].len()
    }

    pub(crate) fn is_satisfied(&self, node: usize) -> bool {
        self.scheduler.is_satisfied(node)
    }

    pub(crate) fn last_call(&self) -> Option<CallKind> {
        self.trace.last().map(|event| event.call)
    }
}
