//! The pinned host file cache's Rust side that needs no FFI: the session option as a
//! DataFusion config extension, the values the benchmark prints, and the files a plan reads.
//! The cache itself is C++ (`cpp/src/pinned_host_cache.cpp`); `instrument.rs` reaches it.

use std::any::Any;
use std::collections::BTreeSet;
use std::fmt;
use std::path::PathBuf;

use datafusion::common::config::{ConfigEntry, ConfigExtension, ExtensionOptions};
use datafusion::error::{DataFusionError, Result};

use crate::executor::{
    PeacockOptions, PinnedCache, PinnedCacheSource, PinnedCacheState, PrefillError,
};
use crate::plan::{GpuLoadParquet, GpuNode};

#[cfg(test)]
mod tests;

#[cfg(all(test, not(feature = "rust-only")))]
mod ffi_tests;

#[cfg(all(test, feature = "gpu"))]
mod gpu_tests;

const CACHE_BYTES: &str = "pinned_host_cache_bytes";

impl ConfigExtension for PeacockOptions {
    const PREFIX: &'static str = "peacock";
}

// By hand rather than `extensions_options!`: the macro parses and prints a field through
// `FromStr` and `Display`, which `Option<u64>` has neither of, and unset is a value here.
impl ExtensionOptions for PeacockOptions {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn cloned(&self) -> Box<dyn ExtensionOptions> {
        Box::new(self.clone())
    }

    fn set(&mut self, key: &str, value: &str) -> Result<()> {
        if key != CACHE_BYTES {
            return Err(DataFusionError::Configuration(format!(
                "peacock.{key} is not an option; the one there is is peacock.{CACHE_BYTES}"
            )));
        }
        let bytes = value.trim().parse::<u64>().map_err(|e| {
            DataFusionError::Configuration(format!(
                "peacock.{CACHE_BYTES} takes a byte count, not {value:?}: {e}"
            ))
        })?;
        self.pinned_host_cache_bytes = Some(bytes);
        Ok(())
    }

    fn entries(&self) -> Vec<ConfigEntry> {
        vec![ConfigEntry {
            key: format!("{}.{CACHE_BYTES}", Self::PREFIX),
            value: self.pinned_host_cache_bytes.map(|bytes| bytes.to_string()),
            description: "Bytes of pinned host memory the process keeps parquet files in; \
                          0 is off, unset is three quarters of host RAM or the cgroup limit",
        }]
    }
}

impl fmt::Display for PinnedCacheSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            PinnedCacheSource::Default => "default",
            PinnedCacheSource::Session => "session",
            PinnedCacheSource::Env => "env",
        })
    }
}

impl fmt::Display for PinnedCache {
    /// GiB with one decimal, as `RmmPool`'s line prints them.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        const GIB: f64 = 1073741824.0;
        match self.state {
            PinnedCacheState::On => write!(
                f,
                "on cap={:.1}GiB resident={:.1}GiB files={} fills={} fill_us={} fallbacks={} \
                 evictions={} reloads={} source={}",
                self.capacity_bytes as f64 / GIB,
                self.resident_bytes as f64 / GIB,
                self.files,
                self.fills,
                self.fill_us,
                self.fallbacks,
                self.evictions,
                self.reloads,
                self.source,
            ),
            PinnedCacheState::Off => write!(f, "off source={}", self.source),
            PinnedCacheState::Integrated => f.write_str("off integrated"),
            PinnedCacheState::NoDevice => f.write_str("off no-device"),
        }
    }
}

impl fmt::Display for PrefillError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PrefillError::DoesNotFit { needed, limit } => {
                write!(f, "needs {needed} > limit {limit}")
            }
            PrefillError::Io { path, error } => write!(f, "{}: {error}", path.display()),
            PrefillError::Off => f.write_str("the pinned host cache is off"),
        }
    }
}

/// A load is found through `as_any` rather than the node registry, so a node standing in for
/// a shared load answers with the load it wraps.
pub(crate) fn plan_files(roots: &[&dyn GpuNode]) -> BTreeSet<PathBuf> {
    let mut files = BTreeSet::new();
    let mut pending: Vec<&dyn GpuNode> = roots.to_vec();
    while let Some(node) = pending.pop() {
        if let Some(load) = node.as_any().downcast_ref::<GpuLoadParquet>() {
            if !load.rows_only() {
                files.insert(PathBuf::from(&load.file));
            }
        }
        pending.extend(node.children());
    }
    files
}
