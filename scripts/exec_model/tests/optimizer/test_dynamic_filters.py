"""Dynamic filters, planning half: candidates, the probe plan, the host reducer."""

from __future__ import annotations

if __package__ in (None, ""):  # allow `python scripts/exec_model/tests/optimizer/<file>.py`
    import pathlib as _pathlib, sys as _sys

    _sys.path.insert(0, str(_pathlib.Path(__file__).resolve().parents[4]))
    __package__ = "scripts.exec_model.tests.optimizer"

import pathlib
import tempfile

import pandas as pd
import pyarrow as pa
import pyarrow.parquet as pq

from ..corpus import ParquetTables, execute, execute_lanes
from ..harness import main
from ...optimizer.dynamic_filters import (
    IN_LIMIT, Batching, KeySummary, Pruned, apply, candidates, probe_plan, remap,
    row_group_survives, summarize,
)
from ...plans.engine_nodes import build
from ...plans.engine_plan import parse_plans
from ...optimizer.replan import WithMaterialized

_DIR = tempfile.TemporaryDirectory()
#: facts in four row groups of 3: `d` ordered (a date key), `u` cycling through every group
FACTS = pd.DataFrame({"d": list(range(12)), "v": [10 * i for i in range(12)], "u": [i % 3 for i in range(12)]})
DIM = pd.DataFrame({"dk": list(range(12)), "y": [2000 if 3 <= i <= 5 else 1999 for i in range(12)]})


def tables() -> ParquetTables:
    for name, rows, size in (("f", FACTS, 3), ("dim", DIM, 12)):
        path = pathlib.Path(_DIR.name) / f"{name}.parquet"
        if not path.exists():
            pq.write_table(pa.Table.from_pandas(rows, preserve_index=False), path, row_group_size=size)
    return ParquetTables(pathlib.Path(_DIR.name))


F = "schema=[d:Int64, v:Int64, u:Int64]"
FACT_SCAN = ("GpuLoadParquet: table=f, projections=[d@0, v@1, u@2], "
             f"partition_groups=[[[0,1,2,3]]], lanes=1, batches=multiple, {F}")
DIM_BUILD = """GpuCoalesceAllBatches: lanes=1, batches=single, schema=[dk:Int64]
      GpuFilter: predicate=y@1 = 2000, projection=[dk@0], lanes=1, batches=multiple, schema=[dk:Int64]
        GpuLoadParquet: table=dim, projections=[dk@0, y@1], partition_groups=[[[0]]], lanes=1, batches=multiple, schema=[dk:Int64, y:Int64]"""


def plan(join_type="Inner", key="d@0", probe=FACT_SCAN, build=DIM_BUILD):
    text = f"""== q
GpuUnload
  GpuHashJoin: join_type={join_type}, on=[(dk@0, {key})], projection=[v@2], lanes=1, batches=multiple, schema=[v:Int64]
    {build}
    {probe}
"""
    return parse_plans(text, "test")["q"]


def test_a_filtered_build_prunes_an_ordered_fact_scan_by_its_join_key():
    [found] = candidates(plan(), tables())
    assert (found.scan.fields["table"], found.column, found.row_groups) == ("f", "d", (0, 1, 2, 3))
    # Groups span 0–2, 3–5, 6–8, 9–11: their widths sum to 8 of the 11 the column covers.
    assert found.clustering == 8 / 11


def test_no_candidate_where_the_scan_is_not_ordered_by_the_key():
    # `u` spans every group's whole range: its clustering is the group count, 4.
    assert candidates(plan(key="u@2"), tables()) == []


def test_no_candidate_for_a_join_that_keeps_unmatched_probe_rows():
    assert candidates(plan(join_type="Right"), tables()) == []
    assert candidates(plan(join_type="Full"), tables()) == []


def test_no_candidate_without_a_filter_on_the_build_side():
    unfiltered = """GpuCoalesceAllBatches: lanes=1, batches=single, schema=[dk:Int64, y:Int64]
      GpuLoadParquet: table=dim, projections=[dk@0, y@1], partition_groups=[[[0]]], lanes=1, batches=multiple, schema=[dk:Int64, y:Int64]"""
    assert candidates(plan(build=unfiltered), tables()) == []


def test_no_candidate_on_a_scan_of_one_row_group():
    one = FACT_SCAN.replace("partition_groups=[[[0,1,2,3]]]", "partition_groups=[[[2]]]")
    assert candidates(plan(probe=one), tables()) == []


def test_the_key_lifts_through_a_projection_and_an_inner_join():
    probe = f"""GpuProject: exprs=[v@1, d@0], lanes=1, batches=multiple, schema=[v:Int64, d:Int64]
      GpuHashJoin: join_type=Inner, on=[(u@0, u@2)], projection=[d@1, v@2], lanes=1, batches=multiple, schema=[d:Int64, v:Int64]
        GpuCoalesceAllBatches: lanes=1, batches=single, schema=[u:Int64]
          GpuLoadParquet: table=f, projections=[u@2], partition_groups=[[[0]]], lanes=1, batches=multiple, schema=[u:Int64]
        {FACT_SCAN}"""
    [found] = candidates(plan(key="d@1", probe=probe), tables())
    assert (found.column, found.scan.fields["partition_groups"]) == ("d", "[[[0,1,2,3]]]")


def test_the_probe_plan_returns_the_filtered_keys_and_the_reducer_bounds_them():
    [found] = candidates(plan(), tables())
    keys, _ = execute(build(probe_plan(found), tables()))
    assert sorted(keys["dk"]) == [3, 4, 5]
    assert summarize(keys["dk"]) == summarize(pd.Series([5, 3, 4]))
    assert summarize(pd.Series([5, 3, 4])).values == (3, 4, 5)


def test_the_reducer_drops_the_in_list_past_its_limit_and_reads_no_key_as_none():
    many = summarize(pd.Series(range(IN_LIMIT + 1)))
    assert (many.low, many.high, many.values) == (0, IN_LIMIT, None)
    nothing = summarize(pd.Series([None, None], dtype="float64"))
    assert (nothing.low, nothing.high, nothing.values) == (None, None, ())



# -- after the probes -------------------------------------------------------------


def test_a_row_group_goes_where_its_range_misses_the_keys_or_holds_none_of_them():
    bounded = KeySummary(3, 5, None)
    assert not row_group_survives((6, 8), bounded)
    assert row_group_survives((5, 8), bounded)
    few = KeySummary(3, 9, (3, 9))
    assert not row_group_survives((5, 7), few)  # inside the bounds, but neither value is
    assert row_group_survives(None, bounded)  # no statistics, nothing to reason with
    assert not row_group_survives((0, 100), KeySummary(None, None, ()))  # no key at all


def test_survivors_are_laid_out_as_the_engines_partitioner_lays_them():
    # partition.rs: 10 rows over 2 lanes wants 5 in the first; 3 then 6 is closer than
    # stopping at 3, and 9 is further than 6, so the first lane takes groups 0 and 1.
    assert remap([[[0]], [[1]]], [0, 1, 2, 3], [3, 3, 3, 1], Batching.ONE_PER_LANE) == [[[0, 1]], [[2, 3]]]
    assert remap([[[0]], [[1]]], [0, 1, 2, 3], [3, 3, 3, 1], Batching.ONE_PER_ROW_GROUP) == [
        [[0], [1]], [[2], [3]]]
    assert remap([[[0, 1], [2]], [[3]]], [1, 3], [3, 3, 3, 1], Batching.KEEP_BATCHES) == [[[1]], [[3]]]


def run_on(tables_, kept=None):
    frames = {name: lanes for name, (_, lanes) in (kept or {}).items()}
    return lambda engine_plan: execute(build(engine_plan, WithMaterialized(tables_, frames)))[0]


def probe_on(tables_):
    return lambda engine_plan: execute_lanes(build(engine_plan, tables_))


def test_apply_reads_only_the_row_groups_the_keys_can_reach_and_answers_the_same():
    t = tables()
    pruned, report, kept = apply(plan(), t, Batching.ONE_PER_LANE, probe_on(t))
    assert report == [Pruned("f", "d", (0, 1, 2, 3), (1,), (("d", KeySummary(3, 5, (3, 4, 5))),))]
    assert "partition_groups=[[[1]]]" in "".join(f"{k}={v}" for k, v in pruned.children[0].children[1].fields.items())
    before, after = run_on(t)(plan()), run_on(t, kept)(pruned)
    assert sorted(after["v"]) == sorted(before["v"]) == [30, 40, 50]


def test_the_build_a_probe_plan_made_is_the_one_the_main_plan_reads():
    # Every row group survives, so nothing is pruned — and still the dimension is read once:
    # the main plan's build is the probe's, from memory, laid out as the side was.
    every = DIM_BUILD.replace("predicate=y@1 = 2000", "predicate=y@1 >= 1999")
    original = plan(build=every)
    replanned, [probed], kept = apply(original, tables(), Batching.ONE_PER_LANE, probe_on(tables()))
    assert probed.after == probed.before
    [(name, (side, lanes))] = kept.items()
    assert side is original.children[0].children[0] and sorted(lanes[0]["dk"]) == list(range(12))
    build_side, fact = replanned.children[0].children
    assert (build_side.kind, build_side.fields["name"], fact.fields["partition_groups"]) == \
        ("GpuMemorySource", name, original.children[0].children[1].fields["partition_groups"])
    assert sorted(run_on(tables(), kept)(replanned)["v"]) == sorted(run_on(tables())(original)["v"])


if __name__ == "__main__":
    raise SystemExit(main(globals()))
