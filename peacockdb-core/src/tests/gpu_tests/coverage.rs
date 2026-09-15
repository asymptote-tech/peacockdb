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

/// Every kind but a forwarder has a case, every case names a kind, and the exclusions are
/// kinds with no case — so a new node kind, a misspelt one, or a forwarder that gains an
/// executor each turns this red.
#[test]
fn every_kind_has_a_case_or_is_a_forwarder() {
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
        .filter(|k| !covered.contains(*k) && !EXCLUDED.contains(k))
        .collect();
    assert!(missing.is_empty(), "kinds with no case: {missing:?}");
    let unknown: Vec<(&str, &str)> = inventory::iter::<Covers>
        .into_iter()
        .filter(|c| !kinds.contains(c.kind))
        .map(|c| (c.kind, c.case))
        .collect();
    assert!(
        unknown.is_empty(),
        "cases naming a kind that is not one, as (kind, case): {unknown:?}"
    );
    let stale: Vec<&&str> = EXCLUDED.iter().filter(|k| covered.contains(*k)).collect();
    assert!(stale.is_empty(), "excluded, but has a case: {stale:?}");
    let not_kinds: Vec<&&str> = EXCLUDED.iter().filter(|k| !kinds.contains(*k)).collect();
    assert!(
        not_kinds.is_empty(),
        "excluded, but not a kind: {not_kinds:?}"
    );
}
