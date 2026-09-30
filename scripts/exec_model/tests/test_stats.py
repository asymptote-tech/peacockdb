"""Table statistics: NDV from the sidecar, the rest from the footer, and refusal where either
is missing or the file has moved on."""

from __future__ import annotations

if __package__ in (None, ""):  # allow `python scripts/exec_model/tests/<file>.py`
    import pathlib as _pathlib, sys as _sys

    _sys.path.insert(0, str(_pathlib.Path(__file__).resolve().parents[3]))
    __package__ = "scripts.exec_model.tests"

import json
import pathlib
import tempfile

import pyarrow as pa
import pyarrow.parquet as pq

from .harness import main, raises
from ..errors import StatsError
from ..stats import ColumnStats, Statistics, fingerprint

#: four row groups of 3; `n` is NULL throughout the last group and once in the first
FACTS = pa.table({"d": list(range(12)), "n": [None, 1, 2, 3, 4, 5, 6, 7, 8, None, None, None],
                  "a": [i % 2 for i in range(12)], "b": [i % 3 for i in range(12)],
                  "s": ["ab" * (i % 3) for i in range(12)]})
DIM = pa.table({"k": [1, 2, 3]})
#: deliberately not the true counts, so a test can tell which source a number came from
NDV = {"d": 100, "n": 101, "a": 102, "b": 103, "s": 104}


def dataset(facts: pa.Table = FACTS) -> pathlib.Path:
    """A dataset directory with its sidecar at `../stats/<name>.json`, as gen_stats.py lays it."""
    root = pathlib.Path(tempfile.mkdtemp())
    directory = root / "toy.sf1"
    directory.mkdir()
    pq.write_table(facts, directory / "f.parquet", row_group_size=3)
    pq.write_table(DIM, directory / "dim.parquet")
    tables = {
        "f": {"fingerprint": fingerprint(directory / "f.parquet", facts.num_rows),
              "columns": {c: {"ndv": NDV[c], "method": "exact"} | ({"avg_bytes": 2.0} if c == "s" else {})
                          for c in facts.column_names},
              "composite": [{"columns": ["a", "b"], "ndv": 6, "method": "exact"}]},
        "dim": {"fingerprint": fingerprint(directory / "dim.parquet", 3),
                "columns": {"k": {"ndv": 3, "method": "exact"}}},
    }
    (root / "stats").mkdir()
    (root / "stats" / "toy.sf1.json").write_text(json.dumps({"format": 1, "tables": tables}))
    return directory


def test_ndv_comes_from_the_sidecar_and_the_rest_from_the_footer():
    stats = Statistics(dataset())
    assert stats.column("f", "d") == ColumnStats(ndv=100, nulls=0, low=0, high=11)
    assert stats.column("f", "d", row_groups=[1, 2]) == ColumnStats(ndv=100, nulls=0, low=3, high=8)
    assert (stats.rows("f"), stats.rows("f", row_groups=[0, 3])) == (12, 6)


def test_a_string_column_carries_its_mean_length_and_others_none():
    stats = Statistics(dataset())
    assert (stats.column("f", "s").avg_bytes, stats.column("f", "d").avg_bytes) == (2.0, None)


def test_a_group_of_only_nulls_counts_its_nulls_and_bounds_nothing():
    stats = Statistics(dataset())
    assert stats.column("f", "n") == ColumnStats(ndv=101, nulls=4, low=1, high=8)
    assert stats.column("f", "n", row_groups=[3]) == ColumnStats(ndv=101, nulls=3, low=None, high=None)


def test_a_composite_key_is_found_in_any_order_and_an_uncounted_one_is_none():
    stats = Statistics(dataset())
    assert stats.composite_ndv("f", ["b", "a"]) == stats.composite_ndv("f", ["a", "b"]) == 6
    assert stats.composite_ndv("f", ["a", "d"]) is None


def test_no_sidecar_is_refused_not_guessed():
    directory = dataset()
    (directory.parent / "stats" / "toy.sf1.json").unlink()
    with raises(StatsError, match="gen_stats.py"):
        Statistics(directory)


def test_a_file_that_moved_on_is_refused_and_its_neighbours_still_answer():
    directory = dataset()
    changed = FACTS.set_column(0, "d", pa.array(list(range(1, 13))))
    pq.write_table(changed, directory / "f.parquet", row_group_size=3)
    stats = Statistics(directory)
    with raises(StatsError, match="f.parquet does not match"):
        stats.column("f", "d")
    with raises(StatsError, match="f.parquet does not match"):
        stats.rows("f")
    assert stats.column("dim", "k").ndv == 3


def test_a_table_or_column_the_sidecar_lacks_is_refused():
    directory = dataset()
    pq.write_table(DIM, directory / "extra.parquet")
    stats = Statistics(directory)
    with raises(StatsError, match="no table extra"):
        stats.rows("extra")
    with raises(StatsError, match="no column f.z"):
        stats.column("f", "z")


if __name__ == "__main__":
    raise SystemExit(main(globals()))
