"""The engine's cost function (`test_support/cost_model.rs`) over a run in the `cpu.txt` format: the
categories and multipliers read from `testdata/cost_model.conf`, a hand-priced run, the total summed
and rounded as Rust does, a kind the conf does not list, a memory source free, and the engine's own
`.cost.txt` goldens derived again from their `cpu.txt`."""

from __future__ import annotations

if __package__ in (None, ""):  # allow `python scripts/exec_model/tests/plans/<file>.py`
    import pathlib as _pathlib, sys as _sys

    _sys.path.insert(0, str(_pathlib.Path(__file__).resolve().parents[4]))
    __package__ = "scripts.exec_model.tests.plans"

import pathlib

from ..harness import main, raises
from ...plans.cost_model import FREE, RunCost, load, parse
from ...plans.engine_plan import PROTOTYPE_KINDS
from ...plans.goldens import sections

GOLDENS = pathlib.Path(__file__).resolve().parents[4] / "testdata" / "goldens"

CONF = """\
# a comment line
scan_bytes    1.0  GpuLoadParquet   # trailing comment
filter_bytes  2.0  GpuFilter
join_bytes    0.5  GpuHashJoin,GpuNestedLoopJoin

placeholder   1.0
ram_to_vram_bytes 1.0
"""

#: Lines the reader must pass over — two detail lines, a capitalized one that is no node — fields
#: whose brackets and quotes hold `, output_bytes=`, and a node with no `output_bytes` at all.
RUN = """\
early_exit=none
Peak output_bytes=1000
GpuHashJoin: on=[(a@0, b@1)], output_rows=3, output_bytes=5
  in_rows=[[1],[2]] batch_rows=[[3]] batch_bytes=[[5]]
  GpuFilter: predicate=f(x@0, output_bytes=999) = "a, output_bytes=998", output_rows=2, output_bytes=40
    GpuLoadParquet: table=t, output_rows=9, output_bytes=100
  GpuMemorySource: name=m0, output_rows=4, output_bytes=64
  GpuNestedLoopJoin: output_rows=0
"""


def test_the_conf_is_read_as_categories_in_line_order_with_their_multipliers_and_kinds():
    model = parse(CONF)
    assert [(c.name, c.multiplier, c.nodes) for c in model.categories] == [
        ("scan_bytes", 1.0, ("GpuLoadParquet",)),
        ("filter_bytes", 2.0, ("GpuFilter",)),
        ("join_bytes", 0.5, ("GpuHashJoin", "GpuNestedLoopJoin")),
        ("placeholder", 1.0, ()),
        ("ram_to_vram_bytes", 1.0, ()),
    ]


def test_a_run_is_priced_per_category_and_totalled_with_the_multipliers():
    cost = parse(CONF).price(RUN, "hand")
    # 100 scanned, 40 filtered, 5 joined; the memory source's 64 are not priced at all.
    assert cost.bytes == (100, 40, 5, 0, 0)
    # 100 + 2 * 40 + 0.5 * 5 = 182.5, rounded half away from zero as the engine's `f64::round`.
    assert cost.total == 183


def test_runs_add_category_by_category():
    model = parse(CONF)
    first, second = model.price(RUN, "a"), model.price("GpuFilter: output_bytes=5\n", "b")
    assert (first + second).bytes == (100, 45, 5, 0, 0)
    assert (first + second).total == 193


def test_a_kind_the_conf_does_not_list_is_refused_naming_it():
    with raises(ValueError, match="hand: node kind 'GpuSort' is not in the cost taxonomy"):
        parse(CONF).price("GpuSort: output_rows=1, output_bytes=8\n", "hand")


def test_a_memory_source_is_free_whatever_the_conf_says_and_the_only_kind_that_is():
    # The conf's host-to-device category is 1.0 here: a kept build still costs nothing.
    assert FREE == PROTOTYPE_KINDS
    assert parse(CONF).price("GpuMemorySource: name=m0, output_bytes=64\n", "hand").total == 0


def test_the_total_is_summed_in_order_and_rounded_as_f64_round_does():
    # Summed term by term, as the engine's loop does: 1e16 + 1 is 1e16 in a double, so is 1e16 + 1
    # again; a compensated sum would give 1e16 + 2.
    assert RunCost((1.0, 1.0, 1.0), (10**16, 1, 1)).total == 10**16
    # Half away from zero on the double itself: the largest double below 0.5 rounds down.
    assert RunCost((0.49999999999999994,), (1,)).total == 0
    assert RunCost((0.5,), (1,)).total == 1
    # `as u64` saturates: a negative or NaN total is 0, one past the range or infinite is u64::MAX.
    assert RunCost((-2.0,), (3,)).total == 0
    assert RunCost((float("nan"),), (1,)).total == 0
    assert RunCost((2.0**64,), (1,)).total == RunCost((float("inf"),), (1,)).total == 2**64 - 1


def test_lines_split_at_newlines_only_as_rust_s_lines_does():
    # A form feed inside a field is part of the line: no `GpuSort` node starts after it.
    assert parse(CONF).price("GpuFilter: predicate=a\x0cGpuSort: output_bytes=5, output_bytes=40\r\n", "hand").bytes \
        == (0, 40, 0, 0, 0)


def cost_text(model, cpu_text: str, context: str) -> str:
    """`cost_text_from_cpu`: a line per category, then the total."""
    cost = model.price(cpu_text, context)
    lines = [f"{c.name}={n} # {', '.join(c.nodes) or '(placeholder, no node mapping)'}"
             for c, n in zip(model.categories, cost.bytes)]
    return "\n".join(lines + [f"peacockdb_cost={cost.total}"])


def test_the_engine_s_cost_goldens_are_what_this_function_derives_from_their_cpu_runs():
    model, checked = load(), 0
    for cpu in sorted(GOLDENS.glob("*.sf1/*-mini.cpu.txt")):
        # `cost_text_from_sections`: a section that is a marker rather than a run is copied through.
        derived = "".join(
            f"== {query}\n" + (body if body.startswith("skipped: ") else cost_text(model, body, query) + "\n")
            for query, body in sections(cpu.read_text()))
        assert derived == cpu.with_name(cpu.name.replace(".cpu.txt", ".cost.txt")).read_text(), cpu
        checked += 1
    assert checked == 10, checked


if __name__ == "__main__":
    raise SystemExit(main(globals()))
