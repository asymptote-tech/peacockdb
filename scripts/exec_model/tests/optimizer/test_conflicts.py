"""Conflict detection: each join's total eligibility set (TES), what a join of two sets may apply,
and, through engine plans, that every order the conflicts admit answers as the plan does."""

from __future__ import annotations

if __package__ in (None, ""):  # allow `python scripts/exec_model/tests/optimizer/<file>.py`
    import pathlib as _pathlib, sys as _sys

    _sys.path.insert(0, str(_pathlib.Path(__file__).resolve().parents[4]))
    __package__ = "scripts.exec_model.tests.optimizer"

import itertools
import random

import pandas as pd

from ..corpus import execute
from ..harness import main
from ...operators.join_types import JoinType
from ...operators.joins import own_output
from ...optimizer.conflicts import Operator, eligibility, joining
from ...optimizer.disassembly import disassemble
from ...optimizer.multijoin import Edge, clusters, replaced
from ...plans.engine_nodes import build
from ...plans.engine_plan import parse_plans

N, C, O, L = 0b0001, 0b0010, 0b0100, 0b1000


def op(kind, left, right, reads_left=None, reads_right=None, strong=True):
    return Operator(kind, left, right, reads_left or left, reads_right or right, strong)


def test_an_inner_join_over_a_left_join_s_preserved_side_may_go_below_it():
    # left-join-reorder: σ(nation) ⋈ (customer ⟕ orders), the inner key on customer.
    tes = eligibility([op(JoinType.INNER, N, C | O, N, C), op(JoinType.LEFT, C, O)])
    assert tes == [(N, C), (C, O)]


def test_an_inner_join_over_a_left_join_s_null_supplying_side_needs_both_its_sides():
    # left-join-fixed: σ(lineitem) ⋈ (customer ⟕ orders), the inner key on orders.
    tes = eligibility([op(JoinType.INNER, L, C | O, L, O), op(JoinType.LEFT, C, O)])
    assert tes == [(L, C | O), (C, O)]


def test_an_inner_join_over_a_semi_or_anti_join_s_kept_side_may_go_below_it():
    for kind in (JoinType.LEFT_SEMI, JoinType.LEFT_ANTI):
        assert eligibility([op(JoinType.INNER, N, C | O, N, C), op(kind, C, O)]) == [(N, C), (C, O)], kind


def test_a_semi_join_waits_for_every_join_under_its_other_side():
    assert eligibility([op(JoinType.LEFT_SEMI, C, O | L, C, O), op(JoinType.INNER, O, L)]) == \
        [(C, O | L), (O, L)]


def test_two_left_joins_reassociate_only_where_the_upper_key_rejects_nulls():
    lower = op(JoinType.LEFT, N, C)
    assert eligibility([op(JoinType.LEFT, N | C, O, C, O), lower])[0] == (C, O)
    assert eligibility([op(JoinType.LEFT, N | C, O, C, O, strong=False), lower])[0] == (N | C, O)


def test_a_join_of_two_sets_applies_each_edge_with_one_side_within_each():
    inner, other, left = Edge(N, C), Edge(N, O), Edge(C, O, kind=JoinType.LEFT)
    assert joining((inner, other), C | O, N) == (inner, other)
    assert joining((inner, other), N | C, O) == (other,)
    assert joining((inner, left), N | C, O) == (left,)
    assert joining((inner, left), N, O) is None
    # A hyperedge joins only sets that hold each of its sides whole.
    assert joining((Edge(C, N | O),), C | N, O) is None
    assert joining((Edge(C, N | O),), C, N | O) == (Edge(C, N | O),)


# -- every admitted order answers as the plan does ------------------------------------------------

# Two NULLs in every table: a row an outer join pads, matched on NULL by a join that holds NULL
# equal, comes back twice, where unmatched it would be once.
FRAMES = {
    "t1": pd.DataFrame({"a": [1, 2, 3, 4, 4, None, None]}, dtype="Int64"),
    "t2": pd.DataFrame({"b": [1, 2, 2, 5, None, None]}, dtype="Int64"),
    "t3": pd.DataFrame({"c": [2, 3, 5, 5, 6, None, None]}, dtype="Int64"),
    "t4": pd.DataFrame({"d": [1, 3, 5, 7, None, None]}, dtype="Int64"),
}
TYPES = ("Inner", "Left", "Right", "Full", "LeftSemi", "RightSemi", "LeftAnti", "RightAnti")
_BY_NAME = {"Inner": JoinType.INNER, "Left": JoinType.LEFT, "Right": JoinType.RIGHT, "Full": JoinType.FULL,
            "LeftSemi": JoinType.LEFT_SEMI, "RightSemi": JoinType.RIGHT_SEMI,
            "LeftAnti": JoinType.LEFT_ANTI, "RightAnti": JoinType.RIGHT_ANTI}


class Tables:
    def frame(self, table, columns):
        return FRAMES[table][list(columns)].copy()

    def row_counts(self, table):
        return [len(FRAMES[table])]


def _columns(tree) -> list[str]:
    """The columns a plan tree outputs: a table's one column, or a join's own output."""
    if isinstance(tree, str):
        return [FRAMES[tree].columns[0]]
    join_type, build_tree, probe_tree, _, _ = tree
    joined = _columns(build_tree) + _columns(probe_tree)
    return [joined[at] for at in own_output(_BY_NAME[join_type], len(_columns(build_tree)), len(_columns(probe_tree)))]


def _text(tree, indent: int) -> str:
    pad = " " * indent
    if isinstance(tree, str):
        column = FRAMES[tree].columns[0]
        return (f"{pad}GpuLoadParquet: table={tree}, projections=[{column}@0], partition_groups=[[[0]]], "
                f"lanes=1, batches=multiple, schema=[{column}:Int64]")
    join_type, build_tree, probe_tree, (key_build, key_probe), null_equal = tree
    build_columns, probe_columns = _columns(build_tree), _columns(probe_tree)
    schema = ", ".join(f"{c}:Int64" for c in _columns(tree))
    build_schema = ", ".join(f"{c}:Int64" for c in build_columns)
    return (f"{pad}GpuHashJoin: join_type={join_type}, on=[({key_build}@{build_columns.index(key_build)}, "
            f"{key_probe}@{probe_columns.index(key_probe)})]{', null_equals_null=true' if null_equal else ''}, "
            f"lanes=1, batches=multiple, schema=[{schema}]\n"
            f"{pad}  GpuCoalesceAllBatches: lanes=1, batches=single, schema=[{build_schema}]\n"
            f"{_text(build_tree, indent + 4)}\n{_text(probe_tree, indent + 2)}")


def _plan(tree):
    schema = ", ".join(f"{c}:Int64" for c in _columns(tree))
    return parse_plans(f"== q\nGpuUnload: lanes=1, batches=multiple, schema=[{schema}]\n{_text(tree, 2)}\n",
                       "test")["q"]


def _answer(plan) -> list:
    got, _ = execute(build(plan, Tables()))
    # NULL as -1, which no table holds, so rows sort.
    return sorted(tuple(-1 if pd.isna(v) else int(v) for v in row) for row in got.itertuples(index=False))


def _admitted(edges, mask: int):
    """Every tree over `mask` whose joins `joining` allows, each split once."""
    if mask & (mask - 1) == 0:
        yield mask.bit_length() - 1
        return
    low = mask & -mask
    sub = (mask - 1) & mask
    while sub:
        if sub & low and joining(edges, sub, mask & ~sub) is not None:
            for left in _admitted(edges, sub):
                for right in _admitted(edges, mask & ~sub):
                    yield left, right
        sub = (sub - 1) & mask


def _orientations(tree):
    """`tree` with each of its joins either way round."""
    if isinstance(tree, int):
        yield tree
        return
    for left in _orientations(tree[0]):
        for right in _orientations(tree[1]):
            yield left, right
            yield right, left


def _plans(tables: int):
    """Plan trees over `tables` relations, each join of every type, its keys every pair of columns
    its two sides output, holding NULL unequal and equal."""
    names = ["t1", "t2", "t3", "t4"][:tables]

    def shapes(group):
        if len(group) == 1:
            yield group[0]
            return
        for cut in range(1, len(group)):
            for build_tree in shapes(group[:cut]):
                for probe_tree in shapes(group[cut:]):
                    for join_type in TYPES:
                        for keys in itertools.product(_columns(build_tree), _columns(probe_tree)):
                            for null_equal in (False, True):
                                yield join_type, build_tree, probe_tree, keys, null_equal

    yield from shapes(names)


def _check(tree, found: dict, sample: random.Random | None = None) -> None:
    """Every admitted order of `tree`'s plan, every join each way round, or `sample` picking one."""
    plan = _plan(tree)
    [cluster] = clusters(plan)
    want = _answer(plan)
    orders = list(_admitted(cluster.edges, (1 << len(cluster.relations)) - 1))
    found["orders"] += len(orders)
    for order in orders:
        oriented = list(_orientations(order))
        for one in oriented if sample is None else [sample.choice(oriented)]:
            got = _answer(replaced(plan, cluster.root, disassemble(cluster, one, lanes=1)))
            assert got == want, (tree, one, got, want)


def test_every_order_the_conflicts_admit_answers_as_the_plan_does_over_three_relations():
    # Every plan and order, one way round per join drawn for each: all of them take two minutes.
    found, draw = {"orders": 0}, random.Random(3)
    plans = list(_plans(3))
    for tree in plans:
        _check(tree, found, draw)
    # More orders than plans: some are reordered.
    assert found["orders"] > len(plans), found


def test_every_order_the_conflicts_admit_answers_as_the_plan_does_over_four_relations_sampled():
    found, draw = {"orders": 0}, random.Random(12)
    for tree in draw.sample(list(_plans(4)), 60):
        _check(tree, found, draw)
    assert found["orders"] > 60, found


if __name__ == "__main__":
    raise SystemExit(main(globals()))
