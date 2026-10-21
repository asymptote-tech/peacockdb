//! Tests whose subject is the source tree: the module layout rules nothing in rustc checks,
//! the CI wiring of every test target, a doc block a split left behind, and that no
//! peacockdb-core test reads the checkout.
//!
//! In cost-report because this crate is never staged to a remote host: a test that reads
//! `src/`, `tests/` or `.github/` cannot run where the checkout is not. `llm-wiki/coding-style.md`
//! has the layout rules, and `write_order.rs` carries one of the same kind that a task spec
//! states instead. The reason each needs a test rather than a reviewer is recorded at the test.

// A test target's child modules resolve against tests/ itself, and a file there would be
// another target. The path keeps them under a directory named for this one.
#[path = "test_source_formatting/checkout_reads.rs"]
mod checkout_reads;
#[path = "test_source_formatting/ci_coverage.rs"]
mod ci_coverage;
#[path = "test_source_formatting/near_miss.rs"]
mod near_miss;
#[path = "test_source_formatting/privacy.rs"]
mod privacy;
#[path = "test_source_formatting/runners.rs"]
mod runners;
#[path = "test_source_formatting/split_blocks.rs"]
mod split_blocks;
#[path = "test_source_formatting/test_code.rs"]
mod test_code;
#[path = "test_source_formatting/tree.rs"]
mod tree;
#[path = "test_source_formatting/visibility.rs"]
mod visibility;
#[path = "test_source_formatting/walls.rs"]
mod walls;
#[path = "test_source_formatting/write_order.rs"]
mod write_order;

use std::path::PathBuf;

/// The checkout this test reads.
pub(crate) fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

pub(crate) fn src_root() -> PathBuf {
    repo_root().join("peacockdb-core/src")
}
