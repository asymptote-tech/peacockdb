//! The mergeability hint over hand-built fused plans and a baseline whose times are chosen
//! per node: which identical subtrees are entries, what each saves, and the batch's total.

use crate::plan::GpuNode;
use crate::prototypes::fuser::tests::{baseline, buffer_every_hold_lowest, re_read_every_hold};
use crate::prototypes::fuser::tests::{coalesce, join, lineitem, unload};
use crate::prototypes::fuser::tests::{count, filter, filter_above, fused_with, held_pair};
use crate::prototypes::fuser::{Baseline, MergeHint, Origin, merge_hint};

/// Filters take `filters[q]`, aggregates `counts[q]`, loads 100 and unloads 7.
fn times(filters: &[u64], counts: &[u64]) -> impl Fn(usize, &str) -> u64 {
    let (filters, counts) = (filters.to_vec(), counts.to_vec());
    move |query, name| match name {
        "GpuFilter" => filters[query],
        "GpuAggregate" => counts[query],
        "GpuLoadParquet" => 100,
        _ => 7,
    }
}

fn hint_of(trees: &[Box<dyn GpuNode>], baseline: &Baseline) -> MergeHint {
    let fused = fused_with(trees, &mut re_read_every_hold);
    merge_hint(&fused, baseline)
}

fn origin(query: usize, node: usize) -> Origin {
    Origin { query, node }
}

#[test]
fn identical_filters_over_one_source_are_one_entry() {
    let trees = vec![
        unload(filter(lineitem(&[0, 4], &[0]))),
        unload(filter(lineitem(&[0, 4], &[0]))),
    ];
    let hint = hint_of(&trees, &baseline(&trees, times(&[30, 50], &[])));
    assert_eq!(hint.entries.len(), 1);
    let entry = &hint.entries[0];
    assert!(entry.node.starts_with("GpuFilter"), "{}", entry.node);
    assert_eq!(entry.members, [origin(0, 1), origin(1, 1)]);
    assert_eq!((entry.saved_us, hint.saved_us), (30, 30));
}

#[test]
fn different_predicates_are_no_entry() {
    let trees = vec![
        unload(filter(lineitem(&[0, 4], &[0]))),
        unload(filter_above(lineitem(&[0, 4], &[0]), 7)),
    ];
    let hint = hint_of(&trees, &baseline(&trees, times(&[30, 50], &[])));
    assert!(hint.entries.is_empty());
    assert_eq!(hint.saved_us, 0);
}

#[test]
fn only_the_largest_identical_subtree_is_an_entry() {
    let trees = vec![
        unload(count(filter(lineitem(&[0, 4], &[0])))),
        unload(count(filter(lineitem(&[0, 4], &[0])))),
    ];
    let hint = hint_of(&trees, &baseline(&trees, times(&[30, 50], &[5, 9])));
    assert_eq!(hint.entries.len(), 1);
    let entry = &hint.entries[0];
    assert!(entry.node.starts_with("GpuAggregate"), "{}", entry.node);
    assert_eq!(entry.members, [origin(0, 1), origin(1, 1)]);
    assert_eq!(entry.saved_us, (30 + 5) + (50 + 9) - (50 + 9));
    assert_eq!(hint.saved_us, (30 + 50 - 50) + (5 + 9 - 9));
}

#[test]
fn a_nested_group_of_fewer_members_is_counted_once_in_the_total() {
    let trees = vec![
        unload(count(filter(lineitem(&[0, 4], &[0])))),
        unload(count(filter(lineitem(&[0, 4], &[0])))),
        unload(filter(lineitem(&[0, 4], &[0]))),
    ];
    let hint = hint_of(&trees, &baseline(&trees, times(&[10, 20, 40], &[3, 4, 0])));
    let saved: Vec<(&str, u64)> = hint
        .entries
        .iter()
        .map(|entry| (entry.node.split(':').next().unwrap(), entry.saved_us))
        .collect();
    assert_eq!(
        saved,
        [
            ("GpuFilter", 10 + 20 + 40 - 40),
            ("GpuAggregate", 13 + 24 - 24)
        ]
    );
    assert_eq!(hint.entries[0].members.len(), 3);
    assert_eq!(hint.saved_us, (10 + 20 + 40 - 40) + (3 + 4 - 4));
}

#[test]
fn a_buffer_does_not_split_a_group() {
    let trees = held_pair(&[0, 4], &[0, 4]);
    let fused = fused_with(&trees, &mut buffer_every_hold_lowest);
    assert_eq!(
        fused.holds.len(),
        1,
        "q0's filter reads the source through a buffer"
    );
    let hint = merge_hint(&fused, &baseline(&trees, times(&[30, 50], &[])));
    assert_eq!(hint.entries.len(), 1);
    assert!(hint.entries[0].node.starts_with("GpuFilter"));
    assert_eq!(hint.entries[0].saved_us, 30);
}

/// One query joining a filter over lineitem, coalesced, with the same filter: both scans read one shared
/// load, the probe's through a buffer, and the two filters are alike, but merging them is not
/// what the hint reports: only subtrees across queries are.
#[test]
fn a_query_repeating_a_subtree_with_itself_is_no_entry() {
    let trees = vec![unload(join(
        coalesce(filter(lineitem(&[0, 4], &[0]))),
        filter(lineitem(&[0, 4], &[0])),
    ))];
    let fused = fused_with(&trees, &mut buffer_every_hold_lowest);
    assert_eq!(fused.shared.len(), 1, "the two scans read one load");
    assert_eq!(
        fused.holds.len(),
        1,
        "the probe's filter reads it through a buffer"
    );
    let hint = merge_hint(&fused, &baseline(&trees, times(&[30], &[])));
    assert!(hint.entries.is_empty(), "{:?}", hint.entries);
    assert_eq!(hint.saved_us, 0);
}
