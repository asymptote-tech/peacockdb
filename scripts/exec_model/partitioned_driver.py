"""`partitioned_driver` — the scheduler and everything cross-partition.

**The strategy.** Every node carries a height (distance to the root) and a left-to-right
order, both computed once in `plan.py`. A node is *runnable* when any of its partitions
can make progress: a source always can, and any other node can once its inputs for that
lane hold a batch or are known to be finished. Among all runnable nodes the driver picks
the one with the smallest height, breaking ties leftmost, and then runs **every** lane of
that node.

Min-height-first is what makes this a push model: as soon as a node produces a batch its
parent becomes runnable at a strictly lower height, so the batch is carried up the tree
before anything below produces again. It stops at a batch accumulator, a partition
accumulator, or the sink — which is also the livelock argument. The one place a batch can
be blocked is a join whose other side has not arrived; orienting the tree so the build
side is always the left child removes that wait, because at equal heights the leftmost
node wins and the build subtree drains first.

**Queues stay bounded at one batch per lane, with no explicit cap.** A producer's
out-queue is drained by its parent before the producer runs again, since the parent's
height is strictly lower. The one shape that breaks it is a join in its build phase — it
cannot consume a probe batch yet, so nothing drains its probe child — and that is closed
by holding the join's whole probe subtree until the build is set (`Scheduler`'s join hold).
With the hold in place the bound is unconditional, and the draft's cap-Q mechanism is
unnecessary — nothing here caps a queue.

Lane-scoped work is delegated to `single_partition_driver`; this driver owns the
tree, the queues and the three cross-lane categories, and tells `scheduler.Scheduler` what
changed — a readiness index, a join lane leaving build, a limit satisfied — which picks the node.
"""

from __future__ import annotations

from dataclasses import dataclass

from .accounting import ResidentAccountant
from .batch import Batch
from .errors import DriverError
from .executors import LaneEvent
from .node import LANE_SCOPED, BackendSelector, ExecutorCategory
from .plan import Plan, PlanNodeInfo
from .runtime import LaneInputs, NodeState
from .scheduler import Pick, PlanShape, Scheduler
from .single_partition_driver import single_partition_driver


@dataclass(frozen=True)
class TraceEvent:
    step: int
    node: str
    lane: int
    call: str
    n_out: int


class PartitionedDriver:
    def __init__(self, plan: Plan, selector: BackendSelector, budget: int | None = None,
                 prefetch: bool = False):
        self.plan = plan
        self.selector = selector
        self.accountant = ResidentAccountant(budget)
        self.results: list[Batch] = []
        #: the root's output by its lane — what a plan whose root is not a sink hands on
        self.root_lanes: list[list[Batch]] = [[] for _ in range(plan.nodes[plan.root].n_lanes)]
        self.trace: list[TraceEvent] = []
        self.steps = 0
        #: every traced call but a prefetch, which is what `step_cap` owes: executor calls,
        #: forwarder moves, closings, and an unload's release of an unwanted batch
        self.calls = 0
        #: per node: rows of its input stream seen so far, summed over every lane. Only
        #: the driver can hold this — an Unload instance is one lane's.
        self.rows_seen: list[int] = [0] * len(plan.nodes)
        #: rows released without an unload call, per node — the saving, made visible
        self.rows_skipped: list[int] = [0] * len(plan.nodes)
        self.states: list[NodeState] = [
            NodeState.create(info, self._input_lane_count(info)) for info in plan.nodes
        ]
        self.peak_queued: list[int] = [0] * len(plan.nodes)
        #: per node, per its lane: (rows, bytes) of every batch it emitted, in order
        self.emitted: list[list[list[tuple[int, int]]]] = [
            [[] for _ in range(info.n_lanes)] for info in plan.nodes
        ]
        #: per node, per child slot, per that child's lane: rows it took
        self.consumed: list[list[list[int]]] = [
            [[0] * plan.nodes[child].n_lanes for child in info.children] for info in plan.nodes
        ]
        self._prefetching = prefetch
        #: host bytes fetched ahead and not yet decoded, by (source node, lane); and their peak
        self.fetched: dict[tuple[int, int], int] = {}
        self.fetched_peak = 0
        readiness = [self._readiness_count(s) for s in self.states]
        self.scheduler = Scheduler(PlanShape.of(plan, readiness), prefetch)
        for state in self.states:
            self._refresh(state)
        #: calls owed beyond the sources' batches: each readiness index's closing call, and
        #: one per batch queued below the root, for the call that takes it
        self._calls_owed = sum(readiness)
        self._source_lanes = [self._lane_driver(state, lane) for state in self.states
                              if state.info.category is ExecutorCategory.SOURCE
                              for lane in range(state.info.n_lanes)]

    # -- public ------------------------------------------------------------------

    def run(self) -> list[Batch]:
        self._settle_limits()   # a zero-row interval is satisfied before anything runs
        while self.step():
            if max(self.steps, self.calls) > (cap := self.step_cap()):
                raise DriverError(f"no termination after {self.steps} steps and {self.calls} calls: "
                                  f"the run owes {cap}")
        if self.early_exit:
            self._drop_in_flight()
        else:
            self._assert_drained()
        return self.results

    def step_cap(self) -> int:
        """The most calls, and so steps, a run that progresses can have made by now.

        Every call is owed: a source lane's batch, by `max_batches` once the lane has an
        executor; a batch queued below the root, by the one call that takes it; or a readiness
        index's one closing call. A step makes at least one call, so steps outrun this where a
        driver stops making calls, and calls outrun it where a source produces past its count.
        Not a constant, because stacked shuffles multiply a plan's batches (README, "The step cap").
        """
        return self._calls_owed + sum(lane.max_batches() for lane in self._source_lanes)

    @property
    def early_exit(self) -> bool:
        """The run stopped because a limit was satisfied, not because work ran out."""
        return self.scheduler.any_satisfied()

    def satisfied(self) -> list[int]:
        """The nodes whose row interval the run satisfied — the early exits, by node id."""
        return [state.info.id for state in self.states if self.scheduler.is_satisfied(state.info.id)]

    def step(self) -> bool:
        """Run one node — every lane of it — after the source lanes the pick reads ahead.
        False when nothing is runnable."""
        pick = self._pick()
        if pick is None:
            return False
        self.steps += 1
        for node, lane in pick.prefetch:
            self._prefetch(self.states[node], lane)
        chosen = self.states[pick.run]
        self._run(chosen)
        # A node's readiness is a fact about its inputs: the step changed the node's own and,
        # through its queues, its parent's; a child's queue it took from is what makes a source
        # lane fetchable again.
        for state in (chosen, *(self.states[c] for c in chosen.info.children)):
            self._refresh(state)
        if chosen.info.parent is not None:
            self._refresh(self.states[chosen.info.parent])
        self._settle_limits()
        self._drain_root()
        return True

    def runnable_nodes(self) -> list[PlanNodeInfo]:
        return [self.plan.nodes[node] for node in sorted(self.scheduler.runnable())]

    # -- scheduling --------------------------------------------------------------

    def choose(self) -> NodeState | None:
        """The scheduling decision: smallest height among runnable nodes, ties leftmost."""
        pick = self._pick()
        return None if pick is None else self.states[pick.run]

    def _pick(self) -> Pick | None:
        return self.scheduler.pick()

    def _prefetch(self, state: NodeState, lane: int) -> None:
        """The host half of a source's next call: its bytes, held for the decode."""
        self.fetched[(state.info.id, lane)] = self._lane_driver(state, lane).executor.prefetch()
        self.fetched_peak = max(self.fetched_peak, sum(self.fetched.values()))
        self.scheduler.set_fetchable(state.info.id, lane, False)
        self._record(state, lane, "prefetch", 0)

    def _readiness_count(self, state: NodeState) -> int:
        category = state.info.category
        if category is ExecutorCategory.PARTITION_EMITTER:
            return 1
        if category is ExecutorCategory.PARTITION_ACCUMULATOR:
            return len(state.lane_done_sent)
        return state.info.n_lanes

    def _refresh(self, state: NodeState) -> None:
        for index in range(self._readiness_count(state)):
            self.scheduler.set_lane_ready(state.info.id, index, self._index_ready(state, index))
        if self._prefetching and state.info.category is ExecutorCategory.SOURCE:
            for lane in range(state.info.n_lanes):
                self.scheduler.set_fetchable(state.info.id, lane, self._fetchable(state, lane))

    def _fetchable(self, state: NodeState, lane: int) -> bool:
        """A next batch to fetch, and an empty place in the queue for it to go once decoded."""
        driver = self._lane_driver(state, lane)
        return not driver.finished and not state.out_queues[lane] and driver.executor.can_prefetch()

    def _index_ready(self, state: NodeState, index: int) -> bool:
        category = state.info.category
        if category in LANE_SCOPED:
            return self._lane_driver(state, index).can_step(self._lane_inputs(state, index))
        if category is ExecutorCategory.PARTITION_EMITTER:
            inputs = self._lane_inputs(state, 0)
            return inputs.has(0) or (inputs.done(0) and not state.emitter_finished)
        if category is ExecutorCategory.PARTITION_ACCUMULATOR:
            return self._accumulator_lane_ready(state, index)
        if category is ExecutorCategory.BATCH_FORWARDER:
            return self._forwarder_lane_ready(state, index)
        raise DriverError(f"{state.info}: unhandled category {category.value}")

    # -- backpressure ------------------------------------------------------------

    def _is_satisfied(self, state: NodeState) -> bool:
        """Enough rows have reached this node that no later one can change its answer."""
        interval = state.info.node.row_interval()
        return interval is not None and interval.satisfied_by(self.rows_seen[state.info.id])

    def _settle_limits(self) -> None:
        """A satisfied limit will never produce again, so say so before anything waits.

        Without this its hold would also stop the node itself from reporting done, and its
        parent would wait forever for a lane that had in fact finished. The pathological case
        is a zero-row interval, satisfied before a single step: the plan has to complete and
        return nothing, not stall.
        """
        for state in self.states:
            if self._is_satisfied(state) and not self.scheduler.is_satisfied(state.info.id):
                self.scheduler.satisfy(state.info.id)
                state.out_done = [True] * len(state.out_done)
                if state.info.parent is not None:
                    self._refresh(self.states[state.info.parent])

    def _awaits_build(self, state: NodeState) -> bool:
        """True while any of the join's lanes has yet to leave its build phase."""
        return self.scheduler.is_building(state.info.id)

    # -- running -----------------------------------------------------------------

    def _run(self, state: NodeState) -> None:
        category = state.info.category
        if category in LANE_SCOPED:
            self._run_lane_scoped(state)
        elif category is ExecutorCategory.PARTITION_EMITTER:
            self._run_emitter(state)
        elif category is ExecutorCategory.PARTITION_ACCUMULATOR:
            self._run_partition_accumulator(state)
        elif category is ExecutorCategory.BATCH_FORWARDER:
            self._run_forwarder(state)
        self.peak_queued[state.info.id] = max(
            self.peak_queued[state.info.id], state.queued_batches()
        )

    def _run_lane_scoped(self, state: NodeState) -> None:
        interval = self._interval_of(state)
        unloading = state.info.category is ExecutorCategory.UNLOAD
        for lane in range(state.info.n_lanes):
            driver = self._lane_driver(state, lane)
            inputs = self._lane_inputs(state, lane)
            if not driver.can_step(inputs):
                continue
            rows = None
            arriving = 0
            if interval is not None and inputs.has(0):
                arriving = inputs.peek(0).num_rows()
                # Only an unload's drop-narrow-or-pass decision is made here, because its
                # range is an argument of the driver's own call. A mid-plan limit makes
                # the same three-way decision inside its executor — releasing, forwarding,
                # or slicing through `peacock_executor_slice_handle` — so for it the
                # driver only counts, to feed `is_satisfied`. See `accumulators.LimitStream`.
                if unloading:
                    rows = interval.range_of(self.rows_seen[state.info.id], arriving)
                    if rows is None:
                        # Not one row is wanted, so it never crosses the boundary: the
                        # handle is released here. Unbounded saving on the skip prefix,
                        # and the property a test on the rows returned cannot see.
                        unwanted = inputs.take(0)
                        self.rows_seen[state.info.id] += arriving
                        self.rows_skipped[state.info.id] += unwanted.num_rows()
                        self.accountant.release(unwanted)
                        self._record(state, lane, "release/unwanted", 0)
                        continue
                    if rows.covers(arriving):
                        rows = None   # every row wanted: the fetch needs no range
            result = driver.step(inputs, rows)
            self.fetched.pop((state.info.id, lane), None)
            if result.call == "set_build":
                self.scheduler.lane_left_build(state.info.id)
            self.rows_seen[state.info.id] += arriving
            for batch in result.outputs:
                self._enqueue(state, lane, batch)
            if result.finished:
                state.out_done[lane] = True
            self._record(state, lane, result.call, len(result.outputs))

    def _interval_of(self, state: NodeState):
        """This node's row interval: an absorbed root-adjacent limit, or a mid-plan one."""
        return state.info.node.row_interval()

    def _run_emitter(self, state: NodeState) -> None:
        inputs = self._lane_inputs(state, 0)
        executor = self._cross_executor(state)
        if not inputs.has(0):
            state.emitter_finished = True
            for lane in range(state.info.n_lanes):
                state.out_done[lane] = True
            self.accountant.forget(str(state.info))
            self._record(state, 0, "emit/done", 0)
            return
        batch = inputs.take(0)
        label = str(state.info)
        modelled = self.accountant.begin_call(label, executor, batch.num_rows(), batch.byte_size())
        outputs, stats = executor.emit(batch)
        self.accountant.release(batch)
        if len(outputs) != state.info.n_lanes:
            raise DriverError(
                f"{state.info}: emit returned {len(outputs)} lanes, expected {state.info.n_lanes}"
            )
        emitted = 0
        for lane, out in enumerate(outputs):
            # Empty scatter outputs are dropped here, so nothing empty ever traverses a
            # chain because of hash skew.
            if out.num_rows() == 0:
                continue
            self._enqueue(state, lane, out)
            emitted += 1
        self.accountant.end_call(label, executor, stats, modelled)
        self._record(state, 0, "emit", emitted)

    def _run_partition_accumulator(self, state: NodeState) -> None:
        executor = self._cross_executor(state)
        child = self.states[state.info.children[0]]
        for lane in range(len(state.lane_done_sent)):
            inputs = LaneInputs([(child, lane)], self.consumed[state.info.id])
            label = str(state.info)
            if inputs.has(0):
                batch = inputs.take(0)
                modelled = self.accountant.begin_call(
                    label, executor, batch.num_rows(), batch.byte_size()
                )
                event, call = LaneEvent.of(batch), "accumulate_and_fetch"
            elif inputs.done(0) and not state.lane_done_sent[lane]:
                state.lane_done_sent[lane] = True
                modelled = self.accountant.begin_call(label, executor, 0, 0)
                event, call = LaneEvent.done(), "accumulate_and_fetch/done"
            else:
                continue
            outputs, stats = executor.accumulate_and_fetch(lane, event)
            if event.batch is not None:
                self.accountant.release(event.batch)
            for out in outputs:
                self._enqueue(state, 0, out)
            self.accountant.end_call(label, executor, stats, modelled)
            self._record(state, lane, call, len(outputs))
        if all(state.lane_done_sent):
            state.out_done[0] = True
            self.accountant.forget(str(state.info))

    def _run_forwarder(self, state: NodeState) -> None:
        forwarder = state.info.executors.forwarder
        for lane in range(state.info.n_lanes):
            if state.out_done[lane]:
                continue
            sources = forwarder.sources_of(lane)
            forwarded = self._forward_one(state, lane, sources)
            if forwarded is not None:
                self._record(state, lane, "forward", 1)
            elif len(state.retired[lane]) == len(sources):
                state.out_done[lane] = True
                self._record(state, lane, "forward/done", 0)

    def _forward_one(self, state: NodeState, lane: int, sources) -> Batch | None:
        """One batch per visit, cycling `sources_of` in order from the lane's cursor."""
        n = len(sources)
        start = state.cursors[lane]
        for offset in range(n):
            index = (start + offset) % n
            if index in state.retired[lane]:
                continue
            child_index, child_lane = sources[index]
            child = self.states[state.info.children[child_index]]
            if child.out_queues[child_lane]:
                # A move between queues: the batch stays in flight, so no accounting.
                batch = child.out_queues[child_lane].popleft()
                self.consumed[state.info.id][child_index][child_lane] += batch.num_rows()
                self._queue(state, lane, batch)
                state.cursors[lane] = (index + 1) % n
                return batch
            if child.out_done[child_lane]:
                state.retired[lane].add(index)
        return None

    # -- wiring ------------------------------------------------------------------

    def _input_lane_count(self, info: PlanNodeInfo) -> int:
        if info.category is not ExecutorCategory.PARTITION_ACCUMULATOR:
            return 0
        return self.plan.child(info.id, 0).n_lanes

    def _lane_driver(self, state: NodeState, lane: int):
        driver = state.lane_drivers.get(lane)
        if driver is None:
            backends = state.info.executors.backends
            driver = single_partition_driver(
                state.info,
                lane,
                lambda: self.selector.select(state.info.category, backends, lane),
                self.accountant,
            )
            state.lane_drivers[lane] = driver
        return driver

    def _cross_executor(self, state: NodeState):
        if state.cross_executor is None:
            state.cross_executor = self.selector.select(
                state.info.category, state.info.executors.backends, None
            )
        return state.cross_executor

    def _lane_inputs(self, state: NodeState, lane: int) -> LaneInputs:
        sources = [(self.states[child], lane) for child in state.info.children]
        return LaneInputs(sources, self.consumed[state.info.id])

    def _accumulator_lane_ready(self, state: NodeState, lane: int) -> bool:
        child = self.states[state.info.children[0]]
        if child.out_queues[lane]:
            return True
        return child.out_done[lane] and not state.lane_done_sent[lane]

    def _forwarder_lane_ready(self, state: NodeState, lane: int) -> bool:
        if state.out_done[lane]:
            return False
        sources = state.info.executors.forwarder.sources_of(lane)
        live = 0
        for index, (child_index, child_lane) in enumerate(sources):
            if index in state.retired[lane]:
                continue
            child = self.states[state.info.children[child_index]]
            if child.out_queues[child_lane]:
                return True
            if not child.out_done[child_lane]:
                live += 1
        return live == 0

    def _enqueue(self, state: NodeState, lane: int, batch: Batch) -> None:
        self._queue(state, lane, batch)
        self.accountant.hold(batch)

    def _queue(self, state: NodeState, lane: int, batch: Batch) -> None:
        state.out_queues[lane].append(batch)
        self.emitted[state.info.id][lane].append((batch.num_rows(), batch.byte_size()))
        if state.info.id != self.plan.root:
            self._calls_owed += 1

    def _drain_root(self) -> None:
        root = self.states[self.plan.root]
        for lane, queue in enumerate(root.out_queues):
            while queue:
                batch = queue.popleft()
                self.accountant.release(batch)
                self.results.append(batch)
                self.root_lanes[lane].append(batch)

    def _drop_in_flight(self) -> None:
        """Release every batch still queued. Nothing will consume them now."""
        for state in self.states:
            for queue in state.out_queues:
                while queue:
                    self.accountant.release(queue.popleft())

    def _record(self, state: NodeState, lane: int, call: str, n_out: int) -> None:
        self.trace.append(TraceEvent(self.steps, str(state.info), lane, call, n_out))
        if call != "prefetch":
            self.calls += 1

    def _assert_drained(self) -> None:
        stranded = [
            f"{s.info} lane {lane} holds {len(q)}"
            for s in self.states
            for lane, q in enumerate(s.out_queues)
            if q
        ]
        if stranded:
            raise DriverError("nothing runnable but batches remain: " + "; ".join(stranded))


def partitioned_driver(
    plan: Plan, selector: BackendSelector, budget: int | None = None, prefetch: bool = False
) -> PartitionedDriver:
    """Constructor spelled as the driver name the spec uses."""
    return PartitionedDriver(plan, selector, budget, prefetch)
