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

use std::path::PathBuf;

use crate::test_support::BenchNode;

/// The `all` batch's measured runs after its warm-up: it is the longest, and 3 still give a
/// second minimum. The other batches measure `MEASURED_RUNS`.
pub const FUSED_ALL_RUNS: usize = 3;

/// `benchmark-results/<device>/fused/<dataset>.sf<sf>/tp1_single.fused.txt`: one section per
/// batch, `== first3` to `== all`. The extension keeps it out of the `.benchmark.txt` checks.
pub fn fused_results_file(device: &str, dataset: &str, sf: &str) -> PathBuf {
    fuser::fused_results_file(device, dataset, sf)
}

/// One batch's section of a `.fused.txt`, as `parse_fused_section` reads it. `batch`,
/// `pipeline` and `trailer` are `key=value` lines split at the first `=`; `holds` and `merge`
/// are whole lines, since their last value keeps its spaces. `nodes` is the tree in file order,
/// a node several roots read once; `roots` indexes it.
pub struct FusedSection<'a> {
    pub batch: Vec<(&'a str, &'a str)>,
    pub holds: Vec<&'a str>,
    pub merge: Vec<&'a str>,
    pub pipeline: Vec<(&'a str, &'a str)>,
    pub nodes: Vec<BenchNode<'a>>,
    pub roots: Vec<usize>,
    pub trailer: Vec<(&'a str, &'a str)>,
}

impl<'a> FusedSection<'a> {
    /// The value for `key` in the batch, pipeline or run lines. Panics where a key is held
    /// twice: the three blocks share one key space.
    pub fn field(&self, key: &str) -> Option<&'a str> {
        self.batch
            .iter()
            .chain(&self.pipeline)
            .chain(&self.trailer)
            .filter(|(name, _)| *name == key)
            .map(|(_, value)| *value)
            .reduce(|_, _| panic!("the fused section holds `{key}` twice"))
    }

    /// The field as a count; panics when it is present and not a number.
    pub fn count(&self, key: &str) -> Option<u64> {
        self.field(key).map(|value| {
            value
                .parse()
                .unwrap_or_else(|e| panic!("`{key}={value}` is not a count: {e}"))
        })
    }

    /// Every node's `total_us`, each node once however many roots read it: `device_us`.
    pub fn tree_device_us(&self) -> u64 {
        self.nodes.iter().map(|node| node.total_us).sum()
    }

    /// The `GpuLoadParquet` nodes' `total_us`, each once: `load_us`.
    pub fn tree_load_us(&self) -> u64 {
        self.nodes
            .iter()
            .filter(|node| node.line.name == "GpuLoadParquet")
            .map(|node| node.total_us)
            .sum()
    }
}

/// One fused section, or `None` for a marker or a failed batch's record.
pub fn parse_fused_section(body: &str) -> Option<FusedSection<'_>> {
    fuser::parse_fused_section(body)
}
