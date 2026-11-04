"""How the optimizer's report names what a plan reads, so that a relation keeps one name from one
DPhyp call of its cluster to the next, where its index `rN` shifts, until it is itself read back
from memory and named as that source.

A relation is its table, a memory source its name; `(f)` marks a filter over it, the planner's
null-key filter below a shuffle aside; an aggregate is `agg(...)` of the cluster under it, or of
what it reads where no cluster is; a table the plan reads more than once is numbered, `date_dim#2`,
in plan order. A join is written with its build in brackets and its kind as SQL names it, the side
an outer, semi or anti join keeps first: `([nation (f)] join customer) left join [orders]`. So no
join reads as a right join.
"""

from __future__ import annotations

import dataclasses
from collections import Counter
from dataclasses import dataclass

from ..operators.join_types import JoinType
from ..plans import engine_ir
from ..plans.engine_expr import ColumnRef, parse_columns, parse_exprs, parse_join_keys, parse_named_exprs
from ..plans.engine_plan import EngineNode, memory_source
from .multijoin import keys_dropping_nulls, null_key_filter

#: nodes a name reads through: what they emit is their input's rows, a filter's a subset of them
_THROUGH = frozenset({"GpuFilter", "GpuProject", "GpuCoalesceAllBatches", "GpuEmitPartitions",
                      "GpuMergePartitions", "GpuMergeSortedPartitions", "GpuSort", "GpuAccumulateBatchesAndSort"})
_AGGREGATES = frozenset({"GpuAggregate", "GpuAggregateBatches"})
_JOINS = frozenset({"GpuHashJoin", "GpuNestedLoopJoin"})
#: a join type's SQL name, and whether the side it keeps is its probe, written first then
_SQL = {
    JoinType.INNER: ("join", False), JoinType.FULL: ("full join", False),
    JoinType.LEFT: ("left join", False), JoinType.RIGHT: ("left join", True),
    JoinType.LEFT_SEMI: ("semi join", False), JoinType.RIGHT_SEMI: ("semi join", True),
    JoinType.LEFT_ANTI: ("anti join", False), JoinType.RIGHT_ANTI: ("anti join", True),
    JoinType.LEFT_MARK: ("mark join", False),
}


@dataclass(frozen=True)
class Words:
    """A name or a join tree in words; `joined` where it is a join, which a probe side writes in
    parentheses and a build side in brackets."""

    text: str
    joined: bool = False


def joined(join_type: JoinType | None, build: Words, probe: Words) -> Words:
    """The join of `build` and `probe` of `join_type`, None for a cross join."""
    built, probed = f"[{build.text}]", f"({probe.text})" if probe.joined else probe.text
    if join_type is None:
        return Words(f"{built} cross join {probed}", True)
    word, probe_first = _SQL[join_type]
    return Words(f"{probed} {word} {built}" if probe_first else f"{built} {word} {probed}", True)


def join_name(join_type: JoinType) -> str:
    return _SQL[join_type][0]


def relation_name(root: EngineNode) -> str:
    """A relation rooted at `root`, a cluster under it by its id."""
    return _words(root, collapsed=True).text


def described(node: EngineNode, dropped: frozenset[int] = frozenset()) -> str:
    """What `node` computes, its joins written out; a cluster under an aggregate by its id.
    `dropped`: the keys a join above reads `node` on whose NULLs match nothing there."""
    return _words(node, collapsed=False, dropped=dropped).text


def dropped_by_source(*roots: EngineNode) -> dict[str, frozenset[int]]:
    """For each memory source a hash join under `roots` builds from directly, the keys of that join
    whose NULLs match nothing on it (`multijoin.keys_dropping_nulls`)."""
    return {join.children[0].fields["name"]: keys_dropping_nulls(join)[0]
            for root in roots for join in _preorder(root)
            if join.kind == "GpuHashJoin" and join.children[0].kind == "GpuMemorySource"}


def _preorder(node: EngineNode):
    yield node
    for child in node.children:
        yield from _preorder(child)


def _words(node: EngineNode, collapsed: bool, in_aggregate: bool = False,
           dropped: frozenset[int] = frozenset()) -> Words:
    filtered = False
    # The planner's null-key filter drops only what the join reading `node` matches to nothing.
    wiring = null_key_filter(node, dropped) if dropped else None
    # Both phases of an aggregate are one: the partial one under the final is read through.
    while node.kind in _THROUGH or in_aggregate and node.kind in _AGGREGATES:
        filtered |= node.kind == "GpuFilter" and node is not wiring
        node = node.children[0]
    words = _bare(node, collapsed)
    if not filtered:
        return words
    return Words(f"({words.text}) (f)" if words.joined else f"{words.text} (f)")


def _bare(node: EngineNode, collapsed: bool) -> Words:
    if node.kind == "GpuLoadParquet":
        return Words(node.alias or node.fields["table"])
    if node.kind == "GpuMemorySource":
        return Words(node.fields["name"])
    if collapsed and node.mj is not None:
        return Words(node.mj)
    if node.kind in _JOINS or node.kind == "GpuCrossJoin":
        sides = keys_dropping_nulls(node) if node.kind == "GpuHashJoin" else (frozenset(), frozenset())
        build, probe = (_words(child, collapsed, dropped=keys) for child, keys in zip(node.children, sides))
        return joined(None if node.kind == "GpuCrossJoin" else engine_ir.join_type(node), build, probe)
    if node.kind in _AGGREGATES:
        return Words(f"agg({_words(node.children[0], True, in_aggregate=True).text})")
    kind = "union" if node.kind in ("GpuUnion", "GpuInterleave") else node.kind.removeprefix("Gpu").lower()
    return Words(f"{kind}({', '.join(_words(child, collapsed).text for child in node.children)})")


def join_words(join: EngineNode) -> str:
    """A join's kind and keys in words, as a replan names the join whose build missed."""
    if join.kind == "GpuCrossJoin":
        return "cross join"
    words = join_name(engine_ir.join_type(join))
    if join.kind == "GpuNestedLoopJoin":
        words = f"nested loop {words}"
    if "on" in join.fields:
        words += " on " + " and ".join(f"{b.name} = {p.name}" for b, p in parse_join_keys(join.fields["on"]))
    return words


def memory_sources(root: EngineNode) -> tuple[str, ...]:
    """The memory sources `relation_name(root)` shows, in plan order: those under a cluster it names
    by its id are that cluster's."""
    return _sources(root, collapsed=True)


def described_sources(node: EngineNode) -> tuple[str, ...]:
    """The memory sources `described(node)` shows, in plan order."""
    return _sources(node, collapsed=False)


def _sources(node: EngineNode, collapsed: bool) -> tuple[str, ...]:
    if node.kind == "GpuMemorySource":
        return (node.fields["name"],)
    if collapsed and node.mj is not None:
        return ()
    collapsed |= node.kind in _AGGREGATES
    return tuple(name for child in node.children for name in _sources(child, collapsed))


def sourced(name: str, side: EngineNode) -> EngineNode:
    """The memory source `side` becomes, carrying which of its columns are an aggregate's values."""
    found = memory_source(name, side)
    return dataclasses.replace(found, aggregate_columns=tuple(k for k in range(len(side.schema))
                                                       if is_aggregate_output(side, k)))


def is_aggregate_output(root: EngineNode, ordinal: int) -> bool:
    """Whether output `ordinal` of `root` is an aggregate's value rather than one of its keys or a
    column read through: it has no statistics, and a name like `min(t.v)` is no function call."""
    node = root
    while True:
        if node.kind == "GpuMemorySource":
            return ordinal in node.aggregate_columns
        if node.kind in _AGGREGATES:
            return ordinal >= len(parse_exprs(node.fields["group_by"]))
        if node.kind == "GpuProject":
            expr, _ = parse_named_exprs(node.fields["exprs"])[ordinal]
            if not isinstance(expr, ColumnRef):
                return False
            ordinal = expr.index
        elif node.kind == "GpuFilter" and "projection" in node.fields:
            ordinal = parse_columns(node.fields["projection"])[ordinal].index
        elif node.kind not in _THROUGH:
            return False
        node = node.children[0]


def aliased(plan: EngineNode) -> EngineNode:
    """A copy of `plan` in which each scan of a table the plan reads more than once carries
    `<table>#<n>`, numbered in plan order: one name for it in every cluster, build and pruned scan
    the report shows."""
    scans, stack = [], [plan]
    while stack:
        node = stack.pop()
        scans += [node] if node.kind == "GpuLoadParquet" else []
        stack.extend(reversed(node.children))
    reads, numbers, aliases = Counter(scan.fields["table"] for scan in scans), Counter(), {}
    for scan in scans:
        table = scan.fields["table"]
        if reads[table] > 1:
            numbers[table] += 1
            aliases[id(scan)] = f"{table}#{numbers[table]}"
    return _copied(plan, aliases)


def _copied(node: EngineNode, aliases: dict[int, str]) -> EngineNode:
    return dataclasses.replace(node, children=[_copied(child, aliases) for child in node.children],
                               alias=aliases.get(id(node), node.alias))
