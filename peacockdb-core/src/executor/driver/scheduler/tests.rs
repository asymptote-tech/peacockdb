//! The corners are enumerated rather than sampled, and then a differential test asserts
//! the incremental schedule agrees with a naive rescan on randomized DAG shapes: if the two
//! disagree the incremental one is wrong by definition, which is the only cheap way to
//! keep it honest as the holds grow.

use std::num::NonZeroUsize;

use super::*;
use crate::plan::Reader;

/// A shape from each node's readers, `(parent, slot)`. Heights, regions and probe regions
/// are worked out here from their definitions — the longest path to a root, and the nodes a
/// cut node or edge leaves unreachable from every root — rather than by `PlanDag`'s walk, so
/// the scheduler is tested against numbers derived independently of the index.
fn shape(readers: &[Vec<(usize, usize)>], lanes: &[usize], joins: &[usize]) -> PlanShape {
    let n = readers.len();
    let mut children: Vec<Vec<Option<usize>>> = vec![Vec::new(); n];
    for (child, edges) in readers.iter().enumerate() {
        for &(parent, slot) in edges {
            if children[parent].len() <= slot {
                children[parent].resize(slot + 1, None);
            }
            children[parent][slot] = Some(child);
        }
    }
    let children: Vec<Vec<usize>> = children
        .into_iter()
        .map(|slots| {
            slots
                .into_iter()
                .map(|child| child.expect("every slot has a child"))
                .collect()
        })
        .collect();
    let roots: Vec<usize> = (0..n).filter(|node| readers[*node].is_empty()).collect();

    fn height(node: usize, readers: &[Vec<(usize, usize)>], memo: &mut [Option<u32>]) -> u32 {
        if let Some(height) = memo[node] {
            return height;
        }
        let height = readers[node]
            .iter()
            .map(|(parent, _)| height(*parent, readers, memo) + 1)
            .max()
            .unwrap_or(0);
        memo[node] = Some(height);
        height
    }
    let mut memo = vec![None; n];
    let heights: Vec<u32> = (0..n)
        .map(|node| height(node, readers, &mut memo))
        .collect();

    // The nodes no root reaches once `node`, or the edge `(parent, slot)`, is cut.
    let cut_off = |node: Option<usize>, edge: Option<(usize, usize)>| -> Vec<usize> {
        let mut reached = vec![false; n];
        let mut stack: Vec<usize> = roots
            .iter()
            .copied()
            .filter(|root| Some(*root) != node)
            .collect();
        while let Some(at) = stack.pop() {
            if std::mem::replace(&mut reached[at], true) {
                continue;
            }
            for (slot, child) in children[at].iter().enumerate() {
                if Some(*child) != node && Some((at, slot)) != edge {
                    stack.push(*child);
                }
            }
        }
        (0..n).filter(|at| !reached[*at]).collect()
    };
    PlanShape {
        heights,
        lanes: lanes.to_vec(),
        regions: (0..n).map(|node| cut_off(Some(node), None)).collect(),
        readers: readers
            .iter()
            .map(|edges| {
                edges
                    .iter()
                    .map(|&(parent, slot)| Reader { parent, slot })
                    .collect()
            })
            .collect(),
        joins: joins
            .iter()
            .map(|node| JoinShape {
                node: *node,
                probe: cut_off(None, Some((*node, 1))),
                lanes: lanes[*node],
            })
            .collect(),
        cap_regions: roots
            .iter()
            .map(|root| cut_off(Some(*root), None))
            .collect(),
        roots,
        buffers: vec![false; n],
    }
}

impl Scheduler {
    /// No cap: every root admitted from the start, as a single tree always is.
    fn new(shape: &PlanShape) -> Self {
        Self::with_cap(shape, None)
    }
}

/// A tree from parents in pre-order, each child taking its parent's next slot.
fn tree(parents: &[Option<usize>], lanes: &[usize], joins: &[usize]) -> PlanShape {
    let mut slots = vec![0; parents.len()];
    let readers: Vec<Vec<(usize, usize)>> = parents
        .iter()
        .map(|parent| {
            parent
                .map(|parent| {
                    slots[parent] += 1;
                    (parent, slots[parent] - 1)
                })
                .into_iter()
                .collect()
        })
        .collect();
    shape(&readers, lanes, joins)
}

/// A chain root -> ... -> leaf, one lane each.
fn chain(len: usize) -> PlanShape {
    let parents: Vec<Option<usize>> = (0..len)
        .map(|node| if node == 0 { None } else { Some(node - 1) })
        .collect();
    tree(&parents, &vec![1; len], &[])
}

#[test]
fn nothing_ready_is_how_a_run_ends() {
    let mut scheduler = Scheduler::new(&chain(3));
    assert_eq!(scheduler.next(), None);
    scheduler.set_lane_ready(2, 0, true);
    assert_eq!(scheduler.next(), Some(2));
    scheduler.set_lane_ready(2, 0, false);
    assert_eq!(scheduler.next(), None);
}

#[test]
fn one_node_is_its_own_schedule() {
    let mut scheduler = Scheduler::new(&tree(&[None], &[1], &[]));
    assert_eq!(scheduler.next(), None);
    scheduler.set_lane_ready(0, 0, true);
    assert_eq!(scheduler.next(), Some(0));
}

#[test]
fn the_pick_is_the_smallest_height_among_the_ready() {
    let mut scheduler = Scheduler::new(&chain(4));
    for node in [3, 1, 2] {
        scheduler.set_lane_ready(node, 0, true);
    }
    assert_eq!(scheduler.next(), Some(1), "height 1 beats heights 2 and 3");
}

#[test]
fn ties_at_one_height_break_leftmost() {
    // root(0) over two subtrees: 1 -> 2 and 3 -> 4. Nodes 1 and 3 share a height, and
    // the build-side-drains-first argument is exactly this tie going to 1.
    let mut scheduler = Scheduler::new(&tree(
        &[None, Some(0), Some(1), Some(0), Some(3)],
        &[1; 5],
        &[],
    ));
    scheduler.set_lane_ready(3, 0, true);
    assert_eq!(scheduler.next(), Some(3));
    scheduler.set_lane_ready(1, 0, true);
    assert_eq!(
        scheduler.next(),
        Some(1),
        "equal heights go to the leftmost"
    );
}

#[test]
fn a_node_is_ready_while_any_of_its_lanes_is() {
    let mut scheduler = Scheduler::new(&tree(&[None, Some(0)], &[1, 3], &[]));
    for lane in 0..3 {
        scheduler.set_lane_ready(1, lane, true);
    }
    scheduler.set_lane_ready(1, 0, false);
    scheduler.set_lane_ready(1, 2, false);
    assert_eq!(scheduler.next(), Some(1), "lane 1 can still step");
    scheduler.set_lane_ready(1, 1, false);
    assert_eq!(scheduler.next(), None);
}

#[test]
fn a_lane_reported_ready_twice_is_counted_once() {
    let mut scheduler = Scheduler::new(&tree(&[None, Some(0)], &[1, 2], &[]));
    scheduler.set_lane_ready(1, 0, true);
    scheduler.set_lane_ready(1, 0, true);
    scheduler.set_lane_ready(1, 0, false);
    assert_eq!(
        scheduler.next(),
        None,
        "a repeated report must not leave a phantom ready lane behind"
    );
}

#[test]
fn readiness_comes_and_goes_and_comes_back() {
    let mut scheduler = Scheduler::new(&chain(2));
    scheduler.set_lane_ready(1, 0, true);
    assert_eq!(scheduler.next(), Some(1));
    scheduler.set_lane_ready(1, 0, false);
    assert_eq!(scheduler.next(), None);
    scheduler.set_lane_ready(1, 0, true);
    assert_eq!(scheduler.next(), Some(1));
}

/// unload(0) <- join(1) <- [build 2 <- 3, probe 4 <- 5]
fn one_join(lanes: usize) -> PlanShape {
    tree(
        &[None, Some(0), Some(1), Some(2), Some(1), Some(4)],
        &[lanes; 6],
        &[1],
    )
}

/// `one_join` with a hold buffer on the probe edge: 0 unload, 1 join, 2 coalesce, 3 build
/// source, 4 buffer, 5 filter, 6 probe source. The probe region stops at the buffer.
fn buffered_join() -> PlanShape {
    let mut shape = tree(
        &[None, Some(0), Some(1), Some(2), Some(1), Some(4), Some(5)],
        &[1; 7],
        &[1],
    );
    shape.joins[0].probe = vec![4];
    shape.buffers[4] = true;
    shape
}

#[test]
fn a_held_buffer_stays_runnable_for_intake() {
    let mut scheduler = Scheduler::new(&buffered_join());
    scheduler.set_lane_ready(4, 0, true);
    assert!(scheduler.is_held(4), "the join is building");
    assert_eq!(
        scheduler.next(),
        Some(4),
        "held, and still taking what it is offered"
    );
    scheduler.set_lane_ready(4, 0, false);
    scheduler.set_lane_ready(6, 0, true);
    assert!(!scheduler.is_held(6), "the hold stops at the buffer");
    assert_eq!(scheduler.next(), Some(6));
}

#[test]
fn a_limit_still_holds_a_buffer() {
    let mut scheduler = Scheduler::new(&buffered_join());
    scheduler.set_lane_ready(4, 0, true);
    scheduler.satisfy(0);
    assert_eq!(
        scheduler.next(),
        None,
        "a limit's hold never lifts, so nothing waits for it"
    );
}

#[test]
fn a_lifted_hold_says_so_once() {
    let mut scheduler = Scheduler::new(&one_join(2));
    assert!(!scheduler.lane_left_build(1), "one lane is still building");
    assert!(scheduler.lane_left_build(1), "the last lane lifts the hold");
}

#[test]
fn a_join_holds_its_probe_subtree_from_time_zero() {
    let mut scheduler = Scheduler::new(&one_join(1));
    for node in [3, 5] {
        scheduler.set_lane_ready(node, 0, true);
    }
    assert!(scheduler.is_held(4) && scheduler.is_held(5));
    assert_eq!(
        scheduler.next(),
        Some(3),
        "only the build subtree is schedulable before set_build"
    );
    scheduler.set_lane_ready(3, 0, false);
    assert_eq!(
        scheduler.next(),
        None,
        "the probe side has a batch to make and is still held"
    );
    scheduler.lane_left_build(1);
    assert_eq!(scheduler.next(), Some(5), "the probe subtree is free now");
}

#[test]
fn the_hold_lifts_only_when_every_lane_has_left_build() {
    let mut scheduler = Scheduler::new(&one_join(3));
    scheduler.set_lane_ready(5, 0, true);
    scheduler.lane_left_build(1);
    scheduler.lane_left_build(1);
    assert_eq!(scheduler.next(), None, "one lane is still building");
    scheduler.lane_left_build(1);
    assert_eq!(scheduler.next(), Some(5));
}

#[test]
fn a_node_under_two_builds_stays_held_when_the_inner_one_lifts() {
    // unload(0) <- outer join(1) <- [build 2, probe 3 = inner join <- [build 4, probe 5]]
    let inner_probe_holder = 5;
    let mut scheduler = Scheduler::new(&tree(
        &[None, Some(0), Some(1), Some(1), Some(3), Some(3)],
        &[1; 6],
        &[1, 3],
    ));
    scheduler.set_lane_ready(inner_probe_holder, 0, true);
    assert!(scheduler.is_held(inner_probe_holder));
    scheduler.lane_left_build(3);
    assert!(
        scheduler.is_held(inner_probe_holder),
        "the outer join's hold is a second count, not the same one"
    );
    assert_eq!(scheduler.next(), None);
    scheduler.lane_left_build(1);
    assert_eq!(scheduler.next(), Some(inner_probe_holder));
}

#[test]
fn a_join_inside_another_joins_build_subtree_is_not_held() {
    // unload(0) <- outer(1) <- [build 2 = inner join <- [3, 4], probe 5]
    let mut scheduler = Scheduler::new(&tree(
        &[None, Some(0), Some(1), Some(2), Some(2), Some(1)],
        &[1; 6],
        &[1, 2],
    ));
    scheduler.set_lane_ready(3, 0, true);
    assert!(!scheduler.is_held(3), "a build subtree is never held");
    assert_eq!(scheduler.next(), Some(3));
}

/// Root A: unload(0) <- join(1) <- [build 2, filter 3 <- S 4]; root B: unload(5) <- S 4.
fn a_probe_source_another_root_reads() -> PlanShape {
    shape(
        &[
            vec![],
            vec![(0, 0)],
            vec![(1, 0)],
            vec![(1, 1)],
            vec![(3, 0), (5, 0)],
            vec![],
        ],
        &[1; 6],
        &[1],
    )
}

#[test]
fn a_hold_covers_only_what_reaches_a_root_through_the_held_edge() {
    let (filter, s) = (3, 4);
    let shape = a_probe_source_another_root_reads();
    assert_eq!(shape.joins[0].probe, vec![filter]);
    let mut scheduler = Scheduler::new(&shape);
    scheduler.set_lane_ready(s, 0, true);
    scheduler.set_lane_ready(filter, 0, true);
    assert!(
        scheduler.is_held(filter),
        "the filter reaches a root only through the probe edge"
    );
    assert!(
        !scheduler.is_held(s),
        "root B reads S directly, so the join's build cannot hold it"
    );
    assert_eq!(scheduler.next(), Some(s));
}

#[test]
fn a_satisfied_root_holds_only_its_own_region() {
    let (filter, s, b) = (3, 4, 5);
    let mut scheduler = Scheduler::new(&a_probe_source_another_root_reads());
    scheduler.set_lane_ready(s, 0, true);
    scheduler.set_lane_ready(b, 0, true);
    scheduler.satisfy(0);
    assert!(scheduler.is_held(filter));
    assert!(!scheduler.is_held(s) && !scheduler.is_held(b));
    assert_eq!(
        scheduler.next(),
        Some(b),
        "root B still runs, smallest height first"
    );
}

/// Root A: unload(0) <- filter(1) <- S(2); root B: unload(3) <- S(2). S's readers are the
/// filter, ordinal 0, and B, ordinal 1.
fn two_roots_over_a_source() -> PlanShape {
    shape(
        &[vec![], vec![(0, 0)], vec![(1, 0), (3, 0)], vec![]],
        &[1; 4],
        &[],
    )
}

#[test]
fn satisfying_a_consumer_retires_its_reader_and_a_source_with_none_left_is_satisfied() {
    let s = 2;
    let mut scheduler = Scheduler::new(&two_roots_over_a_source());
    scheduler.set_lane_ready(s, 0, true);
    assert_eq!(
        scheduler.satisfy(0),
        vec![(s, 0)],
        "A's filter reads S from outside A's region"
    );
    assert!(!scheduler.is_satisfied(s) && !scheduler.is_held(s));
    assert_eq!(scheduler.next(), Some(s), "B still reads it");
    assert_eq!(scheduler.satisfy(3), vec![(s, 1)]);
    assert!(
        scheduler.is_satisfied(s),
        "no reader of S is left, so S is satisfied in turn"
    );
    assert_eq!(scheduler.next(), None, "and its hold stops it");
}

#[test]
fn a_satisfied_tree_returns_no_reader() {
    let mut scheduler = Scheduler::new(&chain(3));
    assert_eq!(scheduler.satisfy(1), vec![]);
    let mut scheduler = Scheduler::new(&one_join(1));
    assert_eq!(scheduler.satisfy(0), vec![]);
    assert!(
        !scheduler.is_satisfied(5),
        "only the node itself is satisfied"
    );
}

/// Three roots over one source: unload(0) <- filter(1) <- S(2), unload(3) <- filter(4) <- S,
/// unload(5) <- filter(6) <- S.
fn three_roots_over_a_source() -> PlanShape {
    shape(
        &[
            vec![],
            vec![(0, 0)],
            vec![(1, 0), (4, 0), (6, 0)],
            vec![],
            vec![(3, 0)],
            vec![],
            vec![(5, 0)],
        ],
        &[1; 7],
        &[],
    )
}

fn one() -> Option<NonZeroUsize> {
    NonZeroUsize::new(1)
}

#[test]
fn a_cap_holds_the_regions_of_roots_past_it() {
    let mut scheduler = Scheduler::with_cap(&three_roots_over_a_source(), one());
    for node in [1, 2, 4, 6] {
        scheduler.set_lane_ready(node, 0, true);
    }
    assert!(scheduler.is_held(3) && scheduler.is_held(4), "root 1 waits");
    assert!(scheduler.is_held(5) && scheduler.is_held(6), "root 2 waits");
    assert!(!scheduler.is_held(1), "root 0 is admitted");
    assert!(
        !scheduler.is_held(2),
        "a shared source is in no root's region"
    );
    assert_eq!(scheduler.next(), Some(1));
}

#[test]
fn finishing_a_root_admits_the_next() {
    let mut scheduler = Scheduler::with_cap(&three_roots_over_a_source(), one());
    scheduler.set_lane_ready(4, 0, true);
    scheduler.set_lane_ready(6, 0, true);
    assert_eq!(scheduler.next(), None);
    assert_eq!(scheduler.root_finished(0), vec![3, 4], "root 1's region");
    assert_eq!(scheduler.next(), Some(4));
    assert!(scheduler.is_held(6), "root 2 still waits on root 1");
    assert!(
        scheduler.root_finished(0).is_empty(),
        "a root finishes once"
    );
    assert_eq!(scheduler.root_finished(3), vec![5, 6]);
    assert!(!scheduler.is_held(6));
}

#[test]
fn a_cap_hold_stops_at_a_buffer() {
    // unload(0) <- filter(1) <- S(2); unload(3) <- buffer(4) <- filter(5) <- S.
    let mut shape = shape(
        &[
            vec![],
            vec![(0, 0)],
            vec![(1, 0), (5, 0)],
            vec![],
            vec![(3, 0)],
            vec![(4, 0)],
        ],
        &[1; 6],
        &[],
    );
    shape.buffers[4] = true;
    shape.cap_regions[1] = vec![3, 4];
    let mut scheduler = Scheduler::with_cap(&shape, one());
    scheduler.set_lane_ready(4, 0, true);
    scheduler.set_lane_ready(5, 0, true);
    assert!(scheduler.is_held(4) && !scheduler.is_held(5));
    assert_eq!(
        scheduler.next(),
        Some(4),
        "held by the cap, and still taking what it is offered"
    );
    scheduler.set_lane_ready(4, 0, false);
    assert_eq!(scheduler.next(), Some(5), "the filter below it runs");
}

#[test]
fn a_satisfied_limit_and_a_join_hold_the_same_node_independently() {
    let mut scheduler = Scheduler::new(&one_join(1));
    scheduler.set_lane_ready(5, 0, true);
    scheduler.satisfy(4);
    scheduler.lane_left_build(1);
    assert!(
        scheduler.is_held(5),
        "the join's hold lifted; the limit's did not"
    );
    assert_eq!(scheduler.next(), None);
}

#[test]
fn a_limit_of_zero_is_satisfied_before_a_single_step() {
    let mut scheduler = Scheduler::new(&chain(3));
    for node in 0..3 {
        scheduler.set_lane_ready(node, 0, true);
    }
    scheduler.satisfy(0);
    assert!(scheduler.any_satisfied());
    assert_eq!(
        scheduler.next(),
        None,
        "a satisfied root holds the whole plan, itself included"
    );
}

#[test]
fn satisfying_twice_holds_once() {
    let mut scheduler = Scheduler::new(&chain(3));
    scheduler.set_lane_ready(2, 0, true);
    scheduler.satisfy(1);
    scheduler.satisfy(1);
    assert!(scheduler.is_held(2));
    // Nothing lifts a limit's hold, so the check that a second satisfy did not double the
    // counter is that the node is held by exactly one: drop it once and it is free.
    scheduler.limit_holds[2] -= 1;
    scheduler.refresh(2);
    assert_eq!(scheduler.next(), Some(2));
}

// -- the differential test ---------------------------------------------------------

/// The predicate rewritten as the prototype states it: rescan every node, walk both hold
/// relations from scratch. Slow and obviously correct, which is the whole point.
struct Naive {
    shape: PlanShape,
    lane_ready: Vec<Vec<bool>>,
    building: Vec<usize>,
    satisfied: Vec<bool>,
}

impl Naive {
    fn new(shape: &PlanShape) -> Self {
        Self {
            lane_ready: shape.lanes.iter().map(|n| vec![false; *n]).collect(),
            building: shape.joins.iter().map(|join| join.lanes).collect(),
            satisfied: vec![false; shape.node_count()],
            shape: shape.clone(),
        }
    }

    /// A buffer is never masked by a build hold, which only changes what it does.
    fn held(&self, node: usize) -> bool {
        let building = self
            .shape
            .joins
            .iter()
            .zip(&self.building)
            .any(|(join, left)| *left > 0 && join.probe.contains(&node));
        let satisfied = self.satisfied_in_turn();
        let satisfied = (0..self.shape.node_count())
            .any(|other| satisfied[other] && self.shape.regions[other].contains(&node));
        (building && !self.shape.buffers[node]) || satisfied
    }

    /// The satisfied nodes and, to a fixed point, every node all of whose readers sit in a
    /// satisfied region that leaves the node itself outside.
    fn satisfied_in_turn(&self) -> Vec<bool> {
        let regions = &self.shape.regions;
        let mut satisfied = self.satisfied.clone();
        loop {
            let retired = |producer: usize, parent: usize, satisfied: &[bool]| {
                (0..satisfied.len()).any(|other| {
                    satisfied[other]
                        && regions[other].contains(&parent)
                        && !regions[other].contains(&producer)
                })
            };
            let next = (0..satisfied.len()).find(|producer| {
                let readers = &self.shape.readers[*producer];
                !satisfied[*producer]
                    && !readers.is_empty()
                    && readers
                        .iter()
                        .all(|reader| retired(*producer, reader.parent, &satisfied))
            });
            match next {
                Some(producer) => satisfied[producer] = true,
                None => return satisfied,
            }
        }
    }

    fn pick(&self) -> Option<usize> {
        (0..self.shape.node_count())
            .filter(|node| self.lane_ready[*node].iter().any(|ready| *ready))
            .filter(|node| !self.held(*node))
            .min_by_key(|node| (self.shape.heights[*node], *node))
    }
}

/// xorshift64*, seeded and fixed: the determinism rules apply to tests too.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    fn below(&mut self, bound: usize) -> usize {
        (self.next() % bound as u64) as usize
    }
}

/// One or two trees grown by expansion, so each is numbered in pre-order as `PlanIndex`
/// numbers it, then extra reader edges onto leaves from nodes that already have children: the
/// shared reads a DAG adds, and never a cycle, since a leaf gains no child. Joins are drawn
/// among the nodes left with two children; a node reading one leaf twice can be one. A node
/// with one child is a buffer one time in four: the scheduler reads only the flag.
fn random_shape(rng: &mut Rng, size: usize) -> PlanShape {
    fn grow(
        rng: &mut Rng,
        parents: &mut Vec<Option<usize>>,
        parent: Option<usize>,
        left: &mut usize,
    ) {
        let me = parents.len();
        parents.push(parent);
        *left -= 1;
        let children = if *left == 0 { 0 } else { rng.below(3) };
        for _ in 0..children {
            if *left == 0 {
                break;
            }
            grow(rng, parents, Some(me), left);
        }
    }

    let mut parents = Vec::new();
    for _ in 0..1 + rng.below(2) {
        let mut left = size;
        grow(rng, &mut parents, None, &mut left);
    }
    let n = parents.len();
    let mut children: Vec<usize> = vec![0; n];
    let mut readers: Vec<Vec<(usize, usize)>> = vec![Vec::new(); n];
    for (node, parent) in parents.iter().enumerate() {
        if let Some(parent) = parent {
            readers[node].push((*parent, children[*parent]));
            children[*parent] += 1;
        }
    }
    let (inner, leaves): (Vec<usize>, Vec<usize>) = (0..n).partition(|node| children[*node] > 0);
    if !inner.is_empty() {
        for leaf in leaves {
            if rng.below(3) == 0 {
                let parent = inner[rng.below(inner.len())];
                readers[leaf].push((parent, children[parent]));
                children[parent] += 1;
            }
        }
    }
    let lanes: Vec<usize> = (0..n).map(|_| 1 + rng.below(3)).collect();
    let joins: Vec<usize> = (0..n)
        .filter(|node| children[*node] == 2 && rng.below(2) == 0)
        .collect();
    let mut shape = shape(&readers, &lanes, &joins);
    for node in 0..n {
        shape.buffers[node] = children[node] == 1 && rng.below(4) == 0;
    }
    shape
}

/// Every region a shape declares is dominated: a member other than its node is read only from
/// inside the region, or, for a probe region, through the held edge itself. And every reader is
/// strictly lower than what it reads, which is what the heights are for.
fn assert_regions_are_dominated(shape: &PlanShape) {
    let read_from_inside = |member: usize, region: &[usize], edge: Option<Reader>| {
        shape.readers[member]
            .iter()
            .all(|reader| Some(*reader) == edge || region.contains(&reader.parent))
    };
    for node in 0..shape.node_count() {
        for reader in &shape.readers[node] {
            assert!(shape.heights[reader.parent] < shape.heights[node]);
        }
        let region = &shape.regions[node];
        assert!(region.contains(&node), "a region holds its own node");
        for member in region.iter().filter(|member| **member != node) {
            assert!(
                read_from_inside(*member, region, None),
                "node {member} is in node {node}'s region and is read from outside it"
            );
        }
    }
    for join in &shape.joins {
        let edge = Reader {
            parent: join.node,
            slot: 1,
        };
        for member in &join.probe {
            assert!(
                read_from_inside(*member, &join.probe, Some(edge)),
                "node {member} is in join {}'s probe region and is read from outside it",
                join.node
            );
        }
    }
}

#[test]
fn the_incremental_schedule_picks_what_a_full_rescan_would() {
    let mut rng = Rng(0x5eed_1234_9876_4321);
    for _ in 0..200 {
        // Growth stops when a node draws no children, so the tree is at most this big and
        // every index below comes off what it actually produced.
        let budget = 1 + rng.below(12);
        let shape = random_shape(&mut rng, budget);
        assert_regions_are_dominated(&shape);
        let size = shape.node_count();
        let mut scheduler = Scheduler::new(&shape);
        let mut naive = Naive::new(&shape);
        assert_eq!(scheduler.next(), naive.pick());
        for _ in 0..80 {
            let node = rng.below(size);
            match rng.below(6) {
                0..=3 => {
                    let lane = rng.below(shape.lanes[node]);
                    let ready = rng.below(2) == 0;
                    scheduler.set_lane_ready(node, lane, ready);
                    naive.lane_ready[node][lane] = ready;
                }
                4 => {
                    let join = shape.joins.iter().position(|join| join.node == node);
                    if let Some(index) = join.filter(|index| naive.building[*index] > 0) {
                        scheduler.lane_left_build(node);
                        naive.building[index] -= 1;
                    }
                }
                _ => {
                    scheduler.satisfy(node);
                    naive.satisfied[node] = true;
                }
            }
            assert_eq!(
                scheduler.next(),
                naive.pick(),
                "the two schedules disagree on shape {shape:?}"
            );
        }
    }
}
