//! The five planning modes, and the knobs a run at one of them takes.
//!
//! One table, because four tiers plan the same five shapes: the plan goldens, the end-to-end
//! tier and the two corpus binaries. A second copy checked against this one is not the same
//! thing — an agreement test is opt-in per copy, so the next tier that spells the modes out
//! needs someone to remember to write one, and nothing reddens if they do not. There is
//! nothing to copy from instead.

use peacockdb_core::batch_partitioned::plan::{BatchSizing, PlanKnobs};
use peacockdb_core::config::MemoryLimit;

/// The tier every mode is planned at. The plan goldens are written here, so a failure
/// anywhere reads against a committed plan rather than a shape nothing records. The
/// execution goldens carry its label in their names, so the budget and the filename cannot
/// name different tiers.
pub const TIER: MemoryLimit = MemoryLimit::Mini;
pub const BUDGET: u64 = TIER.bytes() as u64;

pub use peacockdb_core::batch_partitioned::plan::SMALL_TABLE_BYTES;

/// One planning mode: what the goldens call it, and the two knobs that make it distinct.
pub struct Mode {
    /// The golden's spelling, `tp4-sized`.
    pub name: &'static str,
    pub target_partitions: usize,
    pub sizing: BatchSizing,
}

impl Mode {
    /// The macro's spelling of the same mode, `tp4_sized` — one derivation rather than
    /// a second field, so the two cannot disagree.
    pub fn ident(&self) -> String {
        self.name.replace('-', "_")
    }

    pub fn knobs(&self) -> PlanKnobs {
        PlanKnobs {
            target_partitions: self.target_partitions,
            sizing: self.sizing,
            budget: BUDGET,
            small_table_bytes: SMALL_TABLE_BYTES,
        }
    }
}

/// The five, in the fixed sequence the widget and the `.result.txt` authority both read:
/// the last enabled one wins in each. One lane and one batch is the degenerate end,
/// row-group granularity is the finest the mapping expresses, and the sized mode is the
/// only one a budget moves.
pub const MODES: [Mode; 5] = [
    Mode {
        name: "tp1-single",
        target_partitions: 1,
        sizing: BatchSizing::OneBatchPerLane,
    },
    Mode {
        name: "tp1-rowgroup",
        target_partitions: 1,
        sizing: BatchSizing::OneBatchPerRowGroup,
    },
    Mode {
        name: "tp4-single",
        target_partitions: 4,
        sizing: BatchSizing::OneBatchPerLane,
    },
    Mode {
        name: "tp4-rowgroup",
        target_partitions: 4,
        sizing: BatchSizing::OneBatchPerRowGroup,
    },
    Mode {
        name: "tp4-sized",
        target_partitions: 4,
        sizing: BatchSizing::Budgeted,
    },
];

/// The mode a macro's ident names. Exhaustive over the table: an unlisted ident panics
/// naming the set, rather than being routed to whichever mode a prefix reached first.
pub fn mode_named(ident: &str) -> &'static Mode {
    MODES
        .iter()
        .find(|mode| mode.ident() == ident)
        .unwrap_or_else(|| {
            let known: Vec<String> = MODES.iter().map(Mode::ident).collect();
            panic!("unknown mode '{ident}' (expected one of {known:?})")
        })
}
