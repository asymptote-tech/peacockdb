use super::*;

/// Row groups with the file indices they would have after pruning dropped groups 0
/// and 3 — the mapping carries file indices, never positions in the survivor list.
fn pruned(sizes: &[(u64, u64)]) -> Vec<RowGroupMeta> {
    let skipped = [0, 3];
    (0..)
        .filter(|i| !skipped.contains(i))
        .zip(sizes)
        .map(|(index, &(rows, bytes))| RowGroupMeta { index, rows, bytes })
        .collect()
}

fn uniform(count: usize, rows: u64) -> Vec<RowGroupMeta> {
    (0..count as u32)
        .map(|index| RowGroupMeta {
            index,
            rows,
            bytes: rows * 4,
        })
        .collect()
}

fn lane_rows(survivors: &[RowGroupMeta], mapping: &[Vec<Vec<u32>>]) -> Vec<u64> {
    mapping
        .iter()
        .map(|batches| {
            batches
                .iter()
                .flatten()
                .map(|g| survivors.iter().find(|m| m.index == *g).unwrap().rows)
                .sum()
        })
        .collect()
}

#[test]
fn fewer_survivors_than_lanes_leaves_lanes_empty() {
    let survivors = uniform(2, 100);
    let mapping = partition(&survivors, 4, Batching::Off).unwrap();
    assert_eq!(mapping, vec![vec![vec![0]], vec![vec![1]], vec![], vec![]]);
}

#[test]
fn three_lanes_take_contiguous_chunks_of_the_survivors() {
    let survivors = uniform(6, 100);
    let mapping = partition(&survivors, 3, Batching::Off).unwrap();
    assert_eq!(
        mapping,
        vec![vec![vec![0, 1]], vec![vec![2, 3]], vec![vec![4, 5]]]
    );
}

#[test]
fn batching_per_row_group_is_one_batch_per_group() {
    let survivors = pruned(&[(10, 40), (10, 500), (10, 30)]);
    let mapping = partition(&survivors, 1, Batching::PerRowGroup).unwrap();
    // The finest the mapping can express, and it needs no target to express it.
    assert_eq!(mapping, vec![vec![vec![1], vec![2], vec![4]]]);
}

#[test]
fn a_row_group_over_target_is_its_own_batch() {
    let survivors = pruned(&[(10, 40), (10, 500), (10, 30), (10, 30)]);
    let mapping = partition(
        &survivors,
        1,
        Batching::Sized {
            target_batch_bytes: 100,
        },
    )
    .unwrap();
    assert_eq!(mapping, vec![vec![vec![1], vec![2], vec![4, 5]]]);
}

#[test]
fn batching_off_is_one_batch_per_chunk() {
    let survivors = pruned(&[(10, 4_000), (10, 4_000), (10, 4_000), (10, 4_000)]);
    let mapping = partition(&survivors, 2, Batching::Off).unwrap();
    assert_eq!(mapping, vec![vec![vec![1, 2]], vec![vec![4, 5]]]);
    assert!(mapping.iter().all(|batches| batches.len() == 1));
}

#[test]
fn an_empty_survivor_set_is_an_error_rather_than_an_empty_map() {
    let err = partition(&[], 4, Batching::Off).unwrap_err();
    assert!(matches!(err, PlanError::Invalid(_)), "{err}");
    assert!(partition(&uniform(2, 10), 0, Batching::Off).is_err());
}

/// The bound holds for what a parquet writer emits: one row-group size per file and a
/// short last group. It is not universal — contiguity is the stronger rule, so row
/// groups differing by orders of magnitude inside one file can beat it.
#[test]
fn lane_rows_differ_by_at_most_one_row_group() {
    for groups in 1..=12usize {
        for tail in [1, 7_431, 122_880] {
            for lanes in 1..=groups {
                let mut survivors = uniform(groups, 122_880);
                survivors.last_mut().unwrap().rows = tail;
                let mapping = partition(&survivors, lanes, Batching::Off).unwrap();

                let rows = lane_rows(&survivors, &mapping);
                let spread = rows.iter().max().unwrap() - rows.iter().min().unwrap();
                let largest = survivors.iter().map(|g| g.rows).max().unwrap();
                assert!(
                    spread <= largest,
                    "{groups} groups (tail {tail}) over {lanes} lanes: \
                     spread {spread} exceeds one row group"
                );
            }
        }
    }
}

/// The mapping for a known input, pinned. A pure function called twice cannot show
/// determinism, so what this asserts is the mapping itself: a policy change moves it.
#[test]
fn the_mapping_for_a_known_input_is_pinned() {
    let survivors = pruned(&[(90, 900), (10, 100), (50, 500), (50, 500), (30, 300)]);
    let batching = Batching::Sized {
        target_batch_bytes: 600,
    };
    let expected = vec![vec![vec![1], vec![2]], vec![vec![4], vec![5], vec![6]]];
    assert_eq!(partition(&survivors, 2, batching).unwrap(), expected);
}
