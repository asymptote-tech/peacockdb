"""Reading the engine planner's plan goldens back into trees."""

from __future__ import annotations

if __package__ in (None, ""):  # allow `python scripts/exec_model/tests/<file>.py`
    import pathlib as _pathlib, sys as _sys

    _sys.path.insert(0, str(_pathlib.Path(__file__).resolve().parents[3]))
    __package__ = "scripts.exec_model.tests"

import pathlib

from .harness import main, raises
from ..engine_plan import KINDS, EngineNode, Refusal, parse_plans, read_plans
from ..errors import EnginePlanFormatError

GOLDENS = pathlib.Path(__file__).resolve().parents[3] / "testdata" / "goldens"


def plan_goldens():
    paths = sorted(GOLDENS.glob("*/*.plans.txt"))
    # An empty glob would make every test below pass having read nothing.
    assert {path.parent.name for path in paths} >= {"tpch.sf1", "tpcds.sf1"}, paths
    return paths


def walk(node):
    yield node
    for child in node.children:
        yield from walk(child)


def rendered(node, depth=0):
    fields = ", ".join(f"{key}={value}" for key, value in node.fields.items())
    yield "  " * depth + node.kind + (f": {fields}" if fields else "")
    for child in node.children:
        yield from rendered(child, depth + 1)


def tree_text(text):
    """Per section, the lines before its first `--- ` marker: the tree, or the refusal."""
    sections, name = {}, None
    for line in text.splitlines():
        if line.startswith("== "):
            name, taking = line[3:], True
            sections[name] = []
        elif line.startswith("--- "):
            taking = False
        elif taking:
            sections[name].append(line)
    return sections


def test_every_plan_golden_parses_to_a_tree_or_a_refusal_per_section():
    for path in plan_goldens():
        text = path.read_text()
        plans = read_plans(path)
        headers = [line[3:] for line in text.splitlines() if line.startswith("== ")]
        assert list(plans) == headers, path
        for name, plan in plans.items():
            if isinstance(plan, Refusal):
                assert plan.text.startswith("refused"), (path, name)
                continue
            kinds = [node.kind for node in walk(plan)]
            assert set(kinds) <= KINDS, (path, name)
            # The validator's rule: a plan ends at the one crossing back to the host.
            assert kinds[0] == "GpuUnload" and kinds.count("GpuUnload") == 1, (path, name)


def test_rendering_the_tree_back_gives_the_golden_lines():
    for path in plan_goldens():
        text = path.read_text()
        expected = tree_text(text)
        for name, plan in read_plans(path).items():
            if isinstance(plan, EngineNode):
                assert list(rendered(plan)) == expected[name], (path, name)


def test_every_node_but_the_sink_declares_its_schema():
    for path in plan_goldens():
        for name, plan in read_plans(path).items():
            if isinstance(plan, Refusal):
                continue
            for node in walk(plan):
                assert ("schema" in node.fields) == (node.kind != "GpuUnload"), (path, name, node.kind)


def test_a_golden_schema_reads_as_name_type_pairs():
    plans = read_plans(GOLDENS / "tpch.sf1" / "tp4-single.plans.txt")
    aggregate = plans["aggregate-groupby"].children[0]
    assert aggregate.kind == "GpuAggregateBatches"
    assert aggregate.schema == (
        ("l_returnflag", "Utf8"),
        ("sum(lineitem.l_quantity)", "Decimal128(25,2)"),
    )


SAMPLE = """\
== q
GpuUnload
  GpuHashJoin: join_type=Inner, on=[(a@0, b@1), (c@2, d@3)], filter=`x, y`@build:0 > 1, schema=[a:Int64, `s(x, y)`:Decimal128(15,2)]
    GpuLoadParquet: table=t, schema=[a:Int64]
    GpuFilter: predicate=c@0 = BUILDING, schema=[]
--- recipes ---
GpuUnload: calling_lanes=4, per handle: result_from_handle(batch, row range)
--- memory ---
budget=1
"""


def test_commas_inside_brackets_and_backticks_do_not_split_a_field():
    join = parse_plans(SAMPLE, "sample")["q"].children[0]
    assert join.fields["on"] == "[(a@0, b@1), (c@2, d@3)]"
    assert join.fields["filter"] == "`x, y`@build:0 > 1"
    assert join.schema == (("a", "Int64"), ("s(x, y)", "Decimal128(15,2)"))


def test_a_value_keeps_its_own_equals_sign():
    predicate = parse_plans(SAMPLE, "sample")["q"].children[0].children[1]
    assert predicate.fields["predicate"] == "c@0 = BUILDING"
    assert predicate.schema == ()


def test_the_recipes_and_memory_sections_are_not_read_as_nodes():
    plan = parse_plans(SAMPLE, "sample")["q"]
    assert [node.kind for node in walk(plan)] == [
        "GpuUnload", "GpuHashJoin", "GpuLoadParquet", "GpuFilter",
    ]


def test_a_refusal_keeps_every_line_of_its_section():
    text = (
        "== q1\nrefused by datafusion: SanityCheckPlan\ncaused by\nError during planning: x\n"
        "== q2\nGpuUnload\n"
    )
    plans = parse_plans(text, "sample")
    assert plans["q1"] == Refusal(
        "refused by datafusion: SanityCheckPlan\ncaused by\nError during planning: x"
    )
    assert plans["q2"].kind == "GpuUnload"


def test_text_the_reader_was_not_taught_is_refused_with_its_line():
    cases = {
        "== q\nGpuUnload\n  GpuTeleport: lanes=1\n": r"sample:3: unknown node kind `GpuTeleport`",
        "== q\nGpuUnload\n   GpuFilter: lanes=1\n": r"sample:3: indent of 3 spaces",
        "== q\nGpuUnload\n    GpuFilter: lanes=1\n": r"sample:3: indented 2 levels",
        "== q\nGpuUnload\nGpuUnload\n": r"sample:3: a second root",
        "== q\nGpuUnload\n  GpuFilter: lanes=1, Home, schema=[]\n": r"`Home` is not a `key=value`",
        "== q\nGpuUnload\n  GpuFilter: lanes=1, lanes=2\n": r"field `lanes` given twice",
        "== q\nGpuUnload\n  GpuFilter: predicate=(a@0, lanes=1\n": r"unbalanced",
        "== q\nGpuUnload\n  GpuFilter: schema=[a]\n": r"`a` is not one `name:type`",
        "== q\nGpuUnload\n  GpuFilter: schema=[a b:Int64]\n": r"sample:3: text after the end",
        "GpuUnload\n": r"sample:1: text before the first",
        "== q\n== r\nGpuUnload\n": r"sample:1: section `q` is empty",
        "== q\nGpuUnload\n== q\nGpuUnload\n": r"sample:3: second section named `q`",
        "== q\n--- recipes ---\n": r"section `q` has no node lines",
    }
    for text, message in cases.items():
        with raises(EnginePlanFormatError, match=message):
            parse_plans(text, "sample")


if __name__ == "__main__":
    raise SystemExit(main(globals()))
