"""The schedule the driver kept before `Scheduler`, as a test oracle: every node, every readiness
index, and both hold chains walked to the root, every step. `CheckedDriver` asks it before each
pick and fails where the incremental scheduler would choose otherwise — the prototype's
`the_incremental_schedule_picks_what_a_full_rescan_would`."""

from __future__ import annotations

from ..engine.node import ExecutorCategory
from ..engine.partitioned_driver import PartitionedDriver
from ..engine.runtime import NodeState
from ..engine.single_partition_driver import PROBE_SLOT, JoinPhase


def rescan(driver: PartitionedDriver) -> NodeState | None:
    """The runnable node of smallest height, ties leftmost, found from scratch."""
    candidates = [state for state in driver.states if _runnable(driver, state)]
    return min(candidates, key=lambda s: (s.info.height, s.info.order), default=None)


def _runnable(driver, state) -> bool:
    if _held_by_a_join_build(driver, state) or _held_by_a_satisfied_limit(driver, state):
        return False
    return any(driver._index_ready(state, index) for index in range(driver._readiness_count(state)))


def _held_by_a_join_build(driver, state) -> bool:
    info = state.info
    while info.parent is not None:
        parent = driver.states[info.parent]
        if parent.info.category is ExecutorCategory.JOIN and info.child_slot == PROBE_SLOT \
                and _awaits_build(parent):
            return True
        info = parent.info
    return False


def _awaits_build(join: NodeState) -> bool:
    """A lane with no driver yet has not been entered, so it is still in build."""
    for lane in range(join.info.n_lanes):
        lane_driver = join.lane_drivers.get(lane)
        if lane_driver is None or (not lane_driver.finished and lane_driver.join_phase is JoinPhase.BUILD):
            return True
    return False


def _held_by_a_satisfied_limit(driver, state) -> bool:
    info = state.info
    while True:
        if driver._is_satisfied(driver.states[info.id]):
            return True
        if info.parent is None:
            return False
        info = driver.states[info.parent].info


class CheckedDriver(PartitionedDriver):
    def _pick(self):
        pick, expected = super()._pick(), rescan(self)
        assert (pick and pick.run) == (expected and expected.info.id), \
            f"step {self.steps}: the scheduler picks {pick and self.plan.nodes[pick.run]}, a rescan {expected and expected.info}"
        return pick
