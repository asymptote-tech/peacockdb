//! A producer's output queue for one lane, read by every edge into the producer.
//!
//! Each reader has a cursor, and an entry stays until the last live reader has taken it. A
//! reader that is not the last takes a [`Batch::share`] holding a clone of the entry's charge;
//! the last takes the original and the queue's own holder. So a batch is charged once however
//! many read it, and on a tree, one reader per queue, nothing is ever shared. Readers take in
//! order, so the last taker of an entry always takes it from the front.

use std::collections::VecDeque;

use super::accounting::Held;
use crate::executor::{BackendError, Batch};

pub(crate) struct OutQueue<T> {
    entries: VecDeque<Held<T>>,
    /// The absolute index of `entries[0]`: how many entries have left the front.
    front: usize,
    /// Per reader, the absolute index of the next entry it takes.
    next: Vec<usize>,
    retired: Vec<bool>,
    high_water: usize,
}

impl<T: Batch> OutQueue<T> {
    pub(crate) fn new(readers: usize) -> Self {
        Self {
            entries: VecDeque::new(),
            front: 0,
            next: vec![0; readers],
            retired: vec![false; readers],
            high_water: 0,
        }
    }

    pub(crate) fn push(&mut self, held: Held<T>) {
        self.entries.push_back(held);
        self.high_water = self.high_water.max(self.entries.len());
    }

    pub(crate) fn has(&self, reader: usize) -> bool {
        self.position(reader).is_some()
    }

    pub(crate) fn front_rows(&self, reader: usize) -> Option<u64> {
        self.position(reader).map(|at| self.entries[at].rows())
    }

    /// The reader's next batch: a share while another live reader has yet to take it, else
    /// the original.
    pub(crate) fn take(&mut self, reader: usize) -> Result<Option<Held<T>>, BackendError> {
        let Some(at) = self.position(reader) else {
            return Ok(None);
        };
        let index = self.next[reader];
        self.next[reader] += 1;
        if self.is_wanted(index) {
            let entry = &self.entries[at];
            return Ok(Some(Held {
                batch: entry.batch.share()?,
                charge: entry.charge.clone(),
            }));
        }
        debug_assert_eq!(at, 0, "the last taker of an entry takes the front");
        self.front += 1;
        Ok(self.entries.pop_front())
    }

    /// The reader stops reading. What only it still wanted comes back, the queue's holder
    /// with it; an entry another live reader wants stays.
    // Its caller is a satisfied limit's reader, which the driver does not retire yet.
    #[allow(dead_code)]
    pub(crate) fn retire(&mut self, reader: usize) -> Vec<Held<T>> {
        self.retired[reader] = true;
        let mut released = Vec::new();
        while !self.entries.is_empty() && !self.is_wanted(self.front) {
            released.extend(self.entries.pop_front());
            self.front += 1;
        }
        released
    }

    /// Every entry once, with how many live readers never took it.
    pub(crate) fn drain(&mut self) -> Vec<(Held<T>, usize)> {
        let mut drained = Vec::new();
        while let Some(held) = self.entries.pop_front() {
            drained.push((held, self.wanting(self.front)));
            self.front += 1;
        }
        drained
    }

    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The most entries ever queued at once.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn high_water(&self) -> usize {
        self.high_water
    }

    /// A retired reader's cursor may lag the front, so it is never subtracted from.
    fn position(&self, reader: usize) -> Option<usize> {
        if self.retired[reader] {
            return None;
        }
        let at = self.next[reader] - self.front;
        (at < self.entries.len()).then_some(at)
    }

    /// Whether a live reader has yet to take the entry at absolute `index`.
    fn is_wanted(&self, index: usize) -> bool {
        self.wanting(index) > 0
    }

    fn wanting(&self, index: usize) -> usize {
        (0..self.next.len())
            .filter(|reader| !self.retired[*reader] && self.next[*reader] <= index)
            .count()
    }
}

#[cfg(test)]
mod tests;
