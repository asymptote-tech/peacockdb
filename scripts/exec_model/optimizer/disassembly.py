"""A join order back into an engine plan: DPhyp's tree, oriented, over a MultiJoin's relations, as
`GpuHashJoin` nodes. Per join, bottom up: its type, keys and residuals from the edges between its
sides — a non-inner type mirrored where its kept side probes — a residual on the first join that
has all it reads; a projection of what the joins above and the cluster's output need; and the
translator's wiring, a side not hashed on its keys shuffled at more than one lane, the build made
one batch per lane, a join over the plan's relations on the plan's lanes. The root keeps the
plan root's schema, names included, and its hash where a node above reads it. `design.md`,
"Disassembly".
"""

from __future__ import annotations

import dataclasses
from dataclasses import dataclass, replace

from ..operators.join_types import NULL_KEY_DROPPABLE, JoinType
from ..operators.joins import own_output
from ..plans.engine_expr import Binary, ColumnRef, Unary, expr_text, parse_columns, quoted
from ..plans.engine_ir import join_type_name
from ..plans.engine_plan import EngineNode
from .conflicts import MIRRORED
from .multijoin import MultiJoin, RelColumn, SidedColumn, clusters, is_reorderable, replaced, sided_columns
from .report import mask

JoinTree = int | tuple  # a relation's index, or (build tree, probe tree)


def baseline(cluster: MultiJoin) -> JoinTree:
    """The order the plan already has."""
    roots = {id(r.root): i for i, r in enumerate(cluster.relations)}

    def tree(node):
        while id(node) not in roots and not is_reorderable(node):
            node = node.children[0]
        if id(node) in roots:
            return roots[id(node)]
        return tree(node.children[0]), tree(node.children[1])

    return tree(cluster.root)


def reassembled(plan: EngineNode, lanes: int, order=baseline) -> EngineNode:
    """`plan` with every cluster, nested ones included, disassembled in `order(cluster)`."""
    for cluster in clusters(plan):
        plan = replaced(plan, cluster.root, disassemble(cluster, order(cluster), lanes))
    return plan


def disassemble(cluster: MultiJoin, tree: JoinTree, lanes: int) -> EngineNode:
    """The plan for `tree`, its root outputting `cluster.output` under the original root's schema.
    Where a hash join or a final aggregate above reads the root's hash before any exchange, a new
    root joining on other keys is shuffled back onto it; per-lane nodes alone read none."""
    disassembly = _Disassembly(cluster, lanes, planned_lanes(cluster))
    part = disassembly.part(tree, list(cluster.output))
    root = cluster.root
    if cluster.hash_read_above and int(root.fields["lanes"]) > 1 and "hashed_on" in root.fields:
        hashed = tuple(c.index for c in parse_columns(root.fields["hashed_on"]))
        if (part.lanes, part.hashed) != (int(root.fields["lanes"]), hashed):
            part = disassembly._wired(part, list(hashed), int(root.fields["lanes"]))
    node = part.node
    return replace(node, fields=dict(node.fields, schema=_schema_text(cluster.root.schema)),
                   schema=cluster.root.schema)


@dataclass(frozen=True)
class _Part:
    """A subtree's node, what it outputs in order, and the lanes, batching and hash it has."""

    node: EngineNode
    columns: tuple[RelColumn, ...]
    lanes: int
    single: bool
    hashed: tuple[int, ...] | None


class _Disassembly:
    def __init__(self, cluster: MultiJoin, lanes: int, planned: dict[int, int]):
        self.cluster, self.mode_lanes, self.planned = cluster, lanes, planned

    def part(self, tree: JoinTree, need: list[RelColumn]) -> _Part:
        if isinstance(tree, int):
            return self._relation(tree)
        build_tree, probe_tree = tree
        build_mask, probe_mask = mask(build_tree), mask(probe_tree)
        join_type, keys, residuals, null_equals_null = self._predicates(build_mask, probe_mask)
        if not keys:
            raise ValueError(f"no key joins relations {build_mask:#b} and {probe_mask:#b}: a hash join needs one")
        read = [c for pair in keys for c in pair] + [c.column for r in residuals for c in sided_columns(r)]
        build = self.part(build_tree, _unique([c for c in need + read if 1 << c.relation & build_mask]))
        probe = self.part(probe_tree, _unique([c for c in need + read if 1 << c.relation & probe_mask]))
        build_keys = [build.columns.index(b) for b, _ in keys]
        probe_keys = [probe.columns.index(p) for _, p in keys]
        lanes = self.planned.get(build_mask | probe_mask, self.mode_lanes)
        droppable = (False, False) if null_equals_null else NULL_KEY_DROPPABLE[join_type]
        build = self._wired(build, build_keys, lanes, droppable[0])
        probe = self._wired(probe, probe_keys, lanes, droppable[1])
        if not build.single:
            build = self._wrap("GpuCoalesceAllBatches", build, single=True)
        return self._join(_Applied(join_type, build_keys, probe_keys, residuals, null_equals_null),
                          build, probe, keys, need)

    def _relation(self, index: int) -> _Part:
        root = self.cluster.relations[index].root
        hashed = tuple(c.index for c in parse_columns(root.fields["hashed_on"])) if "hashed_on" in root.fields else None
        return _Part(root, tuple(RelColumn(index, k) for k in range(len(root.schema))),
                     int(root.fields["lanes"]), root.fields["batches"] == "single", hashed)

    def _predicates(self, build_mask: int, probe_mask: int):
        """The join's type, the key pairs (build column, probe column) and residuals it applies,
        and whether its keys hold NULL equal to NULL."""
        keys, residuals, null_equal = [], [], set()
        for edge in _across(self.cluster, build_mask, probe_mask):
            if edge.residual is not None:
                residuals.append(edge.residual)
            if edge.left & build_mask:
                keys += list(edge.keys)
            else:
                keys += [(right, left) for left, right in edge.keys]
            if edge.keys:
                null_equal.add(edge.null_equals_null)
        if len(null_equal) > 1:
            raise ValueError(f"keys joining {build_mask:#b} and {probe_mask:#b} disagree on whether NULL "
                             "equals NULL: one join holds one rule")
        return join_type_of(self.cluster, build_mask, probe_mask), keys, residuals, null_equal == {True}

    def _wired(self, side: _Part, keys: list[int], lanes: int, drop_null_keys: bool = False) -> _Part:
        """The side as `co_partitioned` wants it: on the join's lanes, and at more than one
        hashed on `keys` — `merged` into one lane, or `shuffled` on the keys, where it is not. A
        side whose NULL keys can match nothing (`drop_null_keys`) is shuffled through the
        planner's null-key filter."""
        if side.lanes == lanes and (lanes == 1 or side.hashed == tuple(keys)):
            return side
        if lanes == 1:
            return self._wrap("GpuMergePartitions", side, lanes=1, single=False, hashed=None)
        was_single = side.single
        if side.lanes > 1:
            side = self._wrap("GpuMergePartitions", side, lanes=1, single=False, hashed=None)
        if was_single and not side.single:
            side = self._wrap("GpuCoalesceAllBatches", side, single=True)
        nullable = [k for k in keys if side.columns[k] in self.cluster.nullable_keys] if drop_null_keys else []
        if nullable:
            # The planner's null-key filter below the shuffle, on the keys the plan's own filters
            # showed can hold a NULL.
            tests = [Unary("IS NOT NULL", ColumnRef(side.node.schema[k][0], k, None)) for k in nullable]
            predicate = tests[0]
            for test in tests[1:]:
                predicate = Binary(predicate, "AND", test)
            side = self._wrap("GpuFilter", side, first={"predicate": expr_text(predicate)}, hashed=None)
        return self._wrap("GpuEmitPartitions", side, lanes=lanes, single=False, hashed=tuple(keys),
                          first={"hash": _columns_text(side, keys)})

    def _wrap(self, kind, side: _Part, first=None, **layout) -> _Part:
        wrapped = replace(side, **layout)
        fields = dict(first or {})
        fields.update(_layout_fields(wrapped, side.node.schema))
        node = EngineNode(kind, fields, side.node.schema, [side.node])
        return replace(wrapped, node=node)

    def _join(self, applied: _Applied, build: _Part, probe: _Part, keys, need) -> _Part:
        joined = build.columns + probe.columns
        own = [joined[at] for at in own_output(applied.join_type, len(build.columns), len(probe.columns))]
        picked = [own.index(c) for c in need]
        schema = tuple(self._column_schema(c) for c in need)
        fields = {"join_type": join_type_name(applied.join_type),
                  "on": "[" + ", ".join(f"({_ref(build, b)}, {_ref(probe, p)})"
                                        for b, p in zip(applied.build_keys, applied.probe_keys)) + "]"}
        if applied.null_equals_null:
            fields["null_equals_null"] = "true"
        if applied.residuals:
            fields["filter"] = expr_text(_conjunction([self._resided(r, build, probe) for r in applied.residuals]))
        if picked != list(range(len(own))):
            names = [self._column_schema(c)[0] for c in own]
            fields["projection"] = "[" + ", ".join(f"{quoted(names[i])}@{i}" for i in picked) + "]"
        hashed = None
        if build.lanes > 1:
            at = {c: out for out, c in enumerate(need)}
            for side in _HASHED_BY.get(applied.join_type, ()):
                positions = [at.get(pair[side]) for pair in keys]
                if None not in positions:
                    hashed = tuple(positions)
                    break
        part = _Part(EngineNode("GpuHashJoin", {}, schema, [build.node, probe.node], mj=self.cluster.mj),
                     tuple(need), build.lanes, False, hashed)
        fields.update(_layout_fields(part, schema))
        return replace(part, node=replace(part.node, fields=fields))

    def _column_schema(self, column: RelColumn) -> tuple[str, str]:
        return self.cluster.relations[column.relation].root.schema[column.ordinal]

    def _resided(self, residual, build: _Part, probe: _Part):
        """A residual with each column read from the side of this join it is now on."""
        if isinstance(residual, SidedColumn):
            side, part = ("build", build) if residual.column in build.columns else ("probe", probe)
            return ColumnRef(self._column_schema(residual.column)[0], part.columns.index(residual.column), side)
        if dataclasses.is_dataclass(residual):
            return replace(residual, **{f.name: self._resided(getattr(residual, f.name), build, probe)
                                        for f in dataclasses.fields(residual)})
        if isinstance(residual, tuple):
            return tuple(self._resided(r, build, probe) for r in residual)
        return residual


@dataclass(frozen=True)
class _Applied:
    """What a join applies: its type, its keys as ordinals of its two sides, its residuals."""

    join_type: JoinType
    build_keys: list[int]
    probe_keys: list[int]
    residuals: list
    null_equals_null: bool


#: The sides, 0 build and 1 probe, whose keys a join's output is hashed on — tried in order. A
#: side an outer join pads holds NULL where it found no match, so it is not hashed on its keys.
_HASHED_BY = {JoinType.INNER: (0, 1), JoinType.LEFT: (0,), JoinType.LEFT_SEMI: (0,), JoinType.LEFT_ANTI: (0,),
              JoinType.RIGHT: (1,), JoinType.RIGHT_SEMI: (1,), JoinType.RIGHT_ANTI: (1,)}


def join_type_of(cluster: MultiJoin, build_mask: int, probe_mask: int) -> JoinType:
    """The type of the join of `build_mask` and `probe_mask`, its build on the left as the engine
    names it: its non-inner edge's, mirrored where the side the edge keeps probes."""
    for edge in _across(cluster, build_mask, probe_mask):
        if edge.kind is not JoinType.INNER:
            return edge.kind if edge.left & build_mask else MIRRORED[edge.kind]
    return JoinType.INNER


def _across(cluster: MultiJoin, build_mask: int, probe_mask: int):
    """The cluster's edges a join of the two sets applies: within both, reaching into each."""
    both = build_mask | probe_mask
    for edge in cluster.edges:
        span = edge.left | edge.right
        if not (span & ~both or not span & build_mask or not span & probe_mask):
            yield edge


def planned_lanes(cluster: MultiJoin) -> dict[int, int]:
    """The lanes of each join the plan had, by the mask of the relations below it."""
    found = {}

    def walk(tree, node):
        while not is_reorderable(node) and id(node) not in roots:
            node = node.children[0]
        if isinstance(tree, tuple):
            found[mask(tree)] = int(node.fields["lanes"])
            walk(tree[0], node.children[0])
            walk(tree[1], node.children[1])

    roots = {id(r.root) for r in cluster.relations}
    walk(baseline(cluster), cluster.root)
    return found



def _unique(columns):
    return list(dict.fromkeys(columns))


def _conjunction(parts):
    result = parts[0]
    for part in parts[1:]:
        result = Binary(result, "AND", part)
    return result


def _ref(side: _Part, at: int) -> str:
    return f"{quoted(side.node.schema[at][0])}@{at}"


def _columns_text(side: _Part, ordinals) -> str:
    return "[" + ", ".join(_ref(side, k) for k in ordinals) + "]"


def _layout_fields(part: _Part, schema) -> dict[str, str]:
    fields = {"lanes": str(part.lanes), "batches": "single" if part.single else "multiple"}
    if part.hashed is not None and part.lanes > 1:
        fields["hashed_on"] = "[" + ", ".join(f"{quoted(schema[k][0])}@{k}" for k in part.hashed) + "]"
    fields["schema"] = _schema_text(schema)
    return fields


def _schema_text(schema) -> str:
    return "[" + ", ".join(f"{quoted(name)}:{type_text}" for name, type_text in schema) + "]"
