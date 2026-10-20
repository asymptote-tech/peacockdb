//! The sequence's helpers on their own: the grouping-id drop, and `decompose`'s refusal no
//! SQL reaches.

use datafusion::functions_aggregate::count::count_udaf;
use datafusion::physical_expr::aggregate::AggregateExprBuilder;
use datafusion::physical_expr::expressions::col;

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

/// No SQL reaches it: the classifier lowers or refuses every DISTINCT it is shown. This is
/// the net for a shape it is not shown, which would otherwise run as non-distinct on both
/// engines, where the cpu-vs-device comparison cannot see it.
#[test]
fn a_distinct_aggregate_reaching_decompose_is_refused() {
    let schema = Arc::new(ArrowSchema::new(vec![Field::new(
        "v",
        DataType::Int64,
        true,
    )]));
    let aggregate = AggregateExprBuilder::new(count_udaf(), vec![col("v", &schema).unwrap()])
        .schema(schema.clone())
        .alias("count(DISTINCT v)")
        .distinct()
        .build()
        .expect("a distinct count");
    let err = decompose(&[(Arc::new(aggregate), InitFrom::Values)], &schema, 0)
        .err()
        .expect("refused");
    assert!(
        matches!(&err, PlanError::Unsupported(what)
            if what.contains("in a shape the lowering does not handle")),
        "{err}"
    );
}
