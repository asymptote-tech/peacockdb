//! What each call declares its firing produces: the six arms whose schema is already in
//! hand declare their node's own, and every other arm answers `None` until
//! `declared-schemas-derived.md` gives it one.

use super::{Given, columns_of};
use crate::plan::{
    BatchLayout, BinaryOp, ColumnOrder, Expr, GpuCoalesceAllBatches, GpuFilter, GpuNode,
    GpuProject, GpuSort, GpuUnload, NamedExpr, NodeRef, Schema, try_as_node_ref,
};
use crate::wire::{RecipePlan, attach_recipes};
use datafusion::arrow::datatypes::DataType;
use datafusion::common::ScalarValue;
use std::collections::BTreeSet;

/// What the calls at one post-order position declare, in call order.
fn declared(node: &dyn GpuNode, index: usize) -> Vec<Option<Schema>> {
    attach_recipes(node)
        .expect("writable")
        .get(index)
        .expect("this node calls the device")
        .calls
        .iter()
        .map(|call| call.output_schema.clone())
        .collect()
}

fn declares(node: &dyn GpuNode) -> Vec<Option<Schema>> {
    vec![node.kind().schema().cloned()]
}

#[test]
fn a_scans_call_declares_the_columns_the_source_declares() {
    let scan = crate::tests::rebuild::source(None);
    assert_eq!(declared(scan.as_ref(), 0), declares(scan.as_ref()));
}

/// Not the input's: a filter projects as well as filters, so its output can be narrower
/// than what it reads.
#[test]
fn a_filters_call_declares_its_own_columns_and_not_its_inputs() {
    let filter = GpuFilter::new(
        Given::input(BatchLayout::MultipleBatches, &["k", "v"]),
        Expr::binary(
            Expr::column(1, "v"),
            BinaryOp::Gt,
            Expr::Literal(ScalarValue::Int64(Some(1))),
            DataType::Boolean,
        ),
        Some(vec![1]),
        columns_of(&["v"]),
    );
    assert_eq!(declared(&filter, 1), vec![Some(columns_of(&["v"]))]);
}

#[test]
fn a_projects_call_declares_the_columns_it_projects() {
    let project = GpuProject::new(
        Given::input(BatchLayout::MultipleBatches, &["k", "v"]),
        vec![NamedExpr::new(Expr::column(1, "v"), "v")],
        columns_of(&["v"]),
    );
    assert_eq!(declared(&project, 1), vec![Some(columns_of(&["v"]))]);
}

#[test]
fn a_sorts_call_declares_the_columns_it_reorders() {
    let sort = GpuSort::new(
        Given::input(BatchLayout::MultipleBatches, &["k", "v"]),
        vec![ColumnOrder {
            column: 0,
            ascending: true,
            nulls_first: false,
        }],
        Some(3),
    );
    assert_eq!(declared(&sort, 1), vec![Some(columns_of(&["k", "v"]))]);
}

#[test]
fn a_coalesce_alls_call_declares_the_columns_it_concatenates() {
    let coalesce =
        GpuCoalesceAllBatches::new(Given::input(BatchLayout::MultipleBatches, &["k", "v"]));
    assert_eq!(declared(&coalesce, 1), vec![Some(columns_of(&["k", "v"]))]);
}

#[test]
fn an_unloads_call_declares_the_columns_that_cross() {
    let unload = GpuUnload::new(
        Given::input(BatchLayout::MultipleBatches, &["k", "v"]),
        None,
    );
    assert_eq!(declared(&unload, 1), vec![Some(columns_of(&["k", "v"]))]);
}

/// Every fixture of every kind, walked in the recipe walk's order. The six declare their
/// node's schema on every call; a call from any other arm answers `None`, which the
/// derived task reads as "not yet" rather than as a schema.
///
/// The rebuild's fixtures are built for field coverage, not for the wire, and the two
/// Welford aggregates are refused by it (an `m2` state written on its own has no name
/// there). Those are skipped, and the set of kinds that did reach a recipe is asserted so
/// the skip cannot quietly widen.
#[test]
fn no_call_outside_the_six_arms_declares_anything_yet() {
    let mut reached: BTreeSet<&'static str> = BTreeSet::new();
    for fixture in crate::tests::rebuild::every_kind() {
        let Ok(plan) = attach_recipes(fixture.as_ref()) else {
            continue;
        };
        let mut index = 0;
        check(fixture.as_ref(), &plan, &mut index, &mut reached);
    }
    let every_arm_with_a_recipe = [
        "GpuAccumulateBatchesAndSort",
        "GpuAggregate",
        "GpuAggregateBatches",
        "GpuCoalesceAllBatches",
        "GpuCrossJoin",
        "GpuEmitPartitions",
        "GpuFilter",
        "GpuHashJoin",
        "GpuLimit",
        "GpuLoadParquet",
        "GpuMergeSortedPartitions",
        "GpuNestedLoopJoin",
        "GpuProject",
        "GpuSort",
        "GpuUnload",
    ];
    assert_eq!(reached, BTreeSet::from(every_arm_with_a_recipe));

    fn check(
        node: &dyn GpuNode,
        plan: &RecipePlan,
        index: &mut usize,
        reached: &mut BTreeSet<&'static str>,
    ) {
        for child in node.children() {
            check(child, plan, index, reached);
        }
        if plan.get(*index).is_some() {
            reached.insert(node.name());
        }
        let one_of_the_six = matches!(
            try_as_node_ref(node),
            Some(
                NodeRef::LoadParquet(_)
                    | NodeRef::Filter(_)
                    | NodeRef::Project(_)
                    | NodeRef::Sort(_)
                    | NodeRef::CoalesceAllBatches(_)
                    | NodeRef::Unload(_)
            )
        );
        for call in plan.get(*index).map_or(&[][..], |recipe| &recipe.calls) {
            if one_of_the_six {
                assert_eq!(
                    call.output_schema.as_ref(),
                    node.kind().schema(),
                    "{} declares its own schema on {:?}",
                    node.name(),
                    call.symbol
                );
            } else {
                assert_eq!(
                    call.output_schema,
                    None,
                    "{} is not one of the six arms and declares nothing yet",
                    node.name()
                );
            }
        }
        *index += 1;
    }
}
