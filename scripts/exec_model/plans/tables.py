"""The generated parquet tables an engine plan's scans read, as `engine_nodes.build` asks for
them: a table's columns, its row-group sizes, its footer ranges. Decimals are read as float64
and dates as `datetime64[ns]` (`typed`), standing in for the precision, scale and
`TIMESTAMP_DAYS` typing the prototype does not have.
"""

from __future__ import annotations

import pathlib

import pandas as pd
import pyarrow as pa
import pyarrow.parquet as pq


def row_group_rows(path: pathlib.Path) -> list[int]:
    """A parquet file's row groups' sizes, file order — what an engine scan's numbers index."""
    metadata = pq.ParquetFile(path).metadata
    return [metadata.row_group(i).num_rows for i in range(metadata.num_row_groups)]


class ParquetTables:
    """An engine plan's tables, out of one directory of `<table>.parquet`: what
    `engine_nodes.build` asks of a scan — its columns, typed as `typed` types them, and its
    file's row-group sizes. Columns are read once for the life of the object, since a replanned
    run builds every scan of its plan again."""

    def __init__(self, directory: pathlib.Path):
        self.directory = pathlib.Path(directory)
        self._read: dict[tuple[str, tuple[str, ...]], pd.DataFrame] = {}

    def frame(self, table: str, columns) -> pd.DataFrame:
        """A frame of the caller's own, which a scan renames; the data is shared, and nothing
        writes into a scan's columns."""
        key = (table, tuple(columns))
        if key not in self._read:
            self._read[key] = typed(pq.read_table(self.directory / f"{table}.parquet", columns=list(columns)))
        return self._read[key].copy(deep=False)

    def row_counts(self, table: str) -> list[int]:
        return row_group_rows(self.directory / f"{table}.parquet")

    def column_ranges(self, table: str, column: str) -> list[tuple | None]:
        """Per row group, the column's (min, max) from the footer; None where it holds none."""
        metadata = pq.ParquetFile(self.directory / f"{table}.parquet").metadata
        at = [metadata.schema.column(i).name for i in range(metadata.num_columns)].index(column)
        ranges = []
        for group in range(metadata.num_row_groups):
            stats = metadata.row_group(group).column(at).statistics
            ranges.append((stats.min, stats.max) if stats is not None and stats.has_min_max else None)
        return ranges


def typed(arrow: pa.Table) -> pd.DataFrame:
    """An arrow table as a frame, with the two casts this file's header names: pandas has no
    decimal dtype, and a `date32` arrives as `datetime.date`, which the expression IR cannot
    compare to.

    Applied to the DuckDB oracle's answer as well as to the tables, so a comparison is
    never about how the two sides were transported.
    """
    frame = arrow.to_pandas()
    for field in arrow.schema:
        if pa.types.is_decimal(field.type):
            frame[field.name] = frame[field.name].astype("float64")
        elif pa.types.is_date(field.type):
            frame[field.name] = pd.to_datetime(frame[field.name])
    return frame
