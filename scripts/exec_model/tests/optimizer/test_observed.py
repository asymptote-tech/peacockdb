"""Intermediate NDV: exact from per-lane counts where the lanes cannot share a value, bounds
where they can."""

from __future__ import annotations

if __package__ in (None, ""):  # allow `python scripts/exec_model/tests/optimizer/<file>.py`
    import pathlib as _pathlib, sys as _sys

    _sys.path.insert(0, str(_pathlib.Path(__file__).resolve().parents[4]))
    __package__ = "scripts.exec_model.tests.optimizer"

import pandas as pd

from ..harness import main
from ...optimizer.observed import NdvBounds, observed_ndv, stream_ndv

#: two lanes hashed on `k`: each `k` on one lane, but `v` = 1 on both
LANES = [pd.DataFrame({"k": [1, 1, 3], "v": [1, 1, 2]}), pd.DataFrame({"k": [2, 4, 4], "v": [1, 3, 3]})]


def test_lanes_hashed_on_the_counted_columns_sum_exactly():
    assert observed_ndv(LANES, ["k"], hashed_on=["k"]) == NdvBounds(4, 4)
    # A pair of columns that includes the hash column is disjoint across lanes too.
    assert observed_ndv(LANES, ["k", "v"], hashed_on=["k"]).exact


def test_other_columns_are_bounded_by_the_largest_lane_and_the_sum():
    got = observed_ndv(LANES, ["v"], hashed_on=["k"])
    assert (got, got.exact) == (NdvBounds(2, 4), False)  # the truth, 3, lies between
    assert observed_ndv(LANES, ["v"], hashed_on=None) == NdvBounds(2, 4)


def test_one_lane_with_rows_is_exact_whatever_the_layout():
    single = [LANES[1], pd.DataFrame({"k": [], "v": []})]
    assert observed_ndv(single, ["v"], hashed_on=None) == NdvBounds(2, 2)
    assert observed_ndv([pd.DataFrame({"v": []})], ["v"], hashed_on=None) == NdvBounds(0, 0)


def test_a_null_key_is_no_value():
    lane = pd.DataFrame({"k": [1, None, None], "v": [1, 1, None]})
    assert observed_ndv([lane], ["k"], hashed_on=None) == NdvBounds(1, 1)
    assert observed_ndv([lane], ["k", "v"], hashed_on=None) == NdvBounds(1, 1)


def test_a_stream_has_no_more_values_than_it_had_or_than_rows():
    assert (stream_ndv(100, 30), stream_ndv(10, 30)) == (30, 10)


if __name__ == "__main__":
    raise SystemExit(main(globals()))
