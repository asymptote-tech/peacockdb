"""Which node runs next — the prototype of `executor/driver/scheduler.rs`, with its interface:
events in, a pick out, nothing known of backends, batches or executors.

The driver reports each readiness index of a node — a lane of a lane-scoped node or of a
forwarder, an input lane of a partition accumulator, the one input of an emitter — as it changes,
and two holds: a join holds its probe subtree until every lane has left its build phase, and a
satisfied limit holds its subtree, itself included, for good. Holds are counters: one node can sit
in two joins' probe subtrees, and lifting one must not free it from the other. A node is runnable
when a readiness index is set and nothing holds it; the pick is the runnable node of smallest
height, ties leftmost.

With the `prefetch` policy the pick also names the source lanes to read ahead of their turn: those
the driver reports fetchable — a next batch not fetched yet, an empty queue — that nothing holds
and that are not the node about to run. A held source does not read ahead, for the reason it is
held. Without the policy `prefetch` is empty: that is the schedule the driver always had.
"""

from __future__ import annotations

from dataclasses import dataclass

from .node import ExecutorCategory
from .plan import Plan


@dataclass(frozen=True)
class Pick:
    run: int
    #: (source node, lane) to read ahead
    prefetch: tuple[tuple[int, int], ...] = ()


@dataclass(frozen=True)
class JoinShape:
    node: int
    lanes: int
    #: the probe subtree, as the node ids `[start, end)`
    probe: tuple[int, int]


@dataclass(frozen=True)
class PlanShape:
    heights: tuple[int, ...]
    orders: tuple[int, ...]
    #: per node, how many readiness indices it has
    readiness: tuple[int, ...]
    joins: tuple[JoinShape, ...]
    #: per node, its subtree as the node ids `[start, end)` — ids are pre-order
    subtree: tuple[tuple[int, int], ...]

    @classmethod
    def of(cls, plan: Plan, readiness: list[int]) -> PlanShape:
        subtree = [None] * len(plan.nodes)
        for info in reversed(plan.nodes):
            end = max((subtree[child][1] for child in info.children), default=info.id + 1)
            subtree[info.id] = (info.id, end)
        joins = tuple(JoinShape(info.id, info.n_lanes, subtree[info.children[1]])
                      for info in plan.nodes if info.category is ExecutorCategory.JOIN)
        return cls(tuple(i.height for i in plan.nodes), tuple(i.order for i in plan.nodes),
                   tuple(readiness), joins, tuple(subtree))


class Scheduler:
    def __init__(self, shape: PlanShape, prefetch: bool = False):
        """A join holds its probe subtree from time zero: no lane has left build yet, and a
        probe batch produced before `set_build` has nowhere to go. The whole subtree, not the
        probe child: held alone, its own child would keep producing and the pile move one node
        down. It cannot deadlock — a join's build subtree is disjoint from its probe subtree, so
        the build always has a runnable node until it completes, and completing it lifts the
        hold; nested joins resolve outermost-first for the same reason."""
        n = len(shape.heights)
        self._shape = shape
        self._rank = {node: (shape.heights[node], shape.orders[node]) for node in range(n)}
        self._ready = [[False] * count for count in shape.readiness]
        self._ready_lanes = [0] * n
        self._build_holds = [0] * n
        self._limit_holds = [0] * n
        self._building = {join.node: join.lanes for join in shape.joins}
        self._joins = {join.node: join for join in shape.joins}
        self._satisfied = [False] * n
        self._runnable: set[int] = set()
        self._prefetch = prefetch
        self._fetchable: set[tuple[int, int]] = set()
        for join in shape.joins:
            for node in range(*join.probe):
                self._build_holds[node] += 1

    def pick(self) -> Pick | None:
        """The runnable node of smallest height, ties leftmost; None ends the run."""
        if not self._runnable:
            return None
        run = min(self._runnable, key=self._rank.__getitem__)
        if not self._prefetch:
            return Pick(run)
        return Pick(run, tuple(sorted((node, lane) for node, lane in self._fetchable
                                      if node != run and not self.is_held(node))))

    def runnable(self) -> set[int]:
        return set(self._runnable)

    def set_lane_ready(self, node: int, lane: int, ready: bool) -> None:
        """Whether this readiness index can make progress. Idempotent, so the driver may
        recompute a node's indices after any step without tracking what changed."""
        if self._ready[node][lane] == ready:
            return
        self._ready[node][lane] = ready
        self._ready_lanes[node] += 1 if ready else -1
        self._refresh(node)

    def set_fetchable(self, node: int, lane: int, fetchable: bool) -> None:
        """Whether this source lane has a next batch to fetch into an empty place."""
        if fetchable:
            self._fetchable.add((node, lane))
        else:
            self._fetchable.discard((node, lane))

    def lane_left_build(self, join: int) -> None:
        """One lane of a join has run `set_build`. The hold lifts when every lane has, since a
        lane still building still cannot take a probe batch."""
        if self._building[join] == 0:
            raise AssertionError(f"join {join}: a lane left its build phase twice")
        self._building[join] -= 1
        if self._building[join] > 0:
            return
        for node in range(*self._joins[join].probe):
            self._build_holds[node] -= 1
            self._refresh(node)

    def is_building(self, join: int) -> bool:
        """Whether any lane of `join` has yet to leave its build phase."""
        return self._building[join] > 0

    def satisfy(self, node: int) -> None:
        """Enough rows have passed `node` that no later one can change its answer. It holds its
        whole subtree, itself included, and the hold never lifts — so a run ends with lanes not
        done and queues non-empty, which the driver's `_drop_in_flight` is for."""
        if self._satisfied[node]:
            return
        self._satisfied[node] = True
        for held in range(*self._shape.subtree[node]):
            self._limit_holds[held] += 1
            self._refresh(held)

    def is_satisfied(self, node: int) -> bool:
        return self._satisfied[node]

    def any_satisfied(self) -> bool:
        return any(self._satisfied)

    def is_held(self, node: int) -> bool:
        return self._build_holds[node] > 0 or self._limit_holds[node] > 0

    def _refresh(self, node: int) -> None:
        if self._ready_lanes[node] > 0 and not self.is_held(node):
            self._runnable.add(node)
        else:
            self._runnable.discard(node)
