"""`.optimizer.txt`'s section, rendered from a hand-built report: each rule's lines, and one line
where nothing fired."""

from __future__ import annotations

if __package__ in (None, ""):  # allow `python scripts/exec_model/tests/optimizer/<file>.py`
    import pathlib as _pathlib, sys as _sys

    _sys.path.insert(0, str(_pathlib.Path(__file__).resolve().parents[4]))
    __package__ = "scripts.exec_model.tests.optimizer"

import pandas as pd

from ..harness import main
from ...optimizer.dynamic_filters import KeySummary, Pruned
from ...optimizer.report import (
    BuildMiss, ChosenJoin, FilterCandidate, JoinOrder, OptimizerReport, PricedSet, Replan,
)
from ...optimizer.report_text import render

STAR = JoinOrder(
    relations=("MemorySource p0", "LoadParquet sales", "LoadParquet spread"),
    edges=((0b001, 0b010, "r0.dk = r1.sk"), (0b001, 0b100, "r0.dk = r2.pk")),
    priced=(PricedSet(0b011, 500.0, 8000.0), PricedSet(0b101, 50.0, 400.0), PricedSet(0b111, 500.0, 12000.25)),
    max_pairs=10_000, tree=((0, 2), 1), unsolved=None, oriented=((2, 0), 1),
    joins=(ChosenJoin(0b100, 0b001, 50.0, 400.0), ChosenJoin(0b101, 0b010, 500.0, 12000.25)),
    plan_cost=20000.25, flipped=((0b100, 0b001),))
PAIR = JoinOrder(
    relations=("MemorySource m0", "LoadParquet spread"), edges=((0b01, 0b10, "r0.dk = r1.pk"),),
    priced=(PricedSet(0b11, 100.0, 800.0),), max_pairs=1, tree=(0, 1), unsolved="budget", oriented=(0, 1),
    joins=(ChosenJoin(0b01, 0b10, 100.0, 800.0),), plan_cost=800.0, flipped=())

EVERY_RULE = OptimizerReport(
    candidates=(FilterCandidate("dk", "sales", "sk", 0.9), FilterCandidate("d", "sales", "sd", 1.0)),
    pruned=(Pruned("sales", "sk,sd", tuple(range(10)), (0, 1, 2, 4), (
        ("sk", KeySummary(500, 549, None)), ("sd", KeySummary(pd.Timestamp("1999-01-02"), 7.0, (7.0,))))),
        Pruned("returns", "rk", (0, 1), (), (("rk", KeySummary(None, None, ())),))),
    probed=(("p0", 50),),
    orders=(STAR,),
    replans=(Replan(BuildMiss("GpuHashJoin on=[(dk@0, sk@0)]", 100, 10.0), kept=(("m0", 100), ("m1", 1000)),
                    orders=(PAIR,)),),
    refused=(BuildMiss("GpuNestedLoopJoin", 3, 12.0),),
    before="GpuUnload\n  GpuFilter: a\n  GpuLoadParquet: t\n",
    after="GpuUnload\n  GpuFilter: b\n  GpuLoadParquet: t\n",
)

EXPECTED = """\
dynamic filters
  candidate dk -> sales.sk, clustering 0.90
  candidate d -> sales.sd, clustering 1.00
  sales: row groups 10 -> 4 (0-2,4)
    sk in [500, 549]
    sd in [1999-01-02, 7], values 7
  returns: row groups 2 -> 0
    rk: no keys
  build p0 read from memory: 50 rows
join order 1: 3 relations
  r0 MemorySource p0
  r1 LoadParquet sales
  r2 LoadParquet spread
  edge r0-r1: r0.dk = r1.sk
  edge r0-r2: r0.dk = r2.pk
  priced 3 sets
  DPhyp ((r0 r2) r1)
  oriented ((r2 r0) r1), build first
  join r2 x r0: 50.0 rows, 400 bytes
  join r0+r2 x r1: 500.0 rows, 12000 bytes
  C_out 12400 bytes, the plan's order 20000
  flipped: r2 builds, r0 probes
replan 1: a build of 100 rows against 10.0 estimated (q-error 10.00) for GpuHashJoin on=[(dk@0, sk@0)]
  kept m0 (100 rows), m1 (1000 rows)
  join order 1: 2 relations
    r0 MemorySource m0
    r1 LoadParquet spread
    edge r0-r1: r0.dk = r1.pk
    priced 1 sets
    DPhyp stopped at its budget of 1 pairs after pricing 1 sets: the plan's order kept
    oriented (r0 r1), build first
    join r0 x r1: 100.0 rows, 800 bytes
    C_out 800 bytes, the plan's order 800
replan refused: a build of 3 rows against 12.0 estimated (q-error 4.00) for GpuNestedLoopJoin
plan diff
--- before
+++ after
@@ -1,3 +1,3 @@
 GpuUnload
-  GpuFilter: a
+  GpuFilter: b
   GpuLoadParquet: t
"""


def test_every_rule_renders_its_lines_and_the_plan_its_diff():
    assert render(EVERY_RULE) == EXPECTED, render(EVERY_RULE)


def test_a_report_where_nothing_fired_is_one_line():
    plan = "GpuUnload\n  GpuLoadParquet: t\n"
    assert render(OptimizerReport((), (), (), (), (), (), plan, plan)) == "nothing fired\n"


def test_a_cluster_left_as_it_was_says_the_plan_did_not_change():
    plan = "GpuUnload\n  GpuLoadParquet: t\n"
    text = render(OptimizerReport((), (), (), (PAIR,), (), (), plan, plan))
    assert text.startswith("join order 1: 2 relations\n") and text.endswith("plan unchanged\n"), text


if __name__ == "__main__":
    raise SystemExit(main(globals()))
