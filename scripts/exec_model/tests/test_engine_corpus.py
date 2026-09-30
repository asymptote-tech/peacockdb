"""Every engine plan golden, built as a prototype plan: it validates, and every node declares
the layout the engine printed for it — lanes, batching, hash and order."""

from __future__ import annotations

if __package__ in (None, ""):  # allow `python scripts/exec_model/tests/<file>.py`
    import pathlib as _pathlib, sys as _sys

    _sys.path.insert(0, str(_pathlib.Path(__file__).resolve().parents[3]))
    __package__ = "scripts.exec_model.tests"

import pathlib

import pandas as pd

from .harness import main
from ..engine_expr import parse_column_orders, parse_columns
from ..engine_nodes import build
from ..engine_plan import EngineNode, read_plans
from ..layout import BatchLayout, KeyDistributionKind
from ..plan import Plan

GOLDENS = pathlib.Path(__file__).resolve().parents[3] / "testdata" / "goldens"
#: more row groups than any sf1 table has: a scan's mapping names only the ones it reads
ROW_GROUPS = 4096


class Shapes:
    """Tables of one row per row group — the shape a plan is built from, with no data, so
    this runs where the datasets do not."""

    def row_counts(self, table):
        return [1] * ROW_GROUPS

    def frame(self, table, columns):
        return pd.DataFrame({column: [0] * ROW_GROUPS for column in columns}, index=range(ROW_GROUPS))


def engine_layout(node):
    fields = node.fields
    hashed = tuple(c.index for c in parse_columns(fields["hashed_on"])) if "hashed_on" in fields else ()
    order = tuple(o.column.index for o in parse_column_orders(fields["sorted_on"])) if "sorted_on" in fields else ()
    return int(fields["lanes"]), fields["batches"], hashed, order


def prototype_layout(node):
    layout = node.output_partitions()
    hashed = layout.key_distribution.hash_keys if layout.key_distribution.kind is KeyDistributionKind.BY_HASH else ()
    batches = "single" if layout.batch_layout is BatchLayout.SINGLE_BATCH else "multiple"
    return layout.n, batches, tuple(hashed), tuple(c.column for c in layout.sort_order.columns)


def test_every_golden_plan_validates_and_declares_the_engines_layout():
    paths = sorted(GOLDENS.glob("*/*.plans.txt"))
    assert {path.parent.name for path in paths} >= {"tpch.sf1", "tpcds.sf1"}, paths
    wrong = []
    for path in paths:
        for name, plan in read_plans(path).items():
            if not isinstance(plan, EngineNode):
                continue
            root = build(plan, Shapes())
            Plan.build(root)
            pairs = [(plan, root)]
            while pairs:
                engine, prototype = pairs.pop()
                pairs.extend(zip(engine.children, prototype.children()))
                if engine.kind != "GpuUnload" and engine_layout(engine) != prototype_layout(prototype):
                    wrong.append((path.name, name, engine.kind, engine_layout(engine),
                                  prototype_layout(prototype)))
    assert not wrong, (len(wrong), wrong[:5])


if __name__ == "__main__":
    raise SystemExit(main(globals()))
