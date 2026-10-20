//! An aggregate's init as the recipe writes it: its calls per batch, and the same calls once
//! more at done for a lane where nothing went out, the first handed nothing.

use super::*;
use crate::plan::{AggFunc, AggStateColumns};

/// A `sum(n)` init over `[k, n]`, keyed on `k` or keyless.
fn summing_init(keyed: bool) -> GpuAggregate {
    let group_by = if keyed {
        vec![Expr::column(0, "k")]
    } else {
        Vec::new()
    };
    let state: &[&str] = if keyed { &["k", "sum(n)"] } else { &["sum(n)"] };
    GpuAggregate::new(
        Given::input(BatchLayout::MultipleBatches, &["k", "n"]),
        AggregateBody {
            group_by,
            grouping_sets: Vec::new(),
            null_exprs: Vec::new(),
            aggs: vec![AggCall {
                func: PlanAgg::Sum,
                args: vec![Expr::column(1, "n")],
                outputs: vec![Field::new("sum(n)", DataType::Int64, true)],
            }],
            finalize: None,
        },
        columns_of(state),
        columns_of(state),
    )
}

/// The done call is the init's own seq, handed nothing: no second wire node. Grouped and
/// keyless alike — what the call answers over nothing is the engine's, not the recipe's.
#[test]
fn an_init_calls_its_own_seq_again_at_done_if_nothing_went_out() {
    for keyed in [true, false] {
        let recipe = aggregate(
            &summing_init(keyed),
            &[&columns_of(&["k", "n"])],
            &mut writer(),
        )
        .expect("the aggregate's payloads are writable")
        .expect("an aggregate drives the ABI");
        assert_eq!(
            shape(&recipe),
            vec![
                (FbKind::Aggregate { merge: false }, CallPattern::PerBatch),
                (
                    FbKind::Aggregate { merge: false },
                    CallPattern::AtDoneIfNothingOut
                ),
            ],
            "keyed: {keyed}"
        );
        assert_eq!(
            recipe.calls[0].target, recipe.calls[1].target,
            "keyed: {keyed}"
        );
        assert_eq!(recipe.calls[0].inputs, vec![Input::Batch]);
        assert_eq!(
            recipe.calls[1].inputs,
            Vec::<Input>::new(),
            "handed nothing"
        );
        let (seq, _) = recipe.calls[0].target.expect("the init addresses a seq");
        assert_eq!(
            recipe.to_string(),
            format!(
                "per batch: execute_node(#{seq} CudfAggregate{{Partial}}, batch); \
                 at done, if nothing went out: execute_node(#{seq} CudfAggregate{{Partial}})"
            ),
            "a call handed nothing names its seq and nothing after it"
        );
    }
}

/// The other node that finalizes: the translation hands its finalize to the init itself
/// where one batch in one lane is already the whole of every group, so no merge exists to
/// carry it.
fn finalizing_init(finalize: Vec<NamedExpr>, state: &[&str]) -> GpuAggregate {
    GpuAggregate::new(
        Given::input(BatchLayout::SingleBatch, &["k", "n"]),
        AggregateBody {
            group_by: vec![Expr::column(0, "k")],
            grouping_sets: Vec::new(),
            null_exprs: Vec::new(),
            aggs: vec![AggCall {
                func: PlanAgg::Sum,
                args: vec![Expr::column(1, "n")],
                outputs: vec![Field::new("sum(n)", DataType::Int64, true)],
            }],
            finalize: Some(finalize),
        },
        columns_of(state),
        columns_of(&["k", "sum(n)"]),
    )
}

/// The finalizing merge's mirror. Both branches of `recipe::aggregate` reach the same
/// `finalize_project`, so what this pins is the call list: a second call, per batch rather
/// than at done, since an init that finalizes itself has no compaction to wait for — and the
/// pair again at done for a lane where nothing went out.
#[test]
fn an_init_that_finalizes_itself_carries_the_finalize_in_a_project_of_its_own() {
    let node = finalizing_init(summed(), &["k", "sum(n)"]);
    let recipe = aggregate(&node, &[&columns_of(&["k", "n"])], &mut writer())
        .expect("the aggregate's payloads are writable")
        .expect("an aggregate drives the ABI");
    assert_eq!(
        shape(&recipe),
        vec![
            (FbKind::Aggregate { merge: false }, CallPattern::PerBatch),
            (
                FbKind::Project(ProjectRole::Finalize),
                CallPattern::PerBatch
            ),
            (
                FbKind::Aggregate { merge: false },
                CallPattern::AtDoneIfNothingOut
            ),
            (
                FbKind::Project(ProjectRole::Finalize),
                CallPattern::AtDoneIfNothingOut
            ),
        ],
        "the init builds state and finalizes it per batch, and once more over nothing at done"
    );
    assert_eq!(recipe.calls[2].target, recipe.calls[0].target);
    assert_eq!(recipe.calls[3].target, recipe.calls[1].target);
    assert_eq!(recipe.calls[2].inputs, Vec::<Input>::new());
    assert_eq!(recipe.calls[3].inputs, vec![Input::PriorOutput]);
}

/// The keyless shortcut: `sum(n)` finalized on the init itself, with no key — the node that
/// owes its identity row finalized.
fn finalizing_keyless_init() -> GpuAggregate {
    GpuAggregate::new(
        Given::input(BatchLayout::SingleBatch, &["k", "n"]),
        AggregateBody {
            group_by: Vec::new(),
            grouping_sets: Vec::new(),
            null_exprs: Vec::new(),
            aggs: vec![AggCall {
                func: PlanAgg::Sum,
                args: vec![Expr::column(1, "n")],
                outputs: vec![Field::new("sum(n)", DataType::Int64, true)],
            }],
            finalize: Some(vec![NamedExpr::new(Expr::column(0, "sum(n)"), "sum(n)")]),
        },
        columns_of(&["sum(n)"]),
        columns_of(&["sum(n)"]),
    )
}

/// The grouped shortcut's four calls, keyless: what the done pair answers over nothing is the
/// engine's, so the recipe is the same whatever the keys.
#[test]
fn a_keyless_init_that_finalizes_itself_calls_the_pair_again_at_done() {
    let recipe = aggregate(
        &finalizing_keyless_init(),
        &[&columns_of(&["k", "n"])],
        &mut writer(),
    )
    .expect("the aggregate's payloads are writable")
    .expect("an aggregate drives the ABI");
    assert_eq!(
        shape(&recipe),
        vec![
            (FbKind::Aggregate { merge: false }, CallPattern::PerBatch),
            (
                FbKind::Project(ProjectRole::Finalize),
                CallPattern::PerBatch
            ),
            (
                FbKind::Aggregate { merge: false },
                CallPattern::AtDoneIfNothingOut
            ),
            (
                FbKind::Project(ProjectRole::Finalize),
                CallPattern::AtDoneIfNothingOut
            ),
        ]
    );
    assert_eq!(recipe.calls[2].target, recipe.calls[0].target);
    assert_eq!(recipe.calls[3].target, recipe.calls[1].target);
    assert_eq!(recipe.calls[2].inputs, Vec::<Input>::new());
    assert_eq!(recipe.calls[3].inputs, vec![Input::PriorOutput]);
}

/// Every column the device emits is named on the wire, and a Welford's ddof rides beside its
/// three names: a sample stddev must not reach the device reading as a population one.
#[test]
fn an_init_writes_each_functions_state_names_and_a_welfords_ddof() {
    let triple = |out: &str| -> Vec<AggCall> {
        [
            (PlanAgg::Count, "$count"),
            (PlanAgg::Mean, "$mean"),
            (PlanAgg::M2, "$m2"),
        ]
        .into_iter()
        .map(|(func, suffix)| AggCall {
            func,
            args: vec![Expr::column(1, "n")],
            outputs: vec![Field::new(format!("{out}{suffix}"), DataType::Int64, true)],
        })
        .collect()
    };
    let mut aggs = triple("stddev(n)");
    aggs.extend(triple("var_pop(n)"));
    aggs.push(AggCall {
        func: PlanAgg::Count,
        args: vec![Expr::column(1, "n")],
        outputs: vec![Field::new("count(n)", DataType::Int64, true)],
    });
    let names = [
        "k",
        "stddev(n)$count",
        "stddev(n)$mean",
        "stddev(n)$m2",
        "var_pop(n)$count",
        "var_pop(n)$mean",
        "var_pop(n)$m2",
        "count(n)",
    ];
    let state = Schema {
        group_keys: vec![0],
        agg_state: vec![
            AggStateColumns {
                output: "stddev(n)".into(),
                func: AggFunc::Stddev,
                ddof: 1,
                positions: vec![1, 2, 3],
            },
            AggStateColumns {
                output: "var_pop(n)".into(),
                func: AggFunc::Var,
                ddof: 0,
                positions: vec![4, 5, 6],
            },
        ],
        ..columns_of(&names)
    };
    let node = GpuAggregate::new(
        Given::input(BatchLayout::MultipleBatches, &["k", "n"]),
        AggregateBody {
            group_by: vec![Expr::column(0, "k")],
            grouping_sets: Vec::new(),
            null_exprs: Vec::new(),
            aggs,
            finalize: None,
        },
        state.clone(),
        state,
    );
    let mut writer = writer();
    let recipe = aggregate(&node, &[&columns_of(&["k", "n"])], &mut writer)
        .expect("the aggregate's payloads are writable")
        .expect("an aggregate drives the ABI");
    let (bytes, _) = writer.finish().expect("one root");
    let plan = flatbuffers::root::<fb::GpuPlan>(&bytes).expect("the buffer verifies");
    let written = node_at(&plan, recipe.seqs()[0])
        .expect("the walk names every kind")
        .and_then(|node| node.node_as_cudf_aggregate())
        .expect("the first seq is the init");
    let funcs: Vec<(String, Vec<String>, i8)> = written
        .aggr_funcs()
        .expect("functions")
        .iter()
        .map(|f| {
            (
                f.name().unwrap_or_default().to_string(),
                f.state_names()
                    .map(|n| n.iter().map(str::to_string).collect())
                    .unwrap_or_default(),
                f.ddof(),
            )
        })
        .collect();
    let owned = |n: &[&str]| n.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    assert_eq!(
        funcs,
        vec![
            ("stddev".to_string(), owned(&names[1..4]), 1),
            ("var_pop".to_string(), owned(&names[4..7]), 0),
            ("count".to_string(), owned(&names[7..8]), 0),
        ]
    );
}
