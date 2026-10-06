"""The engine's expression fields translated into the prototype's expression IR."""

from __future__ import annotations

if __package__ in (None, ""):  # allow `python scripts/exec_model/tests/plans/<file>.py`
    import pathlib as _pathlib, sys as _sys

    _sys.path.insert(0, str(_pathlib.Path(__file__).resolve().parents[4]))
    __package__ = "scripts.exec_model.tests.plans"

import pathlib

import pandas as pd

from ..harness import main, raises
from ...plans.engine_expr import AggCall
from ...plans.engine_ir import Kind, columns, frame_names, lower_field
from ...plans.engine_plan import EngineNode, read_plans
from ...errors import EnginePlanFormatError
from ...operators import expressions as X

GOLDENS = pathlib.Path(__file__).resolve().parents[4] / "testdata" / "goldens"
FIELDS = ("predicate", "filter", "exprs", "final", "group_by", "aggs", "by", "on", "hash", "projection")


def walk(node):
    yield node
    for child in node.children:
        yield from walk(child)


def corpus_nodes():
    paths = sorted(GOLDENS.glob("*/*.plans.txt"))
    assert {path.parent.name for path in paths} >= {"tpch.sf1", "tpcds.sf1"}, paths
    for path in paths:
        for name, plan in read_plans(path).items():
            if isinstance(plan, EngineNode):
                for node in walk(plan):
                    yield path, name, node


def test_every_expression_field_in_the_goldens_lowers():
    seen = set()
    for path, name, node in corpus_nodes():
        for field in FIELDS:
            if field in node.fields:
                lower_field(node, field)
                seen.add(field)
    assert seen == set(FIELDS), set(FIELDS) - seen


# -- synthetic nodes, one rule each ------------------------------------------------------------


def scan(*schema):
    return EngineNode("GpuLoadParquet", {}, tuple(schema))


def node(kind, children, schema=(), **fields):
    return EngineNode(kind, fields, tuple(schema), list(children))


def predicate(text, *schema):
    return lower_field(node("GpuFilter", [scan(*schema)], predicate=text), "predicate")


def test_a_literal_takes_the_kind_of_the_operand_across_from_it():
    zip_code = predicate("substr(ca_zip@0, 1, 5) = 13", ("ca_zip", "Utf8"))
    assert zip_code.right == X.Lit("13")
    assert predicate("ss_quantity@0 > 13", ("ss_quantity", "Int32")).right == X.Lit(13)
    assert predicate("l_discount@0 >= 0.05", ("l_discount", "Decimal128(15,2)")).right == X.Lit(0.05)
    day = predicate("d_date@0 >= 1996-01-01", ("d_date", "Date32"))
    assert day.right == X.Lit(pd.Timestamp("1996-01-01"))
    shifted = predicate("(d_date@0 + `90 days`) > d_date@0", ("d_date", "Date32"))
    assert shifted.left.right == X.Lit(pd.Timedelta(days=90))


def test_a_literal_with_only_literals_beside_it_takes_its_output_column():
    project = node(
        "GpuProject", [scan(("d", "Int64"))], schema=[("bucket", "Utf8")],
        exprs="[CASE WHEN d@0 <= 30 THEN `30 days` ELSE `31-60 days` END as bucket]",
    )
    [bucket] = lower_field(project, "exprs")
    assert bucket == X.Alias(
        X.Case(((X.Binary("<=", X.Col("d"), X.Lit(30)), X.Lit("30 days")),), X.Lit("31-60 days")), "bucket"
    )
    aggregate = node(
        "GpuAggregate", [scan(("k", "Int64"), ("d", "Int64"))],
        schema=[("k", "Int64"), ("late", "Int64")],
        group_by="[k@0]", aggs="[sum(CASE WHEN d@1 > 30 THEN 1 ELSE 0 END) as late]",
    )
    [late] = lower_field(aggregate, "aggs")
    assert late.args[0].whens[0][1] == X.Lit(1) and late.args[0].otherwise == X.Lit(0)


def test_lowered_expressions_evaluate_over_a_frame_named_by_frame_names():
    frame = pd.DataFrame({
        "ca_zip": ["13000", "85669", None],
        "d_date": pd.to_datetime(["1999-02-20", "1999-03-01", "1999-02-22"]),
    })
    lowered = predicate(
        "((substr(ca_zip@0, 1, 2) = 13) OR (ca_zip@0 IS NULL)) AND ((d_date@1 + `5 days`) < 1999-03-01)",
        ("ca_zip", "Utf8"), ("d_date", "Date32"),
    )
    assert lowered.evaluate(frame).fillna(False).tolist() == [True, False, True]


def test_a_repeated_name_is_framed_by_ordinal_and_resolved_through_it():
    assert frame_names(["a", "b", "a"]) == ["a@0", "b", "a@2"]
    both = (("d_date_sk", "Int64"), ("x", "Int64"), ("d_date_sk", "Int64"))
    assert predicate("d_date_sk@2 = 1", *both) == X.Binary("==", X.Col("d_date_sk@2"), X.Lit(1))
    with raises(EnginePlanFormatError, match=r"`x@0` is `d_date_sk` there"):
        predicate("x@0 = 1", *both)


def test_a_join_filter_and_projection_index_build_and_probe_side_by_side():
    join = node(
        "GpuHashJoin", [scan(("k", "Int64"), ("v", "Int64")), scan(("k", "Int64"), ("w", "Int64"))],
        on="[(k@0, k@0)]", filter="k@build:0 < k@probe:0", projection="[k@0, w@3]",
    )
    # Keys and filter name both sides as one table, where `k` is two columns; the
    # projection indexes the join type's own output, here the same two sides.
    assert lower_field(join, "on") == [(X.Col("k@0"), X.Col("k@2"))]
    assert lower_field(join, "filter") == X.Binary("<", X.Col("k@0"), X.Col("k@2"))
    assert lower_field(join, "projection") == [X.Col("k@0"), X.Col("w")]


def test_case_forms_the_ir_lacks_are_rewritten_into_the_one_it_has():
    project = node(
        "GpuProject", [scan(("n", "Int64"), ("r", "Float64"))], schema=[("ratio", "Float64")],
        exprs="[CASE n@0 WHEN 0 THEN NULL ELSE r@1 END as ratio]",
    )
    [ratio] = lower_field(project, "exprs")
    [(when, then)] = ratio.inner.whens
    assert when == X.Binary("==", X.Col("n"), X.Lit(0)) and pd.isna(then.value)
    no_else = predicate("CASE WHEN n@0 > 1 THEN true END", ("n", "Int64"))
    assert no_else.otherwise == X.Lit(None)


def test_final_reads_the_aggregate_state_after_the_keys():
    aggregate = node(
        "GpuAggregateBatches", [scan(("k", "Utf8"), ("v", "Decimal128(15,2)"))],
        schema=[("k", "Utf8"), ("avg(v)", "Float64")],
        group_by="[k@0]", aggs="[sum(v@1) as avg(v)$sum, count(v@1) as avg(v)$count]",
        final="[avg(v)$sum@1 / CAST(avg(v)$count@2 AS Float64) as avg(v)]",
    )
    [average] = lower_field(aggregate, "final")
    assert average == X.Alias(
        X.Binary("/", X.Col("avg(v)$sum"), X.Cast(X.Col("avg(v)$count"), "float64")), "avg(v)"
    )
    assert lower_field(aggregate, "aggs")[1] == AggCall("count", (X.Col("v"),), ("avg(v)$count",))


def test_what_the_ir_cannot_say_is_refused_naming_the_field():
    cases = {
        ("1 = 1", ("a", "Int64")): r"GpuFilter predicate: literal `1` has nothing beside it",
        ("a@0 = x1", ("a", "Int64")): r"literal `x1` does not read as integer",
        ("a@build:0 = 1", ("a", "Int64")): r"addressed build, which this field is not",
        ("(a@0 / b@1) > 1", ("a", "Int64"), ("b", "Int64")): r"integer division",
        ("sha256(a@0) = 1", ("a", "Utf8")): r"`sha256` with 1 arguments has no IR counterpart",
        ("a@0 IS DISTINCT FROM b@1", ("a", "Int64"), ("b", "Int64")): r"`IS DISTINCT FROM` has no IR",
        ("a@0 = 1", ("a", "Time64(ns)")): r"type `Time64\(ns\)` has no kind here",
        ("a@0 ILIKE x", ("a", "Utf8")): r"ILIKE",
    }
    for (text, *schema), message in cases.items():
        with raises(EnginePlanFormatError, match=message):
            predicate(text, *schema)
    project = node("GpuProject", [scan(("a", "Int64"))], schema=[("b", "Int64")], exprs="[a@0]")
    with raises(EnginePlanFormatError, match=r"GpuProject exprs: output `a` lands in `b`"):
        lower_field(project, "exprs")
    project = node("GpuProject", [scan(("a", "Utf8"))], schema=[("a", "Int64")], exprs="[a@0]")
    with raises(EnginePlanFormatError, match=r"`a` is string, its column integer"):
        lower_field(project, "exprs")


def test_the_frame_names_of_a_schema_are_its_columns():
    assert [c.frame for c in columns((("a", "Int64"), ("a", "Utf8")))] == ["a@0", "a@1"]
    assert columns((("a", "Decimal128(7,2)"),))[0].kind is Kind.FLOAT


if __name__ == "__main__":
    raise SystemExit(main(globals()))
