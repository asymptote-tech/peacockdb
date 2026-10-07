"""An engine plan's aggregates: the planner's calls, run over a frame.

An engine plan arrives decomposed. Its init node runs sum/count/min/max/mean/m2 over raw rows,
its merge node sum/min/max/merge_m2 over the state, each call over expressions and each output
named by the plan, and a finalizing node carries an explicit `final` list. Nothing here decides
a decomposition — `avg` is the planner's sum and count, `count(*)` its `count(1)`.
"""

from __future__ import annotations

from dataclasses import dataclass

import numpy as np
import pandas as pd

from .expressions import project
from .frame import concatenate, normalize

#: The column a grouping-set expansion tags its rows with, sitting between the group keys
#: and the state columns — the position `cpp/src/operators/aggregate.cpp` gives it.
GROUPING_ID = "__grouping_id"

SUM = "sum"
MIN = "min"
MAX = "max"
COUNT = "count"
MEAN = "mean"


@dataclass(frozen=True)
class PlanCall:
    """`func(args…) AS outputs…` — one output, or merge_m2's count, mean and m2."""

    func: str
    args: tuple
    outputs: tuple[str, ...]


@dataclass(frozen=True)
class PlanAggregate:
    """An engine aggregate node's body. `keys` and `calls` read its input; `masks`, where
    it expands grouping sets, are per set the key positions masked out; `state` names what
    the calls produce — keys, the grouping id, every call's outputs; `final`, where it
    finalizes, reads `state`; `output` names what the node emits."""

    keys: tuple
    calls: tuple[PlanCall, ...]
    masks: tuple | None
    state: tuple[str, ...]
    final: tuple | None
    output: tuple[str, ...]

    def aggregate(self, frame: pd.DataFrame) -> pd.DataFrame:
        """The calls over `frame` — raw rows at an init, state (this node's own included)
        at a merge, whose input and state are one layout."""
        return plan_aggregate(frame, self.keys, self.calls, self.masks).set_axis(list(self.state), axis=1)

    def emit(self, state: pd.DataFrame) -> pd.DataFrame:
        if self.final is not None:
            n_keys = len(self.state) - sum(len(call.outputs) for call in self.calls)
            state = plan_finalize(state, n_keys, self.final)
        return state.set_axis(list(self.output), axis=1)


#: A global aggregate over no rows is still one row: each function's value over nothing.
_OVER_NOTHING = {SUM: [np.nan], COUNT: [0], MIN: [np.nan], MAX: [np.nan], MEAN: [np.nan],
                 "m2": [0.0], "merge_m2": [0, np.nan, 0.0]}


def plan_aggregate(frame: pd.DataFrame, keys, calls, masks=None) -> pd.DataFrame:
    """An engine aggregate's calls over `frame`: key columns (then the grouping id, where
    `masks` names grouping sets), then every call's outputs, in the plan's order. Columns
    are unnamed — positional; the node names them.

    `masks` are per set the key positions masked out, as the plan's sets imply. The id is
    DataFusion's (the first key the highest bit): it routes rows to lanes, and a lane-wise
    comparison with the engine's own run needs the engine's placement.
    """
    if masks is None:
        return _plan_group(frame, keys, calls, None)
    pieces = []
    for mask in masks:
        piece = _plan_group(frame, keys, calls, mask)
        grouping_id = 0
        for masked in mask:
            grouping_id = (grouping_id << 1) | int(masked)
        piece.insert(len(keys), GROUPING_ID, pd.Series([grouping_id] * len(piece), dtype="int64"))
        pieces.append(piece.set_axis(range(piece.shape[1]), axis=1))
    return concatenate(pieces)


def _plan_group(frame, keys, calls, mask) -> pd.DataFrame:
    work = pd.DataFrame(index=frame.index)
    for i, key in enumerate(keys):
        work[f"k{i}"] = np.nan if mask is not None and mask[i] else key.evaluate(frame)
    for c, call in enumerate(calls):
        for j, arg in enumerate(call.args):
            work[f"a{c}.{j}"] = arg.evaluate(frame)
    by = [f"k{i}" for i in range(len(keys))]
    if not by:
        if not len(work):
            return pd.DataFrame([[v for call in calls for v in _OVER_NOTHING[call.func]]])
        work["k"], by = 0, ["k"]
    for c, call in enumerate(calls):
        if call.func == "merge_m2":
            _prepare_merge_m2(work, by, c)
    # The null group is kept: cuDF groups with null_policy::INCLUDE, and pandas' default drop
    # loses tpcds q15's NULL `ca_zip` row.
    grouped = work.groupby(by, dropna=False, sort=True)
    values = [v for c, call in enumerate(calls) for v in _plan_values(grouped, call.func, c)]
    # No calls is a DISTINCT: the groups themselves are the output.
    table = pd.concat(values, axis=1) if values else pd.DataFrame(index=grouped.size().index)
    out = table.reset_index(drop=not keys)
    return normalize(out.set_axis(range(out.shape[1]), axis=1))


def _prepare_merge_m2(work, by, c) -> None:
    """The per-row terms of Chan's merge, cuDF's MERGE_M2 — not an average of deviations: each
    partial's count, its count-weighted mean, and its spread about the group's mean."""
    counts = work[f"a{c}.0"].astype("float64")
    means = work[f"a{c}.1"].astype("float64").where(counts > 0, 0.0)
    work[f"n{c}"], work[f"nm{c}"] = counts, counts * means
    grouped = work.groupby(by, dropna=False, sort=True)
    group_mean = grouped[f"nm{c}"].transform("sum") / grouped[f"n{c}"].transform("sum")
    work[f"spread{c}"] = (counts * (means - group_mean) ** 2).where(counts > 0, 0.0)


def _plan_values(grouped, func: str, c: int) -> list[pd.Series]:
    """One call over grouped rows, per output."""
    first = grouped[f"a{c}.0"]
    if func == SUM:
        return [first.sum(min_count=1)]  # NULL over only nulls, as cuDF and SQL have it
    if func == COUNT:
        return [first.count()]
    if func == MIN:
        return [first.min()]
    if func == MAX:
        return [first.max()]
    if func == MEAN:
        return [first.mean()]
    if func == "m2":
        # Σ(x − mean)² over the non-null values: 0 for a group with none, as a merge needs.
        return [(first.var(ddof=0) * first.count()).fillna(0.0)]
    if func == "merge_m2":
        total = grouped[f"n{c}"].sum()
        mean = (grouped[f"nm{c}"].sum() / total).where(total > 0)
        return [total, mean, grouped[f"a{c}.2"].sum() + grouped[f"spread{c}"].sum()]
    raise ValueError(f"unsupported plan aggregate {func!r}")


def plan_finalize(state: pd.DataFrame, n_keys: int, final) -> pd.DataFrame:
    """A finalizing node's output: the state's leading `n_keys` columns, then `final` —
    one expression per remaining output, over the state."""
    values = project(state, list(final))
    out = pd.concat([state.iloc[:, :n_keys].reset_index(drop=True), values], axis=1)
    return normalize(out.set_axis(range(out.shape[1]), axis=1))
