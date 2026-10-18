"""MultiJoin: a cluster of reorderable joins, as DPhyp takes it in.

A cluster is a maximal tree of the hash joins `is_reorderable` admits, through the wiring the
translator derives from orientation and projects that only pick columns. Anything else roots a
relation: a scan, an aggregate, a join it does not admit, a computing project, a filter — an
`IS NULL` between joins is what makes a left join an anti join. Columns are identities, never
positions. A cluster of inner joins has an edge per relation pair its keys join and one per
residual; a cluster holding a non-inner join has one edge per join, its whole predicate, on its
TES (`conflicts.py`). `design.md`, "Clusters" and "Non-inner joins: TES".
"""

from __future__ import annotations

import dataclasses
from dataclasses import dataclass

from ..plans import engine_ir
from .conflicts import MIRRORED, Operator, eligibility, logical
from ..plans.engine_expr import (
    Binary, ColumnRef, Unary, parse_columns, parse_expr, parse_join_keys, parse_named_exprs,
)
from ..plans.engine_plan import EngineNode
from ..operators.join_types import NULL_KEY_DROPPABLE, JoinType
from ..operators.joins import own_output

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
    column a `SidedColumn`. `kind` is the join's logical type, the side a non-inner join keeps
    or preserves on the left. In a cluster of inner joins an edge has keys or a residual, not
    both; with a non-inner join an edge is one join's, and its sides are the join's TES."""

    left: int
    right: int
    keys: tuple[tuple[RelColumn, RelColumn], ...] = ()
    null_equals_null: bool = False
    residual: object | None = None
    kind: JoinType = JoinType.INNER
    #: whether the plan's join built its `left` side, the side a non-inner join keeps
    builds_left: bool = True


@dataclass(frozen=True)
class MultiJoin:
    """`root` is the cluster's topmost join as the plan has it — the baseline order;
    `output` names each of its output columns. `joins` are the joins it holds and `mj` their
    id, none until `named` has given one."""

    root: EngineNode
    relations: tuple[Relation, ...]
    edges: tuple[Edge, ...]
    output: tuple[RelColumn, ...]
    joins: tuple[EngineNode, ...]
    mj: str | None
    #: whether a node above `root`, before the next exchange, needs its rows on `root`'s hash: a
    #: hash join, a final aggregate, or an enclosing cluster's join that may take its hash as keyed
    hash_read_above: bool = False
    #: the relation columns a null-key filter below a shuffle tested: the keys the plan shows can
    #: hold a NULL, which reassembly filters again where it shuffles them
    nullable_keys: frozenset[RelColumn] = frozenset()

    @property
    def holds_non_inner(self) -> bool:
        """Whether a join of the cluster is not inner: then each edge is one join, on its TES."""
        return any(edge.kind is not JoinType.INNER for edge in self.edges)


def null_key_columns(node: EngineNode) -> list[int] | None:
    """Where `node` is a filter of `<key> IS NOT NULL` conjuncts alone — what the planner puts
    below a shuffle, so a NULL that can match nothing does not crowd one lane — the ordinals it
    tests; else None. Below an emit it is wiring a cluster reads through."""
    if node.kind != "GpuFilter" or "projection" in node.fields:
        return None
    found = []

    def walk(expr) -> bool:
        if isinstance(expr, Binary) and expr.op == "AND":
            return walk(expr.left) and walk(expr.right)
        if isinstance(expr, Unary) and expr.op == "IS NOT NULL" and isinstance(expr.arg, ColumnRef):
            found.append(expr.arg.index)
            return True
        return False

    return found if walk(parse_expr(node.fields["predicate"])) else None


def _is_null_key_shuffle(node: EngineNode, dropped_keys: frozenset[int]) -> bool:
    """A shuffle over the planner's null-key filter: a filter of `IS NOT NULL` alone on columns
    the shuffle hashes and `dropped_keys` holds. A query's own `IS NOT NULL` can sit there too,
    over a one-lane input, and stays a filter."""
    if node.kind != "GpuEmitPartitions":
        return False
    tested = null_key_columns(node.children[0])
    hashed = {c.index for c in parse_columns(node.fields["hash"])}
    return tested is not None and set(tested) <= hashed & dropped_keys


def is_reorderable(node: EngineNode) -> bool:
    """A hash join of any type but mark, residual or not, as the engine runs every one. A mark
    join stays out: its mark column is no relation's. A residual on a non-inner join is read as
    rejecting no NULL, which conflict detection takes conservatively."""
    if node.kind != "GpuHashJoin":
        return False
    return engine_ir.join_type(node) in MIRRORED


def clusters(plan: EngineNode) -> list[MultiJoin]:
    """Every cluster of `plan`, outermost first, those inside a cluster's relations included. A hash
    the root declares is read by what consumes the plan: a probe plan's root feeds a join's build."""
    found, stack = [], [(plan, "hashed_on" in plan.fields)]
    while stack:
        node, hash_read = stack.pop()
        if is_reorderable(node):
            cluster = assemble(node, hash_read)
            found.append(cluster)
            stack.extend(reversed([(relation.root, _hash_joined_on(cluster, index))
                                   for index, relation in enumerate(cluster.relations)]))
        else:
            stack.extend(reversed([(child, _reads_hash(node, hash_read)) for child in node.children]))
    return found


def _hash_joined_on(cluster: MultiJoin, index: int) -> bool:
    """Whether a rebuilt join of `cluster` may take relation `index` as hashed on its keys without a
    shuffle: the hash it declares is on columns among its keys. Keys covering the hash need not be
    the keys a join takes, so this can shuffle more than needed; over the corpus it adds no shuffle
    beyond those q10, q35, q56 and q60 get from a join or final aggregate above."""
    root = cluster.relations[index].root
    if "hashed_on" not in root.fields:
        return False
    keys = {c.ordinal for edge in cluster.edges for pair in edge.keys for c in pair if c.relation == index}
    return {c.index for c in parse_columns(root.fields["hashed_on"])} <= keys


_EXCHANGES = frozenset({"GpuEmitPartitions", "GpuMergePartitions", "GpuMergeSortedPartitions", "GpuUnload"})


def _reads_hash(node: EngineNode, above: bool) -> bool:
    """Whether `node`'s children's hash is read by `node` or, through it, a node above: a hash
    join co-locates its sides, a final aggregate its groups; an exchange reads no hash, and any
    other node passes on what is above it."""
    if node.kind in _EXCHANGES:
        return False
    if node.kind == "GpuHashJoin" or node.kind.startswith("GpuAggregate") and "final" in node.fields:
        return True
    return above


def named(plan: EngineNode) -> EngineNode:
    """`plan` with each cluster's joins carrying its id, `MJ<n>`, numbered in `clusters`' order. A
    plan is named whole or not at all, so a named one — a replan's — comes back unchanged.

    The id is an attribute of the join nodes, not a table keyed by `id(node)`: `replaced`, a
    replan's memory sources and a reorder all copy nodes, and `dataclasses.replace` copies it with
    them where an identity-keyed table would lose them. Disassembly gives the joins it makes the
    cluster's id."""
    found = clusters(plan)
    if not found or any(cluster.mj is not None for cluster in found):
        return plan
    return _labelled(plan, {id(join): f"MJ{number}" for number, cluster in enumerate(found, 1)
                            for join in cluster.joins})


def _labelled(node: EngineNode, ids: dict[int, str]) -> EngineNode:
    """A copy of `node`'s tree, every node in `ids` carrying its id: the plan given is not changed."""
    return dataclasses.replace(node, children=[_labelled(child, ids) for child in node.children],
                               mj=ids.get(id(node), node.mj))


def assemble(root: EngineNode, hash_read_above: bool = False) -> MultiJoin:
    """The cluster whose topmost join is `root`."""
    return dataclasses.replace(_Assembly(root).multijoin(), hash_read_above=hash_read_above)


class _Assembly:
    def __init__(self, root: EngineNode):
        self.root = root
        self.relations: list[Relation] = []
        self.index: dict[int, int] = {}
        self.edges: list[Edge] = []
        self.joins: list[EngineNode] = []
        #: each join, bottom up, as conflict detection reads it, and its edges' keys and residual
        self.operators: list[tuple[Operator, Edge]] = []
        self.nullable: set[RelColumn] = set()
        self._collect(root)

    def multijoin(self) -> MultiJoin:
        output = tuple(self.lift(self.root, i) for i in range(len(self.root.schema)))
        edges = tuple(self.edges)
        # One edge per join: a set S holds at most |S| − 1 of them, so each pair DPhyp joins has
        # exactly one edge across it, its sides unsplit.
        if any(operator.kind is not JoinType.INNER for operator, _ in self.operators):
            tes = eligibility([operator for operator, _ in self.operators])
            edges = tuple(dataclasses.replace(whole, left=left, right=right)
                          for (_, whole), (left, right) in zip(self.operators, tes))
        return MultiJoin(self.root, tuple(self.relations), edges, output, tuple(self.joins), self.root.mj,
                         nullable_keys=frozenset(self.nullable))

    def _collect(self, node: EngineNode, dropped_keys: frozenset[int] = frozenset()) -> int:
        """The relations under `node`, as a mask, `node`'s joins and relations collected.
        `dropped_keys`: `node`'s columns that key the join above it on a side whose NULL keys can
        match nothing, the keys the planner filters below that side's shuffle."""
        if _is_null_key_shuffle(node, dropped_keys):
            filtered = node.children[0]
            found = self._collect(filtered.children[0])
            self.nullable.update(self.lift(filtered, k) for k in null_key_columns(filtered))
            return found
        if node.kind in _WIRING:
            return self._collect(node.children[0], dropped_keys)
        if _picks_columns(node):
            exprs = parse_named_exprs(node.fields["exprs"])
            return self._collect(node.children[0], frozenset(exprs[k][0].index for k in dropped_keys))
        if is_reorderable(node):
            self.joins.append(node)
            droppable = ((False, False) if node.fields.get("null_equals_null") == "true"
                         else NULL_KEY_DROPPABLE[engine_ir.join_type(node)])
            keys = parse_join_keys(node.fields["on"])
            dropped = [frozenset(pair[side].index for pair in keys) if drops else frozenset()
                       for side, drops in enumerate(droppable)]
            build, probe = [self._collect(child, side) for child, side in zip(node.children, dropped)]
            edges = self._edges(node)
            self.edges += edges
            self.operators.append(_operator(node, build, probe, edges))
            return build | probe
        self.index[id(node)] = len(self.relations)
        self.relations.append(Relation(node))
        return 1 << self.index[id(node)]

    def lift(self, node: EngineNode, ordinal: int) -> RelColumn:
        """Output `ordinal` of `node`, a node of this cluster, as the relation column it is."""
        while id(node) not in self.index:
            # A filter that roots a relation is indexed, so only the planner's is passed here.
            if node.kind in _WIRING or null_key_columns(node) is not None:
                node = node.children[0]
            elif node.kind == "GpuProject":
                expr, _ = parse_named_exprs(node.fields["exprs"])[ordinal]
                node, ordinal = node.children[0], expr.index
            else:
                build, probe = node.children
                if "projection" in node.fields:
                    ordinal = parse_columns(node.fields["projection"])[ordinal].index
                joined = own_output(engine_ir.join_type(node), len(build.schema), len(probe.schema))[ordinal]
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


def _operator(join: EngineNode, build: int, probe: int, edges: list[Edge]) -> tuple[Operator, Edge]:
    """`join` as conflict detection reads it, and as one edge over its plan sides `build` and
    `probe`, logical left first: all its keys and its residual."""
    kind, swapped = logical(engine_ir.join_type(join))
    reads = [0, 0]
    keys, residual = [], None
    for edge in edges:
        reads[0] |= edge.left
        reads[1] |= edge.right
        keys += edge.keys
        residual = edge.residual if edge.residual is not None else residual
    null_equals_null = join.fields.get("null_equals_null") == "true"
    strong = residual is None and not null_equals_null
    if swapped:
        build, probe, reads = probe, build, reads[::-1]
        keys = [(right, left) for left, right in keys]
    whole = Edge(build, probe, tuple(keys), null_equals_null, residual, kind, not swapped)
    return Operator(kind, build, probe, reads[0], reads[1], strong), whole


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

