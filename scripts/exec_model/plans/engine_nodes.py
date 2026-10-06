"""The engine's plan trees as prototype plans: one engine node, one prototype node.

Every node's output frame names its columns `frame_names(its schema)`, by position, so an
`engine_ir` reference into a child always lands. Nodes that pass rows through keep their
child's columns, whose schema the engine prints identical; nodes that build columns name them.

`tables` is what the scans read: `frame(table, columns)` — every row, file order — and
`row_counts(table)`, the file's row-group sizes (`tables.ParquetTables`) — and, for a
plan with a `GpuMemorySource`, `materialized(name)`, its frames by lane. `fanouts` gives
a hash join its output rows per probe row, which sizes its scratch; without one it is the
constant 1 the engine's estimator has today.
"""

from __future__ import annotations

import json

from . import engine_ir
from ..optimizer.cardinality import estimate
from .engine_expr import (
    ColumnRef, parse_column_orders, parse_columns, parse_exprs, parse_grouping_sets,
    parse_named_exprs,
)
from .engine_plan import KINDS, PROTOTYPE_KINDS, EngineNode
from ..engine.layout import ColumnOrder
from ..operators import nodes as N
from ..operators.aggregates import PlanAggregate, PlanCall
from ..operators.expressions import Alias
from ..operators.joins import TRIVIAL_FANOUT, Positional
from ..optimizer.stats import Statistics


def build(plan: EngineNode, tables, fanouts: dict[int, float] | None = None) -> N.PandasNode:
    """The prototype plan for an engine plan tree, root included; `fanouts` by `id(node)`."""
    return _Builder(tables, fanouts or {}).node(plan)


def estimated_fanouts(plan: EngineNode, stats: Statistics) -> dict[int, float]:
    """Each hash join's output rows per probe row, as `cardinality.estimate` has them."""
    rows = estimate(plan, stats)
    return {id(n): rows[id(n)].rows / max(rows[id(n.children[1])].rows, 1.0)
            for n in _preorder(plan) if n.kind == "GpuHashJoin"}


def _preorder(node):
    yield node
    for child in node.children:
        yield from _preorder(child)


class _Builder:
    def __init__(self, tables, fanouts):
        self.tables, self.fanouts = tables, fanouts
        self.count = 0

    def node(self, node: EngineNode) -> N.PandasNode:
        children = [self.node(child) for child in node.children]
        self.count += 1
        name = f"{node.kind}#{self.count}"
        return _BUILDERS[node.kind](self, node, name, children)

    def scan(self, node, name, children):
        table = node.fields["table"]
        columns = [column.name for column in parse_columns(node.fields["projections"])]
        frame = self.tables.frame(table, columns)
        frame.columns = engine_ir.frame_names([column for column, _ in node.schema])
        limit = int(node.fields["limit"]) if "limit" in node.fields else None
        groups = json.loads(node.fields["partition_groups"])
        return N.parquet_scan(name, frame, self.tables.row_counts(table), groups, limit)

    def memory(self, node, name, children):
        hashed = ([c.index for c in parse_columns(node.fields["hashed_on"])]
                  if "hashed_on" in node.fields else None)
        return N.memory_source(name, self.tables.materialized(node.fields["name"]), hashed)


def _filter(builder, node, name, children):
    projection, sources = None, list(range(len(node.children[0].schema)))
    if "projection" in node.fields:
        outputs = engine_ir.frame_names([column for column, _ in node.schema])
        kept = engine_ir.lower_field(node, "projection")
        projection = [Alias(column, output) for column, output in zip(kept, outputs)]
        sources = [column.index for column in parse_columns(node.fields["projection"])]
    predicate = engine_ir.lower_field(node, "predicate")
    return N.filter_(name, children[0], predicate, projection, sources)


def _project(_, node, name, children):
    sources = [expr.index if isinstance(expr, ColumnRef) else None
               for expr, _ in parse_named_exprs(node.fields["exprs"])]
    return N.project(name, children[0], engine_ir.lower_field(node, "exprs"), sources)


def _orders(node):
    """`by=` as the sort builders take it: columns, then per key the order and null placement."""
    keys = engine_ir.lower_field(node, "by")
    return (
        [column.column for column, _, _ in keys],
        [ascending for _, ascending, _ in keys],
        [nulls_first for _, _, nulls_first in keys],
    )


def _fetch(node):
    return int(node.fields["fetch"]) if "fetch" in node.fields else None


def _order(node) -> tuple[ColumnOrder, ...]:
    """`by=` as the layout declares it: positions, each with its direction and nulls."""
    return tuple(ColumnOrder(key.column.index, key.ascending, key.nulls_first)
                 for key in parse_column_orders(node.fields["by"]))


def _sorting(builder):
    def build_sort(_, node, name, children):
        by, ascending, nulls_first = _orders(node)
        return builder(name, children[0], by, ascending, nulls_first, _fetch(node), order=_order(node))

    return build_sort


def _accumulating_sort(builder):
    """A sort that holds its lane and owes one batch at done, typed even when empty."""

    def build_sort(_, node, name, children):
        by, ascending, nulls_first = _orders(node)
        return builder(name, children[0], by, ascending, nulls_first, _fetch(node),
                       schema=engine_ir.dtypes(node.schema), order=_order(node))

    return build_sort


def _interval(builder):
    def build_interval(_, node, name, children):
        return builder(name, children[0], int(node.fields.get("skip", 0)), _fetch(node))

    return build_interval


def _join_columns(node) -> Positional:
    build, probe = node.children
    joined = engine_ir.columns(build.schema + probe.schema)
    projection = None
    if "projection" in node.fields:
        # Lowered for its check — every printed name against the join's own output — and
        # then taken by ordinal, which is what the executor picks by.
        engine_ir.lower_field(node, "projection")
        projection = tuple(column.index for column in parse_columns(node.fields["projection"]))
    output = engine_ir.frame_names([column for column, _ in node.schema])
    return Positional(tuple(c.frame for c in joined), len(build.schema), projection, tuple(output))


def _hash_join(builder, node, name, children):
    keys = engine_ir.lower_field(node, "on")
    residual = engine_ir.lower_field(node, "filter") if "filter" in node.fields else None
    return N.positional_hash_join(
        name, children[0], children[1], _join_columns(node), engine_ir.join_type(node),
        [build.column for build, _ in keys], [probe.column for _, probe in keys],
        node.fields.get("null_equals_null") == "true", residual,
        builder.fanouts.get(id(node), TRIVIAL_FANOUT),
    )


def _nested_loop_join(_, node, name, children):
    return N.positional_nested_loop_join(
        name, children[0], children[1], _join_columns(node), engine_ir.join_type(node),
        engine_ir.lower_field(node, "filter"),
    )


def _aggregate_body(node) -> PlanAggregate:
    group_by = parse_exprs(node.fields["group_by"])
    masks = None
    if "grouping_sets" in node.fields:
        # A set lists the keys it holds; the plan's masks are the ones it does not.
        sets = parse_grouping_sets(node.fields["grouping_sets"])
        masks = tuple(tuple(key not in held for key in group_by) for held in sets)
    calls = tuple(PlanCall(call.func, call.args, call.outputs)
                  for call in engine_ir.lower_field(node, "aggs"))
    final = tuple(engine_ir.lower_field(node, "final")) if "final" in node.fields else None
    return PlanAggregate(
        tuple(engine_ir.lower_field(node, "group_by")), calls, masks,
        tuple(column.frame for column in engine_ir.state_columns(node)), final,
        tuple(engine_ir.frame_names([column for column, _ in node.schema])),
    )


def _aggregate(_, node, name, children):
    sources = [key.index if isinstance(key, ColumnRef) else None
               for key in parse_exprs(node.fields["group_by"])]
    return N.plan_aggregate(name, children[0], _aggregate_body(node), sources)


_BUILDERS = {
    "GpuLoadParquet": _Builder.scan,
    "GpuMemorySource": _Builder.memory,
    "GpuFilter": _filter,
    "GpuProject": _project,
    "GpuCoalesceAllBatches": lambda _, node, name, children: N.coalesce_all(
        name, children[0], engine_ir.dtypes(node.schema)
    ),
    "GpuMergePartitions": lambda _, node, name, children: N.merge_partitions(name, children[0]),
    "GpuEmitPartitions": lambda _, node, name, children: N.emit_partitions(
        name, children[0], [column.column for column in engine_ir.lower_field(node, "hash")],
        int(node.fields["lanes"]),
        key_positions=[column.index for column in parse_columns(node.fields["hash"])],
    ),
    "GpuSort": _sorting(N.sort),
    "GpuAccumulateBatchesAndSort": _accumulating_sort(N.accumulate_and_sort),
    "GpuMergeSortedPartitions": _accumulating_sort(N.merge_sorted_partitions),
    "GpuUnion": lambda _, node, name, children: N.union(name, children),
    "GpuInterleave": lambda _, node, name, children: N.interleave(name, children),
    "GpuLimit": _interval(N.limit),
    "GpuUnload": _interval(N.unload),
    "GpuHashJoin": _hash_join,
    "GpuNestedLoopJoin": _nested_loop_join,
    "GpuCrossJoin": lambda _, node, name, children: N.positional_cross_join(
        name, children[0], children[1], _join_columns(node)
    ),
    "GpuAggregate": _aggregate,
    "GpuAggregateBatches": lambda _, node, name, children: N.plan_aggregate_batches(
        name, children[0], _aggregate_body(node), engine_ir.dtypes(node.schema)
    ),
}
assert set(_BUILDERS) == KINDS | PROTOTYPE_KINDS, (KINDS | PROTOTYPE_KINDS) ^ set(_BUILDERS)
