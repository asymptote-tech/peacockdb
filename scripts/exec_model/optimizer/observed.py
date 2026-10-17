"""NDV of intermediate results, as the planner learns it mid-query.

A build side at `build_done` is materialized, one batch per lane, so its distinct keys can be
counted exactly — one `cudf::distinct_count` per lane, never a cross-lane pass. Whether the
per-lane counts add up to the whole depends on the layout: lanes hashed on columns among those
counted hold disjoint values and the sum is exact; otherwise the NDV lies between the largest
lane's count and the sum. Every build side of every golden plan is on one lane or hashed on
its own join keys, so a join's own key NDV is always exact; bounds are for the build's other
columns, keys of joins further up.

A stream nothing materializes is only estimated: it has at most as many values as rows.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Sequence

import pandas as pd


@dataclass(frozen=True)
class NdvBounds:
    low: int
    high: int

    @property
    def exact(self) -> bool:
        return self.low == self.high


def observed_ndv(lanes: Sequence[pd.DataFrame], columns: Sequence[str],
                 hashed_on: Sequence[str] | None) -> NdvBounds:
    """The NDV of `columns` over a build side's lanes, from per-lane counts alone. A row with a
    NULL among `columns` is no value, as in a join key. `hashed_on` is the lanes' hash columns,
    None where they are not hash-placed."""
    counts = [len(lane[list(columns)].dropna().drop_duplicates()) for lane in lanes]
    disjoint = sum(1 for n in counts if n) <= 1 or (hashed_on is not None and set(hashed_on) <= set(columns))
    return NdvBounds(sum(counts) if disjoint else max(counts, default=0), sum(counts))


def stream_ndv(ndv_in: int, rows_out: int) -> int:
    """A stream's NDV after an operator that emits `rows_out` rows: no more than it had, nor
    than it has rows."""
    return min(ndv_in, rows_out)
