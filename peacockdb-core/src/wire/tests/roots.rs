//! Several roots and a shared node on the wire: one buffer, one seq space, a node several
//! parents read written once and indexed once, by the same first-visit post-order the C++
//! indexes with.

use super::*;
use crate::plan::new_hold_buffer;
use crate::tests::plans::{coalesce_all, filter, join, shared, source, unload};

fn refs(roots: &[Box<dyn GpuNode>]) -> Vec<&dyn GpuNode> {
    roots.iter().map(|root| root.as_ref()).collect()
}

fn buffer_of(plan: &RecipePlan) -> fb::GpuPlan<'_> {
    flatbuffers::root::<fb::GpuPlan>(plan.bytes()).expect("the buffer verifies")
}

/// The only seq a node's recipe addresses.
fn seq_of(plan: &RecipePlan, node: usize) -> Seq {
    let seqs = plan.get(node).expect("the node has a recipe").seqs();
    assert_eq!(seqs.len(), 1, "node {node} addresses {seqs:?}");
    seqs[0]
}

/// Where the node at `seq` sits in the buffer: two parents holding one offset hold one node.
fn location(plan: &RecipePlan, seq: Seq) -> usize {
    indexed(&buffer_of(plan)).expect("the plan reads back")[seq as usize]
        ._tab
        .loc()
}

/// `unload(filter(S))` twice: post-order 0 S, 1 filter, 2 unload, 3 filter, 4 unload.
fn two_filters_of_one_source() -> Vec<Box<dyn GpuNode>> {
    let s = shared("s", 1);
    (0..2)
        .map(|_| unload(filter(Box::new(s.clone()))))
        .collect()
}

#[test]
fn a_shared_source_takes_one_seq() {
    let roots = two_filters_of_one_source();
    let plan = attach_recipes_for_roots(&refs(&roots), DEFAULT_CHUNK_BYTES).expect("attaches");
    let alone = attach_recipes(unload(filter(source("s", 1))).as_ref(), DEFAULT_CHUNK_BYTES)
        .expect("attaches");
    assert_eq!(plan.nodes(), 5, "one recipe per node, S once");
    assert_eq!(
        plan.wire_nodes(),
        2 * alone.wire_nodes() - 1,
        "S is written once between the two roots"
    );
    let s = location(&plan, seq_of(&plan, 0));
    for filter in [1, 3] {
        let written = indexed(&buffer_of(&plan)).expect("reads")[seq_of(&plan, filter) as usize];
        let input = children(&written).expect("a filter has its input")[0];
        assert_eq!(input._tab.loc(), s, "filter {filter} reads S's one node");
    }
}

#[test]
fn several_roots_share_one_seq_space() {
    let roots = vec![
        unload(filter(source("a", 1))),
        unload(filter(source("b", 1))),
    ];
    let plan = attach_recipes_for_roots(&refs(&roots), DEFAULT_CHUNK_BYTES).expect("attaches");
    let first: Vec<Seq> = (0..3)
        .flat_map(|node| plan.get(node).map(Recipe::seqs))
        .flatten()
        .collect();
    let second: Vec<Seq> = (3..6)
        .flat_map(|node| plan.get(node).map(Recipe::seqs))
        .flatten()
        .collect();
    assert!(
        first.iter().max() < second.iter().min(),
        "root 1 is numbered after root 0: {first:?} {second:?}"
    );
    assert_eq!(check_seq_kinds(&plan), Ok(()));
    let roots_written = buffer_of(&plan)
        .roots()
        .expect("several roots are written as roots");
    assert_eq!(roots_written.len(), 2);
    assert!(buffer_of(&plan).root().is_none());
}

#[test]
fn one_root_writes_no_roots_field() {
    let root = unload(filter(source("s", 1)));
    let plan = attach_recipes(root.as_ref(), DEFAULT_CHUNK_BYTES).expect("attaches");
    assert!(buffer_of(&plan).roots().is_none());
    assert!(buffer_of(&plan).root().is_some());
    let through_roots =
        attach_recipes_for_roots(&[root.as_ref()], DEFAULT_CHUNK_BYTES).expect("attaches");
    assert_eq!(
        through_roots.bytes(),
        plan.bytes(),
        "one root, the same bytes"
    );
}

#[test]
fn the_reader_visits_a_shared_node_once() {
    let roots = two_filters_of_one_source();
    let plan = attach_recipes_for_roots(&refs(&roots), DEFAULT_CHUNK_BYTES).expect("attaches");
    assert_eq!(check_seq_kinds(&plan), Ok(()));
    assert_eq!(
        indexed(&buffer_of(&plan)).expect("reads").len(),
        plan.wire_nodes()
    );
    let alone = attach_recipes(unload(filter(source("s", 1))).as_ref(), DEFAULT_CHUNK_BYTES)
        .expect("attaches");
    assert_eq!(
        depth(&plan),
        depth(&alone),
        "no root is deeper than it is alone"
    );
}

#[test]
fn a_buffered_shared_reader_takes_the_sources_seq() {
    // Post-order: 0 S, 1 coalesce, 2 buffer, 3 join, 4 unload. The buffer writes nothing,
    // so the probe slot holds S's own offset.
    let s = shared("s", 1);
    let root = unload(join(
        coalesce_all(Box::new(s.clone())),
        Box::new(new_hold_buffer(Box::new(s.clone()))),
    ));
    let plan = attach_recipes_for_roots(&[root.as_ref()], DEFAULT_CHUNK_BYTES).expect("attaches");
    assert_eq!(plan.nodes(), 5);
    assert!(plan.get(2).is_none(), "a hold buffer is not on the wire");
    assert_eq!(check_seq_kinds(&plan), Ok(()));
    let s_at = location(&plan, seq_of(&plan, 0));
    let nodes = indexed(&buffer_of(&plan)).expect("reads");
    let readers = nodes
        .iter()
        .flat_map(|node| children(node).expect("a known kind"))
        .filter(|child| child._tab.loc() == s_at)
        .count();
    assert_eq!(
        readers, 2,
        "the build's coalesce and the join's probe slot read one S"
    );
}
