"""Running plans, and reading the generated datasets for the engine-plan files.

`execute` runs a prototype plan. The engine-plan files read the sf1 tables through
`ParquetTables` and compare with DuckDB through `duckdb_answer` and `matches_oracle`; a missing
table **fails**, naming the generator, since a skipped suite reads like a passing one. Decimals
are read as float64 and dates as `datetime64[ns]` (`_typed`), standing in for the precision,
scale and `TIMESTAMP_DAYS` typing the prototype does not have. Tables are read whole; the
README's "The corpus" says why a sample will not do.
"""

from __future__ import annotations

import functools
import os
import pathlib
import re
import shutil
import subprocess
import tempfile

import numpy as np
import pandas as pd
import pyarrow as pa
import pyarrow.parquet as pq

from ..partitioned_driver import partitioned_driver
from ..node import CpuBackendSelector
from ..operators.frame import concatenate
from ..plan import Plan

#: `execute`'s default: far from unbounded, so the accountant is exercised on every call and
#: a blown resident set fails the test.
BUDGET = 256 * 1024 * 1024

#: What a corpus query runs under. The legacy tiers' "mini" device is 2 GiB and this is the
#: same number, which is the point: a row costs more in pandas than in cuDF, so a budget
#: that binds here binds harder than the GPU's would — a plan that fits is not flattered.
CORPUS_BUDGET = 2 * 1024 * 1024 * 1024

_ROOT = pathlib.Path(__file__).resolve().parents[3] / "testdata"

#: bench → the directories to try, in order. Only TPC-H has a committed fallback.
_DATASETS = {
    "tpch": ("tpch.sf1", "tpch.minimal"),
    "tpcds": ("tpcds.sf1",),
}


def dataset_dir(bench: str, name: str) -> pathlib.Path:
    """The directory holding `name`, or a failure that says how to make one."""
    for candidate in _DATASETS[bench]:
        if (_ROOT / candidate / f"{name}.parquet").exists():
            return _ROOT / candidate
    raise FileNotFoundError(
        f"no {name}.parquet under {' or '.join(_DATASETS[bench])} in {_ROOT}. "
        "The corpus tests need the generated sf1 tables: run testdata/generate_testdata.sh "
        "(exec-model-corpus.yml does this before running the engine-plan files)."
    )


def row_group_rows(path: pathlib.Path) -> list[int]:
    """A parquet file's row groups' sizes, file order — what an engine scan's numbers index."""
    metadata = pq.ParquetFile(path).metadata
    return [metadata.row_group(i).num_rows for i in range(metadata.num_row_groups)]


class ParquetTables:
    """An engine plan's tables, out of one directory of `<table>.parquet`: what
    `engine_nodes.build` asks of a scan — its columns, typed as `_typed` types them, and its
    file's row-group sizes."""

    def __init__(self, directory: pathlib.Path):
        self.directory = pathlib.Path(directory)

    def frame(self, table: str, columns) -> pd.DataFrame:
        return _typed(pq.read_table(self.directory / f"{table}.parquet", columns=list(columns)))

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


def _typed(arrow: pa.Table) -> pd.DataFrame:
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


# -- the oracle -------------------------------------------------------------------


def duckdb_binary() -> str:
    """A DuckDB CLI, or a failure saying where one comes from."""
    for candidate in (os.environ.get("DUCKDB"), "duckdb",
                      str(pathlib.Path.home() / ".duckdb/cli/latest/duckdb")):
        found = shutil.which(candidate) if candidate else None
        if found:
            return found
    raise FileNotFoundError(
        "no duckdb binary for the corpus oracle: set $DUCKDB, or install the CLI. CI's "
        "corpus workflow downloads the pinned v1.5.4 release the goldens are generated with."
    )


@functools.lru_cache(maxsize=32)
def duckdb_answer(bench: str, query: str) -> pd.DataFrame:
    """The query's **own text**, run by DuckDB over the same parquet files.

    An oracle that is neither this prototype nor pandas. A hand-written pandas equivalent
    only catches what the *mode* gets wrong — batching, lanes, a finish pass — because both
    sides would share one reading of the SQL; this catches the reading too, which is the
    circularity #80 complains of in the legacy tiers.

    The answer comes back as **parquet**, not CSV: CSV carries no types, so a column of
    surrogate keys came back as float, a zip code came back as an integer, and the compare
    was then arguing about the transport rather than the answer. Read with the same
    conversions `ParquetTables` applies to the tables, so both sides of the compare are typed
    alike.
    """
    sql = (_ROOT / f"{bench}-queries" / f"{query}.sql").read_text().strip()
    directory = dataset_dir(bench, "lineitem" if bench == "tpch" else "store_sales")
    views = "".join(
        f"CREATE VIEW {path.stem} AS SELECT * FROM read_parquet('{path}');\n"
        for path in sorted(directory.glob("*.parquet"))
    )
    with tempfile.TemporaryDirectory() as scratch:
        answer = pathlib.Path(scratch) / "answer.parquet"
        subprocess.run(
            [duckdb_binary(), "-c",
             f"{views}COPY ({sql.rstrip().rstrip(';')}) TO '{answer}' (FORMAT PARQUET);"],
            capture_output=True, text=True, check=True,
        )
        return _typed(pq.read_table(answer))


def _compare_columns(got: pd.DataFrame, want: pd.DataFrame, columns, label: str, what: str):
    """Column by column, aligning only what the transport could not carry.

    A date is compared against the engine's own dtype. Money is compared with a tolerance,
    since summing six million floats in a different order moves the last bits.
    """
    for column in columns:
        left, right = got[column], want[column]
        if pd.api.types.is_datetime64_any_dtype(left):
            right = pd.to_datetime(right)
        if pd.api.types.is_numeric_dtype(left) and pd.api.types.is_numeric_dtype(right):
            assert np.allclose(
                left.to_numpy(dtype=float), right.to_numpy(dtype=float),
                rtol=1e-9, atol=1e-2, equal_nan=True,
            ), f"{label}: {what} {column} differs from DuckDB"
        else:
            def nulls_alike(values):
                return [None if pd.isna(v) else v for v in values]

            assert nulls_alike(left) == nulls_alike(right), (
                f"{label}: {what} {column} differs from DuckDB"
            )


def _canonical(frame: pd.DataFrame, inexact) -> pd.DataFrame:
    """One frame's rows in an order that depends on nothing but their values.

    Exact columns lead and the inexact ones follow, rounded before they are sorted on: two
    engines that summed the same money in a different order disagree in the last bits, and
    an unrounded sort key would let that noise reorder the rows it is meant to align.
    Rounding is only for the ordering — the values themselves are still compared at full
    precision, with a tolerance.

    `inexact` comes from both frames at once and not from this one. A column of surrogate
    keys arrives from parquet as float where some are null and from DuckDB's CSV as int, and
    a per-frame split would then sort the two sides by different column orders — which
    looked exactly like a wrong answer the first time it happened.
    """
    keys = frame.copy()
    for column in inexact:
        keys[column] = pd.to_numeric(keys[column], errors="coerce").round(4)
    order = [c for c in keys.columns if c not in inexact] + list(inexact)
    return frame.loc[
        keys.sort_values(order, kind="stable", na_position="first").index
    ].reset_index(drop=True)


def matches_oracle(got: pd.DataFrame, want: pd.DataFrame, label: str, order_by=None):
    """Compare an engine result against DuckDB's, exactly as strictly as SQL allows.

    Column names and their order are asserted rather than aligned: they are the query's
    declared output, and a plan that renamed or reordered them answered a different query.

    Two checks, because `ORDER BY` promises less than a positional compare asserts. The
    rows are compared as a **multiset** — both sides canonically sorted — which is the whole
    of what the query determines. Then the `order_by` columns are compared **positionally**,
    which is the whole of what the sort determines: where the sort keys tie, SQL leaves the
    order of those rows open and two correct engines may disagree. Comparing full rows
    positionally would make a tie into a failure, which is how tpch q11 first failed.

    `order_by` is the query's ORDER BY as output column names; a query with no ORDER BY
    passes None and gets the multiset check alone.
    """
    assert list(got.columns) == list(want.columns), (
        f"{label}: {list(got.columns)} vs the query's {list(want.columns)}"
    )
    assert len(got) == len(want), f"{label}: {len(got)} rows vs DuckDB's {len(want)}"
    if order_by:
        missing = [column for column in order_by if column not in want.columns]
        assert not missing, f"{label}: ORDER BY names {missing}, which the output has not"
        _compare_columns(got, want, order_by, label, "sort key")
    inexact = [
        column for column in want.columns
        if pd.api.types.is_float_dtype(got[column]) or pd.api.types.is_float_dtype(want[column])
    ]
    _compare_columns(_canonical(got, inexact), _canonical(want, inexact),
                     list(want.columns), label, "column")


# -- the engine's CPU run ---------------------------------------------------------


def _sections(path: pathlib.Path) -> dict:
    """The file split by its `== <query>` headers."""
    if not path.exists():
        return {}
    sections, current = {}, None
    for line in path.read_text().splitlines(keepends=True):
        if line.startswith("== "):
            current = line[3:].strip()
            sections[current] = line
        elif current is not None:
            sections[current] += line
    return sections


def cpu_rows(path: pathlib.Path) -> dict[str, list[tuple[str, int]]]:
    """A `<mode>-<tier>.cpu.txt` golden as each query's nodes in pre-order, `(kind, output_rows)`
    — the engine's own CPU run over the whole dataset."""
    rows = {}
    for query, text in _sections(path).items():
        rows[query] = [(match[1], int(match[2])) for match in
                       re.finditer(r"^\s*(Gpu\w+)\b.*?\boutput_rows=(\d+)", text, re.MULTILINE)]
    return rows


# -- running a plan ---------------------------------------------------------------


def execute(root, budget: int | None = BUDGET):
    driver = partitioned_driver(Plan.build(root), CpuBackendSelector(), budget)
    driver.run()
    # The plan's own concatenate, not pandas': it keeps the first batch's schema when every
    # batch is empty, and a query whose answer is the empty set still has to report its
    # columns. TPC-DS q17 is that query — dropping the empty batches here left a frame with
    # no columns at all, which reads as a plan that produced the wrong ones.
    frames = [batch.frame for batch in driver.results]
    got = concatenate(frames) if frames else pd.DataFrame()
    return got, driver


def execute_lanes(root, budget: int | None = BUDGET) -> list[pd.DataFrame]:
    """`root` run, its output by lane — a build side's batch per lane, as a probe plan keeps it."""
    driver = partitioned_driver(Plan.build(root), CpuBackendSelector(), budget)
    driver.run()
    return [concatenate([batch.frame for batch in lane]) for lane in driver.root_lanes]
