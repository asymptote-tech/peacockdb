"""C_out in bytes: a join's rows times the width of what it passes on, strings by their length;
and the fanout a hash join's executor sizes its scratch by."""

from __future__ import annotations

if __package__ in (None, ""):  # allow `python scripts/exec_model/tests/optimizer/<file>.py`
    import pathlib as _pathlib, sys as _sys

    _sys.path.insert(0, str(_pathlib.Path(__file__).resolve().parents[4]))
    __package__ = "scripts.exec_model.tests.optimizer"

import pandas as pd

from ..harness import main
from .test_cardinality import dataset, join, scan
from ...optimizer.cardinality import Column
from ...optimizer.cost import join_bytes, plan_cost, row_width
from ...plans.engine_nodes import build, estimated_fanouts
from ...plans.engine_plan import parse_plans
from ...optimizer.stats import ColumnStats


def test_a_row_is_as_wide_as_arrow_lays_it_out_and_a_string_adds_its_length():
    text = ColumnStats(ndv=10, nulls=0, low="a", high="z", avg_bytes=12.0)
    columns = (Column(("t", "k"), 10), Column(("t", "s"), 10, text, null_share=0.25), Column(None, 10))
    schema = [("k", "Int64"), ("s", "Utf8"), ("p", "Decimal128(15,2)")]
    # a validity bit each; 8 + (4 offset + 12 × ¾ of the rows) + 16
    assert row_width(schema, columns) == 3 / 8 + 8 + 4 + 9 + 16


def plan(text: str):
    return parse_plans(f"== q\nGpuUnload\n{text}\n", "test")["q"]


def test_c_out_is_each_join_s_rows_times_its_width():
    # 40 orders each find their customer: 40 rows of two Int64 columns.
    tree = plan(join("Inner", "ck@0", "ok@0", scan("cust", ["ck"]), scan("orders", ["ok"]), "ck:Int64, ok:Int64"))
    [joined] = tree.children
    assert join_bytes(tree, dataset()) == {id(joined): 40 * (2 / 8 + 16)}
    assert plan_cost(tree, dataset()) == 40 * (2 / 8 + 16)


def test_a_hash_join_s_executor_is_built_with_the_estimated_fanout():
    # The semi join keeps the 5 of 10 customers that have orders: half a row per probe row.
    tree = plan(join("RightSemi", "ok@0", "ck@0", scan("orders", ["ok"]), scan("cust", ["ck"]), "ck:Int64"))
    [joined] = tree.children
    fanouts = estimated_fanouts(tree, dataset())
    assert fanouts == {id(joined): 0.5}
    [join_node] = build(tree, _NoTables(), fanouts).children()
    assert join_node._factory(0).fanout == 0.5
    [join_node] = build(tree, _NoTables()).children()
    assert join_node._factory(0).fanout == 1.0


class _NoTables:
    """Scans are built but never read here."""

    def frame(self, table, columns):
        return pd.DataFrame({c: pd.Series([], dtype="int64") for c in columns})

    def row_counts(self, table):
        return [0]


if __name__ == "__main__":
    raise SystemExit(main(globals()))
