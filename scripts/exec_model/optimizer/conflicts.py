"""Conflict detection: each join's total eligibility set (TES), the relations that must be on each
of its sides before it applies. CD-A of Moerkotte, Fender and Eich (SIGMOD 2013, §5): the
relations its predicate reads, plus, for each join below that the two may not swap past by the
`assoc`, `l-asscom` and `r-asscom` tables, that join's side which would be left behind. CD-A
keeps no conflict rules; the paper's CD-B and CD-C test rules per (left, right) pair, which
DPhyp's per-set cost cannot. Correct and not complete; `design.md`, "Non-inner joins: TES".

A join is taken logically: `LEFT`, `LEFT_SEMI` and `LEFT_ANTI` with the side they keep on the
left, `INNER` and `FULL` as planned; a `RIGHT…` join is its `LEFT…` mirror.
"""

from __future__ import annotations

from dataclasses import dataclass

from ..operators.join_types import JoinType

MIRRORED = {JoinType.INNER: JoinType.INNER, JoinType.FULL: JoinType.FULL,
            JoinType.LEFT: JoinType.RIGHT, JoinType.RIGHT: JoinType.LEFT,
            JoinType.LEFT_SEMI: JoinType.RIGHT_SEMI, JoinType.RIGHT_SEMI: JoinType.LEFT_SEMI,
            JoinType.LEFT_ANTI: JoinType.RIGHT_ANTI, JoinType.RIGHT_ANTI: JoinType.LEFT_ANTI}
_LOGICAL = frozenset({JoinType.INNER, JoinType.FULL, JoinType.LEFT, JoinType.LEFT_SEMI, JoinType.LEFT_ANTI})


def logical(join_type: JoinType) -> tuple[JoinType, bool]:
    """The join's logical type, and whether its plan sides are the other way round."""
    return (join_type, False) if join_type in _LOGICAL else (MIRRORED[join_type], True)


@dataclass(frozen=True)
class Operator:
    """A join as conflict detection reads it: its logical type, the relations under each of its
    sides (masks), those its predicate reads on each, and whether its predicate rejects NULLs on
    both sides — equi-keys alone, a NULL equal to nothing."""

    kind: JoinType
    left: int
    right: int
    reads_left: int
    reads_right: int
    strong: bool


def eligibility(operators: list[Operator]) -> list[tuple[int, int]]:
    """Each operator's TES, as the relations of it on its (left, right) side."""
    found = []
    for upper in operators:
        tes = upper.reads_left | upper.reads_right
        for lower in operators:
            under = lower.left | lower.right
            if under & ~upper.left == 0:
                if not _assoc(lower, upper):
                    tes |= lower.left
                if not _l_asscom(lower, upper):
                    tes |= lower.right
            elif under & ~upper.right == 0:
                if not _assoc(upper, lower):
                    tes |= lower.right
                if not _r_asscom(upper, lower):
                    tes |= lower.left
        found.append((tes & upper.left, tes & upper.right))
    return found


_KEEPS_ONE_SIDE = frozenset({JoinType.LEFT_SEMI, JoinType.LEFT_ANTI})


def _assoc(a: Operator, b: Operator) -> bool:
    """(e1 a e2) b e3 ≡ e1 a (e2 b e3), b's predicate reading e2 and e3."""
    if a.kind is JoinType.INNER:
        return b.kind is not JoinType.FULL
    if a.kind in _KEEPS_ONE_SIDE:
        return False
    if b.kind is JoinType.LEFT:
        return b.strong
    return a.kind is JoinType.FULL and b.kind is JoinType.FULL and a.strong and b.strong


def _l_asscom(a: Operator, b: Operator) -> bool:
    """(e1 a e2) b e3 ≡ (e1 b e3) a e2, a reading e1 and e2, b e1 and e3."""
    if JoinType.FULL not in (a.kind, b.kind):
        return True
    if a.kind is JoinType.FULL and b.kind is JoinType.FULL:
        return a.strong and b.strong
    if a.kind is JoinType.LEFT:
        return a.strong
    if b.kind is JoinType.LEFT:
        return b.strong
    return False


def _r_asscom(a: Operator, b: Operator) -> bool:
    """e1 a (e2 b e3) ≡ e2 b (e1 a e3), a reading e1 and e3, b e2 and e3."""
    if a.kind is JoinType.INNER and b.kind is JoinType.INNER:
        return True
    return a.kind is JoinType.FULL and b.kind is JoinType.FULL and a.strong and b.strong


def joining(edges, left: int, right: int):
    """The edges a join of the sets `left` and `right` applies — those with one side within each,
    as DPhyp joins two sets — or None where there is none."""
    applied = tuple(e for e in edges if e.left & ~left == 0 and e.right & ~right == 0
                    or e.left & ~right == 0 and e.right & ~left == 0)
    return applied or None
