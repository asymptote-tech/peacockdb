"""Dynamic filters: which joins can prune a fact scan, the probe plan that measures the keys
to prune by, and the replan that drops the row groups those keys rule out.

A join is a candidate when its build side holds what `Builds` admits and its probe key lifts
through pass-through nodes, hash joins and the branches of a union to a column of a parquet scan
that has more than one row group and is **ordered** by that column — the footer's
`Σ(max − min) / (MAX − MIN)` is 1 on an ordered column and the group count on an unordered one,
so anything under 1.5 is ordered; one candidate per scan the key reaches. The probe plan is the
build side itself, run to its root; `summarize` is the host-side reducer over its join key. A join
that equates NULLs prunes nothing by a build holding a NULL key.

`apply` runs each candidate join's probe plan once, keeps a row group only where its footer range
meets the keys' bounds (and holds one of their values, where they are few), and re-maps each
pruned scan's survivors onto the same lanes by the engine's own policy
(`scan_mapping/partition.rs`). The build the probe plan made is not thrown away: the main plan
reads it as a `GpuMemorySource` in the build side's place, laid out as the side was — so the side
is read once. A probe that prunes nothing still materialises its build, which gives the main run no
miss of its own: the cluster around it is reordered only where another build misses (`design.md`,
"Probe plans run adaptively").
"""

from __future__ import annotations

import datetime
import decimal
import json
import numbers
from dataclasses import dataclass, replace
from enum import Enum

import pandas as pd

from ..plans import engine_ir
from ..plans.engine_expr import ColumnRef, parse_columns, parse_join_keys, parse_named_exprs
from ..plans.engine_plan import EngineNode
from .multijoin import keys_dropping_nulls, null_key_filter, replaced
from .names import sourced
from ..operators.join_types import JoinType
from ..operators.joins import own_output

#: Join types that emit no unmatched probe row: a probe row whose key the build side lacks
#: is dropped by the join anyway, so a scan need not read it.
PRUNES_PROBE = frozenset({
    JoinType.INNER, JoinType.LEFT, JoinType.LEFT_SEMI, JoinType.LEFT_ANTI, JoinType.LEFT_MARK,
    JoinType.RIGHT_SEMI,
})
#: Nodes a probe column passes through by ordinal, unchanged.
_PASS = frozenset({"GpuEmitPartitions", "GpuMergePartitions", "GpuCoalesceAllBatches"})
#: Nodes whose output column k is column k of each child.
_BRANCHES = frozenset({"GpuUnion", "GpuInterleave"})
_JOINS = ("GpuHashJoin", "GpuNestedLoopJoin", "GpuCrossJoin")
ORDERED_BELOW = 1.5
#: DuckDB's `dynamic_or_filter_threshold`: an IN list above this many values is dropped.
IN_LIMIT = 50


class Builds(Enum):
    """Which build sides may prune. `FILTERED`, on the engine's plan: one holding a filter and no
    join — a join there is a cluster DPhyp has not ordered yet. `FILTERED_OR_JOINED`, on the plan
    DPhyp ordered: one holding a filter, a join, or both. A bare scan never: its keys span the
    whole domain."""

    FILTERED = "a filter, no join"
    FILTERED_OR_JOINED = "a filter or a join"


@dataclass(frozen=True)
class Candidate:
    """A join whose build keys can prune `scan`'s row groups on `column`."""

    join: EngineNode
    build_key: ColumnRef
    scan: EngineNode
    column: str
    row_groups: tuple[int, ...]
    clustering: float

    @property
    def scan_name(self) -> str:
        """The scan as the report names it: its alias where the plan reads its table twice."""
        return self.scan.alias or self.scan.fields["table"]


@dataclass(frozen=True)
class KeySummary:
    """What the probe plan's keys reduce to: `values` where there are at most `IN_LIMIT`, and
    whether one is NULL."""

    low: object
    high: object
    values: tuple | None
    null: bool = False


def candidates(plan: EngineNode, tables, builds: Builds) -> list[Candidate]:
    """Every (join, key, scan) of `plan` a dynamic filter could prune by, in pre-order, its build
    side admitted by `builds`. `tables.column_ranges(table, column)` is the footer's (min, max)
    per row group."""
    found, stack = [], [plan]
    while stack:
        node = stack.pop()
        stack.extend(reversed(node.children))
        if node.kind == "GpuHashJoin" and engine_ir.join_type(node) in PRUNES_PROBE:
            found += _join_candidates(node, tables, builds)
    return found


def _join_candidates(join, tables, builds: Builds) -> list[Candidate]:
    build, probe = join.children
    if not _admitted(join, builds):
        return []
    found = []
    for build_key, probe_key in parse_join_keys(join.fields["on"]):
        for scan, column in lift(probe, probe_key.index):
            groups = tuple(sorted({g for lane in _groups(scan) for batch in lane for g in batch}))
            if len(groups) < 2:
                continue
            clustering = _clustering(tables.column_ranges(scan.fields["table"], column), groups)
            if clustering is not None and clustering < ORDERED_BELOW:
                found.append(Candidate(join, build_key, scan, column, groups, clustering))
    return found


def lift(node: EngineNode, ordinal: int) -> list[tuple[EngineNode, str]]:
    """Each scan and file column `node`'s output column `ordinal` is, read through nodes that
    pass it unchanged and hash joins of any type, into the side the column is from, and into
    every branch of a union or an interleave, whose output column k is each branch's column k.
    A branch where the column is computed or comes from elsewhere adds nothing: its rows reach
    the join unpruned, which drops them as before. `design.md`, "Why a non-inner join passes the
    key"."""
    while True:
        if node.kind == "GpuLoadParquet":
            return [(node, parse_columns(node.fields["projections"])[ordinal].name)]
        if node.kind in _BRANCHES:
            return [leaf for child in node.children for leaf in lift(child, ordinal)]
        if node.kind in _PASS:
            node = node.children[0]
        elif node.kind == "GpuFilter":
            if "projection" in node.fields:
                ordinal = parse_columns(node.fields["projection"])[ordinal].index
            node = node.children[0]
        elif node.kind == "GpuProject":
            expr, _ = parse_named_exprs(node.fields["exprs"])[ordinal]
            if not isinstance(expr, ColumnRef):
                return []
            node, ordinal = node.children[0], expr.index
        elif node.kind == "GpuHashJoin":
            build, probe = node.children
            if "projection" in node.fields:
                ordinal = parse_columns(node.fields["projection"])[ordinal].index
            joined = own_output(engine_ir.join_type(node), len(build.schema), len(probe.schema))[ordinal]
            if joined is None:
                return []
            node, ordinal = (build, joined) if joined < len(build.schema) else (probe, joined - len(build.schema))
        else:
            return []


def probe_plan(candidate: Candidate) -> EngineNode:
    """The build side of the candidate's join, whole: run to its root, its lanes are the build
    the main plan will read."""
    return candidate.join.children[0]


def summarize(keys: pd.Series) -> KeySummary:
    """The host reducer: min, max, the distinct values where they are few, and whether a key is
    NULL. No key at all is a summary too — every row group of the scan can go."""
    present = keys.dropna()
    null = len(present) < len(keys)
    if not len(present):
        return KeySummary(None, None, (), null)
    distinct = present.unique()
    values = tuple(sorted(distinct)) if len(distinct) <= IN_LIMIT else None
    return KeySummary(present.min(), present.max(), values, null)


def _admitted(join: EngineNode, builds: Builds) -> bool:
    """Whether `join`'s build side is filtered and joinless, or under `FILTERED_OR_JOINED` filtered or
    joined. The planner's null-key filter below its shuffle filters nothing the join could emit, as
    a cluster reads it."""
    build = join.children[0]
    wiring = null_key_filter(build, keys_dropping_nulls(join)[0])
    filtered = any(node.kind == "GpuFilter" and node is not wiring for node in _nodes(build))
    if builds is Builds.FILTERED:
        return filtered and not _contains(build, *_JOINS)
    return filtered or _contains(build, *_JOINS)


def _contains(node, *kinds) -> bool:
    return node.kind in kinds or any(_contains(child, *kinds) for child in node.children)


def _nodes(node: EngineNode):
    yield node
    for child in node.children:
        yield from _nodes(child)


def _groups(scan):
    return json.loads(scan.fields["partition_groups"])


def _clustering(ranges, groups) -> float | None:
    """`Σ(max − min) / (MAX − MIN)` over the scan's row groups with statistics — a group
    without them (tpcds `store_sales`' NULL-date tail) can be pruned by nothing anyway."""
    spans = [(as_number(ranges[g][0]), as_number(ranges[g][1])) for g in groups if ranges[g] is not None]
    if not spans or any(lo is None or hi is None for lo, hi in spans):
        return None
    low, high = min(lo for lo, _ in spans), max(hi for _, hi in spans)
    if high == low:
        return None
    return sum(hi - lo for lo, hi in spans) / (high - low)


def as_number(value) -> float | None:
    """The statistic on a number line, or None for one (a string) that has no distance."""
    if isinstance(value, datetime.date):
        return float(value.toordinal())
    if isinstance(value, (numbers.Real, decimal.Decimal)):
        return float(value)
    return None


# -- after the probes: which row groups survive, and the replan ---------------------


class Batching(Enum):
    """How a mode packs a lane's row groups (`plan::Batching`); `KEEP_BATCHES` keeps a
    sized mode's batches minus what was pruned, the estimator's byte target being unknown here."""

    ONE_PER_LANE = "single"
    ONE_PER_ROW_GROUP = "rowgroup"
    KEEP_BATCHES = "sized"


def row_group_survives(footer_range, summary: KeySummary) -> bool:
    """`duckdb_cost.py::rowgroup_survives` over one key: a group goes only where its footer
    range misses the keys' bounds, or holds none of their values when they are few. A group
    without statistics stays; no keys at all leave nothing."""
    if summary.values == ():
        return False
    if footer_range is None:
        return True
    low, high = (as_number(v) for v in footer_range)
    if low is None or high is None:
        return True
    if high < as_number(summary.low) or low > as_number(summary.high):
        return False
    return summary.values is None or any(low <= as_number(v) <= high for v in summary.values)


@dataclass(frozen=True)
class PrunedBy:
    """A dynamic filter that reached a scan: the scan's column, the keys it pruned by, and the build
    that made them — its memory source's name and its key column."""

    column: str
    keys: KeySummary
    build: str
    build_key: str


@dataclass(frozen=True)
class Pruned:
    """One probed scan: its row groups before and after the dynamic filters that reached it,
    each filter in `by`, and the scan's alias where the plan reads its table more than once.
    `after == before` is a scan the keys could not narrow."""

    table: str
    column: str
    before: tuple[int, ...]
    after: tuple[int, ...]
    by: tuple[PrunedBy, ...] = ()
    alias: str | None = None


def apply(plan: EngineNode, tables, batching: Batching, run, builds: Builds, kept: dict | None = None
          ) -> tuple[EngineNode, list[Pruned], dict]:
    """The plan with every candidate join's probe plan run (`run(plan, builds) -> frames by lane`,
    `builds` those made so far, which it may read), its build side a memory source over what the
    probe made, and each pruned scan re-mapped; the builds this call kept, by memory source name:
    (the side each replaces, its frames by lane). `kept` are those earlier calls kept, which
    `plan` reads; names continue after theirs.

    A candidate whose build holds another's join, or a scan another prunes this round, waits for a
    later round, so that its probe reads the inner build from memory and the scan pruned; a source
    is neither filter nor join, so each round admits fewer builds, and a plan without nesting
    takes one."""
    built, report = dict(kept or {}), []
    while found := candidates(plan, tables, builds):
        plan, pruned = _round(plan, _ready(found), tables, batching, run, built)
        report += pruned
    return plan, report, {name: build for name, build in built.items() if name not in (kept or {})}


def _ready(found: list[Candidate]) -> list[Candidate]:
    """The candidates whose build holds neither another's join nor a scan another ready one prunes.
    The topmost ready join waits for nothing, so none is ever empty."""
    joins = {id(c.join) for c in found}
    ready = [c for c in found if not any(id(node) in joins for node in _nodes(probe_plan(c)))]
    scans = {id(c.scan) for c in ready}
    return [c for c in ready if not any(id(node) in scans for node in _nodes(probe_plan(c)))]


def _round(plan, found: list[Candidate], tables, batching: Batching, run, built: dict):
    """`apply` over `found`, whose builds hold none of their joins; `built` gains the builds."""
    survivors: dict[int, set[int]] = {}
    scans, summaries, sides = {}, {}, {}
    for candidate in found:
        side = probe_plan(candidate)
        if id(side) not in sides:
            name = f"p{len(built)}"
            built[name] = (side, run(side, dict(built)))
            sides[id(side)] = name, side
        name, _ = sides[id(side)]
        column = engine_ir.frame_names([n for n, _ in side.schema])[candidate.build_key.index]
        summary = summarize(pd.concat([lane[column] for lane in built[name][1]]))
        ranges = tables.column_ranges(candidate.scan.fields["table"], candidate.column)
        if summary.null and candidate.join.fields.get("null_equals_null") == "true":
            # A NULL build key matches the NULL probe keys any row group may hold.
            kept = set(candidate.row_groups)
        else:
            kept = {g for g in candidate.row_groups if row_group_survives(ranges[g], summary)}
        key = id(candidate.scan)
        survivors[key] = survivors.get(key, set(candidate.row_groups)) & kept
        scans[key] = candidate.scan
        summaries.setdefault(key, []).append(PrunedBy(candidate.column, summary, name, candidate.build_key.name))
    report, mappings = [], {}
    for key, kept in survivors.items():
        scan = scans[key]
        before = tuple(sorted({g for lane in _groups(scan) for batch in lane for g in batch}))
        if len(kept) < len(before):
            mappings[key] = remap(_groups(scan), sorted(kept), tables.row_counts(scan.fields["table"]), batching)
        columns = ",".join(by.column for by in summaries[key])
        report.append(Pruned(scan.fields["table"], columns, before, tuple(sorted(kept)), tuple(summaries[key]),
                             scan.alias))
    for name, side in sides.values():
        plan = replaced(plan, side, sourced(name, side))
    return (_with_mappings(plan, mappings) if mappings else plan), report


def remap(groups, kept: list[int], row_counts: list[int], batching: Batching) -> list:
    """The survivors on the scan's lanes, as `partition.rs` lays them out: contiguous chunks
    balanced by rows, then batches per the mode."""
    if batching is Batching.KEEP_BATCHES:
        wanted = set(kept)
        return [[[g for g in batch if g in wanted] for batch in lane if set(batch) & wanted]
                for lane in groups]
    chunks = _balanced_chunks([row_counts[g] for g in kept], len(groups))
    lanes = []
    for start, stop in chunks:
        chunk = kept[start:stop]
        if not chunk:
            lanes.append([])
        elif batching is Batching.ONE_PER_LANE:
            lanes.append([chunk])
        else:
            lanes.append([[g] for g in chunk])
    return lanes


def _balanced_chunks(rows: list[int], lanes: int) -> list[tuple[int, int]]:
    """`balanced_chunks` in `partition.rs`: each lane stops where one more group would land
    further from its share than stopping does."""
    total, taken, index, chunks = sum(rows), 0, 0, []
    for part in range(lanes):
        want = -(-(total - taken) // (lanes - part))
        start, got = index, 0
        while index < len(rows):
            after = got + rows[index]
            if index > start and abs(after - want) >= abs(got - want):
                break
            got, index = after, index + 1
        chunks.append((start, index))
        taken += got
    return chunks


def _with_mappings(node: EngineNode, mappings: dict) -> EngineNode:
    children = [_with_mappings(child, mappings) for child in node.children]
    fields = dict(node.fields)
    if id(node) in mappings:
        fields["partition_groups"] = json.dumps(mappings[id(node)], separators=(",", ":"))
    return replace(node, fields=fields, children=children)
