//! DPhyp against a brute-force DP over every split of every connected set, on the paper's four
//! graph shapes and on hypergraphs, and its pair count against the paper's closed forms.

use std::collections::HashMap;
use std::ffi::c_void;

use crate::{JoinTree, RelSet, Unsolved, dphyp_solve, solve};

fn chain(n: usize) -> Vec<(RelSet, RelSet)> {
    (1..n).map(|r| (1 << (r - 1), 1 << r)).collect()
}

fn star(n: usize) -> Vec<(RelSet, RelSet)> {
    (1..n).map(|r| (1, 1 << r)).collect()
}

fn clique(n: usize) -> Vec<(RelSet, RelSet)> {
    (0..n)
        .flat_map(|a| (a + 1..n).map(move |b| (1 << a, 1 << b)))
        .collect()
}

fn cycle(n: usize) -> Vec<(RelSet, RelSet)> {
    let mut edges = chain(n);
    edges.push((1 << (n - 1), 1));
    edges
}

/// A deterministic cost per set, varied enough that ties are rare.
fn cost_of(seed: u64) -> impl FnMut(RelSet) -> f64 {
    move |set| {
        let mut x = set.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ seed;
        x ^= x >> 29;
        (x % 1000) as f64 + 1.0
    }
}

/// The cheapest plan by trying every split of every set, smallest sets first.
fn brute_force(
    n: usize,
    edges: &[(RelSet, RelSet)],
    mut cost: impl FnMut(RelSet) -> f64,
) -> Option<f64> {
    let joins = |left: RelSet, right: RelSet| {
        edges.iter().any(|&(a, b)| {
            (a & !left == 0 && b & !right == 0) || (b & !left == 0 && a & !right == 0)
        })
    };
    let mut best: HashMap<RelSet, f64> = (0..n).map(|r| (1 << r, 0.0)).collect();
    let all: RelSet = (1 << n) - 1;
    let mut sets: Vec<RelSet> = (1..=all).filter(|s| s.count_ones() > 1).collect();
    sets.sort_by_key(|s| s.count_ones());
    for set in sets {
        let mut left = (set - 1) & set;
        while left != 0 {
            let right = set & !left;
            if let (Some(&a), Some(&b)) = (best.get(&left), best.get(&right))
                && joins(left, right)
            {
                let candidate = cost(set) + a + b;
                if best.get(&set).is_none_or(|&have| candidate < have) {
                    best.insert(set, candidate);
                }
            }
            left = (left - 1) & set;
        }
    }
    best.get(&all).copied()
}

fn cost_of_tree(tree: &JoinTree, cost: &mut impl FnMut(RelSet) -> f64) -> f64 {
    match tree {
        JoinTree::Rel(_) => 0.0,
        JoinTree::Join { left, right, .. } => {
            cost(tree.relations()) + cost_of_tree(left, cost) + cost_of_tree(right, cost)
        }
    }
}

#[test]
fn dphyp_finds_the_brute_force_optimum_on_every_shape_up_to_six_relations() {
    for n in 2..=6 {
        for (name, edges) in [
            ("chain", chain(n)),
            ("star", star(n)),
            ("clique", clique(n)),
            ("cycle", cycle(n)),
        ] {
            if name == "cycle" && n < 3 {
                continue;
            }
            for seed in 0..20 {
                let solved = solve(n, &edges, cost_of(seed), u64::MAX).unwrap();
                let optimum = brute_force(n, &edges, cost_of(seed)).unwrap();
                assert_eq!(solved.cost, optimum, "{name} n={n} seed={seed}");
                assert_eq!(
                    cost_of_tree(&solved.tree, &mut cost_of(seed)),
                    solved.cost,
                    "{name} n={n}"
                );
                assert_eq!(solved.tree.relations(), (1 << n) - 1);
            }
        }
    }
}

#[test]
fn a_hyperedge_joins_only_once_both_its_sides_are_there() {
    // 0–1 and 2–3 by simple edges, the two pairs only by a hyperedge {0,1}–{2,3}: no plan may
    // join 0 or 1 alone with 2 or 3.
    let edges = [(0b0001, 0b0010), (0b0100, 0b1000), (0b0011, 0b1100)];
    for seed in 0..20 {
        let solved = solve(4, &edges, cost_of(seed), u64::MAX).unwrap();
        assert_eq!(Some(solved.cost), brute_force(4, &edges, cost_of(seed)));
        let JoinTree::Join { left, right, edge } = &solved.tree else {
            panic!()
        };
        assert_eq!(*edge, 2);
        let mut sides = [left.relations(), right.relations()];
        sides.sort();
        assert_eq!(sides, [0b0011, 0b1100]);
    }
    // A chain 0–1–2 plus a hyperedge {0,2}–{3}: relation 3 joins only a set holding 0 and 2.
    let edges = [(0b0001, 0b0010), (0b0010, 0b0100), (0b0101, 0b1000)];
    for seed in 0..20 {
        assert_eq!(
            Some(solve(4, &edges, cost_of(seed), u64::MAX).unwrap().cost),
            brute_force(4, &edges, cost_of(seed))
        );
    }
}

#[test]
fn each_connected_pair_is_enumerated_once_as_the_paper_counts() {
    // Moerkotte & Neumann 2006, "Analysis of two existing and one new dynamic programming
    // algorithm": the csg-cmp pairs of chain, star, cycle and clique, unordered.
    for n in 2..=12u64 {
        let pairs = |edges: Vec<(RelSet, RelSet)>| {
            solve(n as usize, &edges, |_| 1.0, u64::MAX).unwrap().pairs
        };
        assert_eq!(pairs(chain(n as usize)), (n * n * n - n) / 6, "chain {n}");
        assert_eq!(pairs(star(n as usize)), (n - 1) << (n - 2), "star {n}");
        assert_eq!(
            pairs(clique(n as usize)),
            (3u64.pow(n as u32) + 1 - (1 << (n + 1))) / 2,
            "clique {n}"
        );
        if n >= 3 {
            assert_eq!(
                pairs(cycle(n as usize)),
                (n * n * n - 2 * n * n + n) / 2,
                "cycle {n}"
            );
        }
    }
}

#[test]
fn a_budget_stops_the_enumeration_and_says_so() {
    assert_eq!(
        solve(14, &clique(14), |_| 1.0, 10_000),
        Err(Unsolved::BudgetExhausted)
    );
    assert!(solve(14, &chain(14), |_| 1.0, 10_000).is_ok());
}

#[test]
fn malformed_input_is_refused() {
    assert_eq!(solve(0, &[], |_| 1.0, 10), Err(Unsolved::RelationCount(0)));
    assert_eq!(
        solve(65, &[], |_| 1.0, 10),
        Err(Unsolved::RelationCount(65))
    );
    assert_eq!(
        solve(3, &[(0b001, 0b010), (0b001, 0b1000)], |_| 1.0, 10),
        Err(Unsolved::Edge(1))
    );
    assert_eq!(
        solve(3, &[(0b011, 0b010)], |_| 1.0, 10),
        Err(Unsolved::Edge(0))
    );
    assert_eq!(
        solve(3, &[(0b001, 0b010)], |_| 1.0, 10),
        Err(Unsolved::Disconnected)
    );
    assert_eq!(solve(1, &[], |_| 1.0, 10).unwrap().tree, JoinTree::Rel(0));
}

extern "C" fn by_popcount(set: u64, ctx: *mut c_void) -> f64 {
    // SAFETY: the test passes a `u32` counter as `ctx`.
    unsafe { *(ctx as *mut u32) += 1 };
    set.count_ones() as f64
}

#[test]
fn the_c_abi_writes_the_tree_in_postfix() {
    let (lefts, rights): (Vec<u64>, Vec<u64>) = chain(3).into_iter().unzip();
    let mut asked = 0u32;
    let mut tree = [0i32; 5];
    let mut len = 0u32;
    // SAFETY: the arrays are the lengths passed, and `asked` outlives the call.
    let code = unsafe {
        dphyp_solve(
            3,
            lefts.as_ptr(),
            rights.as_ptr(),
            2,
            by_popcount,
            &mut asked as *mut u32 as *mut c_void,
            1000,
            tree.as_mut_ptr(),
            5,
            &mut len,
        )
    };
    assert_eq!((code, len), (0, 5));
    // Read back as a stack machine: one tree over all three relations, each join on an edge of
    // the chain. By popcount the left- and right-deep trees tie, so either may come back.
    let mut stack: Vec<RelSet> = Vec::new();
    for &x in &tree {
        if x >= 0 {
            stack.push(1 << x);
        } else {
            let (right, left) = (stack.pop().unwrap(), stack.pop().unwrap());
            assert!((-1 - x) < 2 && left & right == 0);
            stack.push(left | right);
        }
    }
    assert_eq!(stack, [0b111]);
    assert_eq!(asked, 3); // {0,1}, {1,2} and {0,1,2}, each once
    let mut small = [0i32; 4];
    // SAFETY: as above; the capacity is the one refused.
    let short = unsafe {
        dphyp_solve(
            3,
            lefts.as_ptr(),
            rights.as_ptr(),
            2,
            by_popcount,
            &mut asked as *mut u32 as *mut c_void,
            1000,
            small.as_mut_ptr(),
            4,
            &mut len,
        )
    };
    assert_eq!(short, -3);
}
