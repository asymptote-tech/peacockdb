//! The budget a corpus case runs under, as a label rather than a number.
//!
//! Lives beside `mode.rs` until `test-layout.md` creates `src/test_support/` for the
//! harness that reads it. It was the only part of the crate's `config.rs` anything still
//! used; the rest — `TargetPartitions`, `TARGET_PARTITIONS`, `BATCH_STRESS_BUDGET` — was
//! named by nothing but that file's own unit test.

/// Resident-memory budget tier handed to `GpuMemoryBudgetRule`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryLimit {
    /// 100 MiB — sits in the gap between the corpus' top query (tpcds q78 ≈ 135.5
    /// MB) and the next one down, so the OOM tests get a real boundary to cross.
    Micro,
    /// 2 GiB.
    Mini,
    /// 12 GiB.
    Standard,
    /// 70 GiB.
    Full,
}

impl MemoryLimit {
    pub const fn bytes(self) -> usize {
        match self {
            MemoryLimit::Micro => 100 * 1024 * 1024,
            MemoryLimit::Mini => 2 * 1024 * 1024 * 1024,
            MemoryLimit::Standard => 12 * 1024 * 1024 * 1024,
            MemoryLimit::Full => 70 * 1024 * 1024 * 1024,
        }
    }

    /// Label component of a device string.
    pub const fn label(self) -> &'static str {
        match self {
            MemoryLimit::Micro => "micro",
            MemoryLimit::Mini => "mini",
            MemoryLimit::Standard => "standard",
            MemoryLimit::Full => "full",
        }
    }

    pub fn from_label(s: &str) -> Option<Self> {
        match s {
            "micro" => Some(MemoryLimit::Micro),
            "mini" => Some(MemoryLimit::Mini),
            "standard" => Some(MemoryLimit::Standard),
            "full" => Some(MemoryLimit::Full),
            _ => None,
        }
    }
}
