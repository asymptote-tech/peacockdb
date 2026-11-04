"""The committed statistics sidecars against the generated sf1 datasets: `gen_stats.py` counts
each of them again byte for byte, and the reader serves every column it lists.

The recount is what catches a stale NDV: data can change with no row group's null count, min or
max moving, and then the fingerprint the reader checks is unchanged too. Needs
`testdata/{tpch,tpcds}.sf1` and the DuckDB CLI (`DUCKDB`, or `duckdb` on the PATH). Red means
the data generator moved and the sidecar did not — regenerate it with `testdata/gen_stats.py`.
"""

from __future__ import annotations

if __package__ in (None, ""):  # allow `python scripts/exec_model/tests/optimizer/<file>.py`
    import pathlib as _pathlib, sys as _sys

    _sys.path.insert(0, str(_pathlib.Path(__file__).resolve().parents[4]))
    __package__ = "scripts.exec_model.tests.optimizer"

import json
import os
import pathlib
import subprocess
import sys
import tempfile

from ..harness import main
from ..plans.test_engine_answers import ROOT
from ...optimizer.stats import Statistics


def run(command: list) -> None:
    done = subprocess.run([str(c) for c in command], capture_output=True, text=True,
                          env=os.environ | {"PYTHON": sys.executable})
    assert done.returncode == 0, f"{' '.join(map(str, command))} failed:\n{done.stdout[-2000:]}{done.stderr[-2000:]}"


def assert_counts_the_committed_sidecar(bench: str, data: pathlib.Path) -> None:
    """`gen_stats.py` over `data` writes `testdata/stats/<bench>.sf1.json` byte for byte."""
    committed = ROOT / "stats" / f"{bench}.sf1.json"
    with tempfile.TemporaryDirectory() as scratch:
        generated = pathlib.Path(scratch) / committed.name
        run([sys.executable, ROOT / "gen_stats.py", "--bench", bench, "--sf", "1", "--data-dir", data,
             "--out", generated])
        got, want = (json.loads(p.read_text())["tables"] for p in (generated, committed))
        differ = sorted(t for t in got.keys() | want.keys() if got.get(t) != want.get(t))
        assert not differ, f"tables whose entries differ from {committed}: {differ}"
        assert generated.read_bytes() == committed.read_bytes()


def _recount_test(bench: str):
    def test():
        assert_counts_the_committed_sidecar(bench, ROOT / f"{bench}.sf1")

    test.__name__ = f"test_the_{bench}_sidecar_is_what_gen_stats_counts_from_its_data"
    return test


def _served_test(bench: str):
    def test():
        directory = ROOT / f"{bench}.sf1"
        stats = Statistics(directory)
        tables = json.loads((ROOT / "stats" / f"{bench}.sf1.json").read_text())["tables"]
        assert sorted(tables) == sorted(p.stem for p in directory.glob("*.parquet"))
        for table, entry in tables.items():
            assert stats.rows(table) == entry["fingerprint"]["rows"]
            for column in entry["columns"]:
                served = stats.column(table, column)
                assert served.low is None or served.low <= served.high, (table, column)
            for key in entry.get("composite", []):
                assert stats.composite_ndv(table, reversed(key["columns"])) == key["ndv"]

    test.__name__ = f"test_every_column_of_the_{bench}_sidecar_is_served"
    return test


for _bench in ("tpch", "tpcds"):
    for _test in (_recount_test(_bench), _served_test(_bench)):
        globals()[_test.__name__] = _test

if __name__ == "__main__":
    raise SystemExit(main(globals()))
