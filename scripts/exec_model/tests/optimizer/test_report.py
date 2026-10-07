"""The optimizer's report of one run (`pipeline.run_optimized`): each rule recorded where it fired,
and nothing where it did not. Needs the DPhyp library."""

from __future__ import annotations

if __package__ in (None, ""):  # allow `python scripts/exec_model/tests/optimizer/<file>.py`
    import pathlib as _pathlib, sys as _sys

    _sys.path.insert(0, str(_pathlib.Path(__file__).resolve().parents[4]))
    __package__ = "scripts.exec_model.tests.optimizer"

from ..corpus import ParquetTables, execute
from ..harness import main
from .test_cardinality import dataset, scan
from .test_optimize import PLAN as REORDERED
from .test_replan import NESTED, UNION
from ...optimizer.join_order import MAX_PAIRS, SetEstimates, ordered
from ...optimizer.cardinality import estimator
from ...optimizer.multijoin import clusters
from ...optimizer.pipeline import mode_shape, run_optimized
from ...optimizer.report import BuildMiss, ChosenJoin, FilterCandidate
from ...plans.engine_nodes import build
from ...plans.engine_plan import parse_plans, plan_text

L = "lanes=1, batches=multiple"
TEN = "[[[" + ",".join(map(str, range(10))) + "]]]"
# Dates and sales in row groups of 100: sales' sk is ordered, 500 + i // 10, so group g holds
# sk 500 + 10g .. 509 + 10g, and the 50 dates below 550 reach groups 0..4 alone.
PRUNABLE = f"""GpuUnload
  GpuHashJoin: join_type=Inner, on=[(dk@0, sk@0)], projection=[sk@1], {L}, schema=[sk:Int64]
    GpuCoalesceAllBatches: lanes=1, batches=single, schema=[dk:Int64]
      GpuFilter: predicate=(y@1 = 5) AND (dk@0 < 550), projection=[dk@0], {L}, schema=[dk:Int64]
        GpuLoadParquet: table=dates, projections=[dk@0, y@1], partition_groups={TEN}, {L}, schema=[dk:Int64, y:Int64]
    GpuLoadParquet: table=sales, projections=[sk@0], partition_groups={TEN}, {L}, schema=[sk:Int64]
"""
# The plan builds the 1000 sales and probes the 100 dates of year 5: orientation turns it round.
BACKWARDS = f"""GpuUnload
  GpuHashJoin: join_type=Inner, on=[(sk@0, dk@0)], {L}, schema=[sk:Int64, dk:Int64]
    GpuCoalesceAllBatches: lanes=1, batches=single, schema=[sk:Int64]
      {scan('sales', ['sk'])}
    GpuFilter: predicate=y@1 = 5, projection=[dk@0], {L}, schema=[dk:Int64]
      {scan('dates', ['dk', 'y'])}
"""
NO_JOIN = f"""GpuUnload
  GpuFilter: predicate=y@1 = 5, projection=[dk@0], {L}, schema=[dk:Int64]
    {scan('dates', ['dk', 'y'])}
"""


def optimized(text, stats=None):
    stats = stats or dataset()
    plan = parse_plans(f"== q\n{text}", "test")["q"]
    tables = ParquetTables(stats.directory)
    run = run_optimized(plan, stats, tables, mode_shape("tp1-single"))
    want, _ = execute(build(plan, tables))
    got = sorted(row for batch in run.adaptive.results for row in batch.frame.itertuples(index=False))
    assert got == sorted(want.itertuples(index=False))
    return plan, run.report


def test_a_dynamic_filter_records_its_candidate_its_keys_and_the_row_groups_it_left():
    _, report = optimized(PRUNABLE, dataset(row_group_size=100))
    [candidate] = report.candidates
    assert candidate == FilterCandidate("dk", "sales", "sk", candidate.clustering)
    assert candidate.clustering < 1.5
    [scan_] = report.pruned
    assert (scan_.table, scan_.before, scan_.after) == ("sales", tuple(range(10)), (0, 1, 2, 3, 4))
    [(column, keys)] = scan_.keys
    assert (column, keys.low, keys.high) == ("sk", 500, 549)
    assert report.probed == (("p0", 50),)
    assert not report.replans and not report.refused


def test_a_dphyp_call_records_its_input_the_sets_it_priced_its_tree_and_each_flip():
    plan, report = optimized(BACKWARDS)
    [order] = report.orders
    assert order.relations == ("LoadParquet sales", "Filter dates")
    assert order.edges == ((0b01, 0b10, "r0.sk = r1.dk"),)
    # Each priced set at the cost the callback returned: the set estimates', unchanged.
    [cluster] = clusters(plan)
    sets = SetEstimates(cluster, estimator(plan, dataset()))
    assert {p.relations for p in order.priced} >= {0b11}
    assert all(abs(p.cost - sets.cost(p.relations)) < 1e-6 and abs(p.rows - sets.rows(p.relations)) < 1e-6
               for p in order.priced)
    assert order.unsolved is None and set(order.tree) == {0, 1}
    assert order.oriented == (1, 0) and order.flipped == ((0b10, 0b01),)
    # The chosen tree's one join, and the plan's order costing the same set.
    assert order.joins == (ChosenJoin(0b10, 0b01, sets.rows(0b11), sets.cost(0b11)),)
    assert order.plan_cost == sets.cost(0b11) and order.max_pairs == MAX_PAIRS
    assert report.before == plan_text(plan) and report.after != report.before
    assert not report.candidates and not report.replans


def test_an_oriented_join_that_keeps_the_plan_s_build_is_no_flip():
    forwards = f"""GpuUnload
  GpuHashJoin: join_type=Inner, on=[(dk@0, sk@0)], {L}, schema=[dk:Int64, sk:Int64]
    GpuCoalesceAllBatches: lanes=1, batches=single, schema=[dk:Int64]
      GpuFilter: predicate=y@1 = 5, projection=[dk@0], {L}, schema=[dk:Int64]
        {scan('dates', ['dk', 'y'])}
    {scan('sales', ['sk'])}
"""
    _, report = optimized(forwards)
    [order] = report.orders
    assert order.oriented == (0, 1) and order.flipped == ()


def test_a_call_past_its_budget_records_the_plan_s_order_and_its_cost():
    stats = dataset()
    plan = parse_plans(f"== q\n{NESTED}", "test")["q"]
    [order] = ordered(plan, stats, 1, max_pairs=1)[1]
    [cluster] = clusters(plan)
    sets = SetEstimates(cluster, estimator(plan, stats))
    # The plan joins year five's dates to sales, then spread to that: relations 1+2, then 0.
    assert (order.unsolved, order.max_pairs) == ("budget", 1)
    assert [(j.build | j.probe) for j in order.joins] == [0b110, 0b111]
    assert order.plan_cost == sum(sets.cost(j.build | j.probe) for j in order.joins)
    assert [j.cost for j in order.joins] == [sets.cost(0b110), sets.cost(0b111)]


def test_the_chosen_tree_is_costed_beside_the_plan_s_own_order():
    # test_optimize's plan: dates and sales first, then spread; DPhyp joins dates to spread first.
    stats = dataset()
    plan = parse_plans(f"== q\n{REORDERED}", "test")["q"]
    [order] = ordered(plan, stats, 1)[1]
    [cluster] = clusters(plan)
    sets = SetEstimates(cluster, estimator(plan, stats))
    dates_and_sales = 0b011
    assert order.plan_cost == sets.cost(dates_and_sales) + sets.cost(0b111)
    assert dates_and_sales not in [j.build | j.probe for j in order.joins]
    assert sum(j.cost for j in order.joins) < order.plan_cost


def test_a_replan_records_the_build_that_missed_and_what_it_kept():
    _, report = optimized(NESTED)
    first = report.replans[0]
    assert (first.miss.rows, round(first.miss.estimate)) == (100, 10)
    assert first.kept and all(rows > 0 for _, rows in first.kept)
    assert [name for name, _ in first.kept] == [f"m{i}" for i in range(len(first.kept))]
    assert not report.refused


def test_a_refused_replan_records_the_miss_and_changes_nothing():
    _, report = optimized(UNION)
    assert report.refused == (BuildMiss(report.refused[0].join, 100, report.refused[0].estimate),)
    assert round(report.refused[0].estimate) == 10 and not report.replans


def test_a_plan_no_rule_touches_records_nothing():
    plan, report = optimized(NO_JOIN)
    assert (report.candidates, report.pruned, report.probed, report.orders, report.replans, report.refused) == \
        ((), (), (), (), (), ())
    assert report.before == report.after == plan_text(plan)


if __name__ == "__main__":
    raise SystemExit(main(globals()))
