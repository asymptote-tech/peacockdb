//! The `--- run ---` trailer of a benchmark section: what the whole run cost, and under what
//! conditions.
//!
//! Named after the `--- recipes ---` and `--- memory ---` the plan goldens carry, so a reader
//! who has seen one file knows where a section ends in the other.

#[cfg(test)]
mod tests;

use super::provenance::fields;
use super::{BUILD, Provenance};
use crate::executor::{PinnedCache, PrefillStats};

/// The keys `run_section` writes, in order, the five provenance keys last. A resumed run keeps a
/// section only when its trailer is exactly these.
pub(crate) const TRAILER_KEYS: [&str; 17] = [
    "run_us",
    "device_us",
    "load_us",
    "execute_us",
    "peak_bytes",
    "runs",
    "build",
    "allocator",
    "pinned_cache",
    "prefill_files",
    "prefill_bytes",
    "prefill_us",
    "device",
    "driver",
    "cuda",
    "cudf",
    "host",
];

/// What the chosen run cost, as the trailer reports it.
pub(crate) struct RunTotals {
    pub(crate) run_us: u64,
    pub(crate) device_us: u64,
    /// The tree's `GpuLoadParquet` totals, a part of `device_us`.
    pub(crate) load_us: u64,
    pub(crate) peak_bytes: u64,
}

/// The trailer, ending with the run's provenance, which a resumed run keeps a section by.
///
/// **Two totals, deliberately named apart.** `run_us` is the whole execution end to end;
/// `device_us` is the sum of the tree's `total_us`, and the gap is the host: the driver's
/// scheduling and the prologue between calls. `execute_us` is `device_us` less `load_us`.
/// `peak_bytes` is the accountant's peak, a lower bound on device memory: a call's scratch,
/// cuDF's temporaries, pool fragmentation and the reader's staging are outside it. `prefill_*`
/// is host IO, kept apart from `load_us`. build-test.md, "The tree", defines every key.
pub(crate) fn run_section(
    totals: &RunTotals,
    spread: &[u64],
    allocator: &str,
    end: &PinnedCache,
    prefill: &PrefillStats,
    provenance: &Provenance,
) -> String {
    let spread: Vec<String> = spread.iter().map(u64::to_string).collect();
    let mut trailer = format!(
        "--- run ---\n\
         run_us={}\n\
         device_us={}\n\
         load_us={}\n\
         execute_us={}\n\
         peak_bytes={}\n\
         runs=[{}]\n\
         build={BUILD}\n\
         allocator={allocator}\n\
         pinned_cache={end}\n\
         prefill_files={}\n\
         prefill_bytes={}\n\
         prefill_us={}\n",
        totals.run_us,
        totals.device_us,
        totals.load_us,
        totals.device_us - totals.load_us,
        totals.peak_bytes,
        spread.join(","),
        prefill.files,
        prefill.bytes,
        prefill.us,
    );
    for (key, value) in fields(provenance) {
        trailer.push_str(&format!("{key}={value}\n"));
    }
    trailer
}
