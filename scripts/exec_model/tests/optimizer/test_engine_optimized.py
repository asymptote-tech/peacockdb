"""The engine's plans with every cluster of inner joins reordered by DPhyp and oriented
(`join_order.optimize`), run by the prototype: each still answers as DuckDB does.

The order changes what every join emits, so only the answer is compared. Needs the sf1 datasets,
DuckDB and the DPhyp library (`PEACOCK_DPHYP_LIB`). `PCK_MODE` picks the golden (default
`tp4-single`); `PCK_SHARD=k/n` takes a share.
"""

from __future__ import annotations

if __package__ in (None, ""):  # allow `python scripts/exec_model/tests/optimizer/<file>.py`
    import pathlib as _pathlib, sys as _sys

    _sys.path.insert(0, str(_pathlib.Path(__file__).resolve().parents[4]))
    __package__ = "scripts.exec_model.tests.optimizer"

from .. import corpus
from ..harness import main
from ..plans.test_engine_answers import EMPTY_BY_DESIGN, MODE, ROOT, order_positions
from ...plans.answers import UNORDERED_LIMIT
from ...plans.engine_nodes import build
from ...optimizer.pipeline import mode_shape
from ...plans.engine_plan import EngineNode, read_plans
from ...optimizer.join_order import optimize
from ...optimizer.multijoin import clusters
from ...optimizer.stats import Statistics

LANES = mode_shape(MODE).lanes


def _optimized_test(bench: str, query: str, plan: EngineNode):
    def test():
        optimized = optimize(plan, Statistics(ROOT / f"{bench}.sf1"), LANES)
        want = corpus.duckdb_answer(bench, query)
        assert len(want) or (bench, query) in EMPTY_BY_DESIGN, f"{query}: DuckDB returns no rows"
        got, _ = corpus.execute(build(optimized, corpus.ParquetTables(ROOT / f"{bench}.sf1")), corpus.CORPUS_BUDGET)
        label = f"{bench} {query} {MODE} optimized"
        assert len(got.columns) == len(want.columns), (label, list(got.columns), list(want.columns))
        got = got.set_axis(list(want.columns), axis=1)
        if (bench, query) in UNORDERED_LIMIT:
            assert len(got) == len(want), (label, len(got), len(want))
            return
        positions = order_positions(optimized)
        corpus.matches_oracle(got, want, label,
                              order_by=None if positions is None else [want.columns[at] for at in positions])

    test.__name__ = f"test_{bench}_{query}".replace("-", "_")
    return test


for _bench in ("tpch", "tpcds"):
    for _query, _plan in read_plans(ROOT / "goldens" / f"{_bench}.sf1" / f"{MODE}.plans.txt").items():
        if isinstance(_plan, EngineNode) and (ROOT / f"{_bench}-queries" / f"{_query}.sql").exists() \
                and clusters(_plan):
            _test = _optimized_test(_bench, _query, _plan)
            globals()[_test.__name__] = _test

if __name__ == "__main__":
    raise SystemExit(main(globals()))
