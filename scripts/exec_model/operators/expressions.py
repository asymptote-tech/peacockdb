"""A small typed expression IR, evaluated with pandas' vectorized operators only.

Mirrors what `cudf::ast` accepts: column references, literals, and a fixed set of binary
and unary operations over whole columns. There is deliberately no escape hatch — no
`apply`, no lambda — because anything reachable only through one has no C++ counterpart
(subset rule 2 in `frame.py`).

Columns are named here rather than addressed by ordinal. The real IR uses ordinals
(`ColumnRef.index`, architecture.md's "Column indexing"), and nothing checks them; names
are chosen for the prototype because a wrong name fails loudly and a wrong ordinal is the
silent class of bug the prototype is not trying to model.
"""

from __future__ import annotations

import re
from dataclasses import dataclass

import pandas as pd


class Expr:
    """Evaluates to one column, given a frame."""

    def evaluate(self, frame: pd.DataFrame) -> pd.Series:
        raise NotImplementedError

    def name(self) -> str:
        """The output column's name: an `Alias`'s, which is how a plan names every output."""
        raise TypeError(f"{self} is unnamed; a projected expression is an Alias")


@dataclass(frozen=True)
class Col(Expr):
    column: str

    def evaluate(self, frame: pd.DataFrame) -> pd.Series:
        if self.column not in frame.columns:
            raise KeyError(f"no column {self.column!r} in {list(frame.columns)}")
        return frame[self.column]


@dataclass(frozen=True)
class Lit(Expr):
    value: object

    def evaluate(self, frame: pd.DataFrame) -> pd.Series:
        # Broadcast, not materialized element by element: `[value] * len(frame)` builds a
        # python object per row, which over a corpus query's six million rows is most of
        # the query's time (measured: 35 s of q6's 46 s, in the literals of one predicate).
        # A `cudf::ast` literal is a scalar the kernel broadcasts, so this is also the
        # closer model — and pandas infers the dtype from the value, where a list of
        # Timestamps would have arrived as object.
        return pd.Series(self.value, index=frame.index)


#: The operator set cudf::ast exposes over two columns. Anything not here is unsupported
#: on purpose: adding a python-only operation would make the prototype unportable.
_BINARY = {
    "+": lambda a, b: a + b,
    "-": lambda a, b: a - b,
    "*": lambda a, b: a * b,
    "/": lambda a, b: a / b,
    "==": lambda a, b: a == b,
    "!=": lambda a, b: a != b,
    "<": lambda a, b: a < b,
    "<=": lambda a, b: a <= b,
    ">": lambda a, b: a > b,
    ">=": lambda a, b: a >= b,
    "and": lambda a, b: a & b,
    "or": lambda a, b: a | b,
}

#: The comparisons, which are the operators that have to say NULL rather than False.
_COMPARISONS = frozenset({"==", "!=", "<", "<=", ">", ">="})


def three_valued(result: pd.Series, *operands: pd.Series) -> pd.Series:
    """A comparison's result as SQL and cuDF have it: NULL where an operand was NULL.

    pandas answers `None != 'x'` with True on an object column, where SQL answers NULL and
    a filter therefore drops the row. TPC-DS q19 is the case: `substr(ca_zip,…) <>
    substr(s_zip,…)` over an address that has no zip is not a match, and taking it for one
    invented a group with the highest revenue in the answer.

    The result is pandas' nullable `boolean`, whose `&`, `|` and `~` are already Kleene
    logic — so `AND`/`OR`/`NOT` over these need no special case, and the only other place
    that has to know is whatever turns a mask into rows (`FilterExec`, `Case`), which reads
    NULL as false.
    """
    valid = operands[0].notna()
    for operand in operands[1:]:
        valid &= operand.notna()
    return pd.Series(result, index=valid.index).astype("boolean").mask(~valid)


@dataclass(frozen=True)
class Binary(Expr):
    op: str
    left: Expr
    right: Expr

    def __post_init__(self):
        if self.op not in _BINARY:
            raise ValueError(f"unsupported operator {self.op!r}; cudf::ast has no counterpart")

    def evaluate(self, frame: pd.DataFrame) -> pd.Series:
        left, right = self.left.evaluate(frame), self.right.evaluate(frame)
        result = _BINARY[self.op](left, right)
        if self.op in _COMPARISONS:
            return three_valued(result, left, right)
        return result


@dataclass(frozen=True)
class Not(Expr):
    inner: Expr

    def evaluate(self, frame: pd.DataFrame) -> pd.Series:
        return ~self.inner.evaluate(frame)


@dataclass(frozen=True)
class IsNotNull(Expr):
    """The predicate #137 wants the planner to insert under a shuffle."""

    inner: Expr

    def evaluate(self, frame: pd.DataFrame) -> pd.Series:
        return self.inner.evaluate(frame).notna()


@dataclass(frozen=True)
class IsNull(Expr):
    inner: Expr

    def evaluate(self, frame: pd.DataFrame) -> pd.Series:
        return self.inner.evaluate(frame).isna()


@dataclass(frozen=True)
class Sqrt(Expr):
    """`cudf::unary_operator::SQRT`, which the IR gains for the stddev finalize."""

    inner: Expr

    def evaluate(self, frame: pd.DataFrame) -> pd.Series:
        import numpy as np

        return pd.Series(np.sqrt(self.inner.evaluate(frame).to_numpy(dtype="float64")))


@dataclass(frozen=True)
class Like(Expr):
    """SQL `LIKE`, as `LikeExprNode` carries it — `%` any run, `_` one character.

    cuDF's AST has no LIKE, so the C++ evaluates it on the column-producing path with
    `cudf::strings::like` (`expr.cpp` ~L382). pandas' `str.match` takes a regex, so the
    pattern is translated rather than passed through: the point is to model what the
    engine does, and what the engine does is a pattern language with two metacharacters.
    """

    inner: Expr
    pattern: str
    negated: bool = False

    def evaluate(self, frame: pd.DataFrame) -> pd.Series:
        regex = "^" + "".join(
            ".*" if char == "%" else "." if char == "_" else re.escape(char)
            for char in self.pattern
        ) + "$"
        matched = self.inner.evaluate(frame).str.match(regex, na=False)
        return ~matched if self.negated else matched


@dataclass(frozen=True)
class DatePart(Expr):
    """`date_part(field, ts)` — the `ScalarFunctionExprNode` the C++ maps to
    `cudf::datetime::extract_datetime_component` (`expr.cpp` ~L646). TPC-H's shipping
    queries group by year, which is what this is for."""

    field: str
    inner: Expr

    def evaluate(self, frame: pd.DataFrame) -> pd.Series:
        values = pd.to_datetime(self.inner.evaluate(frame))
        return getattr(values.dt, self.field.lower())


@dataclass(frozen=True)
class Substring(Expr):
    """`substr(col, start, length)`, 1-based as SQL is — `cudf::strings::slice_strings`
    (`expr.cpp` ~L680). q22 groups by the first two characters of a phone number."""

    inner: Expr
    start: int
    length: int

    def evaluate(self, frame: pd.DataFrame) -> pd.Series:
        begin = self.start - 1
        return self.inner.evaluate(frame).str.slice(begin, begin + self.length)


@dataclass(frozen=True)
class Round(Expr):
    """`round(x [, places])` — `cudf::round` with HALF_UP over float64 (`expr.cpp` ~L702).

    Half **away from zero**, which is DataFusion's `f64::round` and not python's
    banker's rounding, so `round(2.5)` is 3 and not 2. TPC-DS q54 buckets revenue with it,
    where the difference decides which segment a customer lands in.
    """

    inner: Expr
    places: int = 0

    def evaluate(self, frame: pd.DataFrame) -> pd.Series:
        import numpy as np

        values = self.inner.evaluate(frame).to_numpy(dtype="float64")
        scale = 10.0 ** self.places
        return pd.Series(np.sign(values) * np.floor(np.abs(values) * scale + 0.5) / scale,
                         index=frame.index)


@dataclass(frozen=True)
class Cast(Expr):
    """`cast(x AS <type>)` — `CastExprNode`, which the C++ turns into `cudf::cast`
    (`expr.cpp` ~L292). Only the casts the corpus needs; `dtype` is a pandas dtype."""

    inner: Expr
    dtype: str

    def evaluate(self, frame: pd.DataFrame) -> pd.Series:
        import numpy as np

        values = self.inner.evaluate(frame)
        if self.dtype == "int64" and values.isna().any():
            # An integer column holding a null is float64 here (`frame.py`): the cast
            # truncates what it can and keeps the null, where `astype` would refuse the batch.
            return np.trunc(values.astype("float64"))
        return values.astype(self.dtype)


@dataclass(frozen=True)
class Upper(Expr):
    """`upper(col)` — `cudf::strings::to_upper` (`expr.cpp` ~L730)."""

    inner: Expr

    def evaluate(self, frame: pd.DataFrame) -> pd.Series:
        return self.inner.evaluate(frame).str.upper()


@dataclass(frozen=True)
class Lower(Expr):
    """`lower(col)` — `cudf::strings::to_lower`, the sibling of the `upper` `expr.cpp`
    handles at ~L730. TPC-DS q99 sorts call centres by their lowercased name."""

    inner: Expr

    def evaluate(self, frame: pd.DataFrame) -> pd.Series:
        return self.inner.evaluate(frame).str.lower()


@dataclass(frozen=True)
class Concat(Expr):
    """`concat(a, b, …)` — `cudf::strings::concatenate` with an empty separator.

    A null argument contributes the empty string rather than nulling the row, which is
    DataFusion's `concat` and what `expr.cpp` passes as `narep` (~L748). TPC-DS builds
    display names this way, over columns that are legitimately null.
    """

    parts: tuple

    def evaluate(self, frame: pd.DataFrame) -> pd.Series:
        result = None
        for part in self.parts:
            piece = part.evaluate(frame).astype("string").fillna("")
            result = piece if result is None else result + piece
        return result


@dataclass(frozen=True)
class Coalesce(Expr):
    """`coalesce(a, b, …)` — the first non-null per row, as `expr.cpp`'s `copy_if_else`
    fold from the last argument back (~L754)."""

    parts: tuple

    def evaluate(self, frame: pd.DataFrame) -> pd.Series:
        result = self.parts[-1].evaluate(frame)
        for part in reversed(self.parts[:-1]):
            values = part.evaluate(frame)
            result = values.where(values.notna(), result)
        return result


@dataclass(frozen=True)
class Case(Expr):
    """Search-form `CASE WHEN c THEN t … ELSE e END`.

    Folded from the last branch backwards, each step selecting `then` where the condition
    holds and the accumulated result otherwise — the same shape as `build_column_case`'s
    `copy_if_else` fold in `cpp/src/expr.cpp`. Value-form CASE is deliberately absent: it
    is what #57 withheld on the GPU.
    """

    whens: tuple           # ((condition, then), …)
    otherwise: Expr

    def evaluate(self, frame: pd.DataFrame) -> pd.Series:
        result = self.otherwise.evaluate(frame)
        for condition, then in reversed(self.whens):
            mask = condition.evaluate(frame).fillna(False).astype(bool)
            result = then.evaluate(frame).where(mask, result)
        return result


@dataclass(frozen=True)
class Alias(Expr):
    """A projection's `expr AS name`."""

    inner: Expr
    as_name: str

    def evaluate(self, frame: pd.DataFrame) -> pd.Series:
        return self.inner.evaluate(frame)

    def name(self) -> str:
        return self.as_name


def columns_of(expr: Expr) -> list[str]:
    """Every column the expression reads, in first-seen order.

    What the wire calls a filter's intermediate schema: `filter_columns[i]` is the origin
    of `ColumnRef(i)`, and this is the list that map is built from (`recipe.JoinFilter`).
    """
    found: list[str] = []

    def walk(node):
        if isinstance(node, Col):
            if node.column not in found:
                found.append(node.column)
            return
        for value in vars(node).values():
            for item in value if isinstance(value, (list, tuple)) else (value,):
                if isinstance(item, Expr):
                    walk(item)
                elif isinstance(item, (list, tuple)):
                    for inner in item:
                        if isinstance(inner, Expr):
                            walk(inner)

    walk(expr)
    return found


def project(frame: pd.DataFrame, exprs: list[Expr]) -> pd.DataFrame:
    """Build a new frame from an expression list — column order is the list order."""
    # `.array`, not `.to_numpy()`: both drop the index, but only the array keeps a nullable
    # dtype — a comparison's `boolean` would come out object, holding NA no `&` accepts.
    # The index keeps the row count when the list is empty: `SELECT 1 FROM t` projects no
    # column of `t`, and the rows it counts must survive to the cross join above (tpcds q9).
    return pd.DataFrame({expr.name(): expr.evaluate(frame).array for expr in exprs},
                        index=pd.RangeIndex(len(frame)))
