//! What the index derives, against a plan whose shape is written down here.
//!
//! Every one of these is read far from where it is computed — a region decides which nodes
//! a join holds, `ready_lanes` decides when a cross-lane node is runnable, and
//! `slot_base` decides whose residency a call is charged to — so a wrong one shows as a
//! deadlock or a mis-billed byte rather than as a wrong index.

use super::*;
use crate::plan::Reader;
use crate::tests::plans::*;
use crate::tests::rebuild::every_kind;
use crate::tests::share::share_sources;

/// unload <- join <- [coalesce <- merge_sorted <- source(build, 2),
///                    emit(4) <- source(probe, 1)]
///
/// One of everything the counts below distinguish: a join with two children, a partition
/// accumulator with two input lanes, an emitter reading one lane and filling four, and a
/// lane-scoped node on each side of it.
fn plan() -> Box<dyn GpuNode> {
    unload(join(
        coalesce_all(merge_sorted(sort(source("build", 2)))),
        emit(source("probe", 1), 4),
    ))
}

/// The plan above in pre-order, which is the numbering every assertion here is written in.
const PRE_ORDER: [&str; 8] = [
    "GpuUnload",
    "GpuHashJoin",
    "GpuCoalesceAllBatches",
    "GpuMergeSortedPartitions",
    "GpuSort",
    "GpuLoadParquet",
    "GpuEmitPartitions",
    "GpuLoadParquet",
];

fn indexed(root: &dyn GpuNode) -> PlanIndex<'_> {
    PlanIndex::build(root).expect("the plan indexes")
}

#[test]
fn nodes_are_numbered_pre_order_and_each_knows_its_readers() {
    let plan = plan();
    let index = indexed(plan.as_ref());
    let names: Vec<&str> = index.nodes.iter().map(|node| node.node.name()).collect();
    assert_eq!(names, PRE_ORDER);
    let readers: Vec<Vec<Reader>> = index
        .nodes
        .iter()
        .map(|node| node.readers.clone())
        .collect();
    assert_eq!(
        readers,
        vec![
            vec![],
            vec![reader(0, 0)],
            vec![reader(1, 0)],
            vec![reader(2, 0)],
            vec![reader(3, 0)],
            vec![reader(4, 0)],
            vec![reader(1, 1)],
            vec![reader(6, 0)],
        ],
        "the root has no reader and every other node is read once, by its parent"
    );
    let children: Vec<Vec<usize>> = index
        .nodes
        .iter()
        .map(|node| node.children.clone())
        .collect();
    assert_eq!(
        children,
        vec![
            vec![1],
            vec![2, 6],
            vec![3],
            vec![4],
            vec![5],
            Vec::new(),
            vec![7],
            Vec::new(),
        ],
        "the join's build side is its first child, which is what makes BUILD_SLOT zero"
    );
}

/// On a tree a region is the node's subtree, a contiguous range of the numbering. It is what
/// stops a join's hold from covering nodes outside its probe.
#[test]
fn a_region_on_a_tree_is_a_contiguous_range_of_the_numbering() {
    let plan = plan();
    let index = indexed(plan.as_ref());
    let ends = [8, 8, 6, 6, 6, 6, 8, 8];
    let ranges: Vec<Vec<usize>> = ends
        .iter()
        .enumerate()
        .map(|(node, end)| (node..*end).collect())
        .collect();
    assert_eq!(
        index.shape.regions, ranges,
        "each node's region starts at itself and ends past its last descendant"
    );
    let join = &index.shape.joins;
    assert_eq!(join.len(), 1);
    assert_eq!(
        join[0].probe,
        vec![6, 7],
        "a join holds its PROBE subtree, which is its second child's range and not its \
         first's"
    );
}

/// Three counts, three categories, and they differ for reasons a reader cannot see from
/// the lane count alone.
#[test]
fn the_counts_a_category_changes_are_the_ones_it_should() {
    let plan = plan();
    let index = indexed(plan.as_ref());
    let of = |name: &str| {
        index
            .nodes
            .iter()
            .position(|node| node.node.name() == name)
            .expect("a node of that kind")
    };
    let merge = &index.nodes[of("GpuMergeSortedPartitions")];
    assert_eq!(
        (merge.lanes, merge.input_lanes, merge.ready_lanes),
        (1, 2, 2),
        "a partition accumulator emits one lane, owes a Done per input lane, and becomes \
         ready one input lane at a time"
    );
    let emitter = &index.nodes[of("GpuEmitPartitions")];
    assert_eq!(
        (emitter.lanes, emitter.input_lanes, emitter.ready_lanes),
        (4, 0, 1),
        "an emitter fills four lanes from the one it reads, and it is that one that makes \
         it runnable"
    );
    let sort = &index.nodes[of("GpuSort")];
    assert_eq!(
        (sort.lanes, sort.input_lanes, sort.ready_lanes),
        (2, 0, 2),
        "a lane-scoped node is ready per lane, and owes no Done events at all"
    );
}

/// One slot per lane where an executor is per lane, one for the node where it is not —
/// which is what makes a cross-lane node's residency a single figure rather than N.
#[test]
fn a_slot_is_per_lane_only_where_the_executor_is() {
    let plan = plan();
    let index = indexed(plan.as_ref());
    let bases: Vec<usize> = index.nodes.iter().map(|node| node.slot_base).collect();
    // Four lanes for the unload and the join, which take the emitter's; one for the
    // coalesce; one apiece for the merge and the emitter, which are cross-lane; and two
    // for the sort and its source, which are the build side's.
    assert_eq!(bases, vec![0, 4, 8, 9, 10, 12, 14, 15]);
    assert_eq!(index.slots, 16);
    let merge = 3;
    assert_eq!(
        (index.slot(merge, 0), index.slot(merge, 1)),
        (9, 9),
        "every lane of a cross-lane node bills the same slot"
    );
    let sort = 4;
    assert_eq!(
        (index.slot(sort, 0), index.slot(sort, 1)),
        (10, 11),
        "and a lane-scoped one bills its own"
    );
}

fn reader(parent: usize, slot: usize) -> Reader {
    Reader { parent, slot }
}

fn indexed_roots<'a>(roots: &'a [Box<dyn GpuNode>]) -> PlanIndex<'a> {
    let refs: Vec<&dyn GpuNode> = roots.iter().map(|root| root.as_ref()).collect();
    PlanIndex::build_roots(&refs).expect("the roots index")
}

/// `unload(filter(S))` and `unload(project(S))`, over one shared S.
fn two_readers() -> Vec<Box<dyn GpuNode>> {
    let (roots, shared) = share_sources(vec![
        unload(filter(source("s", 1))),
        unload(project(source("s", 1))),
    ]);
    assert_eq!(shared, 1);
    roots
}

#[test]
fn a_shared_source_is_indexed_once_with_two_readers() {
    let roots = two_readers();
    let index = indexed_roots(&roots);
    let names: Vec<&str> = index.nodes.iter().map(|node| node.node.name()).collect();
    assert_eq!(
        names,
        [
            "GpuUnload",
            "GpuFilter",
            "GpuLoadParquet",
            "GpuUnload",
            "GpuProject"
        ]
    );
    let (filter, s, project) = (1, 2, 4);
    assert_eq!(
        index.nodes[s].readers,
        vec![reader(filter, 0), reader(project, 0)]
    );
    assert_eq!(index.shape.readers[s], index.nodes[s].readers);
    assert_eq!(index.nodes[filter].children, vec![s]);
    assert_eq!(index.nodes[project].children, vec![s]);
    assert_eq!(
        (
            index.nodes[filter].child_readers.clone(),
            index.nodes[project].child_readers.clone()
        ),
        (vec![0], vec![1]),
        "each consumer's edge is its own ordinal among the source's readers"
    );
    assert_eq!(
        index.slots, 5,
        "the source bills one slot, not one per reader"
    );
}

#[test]
fn heights_are_the_longest_path_to_a_root() {
    let (roots, _) = share_sources(vec![
        unload(filter(source("s", 1))),
        unload(filter(project(filter(source("s", 1))))),
    ]);
    let index = indexed_roots(&roots);
    let s = 2;
    assert_eq!(index.nodes[s].node.name(), "GpuLoadParquet");
    assert_eq!(index.shape.heights, vec![0, 1, 4, 0, 1, 2, 3]);
    for reader in &index.nodes[s].readers {
        assert!(
            index.shape.heights[reader.parent] < index.shape.heights[s],
            "every reader runs at a strictly lower height, so it drains first"
        );
    }
}

#[test]
fn every_root_is_indexed_in_order() {
    let roots = two_readers();
    let index = indexed_roots(&roots);
    assert_eq!(index.shape.roots, vec![0, 3]);
    for root in &index.shape.roots {
        assert_eq!(index.nodes[*root].node.name(), "GpuUnload");
        assert!(index.nodes[*root].readers.is_empty());
    }
    assert_eq!(
        index.shape.regions[3],
        vec![3, 4],
        "the second root's region stops short of the source the first one also reads"
    );
}

/// What the recursive tree walk the index made before it read a DAG gave each node, written
/// out again here so the DAG index is held to it.
struct TreeWalk {
    heights: Vec<u32>,
    lanes: Vec<usize>,
    post_order: Vec<usize>,
    end: Vec<usize>,
    slot_base: Vec<usize>,
}

fn tree_walk(root: &dyn GpuNode) -> TreeWalk {
    fn visit(node: &dyn GpuNode, height: u32, walk: &mut TreeWalk, post: &mut usize) -> usize {
        let at = walk.heights.len();
        walk.heights.push(height);
        walk.lanes.push(0);
        walk.post_order.push(0);
        walk.end.push(0);
        let children: Vec<usize> = node
            .children()
            .into_iter()
            .map(|child| visit(child, height + 1, walk, post))
            .collect();
        walk.lanes[at] = match node.kind().layout() {
            Some(layout) => layout.n,
            None => walk.lanes[children[0]],
        };
        walk.post_order[at] = *post;
        *post += 1;
        walk.end[at] = walk.heights.len();
        at
    }
    let mut walk = TreeWalk {
        heights: Vec::new(),
        lanes: Vec::new(),
        post_order: Vec::new(),
        end: Vec::new(),
        slot_base: Vec::new(),
    };
    visit(root, 0, &mut walk, &mut 0);
    let mut nodes = Vec::new();
    collect(root, &mut nodes);
    let mut slots = 0;
    for (at, node) in nodes.iter().enumerate() {
        walk.slot_base.push(slots);
        slots += if category_of(*node).is_lane_scoped() {
            walk.lanes[at]
        } else {
            1
        };
    }
    walk
}

fn collect<'a>(node: &'a dyn GpuNode, nodes: &mut Vec<&'a dyn GpuNode>) {
    nodes.push(node);
    for child in node.children() {
        collect(child, nodes);
    }
}

#[test]
fn a_tree_indexes_exactly_as_before() {
    let mut fixtures = every_kind();
    fixtures.push(plan());
    for fixture in &fixtures {
        let name = fixture.name();
        let index = indexed(fixture.as_ref());
        let walk = tree_walk(fixture.as_ref());
        assert_eq!(index.len(), walk.heights.len(), "{name}");
        assert_eq!(index.shape.roots, vec![0], "{name}");
        assert_eq!(index.shape.heights, walk.heights, "{name}");
        for (at, node) in index.nodes.iter().enumerate() {
            assert_eq!(
                index.shape.regions[at],
                (at..walk.end[at]).collect::<Vec<usize>>(),
                "{name} node {at}"
            );
            assert_eq!(node.lanes, walk.lanes[at], "{name} node {at}");
            assert_eq!(node.post_order, walk.post_order[at], "{name} node {at}");
            assert_eq!(node.slot_base, walk.slot_base[at], "{name} node {at}");
            assert_eq!(node.child_readers, vec![0; node.children.len()], "{name}");
        }
        for join in &index.shape.joins {
            let probe = index.nodes[join.node].children[PROBE_CHILD];
            assert_eq!(
                join.probe,
                (probe..walk.end[probe]).collect::<Vec<usize>>(),
                "{name}"
            );
        }
    }
}
