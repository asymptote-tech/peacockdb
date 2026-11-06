//! Several roots: holds and the buffers that answer them, the wait-for graph between holds,
//! and the concurrency cap.

use super::*;

fn shared_source(table: &str) -> SharedSource {
    let load = crate::tests::plans::source(table, 1)
        .into_any_box()
        .downcast::<GpuLoadParquet>()
        .expect("plans::source builds a loader");
    SharedSource::new(*load)
}

fn read(source: &SharedSource) -> Box<dyn GpuNode> {
    Box::new(source.clone())
}

fn buffered(input: Box<dyn GpuNode>) -> Box<dyn GpuNode> {
    Box::new(new_hold_buffer(input))
}

fn refs(roots: &[Box<dyn GpuNode>]) -> Vec<&dyn GpuNode> {
    roots.iter().map(|root| root.as_ref()).collect()
}

fn unbuffered(
    hold: usize,
    reader: usize,
    slot: usize,
    producer: usize,
) -> (usize, usize, usize, usize) {
    (hold, reader, slot, producer)
}

/// The refusal's four indices, or a panic naming whatever came back instead.
fn refused_edge(result: Result<(), PlanError>) -> (usize, usize, usize, usize) {
    match result {
        Err(PlanError::UnbufferedHold {
            hold,
            reader,
            slot,
            producer,
            ..
        }) => (hold, reader, slot, producer),
        other => panic!("expected an unbuffered hold, got {other:?}"),
    }
}

/// A = `unload(join(coalesce(b), filter(S)))`, B = `unload(project(S))`.
fn a_shared_source_read_inside_a_hold() -> Vec<Box<dyn GpuNode>> {
    use crate::tests::plans::{coalesce_all, filter, project, projected_join, source, unload};
    let s = shared_source("s");
    vec![
        unload(projected_join(
            coalesce_all(source("b", 1)),
            filter(read(&s)),
        )),
        unload(project(read(&s))),
    ]
}

/// A = `unload(join(coalesce(S1), S2))`, B = `unload(join(coalesce(S2), S1))`: each build
/// waits on the source the other one's probe holds.
fn builds_waiting_on_each_others_probe(buffers: bool) -> Vec<Box<dyn GpuNode>> {
    use crate::tests::plans::{coalesce_all, projected_join, unload};
    let (s1, s2) = (shared_source("s1"), shared_source("s2"));
    let probe = |source: &SharedSource| match buffers {
        true => buffered(read(source)),
        false => read(source),
    };
    vec![
        unload(projected_join(coalesce_all(read(&s1)), probe(&s2))),
        unload(projected_join(coalesce_all(read(&s2)), probe(&s1))),
    ]
}

#[test]
fn an_unbuffered_reader_of_a_shared_source_inside_a_hold_is_refused() {
    let roots = a_shared_source_read_inside_a_hold();
    // 0 unload, 1 join, 2 coalesce, 3 b, 4 filter, 5 S, 6 unload, 7 project.
    let refused = validate_roots(&refs(&roots));
    assert_eq!(refused_edge(refused.clone()), unbuffered(1, 4, 0, 5));
    assert_eq!(
        refused.expect_err("refused").to_string(),
        "invalid plan: GpuFilter (node 4) reads GpuLoadParquet (node 5) from outside the hold \
         of GpuHashJoin (node 1); a GpuHoldBuffer must sit on that edge"
    );
}

#[test]
fn a_join_reading_one_source_on_both_sides_without_a_buffer_is_refused() {
    use crate::tests::plans::{coalesce_all, projected_join, unload};
    let s = shared_source("s");
    // 0 unload, 1 join, 2 coalesce, 3 S.
    let roots = vec![unload(projected_join(coalesce_all(read(&s)), read(&s)))];
    assert_eq!(
        refused_edge(validate_roots(&refs(&roots))),
        unbuffered(1, 1, 1, 3)
    );
}

#[test]
fn a_buffer_on_that_edge_passes() {
    use crate::tests::plans::{coalesce_all, filter, project, projected_join, source, unload};
    let s = shared_source("s");
    let roots = vec![
        unload(projected_join(
            coalesce_all(source("b", 1)),
            filter(buffered(read(&s))),
        )),
        unload(project(read(&s))),
    ];
    assert_eq!(validate_roots(&refs(&roots)), Ok(()));
    let s = shared_source("s");
    let roots = vec![unload(projected_join(
        coalesce_all(read(&s)),
        buffered(read(&s)),
    ))];
    assert_eq!(validate_roots(&refs(&roots)), Ok(()));
}

#[test]
fn two_roots_whose_builds_wait_on_each_others_probe_are_refused_without_buffers() {
    let roots = builds_waiting_on_each_others_probe(false);
    // 0 unload, 1 join, 2 coalesce, 3 S1, 4 S2, 5 unload, 6 join, 7 coalesce.
    assert_eq!(
        refused_edge(validate_roots(&refs(&roots))),
        unbuffered(1, 1, 1, 4)
    );
    let roots = builds_waiting_on_each_others_probe(true);
    assert_eq!(validate_dag(&refs(&roots), None), Ok(()));
}

#[test]
fn a_tree_never_needs_a_buffer() {
    for fixture in crate::tests::rebuild::every_kind() {
        let root = rooted(fixture);
        let dag = PlanDag::of(&[root.as_ref()]);
        assert_eq!(
            crate::plan::validate::holds_are_buffered(&dag),
            Ok(()),
            "{root:?}"
        );
    }
}

#[test]
fn a_hold_cycle_is_refused_by_name() {
    assert_eq!(
        crate::plan::validate::first_cycle(&[(1, 5), (5, 9), (9, 1)]),
        Some(vec![1, 5, 9])
    );
    assert_eq!(
        crate::plan::validate::first_cycle(&[(1, 5), (5, 9), (9, 5)]),
        Some(vec![5, 9])
    );
    assert_eq!(
        crate::plan::validate::first_cycle(&[(1, 5), (5, 9), (1, 9)]),
        None
    );
    assert_eq!(crate::plan::validate::first_cycle(&[]), None);
    let roots = builds_waiting_on_each_others_probe(false);
    let dag = PlanDag::of(&refs(&roots));
    let refused = crate::plan::validate::hold_cycle(&dag, vec![1, 6]);
    assert!(matches!(&refused, PlanError::HoldCycle { holds, .. } if *holds == vec![1, 6]));
    assert_eq!(
        refused.to_string(),
        "invalid plan: holds wait on each other: GpuHashJoin (node 1) → GpuHashJoin (node 6)"
    );
}

#[test]
fn a_join_waits_on_the_hold_its_build_side_sits_in() {
    use crate::tests::plans::{coalesce_all, filter, projected_join, source, unload};
    // The inner join is the outer one's probe side, so the inner build is held by the outer
    // join: 0 unload, 1 outer, 2 coalesce, 3 c, 4 inner, 5 coalesce, 6 a, 7 filter, 8 b.
    let inner = projected_join(coalesce_all(source("a", 1)), filter(source("b", 1)));
    let root = unload(projected_join(coalesce_all(source("c", 1)), inner));
    let dag = PlanDag::of(&[root.as_ref()]);
    assert_eq!(
        crate::plan::validate::wait_for_edges(&dag, None),
        vec![(4, 1)]
    );
    assert_eq!(validate_dag(&[root.as_ref()], None), Ok(()));
}

#[test]
fn validate_dag_refuses_what_its_parts_refuse() {
    use crate::tests::plans::{coalesce_all, projected_join, unload};
    let s = shared_source("s");
    let shapes = vec![
        a_shared_source_read_inside_a_hold(),
        builds_waiting_on_each_others_probe(false),
        vec![unload(projected_join(coalesce_all(read(&s)), read(&s)))],
        vec![rooted(rooted(plain_source()))],
    ];
    for roots in shapes {
        let refused = validate_roots(&refs(&roots));
        assert!(refused.is_err(), "{roots:?}");
        assert_eq!(validate_dag(&refs(&roots), None), refused);
    }
}

#[test]
fn buffering_each_refused_edge_makes_a_plan_valid() {
    use crate::tests::share::buffer_unbuffered_holds;
    let (roots, inserted) =
        buffer_unbuffered_holds(builds_waiting_on_each_others_probe(false), None);
    assert_eq!(inserted, 2);
    assert_eq!(validate_dag(&refs(&roots), None), Ok(()));
    let (roots, inserted) = buffer_unbuffered_holds(a_shared_source_read_inside_a_hold(), None);
    assert_eq!(inserted, 1);
    let dag = PlanDag::of(&refs(&roots));
    assert_eq!(dag.nodes[4].node.name(), "GpuFilter");
    assert!(dag.is_buffer(dag.nodes[4].children[0]));
    assert_eq!(validate_dag(&refs(&roots), None), Ok(()));
}

// -- the concurrency cap -------------------------------------------------------------

/// Three roots `unload(filter(S))`: 0 unload, 1 filter, 2 S, 3 unload, 4 filter, 5 unload,
/// 6 filter.
fn three_filters_of_one_source() -> Vec<Box<dyn GpuNode>> {
    use crate::tests::plans::{filter, unload};
    let s = shared_source("s");
    (0..3).map(|_| unload(filter(read(&s)))).collect()
}

/// Three roots `unload(join(coalesce(b_i), buffer(filter(S))))`: root 0 is nodes 0–5 with S
/// at 6, root 1 is 7–12 (join 8), root 2 is 13–18 (join 14).
fn capped_joins() -> Vec<Box<dyn GpuNode>> {
    use crate::tests::plans::{coalesce_all, filter, projected_join, source, unload};
    let s = shared_source("s");
    (0..3)
        .map(|at| {
            unload(projected_join(
                coalesce_all(source(&format!("b{at}"), 1)),
                buffered(filter(read(&s))),
            ))
        })
        .collect()
}

#[test]
fn a_cap_over_an_unbuffered_shared_reader_is_refused() {
    let roots = three_filters_of_one_source();
    let cap = |k| std::num::NonZeroUsize::new(k);
    assert_eq!(
        refused_edge(validate_cap(&refs(&roots), cap(1))),
        unbuffered(3, 4, 0, 2),
        "root 1 is the first that waits"
    );
    assert_eq!(
        refused_edge(validate_dag(&refs(&roots), cap(2))),
        unbuffered(5, 6, 0, 2),
        "under a cap of two only root 2 waits"
    );
}

#[test]
fn a_cap_at_or_above_the_root_count_needs_no_buffer() {
    let roots = three_filters_of_one_source();
    for cap in [
        None,
        std::num::NonZeroUsize::new(3),
        std::num::NonZeroUsize::new(4),
    ] {
        assert_eq!(validate_cap(&refs(&roots), cap), Ok(()), "{cap:?}");
        assert_eq!(validate_dag(&refs(&roots), cap), Ok(()), "{cap:?}");
    }
}

#[test]
fn a_capped_root_never_waits_on_its_own_region() {
    let roots = capped_joins();
    let dag = PlanDag::of(&refs(&roots));
    let edges = crate::plan::validate::wait_for_edges(&dag, std::num::NonZeroUsize::new(1));
    for (join, root) in [(1, 0), (8, 7), (14, 13)] {
        assert!(!edges.contains(&(join, root)), "join {join}: {edges:?}");
        assert!(!edges.contains(&(root, root)), "root {root}: {edges:?}");
    }
    assert!(
        edges.contains(&(7, 1)),
        "root 1 waits on root 0, whose join holds a node of it: {edges:?}"
    );
    assert!(
        edges.contains(&(13, 7)),
        "root 2 waits on root 1's cap: {edges:?}"
    );
    assert_eq!(
        validate_dag(&refs(&roots), std::num::NonZeroUsize::new(1)),
        Ok(())
    );
}

/// What `UncappedAccumulator` names: the waiting root and the accumulator, or a panic naming
/// whatever came back instead.
fn uncapped(result: Result<(), PlanError>) -> (usize, usize, String) {
    match result {
        Err(PlanError::UncappedAccumulator { root, node, what }) => (root, node, what),
        other => panic!("expected an uncapped accumulator, got {other:?}"),
    }
}

#[test]
fn an_accumulator_between_a_waiting_roots_buffer_and_its_source_is_refused() {
    use crate::tests::plans::{coalesce_all, filter, projected_join, source};
    let cap = std::num::NonZeroUsize::new(1);
    // The cap holds root 1 only down to its buffer, so a coalesce below it would fill with
    // everything S emits while root 1 waits: state alive outside the cap.
    let s = shared_source("s");
    let roots = vec![
        crate::tests::plans::unload(filter(read(&s))),
        crate::tests::plans::unload(buffered(coalesce_all(read(&s)))),
    ];
    let (root, node, what) = uncapped(validate_cap(&refs(&roots), cap));
    assert_eq!((root, node), (3, 5));
    assert!(what.contains("GpuCoalesceAllBatches (node 5)"), "{what}");
    assert!(what.contains("GpuUnload (node 3)"), "{what}");
    // A join's build is state too, and `validate_dag` reaches the same rule.
    let s = shared_source("s");
    let roots = vec![
        crate::tests::plans::unload(filter(read(&s))),
        crate::tests::plans::unload(buffered(projected_join(
            coalesce_all(source("b", 1)),
            buffered(filter(read(&s))),
        ))),
    ];
    let (root, node, what) = uncapped(validate_dag(&refs(&roots), cap));
    assert_eq!((root, node), (3, 5));
    assert!(what.contains("GpuHashJoin (node 5)"), "{what}");
}

#[test]
fn stateless_nodes_between_a_waiting_roots_buffer_and_its_source_are_accepted() {
    use crate::tests::plans::{filter, project};
    let s = shared_source("s");
    let roots = vec![
        crate::tests::plans::unload(filter(read(&s))),
        crate::tests::plans::unload(buffered(project(filter(read(&s))))),
    ];
    let cap = std::num::NonZeroUsize::new(1);
    assert_eq!(validate_cap(&refs(&roots), cap), Ok(()));
    assert_eq!(validate_dag(&refs(&roots), cap), Ok(()));
}
