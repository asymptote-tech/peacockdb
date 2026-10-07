"""Filter selectivity from NDV, min/max and nulls: each rule on a column whose truth is known."""

from __future__ import annotations

if __package__ in (None, ""):  # allow `python scripts/exec_model/tests/optimizer/<file>.py`
    import pathlib as _pathlib, sys as _sys

    _sys.path.insert(0, str(_pathlib.Path(__file__).resolve().parents[4]))
    __package__ = "scripts.exec_model.tests.optimizer"

import datetime

from ..harness import main
from ...optimizer.estimator import DEFAULT, selectivity
from ...operators import expressions as E
from ...optimizer.stats import ColumnStats

ROWS = 1200
#: `moy` is 1..12, a hundred rows each; `half` is NULL in half the rows; `one` is always 7
COLUMNS = {
    "moy": ColumnStats(ndv=12, nulls=0, low=1, high=12),
    "half": ColumnStats(ndv=10, nulls=600, low=1, high=10),
    "one": ColumnStats(ndv=1, nulls=0, low=7, high=7),
    "day": ColumnStats(ndv=366, nulls=0, low=datetime.date(2000, 1, 1), high=datetime.date(2000, 12, 31)),
    "name": ColumnStats(ndv=4, nulls=0, low="a", high="d"),
}


def sel(predicate) -> float:
    return selectivity(predicate, COLUMNS.get, ROWS)


def cmp(column, op, value):
    return E.Binary(op, E.Col(column), E.Lit(value))


def both(op, left, right):
    return E.Binary(op, left, right)


def test_an_equality_is_one_value_of_ndv_and_nothing_outside_the_range():
    assert sel(cmp("moy", "==", 4)) == 1 / 12
    assert sel(cmp("moy", "==", 13)) == 0
    assert sel(cmp("name", "==", "e")) == 0
    assert sel(cmp("moy", "!=", 4)) == 11 / 12


def test_a_range_counts_the_evenly_spaced_values_it_covers():
    assert abs(sel(cmp("moy", "<=", 4)) - 4 / 12) < 1e-12
    assert abs(sel(cmp("moy", "<", 4)) - 3 / 12) < 1e-12
    assert abs(sel(E.Binary(">=", E.Lit(4), E.Col("moy"))) - 4 / 12) < 1e-12  # literal first
    assert sel(cmp("moy", ">", 12)) == 0 and sel(cmp("moy", ">=", 1)) == 1
    march = both("and", cmp("day", ">=", datetime.date(2000, 3, 1)), cmp("day", "<", datetime.date(2000, 4, 1)))
    assert abs(sel(march) - 31 / 366) < 1e-9


def test_bounds_on_one_column_merge_before_anything_multiplies():
    between = both("and", cmp("moy", ">=", 3), cmp("moy", "<=", 5))
    assert abs(sel(between) - 3 / 12) < 1e-12  # not (10/12) · (5/12)
    empty = both("and", cmp("moy", ">", 5), cmp("moy", "<", 3))
    assert sel(empty) == 0


def test_an_or_of_equalities_on_one_column_is_its_distinct_values():
    three = both("or", both("or", cmp("moy", "==", 1), cmp("moy", "==", 2)), cmp("moy", "==", 2))
    assert sel(three) == 2 / 12
    mixed = both("or", cmp("moy", "==", 1), cmp("name", "==", "a"))
    assert abs(sel(mixed) - (1 / 12 + 1 / 4 - 1 / 48)) < 1e-12


def test_null_matches_no_comparison():
    assert sel(cmp("half", "==", 3)) == 0.5 / 10
    assert sel(E.IsNull(E.Col("half"))) == 0.5
    assert sel(E.IsNotNull(E.Col("half"))) == 0.5


def test_a_column_of_one_value_keeps_everything_or_nothing():
    assert sel(cmp("one", ">=", 7)) == 1 and sel(cmp("one", ">", 7)) == 0
    assert sel(cmp("one", "==", 7)) == 1


def test_what_statistics_cannot_speak_to_is_the_default():
    assert sel(E.Like(E.Col("name"), "a%")) == DEFAULT
    assert sel(E.Like(E.Col("name"), "a%", negated=True)) == 1 - DEFAULT
    assert sel(both(">", E.Col("moy"), E.Col("half"))) == DEFAULT
    assert sel(cmp("computed", "==", 1)) == DEFAULT
    assert sel(cmp("name", "<", "b")) == DEFAULT
    assert sel(E.Not(cmp("moy", "==", 4))) == 11 / 12


def test_an_or_of_unknowns_is_one_unknown_not_their_union():
    # q8's shape: `substr(ca_zip, 1, 5) = '…'`, four hundred times over
    prefix = lambda i: E.Binary("==", E.Substring(E.Col("name"), 1, 5), E.Lit(f"{i:05}"))
    unknowns = prefix(0)
    for i in range(1, 400):
        unknowns = both("or", unknowns, prefix(i))
    assert sel(unknowns) == DEFAULT
    assert abs(sel(both("and", unknowns, cmp("moy", "==", 4))) - DEFAULT / 12) < 1e-12


if __name__ == "__main__":
    raise SystemExit(main(globals()))
