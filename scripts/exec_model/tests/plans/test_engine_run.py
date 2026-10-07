"""A prototype run of an engine plan, rendered as the engine renders its own runs."""

from __future__ import annotations

if __package__ in (None, ""):  # allow `python scripts/exec_model/tests/plans/<file>.py`
    import pathlib as _pathlib, sys as _sys

    _sys.path.insert(0, str(_pathlib.Path(__file__).resolve().parents[4]))
    __package__ = "scripts.exec_model.tests.plans"

from ..corpus import execute
from ..harness import main
from .test_engine_nodes import SCAN, T, tables
from ...plans.engine_nodes import build
from ...plans.engine_plan import parse_plans
from ...plans.engine_run import render_run


def rendered(tree: str) -> list[str]:
    plan = parse_plans(f"== q\n{tree}\n", "test")["q"]
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


if __name__ == "__main__":
    raise SystemExit(main(globals()))
