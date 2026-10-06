"""`run.py` without data: how a run's sections go into a file that has some already, the order they
go in, and that arguments naming nothing fail before any work."""

from __future__ import annotations

if __package__ in (None, ""):  # allow `python scripts/exec_model/tests/<file>.py`
    import pathlib as _pathlib, sys as _sys

    _sys.path.insert(0, str(_pathlib.Path(__file__).resolve().parents[3]))
    __package__ = "scripts.exec_model.tests"

import os
import pathlib
import tempfile

from .harness import main, raises
from ..optimizer.dynamic_filters import Batching
from ..optimizer.pipeline import MODES, ModeShape, mode_shape
from ..run import Regeneration, Selection, Task, corpus_order, generate, merged, preflight, sections, tasks

FILE = "== a\nold a\n== gone\nold gone\n== b\nold b\n"


def test_a_filtered_run_replaces_its_sections_and_keeps_every_other_byte():
    text = merged(FILE, ["a", "b", "c"], {"b": "new b\n"}, Regeneration.SECTIONS)
    assert text == "== a\nold a\n== b\nnew b\n== gone\nold gone\n"


def test_a_whole_run_drops_the_sections_no_query_accounts_for():
    text = merged(FILE, ["a", "b", "c"], {"b": "new b\n", "c": "new c\n"}, Regeneration.WHOLE)
    assert text == "== a\nold a\n== b\nnew b\n== c\nnew c\n"


def test_sections_are_read_whole_whatever_their_bodies_hold():
    assert sections("== a\n--- before\n+++ after\n== b\n\n") == [("a", "--- before\n+++ after\n"), ("b", "\n")]


def test_the_corpus_order_is_the_engine_s_with_its_missing_queries_where_they_sort():
    engine = ["q1", "q2", "q10", "a-named"]
    assert corpus_order(engine, ["a-named", "q10", "q1", "q2", "q3", "q11"]) == \
        ["q1", "q2", "q3", "q10", "q11", "a-named"]


def test_the_work_is_every_planned_query_and_each_refusal_a_section_of_its_own():
    selection = tasks(["tpcds"], ["tp4-single"], ["q3", "q12"])
    assert [(t.bench, t.mode, t.query) for t in selection.work] == [("tpcds", "tp4-single", "q3")]
    assert list(selection.skipped[("tpcds", "tp4-single")]) == ["q12"]
    assert selection.skipped[("tpcds", "tp4-single")]["q12"].startswith("skipped: refused")
    # The refusal's first line: q27's goes on to print DataFusion's whole plan.
    refused = tasks(["tpcds"], ["tp4-single"], ["q27"]).skipped[("tpcds", "tp4-single")]["q27"]
    assert refused == "skipped: refused by datafusion: SanityCheckPlan\n", refused[:200]
    # q12 has no section in the engine's cpu golden: the planner refused it. It sorts after q11.
    order = selection.order[("tpcds", "tp4-single")]
    assert order[:13] == [f"q{n}" for n in range(1, 14)] and len(order) == 99


def test_a_query_no_bench_plans_is_refused_before_any_work():
    with raises(ValueError, match="q999"):
        tasks(["tpch", "tpcds"], ["tp4-single"], ["q3", "q999"])
    with raises(ValueError, match="q64"):
        tasks(["tpch"], ["tp4-single"], ["q64"])
    # One bench of two naming it is enough.
    assert [t.bench for t in tasks(["tpch", "tpcds"], ["tp4-single"], ["join-int"]).work] == ["tpch"]


def test_a_mode_name_is_its_lanes_and_batching_and_an_unknown_one_is_refused():
    assert mode_shape("tp4-rowgroup") == ModeShape(4, Batching.ONE_PER_ROW_GROUP)
    assert mode_shape("tp1-single") == ModeShape(1, Batching.ONE_PER_LANE)
    assert mode_shape("tp4-sized") == ModeShape(4, Batching.KEEP_BATCHES)
    assert len(MODES) == 5
    with raises(ValueError, match="tp1-single, tp1-rowgroup, tp4-single, tp4-rowgroup, tp4-sized"):
        mode_shape("tp8-standard")


def test_a_query_that_raises_is_a_failed_section_in_both_files_and_the_run_exits_1():
    # A query the plan golden lacks raises KeyError in the worker before any data is read.
    key = ("tpcds", "tp4-single")
    selection = Selection([Task(*key, "nope")], {key: ["q27", "nope"]},
                          {key: {"q27": "skipped: refused\n"}})
    with tempfile.TemporaryDirectory() as scratch:
        out = pathlib.Path(scratch)
        assert generate(selection, 1, out, Regeneration.WHOLE) == 1
        for kind in ("cpu", "optimizer"):
            text = (out / "tpcds" / f"tp4-single.{kind}.txt").read_text()
            assert text == "== q27\nskipped: refused\n== nope\nfailed: KeyError: 'nope'\n", text


def test_a_missing_dataset_or_dphyp_library_is_refused_before_any_work():
    with tempfile.TemporaryDirectory() as scratch:
        root = pathlib.Path(scratch)
        with raises(ValueError, match="tpch.sf1"):
            preflight(["tpch"], root)
        (root / "tpch.sf1").mkdir()
        was = os.environ.get("PEACOCK_DPHYP_LIB")
        os.environ["PEACOCK_DPHYP_LIB"] = str(root / "absent.so")
        try:
            with raises(RuntimeError, match="PEACOCK_DPHYP_LIB"):
                preflight(["tpch"], root)
        finally:
            if was is None:
                del os.environ["PEACOCK_DPHYP_LIB"]
            else:
                os.environ["PEACOCK_DPHYP_LIB"] = was


if __name__ == "__main__":
    raise SystemExit(main(globals()))
