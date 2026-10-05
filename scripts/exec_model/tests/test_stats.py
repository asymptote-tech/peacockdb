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
from decimal import Decimal

import pyarrow as pa
import pyarrow.parquet as pq

from .harness import main, raises
from ..errors import StatsError
from ..stats import FORMAT, ColumnStats, Statistics, fingerprint, flat_columns

#: four row groups of 3; `n` is NULL throughout the last group and once in the first
FACTS = pa.table({"d": list(range(12)), "n": [None, 1, 2, 3, 4, 5, 6, 7, 8, None, None, None],
                  "a": [i % 2 for i in range(12)], "b": [i % 3 for i in range(12)],
                  "s": ["ab" * (i % 3) for i in range(12)]})
DIM = pa.table({"k": [1, 2, 3]})
VECTOR = pa.list_(pa.float32())
#: deliberately not the true counts, so a test can tell which source a number came from
NDV = {"d": 100, "n": 101, "a": 102, "b": 103, "s": 104, "p": 105, "q": 106, "w": 107, "x": 108}


def with_vectors(facts: pa.Table, width: int) -> pa.Table:
    """`facts` with an embedding column of `width` floats per row, as the tpch generator appends."""
    values = [[float(i * width + j) for j in range(width)] for i in range(facts.num_rows)]
    return facts.append_column("v", pa.array(values, VECTOR))


def dataset(facts: pa.Table = FACTS) -> pathlib.Path:
    """A dataset directory with its sidecar at `../stats/<name>.json`, as gen_stats.py lays it."""
    root = pathlib.Path(tempfile.mkdtemp())
    directory = root / "toy.sf1"
    directory.mkdir()
    # Decimals as DuckDB writes them: INT32/INT64 up to precision 18, fixed-length bytes past it.
    pq.write_table(facts, directory / "f.parquet", row_group_size=3, store_decimal_as_integer=True)
    pq.write_table(DIM, directory / "dim.parquet")
    footer = pq.read_metadata(directory / "f.parquet")
    tables = {
        "f": {"fingerprint": fingerprint(footer),
              "columns": {c: {"ndv": NDV[c], "method": "exact"} | ({"avg_bytes": 2.0} if c == "s" else {})
                          for c in flat_columns(footer)},
              "composite": [{"columns": ["a", "b"], "ndv": 6, "method": "exact"}]},
        "dim": {"fingerprint": fingerprint(pq.read_metadata(directory / "dim.parquet")),
                "columns": {"k": {"ndv": 3, "method": "exact"}}},
    }
    (root / "stats").mkdir()
    (root / "stats" / "toy.sf1.json").write_text(json.dumps({"format": FORMAT, "tables": tables}))
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


def test_a_decimal_s_bounds_are_read_whatever_its_storage():
    prices = [Decimal(f"{i - 4}.{i:02d}") for i in range(12)]
    facts = FACTS
    for name, precision in (("p", 7), ("q", 15), ("w", 25)):
        facts = facts.append_column(name, pa.array(prices, pa.decimal128(precision, 2)))
    wide = [Decimal(f"9{i:035d}.{i:02d}") for i in range(12)]  # 38 digits, past the default context's 28
    stats = Statistics(dataset(facts.append_column("x", pa.array(wide, pa.decimal128(38, 2)))))
    for name in "pqw":
        assert (stats.column("f", name).low, stats.column("f", name).high) == (Decimal("-4.00"), Decimal("7.11"))
        assert stats.column("f", name, row_groups=[1]).low == Decimal("-1.03")
    assert (stats.column("f", "x").low, stats.column("f", "x").high) == (wide[0], wide[-1])


def test_a_vector_column_is_not_a_flat_column():
    path = pathlib.Path(tempfile.mkdtemp()) / "f.parquet"
    pq.write_table(with_vectors(FACTS, 8), path)
    assert flat_columns(pq.read_metadata(path)) == FACTS.column_names


def test_a_file_whose_vectors_alone_changed_still_matches():
    directory = dataset(with_vectors(FACTS, 8))
    pq.write_table(with_vectors(FACTS, 96), directory / "f.parquet", row_group_size=3)
    stats = Statistics(directory)
    assert (stats.column("f", "d").ndv, stats.rows("f")) == (100, 12)


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
