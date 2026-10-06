"""A materialization fed back as `GpuMemorySource`: it runs as the build it replaces, keeps the
layout it was made with so nothing shuffles it again, and is estimated by what it measured."""

from __future__ import annotations

if __package__ in (None, ""):  # allow `python scripts/exec_model/tests/optimizer/<file>.py`
    import pathlib as _pathlib, sys as _sys

    _sys.path.insert(0, str(_pathlib.Path(__file__).resolve().parents[4]))
    __package__ = "scripts.exec_model.tests.optimizer"

import pandas as pd

from ..corpus import execute
from ..harness import main
from ..engine.test_adaptive import JOIN, adaptive, states
from .test_cardinality import dataset, scan
from ..plans.test_engine_nodes import U, tables
from ...optimizer.replan import WithMaterialized
from ...optimizer.cardinality import Column, Estimate, estimate, measured
from ...optimizer.disassembly import reassembled
from ...plans.engine_nodes import build
from ...plans.engine_plan import parse_plans

# JOIN with u's coalesced build replaced by what a stopped run left of it.
BUILD = JOIN.split("\n")[3:6]


def replaced(hashed_on: str) -> str:
    memory = f"      GpuMemorySource: name=m, lanes=2, batches=single, hashed_on={hashed_on}, {U}"
    return JOIN.replace("\n".join(BUILD), memory)


def materialized():
    """u's build as the run stopped on its `BuildDone` left it, one frame a lane."""
    _, _, driver = adaptive(stops=lambda event: True)
    driver.run()
    [coalesce] = states(driver, "GpuCoalesceAllBatches")
    return [batch.frame for queue in coalesce.out_queues for batch in queue]


def test_a_memory_source_runs_as_the_build_it_replaces():
    original = parse_plans(f"== q\n{JOIN}", "test")["q"]
    fed = parse_plans(f"== q\n{replaced('[k@0]')}", "test")["q"]
    want, _ = execute(build(original, tables()))
    got, driver = execute(build(fed, WithMaterialized(tables(), {"m": materialized()})))
    key = list(want.columns)
    pd.testing.assert_frame_equal(got.sort_values(key, ignore_index=True), want.sort_values(key, ignore_index=True))
    # Nothing re-made the build: no coalesce, and the one shuffle left is the probe's.
    assert not states(driver, "GpuCoalesceAllBatches") and len(states(driver, "GpuEmitPartitions")) == 1


def test_a_memory_source_hashed_on_its_join_keys_is_left_unwired_and_rehashed_otherwise():
    def build_side(hashed_on):
        plan = reassembled(parse_plans(f"== q\n{replaced(hashed_on)}", "test")["q"], lanes=2)
        node, kinds = plan.children[0].children[0].children[0], []
        while node.children:
            kinds.append(node.kind)
            node = node.children[0]
        return kinds + [node.kind]

    assert build_side("[k@0]") == ["GpuMemorySource"]
    assert build_side("[w@1]") == ["GpuCoalesceAllBatches", "GpuEmitPartitions", "GpuCoalesceAllBatches",
                                   "GpuMergePartitions", "GpuMemorySource"]


def test_a_measured_estimate_keeps_the_lineage_with_true_rows_and_ndv_inside_the_counts():
    lanes = [pd.DataFrame({"k": [1, 1], "w": [10, None]}), pd.DataFrame({"k": [2, 9], "w": [21, 90]})]
    guessed = Estimate(1000.0, (Column(("u", "k"), 100.0), Column(("u", "w"), 1.0)))
    got = measured(guessed, lanes, ["k"])
    assert got.rows == 4 and [c.base for c in got.columns] == [("u", "k"), ("u", "w")]
    # k is hashed on: its lanes are disjoint and 3 is exact. w's lanes hold 1 and 2 values,
    # so its NDV lies in 2..3, and the guess of 1 is raised to the bound.
    assert [c.ndv for c in got.columns] == [3.0, 2.0]
    assert got.columns[1].null_share == 0.25


def test_the_estimator_takes_a_memory_source_as_measured():
    text = f"""GpuUnload
  GpuHashJoin: join_type=Inner, on=[(dk@0, sk@0)], lanes=1, batches=multiple, schema=[dk:Int64, sk:Int64]
    GpuMemorySource: name=m, lanes=1, batches=single, schema=[dk:Int64]
    {scan('sales', ['sk'])}
"""
    plan = parse_plans(f"== q\n{text}", "test")["q"]
    join, memory = plan.children[0], plan.children[0].children[0]

    def rows(n):
        known = {"m": Estimate(float(n), (Column(("dates", "dk"), float(n)),))}
        found = estimate(plan, dataset(), known)
        assert found[id(memory)] is known["m"]
        return found[id(join)].rows

    # Every one of sales' 100 keys among the dates matches ten sales rows.
    assert abs(rows(100) / rows(10) - 10) < 1e-9


if __name__ == "__main__":
    raise SystemExit(main(globals()))
