"""Dynamic filters: which joins can prune a fact scan, the probe plan that measures the keys
to prune by, and the replan that drops the row groups those keys rule out.

A join is a candidate when its build side is a filtered subtree with no join in it, and its
probe key lifts through pass-through nodes (and inner joins) to a column of a parquet scan that
has more than one row group and is **ordered** by that column — the footer's
`Σ(max − min) / (MAX − MIN)` is 1 on an ordered column and the group count on an unordered
one, so anything under 1.5 is ordered. The probe plan is the build side itself, run to its
root; `summarize` is the host-side reducer over its join key.

`apply` runs each candidate join's probe plan once, keeps a row group only where its footer range
meets the keys' bounds (and holds one of their values, where they are few), and re-maps each
pruned scan's survivors onto the same lanes by the engine's own policy
(`scan_mapping/partition.rs`). The build the probe plan made is not thrown away: the main plan
reads it as a `GpuMemorySource` in the build side's place, laid out as the side was — so the side
is read once, and a probe that prunes nothing costs nothing but its turn coming first.
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
from ..plans.engine_plan import EngineNode, memory_source
from .multijoin import replaced
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
_JOINS = ("GpuHashJoin", "GpuNestedLoopJoin", "GpuCrossJoin")
ORDERED_BELOW = 1.5
#: DuckDB's `dynamic_or_filter_threshold`: an IN list above this many values is dropped.
IN_LIMIT = 50


@dataclass(frozen=True)
class Candidate:
    """A join whose build keys can prune `scan`'s row groups on `column`."""

    join: EngineNode
    build_key: ColumnRef
    scan: EngineNode
    column: str
    row_groups: tuple[int, ...]
    clustering: float


@dataclass(frozen=True)
class KeySummary:
    """What the probe plan's keys reduce to: `values` where there are at most `IN_LIMIT`."""

    low: object
    high: object
    values: tuple | None


def candidates(plan: EngineNode, tables) -> list[Candidate]:
    """Every (join, key) of `plan` a dynamic filter could prune a scan by, in pre-order.
    `tables.column_ranges(table, column)` is the footer's (min, max) per row group."""
    found, stack = [], [plan]
    while stack:
        node = stack.pop()
        stack.extend(reversed(node.children))
        if node.kind == "GpuHashJoin" and engine_ir.join_type(node) in PRUNES_PROBE:
            found += _join_candidates(node, tables)
    return found


def _join_candidates(join, tables) -> list[Candidate]:
    build, probe = join.children
    if not _contains(build, "GpuFilter") or _contains(build, *_JOINS):
        return []
    found = []
    for build_key, probe_key in parse_join_keys(join.fields["on"]):
        lifted = lift(probe, probe_key.index)
        if lifted is None:
            continue
        scan, column = lifted
        groups = tuple(sorted({g for lane in _groups(scan) for batch in lane for g in batch}))
        if len(groups) < 2:
            continue
        clustering = _clustering(tables.column_ranges(scan.fields["table"], column), groups)
        if clustering is not None and clustering < ORDERED_BELOW:
            found.append(Candidate(join, build_key, scan, column, groups, clustering))
    return found


def lift(node: EngineNode, ordinal: int) -> tuple[EngineNode, str] | None:
    """The scan and file column `node`'s output column `ordinal` is, read through nodes that
    pass it unchanged and inner joins; None where it is computed or comes from elsewhere."""
    while True:
        if node.kind == "GpuLoadParquet":
            return node, parse_columns(node.fields["projections"])[ordinal].name
        if node.kind in _PASS:
            node = node.children[0]
        elif node.kind == "GpuFilter":
            if "projection" in node.fields:
                ordinal = parse_columns(node.fields["projection"])[ordinal].index
            node = node.children[0]
        elif node.kind == "GpuProject":
            expr, _ = parse_named_exprs(node.fields["exprs"])[ordinal]
            if not isinstance(expr, ColumnRef):
                return None
            node, ordinal = node.children[0], expr.index
        elif node.kind == "GpuHashJoin" and engine_ir.join_type(node) is JoinType.INNER:
            build, probe = node.children
            own = own_output(JoinType.INNER, len(build.schema), len(probe.schema))
            if "projection" in node.fields:
                ordinal = parse_columns(node.fields["projection"])[ordinal].index
            joined = own[ordinal]
            node, ordinal = (build, joined) if joined < len(build.schema) else (probe, joined - len(build.schema))
        else:
            return None


def probe_plan(candidate: Candidate) -> EngineNode:
    """The build side of the candidate's join, whole: run to its root, its lanes are the build
    the main plan will read."""
    return candidate.join.children[0]


def summarize(keys: pd.Series) -> KeySummary:
    """The host reducer: min, max, and the distinct values where they are few. No key at all
    is a summary too — every row group of the scan can go."""
    present = keys.dropna()
    if not len(present):
        return KeySummary(None, None, ())
    distinct = present.unique()
    values = tuple(sorted(distinct)) if len(distinct) <= IN_LIMIT else None
    return KeySummary(present.min(), present.max(), values)


def _contains(node, *kinds) -> bool:
    return node.kind in kinds or any(_contains(child, *kinds) for child in node.children)


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
class Pruned:
    """One probed scan: its row groups before and after the dynamic filters that reached it,
    and per filter the column and the keys it pruned by. `after == before` is a scan the
    keys could not narrow."""

    table: str
    column: str
    before: tuple[int, ...]
    after: tuple[int, ...]
    keys: tuple[tuple[str, KeySummary], ...] = ()


def apply(plan: EngineNode, tables, batching: Batching, run) -> tuple[EngineNode, list[Pruned], dict]:
    """The plan with every candidate join's probe plan run (`run(plan) -> frames by lane`), its
    build side a memory source over what the probe made, and each pruned scan re-mapped; the
    builds kept, by memory source name: (the side each replaces, its frames by lane)."""
    survivors: dict[int, set[int]] = {}
    scans, summaries, kept_builds = {}, {}, {}
    for candidate in candidates(plan, tables):
        side = probe_plan(candidate)
        if id(side) not in kept_builds:
            kept_builds[id(side)] = (f"p{len(kept_builds)}", side, run(side))
        _, _, lanes = kept_builds[id(side)]
        column = engine_ir.frame_names([name for name, _ in side.schema])[candidate.build_key.index]
        summary = summarize(pd.concat([lane[column] for lane in lanes]))
        ranges = tables.column_ranges(candidate.scan.fields["table"], candidate.column)
        kept = {g for g in candidate.row_groups if row_group_survives(ranges[g], summary)}
        key = id(candidate.scan)
        survivors[key] = survivors.get(key, set(candidate.row_groups)) & kept
        scans[key] = candidate.scan
        summaries.setdefault(key, []).append((candidate.column, summary))
    report, mappings = [], {}
    for key, kept in survivors.items():
        scan = scans[key]
        before = tuple(sorted({g for lane in _groups(scan) for batch in lane for g in batch}))
        if len(kept) < len(before):
            mappings[key] = remap(_groups(scan), sorted(kept), tables.row_counts(scan.fields["table"]), batching)
        columns = ",".join(column for column, _ in summaries[key])
        report.append(Pruned(scan.fields["table"], columns, before, tuple(sorted(kept)), tuple(summaries[key])))
    for name, side, _ in kept_builds.values():
        plan = replaced(plan, side, memory_source(name, side))
    plan = _with_mappings(plan, mappings) if mappings else plan
    return plan, report, {name: (side, lanes) for name, side, lanes in kept_builds.values()}


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
