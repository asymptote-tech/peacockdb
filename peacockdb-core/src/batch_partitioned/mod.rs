//! The engine: a lane holds a stream of batches rather than one resident table.
//!
//! This module is the vocabulary — plan nodes, the layout and schema they declare, and
//! the executor contracts the drivers call. The reasons behind each shape are in
//! `llm-wiki/architecture.md`.

pub mod aggregates;
pub mod cpu_backend;
pub mod error;
pub mod estimator;
pub mod expr;
pub mod expr_physical;
pub mod expr_translate;
pub mod layout;
pub mod node;
pub mod nodes;
pub mod nulls;
pub mod parquet_meta;
pub mod partitioner;
pub mod plan;
pub mod schema;
pub mod translate;
pub mod validate;

#[cfg(not(feature = "rust-only"))]
pub mod gpu_backend;

pub use error::PlanError;
pub use expr::{BinaryOp, ColumnRef, Expr, UnaryOp};
pub use layout::{BatchLayout, KeyDistribution, NodeKind, PartitionLayout, SortOrder};
pub use node::{GpuNode, RowInterval};
pub use nodes::{ExecutorCategory, category_of};
pub use partitioner::{Batching, RowGroupMeta};
pub use schema::Schema;

