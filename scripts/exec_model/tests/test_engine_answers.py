"""The engine's own plans, built by `engine_nodes` and run by the prototype, against DuckDB.

Every planned query of a mode's `plans.txt` golden that has a SQL text runs over the sf1
tables and is compared with DuckDB's answer to that text (`corpus.matches_oracle`). Two things
in that compare follow from the plan being DataFusion's:

- **Columns are compared by position.** The output's names are DataFusion's
  (`sum(lineitem.l_quantity)`), the oracle's DuckDB's; which engine spells an unaliased
  expression how is not what this test asks.
- **The order is the plan's.** The sort nearest the root, its keys carried through the
  projections above it to output positions, is what the positional half of the compare checks.

`PCK_MODE` picks the golden (default `tp4-single`); `PCK_SHARD=k/n` takes a share. Needs the
sf1 datasets and DuckDB, so it runs with the corpus files, not the cheap tier.
"""

from __future__ import annotations

if __package__ in (None, ""):  # allow `python scripts/exec_model/tests/<file>.py`
    import pathlib as _pathlib, sys as _sys

    _sys.path.insert(0, str(_pathlib.Path(__file__).resolve().parents[3]))
    __package__ = "scripts.exec_model.tests"

import os
import pathlib

from . import corpus
from .harness import main
from ..engine_expr import ColumnRef, parse_column_orders, parse_named_exprs
from ..engine_nodes import build
from ..engine_plan import EngineNode, read_plans

ROOT = pathlib.Path(__file__).resolve().parents[3] / "testdata"
MODE = os.environ.get("PCK_MODE", "tp4-single")
SORTS = frozenset({"GpuMergeSortedPartitions", "GpuAccumulateBatchesAndSort", "GpuSort"})

#: Correct answers that are empty at sf1 — declared, since an empty compare asserts nothing.
EMPTY_BY_DESIGN = frozenset({("tpcds", "q17")})
#: A LIMIT over unordered rows: which rows is not determined, only how many.
UNORDERED_LIMIT = frozenset({("tpch", "scan-limit"), ("tpch", "nested-limits")})


def order_positions(plan: EngineNode) -> list[int] | None:
    """The output positions the plan sorts on, leading keys first; None where it does not
    sort. A key a projection above the sort drops ends the list — later keys order nothing
    the output can show."""
    projects, node = [], plan.children[0]
    while node.kind == "GpuProject":
        projects.append(node)
        node = node.children[0]
    if node.kind not in SORTS:
        return None
    positions = [key.column.index for key in parse_column_orders(node.fields["by"])]
    for project in reversed(projects):
        exprs = parse_named_exprs(project.fields["exprs"])
        sources = {expr.index: at for at, (expr, _) in enumerate(exprs) if isinstance(expr, ColumnRef)}
        carried = []
        for position in positions:
            if position not in sources:
                break
            carried.append(sources[position])
        positions = carried
    return positions


def _answer_test(bench: str, query: str, plan: EngineNode):
    def test():
        want = corpus.duckdb_answer(bench, query)
        assert len(want) or (bench, query) in EMPTY_BY_DESIGN, f"{query}: DuckDB returns no rows"
        got, _ = corpus.execute(build(plan, corpus.ParquetTables(ROOT / f"{bench}.sf1")),
                                corpus.CORPUS_BUDGET)
        label = f"{bench} {query} {MODE}"
        assert len(got.columns) == len(want.columns), (label, list(got.columns), list(want.columns))
        got = got.set_axis(list(want.columns), axis=1)
        if (bench, query) in UNORDERED_LIMIT:
            assert len(got) == len(want), (label, len(got), len(want))
            return
        positions = order_positions(plan)
        order_by = None if positions is None else [want.columns[at] for at in positions]
        corpus.matches_oracle(got, want, label, order_by=order_by)

    test.__name__ = f"test_{bench}_{query}".replace("-", "_")
    return test


for _bench in ("tpch", "tpcds"):
    for _query, _plan in read_plans(ROOT / "goldens" / f"{_bench}.sf1" / f"{MODE}.plans.txt").items():
        if isinstance(_plan, EngineNode) and (ROOT / f"{_bench}-queries" / f"{_query}.sql").exists():
            _test = _answer_test(_bench, _query, _plan)
            globals()[_test.__name__] = _test

if __name__ == "__main__":
    raise SystemExit(main(globals()))
