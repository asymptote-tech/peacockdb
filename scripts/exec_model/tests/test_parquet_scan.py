"""An engine scan: a parquet file's own row groups, read as the planner mapped them."""

from __future__ import annotations

if __package__ in (None, ""):  # allow `python scripts/exec_model/tests/<file>.py`
    import pathlib as _pathlib, sys as _sys

    _sys.path.insert(0, str(_pathlib.Path(__file__).resolve().parents[3]))
    __package__ = "scripts.exec_model.tests"

import itertools
import pathlib
import tempfile

import pandas as pd
import pyarrow as pa
import pyarrow.parquet as pq

from .corpus import execute, row_group_rows
from .harness import main, raises
from ..operators import nodes as N
from ..operators import source


_DIR = tempfile.TemporaryDirectory()
_FILES = itertools.count()


def written(rows=10, row_group_size=3) -> pathlib.Path:
    """A file whose row groups are 3, 3, 3 and 1 rows: `id` is the row's position."""
    path = pathlib.Path(_DIR.name) / f"t{next(_FILES)}.parquet"
    table = pa.table({"id": list(range(rows)), "tag": [f"r{i}" for i in range(rows)]})
    pq.write_table(table, path, row_group_size=row_group_size)
    return path


def scan(path, partition_groups, columns=("id", "tag"), limit=None):
    frame = pq.read_table(path, columns=list(columns)).to_pandas()
    return N.parquet_scan("t", frame, row_group_rows(path), partition_groups, limit)


def lane_batches(node, lane) -> list[list]:
    executor = node.make_executors().backends.cpu(lane)
    out = []
    while (produced := executor.next_batch()) is not None:
        out.append(produced[0].consume())
    return out


def test_the_row_group_sizes_come_from_the_files_metadata():
    assert row_group_rows(written()) == [3, 3, 3, 1]


def test_a_lane_reads_its_mapped_row_groups_as_its_batches():
    node = scan(written(), [[[0], [1, 2]], [[3]], []])
    assert node.output_partitions().n == 3
    assert [list(b["id"]) for b in lane_batches(node, 0)] == [[0, 1, 2], [3, 4, 5, 6, 7, 8]]
    assert [list(b["id"]) for b in lane_batches(node, 1)] == [[9]]
    assert lane_batches(node, 2) == []


def test_a_plan_over_the_scan_reads_every_mapped_row_once():
    root = N.unload("u", N.merge_partitions("m", scan(written(), [[[0], [1, 2]], [[3]], []])))
    got, _ = execute(root)
    assert sorted(got["id"]) == list(range(10))


def test_a_scan_limit_stops_the_lane_once_it_has_its_rows():
    batches = lane_batches(scan(written(), [[[0, 1], [2, 3]]], limit=4), 0)
    assert [list(b["id"]) for b in batches] == [[0, 1, 2, 3]]


def test_a_scan_of_no_columns_still_counts_its_rows():
    batches = lane_batches(scan(written(), [[[0], [1, 2]]], columns=()), 0)
    assert [(len(b), len(b.columns)) for b in batches] == [(3, 0), (6, 0)]


def test_a_batch_fetched_ahead_decodes_as_the_one_read_in_turn():
    # Batches of two row groups and one, a limit that ends mid-batch, empty batches between.
    frame = pd.DataFrame({"k": range(10)})
    groups = source.row_group_ranges([2, 2, 3, 3])
    def lane(**kwargs):
        return source.TableSource(frame, groups, [[0, 1], [2], [3]], "t", 0, **kwargs)

    for kwargs in ({}, {"limit": 6}, {"empty_probability": 0.5, "seed": 3}):
        plain, ahead = lane(**kwargs), lane(**kwargs)
        want, got = [], []
        while (batch := plain.next_batch()) is not None:
            want.append(list(batch[0].frame["k"]))
        while True:
            if ahead.can_prefetch():
                assert ahead.prefetch() > 0
                assert not ahead.can_prefetch()  # one batch ahead, not two
            batch = ahead.next_batch()
            if batch is None:
                break
            got.append(list(batch[0].frame["k"]))
        assert got == want, kwargs
        assert ahead.reads == plain.reads == sum(1 for batch in want if batch), kwargs  # each read once


def test_a_mapping_the_file_cannot_hold_is_refused():
    path = written()
    frame = pq.read_table(path).to_pandas()
    with raises(ValueError, match=r"row groups hold 9 rows, the frame 10"):
        N.parquet_scan("t", frame, [3, 3, 3], [[[0]]])
    with raises(ValueError, match=r"a scan limit over 2 lanes"):
        N.parquet_scan("t", frame, row_group_rows(path), [[[0]], [[1]]], limit=5)


if __name__ == "__main__":
    raise SystemExit(main(globals()))
