"""What the optimizer did to one plan, as data: every rule that changed it, and the plan before
and after. `report_text` renders it as a `.optimizer.txt` section; `pipeline.run_optimized` makes
it. Relations are named by their index in their cluster, and a set of them by its bit mask, as
DPhyp names them.
"""

from __future__ import annotations

from dataclasses import dataclass

from .dynamic_filters import Pruned

#: a relation's index, or a pair of trees — `(build, probe)` once oriented
JoinTree = int | tuple


@dataclass(frozen=True)
class FilterCandidate:
    """A join whose build key `build_key` can prune `table`'s scan on `column`."""

    build_key: str
    table: str
    column: str
    clustering: float


@dataclass(frozen=True)
class FilterPass:
    """One pass of dynamic filters: the candidates it found, the scans its probes reached, the
    builds they made, read from memory by the main plan — name, rows, bytes and the clusters it
    holds — and the replans and refused replans inside each probe plan, by its build's name."""

    candidates: tuple[FilterCandidate, ...] = ()
    pruned: tuple[Pruned, ...] = ()
    probed: tuple[tuple[str, int, int, tuple[str, ...]], ...] = ()
    replans: tuple[tuple[str, Replan], ...] = ()
    refused: tuple[tuple[str, BuildMiss], ...] = ()


@dataclass(frozen=True)
class PricedSet:
    """A set of relations DPhyp asked the cost of, and what the estimates answered."""

    relations: int
    rows: float
    cost: float


@dataclass(frozen=True)
class ChosenJoin:
    """A join of the oriented tree: its build and probe relations, and the estimates of the set
    it makes."""

    build: int
    probe: int
    rows: float
    cost: float


@dataclass(frozen=True)
class JoinOrder:
    """One DPhyp call over the cluster `mj` names. `edges` are its input — (left mask,
    right mask, keys) — and `priced` every set it asked, in the order asked, within `max_pairs`.
    `tree` is the order it returned, or the plan's own where it ran out of pairs (`unsolved`);
    `oriented` is that tree with each join's build first, `joins` its joins bottom up, and
    `plan_cost` the plan's own order's C_out. `flipped` is the (build, probe) of each join whose
    build is the other side from the plan's join over the same two sides."""

    mj: str
    relations: tuple[str, ...]
    edges: tuple[tuple[int, int, str], ...]
    priced: tuple[PricedSet, ...]
    max_pairs: int
    tree: JoinTree
    unsolved: str | None
    oriented: JoinTree
    joins: tuple[ChosenJoin, ...]
    plan_cost: float
    flipped: tuple[tuple[int, int], ...]


@dataclass(frozen=True)
class BuildMiss:
    """A build that came out further from its estimate than the replan's threshold; `join` is
    its join's kind and keys."""

    join: str
    rows: int
    estimate: float


@dataclass(frozen=True)
class Replan:
    """A run stopped at `miss`: the builds `kept` as memory sources (name, rows), and the DPhyp
    calls of the plan optimized again."""

    miss: BuildMiss
    kept: tuple[tuple[str, int], ...]
    orders: tuple[JoinOrder, ...]


@dataclass(frozen=True)
class OptimizerReport:
    """The rules in the order they act: dynamic filters on the plan as written, the DPhyp calls
    before the run, dynamic filters on the plan they ordered, the replans; `refused` the misses a
    replan could not take, the run going on."""

    filters_before_order: FilterPass
    orders: tuple[JoinOrder, ...]
    filters_after_order: FilterPass
    replans: tuple[Replan, ...]
    refused: tuple[BuildMiss, ...]
    before: str
    after: str


@dataclass(frozen=True)
class Fired:
    """How often each rule acted on one run: probe plans run and scans they narrowed, before and
    after join order; DPhyp calls — before the run and in replans — those whose tree's C_out is
    below the plan order's, and those stopped at their budget; joins flipped, replans and replans
    refused, the main run's and the probe plans'."""

    probes: int = 0
    narrowed: int = 0
    probes_after_order: int = 0
    narrowed_after_order: int = 0
    dphyp: int = 0
    reordered: int = 0
    budget: int = 0
    flips: int = 0
    replans: int = 0
    refused: int = 0


def fired(report: OptimizerReport) -> Fired:
    before, after = report.filters_before_order, report.filters_after_order
    replans = report.replans + tuple(replan for _, replan in before.replans + after.replans)
    orders = report.orders + tuple(order for replan in replans for order in replan.orders)
    return Fired(probes=len(before.probed), narrowed=_narrowed(before),
                 probes_after_order=len(after.probed), narrowed_after_order=_narrowed(after),
                 dphyp=len(orders), reordered=sum(sum(j.cost for j in o.joins) < o.plan_cost for o in orders),
                 budget=sum(o.unsolved is not None for o in orders), flips=sum(len(o.flipped) for o in orders),
                 replans=len(replans),
                 refused=len(report.refused) + len(before.refused) + len(after.refused))


def _narrowed(filters: FilterPass) -> int:
    return sum(len(p.after) < len(p.before) for p in filters.pruned)
