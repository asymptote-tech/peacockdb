"""The cost of one call on the device, by kind: `fixed + slope × volume`, fitted from the
benchmark's calibration record — tpch sf40's, fetched into `RECORD` by `fetch_record.sh tpch.sf40`.

What `cost.py`'s C_out cannot see: a plan of the same bytes in more calls pays each call's fixed
part again, and on a GPU that part is large — a parquet scan costs ~45 ms however small its row
group, so reading one row group per call is two orders of magnitude slower than one call per lane.
It would decide lanes, batch size and shuffles; no rule reads it yet. The volume is the one that
predicts a kind best: a scan's decoded output bytes, an aggregate's input rows, anything else's
input bytes. A call is fitted as the median of its measured runs; the fit minimizes relative
error, so small calls weigh as much. A forwarder makes no call and costs nothing.
"""

from __future__ import annotations

import pathlib
from dataclasses import dataclass

import numpy as np
import pandas as pd

RECORD = pathlib.Path(__file__).resolve().parents[3] / "testdata" / "calibration" / "tpch.sf40" / "records.tsv"
#: the volume each kind is priced by; any other kind by its input bytes
VOLUME = {"CudfScan": "out_bytes", "CudfAggregate{Partial}": "in_rows", "CudfAggregate{Merge}": "in_rows"}
_CALL = ["query", "mode", "node_seq", "lane", "recipe_seq", "recipe_kind", "call_index"]


@dataclass(frozen=True)
class CallCost:
    """Microseconds for a call of `volume` units of `unit`."""

    fixed_us: float
    per_unit_us: float
    unit: str

    def __call__(self, volume: float) -> float:
        return self.fixed_us + self.per_unit_us * volume


def calls(record: pathlib.Path = RECORD) -> pd.DataFrame:
    """One row per call — its measured runs' median time — with the volume its kind is priced by."""
    rows = pd.read_csv(record, sep="\t", comment="#")
    found = rows.groupby(_CALL, as_index=False).agg(
        device_us=("device_us", "median"), in_rows=("in_rows", "first"),
        in_bytes=("in_bytes", "first"), out_bytes=("out_bytes", "first"))
    found["volume"] = [getattr(row, VOLUME.get(row.recipe_kind, "in_bytes")) for row in found.itertuples()]
    return found


def fit(record: pathlib.Path = RECORD) -> dict[str, CallCost]:
    """Every kind of call in the record, fitted. A kind measured at one volume only has no slope
    to find, and is its median time."""
    return {kind: _fit(group, VOLUME.get(kind, "in_bytes")) for kind, group in calls(record).groupby("recipe_kind")}


def _fit(group: pd.DataFrame, unit: str) -> CallCost:
    x, y = group.volume.to_numpy(dtype=float), group.device_us.to_numpy(dtype=float)
    if len(set(x)) < 2:
        return CallCost(float(np.median(y)), 0.0, unit)
    weight = 1 / np.maximum(y, 1.0)
    design = np.vstack([np.ones_like(x), x]).T * weight[:, None]
    fixed, slope = np.linalg.lstsq(design, y * weight, rcond=None)[0]
    return CallCost(max(float(fixed), 0.0), max(float(slope), 0.0), unit)
