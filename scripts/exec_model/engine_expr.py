"""The engine's expression text, parsed back into a syntax tree.

`peacockdb-core/src/plan_text/{expr_text,node_text}.rs` render so that the text reads back
unambiguously: a column is `name@ordinal` (`name@build:ordinal`, `name@probe:ordinal` in a join
filter); a name or literal holding whitespace, `,`, `[`, `]`, `@` or a backtick is backticked,
backticks doubled; a binary or CASE operand is always parenthesized, so a level holds at most one
binary operator. This inverts that rendering and stops at syntax: a literal stays the text it
was printed as, because the rendering does not print its type.
"""

from __future__ import annotations

from dataclasses import dataclass

from .errors import EnginePlanFormatError

#: `binary_op_text` in `expr_text.rs`; rendered with one space on each side.
BINARY_OPS = (
    "=", "!=", "<", "<=", ">", ">=", "+", "-", "*", "/", "%", "AND", "OR", "&", "|", "^", "<<",
    ">>", "||", "IS DISTINCT FROM", "IS NOT DISTINCT FROM",
)
#: `plan_agg_name` in `node_text.rs`.
AGGREGATES = frozenset({"sum", "min", "max", "count", "mean", "m2", "merge_m2"})
_LIKES = (("NOT ILIKE", True, True), ("NOT LIKE", True, False), ("ILIKE", False, True), ("LIKE", False, False))
_STOP = frozenset(" \t\n,[]@`")  # what `quoted` backticks a name for
_OPEN, _CLOSE = "([{", ")]}"


@dataclass(frozen=True)
class ColumnRef:
    name: str
    index: int
    side: str | None = None  # "build" or "probe" in a join filter, else None


@dataclass(frozen=True)
class Literal:
    text: str


@dataclass(frozen=True)
class Binary:
    left: object
    op: str
    right: object


@dataclass(frozen=True)
class Unary:
    op: str  # "NOT", "-", "IS NULL", "IS NOT NULL"; the engine's Sqrt renders as a call
    arg: object


@dataclass(frozen=True)
class Cast:
    expr: object
    target: str


@dataclass(frozen=True)
class Like:
    expr: object
    pattern: object
    negated: bool
    case_insensitive: bool


@dataclass(frozen=True)
class Case:
    comparand: object | None
    when_then: tuple[tuple[object, object], ...]
    otherwise: object | None


@dataclass(frozen=True)
class ScalarFunction:
    name: str
    args: tuple


@dataclass(frozen=True)
class AggCall:
    func: str
    args: tuple
    outputs: tuple[str, ...]


@dataclass(frozen=True)
class ColumnOrder:
    column: ColumnRef
    ascending: bool
    nulls_first: bool


def parse_expr(text: str):
    """`predicate=`, and a join's `filter=`."""
    return _whole(text, _Parser.expr)


def parse_exprs(text: str) -> list:
    """`group_by=`."""
    return _whole(text, lambda p: p.listing(p.expr))


def parse_named_exprs(text: str) -> list[tuple[object, str | None]]:
    """`exprs=`, `final=`: an expression, and its name where it is not the column's own."""
    return _whole(text, lambda p: p.listing(p.named_expr))


def parse_agg_calls(text: str) -> list[AggCall]:
    """`aggs=`."""
    return _whole(text, lambda p: p.listing(p.agg_call))


def parse_column_orders(text: str) -> list[ColumnOrder]:
    """`by=`, `sorted_on=`."""
    return _whole(text, lambda p: p.listing(p.column_order))


def parse_columns(text: str) -> list[ColumnRef]:
    """`hash=`, `hashed_on=`, `projection=`, `projections=`."""
    return _whole(text, lambda p: p.listing(p.column_ref))


def parse_join_keys(text: str) -> list[tuple[ColumnRef, ColumnRef]]:
    """`on=`: (build, probe) per key."""
    return _whole(text, lambda p: p.listing(p.join_key))


def parse_grouping_sets(text: str) -> list[list]:
    """`grouping_sets=`: per set, the keys it groups on."""
    return _whole(text, lambda p: p.listing(lambda: p.listing(p.expr)))


def expr_text(expr) -> str:
    """An expression as `expr_text.rs` prints it — `parse_expr`'s inverse. A sub-expression
    that is itself an operator is parenthesized; a left-deep chain — an IN list is 399 ORs in
    tpcds q8 — is walked, not recursed."""
    if isinstance(expr, Binary):
        spine = []
        while isinstance(expr, Binary):
            spine.append(expr)
            expr = expr.left
        text = _nested(expr)
        for i, node in enumerate(reversed(spine)):
            text = f"{text if i == 0 else '(' + text + ')'} {node.op} {_nested(node.right)}"
        return text
    if isinstance(expr, ColumnRef):
        return f"{quoted(expr.name)}@{expr.side + ':' if expr.side else ''}{expr.index}"
    if isinstance(expr, Literal):
        return quoted(expr.text)
    if isinstance(expr, Unary):
        if expr.op in ("NOT", "-"):
            return f"{expr.op}{' ' if expr.op == 'NOT' else ''}{_nested(expr.arg)}"
        return f"{_nested(expr.arg)} {expr.op}"
    if isinstance(expr, Cast):
        return f"CAST({expr_text(expr.expr)} AS {expr.target})"
    if isinstance(expr, Like):
        op = ("NOT " if expr.negated else "") + ("ILIKE" if expr.case_insensitive else "LIKE")
        return f"{_nested(expr.expr)} {op} {_nested(expr.pattern)}"
    if isinstance(expr, Case):
        text = "CASE" + (f" {expr_text(expr.comparand)}" if expr.comparand is not None else "")
        for when, then in expr.when_then:
            text += f" WHEN {expr_text(when)} THEN {expr_text(then)}"
        return text + (f" ELSE {expr_text(expr.otherwise)}" if expr.otherwise is not None else "") + " END"
    if isinstance(expr, ScalarFunction):
        return f"{expr.name}({', '.join(expr_text(a) for a in expr.args)})"
    raise TypeError(f"no text for {expr!r}")


def _nested(expr) -> str:
    return f"({expr_text(expr)})" if isinstance(expr, (Binary, Case)) else expr_text(expr)


def quoted(name: str) -> str:
    """A name or literal as `quoted` in `node_text.rs` prints it — `parse_name`'s inverse."""
    plain = name and not any(c.isspace() or c in ",[]@`" for c in name)
    return name if plain else "`" + name.replace("`", "``") + "`"


def parse_name(text: str) -> str:
    """One name as `quoted` printed it: a schema entry's, an alias."""
    return _whole(text, _Parser.name)


def _whole(text, read):
    parser = _Parser(text)
    value = read(parser)
    if parser.pos != len(text):
        parser.fail("text after the end")
    return value


class _Parser:
    def __init__(self, text: str):
        self.text, self.pos = text, 0

    def fail(self, what):
        raise EnginePlanFormatError(f"{what}, at {self.pos} of `{self.text}`")

    def take(self, token: str) -> bool:
        if self.text.startswith(token, self.pos):
            self.pos += len(token)
            return True
        return False

    def expect(self, token: str) -> None:
        if not self.take(token):
            self.fail(f"expected `{token}`")

    def listing(self, item) -> list:
        self.expect("[")
        items = []
        if not self.take("]"):
            items.append(item())
            while self.take(", "):
                items.append(item())
            self.expect("]")
        return items

    def expr(self):
        return self.rest_of_expr(self.operand())

    def rest_of_expr(self, left):
        op = self.binary_op()
        if op is None:
            return left
        right = self.operand()
        if self.binary_op(peek=True):
            self.fail("a second binary operator on one level, which the rendering parenthesizes")
        return Binary(left, op, right)

    def binary_op(self, peek=False):
        for op in BINARY_OPS:
            if self.text.startswith(f" {op} ", self.pos):
                if not peek:
                    self.pos += len(op) + 2
                return op
        return None

    def operand(self):
        # `NOT x IS NULL` is what both Not(IsNull(x)) and IsNull(Not(x)) render as; it is read
        # as SQL reads it, NOT looser than IS NULL and LIKE.
        if self.take("NOT "):
            return Unary("NOT", self.operand())
        return self.postfix(self.unary())

    def postfix(self, value):
        while True:
            if self.take(" IS NOT NULL"):
                value = Unary("IS NOT NULL", value)
            elif self.take(" IS NULL"):
                value = Unary("IS NULL", value)
            else:
                like = self.like_op()
                if like is None:
                    return value
                value = Like(value, self.unary(), *like)

    def like_op(self) -> tuple[bool, bool] | None:
        for text, negated, case_insensitive in _LIKES:
            if self.take(f" {text} "):
                return negated, case_insensitive
        return None

    def unary(self):
        # A negative number prints as one token, `-5`; a negated expression as `-` before it.
        if self.text.startswith("-", self.pos) and not self.text[self.pos + 1 : self.pos + 2].isdigit():
            self.pos += 1
            return Unary("-", self.unary())
        return self.primary()

    def primary(self):
        if self.text.startswith("(", self.pos):
            return self.parenthesized()
        if self.take("CAST("):
            inner = self.expr()
            self.expect(" AS ")
            target = self.bracketed_run()
            self.expect(")")
            return Cast(inner, target)
        if self.text.startswith("CASE ", self.pos):
            return self.case()
        if self.text.startswith("`", self.pos):
            return self.column_or_literal(self.quoted_token())
        start = self.pos
        run, balanced = self.plain_run()
        if balanced:
            return self.column_or_literal(run)
        # `count(*)@1` is a column named `count(*)`; `sum(x@1)` is a call. A name reaches the
        # `@` with its parentheses closed, a call's first argument does not.
        name = run[: run.index("(")]
        if not name.isidentifier():
            self.fail(f"`{run}` is neither a name nor a call")
        self.pos = start + len(name) + 1
        return ScalarFunction(name, tuple(self.arguments()))

    def parenthesized(self):
        # An IN list renders as a left-deep OR chain, one `(` per element — 399 in tpcds q8 —
        # deeper than Python recurses, so a run of `(` is unwound here instead.
        depth = 0
        while self.take("("):
            depth += 1
        value = self.expr()
        self.expect(")")
        for _ in range(depth - 1):
            value = self.rest_of_expr(self.postfix(value))
            self.expect(")")
        return value

    def arguments(self) -> list:
        args = []
        if not self.take(")"):
            args.append(self.expr())
            while self.take(", "):
                args.append(self.expr())
            self.expect(")")
        return args

    def case(self):
        self.expect("CASE")
        comparand = None
        if not self.text.startswith(" WHEN ", self.pos):
            self.expect(" ")
            comparand = self.expr()
        pairs = []
        while self.take(" WHEN "):
            when = self.expr()
            self.expect(" THEN ")
            pairs.append((when, self.expr()))
        otherwise = self.expr() if self.take(" ELSE ") else None
        self.expect(" END")
        return Case(comparand, tuple(pairs), otherwise)

    def column_or_literal(self, name: str):
        if not self.take("@"):
            return Literal(name)
        side = next((s for s in ("build", "probe") if self.take(f"{s}:")), None)
        start = self.pos
        while self.pos < len(self.text) and self.text[self.pos].isdigit():
            self.pos += 1
        if start == self.pos:
            self.fail("expected an ordinal after `@`")
        return ColumnRef(name, int(self.text[start : self.pos]), side)

    def column_ref(self) -> ColumnRef:
        name = self.quoted_token() if self.text.startswith("`", self.pos) else self.name_run()
        column = self.column_or_literal(name)
        if not isinstance(column, ColumnRef):
            self.fail("expected `name@ordinal`")
        return column

    def named_expr(self):
        value = self.expr()
        return value, (self.name() if self.take(" as ") else None)

    def agg_call(self) -> AggCall:
        open_at = self.text.find("(", self.pos)
        func = self.text[self.pos : open_at] if open_at >= 0 else ""
        if func not in AGGREGATES:
            self.fail(f"unknown aggregate `{func}`")
        self.pos = open_at + 1
        args = tuple(self.arguments())
        self.expect(" as ")
        outputs = self.listing(self.name) if self.text.startswith("[", self.pos) else [self.name()]
        return AggCall(func, args, tuple(outputs))

    def column_order(self) -> ColumnOrder:
        column = self.column_ref()
        return ColumnOrder(column, self.either(" asc", " desc"), self.either(" nulls_first", " nulls_last"))

    def either(self, yes: str, no: str) -> bool:
        if self.take(yes):
            return True
        self.expect(no)
        return False

    def join_key(self) -> tuple[ColumnRef, ColumnRef]:
        self.expect("(")
        build = self.column_ref()
        self.expect(", ")
        probe = self.column_ref()
        self.expect(")")
        return build, probe

    def name(self) -> str:
        return self.quoted_token() if self.text.startswith("`", self.pos) else self.name_run()

    def name_run(self) -> str:
        run, balanced = self.plain_run()
        if not run or not balanced:
            self.fail("expected a name")
        return run

    def plain_run(self) -> tuple[str, bool]:
        """The unquoted token here, and whether its parentheses close. A `)` it never opened
        belongs to an enclosing call and ends it."""
        start, depth = self.pos, 0
        while self.pos < len(self.text) and self.text[self.pos] not in _STOP:
            ch = self.text[self.pos]
            if ch == "(":
                depth += 1
            elif ch == ")":
                if depth == 0:
                    break
                depth -= 1
            self.pos += 1
        if start == self.pos:
            self.fail("expected an expression")
        return self.text[start : self.pos], depth == 0

    def quoted_token(self) -> str:
        self.expect("`")
        parts = []
        while True:
            end = self.text.find("`", self.pos)
            if end < 0:
                self.fail("unterminated backtick")
            parts.append(self.text[self.pos : end])
            self.pos = end + 1
            if not self.take("`"):
                return "".join(parts)
            parts.append("`")

    def bracketed_run(self) -> str:
        """A type up to the `)` that closes the CAST: `Decimal128(30,15)`, `Timestamp(ns, None)`."""
        start, depth = self.pos, 0
        while self.pos < len(self.text):
            ch = self.text[self.pos]
            if ch in _OPEN:
                depth += 1
            elif ch in _CLOSE:
                if depth == 0:
                    return self.text[start : self.pos]
                depth -= 1
            self.pos += 1
        self.fail("CAST without its closing `)`")
