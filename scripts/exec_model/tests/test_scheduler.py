"""`Scheduler` on a shape alone — no plan, no executors: holds are counters, a satisfied limit
holds its subtree for good, and the pick is the runnable node of smallest height, ties leftmost."""

from __future__ import annotations

if __package__ in (None, ""):  # allow `python scripts/exec_model/tests/<file>.py`
    import pathlib as _pathlib, sys as _sys

    _sys.path.insert(0, str(_pathlib.Path(__file__).resolve().parents[3]))
    __package__ = "scripts.exec_model.tests"

from .harness import main, raises
from ..scheduler import JoinShape, Pick, PlanShape, Scheduler

# Pre-order ids. Join A (0) probes 3..5; join B (3), on two lanes, probes 5 — so 5 sits in both.
#
#   0 A ── 1 ── 2
#        └ 3 B ── 4
#               └ 5
SHAPE = PlanShape(
    heights=(0, 1, 2, 1, 2, 2),
    orders=(0, 1, 2, 3, 4, 5),
    readiness=(1, 1, 1, 2, 1, 1),
    joins=(JoinShape(0, 1, (3, 6)), JoinShape(3, 2, (5, 6))),
    subtree=((0, 6), (1, 3), (2, 3), (3, 6), (4, 5), (5, 6)),
)


def all_ready():
    scheduler = Scheduler(SHAPE)
    for node, count in enumerate(SHAPE.readiness):
        for lane in range(count):
            scheduler.set_lane_ready(node, lane, True)
    return scheduler


def test_a_node_in_two_probe_subtrees_is_held_until_both_joins_have_built():
    scheduler = all_ready()
    assert scheduler.is_held(5)
    scheduler.lane_left_build(0)
    assert scheduler.is_held(5) and not scheduler.is_held(4)  # B still builds on both lanes
    scheduler.lane_left_build(3)
    assert scheduler.is_held(5)  # one of B's two lanes
    scheduler.lane_left_build(3)
    assert not scheduler.is_held(5)
    with raises(AssertionError, match="twice"):
        scheduler.lane_left_build(3)


def test_the_pick_is_the_smallest_height_then_the_leftmost_among_the_runnable():
    scheduler = all_ready()
    assert scheduler.pick() == Pick(0)  # the root; the probe subtrees are held
    scheduler.set_lane_ready(0, 0, False)
    assert scheduler.pick() == Pick(1)  # height 1 beats 2; 3 is held
    scheduler.set_lane_ready(1, 0, False)
    scheduler.set_lane_ready(1, 0, False)  # idempotent
    assert scheduler.pick() == Pick(2)
    scheduler.lane_left_build(0)
    assert scheduler.pick() == Pick(3)  # height 1 again, now unheld
    assert scheduler.pick().prefetch == ()


def test_a_satisfied_limit_holds_its_subtree_itself_included_for_good():
    scheduler = all_ready()
    scheduler.lane_left_build(0)
    scheduler.satisfy(3)
    scheduler.satisfy(3)  # idempotent: one hold, not two
    assert all(scheduler.is_held(node) for node in (3, 4, 5)) and not scheduler.is_held(1)
    scheduler.lane_left_build(3)
    scheduler.lane_left_build(3)
    assert scheduler.is_held(5) and scheduler.is_satisfied(3) and scheduler.any_satisfied()
    assert scheduler.runnable() == {0, 1, 2}


def test_a_fetchable_source_lane_is_read_ahead_unless_held_or_about_to_run():
    scheduler = Scheduler(SHAPE, prefetch=True)
    for node in (1, 2):
        scheduler.set_lane_ready(node, 0, True)
    scheduler.set_fetchable(2, 0, True)
    scheduler.set_fetchable(5, 0, True)            # held by both joins
    assert scheduler.pick() == Pick(1, ((2, 0),))
    scheduler.set_lane_ready(1, 0, False)
    assert scheduler.pick() == Pick(2)             # the source itself runs: nothing to fetch ahead
    scheduler.set_fetchable(2, 0, False)
    scheduler.set_lane_ready(0, 0, True)
    assert scheduler.pick() == Pick(0)
    assert all_ready().pick().prefetch == ()       # without the policy, never


def test_a_node_with_no_ready_index_is_not_runnable_however_unheld():
    scheduler = Scheduler(SHAPE)
    assert scheduler.pick() is None
    scheduler.set_lane_ready(3, 1, True)
    scheduler.lane_left_build(0)
    assert scheduler.pick() == Pick(3)
    scheduler.set_lane_ready(3, 1, False)
    assert scheduler.pick() is None


if __name__ == "__main__":
    raise SystemExit(main(globals()))
