//! The engine: a lane holds a stream of batches rather than one resident table.
//!
//! This module is the vocabulary — plan nodes, the layout and schema they declare, and
//! the executor contracts the drivers call. The reasons behind each shape are in
//! `llm-wiki/architecture.md`.

pub mod cpu_backend;
pub mod expr_physical;

#[cfg(not(feature = "rust-only"))]
pub mod gpu_backend;


