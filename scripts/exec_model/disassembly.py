"""A join order back into an engine plan: the tree DPhyp returns, over a MultiJoin's relations,
as `GpuHashJoin` nodes with their wiring derived again.

A tree is a relation's index or a `(build, probe)` pair of trees; which side builds is already
decided by the time it gets here (section 2). Per join, bottom up:

- **keys and residual** — every edge between the two sides' relations; a residual goes on the
  first join where every relation it reads is below;
- **projection** — what the joins above and the cluster's output need, in their order, and
  nothing else; the pure projects the plan had between joins fold into it;
- **wiring** — the translator's (`translator/nodes.rs::hash_join`, `shuffled`, `merged`): at more
  than one lane each side not already hashed on its own keys is shuffled on them, so the join
  is a partitioned one; at one lane a side on several is merged; the build side is then made one
  batch per lane. A join over the same relations as one the plan had keeps that one's lanes —
  DataFusion's choice to collect a small build into one lane; a new join takes the mode's.

The cluster's root keeps the original root's schema, names included: the node above it reads
it by ordinal and checks the name.
"""

from __future__ import annotations

import dataclasses
from dataclasses import dataclass, replace

from .engine_expr import Binary, ColumnRef, expr_text, parse_columns, quoted
from .engine_plan import EngineNode
from .multijoin import MultiJoin, RelColumn, SidedColumn, clusters, is_reorderable, replaced, sided_columns

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
    """The plan for `tree`, its root outputting `cluster.output` under the original root's schema."""
    part = _Disassembly(cluster, lanes, planned_lanes(cluster)).part(tree, list(cluster.output))
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
        build_mask, probe_mask = _mask(build_tree), _mask(probe_tree)
        keys, residuals = self._predicates(build_mask, probe_mask)
        if not keys:
            raise ValueError(f"no key joins relations {build_mask:#b} and {probe_mask:#b}: a hash join needs one")
        read = [c for pair in keys for c in pair] + [c.column for r in residuals for c in sided_columns(r)]
        build = self.part(build_tree, _unique([c for c in need + read if 1 << c.relation & build_mask]))
        probe = self.part(probe_tree, _unique([c for c in need + read if 1 << c.relation & probe_mask]))
        build_keys = [build.columns.index(b) for b, _ in keys]
        probe_keys = [probe.columns.index(p) for _, p in keys]
        lanes = self.planned.get(build_mask | probe_mask, self.mode_lanes)
        build, probe = self._wired(build, build_keys, lanes), self._wired(probe, probe_keys, lanes)
        if not build.single:
            build = self._wrap("GpuCoalesceAllBatches", build, single=True)
        return self._join(build, probe, keys, build_keys, probe_keys, residuals, need)

    def _relation(self, index: int) -> _Part:
        root = self.cluster.relations[index].root
        hashed = tuple(c.index for c in parse_columns(root.fields["hashed_on"])) if "hashed_on" in root.fields else None
        return _Part(root, tuple(RelColumn(index, k) for k in range(len(root.schema))),
                     int(root.fields["lanes"]), root.fields["batches"] == "single", hashed)

    def _predicates(self, build_mask: int, probe_mask: int):
        """The key pairs (build column, probe column) and residuals this join applies."""
        both, keys, residuals = build_mask | probe_mask, [], []
        for edge in self.cluster.edges:
            span = edge.left | edge.right
            if span & ~both or not span & build_mask or not span & probe_mask:
                continue
            if edge.residual is not None:
                residuals.append(edge.residual)
            elif edge.left & build_mask:
                keys += list(edge.keys)
            else:
                keys += [(right, left) for left, right in edge.keys]
        return keys, residuals

    def _wired(self, side: _Part, keys: list[int], lanes: int) -> _Part:
        """The side as `co_partitioned` wants it: on the join's lanes, and at more than one
        hashed on `keys` — `merged` into one lane, or `shuffled` on the keys, where it is not."""
        if side.lanes == lanes and (lanes == 1 or side.hashed == tuple(keys)):
            return side
        if lanes == 1:
            return self._wrap("GpuMergePartitions", side, lanes=1, single=False, hashed=None)
        was_single = side.single
        if side.lanes > 1:
            side = self._wrap("GpuMergePartitions", side, lanes=1, single=False, hashed=None)
        if was_single and not side.single:
            side = self._wrap("GpuCoalesceAllBatches", side, single=True)
        return self._wrap("GpuEmitPartitions", side, lanes=lanes, single=False, hashed=tuple(keys),
                          first={"hash": _columns_text(side, keys)})

    def _wrap(self, kind, side: _Part, first=None, **layout) -> _Part:
        wrapped = replace(side, **layout)
        fields = dict(first or {})
        fields.update(_layout_fields(wrapped, side.node.schema))
        node = EngineNode(kind, fields, side.node.schema, [side.node])
        return replace(wrapped, node=node)

    def _join(self, build, probe, keys, build_keys, probe_keys, residuals, need) -> _Part:
        own = build.columns + probe.columns
        picked = [own.index(c) for c in need]
        schema = tuple(self._column_schema(c) for c in need)
        fields = {"join_type": "Inner",
                  "on": "[" + ", ".join(f"({_ref(build, b)}, {_ref(probe, p)})"
                                        for b, p in zip(build_keys, probe_keys)) + "]"}
        if residuals:
            fields["filter"] = expr_text(_conjunction([self._resided(r, build, probe) for r in residuals]))
        if picked != list(range(len(own))):
            names = [self._column_schema(c)[0] for c in own]
            fields["projection"] = "[" + ", ".join(f"{quoted(names[i])}@{i}" for i in picked) + "]"
        hashed = None
        if build.lanes > 1:
            at = {joined: out for out, joined in enumerate(picked)}
            for keys_here, offset in ((build_keys, 0), (probe_keys, len(build.columns))):
                positions = [at.get(k + offset) for k in keys_here]
                if None not in positions:
                    hashed = tuple(positions)
                    break
        part = _Part(EngineNode("GpuHashJoin", {}, schema, [build.node, probe.node]),
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


def planned_lanes(cluster: MultiJoin) -> dict[int, int]:
    """The lanes of each join the plan had, by the mask of the relations below it."""
    found = {}

    def walk(tree, node):
        while not is_reorderable(node) and id(node) not in roots:
            node = node.children[0]
        if isinstance(tree, tuple):
            found[_mask(tree)] = int(node.fields["lanes"])
            walk(tree[0], node.children[0])
            walk(tree[1], node.children[1])

    roots = {id(r.root) for r in cluster.relations}
    walk(baseline(cluster), cluster.root)
    return found


def _mask(tree: JoinTree) -> int:
    return 1 << tree if isinstance(tree, int) else _mask(tree[0]) | _mask(tree[1])


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
