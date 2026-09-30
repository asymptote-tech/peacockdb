"""MultiJoin: a cluster of reorderable joins, as DPhyp takes it in.

A cluster is a maximal tree of inner hash joins, reached through the wiring the translator
derives from orientation — `GpuCoalesceAllBatches`, `GpuEmitPartitions`, `GpuMergePartitions` —
and through projects that only pick and rename columns. Anything else roots a relation and
is left alone: a scan, an aggregate, a join of another type, a project that computes, and a
filter, whose `IS NULL` between joins is what makes a left join an anti join. A relation
may hold clusters of its own; `clusters` finds those too.

Columns are identities — which relation, which of its outputs — never positions: a reordered
tree renumbers every position. An edge joins the relations its predicate reads: the key
pairs between one pair of relations make one edge, so a composite key stays whole, and a
residual predicate is an edge of its own over every relation it reads.
"""

from __future__ import annotations

import dataclasses
from dataclasses import dataclass

from . import engine_ir
from .engine_expr import ColumnRef, parse_columns, parse_expr, parse_join_keys, parse_named_exprs
from .engine_plan import EngineNode
from .operators.join_types import JoinType
from .operators.joins import own_output

_WIRING = frozenset({"GpuCoalesceAllBatches", "GpuEmitPartitions", "GpuMergePartitions"})


@dataclass(frozen=True)
class RelColumn:
    """A column of a MultiJoin: output `ordinal` of relation `relation`."""

    relation: int
    ordinal: int


@dataclass(frozen=True)
class Relation:
    root: EngineNode

    @property
    def names(self) -> tuple[str, ...]:
        return tuple(name for name, _ in self.root.schema)


@dataclass(frozen=True)
class Edge:
    """A predicate between the relations in `left` and those in `right` (bit masks). `keys`
    pair a left column with a right one; `residual` is the engine's expression with every
    column a `SidedColumn`, and an edge has keys or a residual, not both."""

    left: int
    right: int
    keys: tuple[tuple[RelColumn, RelColumn], ...] = ()
    null_equals_null: bool = False
    residual: object | None = None


@dataclass(frozen=True)
class MultiJoin:
    """`root` is the cluster's topmost join as the plan has it — the baseline order;
    `output` names each of its output columns."""

    root: EngineNode
    relations: tuple[Relation, ...]
    edges: tuple[Edge, ...]
    output: tuple[RelColumn, ...]


def is_reorderable(node: EngineNode) -> bool:
    return node.kind == "GpuHashJoin" and engine_ir.join_type(node) is JoinType.INNER


def clusters(plan: EngineNode) -> list[MultiJoin]:
    """Every cluster of `plan`, outermost first, those inside a cluster's relations included."""
    found, stack = [], [plan]
    while stack:
        node = stack.pop()
        if is_reorderable(node):
            cluster = assemble(node)
            found.append(cluster)
            stack.extend(reversed([relation.root for relation in cluster.relations]))
        else:
            stack.extend(reversed(node.children))
    return found


def assemble(root: EngineNode) -> MultiJoin:
    """The cluster whose topmost join is `root`."""
    return _Assembly(root).multijoin()


class _Assembly:
    def __init__(self, root: EngineNode):
        self.root = root
        self.relations: list[Relation] = []
        self.index: dict[int, int] = {}
        self.edges: list[Edge] = []
        self._collect(root)

    def multijoin(self) -> MultiJoin:
        output = tuple(self.lift(self.root, i) for i in range(len(self.root.schema)))
        return MultiJoin(self.root, tuple(self.relations), tuple(self.edges), output)

    def _collect(self, node: EngineNode) -> None:
        if node.kind in _WIRING or _picks_columns(node):
            self._collect(node.children[0])
        elif is_reorderable(node):
            for child in node.children:
                self._collect(child)
            self.edges += self._edges(node)
        else:
            self.index[id(node)] = len(self.relations)
            self.relations.append(Relation(node))

    def lift(self, node: EngineNode, ordinal: int) -> RelColumn:
        """Output `ordinal` of `node`, a node of this cluster, as the relation column it is."""
        while id(node) not in self.index:
            if node.kind in _WIRING:
                node = node.children[0]
            elif node.kind == "GpuProject":
                expr, _ = parse_named_exprs(node.fields["exprs"])[ordinal]
                node, ordinal = node.children[0], expr.index
            else:
                build, probe = node.children
                if "projection" in node.fields:
                    ordinal = parse_columns(node.fields["projection"])[ordinal].index
                joined = own_output(JoinType.INNER, len(build.schema), len(probe.schema))[ordinal]
                node, ordinal = (build, joined) if joined < len(build.schema) else (probe, joined - len(build.schema))
        return RelColumn(self.index[id(node)], ordinal)

    def _edges(self, join: EngineNode) -> list[Edge]:
        build, probe = join.children
        null_equals_null = join.fields.get("null_equals_null") == "true"
        by_pair: dict[tuple[int, int], list] = {}
        for key_build, key_probe in parse_join_keys(join.fields["on"]):
            pair = (self.lift(build, key_build.index), self.lift(probe, key_probe.index))
            by_pair.setdefault((pair[0].relation, pair[1].relation), []).append(pair)
        edges = [Edge(1 << left, 1 << right, tuple(keys), null_equals_null)
                 for (left, right), keys in by_pair.items()]
        if "filter" in join.fields:
            residual = self._lifted(parse_expr(join.fields["filter"]), build, probe)
            sides = {"build": 0, "probe": 0}
            for column in sided_columns(residual):
                sides[column.side] |= 1 << column.column.relation
            edges.append(Edge(sides["build"], sides["probe"], residual=residual))
        return edges

    def _lifted(self, expr, build, probe):
        """`expr` with every `name@side:ordinal` replaced by the relation column it is, the side
        kept beside it for the edge's masks."""
        if isinstance(expr, ColumnRef):
            side = build if expr.side == "build" else probe
            return SidedColumn(self.lift(side, expr.index), expr.side)
        if dataclasses.is_dataclass(expr):
            return dataclasses.replace(expr, **{f.name: self._lifted(getattr(expr, f.name), build, probe)
                                                for f in dataclasses.fields(expr)})
        if isinstance(expr, tuple):
            return tuple(self._lifted(e, build, probe) for e in expr)
        return expr


@dataclass(frozen=True)
class SidedColumn:
    """A residual's column, and the side of the plan's join it was read from — the side the
    edge's `left` (build) or `right` (probe) mask counts it on."""

    column: RelColumn
    side: str


def sided_columns(expr):
    """Every `SidedColumn` in a residual."""
    if isinstance(expr, SidedColumn):
        yield expr
    elif dataclasses.is_dataclass(expr):
        for f in dataclasses.fields(expr):
            yield from sided_columns(getattr(expr, f.name))
    elif isinstance(expr, tuple):
        for e in expr:
            yield from sided_columns(e)


def _picks_columns(node: EngineNode) -> bool:
    """A project that computes nothing — every output one of its input's columns."""
    return node.kind == "GpuProject" and all(isinstance(e, ColumnRef) for e, _ in parse_named_exprs(node.fields["exprs"]))


def replaced(plan: EngineNode, old: EngineNode, new: EngineNode) -> EngineNode:
    """`plan` with the subtree `old` — by identity — swapped for `new`. Only the path to it is
    copied: every other node stays the object it was, so a later swap still finds its own."""
    if plan is old:
        return new
    children = [replaced(child, old, new) for child in plan.children]
    if all(a is b for a, b in zip(children, plan.children)):
        return plan
    return dataclasses.replace(plan, children=children)

