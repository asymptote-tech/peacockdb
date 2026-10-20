//! Every join shape pbench shows at the query level, at node level too, so a failure is found
//! at the lowest layer first. `PBENCH_SHAPES` names, per pbench query that plans a join, the
//! cases that hold its shapes; the new shapes are here, the rest in `join_cases.rs`,
//! `join_dimension_cases.rs`, `join_key_type_cases.rs` and `nested_cases.rs`, over the same
//! prefixed synthetic sides. Hand counts read the generator's strides: `key` is NULL on every
//! eleventh row, `b` on every twenty-third.

use std::collections::{BTreeSet, HashSet};
use std::sync::Arc;

use datafusion::arrow::array::{Array, AsArray, RecordBatch};
use datafusion::arrow::compute::concat_batches;
use datafusion::arrow::datatypes::{Field, Int32Type, Int64Type, Schema as ArrowSchema};
use datafusion::common::JoinType;

use super::coverage::Covers;
use super::join_cases::{
    build_batch, hash_join, hash_join_with, one_probe, probe_batch, script, two_probes,
};
use super::script::{Outcome, Script, run_both};
use crate::plan::{Expr, JoinFilterColumn, JoinSide, UnaryOp};
use crate::test_support::{golden_dir_for, ordered_sections, queries_dir_for};
use crate::tests::compare::Order;

/// Every row the device answered, as one batch, once `same` has held the cpu's to it.
fn answered(outcome: &Outcome) -> RecordBatch {
    outcome.same(Order::Any);
    let batches: Vec<RecordBatch> = outcome
        .gpu
        .as_ref()
        .expect("the device answers")
        .iter()
        .flatten()
        .cloned()
        .collect();
    // An export declares a column non-nullable when its batch holds no NULL, so the calls'
    // batches differ there; the rows are what is read.
    let fields: Vec<Field> = batches[0]
        .schema()
        .fields()
        .iter()
        .map(|f| f.as_ref().clone().with_nullable(true))
        .collect();
    let schema = Arc::new(ArrowSchema::new(fields));
    let batches: Vec<RecordBatch> = batches
        .into_iter()
        .map(|b| RecordBatch::try_new(schema.clone(), b.columns().to_vec()).expect("same columns"))
        .collect();
    concat_batches(&schema, &batches).expect("one schema")
}

/// How many rows have a NULL in every one of `null` and none in any of `set`.
fn rows_where(batch: &RecordBatch, null: &[&str], set: &[&str]) -> usize {
    let column = |name: &&str| {
        batch
            .column_by_name(name)
            .expect("a declared column")
            .clone()
    };
    let (null, set): (Vec<_>, Vec<_>) = (
        null.iter().map(column).collect(),
        set.iter().map(column).collect(),
    );
    (0..batch.num_rows())
        .filter(|&row| null.iter().all(|c| c.is_null(row)) && set.iter().all(|c| c.is_valid(row)))
        .count()
}

// NULL keys on both sides, Left and Full, under the SQL default and under `null_equals_null`.
// The build's 32 rows hold two NULL keys (rows 10, 21), the probe's 48 four.

operator_case! {
    GpuHashJoin,
    fn a_left_join_pads_a_null_key_build_row_under_the_sql_default() {
        let out = answered(&run_both(&hash_join(JoinType::Left, false, false, None), one_probe()));
        assert_eq!(rows_where(&out, &["b_key"], &[]), 2, "each NULL-key build row once");
        assert_eq!(rows_where(&out, &["b_key", "p_id"], &[]), 2, "and padded");
    }
}

operator_case! {
    GpuHashJoin,
    fn a_left_join_under_null_equals_null_matches_null_keys() {
        let out = answered(&run_both(&hash_join(JoinType::Left, true, false, None), one_probe()));
        assert_eq!(rows_where(&out, &["b_key", "p_key"], &["p_id"]), 2 * 4);
        assert_eq!(rows_where(&out, &["b_key", "p_id"], &[]), 0, "none padded");
    }
}

operator_case! {
    GpuHashJoin,
    fn a_full_join_pads_null_key_rows_on_both_sides_under_the_sql_default() {
        let out = answered(&run_both(&hash_join(JoinType::Full, false, false, None), one_probe()));
        assert_eq!(rows_where(&out, &["b_key", "p_id"], &["b_id"]), 2, "the build's, padded");
        assert_eq!(rows_where(&out, &["p_key", "b_id"], &["p_id"]), 4, "the probe's, padded");
        assert_eq!(rows_where(&out, &["b_key", "p_key"], &["b_id", "p_id"]), 0, "never paired");
    }
}

operator_case! {
    GpuHashJoin,
    fn a_full_join_under_null_equals_null_matches_null_keys() {
        let out = answered(&run_both(&hash_join(JoinType::Full, true, false, None), one_probe()));
        assert_eq!(rows_where(&out, &["b_key", "p_key"], &["b_id", "p_id"]), 2 * 4);
        assert_eq!(rows_where(&out, &["b_key", "p_id"], &[]), 0, "no build row padded");
        assert_eq!(rows_where(&out, &["p_key", "b_id"], &[]), 0, "no probe row padded");
    }
}

// The semi family over keys repeated on both sides: a build row is answered once however
// many probe rows it meets.
operator_case! {
    GpuHashJoin,
    fn a_left_semi_join_over_many_to_many_keys_emits_each_build_row_once() {
        let keys = |batch: &RecordBatch, name: &str| -> Vec<Option<i32>> {
            let column = batch.column_by_name(name).expect("a key").as_primitive::<Int32Type>();
            (0..column.len()).map(|row| column.is_valid(row).then(|| column.value(row))).collect()
        };
        let (build, probe) = (keys(&build_batch(32), "b_key"), keys(&probe_batch(48, 21), "p_key"));
        let probed: HashSet<i32> = probe.iter().flatten().copied().collect();
        let repeated = |keys: &[Option<i32>]| keys.iter().flatten().count() > keys.iter().flatten().collect::<HashSet<_>>().len();
        assert!(repeated(&build) && repeated(&probe), "keys repeat on both sides");
        let wanted = build.iter().flatten().filter(|key| probed.contains(key)).count();

        let out = answered(&run_both(&hash_join(JoinType::LeftSemi, false, false, None), one_probe()));
        let ids = out.column_by_name("b_id").expect("the build's id").as_primitive::<Int64Type>();
        let distinct: BTreeSet<i64> = ids.values().iter().copied().collect();
        assert_eq!((out.num_rows(), distinct.len()), (wanted, wanted));
    }
}

/// `NOT EXISTS (… WHERE f_k = d_k AND d_kb)`: the condition is the build's own boolean, so a
/// build row whose `b` is NULL meets its keys and is still not matched.
fn preserved_condition(join_type: JoinType) -> crate::plan::GpuHashJoin {
    let condition = vec![JoinFilterColumn {
        side: JoinSide::Build,
        index: 7,
    }];
    hash_join_with(
        join_type,
        false,
        (Some(Expr::column(0, "b_b")), condition),
        None,
    )
}

/// Build row 22: `b` NULL, `key` set, and a probe row holding that key.
const NULL_CONDITION_ROW: usize = 22;

fn null_condition_row_meets_a_probe_key() {
    let build = build_batch(32);
    let row = NULL_CONDITION_ROW;
    let column = |name: &str| build.column_by_name(name).expect("a build column").clone();
    assert!(column("b_b").is_null(row) && column("b_key").is_valid(row));
    let key = column("b_key").as_primitive::<Int32Type>().value(row);
    let probe = probe_batch(48, 21);
    let probe_keys = probe
        .column_by_name("p_key")
        .expect("the probe's key")
        .as_primitive::<Int32Type>();
    assert!(
        probe_keys.iter().any(|k| k == Some(key)),
        "the row's key is probed"
    );
}

/// Where the output row for build row `id` is, if it is there.
fn row_of(out: &RecordBatch, id: usize) -> Option<usize> {
    let ids = out
        .column_by_name("b_id")
        .expect("the build's id")
        .as_primitive::<Int64Type>();
    ids.values().iter().position(|&i| i == id as i64)
}

operator_case! {
    GpuHashJoin,
    fn a_left_anti_join_keeps_a_row_whose_preserved_condition_is_null() {
        null_condition_row_meets_a_probe_key();
        let out = answered(&run_both(&preserved_condition(JoinType::LeftAnti), one_probe()));
        assert!(row_of(&out, NULL_CONDITION_ROW).is_some(), "kept");
    }
}

operator_case! {
    GpuHashJoin,
    fn a_left_mark_join_marks_a_row_whose_preserved_condition_is_null_false() {
        null_condition_row_meets_a_probe_key();
        let out = answered(&run_both(&preserved_condition(JoinType::LeftMark), one_probe()));
        let at = row_of(&out, NULL_CONDITION_ROW).expect("a mark join answers every build row");
        let mark = out.column_by_name("mark").expect("the mark").as_boolean();
        assert!(mark.is_valid(at) && !mark.value(at), "marked false, not NULL");
    }
}

/// `x IS NULL` over one side's column: the `NOT IN` rewrite's conjunct, which the session
/// takes as a per-side condition rather than a cross one.
fn is_null_on(side: JoinSide, name: &str) -> (Option<Expr>, Vec<JoinFilterColumn>) {
    (
        Some(Expr::unary(UnaryOp::IsNull, Expr::column(0, name))),
        vec![JoinFilterColumn { side, index: 2 }],
    )
}

// `not-in-correlated`'s RightAnti over `f_k IS NULL` on the probe, `not-in-under-or`'s
// LeftMark over it on the build.

operator_case! {
    GpuHashJoin,
    fn a_right_anti_join_with_a_probe_side_is_null_residual_agrees() {
        let node = hash_join_with(JoinType::RightAnti, false, is_null_on(JoinSide::Probe, "p_i32"), None);
        run_both(&node, two_probes()).same(Order::Any);
    }
}

operator_case! {
    GpuHashJoin,
    fn a_left_mark_join_with_a_build_side_is_null_residual_agrees() {
        let node = hash_join_with(JoinType::LeftMark, false, is_null_on(JoinSide::Build, "b_i32"), None);
        run_both(&node, two_probes()).same(Order::Any);
    }
}

// A lane whose probe side sent nothing (pbench's `sparse-probe-*` at tp4): the four types
// that answer only per probe row owe nothing at the finish.

fn no_probe() -> Script {
    script(Some(build_batch(32)), Vec::new())
}

fn finishes_with_nothing(join_type: JoinType) {
    let outcome = run_both(&hash_join(join_type, false, false, None), no_probe());
    outcome.same(Order::Any);
    let slots = outcome.gpu.as_ref().expect("the device answers");
    assert!(
        slots.len() == 1 && slots[0].is_empty(),
        "one finish, no batch"
    );
}

operator_case! {
    GpuHashJoin,
    fn inner_finishing_with_no_probe_batch_answers_nothing() {
        finishes_with_nothing(JoinType::Inner);
    }
}

operator_case! {
    GpuHashJoin,
    fn right_finishing_with_no_probe_batch_answers_nothing() {
        finishes_with_nothing(JoinType::Right);
    }
}

operator_case! {
    GpuHashJoin,
    fn right_semi_finishing_with_no_probe_batch_answers_nothing() {
        finishes_with_nothing(JoinType::RightSemi);
    }
}

operator_case! {
    GpuHashJoin,
    fn right_anti_finishing_with_no_probe_batch_answers_nothing() {
        finishes_with_nothing(JoinType::RightAnti);
    }
}

/// Per pbench query that plans a join, the node-level cases holding its shapes.
const PBENCH_SHAPES: &[(&str, &[&str])] = &[
    (
        "anti-null-preserved-condition",
        &["a_left_anti_join_keeps_a_row_whose_preserved_condition_is_null"],
    ),
    (
        "cross-empty-build",
        &[
            "a_cross_join_over_a_zero_row_build_agrees",
            "a_cross_join_with_no_build_batch_is_never_probed",
        ],
    ),
    (
        "cross-projection",
        &["a_cross_join_with_a_projection_agrees"],
    ),
    (
        "decimal15-key-join",
        &["an_inner_join_keyed_on_decimal15_agrees"],
    ),
    (
        "empty-side-left-join",
        &["right_with_no_build_batch_pads_every_probe_row"],
    ),
    (
        "exists-null-keys",
        &[
            "a_left_semi_join_over_many_to_many_keys_emits_each_build_row_once",
            "a_left_semi_join_over_one_probe_batch_agrees",
        ],
    ),
    (
        "exists-or-mark",
        &["a_left_mark_join_with_a_crossing_projection_agrees"],
    ),
    (
        "finish-without-probe",
        &["left_mark_finishing_with_no_probe_batch_marks_every_build_row_false"],
    ),
    (
        "float64-key-join",
        &[
            "an_inner_join_keyed_on_float64_agrees",
            "bug_a_float_join_key_misses_negative_zero_and_nan_pairs_on_the_cpu",
        ],
    ),
    (
        "full-join-null-keys",
        &["a_full_join_pads_null_key_rows_on_both_sides_under_the_sql_default"],
    ),
    (
        "full-join-residual",
        &["a_full_join_with_a_residual_filter_agrees"],
    ),
    (
        "indf-full-join",
        &["a_full_join_under_null_equals_null_matches_null_keys"],
    ),
    (
        "inner-join-hot-keys",
        &["an_inner_join_over_one_probe_batch_agrees"],
    ),
    (
        "left-join-null-keys",
        &["a_left_join_pads_a_null_key_build_row_under_the_sql_default"],
    ),
    (
        "left-join-residual",
        &["a_left_join_with_a_residual_filter_agrees"],
    ),
    (
        "like-column-pattern",
        &["bug_a_like_against_a_column_pattern_is_refused_on_the_device"],
    ),
    (
        "mark-cross-residual",
        &["a_left_mark_join_with_a_residual_filter_agrees"],
    ),
    (
        "nl-full",
        &["a_full_nested_loop_join_over_two_probe_batches_agrees"],
    ),
    (
        "nl-inner",
        &["an_inner_nested_loop_join_over_two_probe_batches_agrees"],
    ),
    (
        "nl-left",
        &["a_left_nested_loop_join_over_two_probe_batches_agrees"],
    ),
    (
        "nl-left-anti",
        &["a_left_anti_nested_loop_join_over_two_probe_batches_agrees"],
    ),
    (
        "nl-left-decimal",
        &["a_left_nested_loop_join_with_a_decimal_predicate_agrees"],
    ),
    (
        "nl-left-semi",
        &["a_left_semi_nested_loop_join_over_two_probe_batches_agrees"],
    ),
    (
        "nl-mark",
        &["a_left_mark_nested_loop_join_over_two_probe_batches_agrees"],
    ),
    (
        "nl-projection",
        &["an_inner_nested_loop_join_with_a_projection_agrees"],
    ),
    (
        "nl-right",
        &["a_right_nested_loop_join_over_two_probe_batches_agrees"],
    ),
    (
        "nl-right-anti",
        &["a_right_anti_nested_loop_join_over_two_probe_batches_agrees"],
    ),
    (
        "nl-right-semi",
        &["a_right_semi_nested_loop_join_over_two_probe_batches_agrees"],
    ),
    (
        "not-exists-null-keys",
        &["a_left_anti_join_keeps_its_null_key_build_rows"],
    ),
    (
        "not-in-correlated",
        &[
            "a_right_anti_join_with_a_probe_side_is_null_residual_agrees",
            "a_right_anti_join_on_a_composite_key_agrees",
        ],
    ),
    (
        "not-in-under-or",
        &[
            "a_left_mark_join_with_a_build_side_is_null_residual_agrees",
            "a_left_mark_join_on_a_composite_key_agrees",
        ],
    ),
    (
        "not-in-uncorrelated",
        &[
            "a_predicate_free_right_nested_loop_join_over_two_probe_batches_agrees",
            "a_cross_join_with_a_projection_agrees",
            "a_right_anti_join_over_two_probe_batches_agrees",
        ],
    ),
    (
        "not-not-in",
        &["a_right_semi_join_over_two_probe_batches_agrees"],
    ),
    (
        "not-or-not-in",
        &["a_right_semi_join_over_two_probe_batches_agrees"],
    ),
    (
        "outer-on-true-empty",
        &["a_predicate_free_right_nested_loop_with_no_build_batch_pads_every_probe_row"],
    ),
    (
        "probe-exists-residual",
        &["a_right_semi_join_with_a_residual_filter_agrees"],
    ),
    (
        "probe-not-exists-residual",
        &["a_right_anti_join_with_a_residual_filter_agrees"],
    ),
    (
        "right-join-null-keys",
        &[
            "a_right_join_over_one_probe_batch_agrees",
            "null_equals_null_is_honoured_by_a_right_join",
        ],
    ),
    (
        "right-join-residual",
        &["a_right_join_with_a_residual_filter_agrees"],
    ),
    (
        "scalar-subquery-cross",
        &[
            "a_predicate_free_left_nested_loop_join_over_two_probe_batches_agrees",
            "a_cross_join_of_two_rowmarker_sides_keeps_one_placeholder",
        ],
    ),
    (
        "sparse-build-anti",
        &["right_anti_with_no_build_batch_returns_every_probe_row"],
    ),
    (
        "sparse-build-full",
        &["full_with_no_build_batch_pads_every_probe_row"],
    ),
    (
        "sparse-build-right",
        &["right_with_no_build_batch_pads_every_probe_row"],
    ),
    (
        "sparse-probe-left",
        &["right_finishing_with_no_probe_batch_answers_nothing"],
    ),
    (
        "sparse-probe-semi",
        &["right_semi_finishing_with_no_probe_batch_answers_nothing"],
    ),
    (
        "ts-key-join",
        &["an_inner_join_keyed_on_timestamp_us_agrees"],
    ),
    ("uint-key-join", &["an_inner_join_keyed_on_uint32_agrees"]),
];

/// Every pbench query whose plan golden holds a join is in `PBENCH_SHAPES`, every query there
/// is a pbench query, and every case it names is one the harness registered — so a pbench
/// join query added without its node-level case, or a case renamed away, turns this red.
#[test]
fn every_pbench_join_shape_has_a_node_level_case() {
    let golden = golden_dir_for("pbench", "1").join("tp1-single.plans.txt");
    let text = std::fs::read_to_string(&golden).expect("pbench's plan golden");
    let joins: BTreeSet<String> = ordered_sections(&text)
        .into_iter()
        .filter(|(_, body)| {
            let tree = body.split("--- recipes ---").next().expect("the tree");
            tree.lines().any(|line| {
                let node = line.trim_start();
                ["GpuHashJoin:", "GpuNestedLoopJoin:", "GpuCrossJoin:"]
                    .iter()
                    .any(|kind| node.starts_with(kind))
            })
        })
        .map(|(name, _)| name)
        .collect();
    let listed: BTreeSet<String> = PBENCH_SHAPES.iter().map(|(q, _)| q.to_string()).collect();
    let unlisted: Vec<&String> = joins.difference(&listed).collect();
    assert!(
        unlisted.is_empty(),
        "pbench joins with no node-level case named: {unlisted:?}"
    );
    let not_queries: Vec<&String> = listed
        .iter()
        .filter(|q| !queries_dir_for("pbench").join(format!("{q}.sql")).exists())
        .collect();
    assert!(
        not_queries.is_empty(),
        "not pbench queries: {not_queries:?}"
    );
    let registered: HashSet<&str> = inventory::iter::<Covers>
        .into_iter()
        .map(|c| c.case)
        .collect();
    let missing: Vec<&&str> = PBENCH_SHAPES
        .iter()
        .flat_map(|(_, cases)| cases.iter())
        .filter(|case| !registered.contains(**case))
        .collect();
    assert!(missing.is_empty(), "named and not registered: {missing:?}");
}
