"""`same_answer`, which `run.py` holds an optimized run's answer to the planned run's by: the rows as
a multiset, money within its tolerance, and only how many where the query leaves which open."""

from __future__ import annotations

if __package__ in (None, ""):  # allow `python scripts/exec_model/tests/plans/<file>.py`
    import pathlib as _pathlib, sys as _sys

    _sys.path.insert(0, str(_pathlib.Path(__file__).resolve().parents[4]))
    __package__ = "scripts.exec_model.tests.plans"

import pathlib
import subprocess
import sys

import pandas as pd

from ..harness import main, raises
from ...plans.answers import same_answer

PLANNED = pd.DataFrame({"k": [1, 2, 3], "money": [10.25, 20.5, 30.75]})


def test_the_same_rows_in_another_order_and_under_other_names_are_the_same_answer():
    got = PLANNED.iloc[[2, 0, 1]].reset_index(drop=True).set_axis(["key", "sum(money)"], axis=1)
    same_answer("tpch", "q3", got, PLANNED, "q3")


def test_money_summed_in_another_order_is_within_the_tolerance():
    got = PLANNED.assign(money=PLANNED["money"] + 1e-9)
    same_answer("tpch", "q3", got, PLANNED, "q3")


def test_a_dropped_row_or_a_changed_value_is_another_answer():
    with raises(AssertionError, match="2 rows vs 3"):
        same_answer("tpch", "q3", PLANNED.iloc[:2], PLANNED, "q3")
    with raises(AssertionError, match="money"):
        same_answer("tpch", "q3", PLANNED.assign(money=[10.25, 20.5, 31.75]), PLANNED, "q3")
    with raises(AssertionError):
        same_answer("tpch", "q3", PLANNED[["k"]], PLANNED, "q3")


def test_a_limit_over_unordered_rows_is_held_to_how_many_alone():
    other_rows = PLANNED.assign(k=[7, 8, 9])
    same_answer("tpch", "scan-limit", other_rows, PLANNED, "scan-limit")
    with raises(AssertionError):
        same_answer("tpch", "scan-limit", other_rows.iloc[:2], PLANNED, "scan-limit")
    with raises(AssertionError):
        same_answer("tpch", "q3", other_rows, PLANNED, "q3")


def test_a_mismatch_is_raised_under_python_dash_o_too():
    # `-O` strips `assert`, and run.py's check must not pass vacuously there.
    script = ("import pandas as pd; from scripts.exec_model.plans.answers import same_answer; "
              "f = pd.DataFrame({'k': [1, 2]}); same_answer('tpch', 'q3', f.iloc[:1], f, 'q3')")
    root = pathlib.Path(__file__).resolve().parents[4]
    done = subprocess.run([sys.executable, "-O", "-c", script], cwd=root, capture_output=True, text=True,
                          timeout=120)
    assert done.returncode != 0 and "AssertionError: q3: 1 rows vs 2" in done.stderr, done.stderr


if __name__ == "__main__":
    raise SystemExit(main(globals()))
