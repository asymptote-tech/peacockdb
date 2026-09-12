use super::*;

fn body(group_by: Vec<u32>, grouping_sets: Vec<Vec<bool>>) -> AggregateBody {
    AggregateBody {
        group_by: group_by
            .into_iter()
            .map(|index| Expr::column(index, "k"))
            .collect(),
        grouping_sets,
        null_exprs: Vec::new(),
        aggs: Vec::new(),
        finalize: None,
    }
}

fn hashed_on(keys: Vec<u32>) -> PartitionLayout {
    PartitionLayout {
        n: 4,
        key_distribution: KeyDistribution::ByHash { hash_keys: keys },
        sort_order: SortOrder::NotSpecified,
        batch_layout: BatchLayout::MultipleBatches,
    }
}

#[test]
fn a_hash_on_a_regrouped_key_survives_at_its_new_ordinal() {
    let claim = regrouped_key_distribution(&hashed_on(vec![3]), &body(vec![7, 3], Vec::new()));
    assert_eq!(claim, KeyDistribution::ByHash { hash_keys: vec![1] });
}

#[test]
fn a_hash_on_a_key_a_grouping_set_drops_does_not_survive() {
    // The rollup's second set substitutes NULL for key 0, so the rows it produces are
    // no longer in the lane the hash on that column put them in — and a finalizing
    // merge that believed the claim would answer per lane for a group spread over all
    // of them.
    let sets = vec![vec![false, false], vec![true, false]];
    let claim = regrouped_key_distribution(&hashed_on(vec![7]), &body(vec![7, 3], sets));
    assert_eq!(claim, KeyDistribution::NotSpecified);
}

#[test]
fn a_mask_shorter_than_the_group_list_drops_the_hash_rather_than_keeping_it() {
    // Masks are as long as the group list by construction, so this is the default and
    // not a shape: a missing entry has to read as excluding, or a malformed body would
    // buy a co-location claim the rows do not have.
    let claim =
        regrouped_key_distribution(&hashed_on(vec![3]), &body(vec![7, 3], vec![vec![false]]));
    assert_eq!(claim, KeyDistribution::NotSpecified);
}

#[test]
fn a_grouping_set_that_drops_some_other_key_leaves_the_hash_alone() {
    let sets = vec![vec![false, false], vec![false, true]];
    let claim = regrouped_key_distribution(&hashed_on(vec![7]), &body(vec![7, 3], sets));
    assert_eq!(claim, KeyDistribution::ByHash { hash_keys: vec![0] });
}
