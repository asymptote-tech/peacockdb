"""Join order for a plan's clusters of joins: what DPhyp is asked of a set of relations, which
side of each join in the tree it returns builds, and `optimize`, which does both for every
cluster of a plan.

DPhyp asks the cost of arbitrary connected sets of a cluster's relations, and `cardinality`
estimates trees. `SetEstimates` estimates a set with the same formulas, over one canonical
order: a set is its highest relation whose removal leaves it connected, joined to the rest —
so every set has one estimate, whatever order DPhyp reaches it in. A residual edge keeps
DuckDB's default share of the rows: its expression is in relation columns, which the filter
estimator does not read. A set's cost is C_out's term for it: its rows times
the width of what the joins above need of it — the cluster's output and the keys of every
edge that leaves it.
"""

from __future__ import annotations

import json

from . import dphyp
from .cardinality import Estimate, Estimator, estimator, shaped
from .cost import row_width
from .estimator import DEFAULT
from .disassembly import baseline, planned_lanes, reassembled
from ..plans.engine_expr import parse_columns
from ..plans.engine_plan import EngineNode
from .multijoin import MultiJoin, RelColumn, sided_columns
from .stats import Statistics

#: DuckDB's budget of connected pairs, past which it stops enumerating exactly
MAX_PAIRS = 10_000


class SetEstimates:
    """Estimates of the connected sets of `cluster`'s relations, by relation mask; `estimator`
    has estimated the plan the cluster is in."""

    def __init__(self, cluster: MultiJoin, estimator: Estimator):
        self.cluster, self.estimator = cluster, estimator
        self._sets: dict[int, tuple[Estimate, dict[RelColumn, int]]] = {}

    def rows(self, mask: int) -> float:
        return self._estimate(mask)[0].rows

    def cost(self, mask: int) -> float:
        """C_out in bytes of the join producing `mask`."""
        estimate, at = self._estimate(mask)
        needed = self.needed(mask)
        schema = [self.cluster.relations[c.relation].root.schema[c.ordinal] for c in needed]
        return estimate.rows * row_width(schema, tuple(estimate.columns[at[c]] for c in needed))

    def needed(self, mask: int) -> list[RelColumn]:
        """What the joins above `mask` and the cluster's output read of it."""
        found = [c for c in self.cluster.output if 1 << c.relation & mask]
        for edge in self.cluster.edges:
            span = edge.left | edge.right
            if span & mask and span & ~mask:
                columns = [c for pair in edge.keys for c in pair]
                columns += [c.column for c in sided_columns(edge.residual)]
                found += [c for c in columns if 1 << c.relation & mask]
        return list(dict.fromkeys(found))

    def _estimate(self, mask: int) -> tuple[Estimate, dict[RelColumn, int]]:
        if mask not in self._sets:
            self._sets[mask] = self._leaf(mask) if mask & (mask - 1) == 0 else self._join(mask)
        return self._sets[mask]

    def _leaf(self, mask: int):
        index = mask.bit_length() - 1
        estimate = self.estimator.estimates[id(self.cluster.relations[index].root)]
        return estimate, {RelColumn(index, k): k for k in range(len(estimate.columns))}

    def _join(self, mask: int):
        last = next(r for r in reversed(range(mask.bit_length()))
                    if mask >> r & 1 and self._connected(mask & ~(1 << r)))
        rest, alone = mask & ~(1 << last), 1 << last
        (left, left_at), (right, right_at) = self._estimate(rest), self._estimate(alone)
        pairs, residual = [], 1.0
        for edge in self.cluster.edges:
            span = edge.left | edge.right
            if span & ~mask or not span & rest or not span & alone:
                continue
            if edge.residual is not None:
                residual *= DEFAULT
            for a, b in edge.keys:
                inner, outer = (a, b) if 1 << a.relation & rest else (b, a)
                pairs.append((left_at[inner], right_at[outer]))
        joined = self.estimator.equi_join(left, right, pairs, residual)
        at = dict(left_at) | {c: k + len(left.columns) for c, k in right_at.items()}
        return shaped(joined.rows, joined.build.columns + joined.probe.columns), at

    def _connected(self, mask: int) -> bool:
        """Whether the edges inside `mask` reach all of it."""
        if mask == 0:
            return False
        reached = mask & -mask
        while True:
            grown = reached
            for edge in self.cluster.edges:
                span = edge.left | edge.right
                if span & ~mask == 0 and span & reached:
                    grown |= span
            if grown == reached:
                return reached == mask
            reached = grown


def orient(tree, sets: SetEstimates, copies: int = 1, lanes: int = 1,
           planned: dict[int, int] | None = None):
    """`tree` — a relation's index or a pair of trees, sides unordered — with each join's cheaper
    side first, as its build. Bottom up, since a join's probe batches are what the
    join below it emits. For an inner join the two configurations differ only in what the build
    costs: held whole, and copied once per probe batch while the build handle cannot outlive a
    streamed probe (#152, `copies` = 1). A join runs on the lanes the plan had for its relations
    (`planned`, by mask), or on `lanes`; a probe not already hashed onto them is shuffled first,
    which cuts each of its batches once per lane."""
    return _orient(tree, sets, copies, lanes, planned or {})[0]


def _orient(tree, sets, copies, lanes, planned):
    """The oriented tree, its relation mask, its bytes, the batches it emits and its rows."""
    if isinstance(tree, int):
        root = sets.cluster.relations[tree].root
        estimate = sets.estimator.estimates[id(root)]
        return (tree, 1 << tree, estimate.rows * row_width(root.schema, estimate.columns), batches(root),
                estimate.rows)
    a, b = _orient(tree[0], sets, copies, lanes, planned), _orient(tree[1], sets, copies, lanes, planned)
    mask = a[1] | b[1]
    width = planned.get(mask, lanes)
    fed = {a[1]: _fed(a, b[1], width, sets), b[1]: _fed(b, a[1], width, sets)}
    build, probe = min((a, b), (b, a), key=lambda side: side[0][2] * (1 + copies * fed[side[1][1]]))
    return (build[0], probe[0]), mask, sets.cost(mask), fed[probe[1]], sets.rows(mask)


def _fed(side, other: int, lanes: int, sets: SetEstimates) -> float:
    """The batches `side` reaches a join on `lanes` lanes with: its own where it needs no shuffle,
    else each cut once per lane, at most one a row. A join's output is taken to need one: it is
    hashed on its own keys, which the next join's seldom are."""
    tree, mask, _, emitted, rows = side
    if lanes == 1 or (isinstance(tree, int) and _hashed_on_keys(tree, mask | other, lanes, sets)):
        return emitted
    return min(emitted * lanes, max(rows, 1.0))


def _hashed_on_keys(relation: int, both: int, lanes: int, sets: SetEstimates) -> bool:
    """Whether `relation` is already on `lanes` lanes hashed on its keys to the rest of `both`."""
    root = sets.cluster.relations[relation].root
    if int(root.fields["lanes"]) != lanes or "hashed_on" not in root.fields:
        return False
    keys = {c.ordinal for edge in sets.cluster.edges
            if edge.keys and (edge.left | edge.right) & ~both == 0 and (edge.left | edge.right) & 1 << relation
            for pair in edge.keys for c in pair if c.relation == relation}
    return {c.index for c in parse_columns(root.fields["hashed_on"])} == keys


_KEEPS_BATCHES = frozenset({"GpuFilter", "GpuProject", "GpuMergePartitions"})


def batches(node) -> int:
    """How many batches `node` emits over all its lanes, as its layout says: one a lane where it
    emits one, a scan its mapping's, a join one per probe batch."""
    if node.fields.get("batches") == "single":
        return int(node.fields["lanes"])
    if node.kind == "GpuLoadParquet":
        return sum(len(lane) for lane in json.loads(node.fields["partition_groups"]))
    if node.kind in _KEEPS_BATCHES:
        return batches(node.children[0])
    if node.kind == "GpuEmitPartitions":
        return batches(node.children[0]) * int(node.fields["lanes"])
    if node.kind in ("GpuHashJoin", "GpuNestedLoopJoin", "GpuCrossJoin"):
        return batches(node.children[1])
    if node.kind in ("GpuUnion", "GpuInterleave"):
        return sum(batches(child) for child in node.children)
    return int(node.fields.get("lanes", 1))


def optimize(plan: EngineNode, stats: Statistics, lanes: int, copies: int = 1,
             max_pairs: int = MAX_PAIRS, known: dict[str, Estimate] | None = None) -> EngineNode:
    """`plan` with every cluster of inner joins reordered by DPhyp on C_out and each join
    oriented. DPhyp sees the key edges only: a hash join needs a key, and two sets that only a
    residual connects would be a nested loop — the residual goes on the first join that has its
    relations anyway. A cluster past `max_pairs` keeps the plan's order, oriented again: the
    greedy fallback is not built yet. `known` are the memory sources' measured estimates."""
    planned = estimator(plan, stats, known)

    def order(cluster: MultiJoin):
        sets = SetEstimates(cluster, planned)
        edges = [(edge.left, edge.right) for edge in cluster.edges if edge.keys]
        try:
            tree = dphyp.solve(len(cluster.relations), edges, sets.cost, max_pairs).tree
        except dphyp.Unsolved as unsolved:
            if unsolved.reason != "budget":
                raise
            tree = baseline(cluster)
        return orient(tree, sets, copies, lanes, planned_lanes(cluster))

    return reassembled(plan, lanes, order)

