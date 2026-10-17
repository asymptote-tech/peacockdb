//! The C ABI the Python prototype calls through ctypes: flat arrays in, a
//! postfix tree out.

use std::ffi::c_void;

use crate::{JoinTree, Unsolved, solve};

/// The cost of the join producing `set`, asked of the caller with its `ctx`.
pub type SetCost = extern "C" fn(set: u64, ctx: *mut c_void) -> f64;

/// Solves the join order and writes it to `out_tree` in postfix: `r ≥ 0` is relation `r`,
/// `-1 - e` a join of the two trees before it on edge `e`; `*out_len` is how many it wrote.
/// Returns 0 solved, 1 budget of `max_pairs` exhausted, 2 disconnected, -1 a relation count
/// outside `1..=64`, -2 a malformed edge, -3 `out_capacity` below the `2n - 1` a tree takes.
///
/// # Safety
/// `edge_left`/`edge_right` point to `n_edges` values each, `out_tree` to `out_capacity`,
/// `out_len` to one; `set_cost` is safe to call with `ctx` for as long as this call runs.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dphyp_solve(
    n_relations: u32,
    edge_left: *const u64,
    edge_right: *const u64,
    n_edges: u32,
    set_cost: SetCost,
    ctx: *mut c_void,
    max_pairs: u32,
    out_tree: *mut i32,
    out_capacity: u32,
    out_len: *mut u32,
) -> i32 {
    let (lefts, rights) = if n_edges == 0 {
        (&[][..], &[][..])
    } else {
        // SAFETY: the caller passes `n_edges` values behind each pointer.
        unsafe {
            (
                std::slice::from_raw_parts(edge_left, n_edges as usize),
                std::slice::from_raw_parts(edge_right, n_edges as usize),
            )
        }
    };
    let edges: Vec<(u64, u64)> = lefts.iter().copied().zip(rights.iter().copied()).collect();
    if n_relations >= 1 && out_capacity < 2 * n_relations - 1 {
        return -3;
    }
    match solve(
        n_relations as usize,
        &edges,
        |set| set_cost(set, ctx),
        max_pairs as u64,
    ) {
        Ok(solved) => {
            let mut postfix = Vec::with_capacity(2 * n_relations as usize - 1);
            write_postfix(&solved.tree, &mut postfix);
            // SAFETY: `out_capacity` ≥ 2n - 1, the length of any tree over n relations.
            unsafe {
                std::ptr::copy_nonoverlapping(postfix.as_ptr(), out_tree, postfix.len());
                *out_len = postfix.len() as u32;
            }
            0
        }
        Err(Unsolved::BudgetExhausted) => 1,
        Err(Unsolved::Disconnected) => 2,
        Err(Unsolved::RelationCount(_)) => -1,
        Err(Unsolved::Edge(_)) => -2,
    }
}

fn write_postfix(tree: &JoinTree, out: &mut Vec<i32>) {
    match tree {
        JoinTree::Rel(r) => out.push(*r as i32),
        JoinTree::Join { left, right, edge } => {
            write_postfix(left, out);
            write_postfix(right, out);
            out.push(-1 - *edge as i32);
        }
    }
}
