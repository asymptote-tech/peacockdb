"""The engine's plan trees built as prototype plans and run by the prototype's driver."""

from __future__ import annotations

if __package__ in (None, ""):  # allow `python scripts/exec_model/tests/<file>.py`
    import pathlib as _pathlib, sys as _sys

    _sys.path.insert(0, str(_pathlib.Path(__file__).resolve().parents[3]))
    __package__ = "scripts.exec_model.tests"

import pathlib
import tempfile

import pandas as pd
import pyarrow as pa
import pyarrow.parquet as pq

from .corpus import ParquetTables, execute
from .harness import main, raises
from ..engine_nodes import build
from ..engine_plan import parse_plans

_DIR = tempfile.TemporaryDirectory()
ROWS = pd.DataFrame({
    "k": [5, 1, 4, 2, 3, 6, 7],
    "v": [1.0, None, 3.0, 3.0, None, 2.0, 0.5],
    "s": ["a", "x", "b", "c", "d", "x", "e"],
})


OTHER = pd.DataFrame({"k": [1, 2, 2, 9], "w": [10, 20, 21, 90]})


def tables() -> ParquetTables:
    """`t` in row groups of 2, 2, 2 and 1 rows; `u` in two of 2."""
    for table, rows in (("t", ROWS), ("u", OTHER)):
        path = pathlib.Path(_DIR.name) / f"{table}.parquet"
        if not path.exists():
            pq.write_table(pa.Table.from_pandas(rows, preserve_index=False), path, row_group_size=2)
    return ParquetTables(pathlib.Path(_DIR.name))


SCAN = ("GpuLoadParquet: table=t, projections=[k@0, v@1, s@2], "
        "partition_groups=[[[0],[1]],[[2,3]]], lanes=2, batches=multiple, "
        "schema=[k:Int64, v:Float64, s:Utf8]")


T = "schema=[k:Int64, v:Float64, s:Utf8]"
U = "schema=[k:Int64, w:Int64]"


def built(scan: str, schema: str, depth: int = 2) -> str:
    """A one-lane scan coalesced to the single batch a join builds from, at `depth`."""
    return f"GpuCoalesceAllBatches: lanes=1, batches=single, {schema}\n{'  ' * (depth + 1)}{scan}"


#: one lane each
T_ONE = ("GpuLoadParquet: table=t, projections=[k@0, v@1, s@2], "
         "partition_groups=[[[0,1,2,3]]], lanes=1, batches=multiple, " + T)
T_BUILD = built(T_ONE, T)
U_BUILD = built("GpuLoadParquet: table=u, projections=[k@0, w@1], "
                "partition_groups=[[[0,1]]], lanes=1, batches=multiple, " + U, U)
T_PROBE = ("GpuLoadParquet: table=t, projections=[k@0, v@1, s@2], "
           "partition_groups=[[[0],[1,2,3]]], lanes=1, batches=multiple, " + T)
U_PROBE = ("GpuLoadParquet: table=u, projections=[k@0, w@1], "
           "partition_groups=[[[0],[1]]], lanes=1, batches=multiple, " + U)


def join(line: str, build: str, probe: str) -> pd.DataFrame:
    return run(f"GpuUnload\n  {line}, lanes=1, batches=multiple\n    {build}\n    {probe}")


def run(tree: str) -> pd.DataFrame:
    plan = parse_plans(f"== q\n{tree}\n", "test")["q"]
    got, _ = execute(build(plan, tables()))
    return got


def test_a_top_n_orders_each_key_with_its_own_null_placement():
    got = run(f"""GpuUnload: skip=1, fetch=3
  GpuMergeSortedPartitions: by=[v@1 desc nulls_first, k@0 asc nulls_last], lanes=1, batches=single, schema=[k:Int64, v:Float64]
    GpuSort: by=[v@1 desc nulls_first, k@0 asc nulls_last], lanes=2, batches=multiple, schema=[k:Int64, v:Float64]
      GpuFilter: predicate=s@2 != x, projection=[k@0, v@1], lanes=2, batches=multiple, schema=[k:Int64, v:Float64]
        {SCAN}""")
    # v descending with its null first, then k ascending: (3,null) (2,3.0) (4,3.0) (5,1.0) …
    assert list(got.columns) == ["k", "v"]
    assert list(got["k"]) == [2, 4, 5]


def test_rows_reach_the_root_through_a_shuffle_a_union_and_a_limit():
    # A limit feeding only the sink is the unload's interval, so this one sits under a project.
    got = run(f"""GpuUnload
  GpuProject: exprs=[k@0], lanes=1, batches=multiple, schema=[k:Int64]
    GpuLimit: skip=2, fetch=100, lanes=1, batches=multiple, schema=[k:Int64, v:Float64, s:Utf8]
      GpuMergePartitions: lanes=1, batches=multiple, schema=[k:Int64, v:Float64, s:Utf8]
        GpuUnion: lanes=4, batches=multiple, schema=[k:Int64, v:Float64, s:Utf8]
          GpuCoalesceAllBatches: lanes=2, batches=single, schema=[k:Int64, v:Float64, s:Utf8]
            GpuEmitPartitions: hash=[k@0], lanes=2, batches=multiple, hashed_on=[k@0], schema=[k:Int64, v:Float64, s:Utf8]
              GpuMergePartitions: lanes=1, batches=multiple, schema=[k:Int64, v:Float64, s:Utf8]
                {SCAN}
          {SCAN}""")
    assert len(got) == 2 * len(ROWS) - 2
    assert set(got["k"]) <= set(ROWS["k"])


def test_an_empty_lane_still_owes_its_coalesce_one_typed_batch():
    got = run(f"""GpuUnload
  GpuMergePartitions: lanes=1, batches=multiple, {T}
    GpuCoalesceAllBatches: lanes=2, batches=single, {T}
      GpuLoadParquet: table=t, projections=[k@0, v@1, s@2], partition_groups=[[[0,1,2,3]],[]], lanes=2, batches=multiple, {T}""")
    assert sorted(got["k"]) == sorted(ROWS["k"])


def test_a_project_names_its_columns_from_the_schema():
    got = run(f"""GpuUnload
  GpuMergePartitions: lanes=1, batches=multiple, schema=[doubled:Int64, k:Int64]
    GpuProject: exprs=[k@0 * 2 as doubled, k@0], lanes=2, batches=multiple, schema=[doubled:Int64, k:Int64]
      {SCAN}""")
    assert list(got.columns) == ["doubled", "k"]
    assert sorted(got["doubled"]) == sorted(2 * k for k in ROWS["k"])


def test_an_inner_join_keeps_both_sides_columns_of_one_name_apart():
    got = join("GpuHashJoin: join_type=Inner, on=[(k@0, k@0)], filter=w@build:1 > k@probe:0, "
               "projection=[k@0, k@2, v@3], schema=[k:Int64, k:Int64, v:Float64]", U_BUILD, T_PROBE)
    assert list(got.columns) == ["k@0", "k@1", "v"]
    assert sorted(zip(got["k@0"], got["k@1"])) == [(1, 1), (2, 2), (2, 2)]


def test_a_mark_join_projects_from_the_build_side_and_its_mark():
    got = join("GpuHashJoin: join_type=LeftMark, on=[(k@0, k@0)], projection=[k@0, mark@3], "
               "schema=[k:Int64, mark:Boolean]", T_BUILD, U_PROBE)
    assert sorted(got.loc[got["mark"], "k"]) == [1, 2]
    assert len(got) == len(ROWS)


def test_a_mark_join_over_an_earlier_marks_output_keeps_both_marks():
    # tpcds q10's shape: two EXISTS under one OR, the second mark join building on the first.
    marked = "schema=[k:Int64, v:Float64, s:Utf8, mark:Boolean]"
    first = f"""GpuHashJoin: join_type=LeftMark, on=[(k@0, k@0)], lanes=1, batches=multiple, {marked}
        {built(T_ONE, T, 4)}
        {U_PROBE}"""
    got = join("GpuHashJoin: join_type=LeftMark, on=[(k@0, k@0)], "
               "schema=[k:Int64, v:Float64, s:Utf8, mark:Boolean, mark:Boolean]",
               built(first, marked), U_PROBE)
    assert list(got.columns) == ["k", "v", "s", "mark@3", "mark@4"]
    assert (got["mark@3"] == got["mark@4"]).all() and got["mark@3"].sum() == 2


def test_a_right_semi_join_emits_the_probe_rows_that_matched():
    got = join("GpuHashJoin: join_type=RightSemi, on=[(k@0, k@0)], " + U, T_BUILD, U_PROBE)
    assert sorted(got["w"]) == [10, 20, 21]


def test_a_left_join_pads_the_unmatched_build_rows():
    got = join("GpuHashJoin: join_type=Left, on=[(k@0, k@0)], "
               "schema=[k:Int64, v:Float64, s:Utf8, k:Int64, w:Int64]", T_BUILD, U_PROBE)
    assert list(got.columns) == ["k@0", "v", "s", "k@3", "w"]
    assert len(got) == 8 and got["w"].isna().sum() == 5


def test_a_nested_loop_join_and_a_cross_join_pair_every_row():
    got = join("GpuNestedLoopJoin: join_type=Inner, filter=k@build:0 > k@probe:0, "
               "projection=[k@0, w@4], schema=[k:Int64, w:Int64]", T_BUILD, U_PROBE)
    assert len(got) == 6 + 5 + 5
    got = join("GpuCrossJoin: schema=[k:Int64, w:Int64, k:Int64, v:Float64, s:Utf8]", U_BUILD, T_PROBE)
    assert len(got) == len(OTHER) * len(ROWS)


STATE = "schema=[s:Utf8, total:Int64, n:Int64]"


def test_an_init_and_a_merge_aggregate_as_the_plan_decomposed_them():
    got = run(f"""GpuUnload
  GpuAggregateBatches: group_by=[s@0], aggs=[sum(total@1) as total, sum(n@2) as n], final=[total@1, n@2], lanes=1, batches=single, {STATE}
    GpuMergePartitions: lanes=1, batches=multiple, {STATE}
      GpuAggregate: group_by=[s@2], aggs=[sum(k@0) as total, count(v@1) as n], lanes=2, batches=multiple, {STATE}
        {SCAN}""")
    want = ROWS.groupby("s").agg(total=("k", "sum"), n=("v", "count")).reset_index()
    assert got.sort_values("s").values.tolist() == want.values.tolist()


def test_a_stddev_merges_its_welford_state_and_finalizes():
    welford = "schema=[sd$count:Int64, sd$mean:Float64, sd$m2:Float64]"
    divisor = "(CAST(sd$count@0 AS Float64) - 1)"
    got = run(f"""GpuUnload
  GpuAggregateBatches: group_by=[], aggs=[merge_m2(sd$count@0, sd$mean@1, sd$m2@2) as [sd$count, sd$mean, sd$m2]], final=[CASE WHEN {divisor} <= 0 THEN NULL ELSE sqrt(sd$m2@2 / {divisor}) END as sd], lanes=1, batches=single, schema=[sd:Float64]
    GpuMergePartitions: lanes=1, batches=multiple, {welford}
      GpuAggregate: group_by=[], aggs=[count(v@1) as sd$count, mean(v@1) as sd$mean, m2(v@1) as sd$m2], lanes=2, batches=multiple, {welford}
        {SCAN}""")
    assert abs(got["sd"].iloc[0] - ROWS["v"].std(ddof=1)) < 1e-12


def test_a_rollup_tags_each_set_with_datafusions_grouping_id():
    sets = "schema=[s:Utf8, k:Int64, __grouping_id:UInt8, n:Int64]"
    got = run(f"""GpuUnload
  GpuAggregateBatches: group_by=[s@0, k@1, __grouping_id@2], aggs=[sum(n@3) as n], lanes=1, batches=single, {sets}
    GpuMergePartitions: lanes=1, batches=multiple, {sets}
      GpuAggregate: group_by=[s@2, k@0], grouping_sets=[[s@2, k@0], [s@2], []], aggs=[count(v@1) as n], lanes=2, batches=multiple, {sets}
        {SCAN}""")
    # The first key is the high bit: (s, k) is 0, (s) masks k and is 1, () is 3.
    assert got["__grouping_id"].value_counts().to_dict() == {0: 7, 1: 6, 3: 1}
    assert got.loc[got["__grouping_id"] == 3, "n"].tolist() == [ROWS["v"].count()]


def test_a_global_aggregate_over_no_rows_is_still_one_row():
    from ..operators.aggregates import PlanCall, plan_aggregate
    from ..operators.expressions import Col

    calls = (PlanCall("count", (Col("v"),), ("n",)), PlanCall("sum", (Col("v"),), ("total",)))
    out = plan_aggregate(ROWS.iloc[0:0], (), calls)
    assert len(out) == 1 and out.iloc[0, 0] == 0 and pd.isna(out.iloc[0, 1])


def test_a_group_by_with_no_calls_is_a_distinct():
    from ..operators.aggregates import plan_aggregate
    from ..operators.expressions import Col

    out = plan_aggregate(ROWS, (Col("s"),), ())
    assert out.iloc[:, 0].tolist() == sorted(set(ROWS["s"]))


def test_a_sum_over_only_nulls_is_null_and_not_zero():
    from ..operators.aggregates import PlanCall, plan_aggregate
    from ..operators.expressions import Col

    out = plan_aggregate(ROWS, (Col("s"),), (PlanCall("sum", (Col("v"),), ("total",)),))
    sums = dict(zip(out.iloc[:, 0], out.iloc[:, 1]))
    assert pd.isna(sums["d"]) and sums["x"] == 2.0


if __name__ == "__main__":
    raise SystemExit(main(globals()))
