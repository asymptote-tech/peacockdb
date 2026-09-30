"""The adaptive loop: run a plan; where a build comes out further than `threshold`
times off its estimate, stop, turn every build the run has made into a `GpuMemorySource`, optimize
the plan again with those sizes known, and run it with a new driver. Nothing but the builds had
started, so nothing runs twice; a replan that would repeat work is refused, and the run goes on.

Each replan turns at least one build into a source measured exactly, whose own `BuildDone` then
matches its estimate, so the loop ends within as many replans as the plan has joins.
"""

from __future__ import annotations

from dataclasses import dataclass, field

import pandas as pd

from .adaptive import AdaptiveDriver, BuildDone, Extraction, hashed_names
from .cardinality import Estimate, Estimator, estimator, measured
from .engine_nodes import build
from .engine_plan import EngineNode, memory_source
from .join_order import optimize
from .multijoin import replaced
from .node import BackendSelector
from .single_partition_driver import BUILD_SLOT
from .stats import Statistics

DEFAULT_THRESHOLD = 2.0


class WithMaterialized:
    """`tables` and the frames by lane of the memory sources a replanned plan reads, by name."""

    def __init__(self, tables, frames: dict[str, list[pd.DataFrame]]):
        self._tables, self._frames = tables, frames

    def __getattr__(self, name):
        return getattr(self._tables, name)

    def materialized(self, name: str) -> list[pd.DataFrame]:
        return self._frames[name]


@dataclass
class AdaptiveRun:
    results: list = field(default_factory=list)
    #: one per plan run, the last one run to its end
    drivers: list[AdaptiveDriver] = field(default_factory=list)
    plans: list[EngineNode] = field(default_factory=list)
    #: builds off their estimate where something outside the builds had started
    refused: list[BuildDone] = field(default_factory=list)
    #: the memory sources' measured estimates, by name
    known: dict[str, Estimate] = field(default_factory=dict)


def run_adaptive(plan: EngineNode, stats: Statistics, tables, lanes: int, selector: BackendSelector,
                 budget: int | None = None, threshold: float = DEFAULT_THRESHOLD,
                 prefetch: bool = False, kept: dict | None = None) -> AdaptiveRun:
    """`plan` run to its answer, replanned where a build is off by more than `threshold`; `lanes`
    is the mode's, for the joins a replan makes anew. `kept` are the memory sources `plan`
    already reads — the builds dynamic filters' probe plans made — by name: (side, frames)."""
    run, frames = AdaptiveRun(), {name: lanes for name, (_, lanes) in (kept or {}).items()}
    known = run.known
    known.update(kept_estimates(kept or {}, stats))
    while True:
        planned = estimator(plan, stats, known)

        def stops(event) -> bool:
            if not isinstance(event, BuildDone):
                return False
            guess = planned.estimates[id(event.join.children[BUILD_SLOT])].rows
            if _q_error(event.rows, guess) <= threshold:
                return False
            if driver.extraction() is None:
                run.refused.append(event)
                return False
            return True

        driver = AdaptiveDriver(plan, build(plan, WithMaterialized(tables, frames)), selector, budget, stops,
                                prefetch=prefetch)
        run.plans.append(plan)
        run.drivers.append(driver)
        run.results = driver.run()
        if driver.stopped is None:
            return run
        plan = _with_memory_sources(plan, driver.extraction(), planned, known, frames)
        plan = optimize(plan, stats, lanes, known=known)


def kept_estimates(kept: dict, stats: Statistics) -> dict[str, Estimate]:
    """The measured estimates of builds kept as memory sources, by name — `kept` as
    `dynamic_filters.apply` returns it: (the side they replace, their frames by lane)."""
    return {name: measured(estimator(side, stats).estimates[id(side)], lanes, hashed_names(side))
            for name, (side, lanes) in kept.items()}


def _with_memory_sources(plan: EngineNode, extraction: Extraction, planned: Estimator,
                         known: dict[str, Estimate], frames: dict) -> EngineNode:
    """`plan` with each extracted build's side a memory source over its frames, laid out as the
    side was; `known` and `frames` gain them by name. A side that is one already stays."""
    for join, lanes in extraction.builds.values():
        side = join.children[BUILD_SLOT]
        if side.kind == "GpuMemorySource":
            continue
        name = f"m{len(frames)}"
        known[name] = measured(planned.estimates[id(side)], lanes, hashed_names(side))
        frames[name] = lanes
        plan = replaced(plan, side, memory_source(name, side))
    return plan


def _q_error(true: float, guess: float) -> float:
    true, guess = max(true, 1.0), max(guess, 1.0)
    return max(true / guess, guess / true)
