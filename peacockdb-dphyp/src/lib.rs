//! DPhyp (Moerkotte & Neumann, "Dynamic Programming Strikes Back", 2008): the cheapest bushy join
//! tree over a hypergraph of relations, each connected subgraph and connected complement it can
//! join to enumerated once.
//!
//! Relations are `0..n`, `n` ≤ 64; an edge is a pair of relation masks, a hyperedge where either
//! has more than one. The crate holds no estimates: the cost of joining a set of relations is
//! asked of the caller, once per set, and a tree costs the sum over its joins — C_out. Which side
//! of a join builds is not decided here; a join names one edge between its sides, and the rest
//! are the caller's to find.

mod enumerate;
mod ffi;
#[cfg(test)]
mod tests;

pub use ffi::dphyp_solve;

/// A set of relations, bit `r` for relation `r`.
pub type RelSet = u64;

/// A join order. `Join` names one edge between its sides; which side builds is the caller's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JoinTree {
    Rel(u32),
    Join {
        left: Box<JoinTree>,
        right: Box<JoinTree>,
        edge: u32,
    },
}

impl JoinTree {
    pub fn relations(&self) -> RelSet {
        match self {
            JoinTree::Rel(r) => 1 << r,
            JoinTree::Join { left, right, .. } => left.relations() | right.relations(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Solved {
    pub tree: JoinTree,
    /// Σ `set_cost` over the tree's joins.
    pub cost: f64,
    /// The connected-subgraph / complement pairs enumerated.
    pub pairs: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Unsolved {
    /// More pairs than `max_pairs`: the graph is too big to enumerate exactly.
    BudgetExhausted,
    /// No edge path joins every relation.
    Disconnected,
    /// `n` is 0 or more than 64.
    RelationCount(usize),
    /// Edge `.0` has an empty side, a relation outside `0..n`, or one on both sides.
    Edge(usize),
}

/// The cheapest tree over `n` relations joined by `edges`. `set_cost(set)` is the cost of the join
/// producing `set`; it is asked once per set DPhyp reaches. Enumeration stops after `max_pairs`
/// pairs.
pub fn solve(
    n: usize,
    edges: &[(RelSet, RelSet)],
    set_cost: impl FnMut(RelSet) -> f64,
    max_pairs: u64,
) -> Result<Solved, Unsolved> {
    if n == 0 || n > 64 {
        return Err(Unsolved::RelationCount(n));
    }
    let all = if n == 64 { RelSet::MAX } else { (1 << n) - 1 };
    for (at, &(left, right)) in edges.iter().enumerate() {
        if left == 0 || right == 0 || left & right != 0 || (left | right) & !all != 0 {
            return Err(Unsolved::Edge(at));
        }
    }
    enumerate::Dp::new(n, edges, set_cost, max_pairs).solve()
}
