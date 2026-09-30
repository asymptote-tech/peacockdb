"""The events of the adaptive loop: the points of a run where the planner learns a
true size and may act on it. `AdaptiveDriver` runs an engine plan as `PartitionedDriver` does and
after every step hands each new event to `stops`; the event it stops on ends the run there, the
materializations left in their queues for the replan to take.

`BuildDone` is a join's build side having emitted its one batch on every lane, before the join
sets a build on any — emitted, not finished: an exec node learns it has finished a step later,
by which time the join has taken the batch. No hold is needed: a build on several lanes is
hashed, a hashed stream emits on every lane in one step, and the check runs after every step.
`BuildExceeds` is what a build side accumulated passing its cap, summed over lanes.
A dynamic filter's `probe_plan_done` needs nothing here: its probe plan is a run of its own,
finished before the main plan starts (`dynamic_filters.apply`).

A stopped run's `extraction` is what a replan may take from it: the build just done, and those of
joins that set theirs and have probed nothing. Only where nothing else has started — else the
new driver would run it again — which is also the rule of not replanning through a started
accumulator, taken whole: the replan is refused rather than cut below it. Bytes a
source fetched ahead and has not decoded are no start: a host buffer, dropped with the run, and
the new driver's source reads that batch in turn.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Callable

import pandas as pd

from . import engine_ir
from .engine_expr import parse_columns, parse_join_keys
from .engine_plan import EngineNode
from .errors import DriverError
from .node import BackendSelector, ExecutorCategory
from .observed import NdvBounds, observed_ndv
from .partitioned_driver import PartitionedDriver
from .plan import Plan
from .runtime import NodeState
from .single_partition_driver import BUILD_SLOT, JoinPhase


@dataclass(frozen=True)
class BuildDone:
    join: EngineNode
    rows: int
    bytes: int
    #: the NDV of the join's build keys; None for a join without keys
    key_ndv: NdvBounds | None


@dataclass(frozen=True)
class BuildExceeds:
    join: EngineNode
    cap: int
    #: what the build side held when it passed `cap` — a lower bound on its size
    bytes: int


Event = BuildDone | BuildExceeds


@dataclass(frozen=True)
class Extraction:
    #: per engine join whose build is taken, by `id`: the join and its build's frames by lane
    builds: dict[int, tuple[EngineNode, list[pd.DataFrame]]]
    #: the prototype nodes those builds and their joins account for
    covered: frozenset[int]


class AdaptiveDriver(PartitionedDriver):
    """`root` is `engine_nodes.build(plan, …)`; `caps` are build caps in bytes, by `id` of the
    engine join. A run `stopped` on an event ends with batches still queued."""

    def __init__(self, plan: EngineNode, root, selector: BackendSelector, budget: int | None = None,
                 stops: Callable[[Event], bool] = lambda event: False,
                 caps: dict[int, int] | None = None, prefetch: bool = False):
        super().__init__(Plan.build(root), selector, budget, prefetch)
        ids = {id(info.node): info.id for info in self.plan.nodes}
        #: engine joins by prototype node id
        self.joins: dict[int, EngineNode] = {}
        pairs = [(plan, root)]
        while pairs:
            node, built = pairs.pop()
            if self.plan.nodes[ids[id(built)]].category is ExecutorCategory.JOIN:
                self.joins[ids[id(built)]] = node
            pairs.extend(zip(node.children, built.children()))
        caps = caps or {}
        self._caps = {at: caps[id(join)] for at, join in self.joins.items() if id(join) in caps}
        self._built: set[int] = set()
        #: per join, its build's frames by lane as `BuildDone` found them
        self._builds: dict[int, list[pd.DataFrame]] = {}
        self._calls: dict[int, set[str]] = {}
        #: the prototype nodes that have made a call
        self.started: set[int] = set()
        self.stops = stops
        self.events: list[Event] = []
        self.stopped: Event | None = None

    def step(self) -> bool:
        if self.stopped is not None or not super().step():
            return False
        for event in self._new_events():
            self.events.append(event)
            if self.stops(event):
                self.stopped = event
                break
        return True

    def extraction(self) -> Extraction | None:
        """What a replan takes from this run, stopped on a `BuildDone`; None where a node outside
        the builds and their joins has started."""
        builds, covered = {}, set()
        for at, join in self.joins.items():
            calls = self._calls.get(at, set())
            fresh = at in self._built and not calls
            waiting = calls == {"set_build"} and not self._awaits_build(self.states[at])
            if fresh or waiting:
                builds[id(join)] = (join, self._builds[at])
                covered |= self._subtree(self.plan.nodes[at].children[BUILD_SLOT]) | {at}
        return Extraction(builds, frozenset(covered)) if self.started <= covered else None

    def _subtree(self, at: int) -> set[int]:
        found, stack = set(), [at]
        while stack:
            found.add(node := stack.pop())
            stack.extend(self.plan.nodes[node].children)
        return found

    def _record(self, state: NodeState, lane: int, call: str, n_out: int) -> None:
        super()._record(state, lane, call, n_out)
        if call != "prefetch":
            self.started.add(state.info.id)
            self._calls.setdefault(state.info.id, set()).add(call)

    def _assert_drained(self) -> None:
        if self.stopped is None:
            super()._assert_drained()

    def _new_events(self):
        """Lazily, so that an event after the one the run stops on stays unreported."""
        for at, join in self.joins.items():
            if at in self._built:
                continue
            build = self._build_side(at)
            if at in self._caps and (held := self._accumulated(build)) > self._caps[at]:
                yield BuildExceeds(join, self._caps.pop(at), held)
            if all(self.emitted[build.info.id]):
                self._built.add(at)
                yield self._build_done(at, join, build)

    def _build_side(self, join: int) -> NodeState:
        return self.states[self.plan.nodes[join].children[BUILD_SLOT]]

    def _accumulated(self, build: NodeState) -> int:
        """What the build side holds and has emitted: a coalesce holds its batches apart
        until done."""
        held = sum(driver.resident_bytes() for driver in build.lane_drivers.values())
        if build.cross_executor is not None:
            held += build.cross_executor.resident_bytes()
        return held + sum(size for lane in self.emitted[build.info.id] for _, size in lane)

    def _build_done(self, at: int, join: EngineNode, build: NodeState) -> BuildDone:
        if any(lane.join_phase is not JoinPhase.BUILD for lane in self.states[at].lane_drivers.values()):
            raise DriverError(f"{self.plan.nodes[at]}: a build was set before its side emitted on every lane")
        batches = [batch for queue in build.out_queues for batch in queue]
        self._builds[at] = [batch.frame for batch in batches]
        rows, size = sum(b.num_rows() for b in batches), sum(b.byte_size() for b in batches)
        if join.kind != "GpuHashJoin":
            return BuildDone(join, rows, size, None)
        side = join.children[BUILD_SLOT]
        names = engine_ir.frame_names([column for column, _ in side.schema])
        keys = [names[key.index] for key, _ in parse_join_keys(join.fields["on"])]
        return BuildDone(join, rows, size, observed_ndv(self._builds[at], keys, hashed_names(side)))


def hashed_names(node: EngineNode) -> list[str] | None:
    """The frame names of the columns `node`'s lanes are hashed on; None where they are not."""
    if "hashed_on" not in node.fields:
        return None
    names = engine_ir.frame_names([column for column, _ in node.schema])
    return [names[c.index] for c in parse_columns(node.fields["hashed_on"])]

