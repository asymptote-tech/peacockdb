use super::*;

#[test]
fn the_grouping_id_leaves_and_the_user_keys_stay() {
    assert_eq!(drop_grouping_id(vec![0, 1, 2], 2), Ok(vec![0, 1]));
}

#[test]
fn a_shuffle_on_the_grouping_id_alone_is_refused() {
    let err = drop_grouping_id(vec![2], 2).expect_err("no user key is left to hash");
    assert!(
        matches!(&err, PlanError::Invalid(why) if why.contains("one must remain")),
        "{err}"
    );
}

#[test]
fn a_key_above_the_grouping_id_is_refused() {
    // Nothing above the id is a user key, so a key there means the shuffle is reading the
    // init's state columns and the drop cannot be argued from the subset rule.
    let err = drop_grouping_id(vec![0, 3], 2).expect_err("3 is not a user key");
    assert!(
        matches!(&err, PlanError::Invalid(why) if why.contains("below it")),
        "{err}"
    );
}
