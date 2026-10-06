"""Dynamic filters on the engine's own plans, against DuckDB — answers and pruning.

For each planned query with a dynamic-filter candidate, `dynamic_filters.apply` runs the
probes and re-maps the pruned scans; the pruned plan — its builds the probes' — must still
answer as DuckDB does, and
each pruned scan must keep no more row groups than DuckDB's own dynamic filter would — the
bound it recorded on that column (`testdata/duckdb-dynfilters`) nearest ours, since DuckDB, as
we do, scans a fact table once per use, each use with its own bound.

`PCK_MODE` picks the golden (default `tp4-single`); `PCK_SHARD=k/n` takes a share. Needs the
sf1 datasets and DuckDB: it runs with the corpus files, not the cheap tier.
"""

from __future__ import annotations

if __package__ in (None, ""):  # allow `python scripts/exec_model/tests/optimizer/<file>.py`
    import pathlib as _pathlib, sys as _sys

    _sys.path.insert(0, str(_pathlib.Path(__file__).resolve().parents[4]))
    __package__ = "scripts.exec_model.tests.optimizer"

import importlib.util
import json
import os

from .. import corpus
from ..harness import main
from ..plans.test_engine_answers import ROOT, order_positions
from ...optimizer.dynamic_filters import KeySummary, apply, as_number, candidates, row_group_survives
from ...optimizer.pipeline import mode_shape
from ...plans.engine_nodes import build
from ...plans.engine_plan import EngineNode, read_plans
from ...optimizer.replan import WithMaterialized

MODE = os.environ.get("PCK_MODE", "tp4-single")
BATCHING = mode_shape(MODE).batching


def _duckdb_bounds(bench: str, query: str) -> list[dict]:
    """DuckDB's recorded bounds per scan, parsed by the file that recorded them."""
    spec = importlib.util.spec_from_file_location("duckdb_cost", ROOT / "duckdb_cost.py")
    duckdb_cost = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(duckdb_cost)
    return [duckdb_cost.parse_range_filters(text)
            for text in json.loads((ROOT / "duckdb-dynfilters" / bench / f"{query}.json").read_text())]


def _nearest(bounds, summary):
    low, high = as_number(summary.low), as_number(summary.high)
    distance = lambda ab: abs(as_number(ab[0]) - low) + abs(as_number(ab[1]) - high)
    return min(bounds, key=distance)


def _pruning_test(bench: str, query: str, plan: EngineNode):
    def test():
        tables = corpus.ParquetTables(ROOT / f"{bench}.sf1")
        probe = lambda engine_plan: corpus.execute_lanes(build(engine_plan, tables), corpus.CORPUS_BUDGET)
        pruned, report, kept = apply(plan, tables, BATCHING, probe)
        want = corpus.duckdb_answer(bench, query)
        frames = {name: lanes for name, (_, lanes) in kept.items()}
        got = corpus.execute(build(pruned, WithMaterialized(tables, frames)), corpus.CORPUS_BUDGET)[0]
        got = got.set_axis(list(want.columns), axis=1)
        positions = order_positions(pruned)
        label = f"{bench} {query} {MODE} pruned"
        corpus.matches_oracle(got, want, label, order_by=None if positions is None else
                              [want.columns[at] for at in positions])
        duckdb = _duckdb_bounds(bench, query)
        for scan in report:
            ranges = tables.column_ranges(scan.table, scan.keys[0][0])
            for column, summary in scan.keys:
                bounds = [(d[column]["lo"], d[column]["hi"]) for d in duckdb if column in d]
                if not bounds or summary.low is None:
                    continue
                low, high = _nearest(bounds, summary)
                theirs = sum(row_group_survives(ranges[g], KeySummary(low, high, None)) for g in scan.before)
                assert len(scan.after) <= theirs, (label, scan.table, column, len(scan.after), theirs)

    test.__name__ = f"test_{bench}_{query}".replace("-", "_")
    return test


for _bench in ("tpch", "tpcds"):
    _tables = corpus.ParquetTables(ROOT / f"{_bench}.sf1")
    for _query, _plan in read_plans(ROOT / "goldens" / f"{_bench}.sf1" / f"{MODE}.plans.txt").items():
        if isinstance(_plan, EngineNode) and (ROOT / f"{_bench}-queries" / f"{_query}.sql").exists() \
                and candidates(_plan, _tables):
            _test = _pruning_test(_bench, _query, _plan)
            globals()[_test.__name__] = _test

if __name__ == "__main__":
    raise SystemExit(main(globals()))
