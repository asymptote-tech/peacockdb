//! Joining the two halves of a measurement: what this side recorded per call, and what the
//! device reported per region.
//!
//! Neither half is a measurement alone: the driver knows WHERE a call was, the device
//! knows WHAT it cost. They meet on `(seq, call_index)`, a key both count to independently
//! and in the same order. What comes out is a [`Measured`] per call — named for the
//! measurement and not for time, because the output of a call in the middle of a node's
//! chain is reported here and nowhere else.

use std::collections::HashMap;

use crate::executor::{Measured, Measurements, Region, RunReport};
use crate::wire::Seq;


/// Cost every recorded call from the regions the device answered with.
///
/// Regions the join did not claim are returned beside the measurements rather than
/// dropped: a region nobody asked for means the two sides disagree about what ran, and
/// silently discarding it would hide exactly that.
pub(crate) fn join_regions(report: &RunReport, regions: &[Region]) -> (Measurements, Vec<Region>) {
    // One `Measured` per cuDF call, summed over that call's output partitions. Summed
    // because a call answering with several partitions charges its shared prologue to
    // partition 0, so any single region is a fraction of the call.
    let mut per_call: HashMap<(Seq, u64), Measured> = HashMap::new();
    for region in regions {
        *per_call.entry((region.seq, region.call_index)).or_default() += region.measured;
    }
    let mut claimed: HashMap<(Seq, u64), ()> = HashMap::new();
    let per_entry = report
        .abi_calls
        .iter()
        .map(|lanes| {
            lanes
                .iter()
                .map(|calls| {
                    calls
                        .iter()
                        .map(|made| {
                            let made = made.recorded()?;
                            let mut total = Measured::default();
                            for call in made {
                                let Some(found) = per_call.get(&(call.seq, call.call_index))
                                else {
                                    continue;
                                };
                                claimed.insert((call.seq, call.call_index), ());
                                total += *found;
                            }
                            Some(total)
                        })
                        .collect()
                })
                .collect()
        })
        .collect();
    let unclaimed = match claimed.len() == per_call.len() {
        true => Vec::new(),
        false => regions
            .iter()
            .filter(|region| !claimed.contains_key(&(region.seq, region.call_index)))
            .copied()
            .collect(),
    };
    (Measurements { per_entry, per_call }, unclaimed)
}

/// One node's whole cost, summed over every lane and every call it made. `None` where the
/// node was not measured at all, which is not the same as a node that cost nothing.
///
/// Three terms and no total, for the reason argued on [`Measured::host_us`].
pub(crate) fn node_measured(times: &Measurements, node: usize) -> Option<Measured> {
    let mut total: Option<Measured> = None;
    for lane in times.lanes(node) {
        for call in lane.iter().flatten() {
            *total.get_or_insert(Measured::default()) += *call;
        }
    }
    total
}
