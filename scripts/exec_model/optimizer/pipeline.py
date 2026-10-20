"""The optimizer end to end over one engine plan: its clusters named, dynamic filters on the plan
as written, every cluster reordered by DPhyp and oriented with the probes' builds known, dynamic
filters again on that order, then the adaptive run, replanning where a build misses its estimate.
`filtered` is everything before the run, which the dynamic filters' corpus suite runs too; the
builds it keeps are what `replan.run_adaptive`'s `kept` is shaped for. What each rule did comes
back as an `OptimizerReport`.
"""

from __future__ import annotations

from dataclasses import dataclass

from .dynamic_filters import Batching, Builds, apply, candidates
from .join_order import ordered
from .multijoin import named
from .names import aliased, described, described_sources, dropped_by_source
from .replan import AdaptiveRun, kept_estimates, run_adaptive
from .report import FilterCandidate, FilterPass, JoinOrder, OptimizerReport, Probed
from .stats import Statistics
from ..engine.node import CpuBackendSelector
from ..engine.partitioned_driver import PartitionedDriver
from ..operators.frame import PandasBatch
from ..plans.engine_plan import EngineNode, plan_text
from ..plans.engine_run import by_lane


@dataclass(frozen=True)
class ModeShape:
    """What the optimizer needs of a mode: the lanes a join it makes runs on, and how a pruned
    scan's row groups are batched."""

    lanes: int
    batching: Batching


#: The engine's five modes, by the name their goldens carry.
MODES = {
    "tp1-single": ModeShape(1, Batching.ONE_PER_LANE),
    "tp1-rowgroup": ModeShape(1, Batching.ONE_PER_ROW_GROUP),
    "tp4-single": ModeShape(4, Batching.ONE_PER_LANE),
    "tp4-rowgroup": ModeShape(4, Batching.ONE_PER_ROW_GROUP),
    "tp4-sized": ModeShape(4, Batching.KEEP_BATCHES),
}


def mode_shape(mode: str) -> ModeShape:
    if mode not in MODES:
        raise ValueError(f"no mode {mode!r}: the modes are {', '.join(MODES)}")
    return MODES[mode]


@dataclass(frozen=True)
class Filtered:
    """The plan `run_adaptive` is given: its builds `kept` by memory source name, (side, frames by
    lane), both passes' in the order made; the DPhyp calls between the passes; each probe plan's
    adaptive run, in the order run."""

    plan: EngineNode
    before_order: FilterPass
    orders: tuple[JoinOrder, ...]
    after_order: FilterPass
    kept: dict
    probes: tuple[AdaptiveRun, ...]


def filtered(plan: EngineNode, stats: Statistics, tables, shape: ModeShape, budget: int | None = None,
             taken: set[str] | None = None) -> Filtered:
    """`plan` with its tables read twice numbered for the report and its clusters named, pruned by
    the builds a filter and no join make, ordered with those builds known, then pruned by the
    builds a filter or a join make on that order. Each probe plan runs adaptively, so a build that
    misses inside it replans as in the main run; its memory sources' names join `taken`. Nothing is
    ordered again after the second pass: a source keeps its side's schema, lanes and hash, so the
    plan stays valid, and a build of the main run off its estimate still replans. `design.md`, "Two
    passes of dynamic filters"."""
    taken = set() if taken is None else taken
    runs: dict[int, AdaptiveRun] = {}

    def probe(side: EngineNode, builds: dict) -> list:
        run = run_adaptive(side, stats, tables, shape.lanes, CpuBackendSelector(), budget, kept=builds, taken=taken)
        runs[id(side)] = run
        return by_lane(run.drivers[-1])

    first, before_order, kept = _pass(named(aliased(plan)), tables, shape.batching, probe, Builds.FILTERED, {}, runs)
    reordered, orders = ordered(first, stats, shape.lanes, known=kept_estimates(kept, stats))
    second, after_order, more = _pass(reordered, tables, shape.batching, probe, Builds.FILTERED_OR_JOINED, kept,
                                      runs)
    return Filtered(second, before_order, tuple(orders), after_order, kept | more, tuple(runs.values()))


def _pass(plan, tables, batching, probe, builds, kept, runs) -> tuple[EngineNode, FilterPass, dict]:
    found = [FilterCandidate(c.build_key.name, c.scan_name, c.column, c.clustering)
             for c in candidates(plan, tables, builds)]
    pruned, scans, made = apply(plan, tables, batching, probe, builds, kept)
    dropped = dropped_by_source(pruned, *(side for side, _ in made.values()))
    probed = tuple(Probed(name, sum(len(frame) for frame in lanes),
                          sum(PandasBatch(frame).byte_size() for frame in lanes), _cluster_ids(side),
                          described(side, dropped.get(name, frozenset())), described_sources(side))
                   for name, (side, lanes) in made.items())
    replans = tuple((name, replan) for name, (side, _) in made.items() for replan in runs[id(side)].replans)
    refused = tuple((name, miss) for name, (side, _) in made.items() for miss in runs[id(side)].refused)
    return pruned, FilterPass(tuple(found), tuple(scans), probed, replans, refused), made


def _cluster_ids(node: EngineNode) -> tuple[str, ...]:
    """The ids of the clusters whose joins `node` holds, outermost first."""
    found, stack = [], [node]
    while stack:
        node = stack.pop()
        found += [node.mj] if node.mj is not None else []
        stack.extend(reversed(node.children))
    return tuple(dict.fromkeys(found))


@dataclass
class OptimizedRun:
    adaptive: AdaptiveRun
    #: each probe plan's last run, the one that made its build, and the plan it ran
    probes: list[tuple[EngineNode, PartitionedDriver]]
    #: each run a replan stopped, in a probe or in the main run, and the plan it ran
    stopped: list[tuple[EngineNode, PartitionedDriver]]
    report: OptimizerReport


def run_optimized(plan: EngineNode, stats: Statistics, tables, shape: ModeShape,
                  budget: int | None = None) -> OptimizedRun:
    """`plan` optimized and run on `tables`, at a mode of `shape`."""
    taken = set()
    stages = filtered(plan, stats, tables, shape, budget, taken)
    run = run_adaptive(stages.plan, stats, tables, shape.lanes, CpuBackendSelector(), budget, kept=stages.kept,
                       taken=taken)
    report = OptimizerReport(stages.before_order, stages.orders, stages.after_order, tuple(run.replans),
                             tuple(run.refused), plan_text(plan), plan_text(run.plans[-1]))
    probes = [(probe.plans[-1], probe.drivers[-1]) for probe in stages.probes]
    stopped = [pair for every in (*stages.probes, run) for pair in zip(every.plans[:-1], every.drivers[:-1])]
    return OptimizedRun(run, probes, stopped, report)
