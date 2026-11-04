"""The second pass of dynamic filters over toy tables, end to end (`pipeline.run_optimized`): which
builds it probes after join order, what they prune, and what a probe costs against the plan with
no second pass. Needs the DPhyp library."""

from __future__ import annotations

if __package__ in (None, ""):  # allow `python scripts/exec_model/tests/optimizer/<file>.py`
    import pathlib as _pathlib, sys as _sys

    _sys.path.insert(0, str(_pathlib.Path(__file__).resolve().parents[4]))
    __package__ = "scripts.exec_model.tests.optimizer"

import pyarrow as pa

from ..corpus import ParquetTables, execute, execute_lanes
from ..harness import main
from .test_cardinality import dataset, scan
from .test_report import restricted_late
from .test_replan import year_five
from ...optimizer.cardinality import estimator
from ...optimizer.dynamic_filters import IN_LIMIT
from ...optimizer.join_order import ordered
from ...optimizer.multijoin import named
from ...engine.node import CpuBackendSelector
from ...optimizer.pipeline import filtered, mode_shape, run_optimized
from ...optimizer.replan import run_adaptive
from ...optimizer.report import FilterPass
from ...plans.cost_model import load
from ...plans.engine_nodes import build
from ...plans.engine_plan import parse_plans
from ...plans.engine_run import render_run

L = "lanes=1, batches=multiple"
FOUR = "lanes=4, batches=multiple"
TEN = "[[[" + ",".join(map(str, range(10))) + "]]]"
# tpcds q6's shape: the dates of the years the first thirty dates fall in, joined on the year, the
# build of the spread's join in the same cluster. The estimator gives that year ten values' worth
# of rows, so the dates' keys about 950; truly the year is one, its keys 0–99, and the spread's ten
# row groups of a hundred keep the first alone.
YEAR_OF_THE_FIRST_DAYS = f"""GpuUnload
  GpuHashJoin: join_type=Inner, on=[(dk@0, pk@0)], projection=[pk@1], {L}, schema=[pk:Int64]
    GpuCoalesceAllBatches: lanes=1, batches=single, schema=[dk:Int64]
      GpuHashJoin: join_type=Inner, on=[(y@0, y@1)], projection=[dk@1], {L}, schema=[dk:Int64]
        GpuCoalesceAllBatches: lanes=1, batches=single, schema=[y:Int64]
          GpuAggregateBatches: group_by=[y@0], aggs=[], lanes=1, batches=single, schema=[y:Int64]
            GpuFilter: predicate=dk@0 < 30, projection=[y@1], {L}, schema=[y:Int64]
              {scan('dates', ['dk', 'y'])}
        {scan('dates', ['dk', 'y'])}
    GpuLoadParquet: table=spread, projections=[pk@0], partition_groups={TEN}, {L}, schema=[pk:Int64]
"""


def optimized(text, stats, mode="tp1-single"):
    """The plan optimized and run at `mode`, its answer held to the plan's."""
    plan = parse_plans(f"== q\n{text}", "test")["q"]
    tables = ParquetTables(stats.directory)
    run = run_optimized(plan, stats, tables, mode_shape(mode))
    want, _ = execute(build(plan, tables))
    got = sorted(row for batch in run.adaptive.results for row in batch.frame.itertuples(index=False))
    assert got == sorted(want.itertuples(index=False))
    return plan, run


def test_a_joined_build_with_few_true_keys_and_many_estimated_prunes_after_join_order():
    stats = dataset(row_group_size={"spread": 100})
    plan, run = optimized(YEAR_OF_THE_FIRST_DAYS, stats)
    reordered = ordered(named(plan), stats, 1)[0]
    side = reordered.children[0].children[0]
    assert estimator(reordered, stats).estimates[id(side)].columns[0].ndv > IN_LIMIT
    [scan_] = run.report.filters_after_order.pruned
    assert (scan_.table, scan_.before, scan_.after) == ("spread", tuple(range(10)), (0,))


def test_a_build_that_misses_inside_a_probe_replans_there_and_the_answer_stays():
    # The year's one value against about ten estimated: the probe stops at that build and replans,
    # as the main run would; `optimized` holds the answer to the plan's.
    _, run = optimized(YEAR_OF_THE_FIRST_DAYS, dataset(row_group_size={"spread": 100}))
    [(name, replan)] = run.report.filters_after_order.replans
    assert name == "p0" and (replan.miss.rows, round(replan.miss.estimate)) == (1, 10)
    assert run.stopped, "the probe's stopped run is work done, priced with the others"


#: tpcds q16's shape: a fact under two dimensions, d1's filter on two columns that are one in truth
#: (estimated one row, truly ten), so DPhyp joins d1 first, making 200 rows where d2 first makes 80.
#: The keys the build makes span the big table's ten row groups: nothing is pruned.
FACT_TABLES = {
    "fact": pa.table({"a": [i % 100 for i in range(2000)], "b": [i % 50 for i in range(2000)],
                      "k": list(range(2000))}),
    "d1": pa.table({"x": list(range(100)), "g": [x // 10 for x in range(100)], "h": [x // 10 for x in range(100)]}),
    "d2": pa.table({"z": list(range(50))}),
    "big": pa.table({"bk": list(range(2000))}),
}


def table_scan(table, columns, groups="[[[0]]]"):
    projections = ", ".join(f"{c}@{i}" for i, c in enumerate(columns))
    schema = ", ".join(f"{c}:Int64" for c in columns)
    return (f"GpuLoadParquet: table={table}, projections=[{projections}], partition_groups={groups}, {L}, "
            f"schema=[{schema}]")


MISORDERED_BUILD = f"""GpuUnload
  GpuHashJoin: join_type=Inner, on=[(k@0, bk@0)], projection=[bk@1], {L}, schema=[bk:Int64]
    GpuCoalesceAllBatches: lanes=1, batches=single, schema=[k:Int64]
      GpuAggregateBatches: group_by=[k@0], aggs=[], lanes=1, batches=single, schema=[k:Int64]
        GpuHashJoin: join_type=Inner, on=[(z@0, b@1)], projection=[k@3], {L}, schema=[k:Int64]
          GpuCoalesceAllBatches: lanes=1, batches=single, schema=[z:Int64]
            GpuFilter: predicate=z@0 < 2, {L}, schema=[z:Int64]
              {table_scan('d2', ['z'])}
          GpuHashJoin: join_type=Inner, on=[(x@0, a@0)], projection=[a@1, b@2, k@3], {L}, schema=[a:Int64, b:Int64, k:Int64]
            GpuCoalesceAllBatches: lanes=1, batches=single, schema=[x:Int64]
              GpuFilter: predicate=(g@1 = 0) AND (h@2 = 0), projection=[x@0], {L}, schema=[x:Int64]
                {table_scan('d1', ['x', 'g', 'h'])}
            {table_scan('fact', ['a', 'b', 'k'])}
    {table_scan('big', ['bk'], TEN)}
"""


def priced(runs) -> int:
    model = load()
    return sum(model.price(render_run(plan, driver), "toy").total for plan, driver in runs)


def test_a_probe_whose_build_holds_its_whole_cluster_costs_no_more_than_the_plan_without_it():
    stats = dataset(row_group_size={"big": 200}, tables=FACT_TABLES)
    plan, run = optimized(MISORDERED_BUILD, stats)
    assert run.report.filters_before_order == FilterPass() and run.report.filters_after_order.probed
    [scan_] = run.report.filters_after_order.pruned
    assert scan_.after == scan_.before
    with_probe = priced([(run.adaptive.plans[-1], run.adaptive.drivers[-1])] + run.probes + run.stopped)
    unprobed = run_adaptive(ordered(named(plan), stats, 1)[0], stats, ParquetTables(stats.directory), 1,
                            CpuBackendSelector())
    without = priced(zip(unprobed.plans, unprobed.drivers))
    assert with_probe <= without, (with_probe, without)


#: A LeftMark (no cluster) over a four-lane build side hashed on k, the root of {d1, fact, k2} joined
#: on k. Ordered on d1's estimate (a row, truly ten) d1 joins the fact first; the probe stops at it
#: and orders k2 ⋈ fact first, making a root on `a`, which must go back onto `k` for the marks.
#: The keys are 100–109, the fact's `a` 0–9, so a lane by `a` is not a lane by `k`.
HASHED_PROBE_ROOT = f"""GpuUnload
  GpuMergePartitions: lanes=1, batches=multiple, schema=[k:Int64, mark:Boolean]
    GpuHashJoin: join_type=LeftMark, on=[(k@0, bk@0)], {FOUR}, schema=[k:Int64, mark:Boolean]
      GpuCoalesceAllBatches: lanes=4, batches=single, hashed_on=[k@0], schema=[k:Int64]
        GpuHashJoin: join_type=Inner, on=[(k@0, kk@0)], projection=[k@0], {FOUR}, hashed_on=[k@0], schema=[k:Int64]
          GpuCoalesceAllBatches: lanes=4, batches=single, hashed_on=[k@0], schema=[k:Int64]
            GpuEmitPartitions: hash=[k@0], {FOUR}, hashed_on=[k@0], schema=[k:Int64]
              GpuHashJoin: join_type=Inner, on=[(x@0, a@0)], projection=[k@2], {L}, schema=[k:Int64]
                GpuCoalesceAllBatches: lanes=1, batches=single, schema=[x:Int64]
                  GpuFilter: predicate=(g@1 = 0) AND (h@2 = 0), projection=[x@0], {L}, schema=[x:Int64]
                    {table_scan('d1', ['x', 'g', 'h'])}
                {table_scan('fact', ['a', 'k'])}
          GpuEmitPartitions: hash=[kk@0], {FOUR}, hashed_on=[kk@0], schema=[kk:Int64]
            {table_scan('k2', ['kk'])}
      GpuEmitPartitions: hash=[bk@0], {FOUR}, hashed_on=[bk@0], schema=[bk:Int64]
        {table_scan('big', ['bk'], TEN)}
"""


def test_a_probe_whose_root_is_hashed_keeps_the_hash_through_its_replan_and_the_answer_stays():
    tables = FACT_TABLES | {"k2": pa.table({"kk": list(range(100, 200))})}
    _, run = optimized(HASHED_PROBE_ROOT, dataset(row_group_size={"big": 200}, tables=tables), "tp4-single")
    assert run.report.filters_after_order.replans
    [(probe, _)] = run.probes
    assert (probe.kind, probe.fields["hashed_on"]) == ("GpuCoalesceAllBatches", "[k@0]")


#: test_replan's UNION inside the build of the spread's join: the union's spread branch streams
#: into the aggregate before the year's dates are built, so the probe cannot stop at their miss.
STREAMED_BEFORE_THE_MISS = f"""GpuUnload
  GpuHashJoin: join_type=Inner, on=[(dk@0, pk@0)], projection=[pk@1], {L}, schema=[pk:Int64]
    GpuCoalesceAllBatches: lanes=1, batches=single, schema=[dk:Int64]
      GpuAggregateBatches: group_by=[dk@0], aggs=[], lanes=1, batches=single, schema=[dk:Int64]
        GpuMergePartitions: {L}, schema=[dk:Int64]
          GpuUnion: lanes=2, batches=multiple, schema=[dk:Int64]
            {scan('spread', ['pk']).replace('schema=[pk:Int64]', 'schema=[dk:Int64]')}
            GpuHashJoin: join_type=Inner, on=[(dk@0, sk@0)], projection=[dk@0], {L}, schema=[dk:Int64]
              {year_five(7)}
              {scan('sales', ['sk'])}
    GpuLoadParquet: table=spread, projections=[pk@0], partition_groups={TEN}, {L}, schema=[pk:Int64]
"""


def test_a_miss_inside_a_probe_after_its_plan_has_streamed_is_a_refused_replan():
    _, run = optimized(STREAMED_BEFORE_THE_MISS, dataset(row_group_size={"spread": 100}))
    [(name, miss)] = run.report.filters_after_order.refused
    assert (name, miss.rows, round(miss.estimate)) == ("p0", 100, 10)
    assert not run.report.filters_after_order.replans


def test_a_probe_with_no_miss_runs_once_as_the_build_side_would():
    stats = dataset(row_group_size={"sales": 100})
    plan = parse_plans(f"== q\n{restricted_late('(dk@0 >= 500) AND (dk@0 < 520)')}", "test")["q"]
    tables = ParquetTables(stats.directory)
    stages = filtered(plan, stats, tables, mode_shape("tp1-single"))
    [probe] = stages.probes
    [(name, (side, lanes))] = stages.kept.items()
    assert len(probe.drivers) == 1 and not probe.replans
    assert [frame.to_dict("list") for frame in lanes] == \
        [frame.to_dict("list") for frame in execute_lanes(build(side, tables))]


if __name__ == "__main__":
    raise SystemExit(main(globals()))
