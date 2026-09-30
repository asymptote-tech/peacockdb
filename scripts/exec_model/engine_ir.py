"""The engine's expression fields, translated into the prototype's expression IR.

Two things the engine's text leaves to its reader are settled here. A column is addressed by
ordinal in the engine and by name in the IR (`operators/expressions.py`), and one schema may
hold a name twice — a join's two sides both carry `d_date_sk` — so every frame names its
columns by `frame_names`, and a reference resolves through its ordinal, the printed name
serving as a check. And a literal is printed without its type — `13` is a number or a zip
code — so it takes the kind of what it meets: the operand across from it, the argument slot
it fills, the branch beside it, or the output column the whole expression lands in.
"""

from __future__ import annotations

import dataclasses
import datetime
import re
from collections import Counter
from dataclasses import dataclass
from enum import Enum

import pandas as pd

from . import engine_expr as E
from .engine_plan import EngineNode
from .errors import EnginePlanFormatError
from .operators import expressions as X
from .operators.join_types import JoinType
from .operators.joins import own_output


class Kind(Enum):
    """A type as far as the prototype tells types apart: decimals are float64 here
    (`tests/corpus.py`), and one integer width is as good as another."""

    BOOLEAN = "boolean"
    INTEGER = "integer"
    FLOAT = "float"
    STRING = "string"
    DATE = "date"
    INTERVAL = "interval"


#: The types the goldens' schemas and casts print, by `type_text` in `node_text.rs`.
_KINDS = {
    "Boolean": Kind.BOOLEAN,
    "Int32": Kind.INTEGER,
    "Int64": Kind.INTEGER,
    "UInt8": Kind.INTEGER,
    "Float64": Kind.FLOAT,
    "Utf8": Kind.STRING,
    "Date32": Kind.DATE,
}


def kind_of(type_text: str) -> Kind:
    if type_text.startswith("Decimal128("):
        return Kind.FLOAT
    if type_text not in _KINDS:
        raise EnginePlanFormatError(f"type `{type_text}` has no kind here")
    return _KINDS[type_text]


def frame_names(names: list[str]) -> list[str]:
    """The prototype frame's column names for an engine schema's: a name the schema holds
    once stays as it is, a repeated one becomes `name@ordinal`."""
    counts = Counter(names)
    framed = [f"{name}@{i}" if counts[name] > 1 else name for i, name in enumerate(names)]
    if len(set(framed)) != len(framed):
        raise EnginePlanFormatError(f"no unique frame names for {names}")
    return framed


@dataclass(frozen=True)
class Column:
    name: str  # the engine's
    kind: Kind | None  # None only for a state column whose aggregate nothing typed
    frame: str  # the prototype frame's


def columns(schema) -> tuple[Column, ...]:
    """An engine schema's (name, type) pairs as columns, framed together."""
    frames = frame_names([name for name, _ in schema])
    return tuple(Column(name, kind_of(type_text), frame) for (name, type_text), frame in zip(schema, frames))


#: The pandas dtype of a kind, as `tests/corpus.py` reads parquet into it.
_DTYPES = {
    Kind.BOOLEAN: "bool", Kind.INTEGER: "int64", Kind.FLOAT: "float64", Kind.STRING: "object",
    Kind.DATE: "datetime64[ns]",
}


def dtypes(schema) -> dict[str, str]:
    """An engine schema as `{frame name: dtype}`: the empty batch a single-batch node owes
    downstream when it accumulated nothing is typed by it."""
    return {column.frame: _DTYPES[column.kind] for column in columns(schema)}


def lower_field(node: EngineNode, field: str):
    """`node.fields[field]` in the IR:

    - `predicate`, `filter`: one `Expr`, a join's over its build and probe rows side by side;
    - `exprs`, `final`: one `Alias` per output column, named as the node's frame names it;
    - `group_by`: an `Expr` per key; `aggs`: each `AggCall` with its arguments lowered;
    - `by`: (`Col`, ascending, nulls_first) per key; `on`: (build `Col`, probe `Col`) per key;
    - `hash`, `projection`: a `Col` each, a join's projection over `join_output`.

    A join's keys and filter are named as its build and probe columns side by side, the one
    table a join executor renames its two inputs to.
    """
    try:
        return _FIELDS[field](node, node.fields[field])
    except EnginePlanFormatError as error:
        raise EnginePlanFormatError(f"{node.kind} {field}: {error}") from None


def _child(node):
    return _Lowering({None: columns(node.children[0].schema)})


def _joined(node) -> tuple[tuple[Column, ...], int]:
    """A join's build and probe columns in one table, as its keys and filter name them, and
    where the probe side starts."""
    build, probe = node.children
    return columns(build.schema + probe.schema), len(build.schema)


def _predicate(node, text):
    return _child(node).lower(E.parse_expr(text), Kind.BOOLEAN)


def _join_filter(node, text):
    joined, width = _joined(node)
    lowering = _Lowering({"build": joined[:width], "probe": joined[width:]})
    return lowering.lower(E.parse_expr(text), Kind.BOOLEAN)


def _projected(node, text):
    return _child(node).outputs(E.parse_named_exprs(text), columns(node.schema))


def _state_prefix(node) -> int:
    """How many state columns precede the aggregates' outputs: the keys, and with grouping
    sets the `__grouping_id` after them."""
    return len(E.parse_exprs(node.fields["group_by"])) + ("grouping_sets" in node.fields)


def state_columns(node) -> tuple[Column, ...]:
    """An aggregate's state: keys, the grouping id with grouping sets, then every call's
    outputs. Its schema where there is no `final`; where there is, no line prints it, so the
    schema's keys and the outputs typed by their functions."""
    if "final" not in node.fields:
        return columns(node.schema)
    keys = _state_prefix(node)
    child = _child(node)
    state = [(name, kind_of(type_text)) for name, type_text in node.schema[:keys]]
    for call in E.parse_agg_calls(node.fields["aggs"]):
        state += zip(call.outputs, _agg_kinds(call.func, [child.kind(arg) for arg in call.args]))
    frames = frame_names([name for name, _ in state])
    return tuple(Column(n, k, f) for (n, k), f in zip(state, frames))


def _finalized(node, text):
    lowering = _Lowering({None: state_columns(node)})
    return lowering.outputs(E.parse_named_exprs(text), columns(node.schema)[_state_prefix(node):])


def _agg_kinds(func, arg_kinds) -> tuple:
    if func == "count":
        return (Kind.INTEGER,)
    if func in ("sum", "min", "max"):
        return (arg_kinds[0],)
    if func in ("mean", "m2"):
        return (Kind.FLOAT,)
    return (Kind.INTEGER, Kind.FLOAT, Kind.FLOAT)  # merge_m2: count, mean, m2


def _group_by(node, text):
    lowering = _child(node)
    return [lowering.lower(key, None) for key in E.parse_exprs(text)]


def _aggs(node, text):
    lowering, calls = _child(node), E.parse_agg_calls(text)
    return [
        dataclasses.replace(call, args=tuple(lowering.lower(arg, kind) for arg in call.args))
        for call, kind in zip(calls, _agg_arg_kinds(node, calls))
    ]


def _agg_arg_kinds(node, calls) -> list:
    """What a literal argument is: `sum(CASE WHEN … THEN 1 ELSE 0 END)` has only its output
    to say, which the schema prints where there is no `final`: the state, as it is."""
    if "final" in node.fields:
        outputs = [None] * sum(len(call.outputs) for call in calls)
    else:
        state = columns(node.schema)[_state_prefix(node):]
        names = [name for call in calls for name in call.outputs]
        if [column.name for column in state] != names:
            raise EnginePlanFormatError(f"outputs {names} are not the schema's {[c.name for c in state]}")
        outputs = [column.kind for column in state]
    kinds, at = [], 0
    for call in calls:
        if call.func == "count":
            kinds.append(Kind.INTEGER)  # `count(1)`: what is counted has no type, and any will do
        else:
            kinds.append(outputs[at] if call.func in ("sum", "min", "max") else None)
        at += len(call.outputs)
    return kinds


def _orders(node, text):
    lowering = _child(node)
    return [
        (lowering.lower(key.column, None), key.ascending, key.nulls_first)
        for key in E.parse_column_orders(text)
    ]


def _join_keys(node, text):
    joined, width = _joined(node)
    lowering = _Lowering({"build": joined[:width], "probe": joined[width:]})
    return [
        (lowering.lower(dataclasses.replace(b, side="build"), None),
         lowering.lower(dataclasses.replace(p, side="probe"), None))
        for b, p in E.parse_join_keys(text)
    ]


def _columns(node, text):
    lowering = _Lowering({None: join_output(node)}) if len(node.children) == 2 else _child(node)
    return [lowering.lower(column, None) for column in E.parse_columns(text)]


#: `JoinType`'s Debug text in the engine, as `join_type=` prints it.
_JOIN_TYPES = {
    "Inner": JoinType.INNER, "Left": JoinType.LEFT, "Right": JoinType.RIGHT, "Full": JoinType.FULL,
    "LeftSemi": JoinType.LEFT_SEMI, "LeftAnti": JoinType.LEFT_ANTI, "LeftMark": JoinType.LEFT_MARK,
    "RightSemi": JoinType.RIGHT_SEMI, "RightAnti": JoinType.RIGHT_ANTI,
}


def join_type(node) -> JoinType:
    """A join node's type; a cross join has none and emits both sides, as an inner one does."""
    text = node.fields.get("join_type", "Inner")
    if text not in _JOIN_TYPES:
        raise EnginePlanFormatError(f"join type `{text}` is not taught")
    return _JOIN_TYPES[text]


def join_output(node) -> tuple[Column, ...]:
    """The columns a join emits before its projection (`joins.own_output`)."""
    build, probe = node.children
    both = build.schema + probe.schema + (("mark", "Boolean"),)
    positions = own_output(join_type(node), len(build.schema), len(probe.schema))
    return columns(tuple(both[-1] if p is None else both[p] for p in positions))


_FIELDS = {
    "predicate": _predicate,
    "filter": _join_filter,
    "exprs": _projected,
    "final": _finalized,
    "group_by": _group_by,
    "aggs": _aggs,
    "by": _orders,
    "on": _join_keys,
    "hash": _columns,
    "projection": _columns,
}

_COMPARISONS = {"=": "==", "!=": "!=", "<": "<", "<=": "<=", ">": ">", ">=": ">="}
_CONNECTIVES = {"AND": "and", "OR": "or"}
_ARITHMETIC = ("+", "-", "*", "/")
#: An integer column holding a null is float64 in pandas (`operators/frame.py`).
_NULLS = {
    Kind.BOOLEAN: None, Kind.INTEGER: float("nan"), Kind.FLOAT: float("nan"),
    Kind.STRING: None, Kind.DATE: pd.NaT, Kind.INTERVAL: pd.NaT,
}
_CAST_DTYPES = {Kind.INTEGER: "int64", Kind.FLOAT: "float64"}
_RETURNS = {
    "substr": Kind.STRING, "upper": Kind.STRING, "lower": Kind.STRING, "concat": Kind.STRING,
    "round": Kind.FLOAT, "sqrt": Kind.FLOAT, "date_part": Kind.INTEGER,
}


def _interval(text):
    # `interval_text` prints `N mons`, `N days`, `N nanos`; only days are a fixed length.
    match = re.fullmatch(r"(-?\d+) days", text)
    if match is None:
        raise ValueError(text)
    return pd.Timedelta(days=int(match.group(1)))


_VALUES = {
    Kind.BOOLEAN: {"true": True, "false": False}.__getitem__,
    Kind.INTEGER: int,
    Kind.FLOAT: float,
    Kind.STRING: str,
    Kind.DATE: lambda text: pd.Timestamp(datetime.date.fromisoformat(text)),
    Kind.INTERVAL: _interval,
}


class _Lowering:
    """Lowers expressions over one scope: per side (`None`, or "build"/"probe" in a join
    filter), the columns its ordinals index."""

    def __init__(self, scope: dict):
        self.scope = scope

    @staticmethod
    def fail(what):
        raise EnginePlanFormatError(what)

    def column(self, ref: E.ColumnRef) -> Column:
        side = self.scope.get(ref.side)
        if side is None:
            self.fail(f"`{ref.name}` is addressed {ref.side or 'without a side'}, which this field is not")
        if ref.index >= len(side):
            self.fail(f"`{ref.name}@{ref.index}` is past the {len(side)} columns")
        column = side[ref.index]
        if column.name != ref.name:
            self.fail(f"`{ref.name}@{ref.index}` is `{column.name}` there")
        return column

    def kind(self, expr) -> Kind | None:
        """What `expr` evaluates to, where something in it says; `None` for a bare literal."""
        if isinstance(expr, E.ColumnRef):
            return self.column(expr).kind
        if isinstance(expr, E.Binary):
            if expr.op not in _ARITHMETIC:
                return Kind.BOOLEAN
            kinds = (self.kind(expr.left), self.kind(expr.right))
            for wins in (Kind.DATE, Kind.FLOAT):
                if wins in kinds:
                    return wins
            return kinds[0] or kinds[1]
        if isinstance(expr, E.Unary):
            return self.kind(expr.arg) if expr.op == "-" else Kind.BOOLEAN
        if isinstance(expr, E.Like):
            return Kind.BOOLEAN
        if isinstance(expr, E.Cast):
            return kind_of(expr.target)
        if isinstance(expr, E.Case):
            return self.first_kind([then for _, then in expr.when_then] + [expr.otherwise])
        if isinstance(expr, E.ScalarFunction):
            return self.first_kind(expr.args) if expr.name == "coalesce" else _RETURNS.get(expr.name)
        return None

    def first_kind(self, exprs) -> Kind | None:
        return next((k for k in map(self.kind, exprs) if k is not None), None)

    def lower(self, expr, expected: Kind | None) -> X.Expr:
        if isinstance(expr, E.ColumnRef):
            return X.Col(self.column(expr).frame)
        if isinstance(expr, E.Literal):
            return self.literal(expr.text, expected)
        if isinstance(expr, E.Binary):
            return self.binary(expr, expected)
        if isinstance(expr, E.Unary):
            return self.unary(expr)
        if isinstance(expr, E.Cast):
            return self.cast(expr)
        if isinstance(expr, E.Like):
            return self.like(expr)
        if isinstance(expr, E.Case):
            return self.case(expr, expected)
        return self.function(expr, expected)

    def outputs(self, named, outputs: tuple[Column, ...]) -> list[X.Alias]:
        """Named expressions, one per output column, each checked against the column it
        fills and typed by it."""
        if len(named) != len(outputs):
            self.fail(f"{len(named)} expressions for {len(outputs)} output columns")
        lowered = []
        for (expr, alias), column in zip(named, outputs):
            name = expr.name if alias is None and isinstance(expr, E.ColumnRef) else alias
            if name != column.name:
                self.fail(f"output `{name}` lands in `{column.name}`")
            if self.kind(expr) not in (None, column.kind):
                self.fail(f"`{name}` is {self.kind(expr).value}, its column {column.kind.value}")
            lowered.append(X.Alias(self.lower(expr, column.kind), column.frame))
        return lowered

    def literal(self, text: str, kind: Kind | None) -> X.Lit:
        if kind is None:
            self.fail(f"literal `{text}` has nothing beside it to say its type")
        if text == "NULL":
            return X.Lit(_NULLS[kind])
        try:
            return X.Lit(_VALUES[kind](text))
        except (ValueError, KeyError):
            self.fail(f"literal `{text}` does not read as {kind.value}")

    def integer(self, expr) -> int:
        if not isinstance(expr, E.Literal):
            self.fail(f"`{expr}` where the IR takes a literal integer")
        return self.literal(expr.text, Kind.INTEGER).value

    def binary(self, expr: E.Binary, expected):
        if expr.op in _CONNECTIVES:
            return self.connective(expr)
        left, right = self.kind(expr.left), self.kind(expr.right)
        if expr.op in _COMPARISONS:
            kind = left or right
            return X.Binary(_COMPARISONS[expr.op], self.lower(expr.left, kind), self.lower(expr.right, kind))
        if expr.op not in _ARITHMETIC:
            self.fail(f"`{expr.op}` has no IR counterpart")
        if expr.op == "/" and left is right is Kind.INTEGER:
            self.fail("integer division, which the IR's `/` does not do")
        return X.Binary(
            expr.op,
            self.lower(expr.left, _operand_kind(left, right, expected)),
            self.lower(expr.right, _operand_kind(right, left, expected)),
        )

    def connective(self, expr: E.Binary):
        # An IN list is a chain of hundreds (tpcds q8), past Python's recursion limit when
        # lowered recursively; AND and OR are associative, so it is flattened and refolded.
        operands, pending = [], [expr]
        while pending:
            item = pending.pop()
            if isinstance(item, E.Binary) and item.op == expr.op:
                pending += [item.right, item.left]
            else:
                operands.append(self.lower(item, Kind.BOOLEAN))
        folded = operands[0]
        for operand in operands[1:]:
            folded = X.Binary(_CONNECTIVES[expr.op], folded, operand)
        return folded

    def unary(self, expr: E.Unary):
        if expr.op == "NOT":
            return X.Not(self.lower(expr.arg, Kind.BOOLEAN))
        if expr.op == "IS NULL":
            return X.IsNull(self.lower(expr.arg, self.kind(expr.arg)))
        if expr.op == "IS NOT NULL":
            return X.IsNotNull(self.lower(expr.arg, self.kind(expr.arg)))
        self.fail(f"unary `{expr.op}` has no IR counterpart")

    def cast(self, expr: E.Cast):
        target = kind_of(expr.target)
        if target not in _CAST_DTYPES:
            self.fail(f"a cast to {expr.target}, which the IR is not taught")
        return X.Cast(self.lower(expr.expr, self.kind(expr.expr)), _CAST_DTYPES[target])

    def like(self, expr: E.Like):
        if expr.case_insensitive or not isinstance(expr.pattern, E.Literal):
            self.fail("ILIKE, or LIKE against a computed pattern, has no IR counterpart")
        return X.Like(self.lower(expr.expr, Kind.STRING), expr.pattern.text, expr.negated)

    def case(self, expr: E.Case, expected):
        kind = self.kind(expr) or expected
        if expr.comparand is None:
            whens = tuple((self.lower(w, Kind.BOOLEAN), self.lower(t, kind)) for w, t in expr.when_then)
        else:
            # `CASE x WHEN v` is `CASE WHEN x = v`, the form the IR has.
            compared = self.kind(expr.comparand) or self.first_kind([w for w, _ in expr.when_then])
            subject = self.lower(expr.comparand, compared)
            whens = tuple(
                (X.Binary("==", subject, self.lower(w, compared)), self.lower(t, kind))
                for w, t in expr.when_then
            )
        if expr.otherwise is None:
            return X.Case(whens, self.literal("NULL", kind))
        return X.Case(whens, self.lower(expr.otherwise, kind))

    def function(self, expr: E.ScalarFunction, expected):
        name, args = expr.name, expr.args
        if name == "substr" and len(args) == 3:
            return X.Substring(self.lower(args[0], Kind.STRING), self.integer(args[1]), self.integer(args[2]))
        if name == "round" and len(args) in (1, 2):
            places = self.integer(args[1]) if len(args) == 2 else 0
            return X.Round(self.lower(args[0], Kind.FLOAT), places)
        if name == "date_part" and len(args) == 2 and isinstance(args[0], E.Literal):
            return X.DatePart(args[0].text, self.lower(args[1], Kind.DATE))
        if name in ("upper", "lower") and len(args) == 1:
            return (X.Upper if name == "upper" else X.Lower)(self.lower(args[0], Kind.STRING))
        if name == "sqrt" and len(args) == 1:
            return X.Sqrt(self.lower(args[0], Kind.FLOAT))
        if name == "concat":
            return X.Concat(tuple(self.lower(arg, Kind.STRING) for arg in args))
        if name == "coalesce":
            kind = self.first_kind(args) or expected
            return X.Coalesce(tuple(self.lower(arg, kind) for arg in args))
        self.fail(f"`{name}` with {len(args)} arguments has no IR counterpart")


def _operand_kind(own: Kind | None, other: Kind | None, expected: Kind | None) -> Kind | None:
    # A literal added to or taken from a date is an interval.
    if own is not None:
        return own
    if other is Kind.DATE:
        return Kind.INTERVAL
    return other or expected
