"""The optimizer end to end over one engine plan: dynamic filters, then every cluster reordered
by DPhyp and oriented with the probes' builds known, then the adaptive run, replanning where a
build misses its estimate. Each step is the one its corpus suite runs; this is their order, the
one `replan.run_adaptive`'s `kept` is shaped for. What each rule did comes back as an
`OptimizerReport`.
"""

from __future__ import annotations

from dataclasses import dataclass

from .dynamic_filters import Batching, apply, candidates
from .join_order import ordered
from .replan import AdaptiveRun, kept_estimates, run_adaptive
from .report import FilterCandidate, OptimizerReport
from .stats import Statistics
from ..engine.node import CpuBackendSelector
from ..engine.partitioned_driver import PartitionedDriver
from ..plans.engine_nodes import build
from ..plans.engine_plan import EngineNode, plan_text
from ..plans.engine_run import by_lane, run_plan


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


@dataclass
class OptimizedRun:
    adaptive: AdaptiveRun
    #: each dynamic filter's probe plan — a build side — and the run that made its build
    probes: list[tuple[EngineNode, PartitionedDriver]]
    report: OptimizerReport


def run_optimized(plan: EngineNode, stats: Statistics, tables, shape: ModeShape,
                  budget: int | None = None) -> OptimizedRun:
    """`plan` optimized and run on `tables`, at a mode of `shape`."""
    lanes = shape.lanes
    probes = []

    def probe(side: EngineNode):
        driver = run_plan(build(side, tables), budget)
        probes.append((side, driver))
        return by_lane(driver)

    found = [FilterCandidate(c.build_key.name, c.scan.fields["table"], c.column, c.clustering)
             for c in candidates(plan, tables)]
    pruned, scans, kept = apply(plan, tables, shape.batching, probe)
    optimized, orders = ordered(pruned, stats, lanes, known=kept_estimates(kept, stats))
    run = run_adaptive(optimized, stats, tables, lanes, CpuBackendSelector(), budget, kept=kept)
    report = OptimizerReport(
        tuple(found), tuple(scans),
        tuple((name, sum(len(frame) for frame in by_lane)) for name, (_, by_lane) in kept.items()),
        tuple(orders), tuple(run.replans), tuple(run.refused), plan_text(plan), plan_text(run.plans[-1]))
    return OptimizedRun(run, probes, report)
