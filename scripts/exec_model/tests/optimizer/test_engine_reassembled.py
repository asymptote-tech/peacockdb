"""The engine's plans with every cluster of joins disassembled in the order it has, run by the
prototype: each still answers as DuckDB does, and each join emits the rows the engine's own
CPU run of the original plan did (`<mode>-mini.cpu.txt`) — same order, same rows at every join.

What disassembly changes — the projects between joins folded into projections, a discarded
shuffle gone — must change nothing a query answers or a join produces. A query whose tree in the
cpu golden differs from its plan is checked for its answer only. `PCK_MODE` picks the golden
(default `tp4-single`); `PCK_SHARD=k/n` takes a share. Needs the sf1 datasets and DuckDB.
"""

from __future__ import annotations

if __package__ in (None, ""):  # allow `python scripts/exec_model/tests/optimizer/<file>.py`
    import pathlib as _pathlib, sys as _sys

    _sys.path.insert(0, str(_pathlib.Path(__file__).resolve().parents[4]))
    __package__ = "scripts.exec_model.tests.optimizer"

from .. import corpus
from ..harness import main
from ..plans.test_engine_answers import EMPTY_BY_DESIGN, MODE, ROOT, UNORDERED_LIMIT, order_positions
from ...optimizer.disassembly import reassembled
from ...plans.engine_nodes import build
from ...plans.engine_plan import EngineNode, read_plans
from ...optimizer.multijoin import clusters

JOINS = ("GpuHashJoin", "GpuNestedLoopJoin", "GpuCrossJoin")
LANES = int(MODE[2])


def _preorder(node):
    yield node
    for child in node.children:
        yield from _preorder(child)


def join_rows(plan: EngineNode, driver) -> list[int]:
    """Each join's output rows, pre-order, as the prototype's run of `plan` emitted them."""
    ids = {id(info.node): info.id for info in driver.plan.nodes}
    found, pairs = [], [(plan, driver.plan.nodes[driver.plan.root].node)]
    while pairs:
        node, prototype = pairs.pop()
        if node.kind in JOINS:
            found.append(sum(rows for lane in driver.emitted[ids[id(prototype)]] for rows, _ in lane))
        pairs.extend(reversed(list(zip(node.children, prototype.children()))))
    return found


def _reassembled_test(bench: str, query: str, plan: EngineNode, truth):
    def test():
        rebuilt = reassembled(plan, LANES)
        want = corpus.duckdb_answer(bench, query)
        assert len(want) or (bench, query) in EMPTY_BY_DESIGN, f"{query}: DuckDB returns no rows"
        got, driver = corpus.execute(build(rebuilt, corpus.ParquetTables(ROOT / f"{bench}.sf1")),
                                     corpus.CORPUS_BUDGET)
        label = f"{bench} {query} {MODE} reassembled"
        assert len(got.columns) == len(want.columns), (label, list(got.columns), list(want.columns))
        got = got.set_axis(list(want.columns), axis=1)
        if (bench, query) in UNORDERED_LIMIT:
            assert len(got) == len(want), (label, len(got), len(want))
        else:
            positions = order_positions(rebuilt)
            order_by = None if positions is None else [want.columns[at] for at in positions]
            corpus.matches_oracle(got, want, label, order_by=order_by)
        if truth is not None and [n.kind for n in _preorder(plan)] == [kind for kind, _ in truth]:
            engine = [rows for kind, rows in truth if kind in JOINS]
            assert join_rows(rebuilt, driver) == engine, (label, join_rows(rebuilt, driver), engine)

    test.__name__ = f"test_{bench}_{query}".replace("-", "_")
    return test


for _bench in ("tpch", "tpcds"):
    _truth = corpus.cpu_rows(ROOT / "goldens" / f"{_bench}.sf1" / f"{MODE}-mini.cpu.txt")
    for _query, _plan in read_plans(ROOT / "goldens" / f"{_bench}.sf1" / f"{MODE}.plans.txt").items():
        if isinstance(_plan, EngineNode) and (ROOT / f"{_bench}-queries" / f"{_query}.sql").exists() \
                and clusters(_plan):
            _test = _reassembled_test(_bench, _query, _plan, _truth.get(_query))
            globals()[_test.__name__] = _test

if __name__ == "__main__":
    raise SystemExit(main(globals()))
