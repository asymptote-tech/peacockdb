"""Reading the generated datasets for the engine-plan files, and DuckDB's answer.

The engine-plan files read the sf1 tables through `ParquetTables` (`plans/tables.py`) and
compare with DuckDB through `duckdb_answer` and `matches_oracle` (`plans/answers.py`); a missing
table **fails**, naming the generator, since a skipped suite reads like a passing one. Tables
are read whole; the README's "The corpus" says why a sample will not do. Running a plan is
`plans/engine_run.py`'s; `execute` here gives it the tests' default budget.
"""

from __future__ import annotations

import functools
import os
import pathlib
import re
import shutil
import subprocess
import tempfile

import pandas as pd
import pyarrow.parquet as pq

from ..plans import engine_run
from ..plans.answers import matches_oracle
from ..plans.engine_run import CORPUS_BUDGET
from ..plans.tables import ParquetTables, row_group_rows, typed

#: `execute`'s default: far from unbounded, so the accountant is exercised on every call and
#: a blown resident set fails the test.
BUDGET = 256 * 1024 * 1024

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
        return typed(pq.read_table(answer))


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
    """`engine_run.execute` under the tests' default budget."""
    return engine_run.execute(root, budget)


def execute_lanes(root, budget: int | None = BUDGET):
    """`engine_run.execute_lanes` under the tests' default budget."""
    return engine_run.execute_lanes(root, budget)
