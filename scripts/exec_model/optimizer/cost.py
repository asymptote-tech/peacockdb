"""The enumerator's cost: C_out in bytes — the bytes a plan's joins emit, summed.

A join emits the rows `cardinality.estimate` gives it, each as wide as the columns it passes on
(its projection: what the plan needs above). A column is sized as `common.rs::
type_structural_size` sizes an Arrow column, plus a string's content — its mean length from the
sidecar, on its non-null rows; a string the plan computed has no length to go by and counts its
offset alone. Every term is a join's output, so weights per kind of call would cancel between
two orders: this cost needs no calibration, only estimates.
"""

from __future__ import annotations

from .cardinality import Column, estimate
from ..plans.engine_plan import EngineNode
from .stats import Statistics

JOINS = ("GpuHashJoin", "GpuNestedLoopJoin", "GpuCrossJoin")
#: bytes per value by Arrow type, as `type_structural_size` has them; a string's is its offset
_FIXED = {
    "Boolean": 1 / 8, "Int8": 1, "UInt8": 1, "Int16": 2, "UInt16": 2, "Int32": 4, "UInt32": 4,
    "Float32": 4, "Date32": 4, "Int64": 8, "UInt64": 8, "Float64": 8, "Date64": 8, "Timestamp": 8,
    "Utf8": 4, "Binary": 4, "LargeUtf8": 8, "LargeBinary": 8, "Decimal128": 16, "Decimal256": 32,
}


def row_width(schema, columns: tuple[Column, ...]) -> float:
    """Bytes per row of a node whose output is `schema`, its columns as `cardinality` sees them."""
    width = 0.0
    for (_, type_text), column in zip(schema, columns):
        width += 1 / 8 + _FIXED[type_text.split("(")[0]]
        if column.stats is not None and column.stats.avg_bytes is not None:
            width += column.stats.avg_bytes * (1 - column.null_share)
    return width


def join_bytes(plan: EngineNode, stats: Statistics) -> dict[int, float]:
    """Each join's estimated output bytes, by `id(node)`."""
    estimates = estimate(plan, stats)
    found = {}

    def walk(node):
        if node.kind in JOINS:
            here = estimates[id(node)]
            found[id(node)] = here.rows * row_width(node.schema, here.columns)
        for child in node.children:
            walk(child)

    walk(plan)
    return found


def plan_cost(plan: EngineNode, stats: Statistics) -> float:
    """C_out in bytes: what the plan's joins emit, together."""
    return sum(join_bytes(plan, stats).values())
