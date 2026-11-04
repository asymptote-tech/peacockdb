//! The enumeration itself, as the paper's four procedures. Relations are ordered by index; a
//! connected subgraph is grown only from its lowest relation, into higher ones, and its
//! complement only from relations above that lowest one — which is what makes each pair appear
//! once. A set has a plan in `plans` exactly when it is connected, so that lookup is also the
//! connectivity test.

use std::collections::HashMap;

use crate::{JoinTree, RelSet, Solved, Unsolved};

#[derive(Clone, Copy)]
struct Plan {
    cost: f64,
    /// `(left, right, edge)` for a join; `None` for a relation.
    split: Option<(RelSet, RelSet, u32)>,
}

pub(crate) struct Dp<F> {
    n: usize,
    /// Every edge in both directions: `(from, to, edge index)`.
    directed: Vec<(RelSet, RelSet, u32)>,
    set_cost: F,
    costs: HashMap<RelSet, f64>,
    plans: HashMap<RelSet, Plan>,
    max_pairs: u64,
    pairs: u64,
}

impl<F: FnMut(RelSet) -> f64> Dp<F> {
    pub(crate) fn new(n: usize, edges: &[(RelSet, RelSet)], set_cost: F, max_pairs: u64) -> Self {
        let directed = edges
            .iter()
            .enumerate()
            .flat_map(|(at, &(left, right))| [(left, right, at as u32), (right, left, at as u32)])
            .collect();
        Dp {
            n,
            directed,
            set_cost,
            costs: HashMap::new(),
            plans: HashMap::new(),
            max_pairs,
            pairs: 0,
        }
    }

    pub(crate) fn solve(mut self) -> Result<Solved, Unsolved> {
        for r in 0..self.n {
            self.plans.insert(
                1 << r,
                Plan {
                    cost: 0.0,
                    split: None,
                },
            );
        }
        for r in (0..self.n).rev() {
            self.emit_csg(1 << r);
            self.enumerate_csg_rec(1 << r, below_or_at(r));
            if self.exhausted() {
                return Err(Unsolved::BudgetExhausted);
            }
        }
        let all = if self.n == 64 {
            RelSet::MAX
        } else {
            (1 << self.n) - 1
        };
        let Some(plan) = self.plans.get(&all).copied() else {
            return Err(Unsolved::Disconnected);
        };
        Ok(Solved {
            tree: self.tree(all),
            cost: plan.cost,
            pairs: self.pairs,
        })
    }

    fn exhausted(&self) -> bool {
        self.pairs > self.max_pairs
    }

    /// The paper's N(S, X): for each edge leaving `set` into relations outside `set ∪ excluded`,
    /// its lowest relation stands for its far side.
    fn neighborhood(&self, set: RelSet, excluded: RelSet) -> RelSet {
        let mut found = 0;
        for &(from, to, _) in &self.directed {
            if from & !set == 0 && to & (set | excluded) == 0 {
                found |= to.isolate_lowest_one();
            }
        }
        found
    }

    /// An edge with one side inside `left` and the other inside `right`.
    fn edge_between(&self, left: RelSet, right: RelSet) -> Option<u32> {
        self.directed
            .iter()
            .find(|&&(from, to, _)| from & !left == 0 && to & !right == 0)
            .map(|e| e.2)
    }

    fn enumerate_csg_rec(&mut self, set: RelSet, excluded: RelSet) {
        let neighbors = self.neighborhood(set, excluded);
        for grown in submasks(neighbors) {
            if self.exhausted() {
                return;
            }
            if self.plans.contains_key(&(set | grown)) {
                self.emit_csg(set | grown);
            }
        }
        for grown in submasks(neighbors) {
            self.enumerate_csg_rec(set | grown, excluded | neighbors);
        }
    }

    /// Every complement `set` can join: grown from each neighbor above `set`'s lowest relation.
    fn emit_csg(&mut self, set: RelSet) {
        let excluded = set | below_or_at(set.trailing_zeros() as usize);
        let neighbors = self.neighborhood(set, excluded);
        for r in (0..self.n).rev().filter(|&r| neighbors & (1 << r) != 0) {
            let other = 1 << r;
            if let Some(edge) = self.edge_between(set, other) {
                self.emit_csg_cmp(set, other, edge);
            }
            self.enumerate_cmp_rec(set, other, excluded | (neighbors & below_or_at(r)));
        }
    }

    fn enumerate_cmp_rec(&mut self, set: RelSet, other: RelSet, excluded: RelSet) {
        let neighbors = self.neighborhood(other, excluded);
        for grown in submasks(neighbors) {
            if self.exhausted() {
                return;
            }
            if self.plans.contains_key(&(other | grown))
                && let Some(edge) = self.edge_between(set, other | grown)
            {
                self.emit_csg_cmp(set, other | grown, edge);
            }
        }
        for grown in submasks(neighbors) {
            self.enumerate_cmp_rec(set, other | grown, excluded | neighbors);
        }
    }

    fn emit_csg_cmp(&mut self, left: RelSet, right: RelSet, edge: u32) {
        self.pairs += 1;
        if self.exhausted() {
            return;
        }
        let joined = left | right;
        let own = match self.costs.get(&joined) {
            Some(&cost) => cost,
            None => {
                let cost = (self.set_cost)(joined);
                self.costs.insert(joined, cost);
                cost
            }
        };
        let cost = own + self.plans[&left].cost + self.plans[&right].cost;
        if self.plans.get(&joined).is_none_or(|plan| cost < plan.cost) {
            self.plans.insert(
                joined,
                Plan {
                    cost,
                    split: Some((left, right, edge)),
                },
            );
        }
    }

    fn tree(&self, set: RelSet) -> JoinTree {
        match self.plans[&set].split {
            None => JoinTree::Rel(set.trailing_zeros()),
            Some((left, right, edge)) => JoinTree::Join {
                left: Box::new(self.tree(left)),
                right: Box::new(self.tree(right)),
                edge,
            },
        }
    }
}

/// Relations `0..=r`.
fn below_or_at(r: usize) -> RelSet {
    if r >= 63 {
        RelSet::MAX
    } else {
        (1 << (r + 1)) - 1
    }
}

/// Every non-empty subset of `set`, smaller values first — so a subset always precedes the sets
/// that contain it.
fn submasks(set: RelSet) -> impl Iterator<Item = RelSet> {
    let mut current: RelSet = 0;
    std::iter::from_fn(move || {
        current = current.wrapping_sub(set) & set;
        (current != 0).then_some(current)
    })
}
