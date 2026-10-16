"""What the optimizer did to one plan, as data: every rule that changed it, and the plan before
and after. `report_text` renders it as a `.optimizer.txt` section; `pipeline.run_optimized` makes
it. Relations are named by their index in their cluster, and a set of them by its bit mask, as
DPhyp names them.
"""

from __future__ import annotations

from dataclasses import dataclass

from ..operators.join_types import JoinType
from .cardinality import Certainty
from .dynamic_filters import Pruned
from .fired import Fired

#: a relation's index, or a pair of trees — `(build, probe)` once oriented
JoinTree = int | tuple


def mask(tree: JoinTree) -> int:
    """The relations a tree joins, as a bit mask."""
    return 1 << tree if isinstance(tree, int) else mask(tree[0]) | mask(tree[1])


def unordered(tree: JoinTree):
    """A tree with its sides unordered: two orders equal so differ only in which side builds."""
    return tree if isinstance(tree, int) else frozenset((unordered(tree[0]), unordered(tree[1])))


def subtree(tree: JoinTree, relations: int) -> JoinTree:
    """The part of `tree` that joins exactly `relations`, one of its joins' sides."""
    while mask(tree) != relations:
        tree = next(side for side in tree if mask(side) & relations == relations)
    return tree


@dataclass(frozen=True)
class FilterCandidate:
    """A join whose build key `build_key` can prune `table`'s scan on `column`."""

    build_key: str
    table: str
    column: str
    clustering: float


@dataclass(frozen=True)
class Probed:
    """A build a probe plan made, which the main plan reads from memory: its name, rows, bytes, the
    clusters it holds, what it is (`names.described`) and the memory sources that names."""

    name: str
    rows: int
    size: int
    clusters: tuple[str, ...]
    holds: str
    sources: tuple[str, ...] = ()


@dataclass(frozen=True)
class FilterPass:
    """One pass of dynamic filters: the candidates it found, the scans its probes reached, the
    builds they made, and the replans and refused replans inside each probe plan, by its build's
    name."""

    candidates: tuple[FilterCandidate, ...] = ()
    pruned: tuple[Pruned, ...] = ()
    probed: tuple[Probed, ...] = ()
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
class RelationEstimate:
    """A relation of a DPhyp call as the estimates see it: its name (`names.relation_name`), its
    rows and how they are known, its bytes per row, the NDV of each column a key edge joins it on,
    by name, and the memory sources its name shows."""

    name: str
    rows: float
    known: Certainty
    width: float
    ndv: tuple[tuple[str, float], ...]
    sources: tuple[str, ...] = ()


@dataclass(frozen=True)
class EdgeColumn:
    """A column an edge reads: its relation's index, its name, and whether it is an aggregate's
    output rather than a column with statistics."""

    relation: int
    name: str
    aggregate: bool = False


@dataclass(frozen=True)
class EdgeReport:
    """A predicate between the relations in `left` and those in `right`: its key pairs, the
    columns of its residual, and its join's logical type, the side it keeps on the left. Only an
    edge with keys is DPhyp's."""

    left: int
    right: int
    keys: tuple[tuple[EdgeColumn, EdgeColumn], ...] = ()
    residual: tuple[EdgeColumn, ...] = ()
    kind: JoinType = JoinType.INNER


@dataclass(frozen=True)
class JoinOrder:
    """One DPhyp call over the cluster `mj` names. `edges` are the cluster's predicates, those
    with keys DPhyp's input, and `priced` every set it asked, in the order asked, within
    `max_pairs`. `tree` is the order it returned, or the plan's own where it ran out of pairs
    (`unsolved`); `planned` is the plan's own order and `oriented` the tree with each join's build
    first, `joins` its joins bottom up, and `plan_cost` the plan's order's C_out. `flipped` is the
    (build, probe) of each join whose build is the other side from the plan's join over the same
    two sides; `types` the (build, probe, type) of each join of `planned` and `oriented` that is
    not inner, its build on the left as the engine names it."""

    mj: str
    relations: tuple[RelationEstimate, ...]
    edges: tuple[EdgeReport, ...]
    priced: tuple[PricedSet, ...]
    max_pairs: int
    tree: JoinTree
    unsolved: str | None
    planned: JoinTree
    oriented: JoinTree
    joins: tuple[ChosenJoin, ...]
    plan_cost: float
    flipped: tuple[tuple[int, int], ...]
    types: tuple[tuple[int, int, JoinType], ...] = ()

    def join_type(self, build: int, probe: int) -> JoinType:
        return next((t for b, p, t in self.types if (b, p) == (build, probe)), JoinType.INNER)

    @property
    def changed(self) -> bool:
        """Whether the call left the cluster other than as planned: reordered, or a join flipped."""
        return self.oriented != self.planned


@dataclass(frozen=True)
class BuildMiss:
    """A build that came out further from its estimate than the replan's threshold; `join` is
    its join's kind and keys in words, `mj` its cluster's id where it is in one."""

    join: str
    rows: int
    estimate: float
    mj: str | None = None

    @property
    def q_error(self) -> float:
        rows, estimate = max(self.rows, 1.0), max(self.estimate, 1.0)
        return max(rows / estimate, estimate / rows)


@dataclass(frozen=True)
class Kept:
    """A build a stopped run had made, kept as a memory source: its name, rows, what it is
    (`names.described`) and the memory sources that names."""

    name: str
    rows: int
    holds: str
    sources: tuple[str, ...] = ()


@dataclass(frozen=True)
class Replan:
    """A run stopped at `miss`: the builds `kept` as memory sources, and the DPhyp calls of the
    plan optimized again."""

    miss: BuildMiss
    kept: tuple[Kept, ...]
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
