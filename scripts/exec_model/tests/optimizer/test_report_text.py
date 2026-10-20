"""`.optimizer.txt`'s section, rendered from a hand-built report: each rule's lines, worded so that
they read without the code, and one line where nothing fired."""

from __future__ import annotations

if __package__ in (None, ""):  # allow `python scripts/exec_model/tests/optimizer/<file>.py`
    import pathlib as _pathlib, sys as _sys

    _sys.path.insert(0, str(_pathlib.Path(__file__).resolve().parents[4]))
    __package__ = "scripts.exec_model.tests.optimizer"

import pandas as pd

from ..harness import main
from ...operators.join_types import JoinType
from ...optimizer.cardinality import Certainty
from ...optimizer.dynamic_filters import KeySummary, Pruned, PrunedBy
from ...optimizer.report import (
    BuildMiss, ChosenJoin, EdgeColumn, EdgeReport, FilterCandidate, FilterPass, JoinOrder, Kept, OptimizerReport,
    PricedSet, Probed, RelationEstimate, Replan,
)
from ...optimizer.report_text import DIFF_ELSEWHERE, ESTIMATES, NAMES, plan_diff, render

AGGREGATE = EdgeColumn(2, "max(spread.pk)", aggregate=True)
# DPhyp moves the aggregate below the sales, and the aggregate builds.
STAR = JoinOrder(
    mj="MJ1",
    relations=(RelationEstimate("p0", 50.0, Certainty.MEASURED, 8.125, (("dk", 49.99),)),
               RelationEstimate("sales", 1000.0, Certainty.EXACT, 8.125, (("sk", 50.0),)),
               RelationEstimate("agg(MJ2) (f)", 100.4, Certainty.ESTIMATED, 16.25, (("max(spread.pk)", 100.4),))),
    edges=(EdgeReport(0b001, 0b010, ((EdgeColumn(0, "dk"), EdgeColumn(1, "sk")),)),
           EdgeReport(0b100, 0b011, ((AGGREGATE, EdgeColumn(0, "dk")),)),
           EdgeReport(0b010, 0b100, residual=(EdgeColumn(1, "sk"), AGGREGATE))),
    priced=(PricedSet(0b011, 500.0, 8000.0), PricedSet(0b101, 50.0, 400.0), PricedSet(0b111, 500.0, 12000.25)),
    max_pairs=10_000, tree=((0, 2), 1), unsolved=None, planned=((0, 1), 2), oriented=((2, 0), 1),
    joins=(ChosenJoin(0b100, 0b001, 50.0, 400.0), ChosenJoin(0b101, 0b010, 500.0, 12000.25)),
    plan_cost=20000.25, flipped=())
# Past its budget: the previous order kept, and its one join turned round, a semi join.
PAIR = JoinOrder(
    mj="MJ2",
    relations=(RelationEstimate("m0", 100.0, Certainty.MEASURED, 8.125, (("dk", 90.0),)),
               RelationEstimate("spread", 1000.0, Certainty.EXACT, 8.125, (("pk", 100.0),))),
    edges=(EdgeReport(0b01, 0b10, ((EdgeColumn(0, "dk"), EdgeColumn(1, "pk")),), kind=JoinType.LEFT_SEMI),),
    priced=(PricedSet(0b11, 100.0, 800.0),), max_pairs=1, tree=(0, 1), unsolved="budget", planned=(0, 1),
    oriented=(1, 0), joins=(ChosenJoin(0b10, 0b01, 100.0, 800.0),), plan_cost=800.0, flipped=((0b10, 0b01),),
    types=((0b10, 0b01, JoinType.RIGHT_SEMI), (0b01, 0b10, JoinType.LEFT_SEMI)))

EVERY_RULE = OptimizerReport(
    filters_before_order=FilterPass(
        candidates=(FilterCandidate("dk", "sales", "sk", 0.9), FilterCandidate("d", "sales", "sd", 1.0)),
        pruned=(Pruned("sales", "sk,sd", tuple(range(10)), (0, 1, 2, 4), (
            PrunedBy("sk", KeySummary(500, 549, None), "p0", "dk"),
            PrunedBy("sd", KeySummary(pd.Timestamp("1999-01-02"), 7.0, (7.0,)), "p0", "d")), "sales#1"),
            Pruned("returns", "rk", (0, 1), (), (PrunedBy("rk", KeySummary(None, None, ()), "p0", "dk"),))),
        probed=(Probed("p0", 50, 400, (), "dates (f)"),)),
    orders=(STAR,),
    filters_after_order=FilterPass(
        candidates=(FilterCandidate("dk", "spread", "pk", 1.0),),
        pruned=(Pruned("spread", "pk", (0, 1, 2), (1,), (PrunedBy("pk", KeySummary(510, 519, None), "p1", "dk"),)),),
        probed=(Probed("p1", 10, 80, ("MJ1",), "[p0] join sales"),),
        replans=(("p1", Replan(BuildMiss("join on pk = sk", 40, 4.0, "MJ1"), kept=(Kept("m2", 40, "[p0] join sales"),),
                               orders=(PAIR,))),),
        refused=(("p1", BuildMiss("join on dk = pk", 30, 6.0, "MJ1")),)),
    replans=(Replan(BuildMiss("join on dk = sk", 100, 10.0, "MJ1"),
                    kept=(Kept("m0", 100, "p0"), Kept("m1", 1000, "union(sales, returns)")), orders=(PAIR,)),),
    refused=(BuildMiss("nested loop join", 3, 12.0),),
    before="GpuUnload\n  GpuFilter: a\n  GpuLoadParquet: t\n",
    after="GpuUnload\n  GpuFilter: b\n  GpuLoadParquet: t\n",
)

EXPECTED = ESTIMATES + "\n" + NAMES + """
dynamic filters on the plan as written
  candidate dk -> sales.sk: row group ranges sum to 0.90x the column's range, ordered (under 1.5)
  candidate d -> sales.sd: row group ranges sum to 1.00x the column's range, ordered (under 1.5)
  sales#1: row groups 10 -> 4 (0-2,4)
    sk by p0.dk in [500, 549]
    sd by p0.d in [1999-01-02, 7], values 7
  returns: row groups 2 -> 0
    rk by p0.dk: no keys
  build p0 = dates (f), run first: 50 rows, 400 B; the plan reads it from memory
MJ1: 3 relations, ordered before the run
  r0 p0: rows 50 measured, 8.1 B/row; NDV dk 50
  r1 sales: rows 1000 exact, 8.1 B/row; NDV sk 50
  r2 agg(MJ2) (f): rows 100.4 est, 16.2 B/row; NDV [max(spread.pk)] 100.4
  edge r0-r1: r0.dk = r1.sk
  edge r2-r0+r1: r2.[max(spread.pk)] (aggregate output) = r0.dk; applies once r0+r1 are joined
  residual r1-r2 on r1.sk, r2.[max(spread.pk)] (aggregate output): keeps 0.2 of the matches, not a DPhyp edge
  plan's order ((r0 r1) r2), build first
  DPhyp priced 3 sets, chose ((r0 r2) r1)
  oriented ((r2 r0) r1), build first
  join r2 x r0: 50.0 rows x 8.0 B = 400 B
  join r0+r2 x r1: 500.0 rows x 24.0 B = 12000 B
  C_out 12400 B; the plan's order, priced by the same estimates, 20000 B
dynamic filters after join order
  candidate dk -> spread.pk: row group ranges sum to 1.00x the column's range, ordered (under 1.5)
  spread: row groups 3 -> 1 (1)
    pk by p1.dk in [510, 519]
  build p1 = [p0] join sales, run first: 10 rows, 80 B, holding MJ1; the plan reads it from memory
  probe p1 replan 1: the build of MJ1's join on pk = sk came out 40 rows, est 4.0 (q-error 10.00)
    kept as memory sources: m2 (40 rows) = [p0] join sales
    MJ2: 2 relations, ordered again
      r0 m0: rows 100 measured, 8.1 B/row; NDV dk 90
      r1 spread: rows 1000 exact, 8.1 B/row; NDV pk 100
      edge r0-r1: r0.dk = r1.pk, a semi join keeping r0's matches
      previous order (r0 r1), build first
      DPhyp stopped at its budget of 1 pairs after pricing 1 sets: the previous order kept
      oriented (r1 r0), build first
      join r1 x r0: 100.0 rows x 8.0 B = 800 B
      C_out 800 B; the previous order, priced by the same estimates, 800 B
      flipped: r1 builds, r0 probes
  probe p1 replan refused, work outside its builds had started: the build of MJ1's join on dk = pk came out 30 rows, est 6.0 (q-error 5.00)
replan 1: the build of MJ1's join on dk = sk came out 100 rows, est 10.0 (q-error 10.00)
  kept as memory sources: m0 (100 rows) = p0; m1 (1000 rows) = union(sales, returns)
  MJ2: 2 relations, ordered again
    r0 m0: rows 100 measured, 8.1 B/row; NDV dk 90
    r1 spread: rows 1000 exact, 8.1 B/row; NDV pk 100
    edge r0-r1: r0.dk = r1.pk, a semi join keeping r0's matches
    previous order (r0 r1), build first
    DPhyp stopped at its budget of 1 pairs after pricing 1 sets: the previous order kept
    oriented (r1 r0), build first
    join r1 x r0: 100.0 rows x 8.0 B = 800 B
    C_out 800 B; the previous order, priced by the same estimates, 800 B
    flipped: r1 builds, r0 probes
replan refused, work outside its builds had started: the build of a nested loop join came out 3 rows, est 12.0 (q-error 4.00)
""" + DIFF_ELSEWHERE + "\n"


def test_every_rule_renders_its_lines_and_points_to_the_plan_s_diff():
    assert render(EVERY_RULE) == EXPECTED, render(EVERY_RULE)
    assert DIFF_ELSEWHERE.startswith("plan diff: ") and ".plandiff.txt" in DIFF_ELSEWHERE


def test_the_plan_s_diff_is_a_section_of_its_own_and_an_unchanged_plan_says_so():
    assert plan_diff(EVERY_RULE) == """plan diff
--- before
+++ after
@@ -1,3 +1,3 @@
 GpuUnload
-  GpuFilter: a
+  GpuFilter: b
   GpuLoadParquet: t
"""
    plan = "GpuUnload\n  GpuLoadParquet: t\n"
    assert plan_diff(OptimizerReport(FilterPass(), (), FilterPass(), (), (), plan, plan)) == "plan unchanged\n"


def test_the_estimates_and_the_names_are_said_once_ahead_of_the_section():
    assert ESTIMATES.startswith("estimates: ") and ESTIMATES.count("\n") == 0
    assert "selectivity" in ESTIMATES and "C_out" in ESTIMATES and "whole row" in ESTIMATES
    assert NAMES.startswith("names: ") and all(word in NAMES for word in ("(f)", "agg(X)", "t#2", "p0", "m0"))
    plan = "GpuUnload\n  GpuLoadParquet: t\n"
    filters_only = OptimizerReport(EVERY_RULE.filters_before_order, (), FilterPass(), (), (), plan, plan)
    assert render(filters_only).startswith(NAMES + "\ndynamic filters on the plan as written\n")


def test_each_edge_says_what_its_join_keeps_and_what_a_residual_beside_its_keys_keeps():
    keys = ((EdgeColumn(0, "a"), EdgeColumn(1, "b")),)
    edges = (EdgeReport(0b01, 0b10, keys, kind=JoinType.LEFT), EdgeReport(0b01, 0b10, keys, kind=JoinType.LEFT_ANTI),
             EdgeReport(0b01, 0b10, keys, kind=JoinType.FULL),
             EdgeReport(0b01, 0b10, keys, residual=(EdgeColumn(0, "x"), EdgeColumn(1, "y"))))
    text = render(OptimizerReport(FilterPass(), (JoinOrder(**{**PAIR.__dict__, "edges": edges}),), FilterPass(), (), (),
                                  "a\n", "a\n"))
    assert "  edge r0-r1: r0.a = r1.b, a left join keeping every row of r0\n" in text, text
    assert "  edge r0-r1: r0.a = r1.b, an anti join keeping r0's rows without a match\n" in text
    assert "  edge r0-r1: r0.a = r1.b, a full join keeping every row\n" in text
    assert "  edge r0-r1: r0.a = r1.b, then a residual on r0.x, r1.y keeping 0.2\n" in text


def test_a_dphyp_call_that_keeps_the_plan_s_shape_says_so():
    kept = render(OptimizerReport(FilterPass(), (STAR.__class__(**{**STAR.__dict__, "tree": ((0, 1), 2)}),),
                                  FilterPass(), (), (), "a\n", "a\n"))
    assert "  DPhyp priced 3 sets, chose ((r0 r1) r2): the plan's order\n" in kept, kept


def test_a_report_where_nothing_fired_is_one_line():
    plan = "GpuUnload\n  GpuLoadParquet: t\n"
    assert render(OptimizerReport(FilterPass(), (), FilterPass(), (), (), plan, plan)) == "nothing fired\n"


def test_a_cluster_left_as_it_was_says_the_plan_did_not_change():
    plan = "GpuUnload\n  GpuLoadParquet: t\n"
    text = render(OptimizerReport(FilterPass(), (PAIR,), FilterPass(), (), (), plan, plan))
    assert text.startswith(ESTIMATES + "\n" + NAMES + "\nMJ2: 2 relations, ordered before the run\n") and text.endswith("plan unchanged\n"), text


if __name__ == "__main__":
    raise SystemExit(main(globals()))
