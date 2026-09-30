"""Table statistics for the estimator: NDV and strings' mean length from the dataset's sidecar
(`testdata/stats/<dataset>.json`, written by `testdata/gen_stats.py`), everything else — rows,
min/max, null counts — from the parquet footer, over whichever row groups a scan reads.

A missing sidecar, a table or column it lacks, or a table whose file no longer matches the
fingerprint recorded for it is refused, table by table. Never a guess in its place: NDV =
rows is exactly the error the sidecar exists to remove.
"""

from __future__ import annotations

import hashlib
import json
import pathlib
import struct
from dataclasses import dataclass

import pyarrow.parquet as pq

from .errors import StatsError

FORMAT = 1


@dataclass(frozen=True)
class ColumnStats:
    """One column over some row groups: `ndv` and `avg_bytes` — a string's mean length over
    its non-null values, None for any other type — are the whole table's, the rest those
    groups'. `nulls`, `low`, `high` are None where a group's footer does not say."""

    ndv: int
    nulls: int | None
    low: object | None
    high: object | None
    avg_bytes: float | None = None


def fingerprint(path: pathlib.Path, rows: int) -> dict:
    """What `gen_stats.py` records per table: rows, bytes and the sha256 of the footer — the
    file's last `4 + length + 8` bytes, its length the little-endian u32 before the magic."""
    size = path.stat().st_size
    with path.open("rb") as file:
        file.seek(size - 8)
        (length,) = struct.unpack("<I", file.read(4))
        file.seek(size - 8 - length)
        footer = file.read(length)
    return {"rows": rows, "bytes": size, "footer_sha256": hashlib.sha256(footer).hexdigest()}


class Statistics:
    """The statistics of one dataset directory of `<table>.parquet`; the sidecar defaults to
    `<directory>/../stats/<directory name>.json`."""

    def __init__(self, directory: pathlib.Path, sidecar: pathlib.Path | None = None):
        self.directory = pathlib.Path(directory)
        path = sidecar or self.directory.parent / "stats" / f"{self.directory.name}.json"
        if not path.exists():
            raise StatsError(f"no statistics sidecar {path}: generate it with testdata/gen_stats.py")
        document = json.loads(path.read_text())
        if document.get("format") != FORMAT:
            raise StatsError(f"{path}: format {document.get('format')}, this reader knows {FORMAT}")
        self._sidecar, self._tables, self._footers = path, document["tables"], {}

    def rows(self, table: str, row_groups=None) -> int:
        metadata = self._footer(table)
        return sum(metadata.row_group(g).num_rows for g in self._groups(metadata, row_groups))

    def column(self, table: str, column: str, row_groups=None) -> ColumnStats:
        metadata = self._footer(table)
        entry = self._tables[table]["columns"].get(column)
        if entry is None:
            raise StatsError(f"{self._sidecar}: no column {table}.{column}")
        paths = [metadata.schema.column(i).path for i in range(metadata.num_columns)]
        if column not in paths:
            raise StatsError(f"{table}.{column} is not a flat column: its footer has no min/max")
        at = paths.index(column)
        stats = [metadata.row_group(g).column(at).statistics for g in self._groups(metadata, row_groups)]
        nulls = None if any(s is None or not s.has_null_count for s in stats) else sum(s.null_count for s in stats)
        # A group of only NULLs (`num_values` counts non-nulls) has no min/max and bounds
        # nothing; one without statistics could hold any value, so the range is unknown.
        if any(s is None or not s.has_min_max and s.num_values > 0 for s in stats):
            low = high = None
        else:
            bounded = [s for s in stats if s.has_min_max]
            low = min((s.min for s in bounded), default=None)
            high = max((s.max for s in bounded), default=None)
        return ColumnStats(entry["ndv"], nulls, low, high, entry.get("avg_bytes"))

    def composite_ndv(self, table: str, columns) -> int | None:
        """The NDV of a same-table key set, in any order; None where the sidecar did not count it."""
        self._footer(table)
        wanted = sorted(columns)
        return next((k["ndv"] for k in self._tables[table].get("composite", [])
                     if sorted(k["columns"]) == wanted), None)

    def _footer(self, table: str):
        """The table's footer, once its file is checked against the sidecar's fingerprint."""
        if table not in self._footers:
            if table not in self._tables:
                raise StatsError(f"{self._sidecar}: no table {table}")
            path = self.directory / f"{table}.parquet"
            metadata = pq.ParquetFile(path).metadata
            recorded, actual = self._tables[table]["fingerprint"], fingerprint(path, metadata.num_rows)
            if recorded != actual:
                raise StatsError(f"{path} does not match {self._sidecar} (recorded {recorded}, file "
                                 f"{actual}): regenerate the sidecar with testdata/gen_stats.py")
            self._footers[table] = metadata
        return self._footers[table]

    @staticmethod
    def _groups(metadata, row_groups):
        return range(metadata.num_row_groups) if row_groups is None else row_groups
