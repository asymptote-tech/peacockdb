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
    """One DPhyp call over a cluster. `edges` are its input — (left mask, right mask, keys) — and
    `priced` every set it asked, in the order asked, within `max_pairs`. `tree` is the order it
    returned, or the plan's own where it ran out of pairs (`unsolved`); `oriented` is that
    tree with each join's build first, `joins` its joins bottom up, and `plan_cost` the plan's own
    order's C_out. `flipped` is the (build, probe) of each join whose build is the other side
    from the plan's join over the same two sides."""

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
    """`probed` are the builds the dynamic filters' probe plans made, read from memory by the
    main plan (name, rows); `orders` the DPhyp calls before the run; `refused` the misses a
    replan could not take, the run going on."""

    candidates: tuple[FilterCandidate, ...]
    pruned: tuple[Pruned, ...]
    probed: tuple[tuple[str, int], ...]
    orders: tuple[JoinOrder, ...]
    replans: tuple[Replan, ...]
    refused: tuple[BuildMiss, ...]
    before: str
    after: str


@dataclass(frozen=True)
class Fired:
    """How often each rule acted on one run: probe plans run and scans they narrowed; DPhyp calls —
    before the run and in replans — those whose tree's C_out is below the plan order's, and those
    stopped at their budget; joins flipped, replans and replans refused."""

    probes: int = 0
    narrowed: int = 0
    dphyp: int = 0
    reordered: int = 0
    budget: int = 0
    flips: int = 0
    replans: int = 0
    refused: int = 0


def fired(report: OptimizerReport) -> Fired:
    orders = report.orders + tuple(order for replan in report.replans for order in replan.orders)
    return Fired(probes=len(report.probed), narrowed=sum(len(p.after) < len(p.before) for p in report.pruned),
                 dphyp=len(orders), reordered=sum(sum(j.cost for j in o.joins) < o.plan_cost for o in orders),
                 budget=sum(o.unsolved is not None for o in orders), flips=sum(len(o.flipped) for o in orders),
                 replans=len(report.replans), refused=len(report.refused))
