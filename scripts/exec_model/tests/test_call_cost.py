"""The per-call model: fitted from a record it recovers the fixed part and the slope, and a kind
measured at one volume costs its median time."""

from __future__ import annotations

if __package__ in (None, ""):  # allow `python scripts/exec_model/tests/<file>.py`
    import pathlib as _pathlib, sys as _sys

    _sys.path.insert(0, str(_pathlib.Path(__file__).resolve().parents[3]))
    __package__ = "scripts.exec_model.tests"

import pathlib
import tempfile

import pandas as pd

from .harness import main
from ..call_cost import fit

HEADER = ["dataset", "sf", "query", "mode", "node_seq", "node_type", "lane", "recipe_seq", "recipe_kind",
          "call_index", "run_index", "in_rows", "in_bytes", "out_rows", "out_bytes", "host_us", "device_us"]


def record(rows) -> pathlib.Path:
    path = pathlib.Path(tempfile.mkdtemp()) / "records.tsv"
    frame = pd.DataFrame(rows, columns=HEADER)
    path.write_text("# run: synthetic\n" + frame.to_csv(sep="\t", index=False))
    return path


def call(kind, seq, volume, run, us):
    in_bytes, out_bytes, in_rows = (0, volume, 0) if kind == "CudfScan" else (volume, 0, volume)
    return ["tpch", 40, "q", "m", seq, "N", 0, seq, kind, 0, run, in_rows, in_bytes, 0, out_bytes, us, us]


def test_a_fit_recovers_the_fixed_part_and_the_slope_from_the_runs_medians():
    rows = []
    for seq, volume in enumerate((1_000, 10_000, 100_000)):
        # a scan of 45 ms plus 1 us per 1000 bytes; the third run of each call is an outlier
        for run, noise in enumerate((0, 0, 900_000)):
            rows.append(call("CudfScan", seq, volume, run, 45_000 + volume / 1000 + noise))
    [scan] = fit(record(rows)).values()
    assert abs(scan.fixed_us - 45_000) < 1 and abs(scan.per_unit_us - 0.001) < 1e-6
    assert scan.unit == "out_bytes" and abs(scan(1_000_000) - scan.fixed_us - 1_000) < 1e-3


def test_a_kind_at_one_volume_is_its_median_time():
    rows = [call("CudfSort", 0, 500, run, us) for run, us in enumerate((10, 30, 20))]
    [sort] = fit(record(rows)).values()
    assert (sort.fixed_us, sort.per_unit_us, sort.unit) == (20, 0, "in_bytes")


if __name__ == "__main__":
    raise SystemExit(main(globals()))
