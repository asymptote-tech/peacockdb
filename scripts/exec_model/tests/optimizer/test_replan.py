"""The replan: a build far off its estimate stops the run, the builds made so far
become memory sources, and a new driver runs the plan optimized again — with nothing the old one
started run twice, and the same answer. Where something outside the builds has started, the
replan is refused and the run goes on."""

from __future__ import annotations

if __package__ in (None, ""):  # allow `python scripts/exec_model/tests/optimizer/<file>.py`
    import pathlib as _pathlib, sys as _sys

    _sys.path.insert(0, str(_pathlib.Path(__file__).resolve().parents[4]))
    __package__ = "scripts.exec_model.tests.optimizer"

from ..corpus import ParquetTables, execute
from ..harness import main
from .test_cardinality import dataset, scan
from ...engine.adaptive import BuildDone
from ...plans.engine_nodes import build
from ...plans.engine_plan import parse_plans
from ...engine.node import CpuBackendSelector
from ...optimizer.multijoin import named
from ...optimizer.replan import run_adaptive

# Dates of year 5 with keys 500..599: 100 rows, which the estimator, taking the two columns as
# independent, puts at 1000 · 1/10 · 1/10 = 10.
def year_five(depth):
    pad = "  " * depth
    return (f"GpuCoalesceAllBatches: lanes=1, batches=single, schema=[dk:Int64]\n"
            f"{pad}  GpuFilter: predicate=((y@1 = 5) AND (dk@0 >= 500)) AND (dk@0 < 600), projection=[dk@0], "
            f"lanes=1, batches=multiple, schema=[dk:Int64]\n"
            f"{pad}    {scan('dates', ['dk', 'y'])}")


L = "lanes=1, batches=multiple"
# spread builds first and is set; the year's dates then build under it, in its probe.
NESTED = f"""GpuUnload
  GpuHashJoin: join_type=Inner, on=[(pk@0, dk@0)], projection=[dk@1, sk@2], {L}, schema=[dk:Int64, sk:Int64]
    GpuCoalesceAllBatches: lanes=1, batches=single, schema=[pk:Int64]
      {scan('spread', ['pk'])}
    GpuHashJoin: join_type=Inner, on=[(dk@0, sk@0)], {L}, schema=[dk:Int64, sk:Int64]
      {year_five(3)}
      {scan('sales', ['sk'])}
"""
# The union's left input streams to the sink before the year's dates are built on its right.
UNION = f"""GpuUnload
  GpuMergePartitions: {L}, schema=[dk:Int64]
    GpuUnion: lanes=2, batches=multiple, schema=[dk:Int64]
      {scan('spread', ['pk']).replace('schema=[pk:Int64]', 'schema=[dk:Int64]')}
      GpuHashJoin: join_type=Inner, on=[(dk@0, sk@0)], projection=[dk@0], {L}, schema=[dk:Int64]
        {year_five(4)}
        {scan('sales', ['sk'])}
"""
# The customers with orders, run whole as the semi join's build, then the year's dates, which stop
# the run: the replan reads that build from memory and the cluster inside it with it.
SEMI = f"""GpuUnload
  GpuHashJoin: join_type=RightSemi, on=[(ck@0, dk@0)], {L}, schema=[dk:Int64, sk:Int64]
    GpuAggregateBatches: group_by=[ck@0], aggs=[], lanes=1, batches=single, schema=[ck:Int64]
      GpuHashJoin: join_type=Inner, on=[(ck@0, ok@0)], projection=[ck@0], {L}, schema=[ck:Int64]
        GpuCoalesceAllBatches: lanes=1, batches=single, schema=[ck:Int64]
          {scan('cust', ['ck'])}
        {scan('orders', ['ok'])}
    GpuHashJoin: join_type=Inner, on=[(dk@0, sk@0)], {L}, schema=[dk:Int64, sk:Int64]
      {year_five(3)}
      {scan('sales', ['sk'])}
"""


def adaptive_run(text):
    """The run, and again reading ahead: the same answer, the same replans at the same events."""
    stats = dataset()
    tables = ParquetTables(stats.directory)
    plan = named(parse_plans(f"== q\n{text}", "test")["q"])
    want, _ = execute(build(plan, tables))
    runs = [run_adaptive(plan, stats, tables, lanes=1, selector=CpuBackendSelector(), prefetch=ahead)
            for ahead in (False, True)]
    for run in runs:
        got = sorted(row for batch in run.results for row in batch.frame.itertuples(index=False))
        assert got == sorted(want.itertuples(index=False))
    stops = [[(d.stopped and d.stopped.rows, d.steps) for d in run.drivers] for run in runs]
    assert stops[0] == stops[1] and len(runs[0].refused) == len(runs[1].refused)
    return runs[0]


def test_a_build_off_its_estimate_is_replanned_with_every_build_made_so_far_kept():
    run = adaptive_run(NESTED)
    first = run.drivers[0]
    assert isinstance(first.stopped, BuildDone) and first.stopped.rows == 100
    extraction = first.extraction()
    # The year's dates and spread, which its join had set and not probed: all that had started.
    assert sorted(len(frame) for _, lanes in extraction.builds.values() for frame in lanes) == [100, 1000]
    assert first.started <= extraction.covered
    assert sum(node.kind == "GpuMemorySource" for node in _nodes(run.plans[1])) == 2
    assert not run.refused


def test_a_replan_is_refused_where_something_outside_the_builds_has_started():
    run = adaptive_run(UNION)
    assert len(run.drivers) == 1 and [event.rows for event in run.refused] == [100]


def test_a_replanned_cluster_keeps_its_id_when_one_before_it_is_read_from_memory():
    run = adaptive_run(SEMI)
    joins = [[(n.fields["join_type"], n.mj) for n in _nodes(plan) if n.kind == "GpuHashJoin"]
             for plan in run.plans]
    assert joins[0] == [("RightSemi", "MJ1"), ("Inner", "MJ2"), ("Inner", "MJ1")]
    assert {mj for _, mj in joins[1]} == {"MJ1"} and len(joins[1]) == 2
    assert [order.mj for order in run.replans[0].orders] == ["MJ1"]


def _nodes(node):
    yield node
    for child in node.children:
        yield from _nodes(child)


if __name__ == "__main__":
    raise SystemExit(main(globals()))
