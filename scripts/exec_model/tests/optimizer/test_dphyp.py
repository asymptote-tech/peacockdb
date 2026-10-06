"""DPhyp from Python through the C ABI: the same optimum a brute force over every split finds, a
clique of 14 in under a second, and a budget that says so — which also checks the arrays cross
the boundary laid out as the crate reads them."""

from __future__ import annotations

if __package__ in (None, ""):  # allow `python scripts/exec_model/tests/optimizer/<file>.py`
    import pathlib as _pathlib, sys as _sys

    _sys.path.insert(0, str(_pathlib.Path(__file__).resolve().parents[4]))
    __package__ = "scripts.exec_model.tests.optimizer"

import time

from ..harness import main, raises
from ...optimizer.dphyp import Unsolved, solve


def chain(n):
    return [(1 << (r - 1), 1 << r) for r in range(1, n)]


def star(n):
    return [(1, 1 << r) for r in range(1, n)]


def clique(n):
    return [(1 << a, 1 << b) for a in range(n) for b in range(a + 1, n)]


def cycle(n):
    return chain(n) + [(1 << (n - 1), 1)]


def cost_of(seed):
    def cost(mask):
        x = (mask * 0x9E3779B97F4A7C15 ^ seed) & (2**64 - 1)
        return float((x ^ (x >> 29)) % 1000 + 1)
    return cost


def brute_force(n, edges, cost):
    def joins(left, right):
        return any((a & ~left == 0 and b & ~right == 0) or (b & ~left == 0 and a & ~right == 0) for a, b in edges)
    best = {1 << r: 0.0 for r in range(n)}
    for mask in sorted(range(1, 1 << n), key=lambda m: bin(m).count("1")):
        left = (mask - 1) & mask
        while left:
            right = mask & ~left
            if left in best and right in best and joins(left, right):
                candidate = cost(mask) + best[left] + best[right]
                best[mask] = min(best.get(mask, candidate), candidate)
            left = (left - 1) & mask
    return best.get((1 << n) - 1)


def tree_cost(tree, cost):
    if isinstance(tree, int):
        return 0.0, 1 << tree
    (a, left), (b, right) = tree_cost(tree[0], cost), tree_cost(tree[1], cost)
    return cost(left | right) + a + b, left | right


def test_the_same_optimum_as_a_brute_force_on_every_shape():
    for n in range(2, 7):
        for name, edges in (("chain", chain(n)), ("star", star(n)), ("clique", clique(n)), ("cycle", cycle(n))):
            if name == "cycle" and n < 3:
                continue
            for seed in range(5):
                solved = solve(n, edges, cost_of(seed))
                total, relations = tree_cost(solved.tree, cost_of(seed))
                assert relations == (1 << n) - 1, (name, n)
                assert total == brute_force(n, edges, cost_of(seed)), (name, n, seed)


def test_a_clique_of_fourteen_in_under_a_second_and_a_budget_that_stops_it():
    started = time.perf_counter()
    solve(14, clique(14), lambda mask: float(bin(mask).count("1")))
    assert time.perf_counter() - started < 1.0
    with raises(Unsolved, match="budget"):
        solve(14, clique(14), lambda mask: 1.0, max_pairs=10_000)


def test_what_the_crate_refuses_comes_back_named():
    with raises(Unsolved, match="disconnected"):
        solve(3, [(0b001, 0b010)], lambda mask: 1.0)
    with raises(Unsolved, match="edge"):
        solve(3, [(0b001, 0b1000)], lambda mask: 1.0)
    assert solve(1, [], lambda mask: 1.0).tree == 0


if __name__ == "__main__":
    raise SystemExit(main(globals()))
