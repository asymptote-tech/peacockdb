//! Which kinds the harness has a case for, read off a registry the cases write — never off
//! source text, which is the reader-that-stops-at-the-first-match class — and checked
//! against the node registry in both directions. The oracle for "every kind" is
//! `every_kind`, which `a_node_rebuilt_over_its_own_children_is_the_node_it_was` holds to
//! all eighteen. `inventory` collects per binary, and the cases are gpu-rung, so the guard
//! sits here and runs where they link.

use std::collections::BTreeSet;

use crate::plan::node_name;
use crate::tests::rebuild::every_kind;

pub(crate) struct Covers {
    pub(crate) kind: &'static str,
    pub(crate) case: &'static str,
}

inventory::collect!(Covers);

/// A case's test, and its registry entry, from one declaration — so a case cannot exist
/// unregistered and an entry cannot name a test that is not there.
macro_rules! operator_case {
    ($kind:ident, fn $name:ident() $body:block) => {
        inventory::submit! {
            $crate::tests::gpu_tests::coverage::Covers {
                kind: stringify!($kind),
                case: stringify!($name),
            }
        }
        #[test]
        fn $name() $body
    };
}

/// The three forwarders have no executor and belong to the driver, which is tested
/// elsewhere. Permanent.
const EXCLUDED: &[&str] = &["GpuMergePartitions", "GpuUnion", "GpuInterleave"];

/// Kinds with no case yet. `operator-cases.md` empties this list and deletes it; until
/// then a kind that gains a case must leave it, or the reverse check goes red.
const PENDING: &[&str] = &[
    "GpuLoadParquet",
    "GpuCoalesceAllBatches",
    "GpuAccumulateBatchesAndSort",
    "GpuAggregate",
    "GpuAggregateBatches",
    "GpuHashJoin",
    "GpuCrossJoin",
    "GpuNestedLoopJoin",
    "GpuEmitPartitions",
    "GpuMergeSortedPartitions",
];

#[test]
fn every_kind_has_a_case_or_is_named_as_pending_or_excluded() {
    let fixtures = every_kind();
    let kinds: BTreeSet<&str> = fixtures
        .iter()
        .map(|node| node_name(node.as_any()))
        .collect();
    let covered: BTreeSet<&str> = inventory::iter::<Covers>
        .into_iter()
        .map(|c| c.kind)
        .collect();
    let missing: Vec<&&str> = kinds
        .iter()
        .filter(|k| !covered.contains(*k) && !PENDING.contains(k) && !EXCLUDED.contains(k))
        .collect();
    assert!(
        missing.is_empty(),
        "kinds with no case and not pending: {missing:?}"
    );
    let unknown: Vec<(&str, &str)> = inventory::iter::<Covers>
        .into_iter()
        .filter(|c| !kinds.contains(c.kind))
        .map(|c| (c.kind, c.case))
        .collect();
    assert!(
        unknown.is_empty(),
        "cases naming a kind that is not one, as (kind, case): {unknown:?}"
    );
    let stale: Vec<&&str> = PENDING
        .iter()
        .chain(EXCLUDED)
        .filter(|k| covered.contains(*k))
        .collect();
    assert!(
        stale.is_empty(),
        "listed as pending or excluded, but has a case: {stale:?}"
    );
    let not_kinds: Vec<&&str> = PENDING
        .iter()
        .chain(EXCLUDED)
        .filter(|k| !kinds.contains(*k))
        .collect();
    assert!(
        not_kinds.is_empty(),
        "listed, but not a kind: {not_kinds:?}"
    );
}
