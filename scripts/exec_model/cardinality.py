"""Row counts for every node of an engine plan, estimated bottom-up from base-table statistics.

Each output column carries its lineage — the base column it is, unchanged — and an NDV no larger
than its node's rows: a filter leaves a column it pins the values it can still hold and thins the
rest (Cardenas), a join leaves its keys the values both sides hold, an aggregate's keys start a
new relation. An equi-join is |build|·|probe| over the largest of each side's key NDV
(containment) and the domain the two can share — DuckDB's total domain cut to where the key
ranges overlap (`Estimator._domain`). A semi join keeps the share of its side's values the other
holds, an anti join the rest; an outer join adds its unmatched rows, NULL on the other side.
A `GpuMemorySource` is measured, not estimated: its estimate is given in `known`, by its name.
"""

from __future__ import annotations

import json
import math
from dataclasses import dataclass, replace

import pandas as pd

from . import engine_ir
from .engine_expr import ColumnRef, parse_columns, parse_exprs, parse_grouping_sets, parse_join_keys, parse_named_exprs
from .engine_plan import EngineNode
from .dynamic_filters import as_number
from .errors import StatsError
from .estimator import restricted_ndv, selectivity
from .observed import observed_ndv
from .operators.join_types import JoinType
from .operators.joins import own_output
from .stats import ColumnStats, Statistics

_PASS = frozenset({"GpuCoalesceAllBatches", "GpuEmitPartitions", "GpuMergePartitions", "GpuSort",
                   "GpuAccumulateBatchesAndSort", "GpuMergeSortedPartitions"})
_KEEPS_BUILD = {JoinType.LEFT_SEMI: "semi", JoinType.LEFT_ANTI: "anti", JoinType.LEFT_MARK: "mark"}
_KEEPS_PROBE = {JoinType.RIGHT_SEMI: "semi", JoinType.RIGHT_ANTI: "anti"}


@dataclass(frozen=True)
class Column:
    """One output column: the base `(table, column)` it is, where it is one, its NDV here, and
    the base footer's statistics a filter above can read."""

    base: tuple[str, str] | None
    ndv: float
    stats: ColumnStats | None = None
    null_share: float = 0.0


@dataclass(frozen=True)
class Estimate:
    rows: float
    columns: tuple[Column, ...]


def estimate(plan: EngineNode, stats: Statistics,
             known: dict[str, Estimate] | None = None) -> dict[int, Estimate]:
    """Every node's estimate, by `id(node)`."""
    return estimator(plan, stats, known).estimates


def estimator(plan: EngineNode, stats: Statistics,
              known: dict[str, Estimate] | None = None) -> Estimator:
    """The estimator of `plan`, its nodes estimated. Two passes: the first finds the equi-join
    edges between base columns, so that the second counts two keys of one class once."""
    first = Estimator(stats, None, known or {})
    first.node(plan)
    second = Estimator(stats, _Classes(first.edges), known or {})
    second.node(plan)
    return second


def measured(estimated: Estimate, lanes: list[pd.DataFrame], hashed_on: list[str] | None) -> Estimate:
    """A materialization's estimate: `estimated`'s lineage with the true rows and NULL shares, and
    each column's NDV clamped into what the lanes' counts bound it to."""
    rows = sum(len(lane) for lane in lanes)
    columns = []
    for name, column in zip(lanes[0].columns, estimated.columns):
        bounds = observed_ndv(lanes, [name], hashed_on)
        nulls = sum(int(lane[name].isna().sum()) for lane in lanes)
        columns.append(replace(column, ndv=float(min(max(column.ndv, bounds.low), bounds.high)),
                               null_share=nulls / rows if rows else 0.0))
    return Estimate(float(rows), tuple(columns))


@dataclass(frozen=True)
class EquiJoin:
    """An equi-join's inner rows, the share of each side's rows that find a match, and both
    sides with each key's NDV cut to the values they share."""

    rows: float
    build_matched: float
    probe_matched: float
    build: Estimate
    probe: Estimate


class _Classes:
    """Union-find over base columns an equi-join makes equal."""

    def __init__(self, edges):
        self.parent = {}
        for a, b in edges:
            self.parent[self.find(a)] = self.find(b)

    def find(self, column):
        self.parent.setdefault(column, column)
        while self.parent[column] != column:
            self.parent[column] = self.parent[self.parent[column]]
            column = self.parent[column]
        return column


class Estimator:
    def __init__(self, stats: Statistics, classes: _Classes | None, known: dict[str, Estimate]):
        self.stats, self.classes, self.known = stats, classes, known
        self.edges, self.estimates = [], {}

    def node(self, node: EngineNode) -> Estimate:
        children = [self.node(child) for child in node.children]
        if node.kind == "GpuLoadParquet":
            found = self._scan(node)
        elif node.kind == "GpuMemorySource":
            found = self.known[node.fields["name"]]
        elif node.kind == "GpuFilter":
            found = self._filter(node, children[0])
        elif node.kind == "GpuProject":
            columns = [children[0].columns[e.index] if isinstance(e, ColumnRef) else None
                       for e, _ in parse_named_exprs(node.fields["exprs"])]
            found = shaped(children[0].rows, columns)
        elif node.kind in _PASS:
            found = children[0]
        elif node.kind in ("GpuLimit", "GpuUnload"):
            fetch = node.fields.get("fetch")
            rows = max(0.0, children[0].rows - int(node.fields.get("skip", 0)))
            found = shaped(rows if fetch is None else min(rows, int(fetch)), children[0].columns)
        elif node.kind in ("GpuUnion", "GpuInterleave"):
            found = _union(children)
        elif node.kind == "GpuHashJoin":
            found = self._hash_join(node, *children)
        elif node.kind in ("GpuNestedLoopJoin", "GpuCrossJoin"):
            found = self._loop_join(node, *children)
        elif node.kind in ("GpuAggregate", "GpuAggregateBatches"):
            found = _aggregate(node, children[0])
        else:
            raise ValueError(f"no estimate for {node.kind}")
        self.estimates[id(node)] = found
        return found

    def _scan(self, node) -> Estimate:
        table = node.fields["table"]
        groups = sorted({g for lane in json.loads(node.fields["partition_groups"]) for b in lane for g in b})
        rows = float(self.stats.rows(table, groups))
        if "limit" in node.fields:
            rows = min(rows, float(node.fields["limit"]))
        columns = []
        for ref in parse_columns(node.fields["projections"]):
            try:
                found = self.stats.column(table, ref.name, groups)
            except StatsError:  # a vector column: nothing a filter or key can use
                columns.append(Column(None, rows))
                continue
            share = found.nulls / rows if found.nulls is not None and rows else 0.0
            columns.append(Column((table, ref.name), min(found.ndv, rows), found, share))
        return Estimate(rows, tuple(columns))

    def _filter(self, node, child: Estimate) -> Estimate:
        predicate = engine_ir.lower_field(node, "predicate")
        frames = [c.frame for c in engine_ir.columns(node.children[0].schema)]
        lookup = _lookup(frames, child.columns, child.rows)
        rows = child.rows * selectivity(predicate, lookup, child.rows)
        pinned = restricted_ndv(predicate, lookup, child.rows)
        columns = [replace(c, ndv=pinned[f]) if f in pinned else _thinned(c, child.rows, rows)
                   for f, c in zip(frames, child.columns)]
        if "projection" in node.fields:
            columns = [columns[ref.index] for ref in parse_columns(node.fields["projection"])]
        return shaped(rows, columns)

    def _hash_join(self, node, build: Estimate, probe: Estimate) -> Estimate:
        pairs = [(b.index, p.index) for b, p in parse_join_keys(node.fields["on"])]
        for b, p in pairs:
            if build.columns[b].base and probe.columns[p].base:
                self.edges.append((build.columns[b].base, probe.columns[p].base))
        residual = self._joined_selectivity(node, build, probe) if "filter" in node.fields else 1.0
        joined = self.equi_join(build, probe, pairs, residual)
        return self._joined(node, joined.build, joined.probe, joined.rows, joined.build_matched,
                            joined.probe_matched)

    def equi_join(self, build: Estimate, probe: Estimate, pairs, residual: float = 1.0) -> EquiJoin:
        """`build` joined to `probe` on the `(build ordinal, probe ordinal)` key `pairs`, a
        residual keeping `residual` of the matches."""
        build_keys, probe_keys = [b for b, _ in pairs], [p for _, p in pairs]
        build_ndv, probe_ndv = self._key_ndv(build, build_keys), self._key_ndv(probe, probe_keys)
        domain = self._domain(build, probe, pairs)
        # Containment — the side with fewer key values finds each on the other — over a domain
        # the two share; a match survives the residual filter as often as the filter keeps a row.
        inner = build.rows * probe.rows / max(build_ndv, probe_ndv, domain, 1.0) * residual
        matched_build, matched_probe = _matched_keys(build, probe, pairs)
        return EquiJoin(inner, min(1.0, probe_ndv / max(build_ndv, 1.0)) * residual,
                        min(1.0, build_ndv / max(probe_ndv, 1.0)) * residual, matched_build, matched_probe)

    def _loop_join(self, node, build: Estimate, probe: Estimate) -> Estimate:
        share = 1.0
        if "filter" in node.fields:
            share = self._joined_selectivity(node, build, probe)
        # A row matches as many rows of the other side as the filter keeps of it.
        return self._joined(node, build, probe, build.rows * probe.rows * share,
                            min(1.0, probe.rows * share), min(1.0, build.rows * share))

    def _joined(self, node, build, probe, inner, build_matched, probe_matched) -> Estimate:
        """The join's rows by its type, from its inner rows and the share of each side's rows
        that find a match."""
        join_type = engine_ir.join_type(node) if "join_type" in node.fields else JoinType.INNER
        if join_type in _KEEPS_BUILD:
            kept, matched, how = build, build_matched, _KEEPS_BUILD[join_type]
        elif join_type in _KEEPS_PROBE:
            kept, matched, how = probe, probe_matched, _KEEPS_PROBE[join_type]
        if join_type in _KEEPS_BUILD or join_type in _KEEPS_PROBE:
            rows = {"semi": kept.rows * matched, "anti": kept.rows * (1 - matched), "mark": kept.rows}[how]
        else:
            rows = inner
            if join_type in (JoinType.LEFT, JoinType.FULL):
                rows += build.rows * (1 - build_matched)
            if join_type in (JoinType.RIGHT, JoinType.FULL):
                rows += probe.rows * (1 - probe_matched)
        # An outer join's unmatched rows carry NULL in the other side's columns.
        if join_type in (JoinType.LEFT, JoinType.FULL):
            probe = _padded(probe, build.rows * (1 - build_matched), rows)
        if join_type in (JoinType.RIGHT, JoinType.FULL):
            build = _padded(build, probe.rows * (1 - probe_matched), rows)
        own = own_output(join_type, len(build.columns), len(probe.columns))
        joined = build.columns + probe.columns
        columns = [Column(None, 2.0) if at is None else joined[at] for at in own]
        if "projection" in node.fields:
            columns = [columns[ref.index] for ref in parse_columns(node.fields["projection"])]
        return shaped(rows, columns)

    def _domain(self, build: Estimate, probe: Estimate, pairs) -> float:
        """The key values the two sides can share, over which each spreads its own.

        Per key, DuckDB's total domain — the larger base NDV — cut to where the two base ranges
        overlap: a date dimension's 73 049 keys span two centuries and a fact's 1 823 dates two
        per cent of them, so only the 1 827 inside the fact's range can match; a demographics
        table's 1.92 million keys and a fact's 226 thousand share one range, so a filtered
        subset of the first is spread over all of it. A key set that is one side's table key
        (PK–FK) is that table's keys, over which a filtered subset of it and the references meet
        independently; one that is neither's (fact to fact, many to many) is every combination, the
        product of its keys' domains. 0 where a key has no base."""
        whole = [self._key_ndv(build, [b for b, _ in pairs], whole=True),
                 self._key_ndv(probe, [p for _, p in pairs], whole=True)]
        if None in whole:
            return 0.0
        per_class = {}
        for b, p in pairs:
            root = self.classes.find(build.columns[b].base) if self.classes else build.columns[b].base
            per_class[root] = max(per_class.get(root, 0.0), self._overlapping_domain(build.columns[b].base,
                                                                                      probe.columns[p].base))
        if len(per_class) == 1:
            return next(iter(per_class.values()))
        if self._is_table_key(build, [b for b, _ in pairs], whole[0]) \
                or self._is_table_key(probe, [p for _, p in pairs], whole[1]):
            return max(whole)
        return math.prod(per_class.values())

    def _overlapping_domain(self, left: tuple[str, str], right: tuple[str, str]) -> float:
        """The larger of two base columns' NDVs, each thinned to the share of its own range the
        other's range covers — values spread evenly over [min, max]."""
        found = []
        for mine, theirs in ((left, right), (right, left)):
            own, other = self.stats.column(*mine), self.stats.column(*theirs)
            span = [as_number(v) for v in (own.low, own.high, other.low, other.high)]
            share = 1.0
            if None not in span and span[1] > span[0]:
                share = max(0.0, min(span[1], span[3]) - max(span[0], span[2])) / (span[1] - span[0])
            found.append(own.ndv * share)
        return max(found)

    def _is_table_key(self, side: Estimate, keys: list[int], ndv: float) -> bool:
        """Whether the keys are one table's and tell its rows apart — every row with no NULL
        among them, since a NULL key is no value and the NDV does not count it."""
        bases = [side.columns[at].base for at in keys]
        if len({table for table, _ in bases}) != 1:
            return False
        table = bases[0][0]
        nulls = sum(self.stats.column(table, name).nulls or 0 for _, name in bases)
        return ndv >= self.stats.rows(table) - nulls

    def _key_ndv(self, side: Estimate, keys: list[int], whole: bool = False) -> float | None:
        """The NDV of a side's join keys — two keys of one class counted once, keys of several
        classes as a set: of the values it holds here, or (`whole`) of its base tables' whole
        columns, None where a key has no base column."""
        bases = [side.columns[at].base for at in keys]
        if whole and None in bases:
            return None
        per_class = {}
        for at, base in zip(keys, bases):
            ndv = self.stats.column(*base).ndv if whole else side.columns[at].ndv
            root = ("computed", at) if base is None else self.classes.find(base) if self.classes else base
            per_class[root] = max(per_class.get(root, 0.0), ndv)
        if len(per_class) == 1:
            return float(next(iter(per_class.values())))
        cap = max(side.rows, 1.0) if not whole else math.inf
        if None not in bases and len({t for t, _ in bases}) == 1:
            counted = self.stats.composite_ndv(bases[0][0], sorted({c for _, c in bases}))
            if counted is not None:
                return min(float(counted), cap)
        return min(math.prod(per_class.values()), cap)

    def _joined_selectivity(self, node, build: Estimate, probe: Estimate) -> float:
        frames = [c.frame for c in engine_ir.columns(node.children[0].schema + node.children[1].schema)]
        rows = max(build.rows, probe.rows)
        return selectivity(engine_ir.lower_field(node, "filter"), _lookup(frames, build.columns + probe.columns, rows), rows)


def _lookup(frames, columns, rows):
    """The estimator's view of a node's input: each frame name's statistics, its NDV capped
    here and its NULLs scaled to these rows."""
    by_frame = dict(zip(frames, columns))

    def column(name):
        found = by_frame.get(name)
        if found is None or found.stats is None:
            return None
        s = found.stats
        return ColumnStats(int(max(found.ndv, 1)), found.null_share * rows, s.low, s.high)

    return column


def _matched_keys(build: Estimate, probe: Estimate, pairs) -> tuple[Estimate, Estimate]:
    """Both sides with each key's NDV cut to the smaller side's: only matched values go on."""
    build_columns, probe_columns = list(build.columns), list(probe.columns)
    for b, p in pairs:
        ndv = min(build_columns[b].ndv, probe_columns[p].ndv)
        build_columns[b] = replace(build_columns[b], ndv=ndv)
        probe_columns[p] = replace(probe_columns[p], ndv=ndv)
    return Estimate(build.rows, tuple(build_columns)), Estimate(probe.rows, tuple(probe_columns))


def _thinned(column: Column, rows_in: float, rows_out: float) -> Column:
    """A column's NDV once a filter keeps `rows_out` of `rows_in` rows that it does not pin:
    Cardenas — each of `ndv` values, on `rows_in / ndv` rows, survives unless all of them go."""
    if rows_in <= 0 or column.ndv <= 0:
        return column
    kept = 1 - (1 - min(1.0, rows_out / rows_in)) ** (rows_in / column.ndv)
    return replace(column, ndv=column.ndv * kept)


def _padded(side: Estimate, unmatched: float, rows: float) -> Estimate:
    """A side's columns as an outer join emits them: `unmatched` of `rows` rows are NULL."""
    if rows <= 0:
        return side
    columns = tuple(replace(c, null_share=min(1.0, (c.null_share * (rows - unmatched) + unmatched) / rows))
                    for c in side.columns)
    return Estimate(side.rows, columns)


def shaped(rows: float, columns) -> Estimate:
    """An estimate whose columns hold no more values than it has rows."""
    capped = tuple(Column(None, rows) if c is None else Column(c.base, min(c.ndv, rows), c.stats, c.null_share)
                   for c in columns)
    return Estimate(rows, capped)


def _union(children) -> Estimate:
    rows = sum(child.rows for child in children)
    columns = []
    for parts in zip(*(child.columns for child in children)):
        # Columns of different tables share no statistics, but hold no more values than all
        # of them together.
        same = len({p.base for p in parts}) == 1
        columns.append(Column(parts[0].base if same else None, sum(p.ndv for p in parts),
                              parts[0].stats if same else None, parts[0].null_share if same else 0.0))
    return shaped(rows, columns)


def _aggregate(node, child: Estimate) -> Estimate:
    keys = parse_exprs(node.fields["group_by"])
    ndvs = [child.columns[k.index].ndv if isinstance(k, ColumnRef) else child.rows for k in keys]
    if "grouping_sets" in node.fields:
        held = [[keys.index(k) for k in s] for s in parse_grouping_sets(node.fields["grouping_sets"])]
    else:
        held = [list(range(len(keys)))]
    rows = sum(min(math.prod(ndvs[i] for i in s), child.rows) if s else min(1.0, child.rows) for s in held)
    # A grouped key is a new relation's: its values are the groups', no longer the base
    # table's domain, so it leaves the base column's class; a filter above still reads its range.
    columns = [_regrouped(child.columns[k.index]) if isinstance(k, ColumnRef) else None for k in keys]
    columns += [None] * (len(node.schema) - len(columns))
    return shaped(rows, columns)


def _regrouped(column: Column) -> Column:
    return Column(None, column.ndv, column.stats, column.null_share)
