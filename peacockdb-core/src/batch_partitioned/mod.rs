//! The engine: a lane holds a stream of batches rather than one resident table.
//!
//! This module is the vocabulary — plan nodes, the layout and schema they declare, and
//! the executor contracts the drivers call. The reasons behind each shape are in
//! `llm-wiki/architecture.md`.

pub mod cpu_backend;
pub mod estimator;
pub mod expr_physical;
pub mod expr_translate;
pub mod nulls;
pub mod parquet_meta;
pub mod partitioner;
pub mod plan;
pub mod translate;

#[cfg(not(feature = "rust-only"))]
pub mod gpu_backend;


