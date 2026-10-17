"""Two answers to one query compared as strictly as SQL allows: the rows as a multiset, the
ORDER BY columns by position, money within a tolerance. The corpus suites compare the prototype
with DuckDB through `matches_oracle`; `run.py` compares an optimized run with the planned one
through `same_answer`.
"""

from __future__ import annotations

import numpy as np
import pandas as pd

#: A LIMIT over unordered rows: which rows is not determined, only how many.
UNORDERED_LIMIT = frozenset({("tpch", "scan-limit"), ("tpch", "nested-limits")})


def same_answer(bench: str, query: str, got: pd.DataFrame, want: pd.DataFrame, label: str) -> None:
    """`got` is `want` as a multiset, or how many where the query does not determine which rows;
    an `AssertionError` naming what differs otherwise. Columns by position: the two runs' names
    may be spelled apart."""
    _require(len(got.columns) == len(want.columns), f"{label}: {list(got.columns)} vs {list(want.columns)}")
    got = got.set_axis(list(want.columns), axis=1)
    if (bench, query) in UNORDERED_LIMIT:
        _require(len(got) == len(want), f"{label}: {len(got)} rows vs {len(want)}")
        return
    matches_oracle(got, want, label)


def _compare_columns(got: pd.DataFrame, want: pd.DataFrame, columns, label: str, what: str):
    """Column by column, aligning only what the transport could not carry.

    A date is compared against the engine's own dtype. Money is compared with a tolerance,
    since summing six million floats in a different order moves the last bits.
    """
    for column in columns:
        left, right = got[column], want[column]
        if pd.api.types.is_datetime64_any_dtype(left):
            right = pd.to_datetime(right)
        if pd.api.types.is_numeric_dtype(left) and pd.api.types.is_numeric_dtype(right):
            _require(np.allclose(left.to_numpy(dtype=float), right.to_numpy(dtype=float),
                                 rtol=1e-9, atol=1e-2, equal_nan=True),
                     f"{label}: {what} {column} differs from the answer it is held to")
        else:
            def nulls_alike(values):
                return [None if pd.isna(v) else v for v in values]

            _require(nulls_alike(left) == nulls_alike(right),
                     f"{label}: {what} {column} differs from the answer it is held to")


def _canonical(frame: pd.DataFrame, inexact) -> pd.DataFrame:
    """One frame's rows in an order that depends on nothing but their values. Exact columns
    lead; the inexact ones follow, rounded for the sort only — money summed in another order
    differs in the last bits, and that noise must not reorder the rows. `inexact` comes from
    both frames, not this one: surrogate keys arrive as float where some are null and as int
    from DuckDB, and a per-frame split would sort the two sides by different columns."""
    keys = frame.copy()
    for column in inexact:
        keys[column] = pd.to_numeric(keys[column], errors="coerce").round(4)
    order = [c for c in keys.columns if c not in inexact] + list(inexact)
    return frame.loc[
        keys.sort_values(order, kind="stable", na_position="first").index
    ].reset_index(drop=True)


def matches_oracle(got: pd.DataFrame, want: pd.DataFrame, label: str, order_by=None):
    """`got` against another answer — DuckDB's, in the corpus suites — as strictly as SQL allows:
    the column names as declared, the rows as a multiset, and the `order_by` columns (the
    query's ORDER BY as output names; None for none) by position, since tied sort keys leave
    the rows' order open. design.md, "DuckDB, the oracle", says why not whole rows by position."""
    _require(list(got.columns) == list(want.columns),
             f"{label}: {list(got.columns)} vs the query's {list(want.columns)}")
    _require(len(got) == len(want), f"{label}: {len(got)} rows vs {len(want)} in the answer it is held to")
    if order_by:
        missing = [column for column in order_by if column not in want.columns]
        _require(not missing, f"{label}: ORDER BY names {missing}, which the output has not")
        _compare_columns(got, want, order_by, label, "sort key")
    inexact = [
        column for column in want.columns
        if pd.api.types.is_float_dtype(got[column]) or pd.api.types.is_float_dtype(want[column])
    ]
    _compare_columns(_canonical(got, inexact), _canonical(want, inexact),
                     list(want.columns), label, "column")


def _require(holds: bool, message: str) -> None:
    """An `AssertionError` raised by hand: `python -O` strips `assert`, and a compare that passes
    vacuously there would let `run.py` write a wrong answer's run."""
    if not holds:
        raise AssertionError(message)
