"""The committed statistics sidecars against the generated sf1 datasets: every table's file
still matches the fingerprint recorded for it, and every column the sidecar lists is served.

Needs `testdata/{tpch,tpcds}.sf1`; reads footers only. Red means the data generator moved and
the sidecar did not — regenerate it with `testdata/gen_stats.py`.
"""

from __future__ import annotations

if __package__ in (None, ""):  # allow `python scripts/exec_model/tests/<file>.py`
    import pathlib as _pathlib, sys as _sys

    _sys.path.insert(0, str(_pathlib.Path(__file__).resolve().parents[3]))
    __package__ = "scripts.exec_model.tests"

import json

from .harness import main
from .test_engine_answers import ROOT
from ..stats import Statistics


def _sidecar_test(bench: str):
    def test():
        directory = ROOT / f"{bench}.sf1"
        stats = Statistics(directory)
        tables = json.loads((ROOT / "stats" / f"{bench}.sf1.json").read_text())["tables"]
        assert sorted(tables) == sorted(p.stem for p in directory.glob("*.parquet"))
        for table, entry in tables.items():
            assert stats.rows(table) == entry["fingerprint"]["rows"]
            for column, counted in entry["columns"].items():
                if not column.endswith("_embedding"):
                    assert stats.column(table, column).ndv == counted["ndv"]
            for key in entry.get("composite", []):
                assert stats.composite_ndv(table, reversed(key["columns"])) == key["ndv"]

    test.__name__ = f"test_the_{bench}_sidecar_matches_its_data"
    return test


for _bench in ("tpch", "tpcds"):
    _test = _sidecar_test(_bench)
    globals()[_test.__name__] = _test

if __name__ == "__main__":
    raise SystemExit(main(globals()))
