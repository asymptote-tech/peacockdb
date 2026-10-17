"""`ParquetTables`: a scan's columns read once for the life of the tables — a replanned run builds
every scan again — and each caller handed a frame of its own."""

from __future__ import annotations

if __package__ in (None, ""):  # allow `python scripts/exec_model/tests/plans/<file>.py`
    import pathlib as _pathlib, sys as _sys

    _sys.path.insert(0, str(_pathlib.Path(__file__).resolve().parents[4]))
    __package__ = "scripts.exec_model.tests.plans"

import pathlib
import tempfile

import pyarrow as pa
import pyarrow.parquet as pq

from ..harness import main
from ...plans.tables import ParquetTables


def test_a_table_s_columns_are_read_once_and_each_caller_gets_a_frame_to_rename():
    with tempfile.TemporaryDirectory() as scratch:
        directory = pathlib.Path(scratch)
        pq.write_table(pa.table({"a": [1, 2], "b": ["x", "y"]}), directory / "t.parquet")
        tables = ParquetTables(directory)
        first = tables.frame("t", ["a", "b"])
        # The file changes under it: the second scan still sees what the first read.
        pq.write_table(pa.table({"a": [9], "b": ["z"]}), directory / "t.parquet")
        first.columns = ["renamed", "too"]
        second = tables.frame("t", ["a", "b"])
        assert list(second.columns) == ["a", "b"] and second["a"].tolist() == [1, 2]
        assert tables.frame("t", ["a"])["a"].tolist() == [9]
        assert ParquetTables(directory).frame("t", ["a", "b"])["a"].tolist() == [9]


if __name__ == "__main__":
    raise SystemExit(main(globals()))
