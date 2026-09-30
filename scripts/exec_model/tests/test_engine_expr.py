"""The engine's expression text parsed back into a syntax tree."""

from __future__ import annotations

if __package__ in (None, ""):  # allow `python scripts/exec_model/tests/<file>.py`
    import pathlib as _pathlib, sys as _sys

    _sys.path.insert(0, str(_pathlib.Path(__file__).resolve().parents[3]))
    __package__ = "scripts.exec_model.tests"

import pathlib

from .harness import main, raises
from ..engine_expr import (
    AggCall, Binary, Case, Cast, ColumnOrder, ColumnRef, Like, Literal, ScalarFunction, Unary,
    parse_agg_calls, parse_column_orders, parse_columns, parse_expr, parse_exprs,
    expr_text, parse_grouping_sets, parse_join_keys, parse_named_exprs, quoted,
)
from ..engine_plan import EngineNode, read_plans
from ..errors import EnginePlanFormatError

GOLDENS = pathlib.Path(__file__).resolve().parents[3] / "testdata" / "goldens"


# -- the engine's list fields, around `expr_text` (`plan_text/node_text.rs`) --------------------


def listed(items, one):
    return "[" + ", ".join(one(item) for item in items) + "]"


def named(item):
    expr, name = item
    return expr_text(expr) + (f" as {quoted(name)}" if name is not None else "")


def agg(call):
    outputs = quoted(call.outputs[0]) if len(call.outputs) == 1 else listed(call.outputs, quoted)
    return f"{call.func}({', '.join(expr_text(arg) for arg in call.args)}) as {outputs}"


def order(key):
    return (f"{expr_text(key.column)} {'asc' if key.ascending else 'desc'} "
            f"{'nulls_first' if key.nulls_first else 'nulls_last'}")


#: field → (parse, render back)
FIELDS = {
    "predicate": (parse_expr, expr_text),
    "filter": (parse_expr, expr_text),
    "exprs": (parse_named_exprs, lambda v: listed(v, named)),
    "final": (parse_named_exprs, lambda v: listed(v, named)),
    "group_by": (parse_exprs, lambda v: listed(v, expr_text)),
    "grouping_sets": (parse_grouping_sets, lambda v: listed(v, lambda s: listed(s, expr_text))),
    "aggs": (parse_agg_calls, lambda v: listed(v, agg)),
    "by": (parse_column_orders, lambda v: listed(v, order)),
    "sorted_on": (parse_column_orders, lambda v: listed(v, order)),
    "on": (parse_join_keys, lambda v: listed(v, lambda k: f"({expr_text(k[0])}, {expr_text(k[1])})")),
    "hash": (parse_columns, lambda v: listed(v, expr_text)),
    "hashed_on": (parse_columns, lambda v: listed(v, expr_text)),
    "projection": (parse_columns, lambda v: listed(v, expr_text)),
    "projections": (parse_columns, lambda v: listed(v, expr_text)),
}


def walk(node):
    yield node
    for child in node.children:
        yield from walk(child)


def corpus_fields():
    paths = sorted(GOLDENS.glob("*/*.plans.txt"))
    assert {path.parent.name for path in paths} >= {"tpch.sf1", "tpcds.sf1"}, paths
    for path in paths:
        for name, plan in read_plans(path).items():
            if isinstance(plan, EngineNode):
                for node in walk(plan):
                    for field, text in node.fields.items():
                        if field in FIELDS:
                            yield path, name, node.kind, field, text


def test_every_expression_field_in_the_goldens_renders_back_to_itself():
    seen = set()
    for path, name, kind, field, text in corpus_fields():
        parse, back = FIELDS[field]
        assert back(parse(text)) == text, (path.parent.name, path.name, name, kind, field, text)
        seen.add(field)
    # A field the corpus never exercised here is a claim nothing checked.
    assert seen == set(FIELDS), set(FIELDS) - seen


# -- what the tree says, on shapes the goldens hold ---------------------------------------------


def test_one_binary_operator_per_level_with_operands_parenthesized():
    expr = parse_expr("(l_shipdate@3 >= 1996-01-01) AND (l_shipdate@3 < 1996-04-01)")
    assert expr == Binary(
        Binary(ColumnRef("l_shipdate", 3), ">=", Literal("1996-01-01")),
        "AND",
        Binary(ColumnRef("l_shipdate", 3), "<", Literal("1996-04-01")),
    )


def test_a_name_with_closed_parentheses_is_a_column_and_an_open_one_is_a_call():
    assert parse_expr("count(*)@1") == ColumnRef("count(*)", 1)
    [call] = parse_agg_calls("[sum(sum(l.l_quantity)@1) as sum(l.l_quantity)]")
    assert call == AggCall("sum", (ColumnRef("sum(l.l_quantity)", 1),), ("sum(l.l_quantity)",))
    assert parse_expr("substr(c_phone@4, 1, 2)") == ScalarFunction(
        "substr", (ColumnRef("c_phone", 4), Literal("1"), Literal("2"))
    )


def test_backticked_names_and_literals_come_back_unquoted():
    assert parse_expr("p_container@build:3 = `SM CASE`") == Binary(
        ColumnRef("p_container", 3, "build"), "=", Literal("SM CASE")
    )
    assert parse_expr("`a``b`@0") == ColumnRef("a`b", 0)
    assert parse_expr("o_orderdate@build:2 + `90 days`") == Binary(
        ColumnRef("o_orderdate", 2, "build"), "+", Literal("90 days")
    )


def test_cast_case_like_and_unary_forms():
    assert parse_expr("CAST(l_quantity@build:0 AS Decimal128(30,15))") == Cast(
        ColumnRef("l_quantity", 0, "build"), "Decimal128(30,15)"
    )
    assert parse_expr("CASE WHEN x@0 = 1 THEN y@1 ELSE 0 END") == Case(
        None, ((Binary(ColumnRef("x", 0), "=", Literal("1")), ColumnRef("y", 1)),), Literal("0")
    )
    assert parse_expr("p_type@1 NOT LIKE `%BRASS`") == Like(
        ColumnRef("p_type", 1), Literal("%BRASS"), True, False
    )
    assert parse_expr("NOT x@0 IS NULL") == Unary("NOT", Unary("IS NULL", ColumnRef("x", 0)))
    assert parse_expr("-5") == Literal("-5")
    assert parse_expr("-x@0") == Unary("-", ColumnRef("x", 0))


def test_list_fields():
    assert parse_named_exprs("[c_nationkey@0, count(*)@1 as cnt]") == [
        (ColumnRef("c_nationkey", 0), None), (ColumnRef("count(*)", 1), "cnt"),
    ]
    assert parse_column_orders("[revenue@2 desc nulls_first]") == [
        ColumnOrder(ColumnRef("revenue", 2), False, True)
    ]
    assert parse_join_keys("[(c_custkey@0, o_custkey@1)]") == [
        (ColumnRef("c_custkey", 0), ColumnRef("o_custkey", 1))
    ]
    assert parse_exprs("[]") == []


def test_text_the_rendering_cannot_produce_is_refused():
    cases = {
        "a@0 = b@1 AND c@2": r"a second binary operator",
        "`open": r"unterminated backtick",
        "x@": r"expected an ordinal",
        "(a@0 = b@1": r"expected `\)`",
        "a@0 = b@1 trailing": r"text after the end",
        "CAST(x@0 AS Int64": r"CAST without its closing",
    }
    for text, message in cases.items():
        with raises(EnginePlanFormatError, match=message):
            parse_expr(text)
    with raises(EnginePlanFormatError, match=r"unknown aggregate `avg`"):
        parse_agg_calls("[avg(x@0) as a]")
    with raises(EnginePlanFormatError, match=r"expected ` desc`"):
        parse_column_orders("[x@0 up nulls_last]")


if __name__ == "__main__":
    raise SystemExit(main(globals()))
