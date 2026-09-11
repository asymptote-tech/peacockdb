//! The test code that sits above every component rather than inside one.
//!
//! The injector and the rebuilder take a plan tree apart and put it back together,
//! constructing plan nodes, wire recipes and backend executors alike, so neither belongs
//! to `plan` or to `executor`. The end-to-end tier is here for the same reason: it starts
//! at a query's text and ends at its rows, so it needs every component at once.
//!
//! `lib.rs` gates this whole subtree on `#[cfg(test)]`, so nothing below carries an
//! attribute of its own and production code that names anything here is `E0433` in a
//! release build.

mod end_to_end;
pub(crate) mod injection;
pub(crate) mod rebuild;
