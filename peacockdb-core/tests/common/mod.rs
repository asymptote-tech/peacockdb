//! The way in to the shared harness in `peacockdb_core::test_support` for the suites that
//! predate it, under the module names they were written against.
//!
//! Each integration-test crate includes this via `#[macro_use] mod common;`, and every
//! suite uses a subset, so an unused re-export is fine. The corpus binaries do not read it:
//! they reach `test_support` directly, and `corpus_cases.inc` beside this file is theirs.
#![allow(unused_imports)]

pub use peacockdb_core::test_support::{golden_dir_for, testdata_root};

pub mod memory_limit {
    pub use peacockdb_core::test_support::MemoryLimit;
}

pub mod golden_text {
    pub use peacockdb_core::test_support::{
        ordered_sections, parse_node_line, section_differences,
    };
}

pub mod result_text {
    pub use peacockdb_core::test_support::results_agree;
}

pub mod corpus {
    pub use peacockdb_core::test_support::{owed_rows, take_rows, wanted_rows, without_its_limit};
}

pub mod cost_model {
    pub use peacockdb_core::test_support::CostModel;
}
