"""Filter selectivity from a column's NDV, min/max and null count (`stats.ColumnStats`).

The model is DuckDB's (Ebergen 2022): a column holds its NDV values spread evenly over
[min, max], each as often as the others, and NULL matches no comparison.

- `col = v` is 1/NDV of the non-null rows, 0 with `v` outside [min, max]; an OR of equalities
  on one column is its distinct literals over NDV;
- a range is the share of the evenly spaced values it covers, each value owning one step of
  the domain — so `d_moy <= 4` over 1..12 is 4/12, not (4 − 1)/(12 − 1). Bounds on one column
  in a conjunction merge into one interval before anything multiplies;
- AND multiplies and OR adds less the overlap, both as if independent; NOT is the complement;
- what the statistics cannot speak to — LIKE, a column against a column, a computed operand, a
  string range — is DuckDB's 20%, once for an AND or OR with nothing else in it.
"""

from __future__ import annotations

from typing import Callable

from .dynamic_filters import as_number
from ..operators import expressions as E
from .stats import ColumnStats

#: DuckDB's selectivity for a filter it has no statistics for
DEFAULT = 0.2
_FLIP = {"==": "==", "!=": "!=", "<": ">", "<=": ">=", ">": "<", ">=": "<="}
_RANGE = frozenset({"<", "<=", ">", ">="})
_HOLDS = {"<": float.__lt__, "<=": float.__le__, ">": float.__gt__, ">=": float.__ge__}


def selectivity(predicate: E.Expr, column: Callable[[str], ColumnStats | None], rows: int) -> float:
    """The share of `rows` input rows `predicate` keeps; `column(name)` is the statistics of an
    input column, None where it has none."""
    return _Estimate(column, rows).of(predicate)


def restricted_ndv(predicate: E.Expr, column: Callable[[str], ColumnStats | None], rows: int) -> dict[str, float]:
    """The NDV a filter leaves each column it pins down — by an equality, a list of them, or a
    range among the predicate's conjuncts: the values it can still hold."""
    estimate, restricted, ranges = _Estimate(column, rows), {}, {}
    for part in _conjuncts(predicate):
        values, atom = _value_set(part), _comparison(part)
        if values is not None:
            stats = column(values[0])
            if stats is not None:
                inside = sum(1 for v in values[1] if _within(v, stats))
                restricted[values[0]] = min(restricted.get(values[0], inside), inside)
        elif atom is not None and atom[1] in _RANGE:
            ranges.setdefault(atom[0], []).append(atom[1:])
    for name, bounds in ranges.items():
        stats = column(name)
        nulls = estimate._null_share(name)
        if stats is not None and nulls is not None and nulls < 1:
            kept = stats.ndv * estimate._interval(name, bounds) / (1 - nulls)
            restricted[name] = min(restricted.get(name, kept), kept)
    return restricted


class _Estimate:
    def __init__(self, column, rows):
        self.column, self.rows = column, rows

    def of(self, expr: E.Expr) -> float:
        if isinstance(expr, E.Binary) and expr.op in ("and", "or") and not self._informed(expr):
            return DEFAULT
        if isinstance(expr, E.Binary) and expr.op == "and":
            return self._conjunction(_conjuncts(expr))
        if isinstance(expr, E.Binary) and expr.op == "or":
            values = _value_set(expr)
            if values is not None:
                return self._equal(*values)
            left, right = self.of(expr.left), self.of(expr.right)
            return left + right - left * right
        if isinstance(expr, E.Not):
            return 1 - self.of(expr.inner)
        if isinstance(expr, (E.IsNull, E.IsNotNull)) and isinstance(expr.inner, E.Col):
            nulls = self._null_share(expr.inner.column)
            if nulls is None:
                return DEFAULT
            return nulls if isinstance(expr, E.IsNull) else 1 - nulls
        if isinstance(expr, E.Like):
            return 1 - DEFAULT if expr.negated else DEFAULT
        atom = _comparison(expr)
        if atom is None:
            return DEFAULT
        name, op, value = atom
        if op == "==":
            return self._equal(name, {value})
        if op == "!=":
            nulls = self._null_share(name)
            return DEFAULT if nulls is None else max(0.0, 1 - nulls - self._equal(name, {value}))
        return self._interval(name, [(op, value)])

    def _conjunction(self, parts) -> float:
        ranges, product = {}, 1.0
        for part in parts:
            atom = _comparison(part)
            if atom is not None and atom[1] in _RANGE:
                ranges.setdefault(atom[0], []).append(atom[1:])
            else:
                product *= self.of(part)
        for name, bounds in ranges.items():
            product *= self._interval(name, bounds)
        return product

    def _equal(self, name: str, values: set) -> float:
        stats = self.column(name)
        if stats is None:
            return DEFAULT
        if stats.ndv == 0:
            return 0.0
        inside = [v for v in values if _within(v, stats)]
        return min(1.0, len(inside) / min(stats.ndv, max(self.rows, 1))) * (1 - self._null_share(name))

    def _interval(self, name: str, bounds) -> float:
        """The share of the column's evenly spaced values that every `(op, value)` bound keeps."""
        stats = self.column(name)
        numbers = [as_number(v) for _, v in bounds]
        if stats is None or stats.ndv == 0 or None in numbers:
            return DEFAULT if stats is None or stats.ndv else 0.0
        low, high = as_number(stats.low), as_number(stats.high)
        if low is None or high is None:
            return DEFAULT ** len(bounds)
        if high <= low:  # one value: each bound either keeps it or not
            kept = all(_HOLDS[op](low, at) for (op, _), at in zip(bounds, numbers))
            return float(kept) * (1 - self._null_share(name))
        step = (high - low) / (stats.ndv - 1) if stats.ndv > 1 else 0.0
        start, stop = low - step / 2, high + step / 2
        for (op, _), at in zip(bounds, numbers):
            if op == ">=":
                start = max(start, at - step / 2)
            elif op == ">":
                start = max(start, at + step / 2)
            elif op == "<=":
                stop = min(stop, at + step / 2)
            else:
                stop = min(stop, at - step / 2)
        share = max(0.0, stop - start) / (high - low + step)
        return min(1.0, share) * (1 - self._null_share(name))

    def _informed(self, expr: E.Expr) -> bool:
        """Whether any atom under `expr` has statistics to speak to it; an AND or OR of none is
        one unknown filter, not many — 400 defaults ORed together would claim every row."""
        if isinstance(expr, E.Binary) and expr.op in ("and", "or"):
            return self._informed(expr.left) or self._informed(expr.right)
        if isinstance(expr, E.Not):
            return self._informed(expr.inner)
        if isinstance(expr, (E.IsNull, E.IsNotNull)):
            return isinstance(expr.inner, E.Col) and self.column(expr.inner.column) is not None
        atom = _comparison(expr)
        return atom is not None and self.column(atom[0]) is not None and \
            (atom[1] not in _RANGE or as_number(atom[2]) is not None)

    def _null_share(self, name: str) -> float | None:
        stats = self.column(name)
        if stats is None:
            return None
        return 0.0 if stats.nulls is None or not self.rows else min(1.0, stats.nulls / self.rows)


def _conjuncts(expr):
    if isinstance(expr, E.Binary) and expr.op == "and":
        return _conjuncts(expr.left) + _conjuncts(expr.right)
    return [expr]


def _comparison(expr):
    """`(column, op, literal)` for a column compared with a literal, either way round."""
    if not isinstance(expr, E.Binary) or expr.op not in _FLIP:
        return None
    if isinstance(expr.left, E.Col) and isinstance(expr.right, E.Lit):
        return expr.left.column, expr.op, expr.right.value
    if isinstance(expr.left, E.Lit) and isinstance(expr.right, E.Col):
        return expr.right.column, _FLIP[expr.op], expr.left.value
    return None


def _value_set(expr):
    """`(column, values)` where an OR tree is equalities on one column and nothing else."""
    if isinstance(expr, E.Binary) and expr.op == "or":
        left, right = _value_set(expr.left), _value_set(expr.right)
        if left is None or right is None or left[0] != right[0]:
            return None
        return left[0], left[1] | right[1]
    atom = _comparison(expr)
    return (atom[0], {atom[2]}) if atom is not None and atom[1] == "==" else None


def _within(value, stats: ColumnStats) -> bool:
    """Whether `value` can be in [min, max]; true where the footer does not say or the two
    do not compare."""
    if stats.low is None or stats.high is None:
        return True
    at, low, high = as_number(value), as_number(stats.low), as_number(stats.high)
    if at is not None and low is not None and high is not None:
        return low <= at <= high
    if isinstance(value, str) and isinstance(stats.low, str) and isinstance(stats.high, str):
        return stats.low <= value <= stats.high
    return True
