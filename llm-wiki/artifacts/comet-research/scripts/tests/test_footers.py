"""Tests for footers.py: per-column chunk sizes summed from a Parquet footer, read by range."""
import io
import sys
from pathlib import Path

import pyarrow as pa
import pyarrow.parquet as pq

SCRIPTS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(SCRIPTS))

import footers  # noqa: E402


def two_row_group_file(path):
    t = pa.table({"a": [i * 7919 % 1000003 for i in range(200_000)], "b": ["x" * 10] * 200_000})
    pq.write_table(t, path, row_group_size=100_000)
    return pq.ParquetFile(path).metadata


def test_column_sizes_sum_every_row_group(tmp_path):
    md = two_row_group_file(tmp_path / "t.parquet")
    rows = footers.column_sizes(md)
    want_a = sum(md.row_group(i).column(0).total_compressed_size for i in range(2))
    assert rows == [
        ("a", 2, want_a, sum(md.row_group(i).column(0).total_uncompressed_size for i in range(2))),
        ("b", 2, sum(md.row_group(i).column(1).total_compressed_size for i in range(2)),
         sum(md.row_group(i).column(1).total_uncompressed_size for i in range(2))),
    ]


def test_range_file_reads_only_the_ranges_asked(tmp_path):
    data = (tmp_path / "t.parquet")
    two_row_group_file(data)
    blob = data.read_bytes()
    asked = []

    def get(start, end):
        asked.append((start, end))
        return blob[start:end + 1]

    md = pq.ParquetFile(footers.RangeFile(len(blob), get)).metadata
    assert md.num_rows == 200_000
    assert sum(e - s + 1 for s, e in asked) < len(blob)
