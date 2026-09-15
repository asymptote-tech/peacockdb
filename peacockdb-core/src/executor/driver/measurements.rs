//! Joining the two halves of a measurement: what this side journalled per call, and what
//! the device reported per region.
//!
//! Neither half is a measurement alone: the driver knows where a call was, the device
//! knows what it cost. They meet on `(seq, call_index)`, which both count to in the same
//! order because it is the driver that makes every call C++ sees.
//!
//! That agreement is not provable here, so it is checked: a call no region answered and a
//! region no call claimed are both refusals, since every call opens one and the
//! alternative is a record pricing a node at nothing because nobody measured it.

use std::collections::HashMap;

use crate::executor::{Measured, Measurements, Region, RunReport};
use crate::wire::Seq;

pub(crate) fn join_regions(report: &RunReport, regions: &[Region]) -> Result<Measurements, String> {
    // One entry per cuDF call, summed over that call's output partitions: a call answering
    // with several spreads itself across them, so any single region is a fraction of it.
    let mut per_region: HashMap<(Seq, u64), Measured> = HashMap::new();
    for region in regions {
        *per_region
            .entry((region.seq, region.call_index))
            .or_default() += Measured {
            host_us: region.host_us,
            device_us: region.device_us,
            regions: 1,
            ..Measured::default()
        };
    }
    let mut per_call: HashMap<(Seq, u64), Measured> = HashMap::new();
    let mut per_entry = Vec::with_capacity(report.abi_calls.len());
    for lanes in &report.abi_calls {
        let mut by_lane = Vec::with_capacity(lanes.len());
        for calls in lanes {
            let mut entries = Vec::with_capacity(calls.len());
            for made in calls {
                entries.push(match made.recorded() {
                    None => None,
                    Some(made) => {
                        let mut total = Measured::default();
                        for call in made {
                            let key = (call.seq, call.call_index);
                            let found = per_region
                                .remove(&key)
                                .ok_or_else(|| unanswered(call.seq, call.call_index))?;
                            let measured = Measured {
                                out_rows: call.out_rows,
                                out_bytes: call.out_bytes,
                                ..found
                            };
                            per_call.insert(key, measured);
                            total += measured;
                        }
                        Some(total)
                    }
                });
            }
            by_lane.push(entries);
        }
        per_entry.push(by_lane);
    }
    // What is left is what nothing claimed. Taken out above rather than counted, so a
    // second call on one key would be caught as an unanswered call rather than double-billed.
    if let Some((seq, call_index)) = per_region.keys().min().copied() {
        return Err(format!(
            "the device answered for (seq {seq}, call {call_index}) and no call journalled it \
             — {} such region group(s)",
            per_region.len()
        ));
    }
    Ok(Measurements {
        per_entry,
        per_call,
    })
}

fn unanswered(seq: Seq, call_index: u64) -> String {
    format!(
        "(seq {seq}, call {call_index}) was journalled and the device answered no region for \
         it — a measured run opens one per call"
    )
}

/// One node's whole cost, summed over every lane and every call it made. `None` where the
/// node was not measured at all, which is not the same as a node that cost nothing.
pub(crate) fn node_measured(times: &Measurements, node: usize) -> Option<Measured> {
    let mut total: Option<Measured> = None;
    for lane in times.lanes(node) {
        for call in lane.iter().flatten() {
            *total.get_or_insert(Measured::default()) += *call;
        }
    }
    total
}
