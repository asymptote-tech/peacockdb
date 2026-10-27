//! The `--- run ---` trailer of a benchmark section: what the whole run cost, and under what
//! conditions.
//!
//! Named after the `--- recipes ---` and `--- memory ---` the plan goldens carry, so a reader
//! who has seen one file knows where a section ends in the other.

#[cfg(test)]
mod tests;

use super::provenance::fields;
use super::{BUILD, Provenance};

/// The trailer, ending with the run's provenance.
///
/// **Two totals, deliberately named apart.** `run_us` is the whole execution end to end;
/// `device_us` is the sum of the tree's `total_us`. They do not agree, and the difference
/// is the point: it is what the run spent outside the calls — the driver's own scheduling
/// and the host prologue between them. One name for both would read as a discrepancy.
pub(crate) fn run_section(
    run_us: u64,
    device_us: u64,
    spread: &[u64],
    allocator: &str,
    provenance: &Provenance,
) -> String {
    let spread: Vec<String> = spread.iter().map(u64::to_string).collect();
    let mut trailer = format!(
        "--- run ---\n\
         run_us={run_us}\n\
         device_us={device_us}\n\
         runs=[{}]\n\
         build={BUILD}\n\
         allocator={allocator}\n",
        spread.join(","),
    );
    for (key, value) in fields(provenance) {
        trailer.push_str(&format!("{key}={value}\n"));
    }
    trailer
}
