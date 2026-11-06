//! Prototypes: code that measures an idea, not code that ships. Each is a directory here whose
//! module doc says what it measures and the scale it runs at. `fuser` (query-fuser) fuses N
//! queries' tp1_single plans into one DAG and runs at sf40 only.
//!
//! Behind `test-support`, as `test_support` is, so `cargo build` cannot name it, and its API is
//! declared here under `test_support`'s rules. No engine module may depend on anything here: no
//! file outside `prototypes/` and the tests names `prototypes::`, which cost-report's
//! `test_source_formatting/prototypes.rs` checks. `llm-wiki/architecture.md`, "Prototypes".

// Reached by its own tests alone until the fused benchmark's entry points are declared here.
#[cfg_attr(not(test), allow(dead_code))]
mod fuser;
