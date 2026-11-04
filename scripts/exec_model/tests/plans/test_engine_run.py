"""A prototype run of an engine plan, rendered as the engine renders its own runs."""

from __future__ import annotations

if __package__ in (None, ""):  # allow `python scripts/exec_model/tests/plans/<file>.py`
    import pathlib as _pathlib, sys as _sys

    _sys.path.insert(0, str(_pathlib.Path(__file__).resolve().parents[4]))
    __package__ = "scripts.exec_model.tests.plans"

from ..corpus import execute
from ..harness import main
from .test_engine_nodes import SCAN, T, T_ONE, U, U_PROBE, built, tables
from ...plans.engine_nodes import build
from ...plans.engine_plan import parse_plans
from ...plans.engine_run import render_run
from ...optimizer.multijoin import named


def rendered(tree: str) -> list[str]:
    plan = named(parse_plans(f"== q\n{tree}\n", "test")["q"])
    _, driver = execute(build(plan, tables()))
    return render_run(plan, driver).splitlines()


def test_a_node_line_is_the_plans_minus_its_schema_plus_what_it_emitted():
    lines = rendered(f"""GpuUnload
  GpuMergePartitions: lanes=1, batches=multiple, {T}
    GpuFilter: predicate=k@0 > 2, lanes=2, batches=multiple, {T}
      {SCAN}""")
    assert lines[0] == "early_exit=none"
    filter_line, filter_detail = lines[5], lines[6]
    assert filter_line.startswith("    GpuFilter: predicate=k@0 > 2, lanes=2, batches=multiple, output_rows=5")
    assert "schema=" not in "".join(lines)
    # The scan's lanes emit k = [5,1] and [4,2] | [3,6,7]; the filter takes 4 and 3 rows from
    # them and keeps one of each pair and all three of the last.
    assert filter_detail.strip().startswith("in_rows=[[4,3]] batch_rows=[[1,1],[3]]")
    assert lines[8].strip().startswith("in_rows=[] batch_rows=[[2,2],[3]]")  # a source takes nothing


def test_a_satisfied_limit_is_named_by_its_post_order_address():
    lines = rendered(f"""GpuUnload: skip=0, fetch=2
  GpuMergePartitions: lanes=1, batches=multiple, {T}
    {SCAN}""")
    # Post-order: the scan is 0, the merge 1, the unload 2.
    assert lines[0] == "early_exit=GpuUnload@2"
    assert lines[1].startswith("GpuUnload: skip=0, fetch=2, output_rows=2")


def test_a_join_in_a_cluster_names_it_and_no_other_join_does():
    lines = rendered(f"""GpuUnload
  GpuCrossJoin: lanes=1, batches=multiple, schema=[k:Int64, k:Int64]
    GpuCoalesceAllBatches: lanes=1, batches=single, schema=[k:Int64]
      GpuNestedLoopJoin: join_type=Inner, filter=k@build:0 > k@probe:0, projection=[k@0], lanes=1, batches=multiple, schema=[k:Int64]
        {built(T_ONE, T, 4)}
        {U_PROBE}
    GpuHashJoin: join_type=LeftSemi, on=[(k@0, k@0)], filter=k@build:0 < k@probe:0, lanes=1, batches=multiple, schema=[k:Int64]
      GpuCoalesceAllBatches: lanes=1, batches=single, schema=[k:Int64]
        GpuHashJoin: join_type=Inner, on=[(k@0, k@0)], projection=[k@0], lanes=1, batches=multiple, schema=[k:Int64]
          {built(T_ONE, T, 5)}
          {U_PROBE}
      GpuCoalesceAllBatches: lanes=1, batches=single, {U}
        {U_PROBE}""")
    joins = [(line.split(",")[0].strip(), "mj=" in line) for line in lines if "Join:" in line]
    # The filtered semi join and the inner join under it are one cluster: a residual walls none.
    assert joins == [("GpuCrossJoin: lanes=1", False), ("GpuNestedLoopJoin: join_type=Inner", False),
                     ("GpuHashJoin: join_type=LeftSemi", True), ("GpuHashJoin: join_type=Inner", True)]
    for joined in [line for line in lines if "mj=" in line]:
        assert ", batches=multiple, mj=MJ1, output_rows=" in joined

if __name__ == "__main__":
    raise SystemExit(main(globals()))
