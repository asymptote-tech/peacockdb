"""Aggregate specs and the three-part decomposition.

An aggregate is declared as three things, and every node runs the parts it needs — there
is no phase flag anywhere (`llm-wiki/architecture.md`, "The aggregate sequence"):

- **init** (`partial`) — aggregators over raw rows, emitting *state* columns. One aggregate
  may emit several: `avg` emits sum and count, `stddev` emits Welford's count, mean and m2.
- **merge** (`merge`) — aggregators over state columns, emitting the same state schema.
  Not the same functions as init: a `count` merges by `sum`, and Welford state merges by
  `merge_m2`.
- **finalize** (`finalize_exprs`) — one expression per output column, over the merged
  state. `avg` is a divide, `stddev` a CASE over a sqrt, `count` and `sum` a rename.

`finalize_exprs` is the prototype's version of the decomposition registry the translation
layer will own: the one place that knows how a SQL aggregate becomes plan-level nodes and
expressions, so adding an aggregate is a row here rather than an arm in C++.

Three rules from the wiki are load-bearing and all three are easy to get wrong:

- **A mean-type aggregate must decompose to partial SUM + COUNT and never average partial
  means** (build-test.md's multi-GPU note), which is why `state_columns` distinguishes an
  aggregate's state schema from its output schema at all.
- **Grouped aggregation includes the null group.** cuDF is called with
  `null_policy::INCLUDE` and dropping it loses tpcds q15's NULL `ca_zip` row
  (architecture.md, cuDF options). pandas `groupby` drops NaN keys by default, so every
  groupby here passes `dropna=False`.
- **A stddev/var merge is Welford, not an average of deviations.** Combining two partials
  needs the count-weighted mean and the cross term `n_a·n_b/(n_a+n_b)·(mean_b−mean_a)²`,
  which is what cuDF's MERGE_M2 does and what `_merge_frame` reproduces here.

`COUNT(*)` and `COUNT(col)` are different aggregates, not one with a flag: the first counts
rows, the second counts non-nulls, and picking the wrong one is always wrong for the other
(architecture.md's rolling-count row).
"""

from __future__ import annotations

from dataclasses import dataclass

import numpy as np
import pandas as pd

from .expressions import Alias, Binary, Case, Col, Expr, Lit, Sqrt, project
from .frame import concatenate, normalize

#: The column a grouping-set expansion tags its rows with, sitting between the group keys
#: and the state columns — the position `cpp/src/operators/aggregate.cpp` gives it.
GROUPING_ID = "__grouping_id"

SUM = "sum"
MIN = "min"
MAX = "max"
COUNT = "count"
MEAN = "mean"
STDDEV = "stddev"
VAR = "var"

#: Welford-state aggregates, whose state is three columns and whose merge is MERGE_M2.
_WELFORD = (STDDEV, VAR)
_SUPPORTED = (SUM, MIN, MAX, COUNT, MEAN, STDDEV, VAR)


@dataclass(frozen=True)
class Agg:
    """`func(column) AS output`. `column=None` with COUNT means COUNT(*).

    `ddof` is the stddev/var divisor's degrees of freedom: 1 for the sample forms
    (STDDEV/STDDEV_SAMP/VAR/VAR_SAMP), 0 for the population ones (STDDEV_POP/VAR_POP),
    matching `stddev_ddof` in `cpp/src/operators/aggregate.cpp`.
    """

    func: str
    column: str | None
    output: str
    ddof: int = 1

    def __post_init__(self):
        if self.func not in _SUPPORTED:
            raise ValueError(f"unsupported aggregate {self.func!r}")
        if self.column is None and self.func != COUNT:
            raise ValueError(f"{self.func} needs a column")
        if self.ddof not in (0, 1):
            raise ValueError("ddof is 0 (population) or 1 (sample)")

    @property
    def state_columns(self) -> list[str]:
        """The init phase's output columns for this aggregate."""
        if self.func == MEAN:
            return [f"{self.output}$sum", f"{self.output}$count"]
        if self.func in _WELFORD:
            return [f"{self.output}$count", f"{self.output}$mean", f"{self.output}$m2"]
        return [self.output]


# -- the three phases --------------------------------------------------------------


def _partial_frame(group: pd.DataFrame, aggs: list[Agg]) -> dict:
    """init: raw rows → state."""
    out = {}
    for agg in aggs:
        if agg.func == MEAN:
            out[f"{agg.output}$sum"] = group[agg.column].sum()
            out[f"{agg.output}$count"] = group[agg.column].count()
        elif agg.func in _WELFORD:
            values = group[agg.column]
            n = values.count()
            mean = values.mean() if n else np.nan
            out[f"{agg.output}$count"] = n
            out[f"{agg.output}$mean"] = mean
            # Σ(x − mean)² over the non-null values; pandas' sum skips NaN, and an empty
            # group sums to 0, which is the identity a later merge needs.
            out[f"{agg.output}$m2"] = ((values - mean) ** 2).sum() if n else 0.0
        elif agg.func == COUNT:
            # COUNT(*) counts rows; COUNT(col) counts non-nulls.
            out[agg.output] = len(group) if agg.column is None else group[agg.column].count()
        elif agg.func == SUM:
            # min_count=1: SUM over a group whose values are all null is NULL, not zero —
            # SQL's rule and cuDF's (null_policy EXCLUDE over an all-null group yields
            # null). pandas' default of 0 put a real number where TPC-DS q93 expects none.
            out[agg.output] = group[agg.column].sum(min_count=1)
        elif agg.func == MIN:
            out[agg.output] = group[agg.column].min()
        else:
            out[agg.output] = group[agg.column].max()
    return out


def _merge_frame(group: pd.DataFrame, aggs: list[Agg]) -> dict:
    """merge: state → the same state. Never the init function — a count merges by sum."""
    out = {}
    for agg in aggs:
        if agg.func == MEAN:
            out[f"{agg.output}$sum"] = group[f"{agg.output}$sum"].sum()
            out[f"{agg.output}$count"] = group[f"{agg.output}$count"].sum()
        elif agg.func in _WELFORD:
            counts = group[f"{agg.output}$count"].to_numpy(dtype="float64")
            means = group[f"{agg.output}$mean"].to_numpy(dtype="float64")
            m2s = group[f"{agg.output}$m2"].to_numpy(dtype="float64")
            total = counts.sum()
            if total > 0:
                # Count-weighted mean, then each partial's deviation from it carried into
                # M2 — Chan's parallel form of Welford, i.e. cuDF's MERGE_M2.
                mean = np.nansum(counts * np.where(counts > 0, means, 0.0)) / total
                m2 = np.nansum(m2s) + np.nansum(
                    counts * np.where(counts > 0, (means - mean) ** 2, 0.0)
                )
            else:
                mean, m2 = np.nan, 0.0
            out[f"{agg.output}$count"] = total
            out[f"{agg.output}$mean"] = mean
            out[f"{agg.output}$m2"] = m2
        elif agg.func == SUM:
            # As in the init phase: a merge of partials that are all null is still null,
            # and pandas' `.sum()` would make it zero.
            out[agg.output] = group[agg.output].sum(min_count=1)
        elif agg.func == COUNT:
            # A count merges by summing, and no count is ever null — an empty merge is 0.
            out[agg.output] = group[agg.output].sum()
        elif agg.func == MIN:
            out[agg.output] = group[agg.output].min()
        else:
            out[agg.output] = group[agg.output].max()
    return out


def finalize_exprs(aggs: list[Agg]) -> list[Expr]:
    """finalize: one expression per output column, over the merged state.

    The decomposition registry — what the translation layer emits as a node's `final`
    list, and what replaces the hardwired `avg_div` / `std_finalize` arms in C++.
    """
    exprs: list[Expr] = []
    for agg in aggs:
        state = agg.state_columns
        if agg.func == MEAN:
            exprs.append(Alias(Binary("/", Col(state[0]), Col(state[1])), agg.output))
        elif agg.func in _WELFORD:
            count, _mean, m2 = (Col(c) for c in state)
            divisor = Binary("-", count, Lit(float(agg.ddof)))
            variance = Binary("/", m2, divisor)
            value = variance if agg.func == VAR else Sqrt(variance)
            # A group with count <= ddof has no dispersion to report: NULL, not a divide
            # by zero or a negative under the root.
            exprs.append(
                Alias(
                    Case(
                        whens=((Binary("<=", divisor, Lit(0.0)), Lit(np.nan)),),
                        otherwise=value,
                    ),
                    agg.output,
                )
            )
        else:
            exprs.append(Alias(Col(agg.output), agg.output))
    return exprs


# -- applying a phase over groups ---------------------------------------------------


def _apply(
    frame: pd.DataFrame, keys: list[str], aggs: list[Agg], builder, out_columns: list[str]
) -> pd.DataFrame:
    if not keys:
        return normalize(pd.DataFrame([builder(frame, aggs)]))
    rows = []
    # dropna=False keeps the null group — cuDF's null_policy::INCLUDE.
    # sort=True gives a deterministic group order, which matters because the goldens
    # compare rendered output and float sums are order-sensitive.
    for key_values, group in frame.groupby(keys, dropna=False, sort=True):
        if not isinstance(key_values, tuple):
            key_values = (key_values,)
        rows.append(dict(zip(keys, key_values)) | builder(group, aggs))
    if not rows:
        # The empty frame must carry the columns THIS PHASE declares. Getting it wrong is
        # not cosmetic: a partial's state columns leaking out of a final produces a frame
        # `cudf::concatenate` would reject, and an all-empty lane is exactly where it shows.
        #
        # Key columns keep the input's dtype rather than defaulting to float64. cuDF has no
        # choice about this — a column has a type — but pandas does, and taking the default
        # is a live bug: concatenating an empty float64 key onto an int64 one retypes the
        # key, `partition_ids` stringifies 5.0 where another lane stringifies 5, and equal
        # keys stop co-locating. Wrong answers, only under an empty batch.
        empty = {
            c: (frame[c].iloc[0:0] if c in frame.columns else pd.Series([], dtype="float64"))
            for c in out_columns
        }
        return pd.DataFrame(empty)
    return normalize(pd.DataFrame(rows))


def partial(frame: pd.DataFrame, keys: list[str], aggs: list[Agg]) -> pd.DataFrame:
    """Per-batch init aggregation; emits state columns."""
    columns = keys + [c for agg in aggs for c in agg.state_columns]
    return _apply(frame, keys, aggs, _partial_frame, columns)


def merge(frame: pd.DataFrame, keys: list[str], aggs: list[Agg]) -> pd.DataFrame:
    """Merge state into the same state schema — what `GpuAggregateBatches` runs."""
    columns = keys + [c for agg in aggs for c in agg.state_columns]
    return _apply(frame, keys, aggs, _merge_frame, columns)


def finalize(frame: pd.DataFrame, keys: list[str], aggs: list[Agg]) -> pd.DataFrame:
    """Apply the finalize expressions to merged state: keys pass through, aggs evaluate."""
    exprs = [Alias(Col(k), k) for k in keys] + finalize_exprs(aggs)
    return project(frame, exprs)


def final(frame: pd.DataFrame, keys: list[str], aggs: list[Agg]) -> pd.DataFrame:
    """Merge state and finalize in one step — the shape an oracle wants."""
    return finalize(merge(frame, keys, aggs), keys, aggs)


def single(frame: pd.DataFrame, keys: list[str], aggs: list[Agg]) -> pd.DataFrame:
    """The 1-partition single-batch shortcut — and the oracle the split is checked against."""
    return final(partial(frame, keys, aggs), keys, aggs)


# -- grouping sets -----------------------------------------------------------------


def grouping_set_id(mask) -> int:
    """The id tagging a set's rows: the bitmask of its MASKED positions.

    Distinct per set, which is all the merge needs to keep sets apart when a placeholder
    NULL collides with a natural one — but not DataFusion's `GROUPING()` encoding, which
    is [#65](../../../llm-wiki/tickets.md#t65) and unobservable until a query projects it.
    """
    return sum(1 << i for i, masked in enumerate(mask) if masked)


def rollup_masks(n_keys: int) -> list[tuple[bool, ...]]:
    """`ROLLUP(k0, …)` — the n+1 sets keeping successively shorter key prefixes.

    A mask entry is True where that key position is masked *out*, matching the fb
    `GroupingSetMask` convention. For two keys that is (F,F), (F,T), (T,T), so the ids are
    0, 2 and 3 rather than 0, 1, 2.
    """
    return [tuple([False] * kept + [True] * (n_keys - kept)) for kept in range(n_keys, -1, -1)]


def partial_over_sets(frame, keys: list[str], aggs: list[Agg], masks) -> pd.DataFrame:
    """Init over grouping sets: one groupby per set, tagged, concatenated into one frame.

    Deliberately not an optional argument on `partial` — a trailing option that selects a
    different algorithm is the shape coding-style.md warns about. The node names which one
    it wants and this is the other one.

    Not a row-multiplying expand either: each set groups the *same* input, so the peak is
    the input plus the sum of the per-set outputs rather than k times the input. One frame
    comes out, never one per set, because an executor may return at most one batch per call
    per output lane.
    """
    if not masks:
        raise ValueError("grouping sets: at least one set is required")
    state_columns = keys + [c for agg in aggs for c in agg.state_columns]
    pieces = []
    for mask in masks:
        if len(mask) != len(keys):
            raise ValueError(
                f"grouping set mask has {len(mask)} entries against {len(keys)} group keys"
            )
        masked = frame
        if any(mask):
            masked = frame.copy()
            for key, hidden in zip(keys, mask):
                if hidden:
                    # A masked position becomes an all-NULL column of that position's
                    # type. pandas cannot hold NA in int64, so an integer key promotes to
                    # float64 here where cuDF keeps a nullable integer — the divergence
                    # frame.py names. Every set promotes alike, so lanes stay comparable.
                    masked[key] = np.nan
        piece = _apply(masked, keys, aggs, _partial_frame, state_columns)
        piece.insert(
            len(keys), GROUPING_ID,
            pd.Series([grouping_set_id(mask)] * len(piece), dtype="int64"),
        )
        pieces.append(piece)
    return concatenate(pieces) if len(pieces) > 1 else pieces[0]


def single_over_sets(frame, keys, aggs, masks) -> pd.DataFrame:
    """The oracle: expand, merge every set globally, finalize. Keys gain the id."""
    with_id = keys + [GROUPING_ID]
    return final(partial_over_sets(frame, keys, aggs, masks), with_id, aggs)


# -- an engine plan's aggregates ----------------------------------------------------
#
# An engine plan arrives decomposed: its init node runs sum/count/min/max/mean/m2 over raw
# rows and its merge node sum/min/max/merge_m2 over the state, each call over expressions
# and each output named by the plan, with an explicit `final` where one finalizes. Nothing
# here decides a decomposition — the calls are the plan's, and the math is the one above.


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
    grouped = work.groupby(by, dropna=False, sort=True)
    values = [v for c, call in enumerate(calls) for v in _plan_values(grouped, call.func, c)]
    # No calls is a DISTINCT: the groups themselves are the output.
    table = pd.concat(values, axis=1) if values else pd.DataFrame(index=grouped.size().index)
    out = table.reset_index(drop=not keys)
    return normalize(out.set_axis(range(out.shape[1]), axis=1))


def _prepare_merge_m2(work, by, c) -> None:
    """The per-row terms of Chan's merge — `_merge_frame`'s formula, vectorized: each
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
        return [first.sum(min_count=1)]
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
