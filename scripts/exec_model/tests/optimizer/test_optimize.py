"""`optimize`: DPhyp on C_out picks the order that makes fewer intermediate rows, orientation
builds the smaller side, and the plan still answers the same. Needs the DPhyp library."""

from __future__ import annotations

if __package__ in (None, ""):  # allow `python scripts/exec_model/tests/optimizer/<file>.py`
    import pathlib as _pathlib, sys as _sys

    _sys.path.insert(0, str(_pathlib.Path(__file__).resolve().parents[4]))
    __package__ = "scripts.exec_model.tests.optimizer"

from ..corpus import ParquetTables, execute
from ..harness import main
from .test_cardinality import dataset, scan
from ...optimizer.cost import plan_cost
from ...plans.engine_nodes import build
from ...plans.engine_plan import parse_plans
from ...optimizer.join_order import optimize

L = "lanes=1, batches=multiple"
DATES = f"""GpuCoalesceAllBatches: lanes=1, batches=single, schema=[dk:Int64]
          GpuFilter: predicate=y@1 = 5, projection=[dk@0], {L}, schema=[dk:Int64]
            {scan('dates', ['dk', 'y'])}"""
# Year 5's 100 dates, joined first to the sales (1000 rows, every one of those dates) and only then
# to the spread (1000 rows, every tenth date): 1000 rows in between where 100 would do.
PLAN = f"""GpuUnload
  GpuHashJoin: join_type=Inner, on=[(dk@0, pk@0)], projection=[sk@1, pk@2], {L}, schema=[sk:Int64, pk:Int64]
    GpuCoalesceAllBatches: lanes=1, batches=single, schema=[dk:Int64, sk:Int64]
      GpuHashJoin: join_type=Inner, on=[(dk@0, sk@0)], {L}, schema=[dk:Int64, sk:Int64]
        {DATES}
        {scan('sales', ['sk'])}
    {scan('spread', ['pk'])}
"""


def rows(plan, tables):
    got, _ = execute(build(plan, tables))
    return sorted(map(tuple, got.to_numpy().tolist()))


def test_the_join_that_keeps_fewer_rows_goes_first_and_the_answer_stays():
    stats = dataset()
    plan = parse_plans(f"== q\n{PLAN}", "test")["q"]
    optimized = optimize(plan, stats, lanes=1)
    [first] = [n for n in _walk(optimized) if n.kind == "GpuHashJoin"
               and not any(m.kind == "GpuHashJoin" for c in n.children for m in _walk(c))]
    assert {n.fields["table"] for n in _walk(first) if n.kind == "GpuLoadParquet"} == {"dates", "spread"}
    assert plan_cost(optimized, stats) < plan_cost(plan, stats)
    data = ParquetTables(stats.directory)
    assert rows(optimized, data) == rows(plan, data)
    assert len(rows(plan, data)) == 1000


def test_dphyp_joins_only_along_keys_and_a_residual_lands_where_its_relations_meet():
    # A residual between sales and spread, which no key joins: however the tree goes, it lands
    # on the join where the two first meet, and every join keeps a key. (A tree joining the two
    # on the residual alone — a hash join with no key — is what disassembly refuses; tpch q7 is
    # the case that made DPhyp pick one before it saw key edges only.)
    text = PLAN.replace("on=[(dk@0, pk@0)],", "on=[(dk@0, pk@0)], filter=sk@build:1 < (pk@probe:0 + 100),")
    stats = dataset()
    optimized = optimize(parse_plans(f"== q\n{text}", "test")["q"], stats, lanes=1)
    joins = [n for n in _walk(optimized) if n.kind == "GpuHashJoin"]
    assert all(n.fields["on"] != "[]" for n in joins)
    assert sum("filter" in n.fields for n in joins) == 1


def _walk(node):
    yield node
    for child in node.children:
        yield from _walk(child)


if __name__ == "__main__":
    raise SystemExit(main(globals()))
