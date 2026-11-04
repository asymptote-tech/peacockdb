use crate::plan::{GpuLoadParquet, GpuNode, PlanDag, Reader, SharedSource};
use crate::tests::plans::{filter, join, project, source, unload};
use crate::tests::rebuild::every_kind;
use crate::tests::share::share_sources;

fn shared(table: &str) -> SharedSource {
    let load = source(table, 1)
        .into_any_box()
        .downcast::<GpuLoadParquet>()
        .expect("plans::source builds a loader");
    SharedSource::new(*load)
}

fn read(source: &SharedSource) -> Box<dyn GpuNode> {
    Box::new(source.clone())
}

fn refs(roots: &[Box<dyn GpuNode>]) -> Vec<&dyn GpuNode> {
    roots.iter().map(|root| root.as_ref()).collect()
}

fn reader(parent: usize, slot: usize) -> Reader {
    Reader { parent, slot }
}

/// What the recursive tree walk gave each node: its depth, its post-order number and the end
/// of its contiguous pre-order range. Written out again here, so the DAG walk is held to the
/// numbering every consumer read before it.
struct TreeWalk {
    depth: Vec<u32>,
    post_order: Vec<usize>,
    end: Vec<usize>,
    children: Vec<Vec<usize>>,
}

fn tree_walk(root: &dyn GpuNode) -> TreeWalk {
    fn visit(node: &dyn GpuNode, depth: u32, walk: &mut TreeWalk, post: &mut usize) -> usize {
        let at = walk.depth.len();
        walk.depth.push(depth);
        walk.post_order.push(0);
        walk.end.push(0);
        walk.children.push(Vec::new());
        for child in node.children() {
            let index = visit(child, depth + 1, walk, post);
            walk.children[at].push(index);
        }
        walk.post_order[at] = *post;
        *post += 1;
        walk.end[at] = walk.depth.len();
        at
    }
    let mut walk = TreeWalk {
        depth: Vec::new(),
        post_order: Vec::new(),
        end: Vec::new(),
        children: Vec::new(),
    };
    visit(root, 0, &mut walk, &mut 0);
    walk
}

#[test]
fn a_tree_numbers_as_the_walk_always_did() {
    for fixture in every_kind() {
        let name = fixture.name();
        let dag = PlanDag::of(&[fixture.as_ref()]);
        let walk = tree_walk(fixture.as_ref());
        assert_eq!(dag.roots, vec![0], "{name}");
        assert_eq!(dag.nodes.len(), walk.depth.len(), "{name}");
        for (at, node) in dag.nodes.iter().enumerate() {
            assert_eq!(node.children, walk.children[at], "{name} node {at}");
            assert_eq!(node.post_order, walk.post_order[at], "{name} node {at}");
            assert_eq!(node.height, walk.depth[at], "{name} node {at}");
            assert_eq!(
                dag.region_of_node(at),
                (at..walk.end[at]).collect::<Vec<usize>>(),
                "{name} node {at}"
            );
            assert_eq!(node.readers.len(), usize::from(at != 0), "{name} node {at}");
            for (slot, &child) in node.children.iter().enumerate() {
                assert_eq!(dag.nodes[child].readers, vec![reader(at, slot)], "{name}");
                assert_eq!(
                    dag.region_of_edge(at, slot),
                    (child..walk.end[child]).collect::<Vec<usize>>(),
                    "{name} edge ({at}, {slot})"
                );
            }
        }
        let mut by_post_order: Vec<usize> = (0..walk.depth.len()).collect();
        by_post_order.sort_by_key(|&at| walk.post_order[at]);
        assert_eq!(dag.in_post_order(), by_post_order, "{name}");
    }
}

/// `unload(filter(S))` and `unload(project(S))`, over one shared S.
fn two_readers() -> Vec<Box<dyn GpuNode>> {
    let s = shared("s");
    vec![unload(filter(read(&s))), unload(project(read(&s)))]
}

#[test]
fn a_shared_source_is_one_node_with_two_readers() {
    let roots = two_readers();
    let dag = PlanDag::of(&refs(&roots));
    assert_eq!(dag.nodes.len(), 5);
    assert_eq!(dag.nodes[2].node.name(), "GpuLoadParquet");
    assert_eq!(dag.nodes[2].readers, vec![reader(1, 0), reader(4, 0)]);
    assert_eq!(dag.nodes[1].children, vec![2]);
    assert_eq!(dag.nodes[4].children, vec![2]);
    assert_eq!(dag.nodes[2].post_order, 0);
    assert_eq!(dag.in_post_order(), vec![2, 1, 0, 4, 3]);
}

#[test]
fn two_roots_number_in_root_order() {
    let roots = two_readers();
    let dag = PlanDag::of(&refs(&roots));
    assert_eq!(dag.roots, vec![0, 3]);
    assert!(dag.nodes[0].readers.is_empty());
    assert!(dag.nodes[3].readers.is_empty());
}

#[test]
fn a_shared_node_sits_below_its_deepest_consumer() {
    let s = shared("s");
    let roots = vec![
        unload(filter(read(&s))),
        unload(filter(project(filter(read(&s))))),
    ];
    let dag = PlanDag::of(&refs(&roots));
    let at = 2;
    assert_eq!(dag.nodes[at].node.name(), "GpuLoadParquet");
    assert_eq!(dag.nodes[at].height, 4);
    for reader in &dag.nodes[at].readers {
        assert!(dag.nodes[reader.parent].height < dag.nodes[at].height);
    }
}

#[test]
fn a_region_stops_at_a_node_read_from_outside_it() {
    // A = unload(join(filter(S), project(other))), B = unload(S): S sits inside A's
    // pre-order range, and B reads it, so A's region skips it.
    let s = shared("s");
    let roots = vec![
        unload(join(filter(read(&s)), project(source("other", 1)))),
        unload(read(&s)),
    ];
    let dag = PlanDag::of(&refs(&roots));
    assert_eq!(dag.nodes[3].node.name(), "GpuLoadParquet");
    assert_eq!(dag.nodes[3].readers, vec![reader(2, 0), reader(6, 0)]);
    assert_eq!(dag.region_of_node(0), vec![0, 1, 2, 4, 5]);
    assert_eq!(dag.region_of_node(6), vec![6]);
    assert_eq!(dag.region_of_edge(1, 1), vec![4, 5]);
}

#[test]
fn a_join_over_one_source_on_both_sides_holds_nothing_below_its_probe_edge() {
    let s = shared("s");
    let roots = vec![unload(join(read(&s), read(&s)))];
    let dag = PlanDag::of(&refs(&roots));
    assert_eq!(dag.nodes.len(), 3);
    assert_eq!(dag.nodes[1].children, vec![2, 2]);
    assert_eq!(dag.nodes[2].readers, vec![reader(1, 0), reader(1, 1)]);
    assert_eq!(dag.region_of_edge(1, 1), Vec::<usize>::new());
    assert_eq!(dag.region_of_edge(1, 0), Vec::<usize>::new());
    assert_eq!(dag.region_of_node(1), vec![1, 2]);
}

#[test]
fn only_a_shared_source_has_a_sharing_key() {
    let s = shared("s");
    let (a, b) = (read(&s), read(&s));
    assert!(a.sharing_key().is_some());
    assert_eq!(a.sharing_key(), b.sharing_key());
    assert_ne!(a.sharing_key(), read(&shared("s")).sharing_key());
    assert_eq!(source("s", 1).sharing_key(), None);
}

#[test]
fn equal_loads_are_shared_and_others_are_not() {
    let roots = vec![
        unload(filter(source("s", 1))),
        unload(join(source("s", 1), source("t", 1))),
        unload(project(source("t", 2))),
    ];
    let (roots, shared) = share_sources(roots);
    assert_eq!(shared, 1);
    let dag = PlanDag::of(&refs(&roots));
    let loads = dag
        .nodes
        .iter()
        .filter(|node| node.node.name() == "GpuLoadParquet")
        .count();
    // s once, t at one lane, t at two lanes: the two t's differ in their mapping.
    assert_eq!(loads, 3);
    assert_eq!(dag.nodes[2].readers, vec![reader(1, 0), reader(4, 0)]);
}
