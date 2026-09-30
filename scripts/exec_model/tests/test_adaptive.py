"""The adaptive loop's events: a build measured whole before its join sets it, a run stopped on
one with the build left queued, and a cap passed mid-accumulation."""

from __future__ import annotations

if __package__ in (None, ""):  # allow `python scripts/exec_model/tests/<file>.py`
    import pathlib as _pathlib, sys as _sys

    _sys.path.insert(0, str(_pathlib.Path(__file__).resolve().parents[3]))
    __package__ = "scripts.exec_model.tests"

from .corpus import execute
from .harness import main
from .test_engine_nodes import OTHER, T, U, tables
from ..adaptive import AdaptiveDriver, BuildDone, BuildExceeds
from ..engine_nodes import build
from ..engine_plan import parse_plans
from ..node import CpuBackendSelector
from ..observed import NdvBounds

HASHED = "lanes=2, batches=multiple, hashed_on=[k@0]"
# u (k = 1, 2, 2, 9) builds in two batches, t probes; both hashed onto two lanes.
JOIN = f"""GpuUnload
  GpuMergePartitions: lanes=1, batches=multiple, schema=[w:Int64, v:Float64]
    GpuHashJoin: join_type=Inner, on=[(k@0, k@0)], projection=[w@1, v@3], lanes=2, batches=multiple, schema=[w:Int64, v:Float64]
      GpuCoalesceAllBatches: lanes=2, batches=single, hashed_on=[k@0], {U}
        GpuEmitPartitions: hash=[k@0], {HASHED}, {U}
          GpuLoadParquet: table=u, projections=[k@0, w@1], partition_groups=[[[0],[1]]], lanes=1, batches=multiple, {U}
      GpuEmitPartitions: hash=[k@0], {HASHED}, {T}
        GpuLoadParquet: table=t, projections=[k@0, v@1, s@2], partition_groups=[[[0],[1],[2],[3]]], lanes=1, batches=multiple, {T}
"""


# The build side an exec node: it emits its batch one step before it knows it has finished.
PROJECTED = f"""GpuUnload
  GpuHashJoin: join_type=Inner, on=[(k@0, k@0)], projection=[w@1, v@3], lanes=1, batches=multiple, schema=[w:Int64, v:Float64]
    GpuProject: exprs=[k@0, w@1], lanes=1, batches=single, {U}
      GpuCoalesceAllBatches: lanes=1, batches=single, {U}
        GpuLoadParquet: table=u, projections=[k@0, w@1], partition_groups=[[[0],[1]]], lanes=1, batches=multiple, {U}
    GpuLoadParquet: table=t, projections=[k@0, v@1, s@2], partition_groups=[[[0],[1],[2],[3]]], lanes=1, batches=multiple, {T}
"""


def adaptive(stops=lambda event: False, caps=None, text=JOIN):
    plan = parse_plans(f"== q\n{text}", "test")["q"]
    join = next(node for node in _preorder(plan) if node.kind == "GpuHashJoin")
    driver = AdaptiveDriver(plan, build(plan, tables()), CpuBackendSelector(), stops=stops,
                            caps={id(join): cap for cap in [caps] if cap is not None})
    return plan, join, driver


def _preorder(node):
    yield node
    for child in node.children:
        yield from _preorder(child)


def states(driver, kind):
    return [s for s in driver.states if s.info.node.name().startswith(kind)]


def test_a_build_is_measured_whole_before_its_join_sets_it():
    calls_at_event = []
    plan, join, driver = adaptive(stops=lambda event: calls_at_event.append(
        [t.call for t in driver.trace if t.node == str(states(driver, "GpuHashJoin")[0].info)]))
    driver.run()
    [coalesce] = states(driver, "GpuCoalesceAllBatches")
    built = sum(size for lane in driver.emitted[coalesce.info.id] for _, size in lane)
    [event] = driver.events
    assert event == BuildDone(join, len(OTHER), built, NdvBounds(3, 3))
    assert calls_at_event == [[]]
    # Continuing leaves the run what it is without the loop.
    got = sorted(b.frame["w"].tolist() for b in driver.results)
    want, _ = execute(build(plan, tables()))
    assert sorted(sum(got, [])) == sorted(want["w"].tolist())


def test_a_build_side_that_has_emitted_but_not_yet_finished_is_done_for_the_event():
    _, join, driver = adaptive(text=PROJECTED)
    driver.run()
    assert [(type(e), e.rows) for e in driver.events] == [(BuildDone, len(OTHER))]


def test_a_run_stopped_on_an_event_leaves_the_build_queued_and_the_probe_unread():
    _, _, driver = adaptive(stops=lambda event: True)
    assert driver.run() == []
    [coalesce] = states(driver, "GpuCoalesceAllBatches")
    queued = [batch for queue in coalesce.out_queues for batch in queue]
    assert len(queued) == 2 and sum(b.num_rows() for b in queued) == driver.stopped.rows
    [join] = states(driver, "GpuHashJoin")
    probe_scan = driver.plan.nodes[join.info.children[1]].children[0]
    assert driver.emitted[probe_scan] == [[]]


def test_a_source_that_fetched_ahead_but_decoded_nothing_has_not_started():
    # Stopped on u's build: t's scan, held as the probe, is made to fetch a batch anyway — the
    # host buffer a replan drops. The run is still one the builds account for whole.
    _, _, driver = adaptive(stops=lambda event: True)
    driver.run()
    [join] = states(driver, "GpuHashJoin")
    probe_scan = driver.states[driver.plan.nodes[join.info.children[1]].children[0]]
    driver._prefetch(probe_scan, 0)
    assert probe_scan.info.id not in driver.started and driver.extraction() is not None


def test_a_cap_passed_mid_accumulation_is_reported_once_before_the_build_is_done():
    _, join, driver = adaptive(caps=1)
    driver.run()
    exceeds, done = driver.events
    assert isinstance(exceeds, BuildExceeds) and isinstance(done, BuildDone)
    assert exceeds.cap == 1 and 1 < exceeds.bytes < done.bytes  # one of u's two batches
    _, _, above = adaptive(caps=10**9)
    above.run()
    assert [type(event) for event in above.events] == [BuildDone]


if __name__ == "__main__":
    raise SystemExit(main(globals()))
