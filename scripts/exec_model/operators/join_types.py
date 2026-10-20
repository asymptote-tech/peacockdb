"""Join types, and the two facts about each that the join executors read.

`JoinType` is `gpu_plan.fbs`'s enum, values included: one vocabulary for the pandas
executors and the spec's table, so a claim in one is the claim in the other. A cross join
and a nested loop are not members — they are plan node kinds, and on the wire all three
plan nodes are one `CudfJoin` carrying a type from this enum.

**Two facts per type** (join-rewrite-design.md §4.1). Every type streams its probe side:
with the build side complete, the join answers one probe batch at a time, residual or not.
And five types **finish** — Left, Full, LeftSemi, LeftAnti, LeftMark — because each emits
build rows by what matched across every probe batch, which no single batch can know; the
session keeps one `matched` flag per build row for them.
"""

from __future__ import annotations

from enum import Enum


class JoinType(Enum):
    """`gpu_plan.fbs`: enum JoinType : byte."""

    INNER = 0
    LEFT = 1
    RIGHT = 2
    FULL = 3
    LEFT_SEMI = 4
    RIGHT_SEMI = 5
    LEFT_ANTI = 6
    RIGHT_ANTI = 7
    LEFT_MARK = 8


#: Emit build rows by whether they matched across every probe batch, so they finish.
NEEDS_FINISH = frozenset(
    {JoinType.LEFT, JoinType.FULL, JoinType.LEFT_SEMI, JoinType.LEFT_ANTI, JoinType.LEFT_MARK}
)
#: Types whose entire output comes from the finish; a probe call answers nothing.
FINISH_ONLY = frozenset({JoinType.LEFT_SEMI, JoinType.LEFT_ANTI, JoinType.LEFT_MARK})


def needs_finish(join_type: JoinType) -> bool:
    """Does a join of this type answer anything at finish? `plan::needs_finish`, the same five."""
    return join_type in NEEDS_FINISH


#: Types where every output row carries the build side's key, so the probe's copy of it is
#: redundant. Not RIGHT or FULL: there an unmatched probe row has a null build key and the
#: probe's copy is the only place the value survives.
_KEY_FROM_BUILD = frozenset({JoinType.INNER, JoinType.LEFT})


def joined_projection(build_names, probe_names, key_pairs, join_type: JoinType) -> tuple[int, ...]:
    """The `projection` an equi-join carries over `[build…, probe…]`.

    Both sides' key columns survive the join, and a query that joined `a.k = b.k` asked for
    one `k`. DataFusion drops the duplicate with the join's own projection, so the model
    does too — by name, since a name collision is what makes it a duplicate, and only for
    the types where the surviving column answers for every row.
    """
    dropped = (
        {probe for (build, probe) in key_pairs if build_names[build] == probe_names[probe]}
        if join_type in _KEY_FROM_BUILD
        else set()
    )
    keep = list(range(len(build_names)))
    keep += [len(build_names) + i for i in range(len(probe_names)) if i not in dropped]
    return tuple(keep)


def joined_names(build_names, probe_names, key_pairs, join_type: JoinType) -> list[str]:
    """The column names `joined_projection` leaves, in order."""
    all_names = list(build_names) + list(probe_names)
    return [all_names[i] for i in joined_projection(build_names, probe_names, key_pairs, join_type)]


#: The sides whose unmatched rows are never emitted, (build, probe): a NULL key there matches
#: nothing, so the planner drops it below the side's shuffle. `null_key_droppable` in
#: `peacockdb-core/src/plan/join.rs`; `test_the_null_key_drop_table_is_the_engines` holds the two equal.
NULL_KEY_DROPPABLE = {
    JoinType.INNER: (True, True), JoinType.LEFT_SEMI: (True, True), JoinType.RIGHT_SEMI: (True, True),
    JoinType.LEFT: (False, True), JoinType.LEFT_ANTI: (False, True), JoinType.LEFT_MARK: (False, True),
    JoinType.RIGHT: (True, False), JoinType.RIGHT_ANTI: (True, False), JoinType.FULL: (False, False),
}
